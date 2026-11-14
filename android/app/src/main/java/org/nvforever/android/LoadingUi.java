package org.nvforever.android;

import android.app.Activity;
import android.os.Build;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;

/** Original, dependency-free loading appearance. Does not touch the native surface/inset listener. */
final class LoadingUi {
    static View create(Activity activity) {
        LinearLayout root = new LinearLayout(activity);
        root.setOrientation(LinearLayout.VERTICAL); root.setGravity(android.view.Gravity.CENTER);
        root.setBackgroundColor(0xff031017); root.setPadding(32, 32, 32, 32);
        TextView label = new TextView(activity); label.setText("Loading NVForever…");
        label.setTextColor(0xffdcf3eb); label.setTextSize(24); root.addView(label);
        ProgressBar progress = new ProgressBar(activity); root.addView(progress);
        TextView hint = new TextView(activity); hint.setText("Back returns to the launcher.");
        hint.setTextColor(0xffb4c9c4); hint.setTextSize(16); root.addView(hint);
        return root;
    }
    static void immersive(Activity activity) {
        View decor = activity.getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= 30) {
            activity.getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController controller = decor.getWindowInsetsController();
            if (controller != null) {
                controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                controller.hide(WindowInsets.Type.statusBars() | WindowInsets.Type.navigationBars());
            }
        } else decor.setSystemUiVisibility(View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
            | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
    }
    static void committed(View view, Runnable callback) {
        // API29 minimum: actual Java/HWUI frame submission, never a native-frame claim.
        view.post(() -> {
            if (!view.isAttachedToWindow() || !view.isHardwareAccelerated()) return;
            view.getViewTreeObserver().registerFrameCommitCallback(callback);
            view.invalidate();
        });
    }
    private LoadingUi() { }
}
