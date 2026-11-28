package xyz.pocketui.app;

/** Android extras on a plain JVM (scripts/android/test/java-reader-rule.sh): PadChord, PadMenu and EmulatorKeys. */
public final class ExtrasKeysTest {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        int D = PadChord.ACTION_DOWN, U = PadChord.ACTION_UP, P1 = 7, P2 = 9;

        // HOME and the pad's HOME/MODE open the menu; the emulator never sees either half.
        for (int home : new int[]{PadChord.HOME, PadChord.MODE}) {
            PadChord c = new PadChord();
            check(c.key(P1, home, D, 0) == PadChord.OPEN, "HOME press opens: " + home);
            check(c.key(P1, home, D, 1) == PadChord.KEEP, "a held HOME repeats, kept: " + home);
            check(c.key(P1, home, U, 0) == PadChord.KEEP, "HOME release stays ours: " + home);
            check(c.key(P1, home, U, 0) == PadChord.PASS, "a second release is not ours: " + home);
        }

        // SELECT + START: SELECT goes on, START opens and both its halves are ours; START alone is the game's.
        PadChord c = new PadChord();
        check(c.key(P1, PadChord.START, D, 0) == PadChord.PASS, "START alone plays");
        check(c.key(P1, PadChord.START, U, 0) == PadChord.PASS, "START alone plays (release)");
        check(c.key(P1, PadChord.SELECT, D, 0) == PadChord.PASS, "SELECT reaches the game");
        check(c.key(P2, PadChord.START, D, 0) == PadChord.PASS, "another pad's START with this pad's SELECT is the game's");
        check(c.key(P2, PadChord.START, U, 0) == PadChord.PASS, "(its release)");
        check(c.key(P1, PadChord.START, D, 0) == PadChord.OPEN, "SELECT held, START opens");
        // The menu is open now: SELECT's release belongs to the emulator, which saw it go down (Sol's review: it stayed held).
        check(!c.menuKey(P1, PadChord.SELECT, U), "SELECT's release goes on to the game while the menu is open");
        check(c.menuKey(P1, PadChord.START, U), "START's release is the menu's");
        check(c.menuKey(P1, PadChord.BUTTON_A, D), "A is the menu's");
        check(c.key(P1, PadChord.BUTTON_A, U, 0) == PadChord.KEEP, "A that closed the menu: its release is ours");
        check(c.key(P1, PadChord.BUTTON_A, D, 0) == PadChord.PASS, "the next A is the game's");
        check(c.key(P1, PadChord.START, D, 0) == PadChord.PASS, "after SELECT went, START is the game's again");
        check(c.key(P1, PadChord.START, U, 0) == PadChord.PASS, "(its release)");
        // A key held into the menu: its repeats stay with the menu, its release goes to the game.
        c = new PadChord();
        check(c.key(P1, PadChord.DPAD_DOWN, D, 0) == PadChord.PASS, "the game's D-pad");
        c.key(P1, PadChord.HOME, D, 0);
        check(c.menuKey(P1, PadChord.DPAD_DOWN, D), "a repeat of the held key stays out of the game");
        check(!c.menuKey(P1, PadChord.DPAD_DOWN, U), "its release reaches the game");
        c.reset();
        check(c.key(P1, PadChord.HOME, U, 0) == PadChord.PASS, "reset forgets the held keys");

        // The menu takes the focus only once nothing the emulator saw go down is held (SELECT's release reaches its window).
        c = new PadChord();
        c.key(P1, PadChord.SELECT, D, 0);
        check(c.heldByGame(), "SELECT down in the game");
        check(c.key(P1, PadChord.START, D, 0) == PadChord.OPEN, "SELECT + START opens");
        check(c.heldByGame(), "the menu waits with the focus while SELECT is down");
        check(c.menuKey(P1, PadChord.START, U), "START's release is the menu's");
        check(c.heldByGame(), "still waiting: SELECT is down");
        check(!c.menuKey(P1, PadChord.SELECT, U), "SELECT's release goes to the game");
        check(!c.heldByGame(), "now the menu may take the focus");
        c = new PadChord();
        c.key(P1, PadChord.MODE, D, 0);
        check(!c.heldByGame(), "HOME/MODE: the focus at once");

