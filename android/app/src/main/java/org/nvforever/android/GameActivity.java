package org.nvforever.android;
import android.app.NativeActivity;
import android.app.AlertDialog;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.ResultReceiver;
import android.os.SystemClock;
import android.view.SurfaceHolder;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.ViewTreeObserver;
import java.util.concurrent.atomic.AtomicBoolean;
/** Only the game lives in :game; native state is fresh for every save load. */
public final class GameActivity extends NativeActivity {
    private ControllerDevices controllers;
    private AndroidGyro gyro;
    private AndroidPerformance performance;
    private volatile float touchBottomDp = 24.0f;
    private final ViewTreeObserver.OnGlobalLayoutListener touchInsetsObserver = this::refreshTouchInsets;
    private final Handler loadingHandler = new Handler(Looper.getMainLooper());
    private final AtomicBoolean restartQueued = new AtomicBoolean();
    private LoadingState loading;
    private View loadingView;
    private ResultReceiver receiver;
    private AlertDialog loadingDialog;
    private boolean destroyed, resumed;
    private String loadRefusal;
    private long committedEpoch = -1;
    private final Runnable watchdog = new Runnable() { public void run() {
        if (destroyed || loading == null) return;
        loading.tick(SystemClock.uptimeMillis());
        if (loadRefusal != null) showLoadRefusal();
        else if (loading.failed()) showLoadingBack(true);
        if (loading.blocking()) loadingHandler.postDelayed(this, 250);
    }};
    @Override public void onCreate(Bundle state) {
        LanDiagnostics.record(this, "GAME_CREATE_STARTED", -1);
        String id = getIntent().getStringExtra(RestartActivity.REQUEST);
        if (id == null || !id.matches("[0-9a-f]{32}")) id = java.util.UUID.randomUUID().toString().replace("-", "");
        loading = new LoadingState(id, SystemClock.uptimeMillis());
        receiver = getIntent().getParcelableExtra(RestartActivity.RECEIVER);
        // Metadata and request state exist before NativeActivity starts its native thread.
        controllers = new ControllerDevices((android.hardware.input.InputManager)getSystemService(INPUT_SERVICE));
        gyro = new AndroidGyro(this);
        performance = new AndroidPerformance(this);
        super.onCreate(state);
        LoadingUi.immersive(this);
        loadingView = LoadingUi.create(this);
        addContentView(loadingView, new ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        LanDiagnostics.record(this, "GAME_CREATED", -1);
        getWindow().getDecorView().getViewTreeObserver().addOnGlobalLayoutListener(touchInsetsObserver);
        refreshTouchInsets(); refreshCover();
    }
    @Override protected void onPause() {
        if (gyro != null) gyro.resumed(false);
        if (performance != null) performance.stop();
        resumed = false;
        if (loading != null) loading.resumed(false, SystemClock.uptimeMillis());
        if (controllers != null) controllers.cancelHeldInput();
        super.onPause();
    }
    @Override protected void onResume() {
        resumed = true;
        if (loading != null) loading.resumed(true, SystemClock.uptimeMillis());
        if (controllers != null) controllers.cancelHeldInput();
        super.onResume();
        if (gyro != null) gyro.resumed(true);
        if (performance != null) performance.start();
        LoadingUi.immersive(this); refreshCover();
        LanDiagnostics.record(this, "GAME_RESUMED", -1); refreshTouchInsets();
    }
    @Override public void onWindowFocusChanged(boolean focused) {
        if (gyro != null) gyro.focused(focused);
        if (loading != null) loading.focused(focused, SystemClock.uptimeMillis());
        if (controllers != null) controllers.cancelHeldInput();
        super.onWindowFocusChanged(focused);
        if (focused) LoadingUi.immersive(this);
        refreshCover();
    }
    @Override public void surfaceCreated(SurfaceHolder holder) {
        if (loading != null) loading.surface(true, SystemClock.uptimeMillis());
        super.surfaceCreated(holder); refreshCover();
    }
    @Override public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        if (loading != null) loading.surface(width > 0 && height > 0, SystemClock.uptimeMillis());
        super.surfaceChanged(holder, format, width, height); refreshCover();
    }
    @Override public void surfaceDestroyed(SurfaceHolder holder) {
        if (loading != null) loading.surface(false, SystemClock.uptimeMillis());
        super.surfaceDestroyed(holder); refreshCover();
    }
    @Override protected void onDestroy() {
        if (gyro != null) gyro.close();
        if (performance != null) performance.close();
        destroyed = true; loadingHandler.removeCallbacks(watchdog);
        if (loadingDialog != null) loadingDialog.dismiss();
        LanDiagnostics.record(this, "GAME_DESTROYED", -1);
        getWindow().getDecorView().getViewTreeObserver().removeOnGlobalLayoutListener(touchInsetsObserver);
        if (controllers != null) controllers.close();
        super.onDestroy();
    }
    private void refreshCover() {
        if (destroyed || loadingView == null || loading == null) return;
        loadingHandler.removeCallbacks(watchdog);
        if (!loading.blocking() && !restartQueued.get()) return;
        loadingView.setVisibility(View.VISIBLE);
        loadingHandler.post(watchdog);
        if (!loading.eligible() || committedEpoch == loading.epoch()) return;
        final long epoch = loading.epoch(); committedEpoch = epoch;
        LoadingUi.committed(loadingView, () -> runOnUiThread(() -> {
            if (destroyed || loading == null) return;
            loading.overlayCommitted(epoch);
            if (loading.eligible() && epoch == loading.epoch()) { sendLoading(RestartActivity.OVERLAY); revealIfReady(); }
        }));
    }
    private void sendLoading(int code) {
        if (receiver == null || loading == null) return;
        Bundle result = new Bundle(); result.putString(RestartActivity.REQUEST, loading.request);
        result.putInt("pid", android.os.Process.myPid()); result.putLong("epoch", loading.epoch());
        receiver.send(code, result);
    }
    /** Called from JNI. A lifecycle event never marks a native frame ready. */
    public String loadingState() {
        if (loading == null) return "";
        if (restartQueued.get()) return loading.request + ":" + loading.epoch() + ":0";
        return loading.state();
    }
    public String beginNewGameCover() {
        final String before = loading == null ? "" : loading.ticket();
        runOnUiThread(() -> {
            if (destroyed || restartQueued.get() || loading == null || loading.failed()) return;
            loading.beginEpisode(SystemClock.uptimeMillis()); refreshCover();
        });
        return before;
    }
    public String contentCoverReady() { return loading != null && loading.coverReady() ? loading.ticket() : ""; }
    public String nativeContentSubmitted(String ticket) {
        runOnUiThread(() -> { if (!destroyed && !restartQueued.get() && loading.nativeSubmitted(ticket)) revealIfReady(); });
        return "";
    }
    private void revealIfReady() {
        final String ticket = loading.ticket();
        loadingView.postOnAnimation(() -> {
            if (destroyed || restartQueued.get() || !loading.reveal(ticket)) return;
            loadingView.setVisibility(View.GONE); loadingHandler.removeCallbacks(watchdog);
            sendLoading(RestartActivity.SUBMITTED);
            LanDiagnostics.record(this, "NATIVE_CONTENT_SUBMITTED", -1);
        });
    }
    public String loadRefused(String reason) {
        runOnUiThread(() -> {
            if (destroyed) return;
            loadRefusal = reason == null ? "The save could not be loaded." : reason.substring(0, Math.min(reason.length(), 4096));
            LanDiagnostics.record(this, "LOAD_REFUSED", -1);
            showLoadRefusal();
        });
        return "";
    }
    private void showLoadRefusal() {
        if (destroyed || !resumed || !hasWindowFocus() || loadRefusal == null || (loadingDialog != null && loadingDialog.isShowing())) return;
        loadingDialog = new AlertDialog.Builder(this).setTitle("The save could not be loaded")
            .setMessage(loadRefusal + "\n\nYour saves are kept. You can choose another save from the main menu.")
            .setCancelable(false).setNegativeButton("Return to launcher", (d, which) -> returnToLauncher())
            .setPositiveButton("Main menu", (d, which) -> { loadRefusal = null; }).create();
        loadingDialog.show();
    }
    public String loadingBack() { runOnUiThread(() -> showLoadingBack(false)); return ""; }
    @Override public void onBackPressed() { if (loading != null && (loading.blocking() || restartQueued.get())) showLoadingBack(false); else super.onBackPressed(); }
    private void showLoadingBack(boolean failed) {
        if (destroyed || !resumed || !hasWindowFocus() || loading == null || (!loading.blocking() && !restartQueued.get()) || (loadingDialog != null && loadingDialog.isShowing())) return;
        // A dialog has its own Java input window; NativeActivity owns the game's input queue.
        AlertDialog.Builder dialog = new AlertDialog.Builder(this).setTitle(failed ? "Loading stopped" : "Return to launcher?")
            .setMessage(failed ? "The game could not finish preparing its view. Saves are kept." : "This stops loading. Saves are kept.")
            .setPositiveButton("Return to launcher", (d, which) -> returnToLauncher());
        if (failed) dialog.setCancelable(false); else dialog.setNegativeButton("Keep loading", null);
        loadingDialog = dialog.create(); loadingDialog.show();
    }
    private void returnToLauncher() {
        sendLoading(RestartActivity.FAILURE);
        startActivity(new Intent(this, LauncherActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP));
        finish(); android.os.Process.killProcess(android.os.Process.myPid());
    }
    private void refreshTouchInsets() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        if (insets != null) updateTouchInsets(insets);
    }
    private void updateTouchInsets(WindowInsets insets) {
        int bottom = Math.max(insets.getSystemWindowInsetBottom(), Math.max(insets.getSystemGestureInsets().bottom, insets.getMandatorySystemGestureInsets().bottom));
        if (insets.getDisplayCutout() != null) bottom = Math.max(bottom, insets.getDisplayCutout().getSafeInsetBottom());
        touchBottomDp = bottom / Math.max(0.1f, getResources().getDisplayMetrics().density);
    }
    public String controllerDevices() { return controllers == null ? "" : controllers.snapshot(); }
    public String performanceState() { return performance == null ? "" : performance.snapshot(); }
    public String gyroSample(String command) { return gyro == null ? "" : gyro.poll(command); }
    public String touchSafeBottom() { return Float.toString(touchBottomDp); }
    public String startupArgs() { String args = getIntent().getStringExtra("args"); return args == null ? "[]" : args; }
    public String restartEngine(String args) {
        try {
            if (args == null || args.length() > 262144) return "Loading arguments exceed bounds";
            org.json.JSONArray parsed = new org.json.JSONArray(args);
            if (parsed.length() > 128) return "Too many loading arguments";
            for (int i = 0; i < parsed.length(); i++) if (!(parsed.get(i) instanceof String)) return "Invalid loading argument";
        } catch (org.json.JSONException invalid) { return "Invalid loading arguments"; }
        if (!restartQueued.compareAndSet(false, true)) return "";
        LanDiagnostics.record(this, "RESTART_REQUESTED", android.os.Process.myPid());
        runOnUiThread(() -> {
            loadingView.setVisibility(View.VISIBLE); LoadingUi.immersive(this);
            String request = java.util.UUID.randomUUID().toString().replace("-", "");
            try {
                startActivity(new Intent(this, RestartActivity.class)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP)
                    .putExtra("args", args).putExtra("pid", android.os.Process.myPid()).putExtra(RestartActivity.REQUEST, request),
                    android.app.ActivityOptions.makeCustomAnimation(this, 0, 0).toBundle());
                LanDiagnostics.record(this, "RESTART_ACTIVITY_QUEUED", android.os.Process.myPid());
            } catch (Exception failure) { loading.fail(); showLoadingBack(true); }
        });
        return "";
    }
    public String engineStopped(String reason) {
        LanDiagnostics.record(this, reason.isEmpty() ? "ENGINE_EXITED" : "ENGINE_STOPPED_WITH_ERROR", -1);
        sendLoading(RestartActivity.FAILURE);
        runOnUiThread(() -> {
            if (!reason.isEmpty()) startActivity(new Intent(this, LauncherActivity.class)
                .putExtra("error", reason).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP));
            finish(); android.os.Process.killProcess(android.os.Process.myPid());
        });
        return "";
    }
}
