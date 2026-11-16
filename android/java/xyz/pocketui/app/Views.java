package xyz.pocketui.app;

import android.content.Context;
import android.content.Intent;
import java.io.File;

/** Which of the two views the app opens in: "handheld" (the Linux pages, HandheldActivity, for the built-in pad) or "mobile"
 *  (PocketActivity, the touch pages). Chosen once at the first start (a pad there: handheld), then only by Settings → View in
 *  either view. One small file in the app's folder, so both views and the launcher entry read the same answer. */
final class Views {
    static final String HANDHELD = "handheld", MOBILE = "mobile";
    private Views() {}

    private static File file(Context context) { return new File(context.getFilesDir(), "view"); }

    /** The chosen view; at the first start the pads decide and the answer is kept. */
    static String chosen(Context context) {
        File f = file(context);
        try (java.io.FileInputStream in = new java.io.FileInputStream(f)) {
            byte[] b = new byte[16];
            int n = in.read(b);
            String v = n > 0 ? new String(b, 0, n, "UTF-8").trim() : "";
            if (v.equals(HANDHELD) || v.equals(MOBILE)) return v;
        } catch (java.io.IOException ignored) {}
        String first = hasGamepad() ? HANDHELD : MOBILE;
        save(context, first);
        android.util.Log.i("PocketMvp", "view: first start chose " + first);
        return first;
    }

    static void save(Context context, String view) {
        File f = file(context), tmp = new File(f.getPath() + ".part");
        try (java.io.FileOutputStream out = new java.io.FileOutputStream(tmp)) {
            out.write(view.getBytes("UTF-8"));
            out.getFD().sync();
        } catch (java.io.IOException e) { android.util.Log.w("PocketMvp", "view: not saved", e); return; }
        if (!tmp.renameTo(f)) android.util.Log.w("PocketMvp", "view: not saved");
    }

    /** A pad is there: built in (Odin, Retroid) or attached. The same test as the player's (`PlayerActivity.hasGamepad`). */
    static boolean hasGamepad() {
        for (int id : android.view.InputDevice.getDeviceIds()) {
            android.view.InputDevice device = android.view.InputDevice.getDevice(id);
            if (device == null) continue;
            int sources = device.getSources();
            if ((sources & android.view.InputDevice.SOURCE_GAMEPAD) == android.view.InputDevice.SOURCE_GAMEPAD
                    || (sources & android.view.InputDevice.SOURCE_JOYSTICK) == android.view.InputDevice.SOURCE_JOYSTICK) return true;
        }
        return false;
    }

    /** The view in front: its task as it was when it is there (a paused game on top stays: starting a singleTask Activity
     *  would clear it away, review of 12.11.2026, #5), else its Activity in a task of its own. */
    static void show(android.app.Activity from, String view) {
        String target = (view.equals(HANDHELD) ? HandheldActivity.class : PocketActivity.class).getName();
        android.app.ActivityManager am = (android.app.ActivityManager) from.getSystemService(Context.ACTIVITY_SERVICE);
        try {
            for (android.app.ActivityManager.AppTask task : am.getAppTasks()) {
                android.content.ComponentName base = task.getTaskInfo().baseActivity;
                if (base != null && base.getClassName().equals(target)) { task.moveToFront(); return; }
            }
        } catch (RuntimeException e) { android.util.Log.w("PocketMvp", "view: task not found", e); }
        from.startActivity(open(from, view));
    }

    /** A NativeActivity of ours can't end in its process (its native loop never ends; NativeActivity.onDestroy waits for
     *  it): the process ends instead, as the app ends on Linux. */
    static void endProcess(android.app.Activity activity) {
        android.util.Log.i("PocketMvp", "view: " + activity.getClass().getSimpleName() + " ends, and its process");
        android.os.Process.killProcess(android.os.Process.myPid());
    }

    /** The Activity of a view, brought up in a task of its own; the caller finishes itself. */
    static Intent open(Context context, String view) {
        Class<?> target = view.equals(HANDHELD) ? HandheldActivity.class : PocketActivity.class;
        return new Intent(context, target).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_NO_ANIMATION);
    }
}