        // A delayed press goes out only into the same game in the same moment (Sol's review of d69eadf3e).
        PressGate g = new PressGate();
        g.window("org.ppsspp.ppsspp");
        int t = g.now();
        check(g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "nothing changed: the press goes out");
        g.window("org.ppsspp.ppsspp");
        check(g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "the emulator's own windows (its mapping page) don't stop it");
        check(!g.mayPress(t, "org.ppsspp.ppsspp", ""), "the game ended (the shelf came back): no press");
        check(!g.mayPress(t, "org.ppsspp.ppsspp", "org.dolphinemu.dolphinemu"), "another game: no press");
        g.window("xyz.pocketui.app");
        check(!g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "our shelf in front: no press");
        g.window("org.ppsspp.ppsspp");
        check(!g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "away and back (A, B, A): the old press stays dropped");
        t = g.now();
        g.window("com.android.systemui");
        check(!g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "Android's panel in front: no press");
        g.window("org.ppsspp.ppsspp");
        t = g.now();
        g.move();
        check(!g.mayPress(t, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "a new menu opened meanwhile: no press");
        check(!new PressGate().mayPress(0, "org.ppsspp.ppsspp", "org.ppsspp.ppsspp"), "no window seen yet: no press");

        // The menu: without key presses only what Android can do; with them save, load and the Teach rows. No Save and quit.
        PadMenu m = new PadMenu(false);
        check(java.util.Arrays.equals(m.rows, new int[]{PadMenu.CONTINUE, PadMenu.SHELF, PadMenu.EMULATOR_MENU}), "no keys: three rows");
        m = new PadMenu(true);
        check(contains(m.rows, PadMenu.SAVE) && contains(m.rows, PadMenu.LOAD) && contains(m.rows, PadMenu.TEACH_SAVE), "keys: save, load, teach");
        check(m.press(PadChord.DPAD_UP) == PadMenu.NOTHING && m.row() == PadMenu.TEACH_LOAD, "Up from the top wraps");
        check(m.press(PadChord.DPAD_DOWN) == PadMenu.NOTHING && m.row() == PadMenu.CONTINUE, "Down from the end wraps");
        m.press(PadChord.DPAD_DOWN);
        check(m.press(PadChord.BUTTON_A) == PadMenu.SAVE, "A carries out Save state");
        for (int close : new int[]{PadChord.BUTTON_B, PadChord.BACK, PadChord.HOME, PadChord.MODE, PadChord.START})
            check(m.press(close) == PadMenu.CLOSE, "closes: " + close);
        for (int row : m.rows) check(!PadMenu.label(row).isEmpty(), "every row has a line: " + row);

        // Keys: F2/F4 for all, keys.txt changes them.
        EmulatorKeys k = EmulatorKeys.of("org.ppsspp.ppsspp", null);
        check(k.save.equals("F2") && k.load.equals("F4"), "F2/F4");
        check(EmulatorKeys.code("F2") == 132 && EmulatorKeys.code("F4") == 134 && EmulatorKeys.code("F12") == 142, "F-key codes");
        check(EmulatorKeys.code("F13") == 0 && EmulatorKeys.code("x") == 0 && EmulatorKeys.code("-") == 0, "no other keys");
        k = EmulatorKeys.of("org.dolphinemu.dolphinemu", "org.dolphinemu.dolphinemu F5 F7\n");
        check(k.save.equals("F5") && k.load.equals("F7"), "keys.txt names other keys");
        check(EmulatorKeys.of("org.dolphinemu.dolphinemu", "org.dolphinemu.dolphinemu F99 F4\n").save.equals("F2"), "a bad line is left out");
        check(EmulatorKeys.watched("org.ppsspp.ppsspp") && !EmulatorKeys.watched("com.android.chrome"), "only emulator apps");

        targetKey();
        launchNoteAndSlot();

        // A key meant for an emulator that lands in pocket-ui's own window instead does nothing there (lead, 27.11.2026).
        for (int code = EmulatorKeys.KEYCODE_F1; code <= EmulatorKeys.KEYCODE_F1 + 11; code++) {
            check(EmulatorKeys.strayEmulatorKey(code, -1), "an injected F" + (code - EmulatorKeys.KEYCODE_F1 + 1) + " is dropped");
            check(!EmulatorKeys.strayEmulatorKey(code, 4), "a real keyboard's F-key still works");
        }
        check(!EmulatorKeys.strayEmulatorKey(PadChord.BUTTON_A, -1) && !EmulatorKeys.strayEmulatorKey(143, -1) && !EmulatorKeys.strayEmulatorKey(130, -1), "only F1 to F12");
        check(EmulatorKeys.strayEmulatorKey(EmulatorKeys.code(EmulatorKeys.of("org.ppsspp.ppsspp", null).save), -1), "the save key we press is one of them");
        System.out.println("PASS: extras pad chord, pause menu rows, emulator keys, key presses bound to the emulator");
    }

    /** A clock that moves only when the code under test sleeps, and can write a file at a moment. */
    private static final class FakeClock implements TargetKey.Clock {
        long t = 1_000_000;
        Runnable at;
        long when = Long.MAX_VALUE;
        @Override public long now() { return t; }
        @Override public void sleep(long ms) {
            t += ms;
            if (at != null && t >= when) { Runnable r = at; at = null; r.run(); }
        }
    }

    /** Writes `name` in `dir` with `bytes` bytes and the modification time `at` (ms). */
    private static void file(java.io.File dir, String name, int bytes, long at) {
        try {
            java.io.File f = new java.io.File(dir, name);
            java.nio.file.Files.write(f.toPath(), new byte[bytes]);
            check(f.setLastModified(at), "time of " + name);
        } catch (java.io.IOException e) { throw new RuntimeException(e); }
    }

