package xyz.pocketui.app;

/**
 * Which way up the launcher, the Handheld view and the player ask Android for. With a controller (a handheld's own pad
 * counts) the picture stays the long way round, either way up; a phone without one turns freely, upright during its first
 * start. A square panel has no long way round: Android counts a square display as upright by nature, so asking it for
 * landscape turned the picture 90 degrees against the pad (the Anbernic RG Rotate's 720x720, whose swivel turns the
 * picture through the system, 24.11.2026). There the app follows the system's own turn. Pure rule, tested on the JVM
 * (scripts/android/test/OrientationTest.java); `squareDisplay` reads the panel.
 */
final class Orientation {
    // ActivityInfo.SCREEN_ORIENTATION_* (values of the platform, kept here so the rule runs without Android).
    static final int UNSPECIFIED = -1, SENSOR_LANDSCAPE = 6, SENSOR_PORTRAIT = 7, FULL_SENSOR = 10;

    private Orientation() {}

    /** A panel within 5 % of square (720x720, 1080x1080). The Retroid Pocket Mini V2 (1240x1080, 15 %) is not one. */
    static boolean square(int w, int h) {
        int lo = Math.min(w, h), hi = Math.max(w, h);
        return lo > 0 && hi * 100L <= lo * 105L;
    }

    /** What to ask Android for. `phone`: smallest width under 600 dp; `setupUpright`: the first start is on. */
    static int wanted(boolean controller, boolean setupUpright, boolean phone, boolean square) {
        if (square) return UNSPECIFIED;
        if (controller) return SENSOR_LANDSCAPE;
        return setupUpright && phone ? SENSOR_PORTRAIT : FULL_SENSOR;
    }

    /** The panel's own size (not the window's, which loses the bars) is square. */
    @SuppressWarnings("deprecation")
    static boolean squareDisplay(android.app.Activity activity) {
        android.graphics.Point size = new android.graphics.Point();
        activity.getWindowManager().getDefaultDisplay().getRealSize(size);
        return square(size.x, size.y);
    }
}
