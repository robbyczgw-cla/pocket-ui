package xyz.pocketui.app;

import android.accessibilityservice.AccessibilityService;
import java.io.File;
import android.content.Context;
import android.content.Intent;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.PixelFormat;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;
import android.view.accessibility.AccessibilityEvent;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): an accessibility service the player turns on (Settings → System → Android extras →
 * Pause menu in other emulators). While a game we started runs in another emulator app (`Extras.gameStarted`), HOME or
 * SELECT + START (`PadChord`) open our pause menu over it (`PadMenu`), drawn as an accessibility overlay (no "draw over other
 * apps" permission). With Shizuku or root on Android 13+ its Save and Load rows press the emulator's keys (`EmulatorKeys`),
 * for the emulator's window only (`TargetKey`: bound by Android on 14+, checked against the input focus on 13).
 *
 * Firmware decides what reaches a key filter: HOME may stay Android's on some handhelds (then SELECT + START still opens it);
 * a back gesture is no key at all. Outside our games the service lets every key through untouched.
 */
public class PadWatch extends AccessibilityService {
    private static final String TAG = "PocketExtras";
    /** Save, Load and Teach in the menu (`canSave`); false turns them off everywhere. */
    static final boolean PRESS_KEYS = true;
    /** Android 13: the first with a key press for one app (`TargetKey`: bound on 14+, focus-checked on 13). */
    static final int TARGETED_SDK = TargetKey.SDK;
    private final PadChord chord = new PadChord();
    private volatile PadMenu menu;
    private EmulatorKeys keys;
    private MenuView view;
    /** The newest menu action (a press's result is shown only while it is still the newest), and one press at a time. */
    private final java.util.concurrent.atomic.AtomicInteger action = new java.util.concurrent.atomic.AtomicInteger();
    /** Until when (uptime ms) the running press holds the slot; 0 = free. A hung Shizuku call frees it by time (Sol's recheck). */
    private final PressSlot slot = new PressSlot();
    /** How long after its planned moment a key may still go down (process start through Shizuku or su). */
    static final long PRESS_WINDOW_MS = 2500;
    /** Whether a delayed press may still go out (`PressGate`): every window in front and every menu moves it on. */
    private final PressGate gate = new PressGate();
    /** The app in front, from the window changes. */
    private volatile String front = "";
    private java.util.Map<String, String> texts = new java.util.HashMap<>();

    @Override public void onAccessibilityEvent(AccessibilityEvent e) {
        if (e == null || e.getEventType() != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED || e.getPackageName() == null) return;
        String pkg = e.getPackageName().toString();
        String cls = e.getClassName() == null ? "" : e.getClassName().toString();
        // An activity of ours (the shelf) came in front of the game: the menu goes with the game (the emulator test of
        // 26.11.2026: it stayed over the shelf). Our own overlay changes nothing.
        if (pkg.equals(getPackageName())) {
            if (cls.endsWith("Activity")) {
                if (gate.window(pkg)) stale();
                close();
            }
            return;
        }
        // Any other window in front (an app, Android's panels and keyboards) stops a press that was on its way, and ends its
        // program if it already runs (the focus check and Android's binding stop it too; this is the earliest).
        if (gate.window(pkg)) stale();
        // Android's bars, panels and keyboards don't change which app the pause menu is for.
        if (pkg.equals("com.android.systemui") || pkg.equals("android") || pkg.contains("inputmethod")) return;
        if (!pkg.equals(front)) {
            android.util.Log.i(TAG, "in front: " + pkg);
            front = pkg;
            close();
            chord.reset();
        }
    }

    @Override public void onInterrupt() {
        gate.move();
        close();
    }

    /** A game we started runs in the app in front. */
    private boolean armed() {
        return !front.isEmpty() && front.equals(Extras.playing(this)) && EmulatorKeys.watched(front);
    }

    @Override protected boolean onKeyEvent(KeyEvent k) {
        int code = k.getKeyCode(), action = k.getAction(), dev = k.getDeviceId();
        if (menu != null) {
            // The release of a key the emulator saw go down (SELECT of SELECT + START) goes on to it.
            if (!chord.menuKey(dev, code, action)) {
                takeFocus();
                return false;
            }
            if (action == KeyEvent.ACTION_DOWN && k.getRepeatCount() == 0) carryOut(menu.press(code));
            return true;
        }
        // Outside our games only the releases of keys we kept are ours.
        if (!armed() && action == KeyEvent.ACTION_DOWN) return false;
        int r = chord.key(dev, code, action, k.getRepeatCount());
        if (r == PadChord.OPEN) open();
        return r != PadChord.PASS;
    }

    private void open() {
        keys = EmulatorKeys.of(front, Extras.read(Extras.file(this, "keys.txt")));
        // Save, Load and Teach only where the press is bound to the emulator's window (Sol's review of 6aaab4b7c: `input
        // keyevent` went to whatever window had the focus a moment after PressGate's check, also our shelf).
        menu = new PadMenu(canSave());
        gate.move();
        action.incrementAndGet();
        // A press still on its way belongs to the moment before this menu (Sol's recheck: bind it to the menu).
        stale();
        texts = Extras.texts(this);
        try {
            view = new MenuView(this);
            WindowManager.LayoutParams lp = new WindowManager.LayoutParams(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                // Not focusable yet: see `takeFocus`.
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                PixelFormat.TRANSLUCENT);
            lp.gravity = Gravity.TOP | Gravity.START;
            if (android.os.Build.VERSION.SDK_INT >= 28) lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            ((WindowManager) getSystemService(WINDOW_SERVICE)).addView(view, lp);
            takeFocus();
            android.util.Log.i(TAG, "pause menu over " + front + (menu.rows.length > 3 ? " with keys" : ""));
        } catch (RuntimeException e) {
            android.util.Log.w(TAG, "pause menu: no overlay", e);
            view = null;
            menu = null;
        }
    }

    /**
     * The menu takes the focus: a pad's D-pad that is a hat (most built-in pads) comes as motion, never through the key filter
     * (emulator test 26.11.2026). Only once no key the emulator saw go down is still held: SELECT of SELECT + START would else
     * be released into our window and stay down in the emulator (Sol's review of d69eadf3e). A moment after the last such
     * release, so Android has handed it to the emulator's window first.
     */
    private void takeFocus() {
        final MenuView v = view;
        if (v == null || chord.heldByGame()) return;
        v.postDelayed(() -> {
            if (v != view || chord.heldByGame()) return;
            try {
                WindowManager.LayoutParams lp = (WindowManager.LayoutParams) v.getLayoutParams();
                lp.flags &= ~WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE;
                ((WindowManager) getSystemService(WINDOW_SERVICE)).updateViewLayout(v, lp);
                v.requestFocus();
            } catch (RuntimeException e) {
                android.util.Log.w(TAG, "pause menu: no focus", e);
            }
        }, 120);
    }

    /**
     * Every press on its way is stale: its token file goes at once (its program then presses nothing, also if it starts only
     * later), and its process is ended on a thread of its own (`destroy` is a binder call that may hang; Sol's recheck).
     */
    private void stale() {
        if (!slot.busy()) return;
        Extras.invalidatePresses(this);
        new Thread(Extras::cancelPress, "extras-cancel").start();
    }

    /** Shizuku or root, and Android 13+ (`TargetKey`); older Android gets Continue, the shelf and the emulator's menu. */
    private boolean canSave() {
        return PRESS_KEYS && android.os.Build.VERSION.SDK_INT >= TARGETED_SDK && !Extras.pressLocked(this) && Extras.canPress(this);
    }

    private void close() {
        if (view != null) {
            try { ((WindowManager) getSystemService(WINDOW_SERVICE)).removeView(view); } catch (RuntimeException ignored) { }
        }
        view = null;
        menu = null;
    }

    private void carryOut(int what) {
        if (what == PadMenu.NOTHING) { if (view != null) view.invalidate(); return; }
        final String pkg = front;
        final EmulatorKeys k = keys;
        close();
        // The moment of this press: anything in front of the emulator before it goes out stops it.
        final int mine = gate.now();
        // This action's number and its latest moment for the key to go down (uptime, the device's monotonic clock).
        final int gen = action.incrementAndGet();
        final long now = android.os.SystemClock.uptimeMillis();
        // The game start this press belongs to: a new start of the same app (the playing note written again) stops it.
        final String since = Extras.playingLaunch(this);
        switch (what) {
            case PadMenu.SAVE: later(250, () -> press(what, k.save, pkg, mine, gen, since, now + 250 + PRESS_WINDOW_MS)); break;
            case PadMenu.LOAD: later(250, () -> press(what, k.load, pkg, mine, gen, since, now + 250 + PRESS_WINDOW_MS)); break;
            case PadMenu.SHELF: shelf(); break;
            case PadMenu.EMULATOR_MENU: performGlobalAction(GLOBAL_ACTION_BACK); break;
            // The emulator's mapping page waits for a key: this is it.
            case PadMenu.TEACH_SAVE: later(1500, () -> press(what, k.save, pkg, mine, gen, since, now + 1500 + PRESS_WINDOW_MS)); break;
            case PadMenu.TEACH_LOAD: later(1500, () -> press(what, k.load, pkg, mine, gen, since, now + 1500 + PRESS_WINDOW_MS)); break;
            default: break;
        }
    }

    /**
     * `key` pressed in `pkg` for the menu's row `what`, only into `pkg`'s window (Sol's reviews 26./27.11.2026: a key meant for
     * the emulator must never land in the shelf or another app). PressGate first drops a press when anything came in front
     * since the menu closed; then `TargetKey` presses it as the shell or root: on Android 14+ bound to `pkg`'s uid by Android
     * itself, on Android 13 guarded by the input dispatcher's focus right before and after the down. `deadline` (uptime ms)
     * travels into the program and is checked right before the down; the press belongs to this menu action and this game
     * start (`action`, `Extras.playingLaunch`); one press at a time (`PressSlot`, taken over by time if Shizuku hangs), ended at once
     * when a new menu, window or game start comes (its token file goes, `stale`), and only the
     * newest action's result is shown (`action`). The line after it: "Saved" only when the emulator wrote a state file
     * (`EmulatorKeys.stateDirs`), "… key sent", "Not sent safely" when Android could not confirm where the key went, or
     * "Not saved" when nothing was pressed.
     */
    private void press(int what, String key, String pkg, int mine, int gen, String since, long deadline) {
        boolean save = what == PadMenu.SAVE, load = what == PadMenu.LOAD;
        String no = save ? "Not saved" : load ? "Not loaded" : "Key not sent";
        int code = EmulatorKeys.code(key);
        if (code == 0) { say(no); return; }
        long t = android.os.SystemClock.uptimeMillis();
        long budget = Math.max(0, deadline - t) + TargetKey.SAVE_WAIT_MS + 4000;
        // One press at a time; one whose time ran out (a hung Shizuku call) is taken over and ends with exactly its own token.
        final PressSlot.Job job = slot.take(t, t + budget + 2000);
        if (job == null) {
            android.util.Log.i(TAG, key + " not pressed: the last press is still running");
            say(no);
            return;
        }
        File token = null;
        try {
            if (menu != null || gen != action.get() || !since.equals(Extras.playingLaunch(this)) || !gate.mayPress(mine, pkg, Extras.playing(this))) {
                android.util.Log.i(TAG, key + " not pressed: something else came in front of " + pkg);
                say(no);
                return;
            }
            int uid = target(pkg);
            if (uid < 0) {
                android.util.Log.i(TAG, key + " not pressed: " + pkg + " is not one app with its own uid here");
                say(no);
                return;
            }
            String nonce = Long.toHexString(new java.security.SecureRandom().nextLong());
            // This press's token (`TargetKey` presses only while it exists); a stale() from here on removes it.
            token = Extras.newPressToken(this, nonce);
            if (token == null) {
                android.util.Log.i(TAG, key + " not pressed: no press token (shared storage)");
                say(no);
                return;
            }
            // Checked again with the token in place: anything that made the press stale before it existed (a menu, a window,
            // a game start) has moved one of these, and anything after it removes the token.
            if (!job.attach(token) || Extras.pressLocked(this) || menu != null || gen != action.get() || !since.equals(Extras.playingLaunch(this)) || !gate.mayPress(mine, pkg, Extras.playing(this))) {
                android.util.Log.i(TAG, key + " not pressed: it went stale while it was prepared");
                say(no);
                return;
            }
            String root = android.os.Environment.getExternalStorageDirectory().getPath();
            String out;
            try {
                out = Extras.privileged(this, TargetKey.command(getApplicationInfo().sourceDir, nonce, token.getPath(), uid, code, deadline, save,
                    EmulatorKeys.statePattern(pkg), EmulatorKeys.stateDirs(pkg, root)), budget, token);
            } catch (RuntimeException e) {
                android.util.Log.w(TAG, "press " + key, e);
                out = "exit=-1";
            }
            String r = TargetKey.outcome(out, nonce);
            android.util.Log.i(TAG, "pressed " + key + " in " + pkg + ": " + r + (out == null ? " (no Shizuku or root)" : " / " + out.trim().replace('\n', ' ')));
            if (gen != action.get()) {
                android.util.Log.i(TAG, "result of an older press, not shown");
                return;
            }
            if (r.equals("missed")) say(no);
            else if (r.equals("unsure")) say("Not sent safely");
            else say(save ? (r.equals("saved") ? "Saved" : "Save key sent") : load ? "Load key sent" : "Key sent");
        } finally {
            if (token != null && token.exists() && !token.delete()) android.util.Log.w(TAG, "press token stays");
            // Only this press's hold: one that timed out and was taken over by a newer press leaves that one alone.
            slot.release(job);
        }
    }

    /**
     * The uid a press for `pkg` may go to, or -1 (Sol's review of 82c82f0ba): an app's own uid in our user, not ours, not a
     * system uid, and not shared with another package (the dispatcher would take any window of that uid).
     */
    private int target(String pkg) {
        try {
            android.content.pm.PackageManager pm = getPackageManager();
            android.content.pm.PackageInfo info = pm.getPackageInfo(pkg, 0);
            int uid = info.applicationInfo.uid;
            String[] owners = pm.getPackagesForUid(uid);
            if (!TargetKey.appUid(uid) || uid == android.os.Process.myUid() || info.sharedUserId != null) return -1;
            if (owners == null || owners.length != 1 || !pkg.equals(owners[0])) return -1;
            return uid;
        } catch (Exception e) {
            return -1;
        }
    }

    /** The line `say` shows (main thread only). */
    private View line;

    /**
     * `english` translated, a moment at the bottom of the screen over the game. It never takes the focus or a touch: the
     * emulator keeps both (a focused window of ours would make the next press miss, `TargetKey`).
     */
    private void say(final String english) {
        new android.os.Handler(android.os.Looper.getMainLooper()).post(() -> {
            final WindowManager wm = (WindowManager) getSystemService(WINDOW_SERVICE);
            if (line != null) { try { wm.removeView(line); } catch (RuntimeException ignored) { } line = null; }
            final String text = tr(english);
            final float dp = getResources().getDisplayMetrics().density;
            final View v = new View(this) {
                private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
                @Override protected void onDraw(Canvas c) {
                    p.setTypeface(Typeface.create(Typeface.DEFAULT, Typeface.BOLD));
                    p.setTextSize(18 * dp);
                    float w = p.measureText(text) + 40 * dp, h = 44 * dp, x = (getWidth() - w) / 2, y = getHeight() - h - 28 * dp;
                    p.setColor(0xE815171C);
                    c.drawRoundRect(new RectF(x, y, x + w, y + h), h / 2, h / 2, p);
                    p.setColor(0xFFF1E8DA);
                    c.drawText(text, x + 20 * dp, y + h / 2 + 6 * dp, p);
                }
            };
            WindowManager.LayoutParams lp = new WindowManager.LayoutParams(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                PixelFormat.TRANSLUCENT);
            try {
                wm.addView(v, lp);
                line = v;
                android.util.Log.i(TAG, "said: " + english);
            } catch (RuntimeException e) {
                android.util.Log.w(TAG, "no line over the game", e);
                return;
            }
            v.postDelayed(() -> {
                if (line != v) return;
                try { wm.removeView(v); } catch (RuntimeException ignored) { }
                line = null;
            }, 2500);
        });
    }

    /** The handheld view in front again; the game keeps running behind it until the shelf counts it ended. */
    private void shelf() {
        try {
            startActivity(new Intent(this, HandheldActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT));
        } catch (RuntimeException e) {
            android.util.Log.w(TAG, "shelf", e);
            performGlobalAction(GLOBAL_ACTION_HOME);
        }
    }

    private static void later(long ms, Runnable r) {
        new Thread(() -> { sleep(ms); r.run(); }, "extras-menu").start();
    }

    private static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException ignored) { }
    }

    private String tr(String english) {
        String t = texts.get(english);
        return t == null || t.isEmpty() ? english : t;
    }

    /** The menu: a dark panel over the dimmed game, the rows in the middle, the keys under them. Touch works too. */
    private final class MenuView extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final float dp;
        private final RectF[] hits = new RectF[8];

        MenuView(Context c) {
            super(c);
            dp = c.getResources().getDisplayMetrics().density;
            paint.setTypeface(Typeface.create(Typeface.DEFAULT, Typeface.BOLD));
            setFocusable(true);
            setFocusableInTouchMode(true);
        }

        @Override protected void onDraw(Canvas c) {
            PadMenu m = menu;
            if (m == null) return;
            c.drawColor(0xB0000000);
            float pad = 20 * dp, w = Math.min(getWidth() - 2 * pad, 460 * dp);
            // Rows shrink to fit a short screen (seven rows ran off a 1080-line screen at 48 dp); the type with them.
            float rowH = Math.max(30 * dp, Math.min(48 * dp, (getHeight() - pad * 4 - 80 * dp) / m.rows.length));
            float h = pad * 2 + 44 * dp + m.rows.length * rowH + 36 * dp;
            float x = (getWidth() - w) / 2, y = Math.max(pad, (getHeight() - h) / 2);
            paint.setColor(0xFF15171C);
            c.drawRoundRect(new RectF(x, y, x + w, y + h), 18 * dp, 18 * dp, paint);
            paint.setTextSize(15 * dp);
            paint.setColor(0xFF9A968E);
            c.drawText(title(), x + pad, y + pad + 16 * dp, paint);
            float top = y + pad + 44 * dp;
            paint.setTextSize(Math.min(20 * dp, rowH * 0.42f));
            for (int i = 0; i < m.rows.length; i++) {
                float ry = top + i * rowH;
                hits[i] = new RectF(x + pad / 2, ry, x + w - pad / 2, ry + rowH);
                if (i == m.sel) {
                    paint.setColor(0xFFF1E8DA);
                    c.drawRoundRect(hits[i], 12 * dp, 12 * dp, paint);
                }
                paint.setColor(i == m.sel ? 0xFF1A1A1F : 0xFFE9E4DA);
                c.drawText(fit(tr(PadMenu.label(m.rows[i])), w - 2 * pad), x + pad, ry + rowH / 2 + 7 * dp, paint);
            }
            paint.setTextSize(14 * dp);
            paint.setColor(0xFF9A968E);
            c.drawText(fit(tr("A Select") + "    " + tr("B Back to the game"), w - 2 * pad), x + pad, y + h - pad, paint);
        }

        private String title() {
            String name = front;
            try {
                android.content.pm.PackageManager pm = getPackageManager();
                name = String.valueOf(pm.getApplicationLabel(pm.getApplicationInfo(front, 0)));
            } catch (Exception ignored) { }
            return fit(tr("pocket-ui pause menu") + " · " + name, 420 * dp);
        }

        private String fit(String s, float room) {
            if (paint.measureText(s) <= room) return s;
            while (s.length() > 1 && paint.measureText(s + "…") > room) s = s.substring(0, s.length() - 1);
            return s + "…";
        }

        /** The D-pad as a hat (AXIS_HAT_Y) or the left stick: one step per push, as the key filter's Up and Down. */
        private int held;

        @Override public boolean onGenericMotionEvent(MotionEvent e) {
            if (menu == null || e.getAction() != MotionEvent.ACTION_MOVE) return super.onGenericMotionEvent(e);
            float y = e.getAxisValue(MotionEvent.AXIS_HAT_Y);
            if (Math.abs(y) < 0.5f) y = e.getAxisValue(MotionEvent.AXIS_Y);
            int now = y > 0.5f ? PadChord.DPAD_DOWN : y < -0.5f ? PadChord.DPAD_UP : 0;
            if (now != 0 && now != held) carryOut(menu.press(now));
            held = now;
            return true;
        }

        @Override public boolean onTouchEvent(MotionEvent e) {
            if (e.getAction() != MotionEvent.ACTION_UP || menu == null) return true;
            for (int i = 0; i < menu.rows.length; i++) {
                if (hits[i] != null && hits[i].contains(e.getX(), e.getY())) {
                    menu.sel = i;
                    carryOut(menu.row());
                    return true;
                }
            }
            carryOut(PadMenu.CLOSE);
            return true;
        }
    }
}
