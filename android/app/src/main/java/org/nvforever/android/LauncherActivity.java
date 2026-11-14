package org.nvforever.android;

import android.app.Activity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.DocumentsContract;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Set;

/** Imports a copy through the directory picker; never writes to the source tree. */
public final class LauncherActivity extends Activity {
    private TextView status;
    private Button start, select;
    private Button diagnostics;
    private boolean busy;
    private static final int PICK_GAME = 1;
    private static final int PICK_LOG_PAIRING = 2;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL); layout.setPadding(32, 32, 32, 32);
        TextView title = new TextView(this);
        title.setText("NVForever for Android · experimental"); title.setTextSize(25); layout.addView(title);
        TextView intro = new TextView(this);
        intro.setText("Import the Fallout: New Vegas folder (or Data folder) from your own copy. This makes a full local copy and needs enough free space. The source is never changed. Saves and settings are stored separately. Uninstalling removes app files.\n\nUse touch controls or a keyboard and mouse. Device performance is still being tested.");
        layout.addView(intro);
        select = new Button(this); select.setText("Import game folder");
        select.setOnClickListener(v -> startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), PICK_GAME)); layout.addView(select);
        start = new Button(this); start.setText("Start NVForever");
        start.setOnClickListener(v -> prepareAndStart()); layout.addView(start);
        status = new TextView(this); layout.addView(status); setContentView(layout);
        if (LanDiagnostics.available(this)) {
            LanDiagnostics.launcher(this);
            diagnostics = new Button(this); diagnostics.setText("LAN test logs");
            diagnostics.setOnClickListener(v -> showDiagnostics()); layout.addView(diagnostics);
        }
    }
    @Override public void onResume() {
        super.onResume();
        String error = getIntent().getStringExtra("error");
        if (error != null && !error.isEmpty()) { status.setText(error); getIntent().removeExtra("error"); }
        if (!busy) {
            start.setEnabled(new File(getFilesDir(), "home/NVForever.ini").isFile());
            select.setEnabled(!runningGame());
        }
    }
    @Override protected void onNewIntent(Intent intent) { super.onNewIntent(intent); setIntent(intent); }
    private void showDiagnostics() { showDiagnostics(null, null); }
    private void showDiagnostics(String importedEndpoint, String importedToken) {
        LanDiagnostics support = LanDiagnostics.launcher(this);
        LinearLayout form = new LinearLayout(this); form.setOrientation(LinearLayout.VERTICAL);
        int padding = (int)(16 * getResources().getDisplayMetrics().density);
        form.setPadding(padding, padding, padding, padding);
        TextView explanation = new TextView(this);
        explanation.setText("Optional testing support. Sends filtered NVForever technical logs and app exit/restart details to your paired PC over private Wi-Fi. Saves and game files are excluded. Your PC must be reachable. Local logs stay on this phone. Turn off to stop uploads.");
        form.addView(explanation);
        android.widget.CheckBox enabled = new android.widget.CheckBox(this);
        enabled.setText("Send test logs to my PC"); enabled.setChecked(support.enabled()); form.addView(enabled);
        android.widget.EditText endpoint = new android.widget.EditText(this);
        endpoint.setSingleLine(true); endpoint.setHint("Private PC upload URL");
        endpoint.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(256)});
        endpoint.setText(importedEndpoint == null ? support.endpoint() : importedEndpoint); form.addView(endpoint);
        android.widget.EditText token = new android.widget.EditText(this);
        token.setSingleLine(true); token.setHint("PC pairing token");
        token.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD);
        token.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(128)});
        token.setText(importedToken == null ? support.token() : importedToken); form.addView(token);
        TextView state = new TextView(this); state.setText(support.status()); form.addView(state);
        android.widget.ScrollView scroll = new android.widget.ScrollView(this); scroll.addView(form);
        android.app.AlertDialog dialog = new android.app.AlertDialog.Builder(this).setTitle("LAN test logs")
            .setNeutralButton("Import pairing file", (d, which) -> startActivityForResult(
                new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("application/json").addCategory(Intent.CATEGORY_OPENABLE), PICK_LOG_PAIRING))
            .setView(scroll).setNegativeButton("Cancel", null).setPositiveButton("Save", null).create();
        android.os.Handler handler = new android.os.Handler(android.os.Looper.getMainLooper());
        Runnable refresh = new Runnable() { @Override public void run() {
            state.setText(support.status()); handler.postDelayed(this, 2000);
        }};
        dialog.setOnDismissListener(d -> handler.removeCallbacks(refresh));
        dialog.setOnShowListener(d -> {
            handler.post(refresh);
            dialog.getButton(android.app.AlertDialog.BUTTON_POSITIVE).setOnClickListener(v -> {
                try {
                    support.configure(enabled.isChecked(), endpoint.getText().toString().trim(), token.getText().toString().trim());
                    diagnostics.setText("LAN test logs · " + support.status()); dialog.dismiss();
                } catch (Exception error) { state.setText("Check the private PC URL and pairing token; settings could not be applied."); }
            });
        });
        dialog.show();
    }
    private boolean runningGame() {
        android.app.ActivityManager manager = (android.app.ActivityManager) getSystemService(ACTIVITY_SERVICE);
        var processes = manager.getRunningAppProcesses();
        if (processes != null) for (var p : processes)
            if (p.uid == android.os.Process.myUid() && p.processName.equals(getPackageName() + ":game")) return true;
        return false;
    }
    private void working(boolean value, String message) {
        busy = value; select.setEnabled(!value && !runningGame());
        start.setEnabled(!value && new File(getFilesDir(), "home/NVForever.ini").isFile()); status.setText(message);
    }
    @Override public void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == PICK_LOG_PAIRING) {
            if (result == RESULT_OK && data != null && data.getData() != null) importLogPairing(data.getData());
            return;
        }
        if (request != PICK_GAME || result != RESULT_OK || data == null || data.getData() == null) return;
        Uri tree = data.getData();
        if (runningGame()) { status.setText("Close the game before importing another install."); return; }
        working(true, "Copying game files. Keep NVForever open until this finishes…");
        new Thread(() -> {
            File imported = null;
            boolean selected = false;
            try {
                File external = getExternalFilesDir(null);
                if (external == null) throw new IOException("App storage is unavailable");
                imported = new File(external, "game-" + java.util.UUID.randomUUID());
                if (!imported.mkdirs()) throw new IOException("Cannot create import directory");
                if (!new File(imported, ".nvf-import").createNewFile()) throw new IOException("Cannot mark import directory");
                String root = DocumentsContract.getTreeDocumentId(tree);
                String child = findChild(tree, root, "Data");
                String dataRoot = child == null ? root : child;
                if (findChild(tree, dataRoot, "FalloutNV.esm") == null) throw new IOException("Select the game folder or its Data folder (FalloutNV.esm is missing)");
                for (String required : InstallConfig.REQUIRED) {
                    if (findChild(tree, dataRoot, required) == null) throw new IOException("The selected game is incomplete: missing " + required);
                }
                copyDirectory(tree, dataRoot, new File(imported, "Data"), new HashSet<>(), 0);
                for (String required : InstallConfig.REQUIRED) {
                    File[] files = new File(imported, "Data").listFiles();
                    boolean present = false;
                    if (files != null) for (File file : files) if (file.isFile() && file.length() > 0 && file.getName().equalsIgnoreCase(required)) present = true;
                    if (!present) throw new IOException("The imported file is missing or empty: " + required);
                }
                if (child != null) {
                    String ini = findChild(tree, root, "Fallout_default.ini");
                    if (ini != null) copyFile(tree, ini, new File(imported, "Fallout_default.ini"));
                }
                File home = new File(getFilesDir(), "home");
                if (!home.isDirectory() && !home.mkdirs()) throw new IOException("Cannot create settings directory");
                android.util.AtomicFile config = new android.util.AtomicFile(new File(home, "NVForever.ini"));
                FileOutputStream out = null;
                try {
                    String old = "";
                    if (config.getBaseFile().isFile()) try (InputStream in = config.openRead()) {
                        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
                        byte[] buffer = new byte[8192]; int n;
                        while ((n = in.read(buffer)) != -1) bytes.write(buffer, 0, n);
                        old = bytes.toString("UTF-8");
                    }
                    String updated = InstallConfig.updateGamePath(old, imported.getAbsolutePath());
                    out = config.startWrite(); out.write(updated.getBytes(StandardCharsets.UTF_8)); config.finishWrite(out);
                } catch (Exception e) { if (out != null) config.failWrite(out); throw e; }
                selected = true;
                var preferences = getSharedPreferences("imports", MODE_PRIVATE);
                String previous = preferences.getString("active", "");
                if (preferences.edit().putString("active", imported.getAbsolutePath()).commit() && !previous.isEmpty()) removeImport(new File(previous));
                runOnUiThread(() -> working(false, "Game files imported. Ready to start."));
            } catch (Exception e) { runOnUiThread(() -> working(false, "Import failed: " + e.getMessage() + ". The previous import is still selected.")); }
            finally { if (!selected && imported != null) removeImport(imported); }
        }, "nvf-import").start();
    }
    private void importLogPairing(Uri uri) {
        new Thread(() -> {
            try (InputStream input = getContentResolver().openInputStream(uri)) {
                if (input == null) throw new IOException("No pairing data");
                java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
                byte[] buffer = new byte[1024]; int count;
                while ((count = input.read(buffer)) != -1) {
                    if (bytes.size() + count > 4096) throw new IOException("Pairing data too large");
                    bytes.write(buffer, 0, count);
                }
                org.json.JSONObject config = new org.json.JSONObject(bytes.toString("UTF-8"));
                if (config.getInt("protocol") != 1) throw new IOException("Unknown pairing format");
                String endpoint = config.getString("endpoint"), token = config.getString("token");
                DiagnosticsTransport.validateEndpoint(endpoint);
                if (endpoint.length() > 256 || !token.matches("[A-Za-z0-9_-]{32,128}")) throw new IOException("Invalid pairing settings");
                runOnUiThread(() -> showDiagnostics(endpoint, token));
            } catch (Exception error) {
                runOnUiThread(() -> status.setText("Could not read pairing settings. Choose the private JSON file from your PC."));
            }
        }, "nvf-log-pairing-import").start();
    }
    private void removeImport(File root) {
        try {
            File external = getExternalFilesDir(null);
            if (external == null || !InstallConfig.ownedImport(external, root)) return;
            String active = getSharedPreferences("imports", MODE_PRIVATE).getString("active", "");
            if (root.getCanonicalPath().equals(active)) return;
            deleteOwnedTree(root);
        } catch (IOException ignored) { /* An incomplete copy may be removed on app uninstall. */ }
    }
    private void deleteOwnedTree(File file) throws IOException {
        if (!java.nio.file.Files.isSymbolicLink(file.toPath()) && file.isDirectory()) {
            File[] children = file.listFiles();
            if (children == null) throw new IOException("Cannot list import directory");
            for (File child : children) deleteOwnedTree(child);
        }
        if (!file.delete()) throw new IOException("Cannot remove old import file");
    }
    private String findChild(Uri tree, String parent, String name) throws IOException {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parent);
        try (Cursor c = getContentResolver().query(children, new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
            if (c == null) throw new IOException("Cannot read the selected folder");
            while (c.moveToNext()) if (name.equalsIgnoreCase(c.getString(1))) return c.getString(0);
        }
        return null;
    }
    private void copyDirectory(Uri tree, String id, File target, Set<String> visited, int depth) throws IOException {
        if (depth > 128 || !visited.add(id)) throw new IOException("Recursive document tree refused");
        if (!target.mkdirs()) throw new IOException("Cannot create " + target.getName());
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id);
        try (Cursor c = getContentResolver().query(children, new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE}, null, null, null)) {
            if (c == null) throw new IOException("Cannot read a game directory");
            while (c.moveToNext()) {
                String name = c.getString(1);
                InstallConfig.checkName(name);
                File child = new File(target, name);
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(c.getString(2))) copyDirectory(tree, c.getString(0), child, visited, depth + 1);
                else copyFile(tree, c.getString(0), child);
            }
        }
    }
    private void copyFile(Uri tree, String id, File target) throws IOException {
        try (InputStream in = getContentResolver().openInputStream(DocumentsContract.buildDocumentUriUsingTree(tree, id)); FileOutputStream out = new FileOutputStream(target)) {
            if (in == null) throw new IOException("Cannot open " + target.getName());
            byte[] buffer = new byte[1024 * 1024]; int n;
            while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
        }
    }
    private void prepareAndStart() {
        working(true, "Preparing engine schemas…");
        new Thread(() -> {
            try {
                copyAsset("schemas", new File(getFilesDir(), "runtime/schemas"));
                copyAsset("lang", new File(getFilesDir(), "home/lang"));
                runOnUiThread(() -> { working(false, "Starting…"); startActivity(new Intent(this, GameActivity.class)); });
            } catch (Exception e) { runOnUiThread(() -> working(false, "Startup failed: " + e.getMessage())); }
        }, "nvf-setup").start();
    }
    private void copyAsset(String asset, File target) throws IOException {
        String[] children = getAssets().list(asset);
        if (children != null && children.length > 0) {
            if (!target.isDirectory() && !target.mkdirs()) throw new IOException("Cannot extract schemas");
            for (String name : children) copyAsset(asset + "/" + name, new File(target, name));
        } else try (InputStream in = getAssets().open(asset); FileOutputStream out = new FileOutputStream(target)) {
            byte[] buffer = new byte[65536]; int n;
            while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
        }
    }
}
