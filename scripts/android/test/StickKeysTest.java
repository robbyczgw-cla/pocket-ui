package xyz.pocketui.app;

/** The launcher's D-pad keys with two pads: a still pad must not cancel the one being held. Run with javac + java on the Air. */
public final class StickKeysTest {
    static void require(boolean value, String message) { if (!value) throw new AssertionError(message); }
    static void is(int[] got, int release, int press, String what) {
        require(got[0] == release && got[1] == press, what + ": got release=" + got[0] + " press=" + got[1] + ", wanted " + release + "/" + press);
    }
    /** The launcher's earlier logic: one direction for all pads. The scenario below is red on it (the failure this class fixes). */
    static final class OneForAll {
        int held;
        int[] update(int next) {
            if (next == held) return new int[] {0, 0};
            int[] out = {held, next};
            held = next;
            return out;
        }
    }
    public static void main(String[] args) {
        OneForAll old = new OneForAll();
        old.update(StickKeys.RIGHT);
        require(old.update(0)[0] == StickKeys.RIGHT, "red: with one direction for all pads a still pad lets go of the held key");
        require(StickKeys.direction(1, 0, 0, 0) == StickKeys.RIGHT, "hat right");
        require(StickKeys.direction(0, 0, 0, -1) == StickKeys.UP, "stick up");
        require(StickKeys.direction(0, 0, .3f, .3f) == 0, "a stick near the middle is no key");
        require(StickKeys.direction(0, 1, -1, 0) == StickKeys.DOWN, "the hat wins over the stick");
        StickKeys keys = new StickKeys();
        // Pad 3 holds Right; pad 7 sits still and reports the centre: Right stays down.
        is(keys.update(3, StickKeys.RIGHT), 0, StickKeys.RIGHT, "pad 3 presses right");
        is(keys.update(7, 0), 0, 0, "a still pad changes nothing");
        // Both hold Right: the key goes up only when the last lets go.
        is(keys.update(7, StickKeys.RIGHT), 0, 0, "the second pad holds the key already down");
        is(keys.update(3, 0), 0, 0, "one lets go, the other still holds");
        is(keys.update(7, 0), StickKeys.RIGHT, 0, "the last lets go");
        // A change of direction on one pad lets go of the old and presses the new.
        is(keys.update(3, StickKeys.UP), 0, StickKeys.UP, "up");
        is(keys.update(3, StickKeys.LEFT), StickKeys.UP, StickKeys.LEFT, "up to left");
        int[] all = keys.releaseAll();
        require(all.length == 1 && all[0] == StickKeys.LEFT, "a pause lets go of what is down");
        require(keys.releaseAll().length == 0, "and only once");
        System.out.println("StickKeysTest ok");
    }
}
