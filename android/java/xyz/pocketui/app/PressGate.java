package xyz.pocketui.app;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): whether a key our pause menu pressed a moment ago may still go out. `input keyevent`
 * goes to whatever window has the focus, so a delayed Save, Load or Teach press must never land elsewhere (Sol's review of
 * d69eadf3e: in the 1.5 s of a Teach row the shelf or Android's own UI could come in front, or the app could go away and
 * come back with another game). Every window that comes in front (any app, our own shelf, Android's panels) and every menu
 * that opens moves the gate on; a press made before is dropped. No Android types (`ExtrasKeysTest`).
 */
final class PressGate {
    private int epoch;
    private String owner = "";

    /** A window of `pkg` came in front (`TYPE_WINDOW_STATE_CHANGED`), whoever it belongs to; true when that moved the gate on. */
    synchronized boolean window(String pkg) {
        if (pkg == null || pkg.equals(owner)) return false;
        owner = pkg;
        epoch++;
        return true;
    }

    /** Something else happened that ends the moment of the last menu (a new menu opened, the service was interrupted). */
    synchronized void move() {
        epoch++;
    }

    /** The moment a press is made in: hand it back to `mayPress`. */
    synchronized int now() {
        return epoch;
    }

    /**
     * The press made at `ticket` for `pkg` may go out: nothing came in front since, `pkg` is still the window in front, and
     * the game we started is still in it (`playing`, the handheld view's note).
     */
    synchronized boolean mayPress(int ticket, String pkg, String playing) {
        return ticket == epoch && pkg != null && pkg.equals(owner) && pkg.equals(playing);
    }
}
