package xyz.pocketui.app;

import android.content.Context;
import android.os.Bundle;
import android.os.IBinder;
import android.os.ParcelFileDescriptor;
import moe.shizuku.server.IRemoteProcess;
import moe.shizuku.server.IShizukuApplication;
import moe.shizuku.server.IShizukuService;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): Shizuku, the app that runs commands for other apps as Android's shell (adb's user)
 * once the player started it (Wireless debugging or root) and said yes to pocket-ui. Its server hands its binder to
 * `ExtrasProvider` (`<package>.shizuku`) in the `:extras` process; this keeps it. The protocol is Shizuku-API's (MIT,
 * android/licences/SHIZUKU-API.txt): the binder interfaces of android/aidl and the attach of `rikka.shizuku.Shizuku`
 * (API 13), written out here because this build has no Gradle or AAR (only what is used: attach, permission, a process).
 */
final class ShizukuLink {
    static final String PACKAGE = "moe.shizuku.privileged.api";
    private static final String TAG = "PocketExtras";
    // rikka.shizuku.ShizukuApiConstants (API 13).
    private static final int API = 13;
    private static final String ATTACH_PACKAGE = "shizuku:attach-package-name", ATTACH_API = "shizuku:attach-api-version";
    private static final String GRANTED = "shizuku:attach-reply-permission-granted", ALLOWED = "shizuku:request-permission-reply-allowed";

    private static volatile IBinder binder;
    private static volatile IShizukuService service;
    private static volatile boolean granted;

    private static final IShizukuApplication.Stub APPLICATION = new IShizukuApplication.Stub() {
        @Override public void bindApplication(Bundle data) {
            granted = data.getBoolean(GRANTED, false);
            android.util.Log.i(TAG, "shizuku: attached, " + (granted ? "allowed" : "not allowed yet"));
        }
        @Override public void dispatchRequestPermissionResult(int requestCode, Bundle data) {
            granted = data.getBoolean(ALLOWED, false);
            android.util.Log.i(TAG, "shizuku: the player said " + (granted ? "yes" : "no"));
        }
        @Override public void showPermissionConfirmation(int uid, int pid, String pkg, int requestCode) { }
    };

    /** Shizuku's server sent its binder (`ExtrasProvider.call` "sendBinder"). */
    static synchronized void received(IBinder b, String pkg) {
        if (b == null || b == binder) return;
        binder = b;
        service = IShizukuService.Stub.asInterface(b);
        granted = false;
        try {
            b.linkToDeath(() -> { binder = null; service = null; granted = false; android.util.Log.i(TAG, "shizuku: stopped"); }, 0);
        } catch (Exception e) {
            android.util.Log.w(TAG, "shizuku: no death notice", e);
        }
        Bundle args = new Bundle();
        args.putInt(ATTACH_API, API);
        args.putString(ATTACH_PACKAGE, pkg);
        try {
            service.attachApplication(APPLICATION, args);
        } catch (Exception e) {
            android.util.Log.w(TAG, "shizuku: attach failed (a server older than API 13?)", e);
        }
    }

    static boolean installed(Context c) {
        try { c.getPackageManager().getPackageInfo(PACKAGE, 0); return true; } catch (Exception e) { return false; }
    }

    static boolean running() {
        IBinder b = binder;
        return b != null && b.pingBinder();
    }

    /** "missing", "stopped", "ask" (running, pocket-ui not allowed yet) or "ready". */
    static String state(Context c) {
        if (running()) {
            if (!granted) {
                try { granted = service.checkSelfPermission(); } catch (Exception e) { android.util.Log.w(TAG, "shizuku: permission check", e); }
            }
            return granted ? "ready" : "ask";
        }
        return installed(c) ? "stopped" : "missing";
    }

    /** Shizuku's own dialog: may pocket-ui use it? The answer comes to `APPLICATION`. */
    static String request() {
        IShizukuService s = service;
        if (s == null || !running()) return "Shizuku is not running";
        try { s.requestPermission(1); return ""; } catch (Exception e) { return String.valueOf(e.getMessage()); }
    }

    /**
     * `cmd` as Android's shell, at most `timeoutMs`: its output (stdout and stderr), and "exit=<n>" in the last line. Null when
     * Shizuku is not there or not allowed.
     */
    /**
     * Each press's process under its own token file (`TargetKey`): a late one never displaces another's (Sol's recheck of
     * 5a47a70b7), and each removes only its own entry.
     */
    private static final java.util.concurrent.ConcurrentHashMap<java.io.File, IRemoteProcess> presses = new java.util.concurrent.ConcurrentHashMap<>();

    /** Ends every press's process whose token is gone (a new menu, window or game start). Not on the UI thread. */
    static void cancelStale() {
        for (java.util.Map.Entry<java.io.File, IRemoteProcess> e : presses.entrySet()) {
            if (!e.getKey().exists()) try { e.getValue().destroy(); } catch (Exception ignored) { }
        }
    }

    static String run(String[] cmd, long timeoutMs) {
        return run(cmd, timeoutMs, null);
    }

    static String run(String[] cmd, long timeoutMs, java.io.File token) {
        IShizukuService s = service;
        if (s == null || !granted || !running()) return null;
        IRemoteProcess p = null;
        IRemoteProcess mine = null;
        try {
            p = s.newProcess(cmd, null, null);
            if (token != null) {
                presses.put(token, p);
                mine = p;
                // The token went while Shizuku started it: end it now (it would not press anyway).
                if (!token.exists()) try { p.destroy(); } catch (Exception ignored) { }
            }
            try { ParcelFileDescriptor in = p.getOutputStream(); if (in != null) in.close(); } catch (Exception ignored) { }
            StringBuilder out = new StringBuilder();
            Thread reader = Extras.drain(new ParcelFileDescriptor.AutoCloseInputStream(p.getInputStream()), out);
            Thread errors = Extras.drain(new ParcelFileDescriptor.AutoCloseInputStream(p.getErrorStream()), out);
            boolean ended = p.waitForTimeout(timeoutMs, "MILLISECONDS");
            reader.join(1000);
            errors.join(1000);
            int code = ended ? p.exitValue() : -1;
            synchronized (out) { return out + "exit=" + code; }
        } catch (Exception e) {
            android.util.Log.w(TAG, "shizuku: " + cmd[0] + " failed", e);
            return "exit=-1 " + e;
        } finally {
            // Every way out ends the process it started (Sol's review of 82c82f0ba: an error after newProcess left it behind).
            if (mine != null) presses.remove(token, mine);
            if (p != null) try { p.destroy(); } catch (Exception ignored) { }
        }
    }
}
