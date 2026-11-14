package org.nvforever.android;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.RandomAccessFile;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.channels.OverlappingFileLockException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/** Owned generated snapshots only. Never uploads, deletes source logs, or stores pairing secrets.
 * The manager must cancel its transport Cancellation on OFF/pair changes before starting another.
 * Each operation takes a nonblocking cross-process lock; callers retry on a later launch if busy.
 */
public final class DiagnosticsQueue {
    public static final int MAX_PENDING = 4;
    public static final long MAX_TOTAL_BYTES = 48L * 1024 * 1024;
    public static final long MAX_SNAPSHOT_BYTES = 16L * 1024 * 1024;
    public static final long FIRST_RETRY_MILLIS = 30_000L;
    public static final long MAX_RETRY_MILLIS = 15L * 60 * 1000;
    private static final int MAX_PAIRS = 4, MAX_ACKS = 64, MAX_FILES = 64;
    private static final Pattern HEX = Pattern.compile("[0-9a-f]{64}");
    private static final Pattern PENDING = Pattern.compile("q-[0-9a-f]{64}-[0-9a-f]{64}\\.(zip|meta)");
    private static final Pattern ACK = Pattern.compile("a-[0-9a-f]{64}\\.ack");
    private static final Pattern PART = Pattern.compile("(q-[0-9a-f]{64}-[0-9a-f]{64}\\.(zip|meta)|a-[0-9a-f]{64}\\.ack)\\.part");
    private final Path directory;
    private final String pairing;
    private final BeforeCommit beforeCommit;
    private volatile String status = "Ready";

