package org.nvforever.android;

import java.io.File;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.channels.Channels;
import java.nio.channels.SeekableByteChannel;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.nio.file.OpenOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.regex.Pattern;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

/**
 * [design] A bounded, local projection of NVForever diagnostics, never a general file
 * uploader. Only direct home/logs engine files are considered. Unknown lines and
 * sensitive categories are omitted, not copied verbatim. This is conservative text
 * filtering, not a guarantee that arbitrary free-form text contains no personal data.
 * The caller owns the output queue, retention and acknowledgement; originals stay put.
 */
public final class DiagnosticsArchive {
    public static final long MAX_ARCHIVE_BYTES = 16L * 1024 * 1024;
    public static final int MAX_FILES = 24;
    public static final int MAX_TAIL_BYTES = 512 * 1024;
    public static final int MAX_TEXT_BYTES = 12 * 1024 * 1024;
    private static final int MAX_SCANNED = 1024;
    private static final int MAX_LINE = 8192;
    private static final int MAX_METADATA = 32 * 1024;
    private static final Pattern OWNED = Pattern.compile(
        "(?:nvforever-[0-9]{8}-[0-9]{6}-[0-9]{1,10}\\.log|"
        + "events-[0-9]{8}-[0-9]{6}-[0-9]{1,10}\\.jsonl|"
        + "crash-[0-9]{8}-[0-9]{6}-[0-9]{1,10}\\.txt)");
    private static final Pattern PRIVATE = Pattern.compile(
        "(?i)(player|save|auth|token|password|passwd|secret|cookie|credential|"
        + "session|username|user_name|account|email|serial|imei|android.?id|"
        + "command.?line|\\bargs\\b|https?://|\\bBearer\\b|\\bBasic\\b)");
    private static final Pattern TECHNICAL = Pattern.compile(
        "(?i)(graphics:|render:|gpu\\b|adapterinfo\\b|memory budget:|terrain:|"
        + "generated lod\\b|loading screen:|pipeline\\b|texture not loaded|"
        + "\\bmodel not loaded\\b|\\bits model did not load; drawn as the error marker\\b|"
        + "\\brefs_no_model\\b|\\brefs_error_marker\\b|"
        + "panicked at|stack backtrace:)");
    // Only a complete numbered frame with a known crate and identifier can bypass
    // free-form filtering. "save" or "player" inside a code identifier is useful;
    // arguments, personal paths and unclassified logger module prefixes are not.
    private static final Pattern FRAME = Pattern.compile(
        "^\\s*([0-9]{1,4}):\\s+(?:0x[0-9a-fA-F]{1,16}\\s+-\\s+)?"
        + "((?:nvforever|nvf_[a-z0-9_]+|fnv_[a-z0-9_]+|bevy(?:_[a-z0-9_]+)?|"
        + "wgpu(?:_[a-z0-9_]+)?|naga|std|core|alloc|rapier3d(?:_f64)?|"
        + "parry3d(?:_f64)?|ash|jni|winit)::[A-Za-z0-9_:<>$.-]{1,512})\\s*$");
    private static final Pattern QUOTED = Pattern.compile("\"[^\"\\r\\n]*\"|'[^'\\r\\n]*'");
    private static final Pattern PATH = Pattern.compile(
        "(?:[A-Za-z]:[\\\\/]|\\\\\\\\|/|file:|content:)[^\\s\"'<>]*");
    private static final Pattern EMAIL = Pattern.compile(
        "[A-Za-z0-9._%+\\-]+@[A-Za-z0-9.\\-]+");
    private static final Pattern IDENTIFIER = Pattern.compile(
        "(?<![A-Za-z0-9])[A-Za-z0-9_+./=\\-]{32,}(?![A-Za-z0-9])");
    private static final Pattern JOURNAL = Pattern.compile(
        "event=[A-Z][A-Z0-9_]{0,47}(?: (?:pid|time_ms|related_pid)=[0-9]{1,20}){0,3}");
    private DiagnosticsArchive() {}

    public static final class Snapshot {
        public final String snapshotId;
        public final String archiveSha256;
        public final String manifestSha256;
        public final long archiveBytes;
        public final int entryCount;
        private Snapshot(String id, String archive, String manifest, long size, int count) {
            snapshotId = id;
            archiveSha256 = archive;
            manifestSha256 = manifest;
            archiveBytes = size;
            entryCount = count;
        }
    }

