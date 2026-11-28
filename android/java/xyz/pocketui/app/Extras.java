package xyz.pocketui.app;

import android.content.ComponentName;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import java.io.File;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * Android extras (Settings → System → Android extras; docs/ANDROID-EXTRAS.md): what pocket-ui can do beyond a normal app once
 * the player gives it more, each step of their own choosing and off by default.
 *
 * - **ADB grant** (once, from a computer: `adb shell pm grant <package> android.permission.WRITE_SECURE_SETTINGS`, or A
 *   on its row with Shizuku or root, which also takes it back): Android's global settings. No pop-up notifications, no
 *   animations, the screen on while charging, the system bars hidden everywhere (Android 10 and older; Android 11 took
 *   that setting away). The pause menu itself is turned on on Android's accessibility page.
 * - **Shizuku** (`ShizukuLink`): Android's shell (and the grant on the player's A): keys pressed in another emulator
 *   (save and load from our pause menu), the whole system log for a bug report.
 * - **Root** (Magisk's `su`): the shell's part too, and what only root may (`android_extras::root` in Rust builds the scripts:
 *   processor and graphics clocks per game, a fan where Linux shows one, the RG556's next boot slot).
 *
 * Everything we change in Android's settings is written down first (`files/extras/before.properties`) and put back when its
 * switch goes off. The handheld view asks through PocketBase's `extrasStatus`/`extrasAction`.
 */
final class Extras {
    private static final String TAG = "PocketExtras";
    static final String WRITE_SECURE = "android.permission.WRITE_SECURE_SETTINGS";

    /** A switch: the global settings it writes and their values while it is on. */
    private static final String[][] SWITCHES = {
        {"heads_up", "heads_up_notifications_enabled", "0"},
        {"animations", "window_animation_scale", "0", "transition_animation_scale", "0", "animator_duration_scale", "0"},
        {"awake", "stay_on_while_plugged_in", "7"},
        // policy_control: hidden bars for every app; Android 11 (API 30) removed it.
        {"bars", "policy_control", "immersive.full=*"},
    };

    private Extras() { }

    static File dir(Context c) {
        File d = new File(c.getFilesDir(), "extras");
        if (!d.isDirectory() && !d.mkdirs()) android.util.Log.w(TAG, "no folder " + d);
        return d;
    }

    static File file(Context c, String name) {
        return new File(dir(c), name);
    }

    static String read(File f) {
        try (InputStream in = new java.io.FileInputStream(f)) {
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
            byte[] b = new byte[8192];
            for (int n; (n = in.read(b)) > 0; ) {
                out.write(b, 0, n);
                if (out.size() > 1 << 20) break;
            }
            return out.toString("UTF-8");
        } catch (java.io.IOException e) {
            return "";
        }
    }

    /** `text` into `f` whole: a temporary file first, then renamed over it. */
    static boolean write(File f, String text) {
        File tmp = new File(f.getPath() + ".tmp");
        try (java.io.FileOutputStream out = new java.io.FileOutputStream(tmp)) {
            out.write(text.getBytes(StandardCharsets.UTF_8));
            out.getFD().sync();
        } catch (java.io.IOException e) {
            android.util.Log.w(TAG, "not written: " + f, e);
            return false;
        }
        return tmp.renameTo(f);
    }

    static Thread drain(InputStream in, StringBuilder out) {
        Thread t = new Thread(() -> {
            try (java.io.BufferedReader r = new java.io.BufferedReader(new java.io.InputStreamReader(in, StandardCharsets.UTF_8))) {
                for (String line; (line = r.readLine()) != null; ) synchronized (out) { out.append(line).append('\n'); }
            } catch (java.io.IOException ignored) { }
        }, "extras-output");
        t.setDaemon(true);
        t.start();
        return t;
    }

    // --- What the player gave -------------------------------------------------------------------------------------------

    static boolean has(Context c, String permission) {
        return c.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED;
    }

    private static boolean inExtrasProcess() {
        String name = android.os.Build.VERSION.SDK_INT >= 28 ? android.app.Application.getProcessName() : "";
        return name != null && name.endsWith(":extras");
    }

    /** A question to `ExtrasProvider` in the `:extras` process (it starts it); null when it can't be asked. */
    private static Bundle provider(Context c, String method, Bundle args) {
        try {
            return c.getContentResolver().call(ExtrasProvider.uri(c), method, null, args == null ? new Bundle() : args);
        } catch (RuntimeException e) {
            android.util.Log.w(TAG, "extras provider: " + method, e);
            return null;
        }
    }

    static String shizuku(Context c) {
        if (inExtrasProcess()) return ShizukuLink.state(c);
        // Without Shizuku on the phone nothing starts the extras' process.
        if (!ShizukuLink.installed(c)) return "missing";
        Bundle b = provider(c, "shizuku-state", null);
        String v = b == null ? null : b.getString("v");
        return v == null ? (ShizukuLink.installed(c) ? "stopped" : "missing") : v;
    }

    /** "yes" once `su` answered as root (`checkRoot`), "no" when it refused, "" before it was asked. Never asks itself. */
    static String root(Context c) {
        return read(file(c, "root")).trim();
    }

    /** Keys can be pressed in other apps: Shizuku is allowed, or root. */
    static boolean canPress(Context c) {
        return "ready".equals(shizuku(c)) || "yes".equals(root(c));
    }

    /** `cmd` as Android's shell through Shizuku, else as root; null when neither is there. Output with "exit=<n>" last. */
    static String privileged(Context c, String[] cmd, long timeoutMs) {
        return privileged(c, cmd, timeoutMs, null);
    }

    /**
     * As `privileged`, for the pause menu's key press (only in the `:extras` process): its process is bound to `token`, and
     * `cancelPress` ends it once the token is gone (also when the process shows up after the token went).
     */
    static String privileged(Context c, String[] cmd, long timeoutMs, File token) {
        String out;
        if (inExtrasProcess()) out = ShizukuLink.run(cmd, timeoutMs, token);
        else {
            Bundle args = new Bundle();
            args.putStringArray("cmd", cmd);
            args.putLong("timeout", timeoutMs);
            Bundle b = "ready".equals(shizuku(c)) ? provider(c, "shell", args) : null;
            out = b == null ? null : b.getString("v");
        }
        if (out != null || !"yes".equals(root(c))) return out;
        StringBuilder line = new StringBuilder();
        for (String part : cmd) line.append(line.length() == 0 ? "" : " ").append(quote(part));
        return su(line.toString(), timeoutMs, token);
    }

    /** Each press's `su` under its own token (a late one never displaces another's; each removes only its own entry). */
    private static final java.util.concurrent.ConcurrentHashMap<File, Process> pressSu = new java.util.concurrent.ConcurrentHashMap<>();

    /**
     * Ends the process of a press whose token is gone (Shizuku's or `su`). Off the accessibility thread: `destroy` is a binder
     * call that may hang (Sol's recheck); the token is what stops the press, this is only the clean-up.
     */
    static void cancelPress() {
        ShizukuLink.cancelStale();
        for (java.util.Map.Entry<File, Process> e : pressSu.entrySet()) if (!e.getKey().exists()) e.getValue().destroy();
    }

    static String quote(String s) {
        return s.matches("[A-Za-z0-9_./:=@%+,-]+") ? s : "'" + s.replace("'", "'\\''") + "'";
    }

    /** `script` through Magisk's `su -c`, at most `timeoutMs`. Output with "exit=<n>" last; null when there is no su. */
    static String su(String script, long timeoutMs) {
        return su(script, timeoutMs, null);
    }

    static String su(String script, long timeoutMs, File token) {
        Process p = null;
        boolean mine = false;
        try {
            p = new ProcessBuilder("su", "-c", script).start();
            if (token != null) {
                pressSu.put(token, p);
                mine = true;
                // Gone already (a menu or window came while su started): end it now.
                if (!token.exists()) p.destroy();
            }
            p.getOutputStream().close();
            StringBuilder out = new StringBuilder();
            Thread a = drain(p.getInputStream(), out), b = drain(p.getErrorStream(), out);
            boolean ended = p.waitFor(timeoutMs, java.util.concurrent.TimeUnit.MILLISECONDS);
            if (!ended) p.destroy();
            a.join(1000);
            b.join(1000);
            synchronized (out) { return out + "exit=" + (ended ? p.exitValue() : -1); }
        } catch (java.io.IOException e) {
            return null;
        } catch (InterruptedException e) {
            return "exit=-1";
        } finally {
            if (mine) pressSu.remove(token, p);
            if (p != null) p.destroy();
        }
    }

    /** Settings' "Check root": Magisk asks the player once; "yes" or "no" is kept (`root`). */
    static String checkRoot(Context c) {
        String out = su("id", 30000);
        boolean yes = out != null && out.contains("uid=0") && out.trim().endsWith("exit=0");
        write(file(c, "root"), yes ? "yes\n" : "no\n");
        android.util.Log.i(TAG, "root: " + (yes ? "yes" : out == null ? "no su" : "refused"));
        return yes ? "yes" : "no";
    }

    /** The Android user this app runs as (`UserHandle`'s id: the uid's hundred-thousands), for `pm` and `am`. */
    static String user() {
        return Integer.toString(android.os.Process.myUid() / 100000);
    }

    /**
     * The ADB grant given (`on`) or taken back through Shizuku or root, only when the player presses A on its row (Sol's review
     * 26.11.2026: nothing permanent behind their back). Taking it back first puts every switch back to Android's value: without
     * the grant pocket-ui could no longer undo them.
     */
    static String grant(Context c, boolean on) {
        if (!on) for (String[] sw : SWITCHES) if ("on".equals(switchState(c, sw[0]))) setSwitch(c, sw[0], false);
        String out = privileged(c, new String[]{"pm", on ? "grant" : "revoke", "--user", user(), c.getPackageName(), WRITE_SECURE}, 15000);
        android.util.Log.i(TAG, (on ? "grant: " : "revoke: ") + (out == null ? "no Shizuku or root" : out.trim()));
        if (out == null) return "Needs Shizuku or root";
        return has(c, WRITE_SECURE) == on ? "" : "Android said: " + out.trim();
    }

    // --- Android's settings ------------------------------------------------------------------------------------------------

    private static String[] switchOf(String name) {
        for (String[] s : SWITCHES) if (s[0].equals(name)) return s;
        return null;
    }

    static boolean barsPossible() {
        return android.os.Build.VERSION.SDK_INT <= 29;
    }

    /** "on" when Android has every value of the switch, "off" when not, "" when it can't be changed here. */
    static String switchState(Context c, String name) {
        String[] s = switchOf(name);
        if (s == null || (name.equals("bars") && !barsPossible())) return "";
        ContentResolver cr = c.getContentResolver();
        for (int i = 1; i < s.length; i += 2) {
            String now = Settings.Global.getString(cr, s[i]);
            if (now == null || !(now.equals(s[i + 1]) || (s[i + 1].equals("0") && now.matches("0(\\.0+)?")))) return "off";
        }
        return "on";
    }

    /** A switch on (Android's values written, the old ones kept first) or off (the old ones back). "" or why not. */
    static String setSwitch(Context c, String name, boolean on) {
        String[] s = switchOf(name);
        if (s == null) return "Unknown switch " + name;
        if (name.equals("bars") && !barsPossible()) return "Android 11 and newer have no setting for this";
        if (!has(c, WRITE_SECURE)) return "Needs the ADB grant or Shizuku";
        ContentResolver cr = c.getContentResolver();
        File keep = file(c, "before.properties");
        java.util.Properties before = new java.util.Properties();
        try (InputStream in = new java.io.FileInputStream(keep)) { before.load(in); } catch (java.io.IOException ignored) { }
        try {
            for (int i = 1; i < s.length; i += 2) {
                String key = "global/" + s[i];
                if (on) {
                    if (!before.containsKey(key)) {
                        String now = Settings.Global.getString(cr, s[i]);
                        before.setProperty(key, now == null ? "\u0000" : now);
                        if (!saveProps(keep, before)) return "Could not keep Android's old setting";
                    }
                    Settings.Global.putString(cr, s[i], s[i + 1]);
                } else {
                    String old = before.getProperty(key);
                    // Never changed by us: Android's own default. Not set before: the empty value (no policy, no override).
                    String back = old == null ? defaultOf(s[i]) : old.equals("\u0000") ? defaultOf(s[i]) : old;
                    Settings.Global.putString(cr, s[i], back);
                    before.remove(key);
                }
            }
        } catch (SecurityException e) {
            return "Android refused: " + e.getMessage();
        }
        saveProps(keep, before);
        android.util.Log.i(TAG, "switch " + name + (on ? " on" : " off"));
        return "";
    }

    private static String defaultOf(String key) {
        switch (key) {
            case "heads_up_notifications_enabled": return "1";
            case "stay_on_while_plugged_in": return "0";
            case "policy_control": return "";
            default: return "1";
        }
    }

    private static boolean saveProps(File f, java.util.Properties p) {
        java.io.StringWriter w = new java.io.StringWriter();
        try { p.store(w, "Android's settings before pocket-ui's extras changed them"); } catch (java.io.IOException e) { return false; }
        return write(f, w.toString());
    }

    // --- The pause menu in other emulators (PadWatch) ----------------------------------------------------------------------

    static boolean watchOn(Context c) {
        String list = Settings.Secure.getString(c.getContentResolver(), Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES);
        if (list == null) return false;
        ComponentName ours = new ComponentName(c.getPackageName(), PadWatch.class.getName());
        for (String s : list.split(":")) if (ours.equals(ComponentName.unflattenFromString(s))) return true;
        return false;
    }

    /**
     * The pause menu's service is turned on and off on Android's own accessibility page, never by writing the setting (Sol's
     * review 26.11.2026: an accessibility service is the player's to switch, where Android shows what it may do).
     */
    static String setWatch(Context c) {
        return openPage(c, new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
    }

    /** The handheld view started a game in another app (`PocketBase.launchApp`): PadWatch opens its menu only over it. */
    static boolean gameStarted(Context c, String pkg) {
        // A new launch id first, then every press token goes (Sol's recheck of 5a47a70b7): a press that made its token before
        // loses it here, one that makes it after sees the new id (`PadWatch.press` checks it with the token in place).
        // Called before the emulator's activity starts.
        // A launch not reliably written locks the presses until the next start (`LaunchNote`; Sol's recheck of 38cfee80f).
        String launch = Long.toHexString(new java.security.SecureRandom().nextLong());
        LaunchNote.Result r = LaunchNote.publish(file(c, "playing").getParentFile(), pressDir(c), pkg, launch, Extras::write);
        if (r != LaunchNote.Result.PUBLISHED) android.util.Log.w(TAG, "launch " + r + ": the pause menu presses no keys until the next start");
        invalidatePresses(c);
        // FAILED: the game before could still be pressed into; the caller doesn't start this one (Sol's recheck of 09f9f7f27).
        return r != LaunchNote.Result.FAILED;
    }

    /** Where a pause-menu press keeps its token: our own shared folder, which Android's shell and root can read. */
    private static File pressDir(Context c) {
        File ext = c.getExternalFilesDir(null);
        return ext == null ? null : new File(ext, "press");
    }

    /**
     * A new token for one press (`TargetKey` presses only while it exists); null when the folder can't be written (then nothing
     * is pressed). Other presses' tokens stay: a press never removes another's (Sol's recheck of 5a47a70b7); stale ones go
     * through `invalidatePresses`.
     */
    static File newPressToken(Context c, String nonce) {
        File dir = pressDir(c);
        if (dir == null || (!dir.isDirectory() && !dir.mkdirs())) return null;
        File token = new File(dir, nonce);
        try {
            return token.createNewFile() ? token : null;
        } catch (java.io.IOException e) {
            return null;
        }
    }

    /** Every press on its way is stale: its token goes (also before its program runs), and the program won't press. */
    static void invalidatePresses(Context c) {
        File dir = pressDir(c);
        File[] all = dir == null ? null : dir.listFiles();
        if (all != null) for (File f : all) {
            // The lock of a launch that couldn't be written stays (`LaunchNote`); only tokens go.
            if (f.getName().equals(LaunchNote.LOCK)) continue;
            if (!f.delete()) android.util.Log.w(TAG, "press token stays: " + f.getName());
        }
    }

    /** The handheld view is in front again: the game in the other app counts as ended. */
    static void gameEnded(Context c) {
        invalidatePresses(c);
        // Not `file`: a player without extras gets no folder of them.
        File f = new File(new File(c.getFilesDir(), "extras"), "playing");
        if (f.exists() && !f.delete()) android.util.Log.w(TAG, "playing note stays");
    }

    private static String playingCache = "";
    private static long playingStamp = -1;

    /** The package of the game we started in another app, or "". Read again only when the file changed. */
    /** The launch id of the game in the playing note (`gameStarted`), "" when there is none: a press belongs to one start. */
    static String playingLaunch(Context c) {
        // Read every time (no cache by modification time: two starts within one tick must still differ).
        return LaunchNote.launch(file(c, "playing").getParentFile());
    }

    /** The pause menu presses no keys: the last start's launch id was not reliably written (`LaunchNote`). */
    static boolean pressLocked(Context c) {
        return LaunchNote.locked(file(c, "playing").getParentFile(), pressDir(c));
    }

    static synchronized String playing(Context c) {
        File f = file(c, "playing");
        long stamp = f.exists() ? f.lastModified() ^ f.length() : 0;
        if (stamp != playingStamp) {
            playingStamp = stamp;
            String note = stamp == 0 ? "" : read(f).trim();
            playingCache = note.contains("\n") ? note.substring(0, note.indexOf('\n')).trim() : note;
        }
        return playingCache;
    }

    /** The overlay's lines in the player's language, as the handheld view wrote them (`texts.tsv`: English, tab, translation). */
    static java.util.Map<String, String> texts(Context c) {
        java.util.Map<String, String> map = new java.util.HashMap<>();
        for (String line : read(file(c, "texts.tsv")).split("\n")) {
            int tab = line.indexOf('\t');
            if (tab > 0) map.put(line.substring(0, tab), line.substring(tab + 1));
        }
        return map;
    }

    // --- The system log for a bug report -----------------------------------------------------------------------------------

    /**
     * The end of Android's whole log through the shell (Shizuku or root), else null: then the bug report reads its own
     * (ReportSupport). The READ_LOGS permission is left out on purpose: Android 13 and newer would ask at every report.
     */
    static String logcat(Context c, int lines) {
        String out = privileged(c, new String[]{"logcat", "-d", "-t", Integer.toString(lines), "-v", "threadtime"}, 4000);
        if (out == null) return null;
        int end = out.lastIndexOf("exit=");
        return end >= 0 ? out.substring(0, end) : out;
    }

    // --- What the handheld view asks -----------------------------------------------------------------------------------

    private static String openPage(Context c, Intent intent) {
        try {
            c.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            return "";
        } catch (RuntimeException e) {
            return "Android has no page for this";
        }
    }

    /** Everything the settings page shows, as one JSON object (`android_extras::Status` reads it). */
    static String status(Context c) {
        org.json.JSONObject o = new org.json.JSONObject();
        try {
            o.put("sdk", android.os.Build.VERSION.SDK_INT);
            o.put("package", c.getPackageName());
            o.put("model", android.os.Build.MODEL);
            o.put("maker", android.os.Build.MANUFACTURER);
            o.put("secure", has(c, WRITE_SECURE));
            o.put("shizuku", shizuku(c));
            o.put("root", root(c));
            o.put("watch", watchOn(c));
            org.json.JSONObject sw = new org.json.JSONObject();
            for (String[] s : SWITCHES) sw.put(s[0], switchState(c, s[0]));
            o.put("switches", sw);
        } catch (org.json.JSONException e) {
            android.util.Log.w(TAG, "status", e);
        }
        return o.toString();
    }

    /** A press on the settings page: "" when done (or under way), else why not, in English for the log. */
    static String action(Context c, String what, String arg) {
        switch (what) {
            case "shizuku-get":
                // Shizuku's own page in the store; without a store its download page.
                if (openPage(c, new Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=" + ShizukuLink.PACKAGE))).isEmpty()) return "";
                return openPage(c, new Intent(Intent.ACTION_VIEW, Uri.parse("https://shizuku.rikka.app/download/")));
            case "shizuku-open": {
                Intent open = c.getPackageManager().getLaunchIntentForPackage(ShizukuLink.PACKAGE);
                return open == null ? "Shizuku is not installed" : openPage(c, open);
            }
            case "shizuku-request": {
                Bundle b = provider(c, "shizuku-request", null);
                return b == null ? "No answer" : String.valueOf(b.getString("v", ""));
            }
            case "root-check": return checkRoot(c);
            case "switch": {
                int eq = arg.indexOf('=');
                return eq < 0 ? "Bad switch" : setSwitch(c, arg.substring(0, eq), arg.substring(eq + 1).equals("on"));
            }
            case "watch": return setWatch(c);
            case "grant": return grant(c, arg.equals("on"));
            case "watch-settings": return openPage(c, new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
            case "app-info": return openPage(c, new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + c.getPackageName())));
            case "texts": return write(file(c, "texts.tsv"), arg) ? "" : "Not written";
            case "game-end": gameEnded(c); return "";
            case "root-run": {
                if (!"yes".equals(root(c))) return "exit=-1 no root";
                String out = su(arg, 20000);
                return out == null ? "exit=-1 no su" : out;
            }
            default: return "Unknown action " + what;
        }
    }
}
