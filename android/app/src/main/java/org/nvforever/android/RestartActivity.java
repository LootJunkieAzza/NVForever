package org.nvforever.android;
import android.app.Activity;
import android.app.ActivityManager;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.widget.TextView;

/** Private main-process activity: free the old game before starting its successor. */
public final class RestartActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        TextView text = new TextView(this);
        text.setText("Loading NVForever…"); text.setTextSize(24); setContentView(text);
        int pid = getIntent().getIntExtra("pid", -1);
        LanDiagnostics.record(this, "RESTART_ACTIVITY_CREATED", pid);
        ActivityManager manager = (ActivityManager) getSystemService(ACTIVITY_SERVICE);
        boolean owned = false;
        var processes = manager.getRunningAppProcesses();
        if (processes != null) for (var p : processes) {
            if (p.pid == pid && p.uid == android.os.Process.myUid()
                    && p.processName.equals(getPackageName() + ":game")) owned = true;
        }
        if (!owned) { LanDiagnostics.record(this, "RESTART_OWNER_REJECTED", pid); text.setText("The previous game could not be identified. Return to the launcher."); return; }
        LanDiagnostics.record(this, "OLD_GAME_STOP_REQUESTED", pid);
        android.os.Process.killProcess(pid);
        new Handler(Looper.getMainLooper()).postDelayed(new Runnable() {
            int attempts;
            @Override public void run() {
                var active = manager.getRunningAppProcesses();
                if (active != null) for (var p : active) if (p.pid == pid) {
                    if (++attempts < 100) { new Handler(Looper.getMainLooper()).postDelayed(this, 100); return; }
                    LanDiagnostics.record(RestartActivity.this, "OLD_GAME_STOP_TIMEOUT", pid);
                    text.setText("The previous game did not stop. Return to the launcher."); return;
                }
                // A killed singleTask activity can retain its old task/Intent. Clear
                // only the game task so the successor receives a fresh load request.
                LanDiagnostics.record(RestartActivity.this, "SUCCESSOR_LAUNCH_REQUESTED", pid);
                startActivity(new Intent(RestartActivity.this, GameActivity.class)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK)
                    .putExtra("args", getIntent().getStringExtra("args")));
                LanDiagnostics.record(RestartActivity.this, "SUCCESSOR_LAUNCH_QUEUED", pid);
                finish();
            }
        }, 100);
    }
}
