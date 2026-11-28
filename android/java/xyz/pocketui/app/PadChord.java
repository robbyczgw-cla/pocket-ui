package xyz.pocketui.app;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): which pad keys open our pause menu over another emulator app, as `PadWatch` sees
 * them in its key filter. No Android types, so `scripts/android/test/ExtrasKeysTest.java` runs it on a plain JVM.
 *
 * - HOME (3) or the pad's HOME/MODE key (110): the menu opens at its press; both halves stay with us.
 * - SELECT held, then START: the menu opens at START's press; START's both halves stay with us. SELECT itself went on to the
 *   emulator when it was pressed (a key filter can't take a press back later), so the game sees a short SELECT press.
 * - Every key whose press we kept keeps its release too (the emulator never sees a lone release), also after the menu closed.
 * - Every key whose press went on to the emulator gets its release there too, also while the menu is open (SELECT of
 *   SELECT + START: the emulator saw it go down; Sol's review 26.11.2026, it stayed held). Keys are told apart per
 *   device (`dev`), so a second pad's keys are its own.
 */
final class PadChord {
    static final int HOME = 3, BACK = 4, DPAD_UP = 19, DPAD_DOWN = 20, DPAD_CENTER = 23, ENTER = 66;
    static final int BUTTON_A = 96, BUTTON_B = 97, START = 108, SELECT = 109, MODE = 110;
    static final int ACTION_DOWN = 0, ACTION_UP = 1;

    /** What the filter does with a key. */
    static final int PASS = 0, KEEP = 1, OPEN = 2;

    /** SELECT is held on these devices. */
    private final java.util.HashSet<Integer> selectHeld = new java.util.HashSet<>();
    /** The keys (device and code) whose press we kept, so their release is kept too. */
    private final java.util.HashSet<Long> kept = new java.util.HashSet<>();
    /** The keys whose press the emulator got, so their release goes there too. */
    private final java.util.HashSet<Long> passed = new java.util.HashSet<>();

    private static long id(int dev, int code) {
        return ((long) dev << 32) | (code & 0xffffffffL);
    }

    /** A key while the menu is closed: PASS to the emulator, KEEP (ours, nothing happens), or OPEN the menu. */
    int key(int dev, int code, int action, int repeat) {
        long k = id(dev, code);
        if (code == SELECT) { if (action == ACTION_DOWN) selectHeld.add(dev); else selectHeld.remove(dev); }
        if (action == ACTION_UP) {
            passed.remove(k);
            return kept.remove(k) ? KEEP : PASS;
        }
        if (action != ACTION_DOWN) return PASS;
        if (kept.contains(k)) return KEEP; // a held key repeats
        boolean opens = code == HOME || code == MODE || (code == START && selectHeld.contains(dev));
        if (!opens) {
            passed.add(k);
            return PASS;
        }
        kept.add(k);
        return repeat == 0 ? OPEN : KEEP;
    }

    /** A key while the menu is open: true when it is the menu's; false for the release of a press the emulator got. */
    boolean menuKey(int dev, int code, int action) {
        long k = id(dev, code);
        if (code == SELECT && action != ACTION_DOWN) selectHeld.remove(dev);
        if (action == ACTION_DOWN) {
            if (passed.contains(k)) return true; // a held key's repeats stay with the menu
            kept.add(k);
            return true;
        }
        if (passed.remove(k)) return false;
        kept.remove(k);
        return true;
    }

    /** A key whose press the emulator got is still down: its release must still reach the emulator's window, so the menu
     *  doesn't take the focus yet (Sol's review of d69eadf3e: SELECT's release went to the focused overlay). */
    boolean heldByGame() {
        return !passed.isEmpty();
    }

    /** The emulator app went: nothing is held for it any more. */
    void reset() {
        selectHeld.clear();
        kept.clear();
        passed.clear();
    }
}
