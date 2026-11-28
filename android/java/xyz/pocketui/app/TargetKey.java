package xyz.pocketui.app;

import java.io.File;
import java.io.IOException;
import java.lang.reflect.Constructor;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): one key press for one app. `PadWatch` runs this through Shizuku (Android's shell,
 * uid 2000) or Magisk's `su` (root) as its own program, the way Android's `input` command runs:
 * `app_process -Djava.class.path=<our APK> /system/bin xyz.pocketui.app.TargetKey <nonce> <token> <uid> <key code> <deadline>
 * <wait ms> <pattern> <dir>…`. `<token>` is a file in our app's own shared folder (`Extras.pressToken`): the app deletes it
 * when the press goes stale (a new menu or window, a new game start), even before this program runs, and the program presses
 * only while it is still there, looked at right before the down (Sol's recheck of f7df20bc8).
 *
 * Android 14 and newer (`ATOMIC_SDK`): `IInputManager.injectInputEventToTarget(event, WAIT_FOR_RESULT, uid)`. Android's input
 * dispatcher checks, at the moment it hands the key to a window, that the focused window belongs to `uid`; if not, it drops
 * the key and the call throws (TARGET_MISMATCH). The key reaches the emulator or nothing (Sol's review of 6aaab4b7c).
 *
 * Android 13 (`SDK`): the same call exists, but its InputManagerService answers a mismatch by injecting the key once more into
 * every window when the caller may inject (the shell and root may: AOSP android13-release, b/228161340; seen on the RG556 on
 * 27.11.2026). So there the press is guarded instead: the input dispatcher's own focused window (its dump, not the window
 * manager's) must belong to `uid` right before the down, and again right after it; else nothing is sent, or the result is
 * "unsure". Left over: a window that takes the focus in the milliseconds between the first look and the down gets the key
 * (ANDROID-EXTRAS.md). Older Android: nothing is pressed.
 *
 * `<deadline>` is `SystemClock.uptimeMillis()` (one clock for the whole device, never jumps): a press that would go down later
 * is dropped, so a stale job can never fire into a game started since. After a save it compares the emulator's state folders
 * with a snapshot from just before the down: "saved" only for a state file that is new or changed and has stopped growing,
 * and only if the snapshot was complete. The one status line is `pocket-targetkey <nonce> <status>[ <detail>]`; everything
 * else (Android's own warnings) is noise for `outcome`.
 *
 * No Android types at compile time (reflection only), so `scripts/android/test/ExtrasKeysTest.java` runs its logic on a JVM.
 */
public final class TargetKey {
    /** Android 13 (API 33): the first with a target uid; guarded by the focus check there. */
    static final int SDK = 33;
    /** Android 14 (API 34): the first that drops a key its target's window would not get, instead of falling back. */
    static final int ATOMIC_SDK = 34;
    /** InputManager.INJECT_INPUT_EVENT_MODE_WAIT_FOR_RESULT: back once a window was picked (or none). */
    static final int WAIT_FOR_RESULT = 1;
    /** How long the key is held: emulators that read their hotkeys once per frame (Dolphin) miss a press of 0 ms. */
    static final long HOLD_MS = 80;
    /** How deep a state folder is walked, and how many entries at most (a big folder must not make the press slow). */
    static final int DEPTH = 4, MAX_ENTRIES = 4000;
    /** Android's first app uid within a user (Process.FIRST_APPLICATION_UID). */
    static final int FIRST_APP_ID = 10000;
    static final String TAG = "pocket-targetkey";
    /** Complete scans in a row that must see a changed state file unchanged before it counts as written. */
    static final int STABLE_LOOKS = 3;

    /** The press itself: down (0) or up (1) towards `uid`'s window, true when Android handed it to a window. */
    interface Injector {
        boolean inject(int action) throws Exception;
    }

    /** The owner uid of the window the input dispatcher sends keys to now; -1 when there is none or it is unclear. */
    interface Focus {
        int uid();
    }

    /** Whether this press is still wanted (its token file exists). */
    interface Token {
        boolean valid();
    }

    /** One look at the state folders (a real scan, or a list of prepared snapshots in the JVM test). */
    interface Scan {
        Snapshot now();
    }

    /** Monotonic milliseconds and the pause between looks, replaceable in the JVM test. */
    interface Clock {
        long now();
        void sleep(long ms);
    }

    static final Clock REAL_CLOCK = new Clock() {
        @Override public long now() { return System.nanoTime() / 1_000_000; }
        @Override public void sleep(long ms) { sleepQuietly(ms); }
    };

    public static void main(String[] args) {
        String nonce = args.length > 0 ? args[0] : "-";
        String out;
        try {
            if (args.length < 7) throw new IllegalArgumentException("usage: <nonce> <token> <uid> <code> <deadline> <wait ms> <pattern> <dir>…");
            final File tokenFile = new File(args[1]);
            int uid = Integer.parseInt(args[2]), code = Integer.parseInt(args[3]);
            long deadline = Long.parseLong(args[4]), waitMs = Long.parseLong(args[5]);
            String[] dirs = new String[args.length - 7];
            System.arraycopy(args, 7, dirs, 0, dirs.length);
            // This program never outlives its job (Sol's review: a hung binder or su must not leave it behind).
            final long life = Math.max(0, deadline - uptime()) + waitMs + 5000;
            Thread guard = new Thread(() -> { sleepQuietly(life); Runtime.getRuntime().halt(3); }, "targetkey-life");
            guard.setDaemon(true);
            guard.start();
            int sdk = sdk();
            if (!appUid(uid) || code < EmulatorKeys.KEYCODE_F1 || code > EmulatorKeys.KEYCODE_F1 + 11) out = "error bad-target";
            else if (sdk < SDK) out = "unsupported";
            else if (uptime() > deadline) out = "late";
            else if (!tokenFile.isFile()) out = "cancelled";
            else {
                Injector inj = android(uid, code);
                Focus focus = sdk >= ATOMIC_SDK ? null : () -> focusedUid(inputDump());
                out = inj == null ? "unsupported" : run(inj, focus, tokenFile::isFile, uid, REAL_CLOCK, deadline - uptime(), waitMs, scanOf(args[6], dirs));
            }
        } catch (Throwable e) {
            Throwable why = e instanceof InvocationTargetException && e.getCause() != null ? e.getCause() : e;
            out = "error " + why.getClass().getSimpleName();
        }
        System.out.println(TAG + " " + nonce + " " + out);
        System.out.flush();
        System.exit(0);
    }

    /** An app's uid of some user (not root, the system, the shell, or "no uid"). */
    static boolean appUid(int uid) {
        return uid > 0 && uid % 100000 >= FIRST_APP_ID;
    }

    /** The scan of the state folders for `pattern`, or null when nothing is to be looked at ("-", no folders). */
    static Scan scanOf(String pattern, String[] dirs) {
        if (pattern == null || pattern.equals("-") || pattern.isEmpty() || dirs.length == 0) return null;
        final Pattern names = Pattern.compile(pattern);
        return () -> Snapshot.of(dirs, names);
    }

    /** `run` with a token that stays valid and the real scan (the JVM test's short form). */
    static String run(Injector inj, Focus focus, int uid, Clock clock, long leftMs, long waitMs, String pattern, String[] dirs) throws Exception {
        return run(inj, focus, () -> true, uid, clock, leftMs, waitMs, scanOf(pattern, dirs));
    }

    /**
     * The press. `focus` is null where Android itself binds it (14+); on Android 13 it is the dispatcher's focus, looked at
     * right before the down and right after it. `leftMs` is the time until the deadline; `token` must still be valid right
     * before the down. `scan` (null: no look) watches the state folders after a save.
     */
    static String run(Injector inj, Focus focus, Token token, int uid, Clock clock, long leftMs, long waitMs, Scan scan) throws Exception {
        boolean look = scan != null && waitMs > 0;
        long stop = clock.now() + leftMs;
        Snapshot before = look ? scan.now() : null;
        if (focus != null && focus.uid() != uid) return "missed";
        // The deadline and the token after every slow step (the snapshot, the focus look): right before the down.
        if (clock.now() > stop) return "late";
        if (!token.valid()) return "cancelled";
        // The token look reads shared storage, which can be slow: the clock once more, the last step before the down.
        if (clock.now() > stop) return "late";
        boolean down;
        try {
            down = inj.inject(0);
        } catch (InvocationTargetException | IllegalArgumentException e) {
            Throwable why = e instanceof InvocationTargetException ? e.getCause() : e;
            // Android 14+: TARGET_MISMATCH, the key went nowhere.
            if (why instanceof IllegalArgumentException) return "missed";
            throw e;
        }
        // Android 13: the focus moved during the down, so the dispatcher's fallback may have sent it elsewhere.
        if (focus != null && focus.uid() != uid) return "unsure";
        // false: a timeout or a failure inside Android; the key may still arrive (Sol's review): not "missed".
        if (!down) return "unsure";
        clock.sleep(HOLD_MS);
        String up = "";
        // The up only into the same window: if the focus left meanwhile, Android cancels the held key there itself.
        if (focus == null || focus.uid() == uid) {
            try { if (!inj.inject(1)) up = " up-unconfirmed"; } catch (Exception e) { up = " up-dropped"; }
        } else {
            up = " up-skipped";
        }
        if (!look) return "sent" + up;
        if (!before.complete) return "sent" + up + " snapshot-incomplete";
        long end = clock.now() + waitMs;
        // A changed file counts once it looked the same in STABLE_LOOKS complete scans in a row; an incomplete scan (a folder
        // unreadable, the budget used up) starts the count over (Sol's recheck: no candidate across a gap).
        Map<String, long[]> last = new HashMap<>();
        Map<String, Integer> same = new HashMap<>();
        while (true) {
            Snapshot now = scan.now();
            if (!now.complete) {
                last.clear();
                same.clear();
            } else {
                Map<String, long[]> changed = new HashMap<>();
                for (Map.Entry<String, long[]> e : now.files.entrySet()) {
                    long[] was = before.files.get(e.getKey()), is = e.getValue();
                    if (was == null || was[0] != is[0] || was[1] != is[1]) changed.put(e.getKey(), is);
                }
                Map<String, Integer> next = new HashMap<>();
                for (Map.Entry<String, long[]> e : changed.entrySet()) {
                    long[] prev = last.get(e.getKey()), is = e.getValue();
                    int n = prev != null && prev[0] == is[0] && prev[1] == is[1] ? same.getOrDefault(e.getKey(), 1) + 1 : 1;
                    if (n >= STABLE_LOOKS && is[1] > 0) return "saved" + up;
                    next.put(e.getKey(), n);
                }
                last = changed;
                same = next;
            }
            if (clock.now() >= end) return "sent" + up;
            clock.sleep(150);
        }
    }

    /** Every state file (path → modification time, size) under the folders, read with a hard budget. */
    static final class Snapshot {
        final Map<String, long[]> files;
        boolean complete;
        private int left = MAX_ENTRIES;

        Snapshot() {
            this(new HashMap<>(), true);
        }

        /** A prepared snapshot (the JVM test). */
        Snapshot(Map<String, long[]> files, boolean complete) {
            this.files = files;
            this.complete = complete;
        }

        static Snapshot of(String[] dirs, Pattern names) {
            Snapshot s = new Snapshot();
            for (String d : dirs) s.walk(new File(d).toPath(), names, DEPTH);
            return s;
        }

        private void walk(Path dir, Pattern names, int depth) {
            // Missing is fine (not every default folder exists); there but unreadable makes the snapshot incomplete.
            if (Files.notExists(dir)) return;
            if (!Files.isDirectory(dir)) { complete = false; return; }
            try (DirectoryStream<Path> list = Files.newDirectoryStream(dir)) {
                for (Path p : list) {
                    if (left-- <= 0) { complete = false; return; }
                    if (Files.isDirectory(p)) {
                        if (depth > 1) walk(p, names, depth - 1);
                        else complete = false;
                    } else if (names.matcher(p.getFileName().toString()).matches()) {
                        File f = p.toFile();
                        files.put(p.toString(), new long[]{f.lastModified(), f.length()});
                    }
                }
            } catch (IOException | RuntimeException e) {
                complete = false;
            }
        }
    }

    /** A window name the parser trusts: no quote, comma, equals sign or line break, so a title can't fake a field or a line. */
    private static final Pattern SAFE_NAME = Pattern.compile("[^',=\\r\\n]{1,200}");
    private static final Pattern FOCUSED_LINE = Pattern.compile("displayId=([0-9]+), name='([^']*)'");
    /** The fields after a window's name, in AOSP 13's fixed order and format; the owner is the last `ownerUid=` of the line. */
    private static final Pattern OWNER = Pattern.compile(", ownerPid=[0-9]+, ownerUid=([0-9]+), dispatchingTimeout=[0-9]+ms, ");

    /**
     * The owner uid of the input dispatcher's focused window, from its dump (`dumpsys input`), or -1 whenever anything is not
     * clear (Sol's recheck of 058919942: a window's name holds its title, which the app sets, and AOSP prints it unescaped
     * before the real fields). The focused display's `FocusedWindows:` entry must be exactly `displayId=N, name='…'` with a
     * name of `SAFE_NAME` (no quote, comma, `=` or line break: nothing that could pose as a field or a line); exactly one
     * window line of that display carries that name; its owner is read from the fixed field run after the name (`OWNER`),
     * which must occur exactly once in the line. No structured source for the dispatcher's focus exists on Android 13 for
     * the shell: the dump is it, read this strictly.
     */
    static int focusedUid(String dump) {
        if (dump == null) return -1;
        String display = null, focused = null;
        int focusedEntries = 0;
        String[] lines = dump.split("\n", -1);
        for (int i = 0; i < lines.length; i++) {
            String l = lines[i].trim();
            if (l.startsWith("FocusedDisplayId:")) {
                if (display != null) return -1;
                display = l.substring(17).trim();
                if (!display.matches("[0-9]+")) return -1;
            }
            if (l.equals("FocusedWindows:")) {
                if (display == null) return -1;
                for (int j = i + 1; j < lines.length && lines[j].trim().startsWith("displayId="); j++) {
                    java.util.regex.Matcher m = FOCUSED_LINE.matcher(lines[j].trim());
                    if (!m.matches()) return -1;
                    if (m.group(1).equals(display)) { focused = m.group(2); focusedEntries++; }
                }
            }
        }
        if (focused == null || focusedEntries != 1 || !SAFE_NAME.matcher(focused).matches()) return -1;
        String head = "name='" + focused + "', id=";
        int uid = -1, found = 0;
        for (String raw : lines) {
            String l = raw.trim();
            int colon = l.indexOf(": ");
            if (colon <= 0 || !l.substring(0, colon).matches("[0-9]+") || !l.startsWith(head, colon + 2)) continue;
            String rest = l.substring(colon + 2 + head.length());
            if (!rest.matches("[0-9]+, displayId=" + display + ", .*")) continue;
            java.util.regex.Matcher m = OWNER.matcher(rest);
            if (!m.find()) return -1;
            int owner;
            try { owner = Integer.parseInt(m.group(1)); } catch (NumberFormatException e) { return -1; }
            if (m.find()) return -1;
            uid = owner;
            found++;
        }
        return found == 1 ? uid : -1;
    }

    /** The most of a dump we read (the RG556's is about 100 KB). */
    static final int DUMP_LIMIT = 4_000_000;

    /** All of `in` up to its end, or null when reading fails or it is longer than `limit` bytes: never a part of it. */
    static String readWhole(java.io.InputStream in, int limit) {
        try {
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[16384];
            for (int n; (n = in.read(buf)) >= 0; ) {
                bytes.write(buf, 0, n);
                if (bytes.size() > limit) return null;
            }
            return bytes.toString("UTF-8");
        } catch (IOException e) {
            return null;
        }
    }

    /** The input service's dump, read through a pipe (no `dumpsys` process): the dispatcher's focus. Null on failure. */
    static String inputDump() {
        try {
            Object binder = Class.forName("android.os.ServiceManager").getMethod("getService", String.class).invoke(null, "input");
            if (binder == null) return null;
            Class<?> os = Class.forName("android.system.Os");
            final java.io.FileDescriptor[] pipe = (java.io.FileDescriptor[]) os.getMethod("pipe").invoke(null);
            final StringBuilder out = new StringBuilder();
            // A dump cut short (a read error, the size limit) is no dump: its end could be missing the real lines.
            final boolean[] whole = {false};
            Thread reader = new Thread(() -> {
                try (java.io.FileInputStream in = new java.io.FileInputStream(pipe[0])) {
                    String all = readWhole(in, DUMP_LIMIT);
                    if (all != null) { out.append(all); whole[0] = true; }
                } catch (IOException ignored) { }
            }, "targetkey-dump");
            reader.start();
            try {
                Class.forName("android.os.IBinder").getMethod("dump", java.io.FileDescriptor.class, String[].class)
                    .invoke(binder, pipe[1], new String[0]);
            } finally {
                os.getMethod("close", java.io.FileDescriptor.class).invoke(null, pipe[1]);
            }
            reader.join(2000);
            return reader.isAlive() || !whole[0] ? null : out.toString();
        } catch (Exception e) {
            return null;
        }
    }

    /**
     * The real press through Android's input manager, found by reflection (this runs outside an app, so Android's hidden-API
     * rules don't apply). Null when this Android has no target uid.
     */
    static Injector android(final int uid, final int code) throws Exception {
        Class<?> event = Class.forName("android.view.InputEvent");
        final Class<?> key = Class.forName("android.view.KeyEvent");
        final Constructor<?> make = key.getConstructor(long.class, long.class, int.class, int.class, int.class, int.class,
            int.class, int.class, int.class, int.class);
        final Object im;
        final Method in;
        try {
            Object binder = Class.forName("android.os.ServiceManager").getMethod("getService", String.class).invoke(null, "input");
            Class<?> ibinder = Class.forName("android.os.IBinder");
            im = Class.forName("android.hardware.input.IInputManager$Stub").getMethod("asInterface", ibinder).invoke(null, binder);
            in = im.getClass().getMethod("injectInputEventToTarget", event, int.class, int.class);
        } catch (ReflectiveOperationException e) {
            return null;
        }
        final long[] down = {0};
        return action -> {
            long t = uptime();
            if (action == 0) down[0] = t;
            // KeyEvent(downTime, eventTime, action, code, repeat, meta, deviceId VIRTUAL_KEYBOARD, scancode, flags, source KEYBOARD)
            Object k = make.newInstance(down[0], t, action, code, 0, 0, -1, 0, 0, 0x101);
            return (Boolean) in.invoke(im, k, WAIT_FOR_RESULT, uid);
        };
    }

    /** `SystemClock.uptimeMillis()`, by reflection; on a plain JVM the JVM's own monotonic clock. */
    static long uptime() {
        try {
            return (Long) Class.forName("android.os.SystemClock").getMethod("uptimeMillis").invoke(null);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return System.nanoTime() / 1_000_000;
        }
    }

    /** Android's API level, by reflection; 0 when unknown (a plain JVM), so nothing is pressed. */
    static int sdk() {
        try {
            return Class.forName("android.os.Build$VERSION").getField("SDK_INT").getInt(null);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return 0;
        }
    }

    private static void sleepQuietly(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException ignored) { }
    }

    /** How long a save press watches for the emulator's new state file (Dolphin and ARMSX2 compress theirs first). */
    static final long SAVE_WAIT_MS = 4000;

    /**
     * The program line for Shizuku's process or `su` (`Extras.privileged`): this class from our APK `apk`, key `code` into
     * `uid` before `deadline` (uptime ms); a save (`save`) watches `dirs` for a new file named like `pattern`.
     */
    static String[] command(String apk, String nonce, String token, int uid, int code, long deadline, boolean save, String pattern, String[] dirs) {
        boolean look = save && dirs.length > 0 && !"-".equals(pattern);
        String[] head = {"/system/bin/app_process", "-Djava.class.path=" + apk, "/system/bin", TargetKey.class.getName(), nonce, token,
            Integer.toString(uid), Integer.toString(code), Long.toString(deadline), look ? Long.toString(SAVE_WAIT_MS) : "0", look ? pattern : "-"};
        String[] cmd = new String[head.length + (look ? dirs.length : 0)];
        System.arraycopy(head, 0, cmd, 0, head.length);
        if (look) System.arraycopy(dirs, 0, cmd, head.length, dirs.length);
        return cmd;
    }

    /**
     * What PadWatch shows for the program's output: "saved", "sent", "unsure" or "missed". Only the one status line with this
     * press's `nonce` counts, and only when the program ended with exit 0 (Shizuku's and `su`'s last line "exit=<n>"). No
     * Shizuku or root at all is "missed" (nothing ran); a timeout, another exit, a missing or doubled status line or an
     * unknown status is "unsure" (the key may have gone down before it).
     */
    static String outcome(String said, String nonce) {
        if (said == null) return "missed";
        String[] lines = said.trim().split("\n");
        String status = null;
        int count = 0;
        for (String line : lines) {
            String[] f = line.trim().split(" ");
            if (f.length >= 3 && f[0].equals(TAG) && f[1].equals(nonce)) {
                status = f[2];
                count++;
            }
        }
        if (count != 1 || !lines[lines.length - 1].trim().equals("exit=0")) return "unsure";
        switch (status) {
            case "saved": return "saved";
            case "sent": return "sent";
            case "missed": case "late": case "cancelled": case "unsupported": return "missed";
            default: return "unsure";
        }
    }
}
