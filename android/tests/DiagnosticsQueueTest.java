package org.nvforever.android;

import java.io.File;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.util.Comparator;
import java.util.List;
import java.util.zip.CRC32;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

/** Plain JDK17 host tests: real owned temporary files, no Android/network/game data. */
public final class DiagnosticsQueueTest {
    private static int passed;
    private interface Test { void run(Path root) throws Exception; }
    private interface IO { void run() throws Exception; }
    public static void main(String[] args) throws Exception {
        test("retry/ack/relaunch dedup preserves candidate", DiagnosticsQueueTest::retryAck);
        test("offline and changed pair cannot transfer old data", DiagnosticsQueueTest::pairChange);
        test("backoff is persisted and capped", DiagnosticsQueueTest::backoff);
        test("injected atomic failure exposes no partial snapshot", DiagnosticsQueueTest::atomicFailure);
        test("corrupt owned metadata/archive never deletes source", DiagnosticsQueueTest::corruption);
        test("snapshot count bound retains unacknowledged files", DiagnosticsQueueTest::countBound);
        test("total byte bound includes old pairings", DiagnosticsQueueTest::byteBound);
        test("pair and acknowledgement histories are bounded", DiagnosticsQueueTest::historyBound);
        test("strict ids/unexpected files/scan bounds fail closed", DiagnosticsQueueTest::strictFiles);
        test("queue lock rejects concurrent owner without blocking", DiagnosticsQueueTest::locking);
        test("symlinks rejected when supported by host", DiagnosticsQueueTest::links);
        test("truncated ZIP and wrong hash are never committed", DiagnosticsQueueTest::incomplete);
        System.out.println("DiagnosticsQueueTest: " + passed + " passed");
    }
    private static void test(String label, Test test) throws Exception {
        Path root = Files.createTempDirectory("nvf-diagnostics-queue-");
        try { test.run(root); passed++; System.out.println("PASS " + label); }
        finally {
            // Only this invocation's generated tree; walk never follows symbolic links.
            try (java.util.stream.Stream<Path> paths = Files.walk(root)) {
                Path absolute = root.toAbsolutePath().normalize();
                for (Path path : (Iterable<Path>) paths.sorted(Comparator.reverseOrder())::iterator) {
                    if (!path.toAbsolutePath().normalize().startsWith(absolute)) throw new AssertionError("Unsafe test cleanup");
                    Files.deleteIfExists(path);
                }
            }
        }
    }
    private static String pair(String suffix) { return DiagnosticsQueue.pairingKey("http://192.168.1.20:8765/upload/" + suffix, "very-private-token"); }
    private static String id(int n) throws Exception { return sha(("canonical-content-" + n).getBytes(StandardCharsets.UTF_8)); }
    private static String sha(byte[] bytes) throws Exception {
        StringBuilder out = new StringBuilder();
        for (byte b : MessageDigest.getInstance("SHA-256").digest(bytes)) out.append(String.format("%02x", b & 255));
        return out.toString();
    }
    private static String sha(File file) throws Exception { return sha(Files.readAllBytes(file.toPath())); }
    private static File zip(Path root, String name, int payloadBytes) throws Exception {
        File result = root.resolve(name).toFile(); byte[] buffer = new byte[8192]; CRC32 crc = new CRC32();
        for (int left = payloadBytes; left > 0; left -= Math.min(left, buffer.length)) crc.update(buffer, 0, Math.min(left, buffer.length));
        try (OutputStream file = Files.newOutputStream(result.toPath()); ZipOutputStream zip = new ZipOutputStream(file)) {
            ZipEntry entry = new ZipEntry("logs/synthetic.txt"); entry.setMethod(ZipEntry.STORED);
            entry.setSize(payloadBytes); entry.setCompressedSize(payloadBytes); entry.setCrc(crc.getValue()); entry.setTime(0);
            zip.putNextEntry(entry);
            for (int left = payloadBytes; left > 0; left -= Math.min(left, buffer.length)) zip.write(buffer, 0, Math.min(left, buffer.length));
            zip.closeEntry();
        }
        return result;
    }
    private static void check(boolean value, String why) { if (!value) throw new AssertionError(why); }
    private static void io(IO operation) throws Exception {
        try { operation.run(); } catch (IOException expected) { return; }
        throw new AssertionError("Expected bounded IO failure");
    }
    private static void retryAck(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 1024); String hash = sha(candidate), snapshot = id(1);
        DiagnosticsQueue q = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("a"));
        check(q.canEnqueue() && q.enqueue(candidate, snapshot, hash), "must enqueue");
        DiagnosticsQueue.Entry e = q.next(1000);
        check(e != null && !e.file.equals(candidate) && sha(e.file).equals(hash), "owned immutable copy");
        q.failed(e, 1000);
        check(q.next(30_999) == null, "first retry waits30s");
        q = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("a"));
        DiagnosticsQueue.Entry retry = q.next(31_000);
        check(retry != null && retry.file.equals(e.file) && retry.snapshotId.equals(snapshot), "relaunch retries same file/id");
        q.acknowledge(retry);
        check(q.totalCount() == 0 && q.containsSnapshot(snapshot), "ack removes generated pending and retains dedup");
        check(candidate.isFile() && sha(candidate).equals(hash), "original candidate untouched");
        check(!q.enqueue(candidate, snapshot, hash), "acknowledged content not requeued");
        DiagnosticsQueue reloaded = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("a"));
        check(reloaded.containsSnapshot(snapshot), "ack survives process restart");
    }
    private static void pairChange(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); String snapshot = id(2);
        File base = root.resolve("queue").toFile();
        DiagnosticsQueue old = new DiagnosticsQueue(base, pair("old")); old.enqueue(candidate, snapshot, sha(candidate));
        DiagnosticsQueue.Entry entry = old.next(0);
        DiagnosticsQueue changed = new DiagnosticsQueue(base, pair("new"));
        check(changed.next(0) == null && changed.count() == 0 && changed.totalCount() == 1, "old pairing stays pending but cannot upload to new endpoint/token");
        check(!changed.containsSnapshot(snapshot), "dedup scope follows pairing");
        io(() -> changed.acknowledge(entry));
        check(old.totalCount() == 1 && entry.file.isFile(), "no acknowledgement through another pairing");
        // OFF is manager-owned: constructing/reopening/scanning a queue performs no network or deletion.
        DiagnosticsQueue offline = new DiagnosticsQueue(base, pair("old"));
        check(offline.count() == 1 && sha(entry.file).equals(sha(candidate)), "offline/relaunch retains immutable data");
    }
    private static void backoff(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); DiagnosticsQueue q = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("retry"));
        q.enqueue(candidate, id(3), sha(candidate)); long now = 0;
        for (long delay : new long[] {30_000, 60_000, 120_000, 240_000, 480_000, 900_000, 900_000}) {
            DiagnosticsQueue.Entry entry = q.next(now); check(entry != null, "due retry"); q.failed(entry, now);
            check(q.next(now + delay - 1) == null, "backoff must not run early"); now += delay;
            check(q.next(now) != null, "backoff expires and remains capped");
        }
    }
    private static void atomicFailure(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); File base = root.resolve("queue").toFile();
        DiagnosticsQueue failing = new DiagnosticsQueue(base, pair("atomic"), () -> { throw new IOException("Injected commit failure"); });
        io(() -> failing.enqueue(candidate, id(4), sha(candidate)));
        DiagnosticsQueue q = new DiagnosticsQueue(base, pair("atomic"));
        check(q.totalCount() == 0 && candidate.isFile(), "partial copy not visible and source retained");
        try (java.util.stream.Stream<Path> paths = Files.list(base.toPath())) { check(paths.noneMatch(p -> p.getFileName().toString().endsWith(".part")), "temporary copies cleaned"); }
        check(q.enqueue(candidate, id(4), sha(candidate)), "same snapshot can retry after failed atomic admission");
    }
    private static void corruption(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); String hash = sha(candidate); File base = root.resolve("queue").toFile();
        DiagnosticsQueue q = new DiagnosticsQueue(base, pair("corrupt")); q.enqueue(candidate, id(5), hash);
        Path metadata;
        try (java.util.stream.Stream<Path> paths = Files.list(base.toPath())) { metadata = paths.filter(p -> p.toString().endsWith(".meta")).findFirst().orElseThrow(); }
        Files.write(metadata, "v=99\npath=../../source.zip\n".getBytes(StandardCharsets.US_ASCII));
        check(q.totalCount() == 0 && candidate.isFile() && sha(candidate).equals(hash), "broken metadata drops generated pair only");
        q.enqueue(candidate, id(6), hash); DiagnosticsQueue.Entry entry = q.next(0);
        byte[] bytes = Files.readAllBytes(entry.file.toPath()); bytes[20] ^= 1; Files.write(entry.file.toPath(), bytes);
        check(q.next(0) == null && q.totalCount() == 0, "modified generated copy not uploadable");
        check(sha(candidate).equals(hash), "source never deleted or changed");
    }
    private static void countBound(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); DiagnosticsQueue q = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("count"));
        for (int i = 0; i < 4; i++) check(q.enqueue(candidate, id(10 + i), sha(candidate)), "four pending admitted");
        check(!q.canEnqueue() && !q.enqueue(candidate, id(14), sha(candidate)) && q.totalCount() == 4, "queue full never discards unACK data");
    }
    private static void byteBound(Path root) throws Exception {
        File candidate = zip(root, "large.zip", 13 * 1024 * 1024); String hash = sha(candidate); File base = root.resolve("queue").toFile();
        DiagnosticsQueue old = new DiagnosticsQueue(base, pair("bytes-old"));
        check(old.enqueue(candidate, id(20), hash) && old.enqueue(candidate, id(21), hash), "two old snapshots");
        DiagnosticsQueue current = new DiagnosticsQueue(base, pair("bytes-new"));
        check(current.enqueue(candidate, id(22), hash), "third under48MiB");
        check(!current.canEnqueue(), "stop collection when another maximum-size snapshot cannot fit");
        check(!current.enqueue(candidate, id(23), hash) && current.totalCount() == 3, "fourth13MiB exceeds global48MiB across pairings");
        check(old.count() == 2 && current.count() == 1 && candidate.isFile(), "old data preserved");
    }
    private static void historyBound(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); String hash = sha(candidate); File base = root.resolve("queue").toFile();
        DiagnosticsQueue q = new DiagnosticsQueue(base, pair("history"));
        for (int i = 0; i < 65; i++) { check(q.enqueue(candidate, id(100 + i), hash), "distinct snapshot"); q.acknowledge(q.next(0)); }
        check(!q.containsSnapshot(id(100)) && q.containsSnapshot(id(101)) && q.containsSnapshot(id(164)), "exactly64 recent ACK identities");
        for (int i = 0; i < 3; i++) { DiagnosticsQueue other = new DiagnosticsQueue(base, pair("history-" + i)); other.enqueue(candidate, id(200 + i), hash); other.acknowledge(other.next(0)); }
        check(!new DiagnosticsQueue(base, pair("fifth")).canEnqueue(), "pair history bounded without deleting existing histories");
        try (java.util.stream.Stream<Path> files = Files.list(base.toPath())) {
            for (Path file : (Iterable<Path>) files::iterator) {
                String text = new String(Files.readAllBytes(file), StandardCharsets.US_ASCII);
                check(!text.contains("very-private-token") && !text.contains("192.168.1.20"), "queue never stores endpoint/token");
            }
        }
    }
    private static void strictFiles(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); File base = root.resolve("queue").toFile(); DiagnosticsQueue q = new DiagnosticsQueue(base, pair("strict"));
        io(() -> q.enqueue(candidate, "../outside", sha(candidate)));
        Path unknown = base.toPath().resolve("do-not-touch.txt"); Files.write(unknown, new byte[] {1});
        io(q::canEnqueue); check(Files.exists(unknown), "unexpected files never deleted"); Files.delete(unknown);
        for (int i = 0; i < 65; i++) Files.write(base.toPath().resolve("q-" + pair("strict") + "-" + id(300 + i) + ".zip.part"), new byte[0]);
        io(q::canEnqueue); check(candidate.isFile(), "bounded directory admission preserves source");
    }
    private static void locking(Path root) throws Exception {
        File base = root.resolve("queue").toFile(); DiagnosticsQueue q = new DiagnosticsQueue(base, pair("lock"));
        try (FileChannel channel = FileChannel.open(base.toPath().resolve(".lock"), StandardOpenOption.CREATE, StandardOpenOption.WRITE); FileLock ignored = channel.lock()) {
            io(q::canEnqueue);
        }
        check(q.canEnqueue(), "lock released after competing owner closes");
    }
    private static void links(Path root) throws Exception {
        File source = zip(root, "source.zip", 64); File base = root.resolve("queue").toFile(); DiagnosticsQueue q = new DiagnosticsQueue(base, pair("link"));
        Path link = base.toPath().resolve("q-" + pair("link") + "-" + id(400) + ".zip");
        try { Files.createSymbolicLink(link, source.toPath()); }
        catch (UnsupportedOperationException | java.nio.file.FileSystemException unavailable) { System.out.println("SKIP symlink branch: host privilege/support"); return; }
        io(q::canEnqueue); check(source.isFile(), "never follows/deletes symlink target"); Files.delete(link);
        Path redirected = root.resolve("linked-queue"); Files.createSymbolicLink(redirected, base.toPath());
        io(() -> new DiagnosticsQueue(redirected.toFile(), pair("link")));
    }
    private static void incomplete(Path root) throws Exception {
        File candidate = zip(root, "source.zip", 64); DiagnosticsQueue q = new DiagnosticsQueue(root.resolve("queue").toFile(), pair("bad"));
        io(() -> q.enqueue(candidate, id(500), id(501)));
        byte[] bytes = Files.readAllBytes(candidate.toPath()); File truncated = root.resolve("incomplete.zip").toFile(); Files.write(truncated.toPath(), java.util.Arrays.copyOf(bytes, bytes.length - 1));
        io(() -> q.enqueue(truncated, id(502), sha(truncated)));
        check(q.totalCount() == 0 && candidate.isFile() && truncated.isFile(), "failed admission never deletes caller candidates");
    }
}
