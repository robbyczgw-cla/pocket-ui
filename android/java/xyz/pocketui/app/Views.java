package xyz.pocketui.app;

import android.content.Context;
import android.content.Intent;
import java.io.File;

/** Which of the two views the app opens in: "handheld" (the Linux pages, HandheldActivity, for the built-in pad or a
 *  controller) or "mobile" (PocketActivity, the touch pages, frozen since 19.11.2026). The first start opens Handheld, pad or
 *  not (before: no pad there, Mobile for good, and a phone whose Bluetooth pad was off or asleep at that start never left
 *  it); then only Settings → View in either view changes it. A view chosen before is kept: Mobile users have their games in
 *  a folder they picked there, which the Handheld view doesn't read. One small file in the app's folder, so both views and
 *  the launcher entry read the same answer (scripts/android/test/ViewsTest.java). */
final class Views {
    static final String HANDHELD = "handheld", MOBILE = "mobile";
    private Views() {}

    private static File file(Context context) { return new File(context.getFilesDir(), "view"); }

    /** The chosen view; at the first start (or with a file that doesn't read) Handheld, and the answer is kept. */
    static String chosen(Context context) {
        File f = file(context);
        try (java.io.FileInputStream in = new java.io.FileInputStream(f)) {
            byte[] b = new byte[16];
            int n = in.read(b);
            String v = n > 0 ? new String(b, 0, n, "UTF-8").trim() : "";
            if (v.equals(HANDHELD) || v.equals(MOBILE)) return v;
        } catch (java.io.IOException ignored) {}
        save(context, HANDHELD);
        android.util.Log.i("PocketMvp", "view: first start chose " + HANDHELD);
        return HANDHELD;
    }

    static void save(Context context, String view) {
        File f = file(context), tmp = new File(f.getPath() + ".part");
        try (java.io.FileOutputStream out = new java.io.FileOutputStream(tmp)) {
            out.write(view.getBytes("UTF-8"));
            out.getFD().sync();
        } catch (java.io.IOException e) { android.util.Log.w("PocketMvp", "view: not saved", e); return; }
        if (!tmp.renameTo(f)) android.util.Log.w("PocketMvp", "view: not saved");
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
