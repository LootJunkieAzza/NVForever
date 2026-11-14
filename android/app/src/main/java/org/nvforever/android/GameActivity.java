package org.nvforever.android;
import android.app.NativeActivity;
import android.content.Intent;
import android.os.Bundle;
import android.view.WindowInsets;
import android.view.ViewTreeObserver;
/** Only the game lives in :game; Android can restart it with a fresh Rust world. */
public final class GameActivity extends NativeActivity {
    private ControllerDevices controllers;
    private volatile float touchBottomDp = 24.0f;
    private final ViewTreeObserver.OnGlobalLayoutListener touchInsetsObserver = this::refreshTouchInsets;
    @Override public void onCreate(Bundle state) {
        LanDiagnostics.record(this, "GAME_CREATE_STARTED", -1);
        // Metadata must exist before NativeActivity starts its native thread.
        controllers = new ControllerDevices((android.hardware.input.InputManager) getSystemService(INPUT_SERVICE));
        super.onCreate(state);
        LanDiagnostics.record(this, "GAME_CREATED", -1);
        getWindow().getDecorView().getViewTreeObserver().addOnGlobalLayoutListener(touchInsetsObserver);
        refreshTouchInsets();
    }
    @Override protected void onPause() {
        if (controllers != null) controllers.cancelHeldInput();
        super.onPause();
    }
    @Override protected void onResume() {
        if (controllers != null) controllers.cancelHeldInput();
        super.onResume();
        LanDiagnostics.record(this, "GAME_RESUMED", -1);
        refreshTouchInsets();
    }
    @Override public void onWindowFocusChanged(boolean focused) {
        if (controllers != null) controllers.cancelHeldInput();
        super.onWindowFocusChanged(focused);
    }
    @Override protected void onDestroy() {
        LanDiagnostics.record(this, "GAME_DESTROYED", -1);
        getWindow().getDecorView().getViewTreeObserver().removeOnGlobalLayoutListener(touchInsetsObserver);
        if (controllers != null) controllers.close();
        super.onDestroy();
    }
    private void refreshTouchInsets() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        if (insets != null) updateTouchInsets(insets);
    }
    private void updateTouchInsets(WindowInsets insets) {
        int bottom = Math.max(insets.getSystemWindowInsetBottom(),
            Math.max(insets.getSystemGestureInsets().bottom, insets.getMandatorySystemGestureInsets().bottom));
        if (insets.getDisplayCutout() != null) bottom = Math.max(bottom, insets.getDisplayCutout().getSafeInsetBottom());
        float density = getResources().getDisplayMetrics().density;
        touchBottomDp = bottom / Math.max(0.1f, density);
    }
    public String controllerDevices() { return controllers == null ? "" : controllers.snapshot(); }
    /** Cached on the UI thread. Does not exclude or consume Android system gestures. */
    public String touchSafeBottom() { return Float.toString(touchBottomDp); }
    public String startupArgs() {
        String args = getIntent().getStringExtra("args");
        return args == null ? "[]" : args;
    }
    public String restartEngine(String args) {
        LanDiagnostics.record(this, "RESTART_REQUESTED", android.os.Process.myPid());
        runOnUiThread(() -> {
            startActivity(new Intent(this, RestartActivity.class)
                .putExtra("args", args).putExtra("pid", android.os.Process.myPid()));
            LanDiagnostics.record(this, "RESTART_ACTIVITY_QUEUED", android.os.Process.myPid());
        });
        return "";
    }
    public String engineStopped(String reason) {
        LanDiagnostics.record(this, reason.isEmpty() ? "ENGINE_EXITED" : "ENGINE_STOPPED_WITH_ERROR", -1);
        runOnUiThread(() -> {
            if (!reason.isEmpty()) startActivity(new Intent(this, LauncherActivity.class)
                .putExtra("error", reason).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP));
            finish();
            android.os.Process.killProcess(android.os.Process.myPid());
        });
        return "";
    }
}
