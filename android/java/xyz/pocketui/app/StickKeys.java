package xyz.pocketui.app;

/**
 * The launcher turns a pad's hat and left stick into D-pad keys. Each pad has its own direction: a second pad that is
 * sitting still (its stick jitters back to centre) must not let go of the direction the first one is holding, and a key
 * stays down until the last pad holding that direction lets go. No Android classes: tested on the JVM (StickKeysTest).
 */
final class StickKeys {
    static final int UP = 19, DOWN = 20, LEFT = 21, RIGHT = 22;
    private final java.util.HashMap<Integer, Integer> held = new java.util.HashMap<>();

    /** The direction key of a hat/stick position, 0 for none. The hat wins over the stick, the longer axis over the shorter. */
    static int direction(float hatX, float hatY, float stickX, float stickY) {
        float y = Math.abs(hatY) < .5f ? stickY : hatY;
        float x = Math.abs(hatX) < .5f ? stickX : hatX;
        return Math.abs(x) > Math.abs(y) ? (x < -.55f ? LEFT : x > .55f ? RIGHT : 0) : (y < -.55f ? UP : y > .55f ? DOWN : 0);
    }

    private boolean holds(int key) {
        for (int value : held.values()) if (value == key) return true;
        return false;
    }

    /** A pad moved: the keys to let go of and to press, {release, press}, 0 where nothing changes. */
    int[] update(int device, int next) {
        int before = held.containsKey(device) ? held.get(device) : 0;
        if (next == before) return new int[] {0, 0};
        if (next == 0) held.remove(device); else held.put(device, next);
        int release = before != 0 && !holds(before) ? before : 0;
        int press = next != 0 && !holdsOther(device, next) ? next : 0;
        return new int[] {release, press};
    }

    private boolean holdsOther(int device, int key) {
        for (java.util.Map.Entry<Integer, Integer> e : held.entrySet()) if (e.getKey() != device && e.getValue() == key) return true;
        return false;
    }

    /** Every direction down at the moment, each once (the activity pauses). */
    int[] releaseAll() {
        java.util.TreeSet<Integer> keys = new java.util.TreeSet<>(held.values());
        held.clear();
        int[] out = new int[keys.size()];
        int i = 0;
        for (int key : keys) out[i++] = key;
        return out;
    }
}