    private static final class Part {
        final String kind;
        final byte[] data;
        final String sha;
        Part(String kind, byte[] data) {
            this.kind = kind;
            this.data = data;
            sha = hash(data);
        }
    }
    private static final class Counts {
        int omittedFiles, truncatedFiles, omittedLines;
    }

    /**
     * Metadata accepts only fixed source keys and numeric/enum exit/journal rows.
     * Never supply raw intents, exception messages, environment or device identifiers.
     * outputZip must not exist; failed writes remove only the newly created output.
     */
    public static Snapshot collect(File logsDir, File outputZip, String sourceText,
                                   String runtimeSummary, String secret) throws IOException {
        try { return collectChecked(logsDir, outputZip, sourceText, runtimeSummary, secret); }
        catch (IOException | RuntimeException e) {
            // Never expose an exception's filesystem path, message or secret-bearing cause.
            throw new IOException("Diagnostics collection failed");
        }
    }

    private static Snapshot collectChecked(File logsDir, File outputZip, String sourceText,
                                          String runtimeSummary, String secret) throws IOException {
        if (logsDir == null || outputZip == null) throw new IOException("Missing diagnostics location");
        noTraversal(logsDir.toPath());
        noTraversal(outputZip.toPath());
        Path logs = logsDir.toPath().toAbsolutePath().normalize();
        Path out = outputZip.toPath().toAbsolutePath().normalize();
        if (logs.getFileName() == null || !logs.getFileName().toString().equals("logs")
            || logs.getParent() == null || logs.getParent().getFileName() == null
            || !logs.getParent().getFileName().toString().equals("home"))
            throw new IOException("Not an engine logs directory");
        noLinks(logs);
        Path parent = out.getParent();
        if (parent == null || out.startsWith(logs)) throw new IOException("Invalid diagnostics output");
        noLinks(parent);
        if (!Files.isDirectory(parent, LinkOption.NOFOLLOW_LINKS)
            || Files.exists(out, LinkOption.NOFOLLOW_LINKS))
            throw new IOException("Diagnostics output unavailable");
        if (secret != null && secret.length() > 4096) throw new IOException("Invalid diagnostics secret");
        Counts counts = new Counts();
        List<Part> parts = new ArrayList<>();
        parts.add(new Part("source", metadata(sourceText, true, secret, counts)));
        parts.add(new Part("runtime", metadata(runtimeSummary, false, secret, counts)));
        List<Path> candidates = new ArrayList<>();
        if (Files.isDirectory(logs, LinkOption.NOFOLLOW_LINKS)) {
            try (DirectoryStream<Path> stream = Files.newDirectoryStream(logs)) {
                int scanned = 0;
                for (Path file : stream) {
                    if (++scanned > MAX_SCANNED) throw new IOException("Too many diagnostics entries");
                    if (OWNED.matcher(file.getFileName().toString()).matches()
                        && !Files.isSymbolicLink(file)
                        && Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)) candidates.add(file);
                }
            }
        } else if (Files.exists(logs, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("Invalid engine logs directory");
        }
        // Strict timestamp names permit chronological retention without uploading names.
        candidates.sort(Comparator.comparing((Path p) -> stamp(p.getFileName().toString()))
            .reversed().thenComparing(p -> p.getFileName().toString()));
        int total = parts.get(0).data.length + parts.get(1).data.length;
        for (Path file : candidates) {
            if (parts.size() - 2 >= MAX_FILES) { counts.omittedFiles++; continue; }
            byte[] tail;
            try { tail = tail(file, logs, counts); }
            catch (IOException e) { counts.omittedFiles++; continue; }
            byte[] clean = scrub(tail, secret, counts);
            if (clean.length > MAX_TEXT_BYTES - total) { counts.omittedFiles++; continue; }
            total += clean.length;
            String name = file.getFileName().toString();
            parts.add(new Part(name.startsWith("events-") ? "events"
                : name.startsWith("crash-") ? "crash" : "engine", clean));
        }
        noLinks(logs);
        parts.sort(Comparator.comparing((Part p) -> p.kind).thenComparing(p -> p.sha));
        StringBuilder description = new StringBuilder();
        description.append("{\"format\":1,\"omitted_files\":").append(counts.omittedFiles)
            .append(",\"truncated_files\":").append(counts.truncatedFiles)
            .append(",\"omitted_lines\":").append(counts.omittedLines)
            .append(",\"max_tail_bytes\":").append(MAX_TAIL_BYTES)
            .append(",\"entries\":[");
        for (int i = 0; i < parts.size(); i++) {
            if (i > 0) description.append(',');
            Part p = parts.get(i);
            description.append("{\"id\":").append(i).append(",\"path\":\"logs/entry-")
                .append(i).append(".txt\",\"kind\":\"").append(p.kind)
                .append("\",\"bytes\":").append(p.data.length).append(",\"sha256\":\"")
                .append(p.sha).append("\"}");
        }
        description.append("]}\n");
        byte[] manifest = description.toString().getBytes(StandardCharsets.UTF_8);
        // Content/multiplicity and truncation metadata, independent of source/output names.
        String id = hash(manifest);
        boolean created = false;
        try {
            noLinks(parent);
            try (SeekableByteChannel channel = Files.newByteChannel(out,
                    options(StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE,
                        LinkOption.NOFOLLOW_LINKS))) {
                created = true;
                try (ZipOutputStream zip = new ZipOutputStream(
                        new CappedOutput(Channels.newOutputStream(channel), MAX_ARCHIVE_BYTES))) {
                    write(zip, "manifest.json", manifest);
                    for (int i = 0; i < parts.size(); i++) write(zip, "logs/entry-" + i + ".txt", parts.get(i).data);
                }
            }
            long length = Files.size(out);
            return new Snapshot(id, fileHash(out), hash(manifest), length, parts.size() + 1);
        } catch (IOException | RuntimeException e) {
            if (created) Files.deleteIfExists(out);
            // Raw filesystem exceptions can contain personal paths.
            throw new IOException("Diagnostics collection failed");
        }
    }

