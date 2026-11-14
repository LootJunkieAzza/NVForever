package org.nvforever.android;

import android.app.ActivityManager;
import android.app.ApplicationExitInfo;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.ApplicationInfo;
import android.os.Build;
import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.nio.channels.FileLock;
import java.nio.channels.FileChannel;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Opt-in test support. Only the launcher process starts this bounded network worker. */
final class LanDiagnostics {
    static final String DEFAULT_ENDPOINT = "http://192.168.50.116:8765/api/v1/log-bundles";
    private static LanDiagnostics instance;
    private final android.app.Application context;
    private final SharedPreferences preferences;
    private final File directory;
    private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread thread = new Thread(r, "nvf-lan-diagnostics"); thread.setDaemon(true); return thread;
    });
    private final AtomicBoolean queued = new AtomicBoolean();
    private final DiagnosticsSession session;
    private volatile String status = "Off";
    private volatile long pausedGeneration = -1;

    static synchronized LanDiagnostics launcher(Context context) {
        if (instance == null) instance = new LanDiagnostics((android.app.Application)context.getApplicationContext());
        return instance;
    }
    static boolean available(Context context) {
        return (context.getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0;
    }
    private LanDiagnostics(android.app.Application context) {
        this.context = context;
        preferences = context.getSharedPreferences("lan-diagnostics", Context.MODE_PRIVATE);
        directory = new File(trustedFiles(context), "diagnostics");
        File marker = new File(directory, "enabled");
        boolean restored = available(context) && preferences.getBoolean("enabled", false) && consentMarker(directory, marker);
        session = new DiagnosticsSession(restored, preferences.getString("endpoint", DEFAULT_ENDPOINT), preferences.getString("token", ""));
        worker.scheduleWithFixedDelay(this::requestTick, 0, 30, TimeUnit.SECONDS);
    }
    boolean enabled() { return available(context) && session.current().enabled; }
    String endpoint() { return session.current().endpoint; }
    String token() { return session.current().token; }
    String status() { return enabled() ? status : "Off"; }

    synchronized void configure(boolean enabled, String endpoint, String token) throws Exception {
        if (!available(context)) throw new IllegalStateException("Only test builds support LAN diagnostics");
        pausedGeneration = -1;
        session.configure(enabled, endpoint, token, (on, url, secret) -> {
            if (Files.isSymbolicLink(directory.toPath()) || (!directory.isDirectory() && !directory.mkdirs()))
                throw new java.io.IOException("Diagnostics storage unavailable");
            File marker = new File(directory, "enabled");
            // Fail closed before validation and preference IO, including failed opt-out commits.
            Files.deleteIfExists(marker.toPath());
            if (url.length() > 256 || secret.length() > 128) throw new java.io.IOException("Pairing settings too long");
            if (on) {
                DiagnosticsTransport.validateEndpoint(url);
                if (!secret.matches("[A-Za-z0-9_-]{32,128}")) throw new java.io.IOException("Invalid pairing token");
            }
            if (!preferences.edit().putBoolean("enabled", on).putString("endpoint", url).putString("token", secret).commit())
                throw new java.io.IOException("Cannot save diagnostics settings");
            if (on) Files.write(marker.toPath(), new byte[]{1}, java.nio.file.StandardOpenOption.CREATE_NEW, java.nio.file.StandardOpenOption.WRITE);
        });
        status = enabled ? "Waiting for PC" : "Off";
        requestTick();
    }
    private void requestTick() {
        if (queued.compareAndSet(false, true)) worker.execute(() -> {
            try { tick(); } finally { queued.set(false); }
        });
    }
    private void tick() {
        DiagnosticsSession.Snapshot snapshot = session.current();
        if (!available(context) || !snapshot.enabled || pausedGeneration == snapshot.generation) return;
        long currentGeneration = snapshot.generation;
        DiagnosticsTransport.Cancellation currentCancellation = snapshot.cancellation;
        String endpoint = snapshot.endpoint, token = snapshot.token;
        File candidate = new File(directory, "candidate.zip");
        try {
            if (!directory.isDirectory() && !directory.mkdirs()) throw new java.io.IOException("Storage unavailable");
            DiagnosticsQueue queue = new DiagnosticsQueue(new File(directory, "queue"), DiagnosticsQueue.pairingKey(endpoint, token));
            boolean capacity = queue.canEnqueue();
            if (capacity) {
                DiagnosticsArchive.Snapshot archive = DiagnosticsArchive.collect(new File(trustedFiles(context), "home/logs"),
                    candidate, sourceIdentity(), runtimeSummary(), token);
                if (!queue.containsSnapshot(archive.snapshotId)) queue.enqueue(candidate, archive.snapshotId, archive.archiveSha256);
            }
            if (session.current() != snapshot || currentCancellation.isCancelled()) return;
            DiagnosticsQueue.Entry entry = queue.next(System.currentTimeMillis());
            if (entry == null) {
                status = !capacity || queue.totalCount() > queue.count() ? "Queue blocked by pending logs; originals kept"
                    : queue.count() > 0 ? "Waiting to retry" : "Up to date";
                return;
            }
            status = "Sending test logs";
            try {
                DiagnosticsTransport.upload(new URL(endpoint), token, entry.file, entry.sha256, currentCancellation);
                if (session.current() == snapshot && !currentCancellation.isCancelled()) {
                    queue.acknowledge(entry);
                    status = "Received by PC";
                }
            } catch (Exception error) {
                if (session.current() == snapshot && !currentCancellation.isCancelled()) {
                    if (error instanceof DiagnosticsTransport.UploadException && !((DiagnosticsTransport.UploadException)error).retryable) {
                        pausedGeneration = currentGeneration;
                        status = "Pairing rejected; check PC settings";
                    } else { queue.failed(entry, System.currentTimeMillis()); status = "PC unavailable; logs queued for retry"; }
                }
            }
        } catch (Exception error) {
            if (session.current() == snapshot) status = "Diagnostics unavailable; original logs kept";
        } finally {
            // This generated scratch archive never contains saves. Original logs stay untouched.
            try { if (!Files.isSymbolicLink(directory.toPath())) Files.deleteIfExists(candidate.toPath()); } catch (Exception ignored) { }
        }
    }
    private String sourceIdentity() throws Exception {
        String source = asset("SOURCE.txt"), nativeIdentity = "";
        // Generic builds without an attested native revision omit it rather than guess.
        // The Java-only support APK's packaging gate requires this separate asset.
        try { nativeIdentity = "\nnative_source_revision=" + revision(asset("NATIVE-SOURCE.txt")); }
        catch (java.io.IOException absent) { }
        return "source_revision=" + revision(source) + nativeIdentity
            + "\npackage_version=" + context.getPackageManager().getPackageInfo(context.getPackageName(), 0).versionName
            + "\nandroid_api=" + Build.VERSION.SDK_INT + "\n";
    }
    private String asset(String name) throws Exception {
        try (InputStream stream = context.getAssets().open(name)) {
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            byte[] buffer = new byte[1024]; int count;
            while ((count = stream.read(buffer)) != -1) {
                if (bytes.size() + count > 4096) throw new java.io.IOException("Build identity too large");
                bytes.write(buffer, 0, count);
            }
            return new String(bytes.toByteArray(), StandardCharsets.UTF_8);
        }
    }
    private static String revision(String text) throws java.io.IOException {
        Matcher matcher = Pattern.compile("/tree/([0-9a-f]{40})(?:\\s|$)").matcher(text);
        if (!matcher.find()) throw new java.io.IOException("Build source identity missing");
        return matcher.group(1);
    }
    private String runtimeSummary() throws Exception {
        StringBuilder text = new StringBuilder();
        if (Build.VERSION.SDK_INT >= 30) {
            ActivityManager manager = (ActivityManager) context.getSystemService(Context.ACTIVITY_SERVICE);
            for (ApplicationExitInfo exit : manager.getHistoricalProcessExitReasons(context.getPackageName(), 0, 6)) {
                if (exit.getRealUid() == context.getApplicationInfo().uid
                        && (context.getPackageName() + ":game").equals(exit.getProcessName())) {
                    text.append("exit_pid=").append(exit.getPid()).append("\nexit_timestamp_ms=").append(exit.getTimestamp())
                        .append("\nexit_reason=").append(exit.getReason()).append("\nexit_status=").append(exit.getStatus()).append('\n');
                }
            }
        }
        File journal = new File(directory, "restart.log");
        if (journal.isFile() && !Files.isSymbolicLink(journal.toPath()) && journal.length() <= 8192)
            text.append(new String(Files.readAllBytes(journal.toPath()), StandardCharsets.UTF_8));
        return text.toString();
    }
    /** Fixed phase names and app-owned PIDs only; no Intent arguments, save paths or tokens. */
    static void record(Context context, String phase, int relatedPid) {
        if (!available(context) || !phase.matches("[A-Z_]{1,40}")) return;
        File directory = new File(trustedFiles(context), "diagnostics");
        if (Files.isSymbolicLink(directory.toPath())) return;
        if (!new File(directory, "enabled").isFile()) return;
        File journal = new File(directory, "restart.log");
        java.util.Set<java.nio.file.OpenOption> options = new java.util.HashSet<>(java.util.Arrays.asList(
            java.nio.file.StandardOpenOption.CREATE, java.nio.file.StandardOpenOption.READ, java.nio.file.StandardOpenOption.WRITE,
            java.nio.file.LinkOption.NOFOLLOW_LINKS));
        try (FileChannel file = FileChannel.open(journal.toPath(), options); FileLock lock = file.tryLock()) {
            if (lock == null || Files.isSymbolicLink(directory.toPath())) return;
            if (file.size() > 4096) {
                int count = (int)Math.min(2048, file.size());
                ByteBuffer tail = ByteBuffer.allocate(count); file.position(file.size() - count);
                while (tail.hasRemaining() && file.read(tail) > 0) { }
                String text = new String(tail.array(), 0, tail.position(), StandardCharsets.US_ASCII);
                int newline = text.indexOf('\n'); text = newline < 0 ? "" : text.substring(newline + 1);
                file.truncate(0); file.position(0); write(file, text);
            }
            file.position(file.size());
            write(file, "event=" + phase + " pid=" + android.os.Process.myPid() + " time_ms=" + System.currentTimeMillis()
                + (relatedPid >= 0 ? " related_pid=" + relatedPid : "") + "\n");
        } catch (Exception ignored) { /* Evidence must never prevent gameplay or restart. */ }
    }
    private static void write(FileChannel file, String text) throws java.io.IOException {
        ByteBuffer bytes = ByteBuffer.wrap(text.getBytes(StandardCharsets.US_ASCII));
        while (bytes.hasRemaining()) file.write(bytes);
    }
    private static File trustedFiles(Context context) {
        try { return context.getFilesDir().getCanonicalFile(); }
        catch (java.io.IOException error) { return context.getFilesDir(); }
    }
    private static boolean consentMarker(File directory, File marker) {
        try {
            return !Files.isSymbolicLink(directory.toPath()) && !Files.isSymbolicLink(marker.toPath())
                && marker.isFile() && marker.length() == 1 && Files.readAllBytes(marker.toPath())[0] == 1;
        } catch (Exception error) { return false; }
    }
}
