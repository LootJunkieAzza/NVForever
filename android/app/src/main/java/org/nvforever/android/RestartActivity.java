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
        ActivityManager manager = (ActivityManager) getSystemService(ACTIVITY_SERVICE);
        boolean owned = false;
        var processes = manager.getRunningAppProcesses();
        if (processes != null) for (var p : processes) {
            if (p.pid == pid && p.uid == android.os.Process.myUid()
                    && p.processName.equals(getPackageName() + ":game")) owned = true;
        }
        if (!owned) { text.setText("The previous game could not be identified. Return to the launcher."); return; }
        android.os.Process.killProcess(pid);
        new Handler(Looper.getMainLooper()).postDelayed(new Runnable() {
            int attempts;
            @Override public void run() {
                var active = manager.getRunningAppProcesses();
                if (active != null) for (var p : active) if (p.pid == pid) {
                    if (++attempts < 100) { new Handler(Looper.getMainLooper()).postDelayed(this, 100); return; }
                    text.setText("The previous game did not stop. Return to the launcher."); return;
                }
                startActivity(new Intent(RestartActivity.this, GameActivity.class)
                    .putExtra("args", getIntent().getStringExtra("args")));
                finish();
            }
        }, 100);
    }
}
