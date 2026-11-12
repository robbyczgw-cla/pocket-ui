package xyz.pocketui.app;

import android.app.Activity;
import android.app.ActivityManager;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.os.Build;
import android.os.PowerManager;
import android.os.StatFs;
import android.util.DisplayMetrics;
import android.view.InputDevice;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TimeZone;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * Settings → Report a problem: what Android knows (device, screen, controllers, the app's own log) and the two ways the
 * text leaves the app, the share sheet and the clipboard. The text itself is written and cleaned in Rust
 * (`pocket_ui::report_phone`); nothing here sends anything anywhere, and neither the phone's name nor a controller's name
 * is read.
 */
final class ReportSupport {
    private static final String TAG = "PocketMvp";
    /** The lines asked from logcat: a few runs of the app and the player. */
    private static final int LOG_LINES = 6000;
    private static final long LOG_WAIT_MS = 6000;

    private ReportSupport() {}

    private static void put(StringBuilder b, String key, String value) {
        b.append(key).append('=').append(value == null ? "" : value.replace('\n', ' ')).append('\n');
    }

    /** `key=value` lines of the device, for `report_phone::device_value`. */
    @SuppressWarnings("deprecation")
    static String device(Activity a) {
        StringBuilder b = new StringBuilder();
        SimpleDateFormat utc = new SimpleDateFormat("yyyy-MM-dd HH:mm 'UTC'", Locale.US);
        utc.setTimeZone(TimeZone.getTimeZone("UTC"));
        put(b, "time", utc.format(new Date()));
        try {
            PackageInfo info = a.getPackageManager().getPackageInfo(a.getPackageName(), 0);
            put(b, "app", info.versionName);
            put(b, "build", Long.toString(Build.VERSION.SDK_INT >= 28 ? info.getLongVersionCode() : info.versionCode));
        } catch (Exception e) {
            android.util.Log.w(TAG, "report: no package info", e);
        }
        put(b, "package", a.getPackageName());
        put(b, "channel", UpdateInstaller.channel(a).startsWith("https://pocketui.xyz/") ? "pocketui.xyz" : "custom (a test channel)");
        put(b, "manufacturer", Build.MANUFACTURER);
        put(b, "model", Build.MODEL);
        put(b, "device", Build.DEVICE);
        put(b, "release", Build.VERSION.RELEASE);
        put(b, "sdk", Integer.toString(Build.VERSION.SDK_INT));
        put(b, "abi", Build.SUPPORTED_ABIS.length > 0 ? Build.SUPPORTED_ABIS[0] : "");
        DisplayMetrics dm = a.getResources().getDisplayMetrics();
        put(b, "screen", dm.widthPixels + "x" + dm.heightPixels);
        put(b, "dpi", Integer.toString(dm.densityDpi));
        try {
            float hz = Build.VERSION.SDK_INT >= 30 && a.getDisplay() != null ? a.getDisplay().getRefreshRate() : a.getWindowManager().getDefaultDisplay().getRefreshRate();
            put(b, "refresh", String.format(Locale.US, "%.0f", hz));
        } catch (Exception e) {
            android.util.Log.w(TAG, "report: no refresh rate", e);
        }
        put(b, "fontscale", String.format(Locale.US, "%.2f", a.getResources().getConfiguration().fontScale));
        // Game controllers by their USB ids only: a Bluetooth pad can carry its owner's name.
        List<String> pads = new ArrayList<>();
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice d = InputDevice.getDevice(id);
            if (d == null || d.isVirtual()) continue;
            int s = d.getSources();
            if ((s & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD || (s & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) {
                pads.add(String.format(Locale.US, "0x%04x:0x%04x", d.getVendorId(), d.getProductId()));
            }
        }
        put(b, "gamepads", pads.isEmpty() ? "" : pads.size() + " (" + String.join(", ", pads) + ")");
        PowerManager pm = (PowerManager) a.getSystemService(Context.POWER_SERVICE);
        put(b, "batterysaver", pm != null && pm.isPowerSaveMode() ? "on" : "off");
        try {
            put(b, "free", String.format(Locale.US, "%.1f GB", new StatFs(a.getFilesDir().getAbsolutePath()).getAvailableBytes() / 1e9));
        } catch (Exception e) {
            android.util.Log.w(TAG, "report: no free space", e);
        }
        return b.toString();
    }

    /**
     * The end of this app's own log, from logcat: the first line names the two processes (`main=<pid> player=<pid>`), the
     * rest are logcat's `threadtime` lines. Kept: what the app and its player wrote (tag PocketMvp), warnings and errors of
     * those two processes, and a crash trace that names this package. Everything else logcat may show on a phone that
     * lets apps read the whole log is dropped here, before it reaches the text.
     */
    static String log(Activity a) {
        int main = android.os.Process.myPid();
        int uid = android.os.Process.myUid();
        int player = -1;
        ActivityManager am = (ActivityManager) a.getSystemService(Context.ACTIVITY_SERVICE);
        if (am != null) {
            List<ActivityManager.RunningAppProcessInfo> running = am.getRunningAppProcesses();
            if (running != null) {
                for (ActivityManager.RunningAppProcessInfo p : running) {
                    if (p.uid == uid && p.processName != null && p.processName.endsWith(":player")) player = p.pid;
                }
            }
        }
        List<String> raw = new ArrayList<>();
        Process process = null;
        try {
            process = new ProcessBuilder("logcat", "-d", "-t", Integer.toString(LOG_LINES), "-v", "threadtime").redirectErrorStream(true).start();
            final Process watched = process;
            Thread watchdog = new Thread(() -> {
                try { Thread.sleep(LOG_WAIT_MS); } catch (InterruptedException e) { return; }
                watched.destroy();
            }, "report-logcat-watchdog");
            watchdog.setDaemon(true);
            watchdog.start();
            try (BufferedReader in = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = in.readLine()) != null) raw.add(line);
            }
            watchdog.interrupt();
        } catch (Exception e) {
            android.util.Log.w(TAG, "report: logcat failed", e);
        } finally {
            if (process != null) process.destroy();
        }
        StringBuilder out = new StringBuilder("main=" + main + " player=" + player + "\n");
        // A crash trace is a group of AndroidRuntime lines of one process; it counts when a line names this package.
        Map<String, Boolean> ours = new HashMap<>();
        for (String line : raw) {
            String[] f = fields(line);
            if (f != null && f[3].equals("AndroidRuntime") && line.contains(a.getPackageName())) ours.put(f[0], true);
        }
        for (String line : raw) {
            String[] f = fields(line);
            if (f == null) continue;
            boolean mine = f[0].equals(Integer.toString(main)) || f[0].equals(Integer.toString(player));
            boolean written = f[3].equals(TAG);
            boolean trouble = f[2].equals("W") || f[2].equals("E") || f[2].equals("F");
            boolean crash = f[3].equals("AndroidRuntime") && ours.containsKey(f[0]);
            if (written || (mine && trouble) || crash) out.append(line).append('\n');
        }
        return out.toString();
    }

    /** `threadtime`: `MM-DD HH:MM:SS.mmm  PID  TID L TAG: message` as {pid, tid, level, tag}; null for any other line. */
    private static String[] fields(String line) {
        String[] p = line.trim().split("\\s+", 7);
        if (p.length < 6 || p[4].length() != 1) return null;
        String tag = p[5].endsWith(":") ? p[5].substring(0, p[5].length() - 1) : p[5];
        return new String[]{p[2], p[3], p[4], tag};
    }

    /** The share sheet with the text; "ok" once it is on screen, "failed" when Android has nothing to share with. */
    static String share(Activity a, String text, String title) {
        return onUi(a, () -> {
            Intent send = new Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_SUBJECT, "pocket-ui problem report").putExtra(Intent.EXTRA_TEXT, text);
            a.startActivity(Intent.createChooser(send, title));
        });
    }

    /** The clipboard with the text; Android 13 shows its own "copied" note, older ones a toast. */
    static String copy(Activity a, String text) {
        return onUi(a, () -> {
            ClipboardManager cm = (ClipboardManager) a.getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null) throw new IllegalStateException("no clipboard");
            cm.setPrimaryClip(ClipData.newPlainText("pocket-ui problem report", text));
        });
    }

    private static String onUi(Activity a, Runnable work) {
        final CountDownLatch done = new CountDownLatch(1);
        final boolean[] ok = {false};
        a.runOnUiThread(() -> {
            try {
                work.run();
                ok[0] = true;
            } catch (RuntimeException e) {
                android.util.Log.w(TAG, "report: hand-over failed", e);
            } finally {
                done.countDown();
            }
        });
        try {
            return done.await(5, TimeUnit.SECONDS) && ok[0] ? "ok" : "failed";
        } catch (InterruptedException e) {
            return "failed";
        }
    }
}
