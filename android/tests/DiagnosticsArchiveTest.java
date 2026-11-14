package org.nvforever.android;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.Comparator;
import java.util.Map;
import java.util.TreeMap;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

/** Authored host fixtures only. Compile/run directly with javac/java; no Android runtime. */
public final class DiagnosticsArchiveTest {
    private static final String REVISION = "0123456789abcdef0123456789abcdef01234567";
    private static final String SOURCE = "source_revision=" + REVISION
        + "\nnative_source_revision=" + REVISION
        + "\npackage_version=0.7.0-android-dev\nandroid_api=35\n";
    private static final String RUNTIME = "exit_reason=CRASH_NATIVE\nexit_status=11\nexit_pid=42\n"
        + "exit_timestamp_ms=1234\nlow_memory_kill_supported=true\n"
        + "event=ENGINE_START pid=42 time_ms=1233 related_pid=41\n";
    private DiagnosticsArchiveTest() {}

    public static void main(String[] args) throws Exception {
        Path root = Files.createTempDirectory("nvf-diagnostics-owned-");
        try {
            scopeRedactionAndIdentity(root.resolve("scope"));
            boundedTailAndRetention(root.resolve("bounded"));
            malformedUtf8AndMetadata(root.resolve("utf8"));
            invalidLocationsAndSymlinks(root.resolve("links"));
            System.out.println("DiagnosticsArchive: 4 fixture groups passed");
        } finally {
            // All paths are inside this freshly created test root; do not follow links.
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(Comparator.reverseOrder()).toArray(Path[]::new))
                    Files.deleteIfExists(path);
            }
        }
    }
    private static Path logs(Path root) throws IOException {
        return Files.createDirectories(root.resolve("home/logs"));
    }
    private static String name(String kind, int number) {
        return kind + "-20261114-" + String.format(java.util.Locale.ROOT, "%06d", number)
            + "-42." + (kind.equals("events") ? "jsonl" : kind.equals("crash") ? "txt" : "log");
    }
    private static Map<String, String> unzip(Path zip) throws Exception {
        Map<String, String> values = new TreeMap<>();
        long total = 0;
        try (ZipFile archive = new ZipFile(zip.toFile())) {
            var entries = archive.entries();
            while (entries.hasMoreElements()) {
                ZipEntry entry = entries.nextElement();
                require(entry.getName().matches("manifest\\.json|logs/entry-[0-9]{1,2}\\.txt"),
                    "only generic archive names");
                byte[] bytes;
                try (var input = archive.getInputStream(entry)) { bytes = input.readAllBytes(); }
                total += bytes.length;
                require(total <= DiagnosticsArchive.MAX_TEXT_BYTES + 32 * 1024,
                    "bounded expanded ZIP payload");
                String text = StandardCharsets.UTF_8.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString();
                require(values.put(entry.getName(), text) == null, "no duplicate ZIP entry");
            }
        }
        return values;
    }
    private static String joined(Map<String, String> files) { return String.join("\n", files.values()); }
    private static void scopeRedactionAndIdentity(Path root) throws Exception {
        Path logs = logs(root);
        String secret = "owned-password-secret-value";
        String data = "render: device lost\nterrain: folded layers ready ✓\n"
            + "graphics: quality 'private nickname'\n"
            + "render: C:\\Users\\ExamplePerson\\private-folder\\asset.bin\n"
            + "render: /storage/emulated/0/Documents/ExamplePerson/asset.bin\n"
            + "render: " + secret + "\nrender: token=do-not-send\n"
            + "player name ExamplePerson\nloading save private-slot.fos\n"
            + "render: Authorization=Bearer xyz\nrender: user_name=ExamplePerson\n"
            + "render: mail@example.invalid\nrender: https://example.invalid/private?key=owned\n"
            + "render: abcdefghijklmnopqrstuvwxyz1234567890\n"
            + "unclassified ExamplePerson prose\n0: nvforever::terrain::prepare\n"
            + "1: nvf_play::save::read_for_load\n2: fnv_save::decode\n"
            + "3: std::panicking::begin_panic_handler\n"
            + "4: nvf_play::save::read_for_load('untrusted-argument')\n"
            + "5: nvf_play::save::read_for_load at C:\\Users\\ExamplePerson\\load.rs:42\n"
            + "6: custom_app::ExamplePerson\n"
            + "INFO nvforever::world: unclassified-module-message\n"
            + "model not loaded: unsupported synthetic vertex layout\n"
            + "model not loaded: C:\\Users\\ExamplePerson\\private-folder\\model.nif: unsupported synthetic mesh\n"
            + "model not loaded: /storage/emulated/0/saves/private-save-detail\n"
            + "INFO nvforever::world: its model did not load; drawn as the error marker (each reference of it)\n"
            + "refs_no_model=3 refs_error_marker=3\n"
            + "render: model_counts_v1 phase=WORLD transition=2 refs_no_model=3 refs_error_marker=2 skinned_meshes=7\n";
        Path original = logs.resolve(name("nvforever", 1));
        Files.writeString(original, data);
        Files.writeString(logs.resolve(name("events", 1)), "render: lifecycle ready\n");
        Files.writeString(logs.resolve("unrelated.log"), "render: unrelated-marker\n");
        Files.writeString(logs.resolve("nvforever-incorrect.log"), "render: bad-name-marker\n");
        // Use a distinct basename: Windows would otherwise overwrite the lower-case log.
        Files.writeString(logs.resolve("nvforever-20261114-000059-42.LOG"), "render: bad-case-marker\n");
        Files.writeString(logs.resolve("nvforever-20261114-000001-42.log.bak"), "render: backup-marker\n");
        Path nested = Files.createDirectory(logs.resolve("nested"));
        Files.writeString(nested.resolve(name("crash", 1)), "render: nested-marker\n");
        Path saves = Files.createDirectories(root.resolve("home/saves"));
        Files.writeString(saves.resolve(name("crash", 1)), "render: save-content-marker\n");
        Path first = root.resolve("one.zip");
        DiagnosticsArchive.Snapshot one = DiagnosticsArchive.collect(logs.toFile(), first.toFile(),
            SOURCE, RUNTIME, secret);
        Map<String, String> files = unzip(first);
        String text = joined(files);
        for (String excluded : new String[] {secret, "ExamplePerson", "private nickname", "private-slot",
                "do-not-send", "mail@", "https:", "abcdefghijklmnopqrstuvwxyz1234567890",
                "unrelated-marker", "bad-name-marker", "bad-case-marker", "backup-marker",
                "nested-marker", "save-content-marker", "untrusted-argument",
                "unclassified-module-message", "private-save-detail", name("nvforever", 1)})
            require(!text.contains(excluded), "private/out-of-scope content omitted: " + excluded);
        require(text.contains("device lost") && text.contains("folded layers ready ✓"), "technical UTF-8 retained");
        require(text.contains("nvforever::terrain::prepare"), "technical stack function retained");
        require(text.contains("1: nvf_play::save::read_for_load\n")
            && text.contains("2: fnv_save::decode\n")
            && text.contains("3: std::panicking::begin_panic_handler\n"),
            "strict known-crate save/panic frames survive private-category words");
        require(text.contains("model not loaded: unsupported synthetic vertex layout\n")
            && text.contains("model not loaded: [path] unsupported synthetic mesh\n")
            && text.contains("its model did not load; drawn as the error marker")
            && text.contains("refs_no_model=3 refs_error_marker=3\n"),
            "specific model-failure and marker diagnostics survive with private paths removed");
        require(text.contains("render: model_counts_v1 phase=WORLD transition=2 refs_no_model=3 refs_error_marker=2 skinned_meshes=7\n"),
            "exact numeric loading-transition discriminator survives unchanged");
        require(text.contains("source_revision=" + REVISION) && text.contains("event=ENGINE_START"),
            "strict build and restart identity retained");
        require(one.snapshotId.matches("[a-f0-9]{64}") && one.archiveSha256.matches("[a-f0-9]{64}"),
            "separate content/archive identifiers");
        require(one.archiveSha256.equals(hash(Files.readAllBytes(first))), "actual ZIP hash");
        require(one.manifestSha256.equals(hash(files.get("manifest.json").getBytes(StandardCharsets.UTF_8))),
            "actual manifest hash");
        require(one.archiveBytes == Files.size(first) && one.entryCount == files.size(), "actual archive receipt");
        require(Files.readString(original).equals(data), "original log unchanged");
        Files.move(original, logs.resolve(name("nvforever", 2)));
        DiagnosticsArchive.Snapshot renamed = DiagnosticsArchive.collect(logs.toFile(), root.resolve("two.zip").toFile(),
            SOURCE, RUNTIME, secret);
        require(one.snapshotId.equals(renamed.snapshotId), "snapshot identity independent of filenames/output path");
        Files.writeString(logs.resolve(name("nvforever", 3)), data);
        DiagnosticsArchive.Snapshot duplicate = DiagnosticsArchive.collect(logs.toFile(), root.resolve("three.zip").toFile(),
            SOURCE, RUNTIME, secret);
        require(!duplicate.snapshotId.equals(one.snapshotId), "multiplicity changes canonical identity");
    }
    private static void boundedTailAndRetention(Path root) throws Exception {
        Path logs = logs(root);
        for (int i = 0; i < DiagnosticsArchive.MAX_FILES + 5; i++)
            Files.writeString(logs.resolve(name("nvforever", i)), "render: retained-number-" + i + "\n");
        DiagnosticsArchive.Snapshot limited = DiagnosticsArchive.collect(logs.toFile(), root.resolve("retained.zip").toFile(),
            SOURCE, RUNTIME, "");
        require(limited.entryCount == DiagnosticsArchive.MAX_FILES + 3, "bounded log count plus metadata/manifest");
        String retained = joined(unzip(root.resolve("retained.zip")));
        require(!retained.contains("retained-number-0\n") && retained.contains("retained-number-28\n"),
            "newest files selected without source names in metadata");
        require(retained.contains("\"omitted_files\":5"), "retention omissions explicit");
        // Raw tails can grow during redaction: repeated one-character paths become [path].
        // Exercise aggregate admission on real files, not just a constant/helper assertion.
        String line = "render: " + "/ ".repeat(3000) + "\n";
        String big = line.repeat(100) + "render: tail-marker\n";
        for (int i = 0; i < DiagnosticsArchive.MAX_FILES; i++)
            Files.writeString(logs.resolve(name("nvforever", i + 100)), big);
        Path archive = root.resolve("bounded.zip");
        DiagnosticsArchive.Snapshot bounded = DiagnosticsArchive.collect(logs.toFile(), archive.toFile(),
            SOURCE, RUNTIME, "");
        String contents = joined(unzip(archive));
        require(bounded.archiveBytes <= DiagnosticsArchive.MAX_ARCHIVE_BYTES, "hard ZIP size bound");
        require(contents.contains("tail-marker"), "tail keeps recent technical diagnostics");
        require(!contents.contains("\"truncated_files\":0"), "tail truncation explicit");
        require(!contents.contains("\"omitted_files\":0"), "aggregate/retention omission explicit");
        try (var files = Files.list(root)) {
            require(files.filter(p -> p.toString().endsWith(".zip")).count() == 2,
                "collector creates only requested outputs; queue owns retention");
        }
    }
    private static void malformedUtf8AndMetadata(Path root) throws Exception {
        Path logs = logs(root);
        Files.write(logs.resolve(name("crash", 1)), new byte[] {(byte) 0xc3, 0x28, '\n'});
        String metadata = SOURCE + "username=ExamplePerson\nsource_revision=not-a-revision\n"
            + "package_version=private value\nsource_path=/private/path\n";
        String runtime = RUNTIME + "event=bad-lowercase\nexit_description=personal-message\n"
            + "event=ENGINE_START pid=42 malicious=oops\n";
        Path archive = root.resolve("utf8.zip");
        DiagnosticsArchive.collect(logs.toFile(), archive.toFile(), metadata, runtime, "");
        String text = joined(unzip(archive));
        require(text.contains("[invalid UTF-8 omitted]"), "malformed bytes fail closed");
        require(text.contains(REVISION) && text.contains("exit_reason=CRASH_NATIVE"), "strict metadata survives");
        for (String excluded : new String[] {"ExamplePerson", "not-a-revision", "private value", "/private/path",
                "bad-lowercase", "personal-message", "malicious", "oops"})
            require(!text.contains(excluded), "metadata allowlist rejects arbitrary values");
        // An oversized line is omitted whole, not cut through a private value.
        Files.writeString(logs.resolve(name("nvforever", 2)), "render: " + "x".repeat(9000)
            + "\nrender: after-oversized-line\n");
        Path second = root.resolve("large-line.zip");
        DiagnosticsArchive.collect(logs.toFile(), second.toFile(), SOURCE, RUNTIME, "");
        String clean = joined(unzip(second));
        require(!clean.contains("x".repeat(100)) && clean.contains("after-oversized-line"), "line bound is fail closed");
        Files.writeString(logs.resolve(name("nvforever", 3)),
            "render: complete line\nrender: q7w8e9k2m3");
        Path partial = root.resolve("partial.zip");
        DiagnosticsArchive.collect(logs.toFile(), partial.toFile(), SOURCE, RUNTIME,
            "q7w8e9k2m3n4p5r6s7t8u9v0w1x2y3z4");
        String partialText = joined(unzip(partial));
        require(partialText.contains("complete line") && !partialText.contains("q7w8e9k2m3"),
            "unterminated concurrent append cannot expose a secret fragment");
    }
    private static void invalidLocationsAndSymlinks(Path root) throws Exception {
        Path logs = logs(root);
        Path existing = root.resolve("existing.zip");
        Files.writeString(existing, "owned existing output");
        fails(() -> DiagnosticsArchive.collect(logs.toFile(), existing.toFile(), SOURCE, RUNTIME, ""));
        require(Files.readString(existing).equals("owned existing output"), "existing outputs not overwritten");
        fails(() -> DiagnosticsArchive.collect(root.toFile(), root.resolve("bad.zip").toFile(), SOURCE, RUNTIME, ""));
        fails(() -> DiagnosticsArchive.collect(logs.toFile(), logs.resolve("bad.zip").toFile(), SOURCE, RUNTIME, ""));
        fails(() -> DiagnosticsArchive.collect(logs.resolve("../logs").toFile(),
            root.resolve("traversal.zip").toFile(), SOURCE, RUNTIME, ""));
        require(!Files.exists(logs.resolve("bad.zip")), "refused output creates no file");
        Path foreign = root.resolve("foreign.txt");
        Files.writeString(foreign, "render: symlink-private-marker\n");
        Path linked = logs.resolve(name("crash", 9));
        try { Files.createSymbolicLink(linked, foreign.toAbsolutePath()); }
        catch (UnsupportedOperationException | java.nio.file.FileSystemException e) {
            System.out.println("DiagnosticsArchive: symlink fixtures unavailable on this host");
            return;
        }
        Path archive = root.resolve("symlink.zip");
        DiagnosticsArchive.collect(logs.toFile(), archive.toFile(), SOURCE, RUNTIME, "");
        require(!joined(unzip(archive)).contains("symlink-private-marker"), "symlink entry never followed");
        Path alias = root.resolve("alias");
        Files.createSymbolicLink(alias, root.toAbsolutePath());
        fails(() -> DiagnosticsArchive.collect(alias.resolve("home/logs").toFile(), root.resolve("alias.zip").toFile(),
            SOURCE, RUNTIME, ""));
        Path outAlias = root.resolve("out-alias");
        Files.createSymbolicLink(outAlias, root.toAbsolutePath());
        fails(() -> DiagnosticsArchive.collect(logs.toFile(), outAlias.resolve("out.zip").toFile(), SOURCE, RUNTIME, ""));
        require(!Files.exists(root.resolve("out.zip")), "symlink output parent refused");
    }
    private interface Operation { void run() throws Exception; }
    private static void fails(Operation op) throws Exception {
        try { op.run(); }
        catch (IOException e) {
            require(e.getCause() == null && e.getMessage().equals("Diagnostics collection failed"),
                "exceptions never disclose raw paths/messages");
            return;
        }
        throw new AssertionError("expected refused collection");
    }
    private static String hash(byte[] bytes) throws Exception {
        byte[] digest = MessageDigest.getInstance("SHA-256").digest(bytes);
        StringBuilder out = new StringBuilder();
        for (byte b : digest) out.append(String.format(java.util.Locale.ROOT, "%02x", b & 255));
        return out.toString();
    }
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