    /**
     * An Android 13 input service as AOSP has it (b/228161340): a targeted key whose target is not in focus is delivered anyway,
     * to whatever window has the focus ("Falling back to injecting into all windows"). Records who got each key.
     */
    private static final class Android13 implements TargetKey.Injector, TargetKey.Focus {
        int focused;
        int switchTo = -1;
        final java.util.List<Integer> got = new java.util.ArrayList<>();
        Android13(int focused) { this.focused = focused; }
        @Override public boolean inject(int action) {
            got.add(focused);
            if (switchTo >= 0) { focused = switchTo; switchTo = -1; }
            return true;
        }
        @Override public int uid() { return focused; }
    }

    /**
     * TargetKey (Sol's reviews of 6aaab4b7c and 82c82f0ba): on Android 14+ Android binds the key to the emulator's uid; on 13
     * its fallback would deliver to any window, so the dispatcher's focus is checked before and after the down; "Saved" only
     * for a state file that is new or changed since just before the down and stopped growing.
     */
    private static void targetKey() {
        final int EMU = 10208, SHELF = 10210, SYSTEM_UI = 10107;
        String[] none = new String[0];
        try {
            // Android 13: another window in front -> the fallback would hand it the key, so nothing is injected at all.
            Android13 a13 = new Android13(SHELF);
            String r = TargetKey.run(a13, a13, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("missed") && a13.got.isEmpty(), "Android 13, shelf in front: never injected: " + r + " " + a13.got);
            a13 = new Android13(SYSTEM_UI);
            check(TargetKey.run(a13, a13, EMU, new FakeClock(), 2000, 0, "-", none).equals("missed") && a13.got.isEmpty(), "13: shade in front");
            a13 = new Android13(-1);
            check(TargetKey.run(a13, a13, EMU, new FakeClock(), 2000, 0, "-", none).equals("missed") && a13.got.isEmpty(), "13: focus unclear");
            // The emulator in front: down and up both reach it.
            a13 = new Android13(EMU);
            r = TargetKey.run(a13, a13, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("sent") && a13.got.equals(java.util.Arrays.asList(EMU, EMU)), "13, emulator in front: " + r + " " + a13.got);
            // The focus moves away just as the key goes down: "unsure" (the fallback may have sent it on), and no up after it.
            a13 = new Android13(EMU);
            a13.switchTo = SHELF;
            r = TargetKey.run(a13, a13, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("unsure") && a13.got.size() == 1, "13, focus moved during the down: " + r + " " + a13.got);

            // Android 14+: Android binds it; a mismatch throws (directly or wrapped by reflection) -> missed, no up.
            java.util.List<Integer> sent = new java.util.ArrayList<>();
            r = TargetKey.run(a -> { sent.add(a); throw new IllegalArgumentException("not directed at a window owned by uid"); }, null, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("missed") && sent.equals(java.util.Arrays.asList(0)), "14: mismatch: " + r + " " + sent);
            r = TargetKey.run(a -> { throw new java.lang.reflect.InvocationTargetException(new IllegalArgumentException("mismatch")); }, null, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("missed"), "14: mismatch through reflection: " + r);
            // false is a timeout or failure inside Android, not proof that nothing arrived.
            sent.clear();
            r = TargetKey.run(a -> { sent.add(a); return false; }, null, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("unsure") && sent.size() == 1, "14: down false is unsure: " + r);
            r = TargetKey.run(a -> { if (a == 1) throw new IllegalStateException("up"); return true; }, null, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("sent up-dropped"), "a refused up is told apart: " + r);
            // The focus look itself took past the deadline (a slow dump): still no down (Sol's recheck).
            final FakeClock slow = new FakeClock();
            sent.clear();
            r = TargetKey.run(a -> { sent.add(a); return true; }, () -> { slow.t += 3000; return EMU; }, EMU, slow, 2000, 0, "-", none);
            check(r.equals("late") && sent.isEmpty(), "a focus look past the deadline: " + r);
            // Documented rest risk on Android 13 (lead, 27.11.2026; ANDROID-EXTRAS.md): the focus goes A -> B -> A between the
            // looks, Android's fallback hands the down to B, and both looks saw A. The program cannot tell; pocket-ui's own
            // windows drop such a key (strayEmulatorKey), other apps may not. Pinned here so a change of it is seen.
            final int[] looks = {0};
            final java.util.List<Integer> to = new java.util.ArrayList<>();
            r = TargetKey.run(a -> { to.add(a == 0 ? SHELF : EMU); return true; }, () -> { looks[0]++; return EMU; }, EMU, new FakeClock(), 2000, 0, "-", none);
            check(r.equals("sent") && to.get(0) == SHELF && looks[0] >= 2, "A->B->A on Android 13 is not seen: " + r + " " + to);
            // The press went stale (a new menu, window or game start removed its token): nothing is injected, also when that
            // happens only during the focus look right before the down (Sol's recheck of f7df20bc8).
            sent.clear();
            r = TargetKey.run(a -> { sent.add(a); return true; }, null, () -> false, EMU, new FakeClock(), 2000, 0, null);
            check(r.equals("cancelled") && sent.isEmpty(), "a removed token: " + r);
            final boolean[] wanted = {true};
            r = TargetKey.run(a -> { sent.add(a); return true; }, () -> { wanted[0] = false; return EMU; }, () -> wanted[0], EMU, new FakeClock(), 2000, 0, null);
            check(r.equals("cancelled") && sent.isEmpty(), "a token removed during the focus look: " + r);
            check(TargetKey.outcome("pocket-targetkey a1 cancelled\nexit=0", "a1").equals("missed"), "cancelled = nothing pressed");
            // A slow token look that ends past the deadline: still no down (Sol's recheck of 5a47a70b7).
            final FakeClock slowToken = new FakeClock();
            sent.clear();
            r = TargetKey.run(a -> { sent.add(a); return true; }, null, () -> { slowToken.t += 3000; return true; }, EMU, slowToken, 2000, 0, null);
            check(r.equals("late") && sent.isEmpty(), "a token look past the deadline: " + r);

            // "Saved" counts complete scans in a row; an incomplete one starts over (deterministic, no file system).
            java.util.Map<String, long[]> none0 = new java.util.HashMap<>(), hit = new java.util.HashMap<>();
            hit.put("/s/A.ppst", new long[]{5000, 7});
            final TargetKey.Snapshot empty = new TargetKey.Snapshot(none0, true), seen = new TargetKey.Snapshot(hit, true),
                gapScan = new TargetKey.Snapshot(new java.util.HashMap<>(hit), false);
            final java.util.List<TargetKey.Snapshot> plan = new java.util.ArrayList<>(java.util.Arrays.asList(empty, seen, seen, gapScan, seen, seen, seen));
            final int[] scans = {0};
            TargetKey.Scan scripted = () -> { scans[0]++; return plan.size() > 1 ? plan.remove(0) : plan.get(0); };
            r = TargetKey.run(a -> true, null, () -> true, EMU, new FakeClock(), 2000, 4000, scripted);
            check(r.equals("saved") && scans[0] == 7, "two looks, a gap, then three looks in a row: " + r + " after " + scans[0]);
            plan.clear();
            plan.addAll(java.util.Arrays.asList(empty, seen, seen, gapScan, seen, seen, gapScan));
            scans[0] = 0;
            r = TargetKey.run(a -> true, null, () -> true, EMU, new FakeClock(), 2000, 1500, scripted);
            check(r.equals("sent"), "never three complete looks in a row: " + r);
            plan.clear();
            plan.addAll(java.util.Arrays.asList(new TargetKey.Snapshot(none0, false), seen));
            check(TargetKey.run(a -> true, null, () -> true, EMU, new FakeClock(), 2000, 1500, scripted).equals("sent snapshot-incomplete"), "an incomplete first snapshot");

            // Too late (the deadline passed while Shizuku started the program): nothing is injected.
            sent.clear();
            r = TargetKey.run(a -> { sent.add(a); return true; }, null, EMU, new FakeClock(), -1, 0, "-", none);
            check(r.equals("late") && sent.isEmpty(), "late: " + r);

            java.io.File dir = java.nio.file.Files.createTempDirectory("extras-states").toFile();
            try {
                String[] dirs = {dir.getPath(), new java.io.File(dir, "missing").getPath()};
                String pattern = EmulatorKeys.statePattern("org.ppsspp.ppsspp");
                file(dir, "ULUS10041_1.00_0.ppst", 10, 500_000);
                // A new state written 1.2 s after the down: saved once it stopped growing.
                FakeClock clock = new FakeClock();
                clock.when = clock.t + 1200;
                clock.at = () -> file(dir, "ULUS10041_1.00_1.ppst", 7, 900_000);
                r = TargetKey.run(a -> true, null, EMU, clock, 2000, 4000, pattern, dirs);
                check(r.equals("saved"), "a new state file is the proof: " + r);
                // The slot written again (same size, new time): saved too.
                clock = new FakeClock();
                clock.when = clock.t + 600;
                clock.at = () -> file(dir, "ULUS10041_1.00_1.ppst", 7, 950_000);
                check(TargetKey.run(a -> true, null, EMU, clock, 2000, 4000, pattern, dirs).equals("saved"), "an overwritten slot");
                // No new or changed state (the key not taught): only "sent", after the whole wait.
                clock = new FakeClock();
                r = TargetKey.run(a -> true, null, EMU, clock, 2000, 4000, pattern, dirs);
                check(r.equals("sent") && clock.t - 1_000_000 >= 4000, "no new file: sent: " + r);
                // Another kind of file is no proof.
                clock = new FakeClock();
                clock.when = clock.t + 300;
                clock.at = () -> file(dir, "ULUS10041_1.00_0.ppst.tmp", 3, 990_000);
                check(TargetKey.run(a -> true, null, EMU, clock, 2000, 2000, pattern, dirs).equals("sent"), "another name: sent");
                // A file still growing at every look is not finished: not "saved".
                clock = new FakeClock();
                final FakeClock grow = clock;
                final int[] size = {1};
                clock.when = clock.t + 100;
                clock.at = new Runnable() { @Override public void run() {
                    file(dir, "ULUS10041_1.00_2.ppst", size[0]++, 1_000_000 + size[0]);
                    grow.when = grow.t + 100;
                    grow.at = this;
                } };
                check(TargetKey.run(a -> true, null, EMU, clock, 2000, 1000, pattern, dirs).equals("sent"), "a growing file is no proof");
                // The down missed: no save looked for.
                check(TargetKey.run(a -> { throw new IllegalArgumentException("mismatch"); }, null, EMU, new FakeClock(), 2000, 4000, pattern, dirs).equals("missed"), "missed: no look");
                // A snapshot that could not see everything (a folder deeper than DEPTH): never "saved".
                java.io.File deep = dir;
                for (int i = 0; i < TargetKey.DEPTH + 1; i++) { deep = new java.io.File(deep, "d" + i); check(deep.mkdir(), "deep"); }
                clock = new FakeClock();
                clock.when = clock.t + 300;
                clock.at = () -> file(dir, "ULUS10041_1.00_3.ppst", 9, 1_100_000);
                r = TargetKey.run(a -> true, null, EMU, clock, 2000, 2000, pattern, dirs);
                check(r.equals("sent snapshot-incomplete"), "incomplete snapshot: " + r);
                java.nio.file.Files.walk(new java.io.File(dir, "d0").toPath()).sorted(java.util.Comparator.reverseOrder()).forEach(q -> q.toFile().delete());
                // A folder that is there but unreadable is not "missing": the snapshot is incomplete, so never "saved".
                java.io.File locked = new java.io.File(dir, "locked");
                check(locked.mkdir() && locked.setReadable(false, false) && locked.setExecutable(false, false), "a locked folder");
                if (!locked.canRead()) {
                    clock = new FakeClock();
                    clock.when = clock.t + 300;
                    clock.at = () -> file(dir, "ULUS10041_1.00_4.ppst", 9, 1_200_000);
                    r = TargetKey.run(a -> true, null, EMU, clock, 2000, 2000, pattern, new String[]{dir.getPath(), locked.getPath()});
                    check(r.equals("sent snapshot-incomplete"), "an unreadable folder: " + r);
                }
                locked.setReadable(true, false);
                locked.setExecutable(true, false);
            } finally {
                java.nio.file.Files.walk(dir.toPath()).sorted(java.util.Comparator.reverseOrder()).forEach(p -> p.toFile().delete());
            }
        } catch (Exception e) {
            throw new AssertionError("targetKey: " + e, e);
        }

        // The status line: only this press's nonce, only with exit 0, exactly once.
        String nc = "a1b2";
        check(TargetKey.outcome(null, nc).equals("missed"), "no Shizuku or root: nothing ran");
        check(TargetKey.outcome("pocket-targetkey a1b2 saved\nexit=0", nc).equals("saved"), "saved");
        check(TargetKey.outcome("W/InputManager: noise\npocket-targetkey a1b2 sent up-dropped\nexit=0", nc).equals("sent"), "sent among noise");
        check(TargetKey.outcome("pocket-targetkey a1b2 late\nexit=0", nc).equals("missed"), "late");
        check(TargetKey.outcome("pocket-targetkey a1b2 unsupported\nexit=0", nc).equals("missed"), "unsupported");
        check(TargetKey.outcome("pocket-targetkey a1b2 missed\nexit=0", nc).equals("missed"), "missed");
        check(TargetKey.outcome("pocket-targetkey a1b2 unsure\nexit=0", nc).equals("unsure"), "unsure");
        check(TargetKey.outcome("pocket-targetkey ffff saved\nexit=0", nc).equals("unsure"), "another nonce (stderr, an old run) counts for nothing");
        check(TargetKey.outcome("pocket-targetkey a1b2 saved\nexit=-1", nc).equals("unsure"), "a timeout: unsure");
        check(TargetKey.outcome("pocket-targetkey a1b2 saved\nexit=3", nc).equals("unsure"), "another exit: unsure");
        check(TargetKey.outcome("pocket-targetkey a1b2 saved\npocket-targetkey a1b2 sent\nexit=0", nc).equals("unsure"), "two lines: unsure");
        check(TargetKey.outcome("pocket-targetkey a1b2 savedx\nexit=0", nc).equals("unsure"), "an unknown status: unsure");
        check(TargetKey.outcome("pocket-targetkey a1b2 error IllegalStateException\nexit=0", nc).equals("unsure"), "an error: unsure");
        check(TargetKey.outcome("exit=-1 android.os.DeadObjectException", nc).equals("unsure"), "Shizuku died: unsure");

        // The dispatcher's focus from its dump (format of the RG556, Android 13, 27.11.2026), read strictly (Sol's recheck of
        // 058919942: the window name is the app's title, printed unescaped before the real fields).
        String tail = ", inputConfig=0x0, alpha=1.00, frame=[0,0][1920,1080], globalScale=1.000000, applicationInfo.name=ActivityRecord{1 u0 x/.Y}, applicationInfo.token=<null>, touchableRegion=[0,0][1920,1080], ownerPid=%d, ownerUid=%d, dispatchingTimeout=5000ms, hasToken=true, touchOcclusionMode=BLOCK_UNTRUSTED\n";
        String emu = "814ed11 org.ppsspp.ppsspp/org.ppsspp.ppsspp.PpssppActivity";
        java.util.function.BiFunction<String, String, String> dumpOf = (focused, windows) ->
            "  FocusedDisplayId: 0\n  FocusedApplications:\n    displayId=0, name='ActivityRecord{4524669 u0 org.ppsspp.ppsspp/.PpssppActivity} t73 d0}', dispatchingTimeout=5000ms\n"
            + "  FocusedWindows:\n" + (focused == null ? "" : "    displayId=0, name='" + focused + "'\n") + "  FocusRequests:\n"
            + "  Display: 0\n    Windows:\n"
            + "      1: name='74cca76 NavigationBar0', id=952, displayId=0" + String.format(tail, 1842, 10107) + windows;
        String dump = dumpOf.apply(emu, "      5: name='" + emu + "', id=7001, displayId=0" + String.format(tail, 17848, 10151));
        check(TargetKey.focusedUid(dump) == 10151, "the focused window's owner: " + TargetKey.focusedUid(dump));
        check(TargetKey.focusedUid(dumpOf.apply(null, "")) == -1, "no focused window");
        check(TargetKey.focusedUid(dump + "      6: name='" + emu + "', id=7002, displayId=0" + String.format(tail, 1, 10999)) == -1, "two windows of that name: unclear");
        check(TargetKey.focusedUid(dump.replace("FocusedDisplayId: 0", "FocusedDisplayId: 2")) == -1, "another display in focus");
        check(TargetKey.focusedUid(dump.replace("ownerUid=10151", "owner=10151")) == -1, "no owner in the line");
        check(TargetKey.focusedUid("") == -1 && TargetKey.focusedUid(null) == -1, "no dump");
        // A dump read only in part is no dump (Sol's recheck): a read error after a good start, or more than the limit.
        byte[] good = dump.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        check(dump.equals(TargetKey.readWhole(new java.io.ByteArrayInputStream(good), TargetKey.DUMP_LIMIT)), "a whole dump is read");
        java.io.InputStream broken = new java.io.InputStream() {
            int at;
            @Override public int read() throws java.io.IOException {
                if (at >= good.length / 2) throw new java.io.IOException("pipe broke");
                return good[at++] & 0xff;
            }
        };
        check(TargetKey.readWhole(broken, TargetKey.DUMP_LIMIT) == null, "a read error after a good start: no dump");
        check(TargetKey.readWhole(new java.io.ByteArrayInputStream(good), good.length - 1) == null, "longer than the limit: no dump");
        check(TargetKey.focusedUid(TargetKey.readWhole(broken, TargetKey.DUMP_LIMIT)) == -1, "and so no uid");
        // A title that poses as the fields or as the emulator: never the emulator's uid, and unclear means -1 (no press).
        for (String title : new String[]{
            "ab12 Evil, ownerPid=1, ownerUid=10151, dispatchingTimeout=5000ms, x",
            "ab12 ownerUid=10151",
            "ab12 it's",
            "ab12 Evil', id=1, displayId=0, ownerPid=1, ownerUid=10151, dispatchingTimeout=5000ms, '",
            "ab12 two\n      9: name='x', id=1, displayId=0, ownerPid=1, ownerUid=10151, dispatchingTimeout=5000ms, hasToken=true",
        }) {
            String d = dumpOf.apply(title, "      7: name='" + title + "', id=7003, displayId=0" + String.format(tail, 666, 10666));
            check(TargetKey.focusedUid(d) == -1, "a title that could fake a field gives no uid: " + title.replace('\n', '|'));
        }
        // A plain title of another app that names the emulator: its own (real) owner, never the emulator's.
        String fake = "ab12 org.ppsspp.ppsspp/org.ppsspp.ppsspp.PpssppActivity";
        check(TargetKey.focusedUid(dumpOf.apply(fake, "      7: name='" + fake + "', id=7003, displayId=0" + String.format(tail, 666, 10666))) == 10666, "the real owner of a look-alike");

        // Who may be a target: an app's own uid of any user, never root, system, shell or "no uid".
        check(TargetKey.appUid(10123) && TargetKey.appUid(1010123), "apps (user 0 and 10)");
        for (int bad : new int[]{-1, 0, 1000, 2000, 1001000, 9999}) check(!TargetKey.appUid(bad), "not an app: " + bad);
        check(TargetKey.SDK == 33 && TargetKey.ATOMIC_SDK == 34, "13 with the focus check, 14 bound by Android");
        // sdk() reads Android's level by reflection: 0 without Android; the gate's JVM run has a stand-in android.os.Build.
        int level;
        try { level = Class.forName("android.os.Build$VERSION").getField("SDK_INT").getInt(null); } catch (ReflectiveOperationException e) { level = 0; }
        check(TargetKey.sdk() == level, "sdk() is Android's level, 0 without Android: " + TargetKey.sdk() + " vs " + level);

        // The program line: our class from our APK, nonce, uid, key and deadline; the folders only for a save.
        String[] cmd = TargetKey.command("/data/app/~~x==/base.apk", "a1b2", "/t/a1b2", 10123, 132, 99999, true, ".*\\.ppst", new String[]{"/a", "/b"});
        check(cmd[0].equals("/system/bin/app_process") && cmd[1].equals("-Djava.class.path=/data/app/~~x==/base.apk"), "app_process");
        check(cmd[3].equals("xyz.pocketui.app.TargetKey") && cmd[4].equals("a1b2") && cmd[5].equals("/t/a1b2") && cmd[6].equals("10123") && cmd[7].equals("132"), "class, nonce, token, uid, key");
        check(cmd[8].equals("99999") && cmd[9].equals("4000") && cmd.length == 13 && cmd[12].equals("/b"), "deadline; a save watches the folders");
        cmd = TargetKey.command("/x.apk", "n", "/t/n", 10123, 134, 5, false, ".*\\.ppst", new String[]{"/a"});
        check(cmd.length == 11 && cmd[9].equals("0") && cmd[10].equals("-"), "load watches nothing");
        cmd = TargetKey.command("/x.apk", "n", "/t/n", 10123, 132, 5, true, "-", new String[0]);
        check(cmd.length == 11 && cmd[9].equals("0"), "an emulator whose states we don't know: no watch");

        // Every emulator of the menu: its state files match its pattern (names as the apps write them), others don't.
        String[][] names = {
            {"org.ppsspp.ppsspp", "ULUS10041_1.00_0.ppst"}, {"org.ppsspp.ppssppgold", "NPUH10091_1.00_2.ppst"},
            {"org.dolphinemu.dolphinemu", "GALE01.s01"}, {"com.armsx2", "SLUS-20062 (ABCD1234).01.p2s"},
            {"xyz.nethersx2.android", "SLES-50330 (1A2B3C4D).00.p2s"}, {"org.azahar_emu.azahar", "00040000001B5000.01.cst"},
            {"io.github.lime3ds.android", "0004000000030800.00.cst"}, {"com.github.stenzek.duckstation", "SLUS-00594_1.sav"},
            {"com.dsemu.drastic", "game_0.dss"}, {"com.retroarch", "Game.state1"}, {"com.retroarch.aarch64", "Game.state"},
        };
        for (String[] n : names) {
            String p = EmulatorKeys.statePattern(n[0]);
            check(n[1].matches(p), n[0] + " " + n[1] + " ~ " + p);
            check(!"cover.png".matches(p) && !(n[1] + ".tmp").matches(p), n[0] + ": only its states");
            check(EmulatorKeys.stateDirs(n[0], "/storage/emulated/0").length > 0, n[0] + " has state folders");
            for (String d : EmulatorKeys.stateDirs(n[0], "/storage/emulated/0")) check(d.startsWith("/storage/emulated/0/"), d);
        }
        check(EmulatorKeys.statePattern("com.rfandango.haku_x").equals("-") && EmulatorKeys.stateDirs("com.rfandango.haku_x", "/s").length == 0,
            "hakuX: folders unknown, so never \"Saved\"");
        for (String app : EmulatorKeys.APPS) check(EmulatorKeys.stateDirs(app, "/s").length > 0 || app.equals("com.rfandango.haku_x"), "known: " + app);
    }