    private static void noLinks(Path path) throws IOException {
        Path at = path.getRoot();
        for (Path component : path) {
            at = at == null ? component : at.resolve(component);
            if (Files.isSymbolicLink(at)) throw new IOException("Symbolic diagnostics path refused");
        }
    }
    private static void noTraversal(Path path) throws IOException {
        for (Path component : path)
            if (component.toString().equals("..") || component.toString().equals("."))
                throw new IOException("Relative diagnostics traversal refused");
    }
    private static String stamp(String name) {
        int first = name.indexOf('-');
        return name.substring(first + 1, name.lastIndexOf('.'));
    }
    private static byte[] tail(Path file, Path logs, Counts counts) throws IOException {
        if (!file.getParent().equals(logs)) throw new IOException("Nested diagnostics file refused");
        noLinks(file);
        try (SeekableByteChannel input = Files.newByteChannel(file,
                options(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS))) {
            long length = input.size();
            int size = (int) Math.min(length, MAX_TAIL_BYTES);
            ByteBuffer buffer = ByteBuffer.allocate(size);
            input.position(length - size);
            while (buffer.hasRemaining() && input.read(buffer) > 0) { /* bounded tail */ }
            noLinks(file);
            byte[] data = new byte[buffer.position()];
            buffer.flip();
            buffer.get(data);
            if (length > size) {
                counts.truncatedFiles++;
                // A clipped first line might start inside a secret or UTF-8 sequence.
                int first = 0;
                while (first < data.length && data[first] != '\n') first++;
                return java.util.Arrays.copyOfRange(data, Math.min(first + 1, data.length), data.length);
            }
            return data;
        }
    }
    private static byte[] metadata(String input, boolean source, String secret, Counts counts) {
        StringBuilder text = new StringBuilder();
        if (input == null) input = "";
        if (input.length() > MAX_METADATA) { input = input.substring(0, MAX_METADATA); counts.truncatedFiles++; }
        for (String line : input.split("\\r?\\n")) {
            if (line.isEmpty()) continue;
            if (secret != null && !secret.isEmpty() && line.contains(secret)) { counts.omittedLines++; continue; }
            boolean accepted = source
                ? line.matches("(?:source_revision|native_source_revision)=[0-9a-f]{40}")
                    || line.matches("package_version=[0-9][A-Za-z0-9._-]{0,63}")
                    || line.matches("android_api=[0-9]{1,3}")
                : line.matches("exit_(?:reason|status|pid|timestamp_ms|importance|pss_kb|rss_kb)=(?:[A-Z][A-Z0-9_]{0,47}|[0-9]{1,20})")
                    || line.matches("low_memory_kill_supported=(?:true|false)")
                    || JOURNAL.matcher(line).matches();
            if (accepted) text.append(line).append('\n'); else counts.omittedLines++;
        }
        return text.toString().getBytes(StandardCharsets.UTF_8);
    }
    private static byte[] scrub(byte[] input, String secret, Counts counts) {
        if (input.length > 0 && input[input.length - 1] != '\n') {
            // The engine may be appending while we read. A partial final line could
            // expose only a fragment of a secret, or end inside a UTF-8 character.
            int end = input.length - 1;
            while (end >= 0 && input[end] != '\n') end--;
            input = java.util.Arrays.copyOf(input, end + 1);
            counts.omittedLines++;
        }
        String text;
        try {
            text = StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(input)).toString();
        } catch (CharacterCodingException e) {
            counts.omittedLines++;
            return "[invalid UTF-8 omitted]\n".getBytes(StandardCharsets.UTF_8);
        }
        StringBuilder clean = new StringBuilder();
        for (String line : text.split("\\r?\\n", -1)) {
            if (line.isEmpty()) continue;
            var frame = FRAME.matcher(line);
            if (line.length() <= MAX_LINE && frame.matches()
                && (secret == null || secret.isEmpty() || !line.contains(secret))) {
                clean.append(frame.group(1)).append(": ").append(frame.group(2)).append('\n');
                continue;
            }
            if (line.length() > MAX_LINE || PRIVATE.matcher(line).find()
                || (secret != null && !secret.isEmpty() && line.contains(secret))
                || !TECHNICAL.matcher(line).find()) {
                counts.omittedLines++;
                continue;
            }
            line = QUOTED.matcher(line).replaceAll("[text]");
            line = PATH.matcher(line).replaceAll("[path]");
            line = EMAIL.matcher(line).replaceAll("[email]");
            line = IDENTIFIER.matcher(line).replaceAll("[identifier]");
            line = line.replaceAll("[\\p{Cc}&&[^\\t]]", "");
            clean.append(line).append('\n');
        }
        return clean.toString().getBytes(StandardCharsets.UTF_8);
    }
    private static void write(ZipOutputStream zip, String name, byte[] data) throws IOException {
        ZipEntry entry = new ZipEntry(name);
        entry.setTime(0);
        zip.putNextEntry(entry);
        zip.write(data);
        zip.closeEntry();
    }
    private static String hash(byte[] data) {
        MessageDigest digest = digest();
        digest.update(data);
        return hex(digest.digest());
    }
    private static String fileHash(Path file) throws IOException {
        MessageDigest digest = digest();
        try (SeekableByteChannel input = Files.newByteChannel(file,
                options(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS))) {
            ByteBuffer buffer = ByteBuffer.allocate(8192);
            while (input.read(buffer) > 0) {
                digest.update(buffer.array(), 0, buffer.position());
                buffer.clear();
            }
        }
        return hex(digest.digest());
    }
    private static Set<OpenOption> options(OpenOption... values) {
        Set<OpenOption> out = new HashSet<>();
        java.util.Collections.addAll(out, values);
        return out;
    }
    private static MessageDigest digest() {
        try { return MessageDigest.getInstance("SHA-256"); }
        catch (NoSuchAlgorithmException e) { throw new IllegalStateException("SHA-256 unavailable"); }
    }
    private static String hex(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) out.append(String.format(java.util.Locale.ROOT, "%02x", b & 255));
        return out.toString();
    }
    private static final class CappedOutput extends OutputStream {
        private final OutputStream target;
        private final long max;
        private long count;
        CappedOutput(OutputStream target, long max) { this.target = target; this.max = max; }
        @Override public void write(int value) throws IOException {
            if (count >= max) throw new IOException("Diagnostics archive limit");
            target.write(value); count++;
        }
        @Override public void write(byte[] bytes, int offset, int size) throws IOException {
            if (size > max - count) throw new IOException("Diagnostics archive limit");
            target.write(bytes, offset, size); count += size;
        }
        @Override public void close() throws IOException { target.close(); }
    }
}
