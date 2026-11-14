package org.nvforever.android;
import android.app.NativeActivity;
import android.content.Intent;
/** Only the game lives in :game; Android can restart it with a fresh Rust world. */
public final class GameActivity extends NativeActivity {
    public String startupArgs() {
        String args = getIntent().getStringExtra("args");
        return args == null ? "[]" : args;
    }
    public String restartEngine(String args) {
        runOnUiThread(() -> startActivity(new Intent(this, RestartActivity.class)
            .putExtra("args", args).putExtra("pid", android.os.Process.myPid())));
        return "";
    }
    public String engineStopped(String reason) {
        runOnUiThread(() -> {
            if (!reason.isEmpty()) startActivity(new Intent(this, LauncherActivity.class)
                .putExtra("error", reason).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP));
            finish();
            android.os.Process.killProcess(android.os.Process.myPid());
        });
        return "";
    }
}