    /**
     * The launch note and the press slot (Sol's recheck of 38cfee80f): a launch id that could not be written leaves the presses
     * locked, so a stale press can't match the old id; a press taken over ends with exactly its own token, never a newer one's.
     */
    private static void launchNoteAndSlot() {
        try {
            java.io.File dir = java.nio.file.Files.createTempDirectory("extras-note").toFile();
            java.io.File shared = java.nio.file.Files.createTempDirectory("extras-shared").toFile();
            try {
                java.util.Set<String> failing = new java.util.HashSet<>();
                LaunchNote.Writer w = (f, text) -> {
                    if (failing.contains(f.getName()) || !f.getParentFile().canWrite()) return false;
                    try { java.nio.file.Files.write(f.toPath(), text.getBytes("UTF-8")); return true; } catch (java.io.IOException e) { return false; }
                };
                LaunchNote.Result ok = LaunchNote.Result.PUBLISHED, locked = LaunchNote.Result.LOCKED, failed = LaunchNote.Result.FAILED;
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "old1", w) == ok && LaunchNote.launch(dir).equals("old1") && !LaunchNote.locked(dir, shared), "a launch published");
                // The note can't be written: the old id stays in the file, but the presses are locked until the next start.
                failing.add(LaunchNote.NOTE);
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new2", w) == locked, "a failed note write locks");
                String stale = "old1";
                check(LaunchNote.locked(dir, shared) || !stale.equals(LaunchNote.launch(dir)), "a stale press made with the old id is refused");
                failing.clear();
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new3", w) == ok && !LaunchNote.locked(dir, shared) && LaunchNote.launch(dir).equals("new3"), "the next start unlocks");
                // No lock can be written: the old note goes first, then the new one (or none).
                failing.add(LaunchNote.LOCK);
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new4", w) == ok && LaunchNote.launch(dir).equals("new4"), "without a lock: the new id replaces the old");
                failing.add(LaunchNote.NOTE);
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new5", w) == locked && LaunchNote.launch(dir).isEmpty(), "neither written: no id at all");
                failing.clear();
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "old6", w) == ok, "back to normal");
                // Sol's recheck of 09f9f7f27: a read-only note folder takes neither the lock nor the removal of the old note.
                check(dir.setWritable(false, false), "a read-only note folder");
                if (!dir.canWrite()) {
                    check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new7", w) == locked, "the lock goes to the shared folder");
                    check(LaunchNote.launch(dir).equals("old6") && LaunchNote.locked(dir, shared), "the old id is still there, but locked");
                    check(shared.setWritable(false, false), "the shared folder read-only too");
                    new java.io.File(shared, LaunchNote.LOCK).delete();
                    if (!shared.canWrite()) {
                        check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new8", w) == failed, "no lock anywhere: FAILED, the game must not start");
                        check(LaunchNote.publish(dir, null, "org.ppsspp.ppsspp", "new9", w) == failed, "no shared folder: FAILED");
                    }
                    shared.setWritable(true, false);
                }
                dir.setWritable(true, false);
                // Writable again: the next start publishes and removes both locks.
                new java.io.File(shared, LaunchNote.LOCK).createNewFile();
                check(LaunchNote.publish(dir, shared, "org.ppsspp.ppsspp", "new10", w) == ok && !LaunchNote.locked(dir, shared), "both locks gone after a good start");
            } finally {
                dir.setWritable(true, false);
                shared.setWritable(true, false);
                for (java.io.File d : new java.io.File[]{dir, shared}) { for (java.io.File f : d.listFiles()) f.delete(); d.delete(); }
            }

            java.io.File tokens = java.nio.file.Files.createTempDirectory("extras-press").toFile();
            try {
                PressSlot slot = new PressSlot();
                PressSlot.Job a = slot.take(0, 100);
                check(a != null && slot.busy() && slot.take(50, 200) == null, "one press at a time");
                java.io.File ta = new java.io.File(tokens, "a");
                check(ta.createNewFile() && a.attach(ta) && ta.exists(), "A's token");
                // A hangs; its time runs out; B takes over: A's token goes.
                PressSlot.Job b = slot.take(150, 300);
                check(b != null && !ta.exists() && a.dead(), "B took over: A's token is gone");
                // B hangs before making its token; C takes over from B.
                PressSlot.Job c = slot.take(350, 500);
                check(c != null, "C took over from B");
                java.io.File tc = new java.io.File(tokens, "c");
                check(tc.createNewFile() && c.attach(tc), "C's token");
                // B comes back late and makes its token: it is dropped at once, C's stays.
                java.io.File tb = new java.io.File(tokens, "b");
                check(tb.createNewFile() && !b.attach(tb) && !tb.exists(), "B, taken over, loses its late token");
                check(tc.exists(), "C keeps its token");
                // A and B finishing late don't free C's slot.
                slot.release(a);
                slot.release(b);
                check(slot.busy() && slot.take(400, 600) == null, "C still holds the slot");
                slot.release(c);
                check(!slot.busy(), "free after C");
            } finally {
                for (java.io.File f : tokens.listFiles()) f.delete();
                tokens.delete();
            }
        } catch (java.io.IOException e) {
            throw new AssertionError("launchNoteAndSlot: " + e, e);
        }
    }

    private static boolean contains(int[] rows, int row) {
        for (int r : rows) if (r == row) return true;
        return false;
    }
}
