package xyz.pocketui.app;

/** The screen stays on while a download the player asked for runs (a system's download, the starter pack, an update, a RomM
 *  game at its start): Android's screen timeout would turn it off mid-way, and with the screen off Android cuts an app's
 *  network sooner. FLAG_KEEP_SCREEN_ON on the view's window, no permission; when the last one ends the device's own timeout
 *  counts again. Per process: the Handheld and the Mobile view each register their window (`PocketBase.onCreate`). */
final class Awake {
    private static final java.util.concurrent.atomic.AtomicInteger running = new java.util.concurrent.atomic.AtomicInteger();
    private static volatile java.lang.ref.WeakReference<android.app.Activity> window = new java.lang.ref.WeakReference<>(null);
    private Awake() {}

    static void window(android.app.Activity activity) {
        window = new java.lang.ref.WeakReference<>(activity);
        apply(running.get() > 0);
    }

    /** Around a download: `try (Awake.Hold hold = Awake.during()) { … }`. */
    static Hold during() {
        if (running.getAndIncrement() == 0) apply(true);
        return new Hold();
    }

    /** For a transfer Rust runs itself (RomM over plain http on the LAN, `romm_net::AndroidNet::fetch`): begin/end in pairs. */
    static void begin() { if (running.getAndIncrement() == 0) apply(true); }
    static void end() { if (running.getAndUpdate(n -> Math.max(0, n - 1)) == 1) apply(false); }

    static final class Hold implements AutoCloseable {
        private boolean closed;
        @Override public void close() {
            if (closed) return;
            closed = true;
            if (running.decrementAndGet() == 0) apply(false);
        }
    }

    private static void apply(boolean on) {
        android.app.Activity a = window.get();
        if (a == null) return;
        a.runOnUiThread(() -> {
            // Read again on the UI thread: a download that ended meanwhile leaves the flag as the count says now.
            boolean keep = running.get() > 0;
            if (keep) a.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            else a.getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            android.util.Log.i("PocketMvp", "awake: screen " + (keep ? "kept on for a download" : "follows Android's timeout"));
        });
    }
}