    interface BeforeCommit { void run() throws IOException; }
    private interface Operation<T> { T run() throws IOException; }
    public DiagnosticsQueue(File baseDir, String pairingKey) throws IOException {
        this(baseDir, pairingKey, () -> { });
    }
    DiagnosticsQueue(File baseDir, String pairingKey, BeforeCommit beforeCommit) throws IOException {
        requireHex(pairingKey);
        if (baseDir == null || beforeCommit == null) throw new IOException("Invalid queue configuration");
        directory = baseDir.toPath().toAbsolutePath().normalize();
        // Reject redirected paths, including a symlink in any existing ancestor.
        noLinks(directory);
        Files.createDirectories(directory);
        if (!Files.isDirectory(directory, LinkOption.NOFOLLOW_LINKS)) throw new IOException("Queue directory unavailable");
        pairing = pairingKey;
        this.beforeCommit = beforeCommit;
    }
    public static String pairingKey(String endpoint, String token) {
        if (endpoint == null || token == null) throw new IllegalArgumentException("Pairing values required");
        MessageDigest digest = digest();
        digest.update(endpoint.getBytes(StandardCharsets.UTF_8));
        digest.update((byte) 0);
        return hex(digest.digest(token.getBytes(StandardCharsets.UTF_8)));
    }
    public String status() { return status; }
    public synchronized boolean canEnqueue() throws IOException {
        return locked(() -> capacity(scan(), MAX_SNAPSHOT_BYTES));
    }
    public synchronized boolean containsSnapshot(String id) throws IOException {
        requireHex(id);
        return locked(() -> {
            State s = scan();
            return s.acknowledged(pairing).containsKey(id) || s.find(pairing, id) != null;
        });
    }
    /** Copies a completed candidate; the caller retains ownership of that candidate. */
    public synchronized boolean enqueue(File candidate, String snapshotId, String archiveSha256) throws IOException {
        requireHex(snapshotId); requireHex(archiveSha256);
        if (candidate == null) throw new IOException("Snapshot candidate required");
        return locked(() -> {
            State s = scan();
            if (s.acknowledged(pairing).containsKey(snapshotId) || s.find(pairing, snapshotId) != null) {
                status = "Snapshot already queued or acknowledged"; return false;
            }
            Path source = candidate.toPath().toAbsolutePath().normalize();
            regular(source);
            if (source.startsWith(directory)) throw new IOException("Candidate must be outside the owned queue");
            long size = Files.size(source);
            if (size <= 0 || size > MAX_SNAPSHOT_BYTES) throw new IOException("Snapshot size is outside bounds");
            if (!capacity(s, size)) return false;
            completeZip(source, size);
            Record r = new Record(pairing, snapshotId, archiveSha256, size, Math.max(0, System.currentTimeMillis()), 0, 0);
            Path zip = zip(r), meta = meta(r), zipPart = part(zip), metaPart = part(meta);
            boolean committed = false;
            try {
                String copiedHash = copy(source, zipPart, size);
                if (!copiedHash.equals(archiveSha256)) throw new IOException("Snapshot checksum does not match");
                writeNew(metaPart, r.encode());
                beforeCommit.run();
                move(zipPart, zip, false);
                // Metadata is the commit marker: an interrupted orphan ZIP is never uploadable.
                move(metaPart, meta, false);
                committed = true;
                status = "Snapshot queued";
                return true;
            } finally {
                Files.deleteIfExists(zipPart); Files.deleteIfExists(metaPart);
                if (!committed && !Files.exists(meta, LinkOption.NOFOLLOW_LINKS)) Files.deleteIfExists(zip);
            }
        });
    }
    /** Returns only this pairing's immutable due entry; no old pairing is ever transferred. */
    public synchronized Entry next(long nowMillis) throws IOException {
        final long now = Math.max(0, nowMillis);
        return locked(() -> {
            State s = scan();
            s.pending.sort(Comparator.comparingLong(r -> r.created));
            for (Record r : s.pending) {
                if (!r.pair.equals(pairing)) continue;
                // A wall-clock reset must not suppress retries indefinitely.
                if (r.next > now && r.next - now <= MAX_RETRY_MILLIS) continue;
                Path file = zip(r);
                if (!hash(file, r.bytes).equals(r.sha)) {
                    discard(r); status = "Damaged generated snapshot discarded"; continue;
                }
                status = "Snapshot ready";
                return new Entry(file.toFile(), r.id, r.sha, r.pair);
            }
            status = s.pending.isEmpty() ? "Queue empty" : "Waiting for retry or an older pairing";
            return null;
        });
    }
    public synchronized void acknowledge(Entry entry) throws IOException {
        locked(() -> {
            State s = scan(); Record r = current(s, entry);
            if (r == null) return null;
            LinkedHashMap<String, String> history = s.acknowledged(pairing);
            history.remove(r.id); history.put(r.id, r.sha);
            while (history.size() > MAX_ACKS) history.remove(history.keySet().iterator().next());
            StringBuilder text = new StringBuilder();
            for (Map.Entry<String, String> a : history.entrySet()) text.append(a.getKey()).append(' ').append(a.getValue()).append('\n');
            atomicWrite(directory.resolve("a-" + pairing + ".ack"), text.toString().getBytes(StandardCharsets.US_ASCII));
            discard(r); status = "Snapshot acknowledged"; return null;
        });
    }
    public synchronized void failed(Entry entry, long nowMillis) throws IOException {
        locked(() -> {
            Record r = current(scan(), entry); if (r == null) return null;
            long delay = Math.min(MAX_RETRY_MILLIS, FIRST_RETRY_MILLIS << Math.min(r.failures, 5));
            r.failures = Math.min(31, r.failures + 1);
            long now = Math.max(0, nowMillis);
            r.next = now > Long.MAX_VALUE - delay ? Long.MAX_VALUE : now + delay;
            atomicWrite(meta(r), r.encode()); status = "Waiting for retry"; return null;
        });
    }
    public synchronized int count() throws IOException {
        return locked(() -> { int count = 0; for (Record r : scan().pending) if (r.pair.equals(pairing)) count++; return count; });
    }
    public synchronized int totalCount() throws IOException { return locked(() -> scan().pending.size()); }
    public static final class Entry {
        public final File file;
        public final String snapshotId, sha256, pairingKey;
        private Entry(File file, String id, String sha, String pair) { this.file = file; snapshotId = id; sha256 = sha; pairingKey = pair; }
    }
    private Record current(State s, Entry entry) throws IOException {
        if (entry == null || !pairing.equals(entry.pairingKey)) throw new IOException("Entry belongs to another pairing");
        Record r = s.find(pairing, entry.snapshotId);
        if (r != null && (!r.sha.equals(entry.sha256) || !zip(r).toFile().equals(entry.file))) throw new IOException("Stale queue entry");
        return r;
    }
    private boolean capacity(State s, long bytes) {
        if (!s.pairs.contains(pairing) && s.pairs.size() >= MAX_PAIRS) { status = "Pairing history full; old pairings retained"; return false; }
        if (s.pending.size() >= MAX_PENDING || bytes > MAX_TOTAL_BYTES - s.bytes) { status = "Queue full; unacknowledged snapshots retained"; return false; }
        status = "Ready"; return true;
    }
    private <T> T locked(Operation<T> operation) throws IOException {
        noLinks(directory);
        if (!Files.isDirectory(directory, LinkOption.NOFOLLOW_LINKS)) throw new IOException("Queue directory unavailable");
        Path lockPath = directory.resolve(".lock");
        if (Files.isSymbolicLink(lockPath)) throw new IOException("Queue links are not allowed");
        try (FileChannel channel = FileChannel.open(lockPath, StandardOpenOption.CREATE, StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS)) {
            try (FileLock lock = channel.tryLock()) {
                if (lock == null) { status = "Queue busy"; throw new IOException(status); }
                return operation.run();
            } catch (OverlappingFileLockException busy) { status = "Queue busy"; throw new IOException(status); }
        }
    }
    private static final class State {
        final List<Record> pending = new ArrayList<>();
        final Map<String, LinkedHashMap<String, String>> acks = new HashMap<>();
        final Set<String> pairs = new HashSet<>();
        long bytes;
        LinkedHashMap<String, String> acknowledged(String pair) { return acks.computeIfAbsent(pair, ignored -> new LinkedHashMap<>()); }
        Record find(String pair, String id) { for (Record r : pending) if (r.pair.equals(pair) && r.id.equals(id)) return r; return null; }
    }
    private State scan() throws IOException {
        List<Path> files = new ArrayList<>();
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(directory)) {
            for (Path p : stream) {
                if (files.size() >= MAX_FILES) { status = "Queue file limit reached; collection paused"; throw new IOException(status); }
                String name = p.getFileName().toString();
                if (!name.equals(".lock") && !PENDING.matcher(name).matches() && !ACK.matcher(name).matches() && !PART.matcher(name).matches()) {
                    status = "Unexpected queue files; collection paused"; throw new IOException(status);
                }
                regular(p); files.add(p);
            }
        }
        State s = new State(); Set<String> names = new HashSet<>();
        for (Path p : files) names.add(p.getFileName().toString());
        for (Path p : files) {
            String name = p.getFileName().toString();
            if (PART.matcher(name).matches()) { Files.delete(p); continue; }
            if (ACK.matcher(name).matches()) {
                String pair = name.substring(2, 66); LinkedHashMap<String, String> acks = new LinkedHashMap<>();
                String text = ascii(p, MAX_ACKS * 130);
                for (String line : text.split("\n")) {
                    if (line.length() != 129 || line.charAt(64) != ' ') throw new IOException("Invalid acknowledgement history");
                    String id = line.substring(0, 64), sha = line.substring(65);
                    requireHex(id); requireHex(sha);
                    if (acks.put(id, sha) != null || acks.size() > MAX_ACKS) throw new IOException("Invalid acknowledgement history");
                }
                s.acks.put(pair, acks); s.pairs.add(pair);
            }
        }
        for (Path p : files) {
            String name = p.getFileName().toString();
            if (!PENDING.matcher(name).matches() || !name.endsWith(".meta")) continue;
            Record r;
            try { r = Record.parse(ascii(p, 2048)); }
            catch (IOException invalid) { removePair(p); status = "Invalid generated queue metadata discarded"; continue; }
            if (!meta(r).equals(p) || !names.contains(zip(r).getFileName().toString()) || Files.size(zip(r)) != r.bytes) {
                removePair(p); status = "Incomplete generated snapshot discarded"; continue;
            }
            String ack = s.acknowledged(r.pair).get(r.id);
            if (ack != null) {
                if (!ack.equals(r.sha)) throw new IOException("Snapshot identity conflicts with acknowledgement");
                discard(r); continue;
            }
            s.pending.add(r); s.bytes += r.bytes; s.pairs.add(r.pair);
        }
        for (Path p : files) {
            String name = p.getFileName().toString();
            if (PENDING.matcher(name).matches() && name.endsWith(".zip") && !names.contains(name.substring(0, name.length() - 4) + ".meta")) Files.deleteIfExists(p);
        }
        if (s.pending.size() > MAX_PENDING || s.bytes > MAX_TOTAL_BYTES || s.pairs.size() > MAX_PAIRS) {
            status = "Queue bounds exceeded; snapshots retained"; throw new IOException(status);
        }
        return s;
    }
    private void removePair(Path metadata) throws IOException {
        String name = metadata.getFileName().toString();
        Path zip = directory.resolve(name.substring(0, name.length() - 5) + ".zip");
        if (Files.exists(zip, LinkOption.NOFOLLOW_LINKS)) { regular(zip); Files.delete(zip); }
        Files.delete(metadata);
    }
    private void discard(Record r) throws IOException { Files.deleteIfExists(meta(r)); Files.deleteIfExists(zip(r)); }
    private Path zip(Record r) { return directory.resolve("q-" + r.pair + "-" + r.id + ".zip"); }
    private Path meta(Record r) { return directory.resolve("q-" + r.pair + "-" + r.id + ".meta"); }
    private static Path part(Path target) { return target.resolveSibling(target.getFileName() + ".part"); }
    private static void noLinks(Path path) throws IOException {
        for (Path p = path; p != null; p = p.getParent()) {
            if (Files.isSymbolicLink(p)) throw new IOException("Queue links are not allowed");
        }
    }
    private static void regular(Path path) throws IOException {
        noLinks(path);
        if (!Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)) throw new IOException("Queue links or non-files are not allowed");
    }
    private static String ascii(Path p, int limit) throws IOException {
        if (Files.size(p) > limit) throw new IOException("Queue metadata exceeds bounds");
        byte[] bytes;
        try (InputStream in = Files.newInputStream(p, StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)) {
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
            byte[] buffer = new byte[Math.min(1024, limit + 1)]; int n;
            while ((n = in.read(buffer, 0, Math.min(buffer.length, limit + 1 - out.size()))) > 0) {
                out.write(buffer, 0, n); if (out.size() > limit) break;
            }
            bytes = out.toByteArray();
        }
        if (bytes.length > limit) throw new IOException("Queue metadata exceeds bounds");
        for (byte b : bytes) if (b != '\n' && (b < 32 || b > 126)) throw new IOException("Invalid queue metadata");
        return new String(bytes, StandardCharsets.US_ASCII);
    }
    private static void atomicWrite(Path target, byte[] bytes) throws IOException {
        Path temporary = part(target);
        try { writeNew(temporary, bytes); move(temporary, target, true); }
        finally { Files.deleteIfExists(temporary); }
    }
    private static void writeNew(Path target, byte[] bytes) throws IOException {
        try (FileChannel channel = FileChannel.open(target, StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS)) {
            ByteBuffer data = ByteBuffer.wrap(bytes); while (data.hasRemaining()) channel.write(data); channel.force(true);
        }
    }
    private static void move(Path from, Path to, boolean replace) throws IOException {
        try {
            if (replace) Files.move(from, to, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            else Files.move(from, to, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException unsupported) {
            if (replace) Files.move(from, to, StandardCopyOption.REPLACE_EXISTING); else Files.move(from, to);
        }
    }
    private static String copy(Path source, Path target, long expected) throws IOException {
        MessageDigest digest = digest(); long total = 0;
        try (InputStream in = Files.newInputStream(source, StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS);
             FileChannel out = FileChannel.open(target, StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS)) {
            byte[] buffer = new byte[8192]; int n;
            while ((n = in.read(buffer)) != -1) {
                total += n; if (total > expected || total > MAX_SNAPSHOT_BYTES) throw new IOException("Snapshot changed while copying");
                digest.update(buffer, 0, n); ByteBuffer data = ByteBuffer.wrap(buffer, 0, n); while (data.hasRemaining()) out.write(data);
            }
            if (total != expected) throw new IOException("Snapshot changed while copying");
            out.force(true);
        }
        return hex(digest.digest());
    }
    private static String hash(Path path, long expected) throws IOException {
        MessageDigest digest = digest(); long total = 0;
        try (InputStream in = Files.newInputStream(path, StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)) {
            byte[] buffer = new byte[8192]; int n;
            while ((n = in.read(buffer)) != -1) { total += n; if (total > expected) throw new IOException("Snapshot changed"); digest.update(buffer, 0, n); }
        }
        if (total != expected) throw new IOException("Snapshot changed");
        return hex(digest.digest());
    }
    private static void completeZip(Path file, long size) throws IOException {
        if (size < 22) throw new IOException("Snapshot is not a complete ZIP");
        try (RandomAccessFile in = new RandomAccessFile(file.toFile(), "r")) {
            if (in.readUnsignedByte() != 'P' || in.readUnsignedByte() != 'K') throw new IOException("Snapshot is not a ZIP");
            int length = (int) Math.min(size, 65557); byte[] tail = new byte[length]; in.seek(size - length); in.readFully(tail);
            for (int i = tail.length - 22; i >= 0; i--) {
                if (tail[i] == 'P' && tail[i + 1] == 'K' && tail[i + 2] == 5 && tail[i + 3] == 6 && i + 22 + u16(tail, i + 20) == tail.length) {
                    long end = size - length + i;
                    if (u16(tail, i + 4) == 0 && u16(tail, i + 6) == 0 && u16(tail, i + 8) == u16(tail, i + 10)
                            && u32(tail, i + 12) + u32(tail, i + 16) <= end) return;
                }
            }
        }
        throw new IOException("Snapshot is not a complete ZIP");
    }
    private static int u16(byte[] b, int i) { return (b[i] & 255) | ((b[i + 1] & 255) << 8); }
    private static long u32(byte[] b, int i) { return (long) u16(b, i) | ((long) u16(b, i + 2) << 16); }
    private static MessageDigest digest() { try { return MessageDigest.getInstance("SHA-256"); } catch (NoSuchAlgorithmException impossible) { throw new AssertionError(impossible); } }
    private static String hex(byte[] bytes) { StringBuilder out = new StringBuilder(64); for (byte b : bytes) out.append(Character.forDigit((b >>> 4) & 15, 16)).append(Character.forDigit(b & 15, 16)); return out.toString(); }
    private static void requireHex(String value) throws IOException { if (value == null || !HEX.matcher(value).matches()) throw new IOException("Invalid snapshot identity"); }
    private static final class Record {
        final String pair, id, sha; final long bytes, created; int failures; long next;
        Record(String pair, String id, String sha, long bytes, long created, int failures, long next) { this.pair = pair; this.id = id; this.sha = sha; this.bytes = bytes; this.created = created; this.failures = failures; this.next = next; }
        byte[] encode() { return ("v=1\npair=" + pair + "\nid=" + id + "\nsha=" + sha + "\nbytes=" + bytes + "\ncreated=" + created + "\nfailures=" + failures + "\nnext=" + next + "\n").getBytes(StandardCharsets.US_ASCII); }
        static Record parse(String text) throws IOException {
            String[] lines = text.split("\n", -1); String[] keys = {"v", "pair", "id", "sha", "bytes", "created", "failures", "next"};
            if (lines.length != 9 || !lines[8].isEmpty()) throw new IOException("Invalid snapshot metadata");
            String[] values = new String[8];
            for (int i = 0; i < keys.length; i++) { String prefix = keys[i] + "="; if (!lines[i].startsWith(prefix)) throw new IOException("Invalid snapshot metadata"); values[i] = lines[i].substring(prefix.length()); }
            if (!values[0].equals("1")) throw new IOException("Unknown snapshot metadata version");
            requireHex(values[1]); requireHex(values[2]); requireHex(values[3]);
            try {
                for (int i = 4; i < 8; i++) if (!values[i].matches("[0-9]{1,19}")) throw new NumberFormatException();
                long bytes = Long.parseLong(values[4]), created = Long.parseLong(values[5]), failures = Long.parseLong(values[6]), next = Long.parseLong(values[7]);
                if (bytes <= 0 || bytes > MAX_SNAPSHOT_BYTES || failures > 31) throw new NumberFormatException();
                return new Record(values[1], values[2], values[3], bytes, created, (int) failures, next);
            } catch (NumberFormatException invalid) { throw new IOException("Invalid snapshot metadata"); }
        }
    }
}
