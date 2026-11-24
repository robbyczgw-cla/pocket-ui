package xyz.pocketui.app;

/**
 * Which way up the app asks Android for (Orientation). Runs on the JVM: javac -cp android.jar -d out
 * android/java/xyz/pocketui/app/Orientation.java scripts/android/test/OrientationTest.java &&
 * java -cp out:android.jar xyz.pocketui.app.OrientationTest
 */
public final class OrientationTest {
    static void require(boolean value, String message) { if (!value) throw new AssertionError(message); }

    /** The rule until 24.11.2026: a controller always asked for landscape. */
    static int oldRule(boolean controller, boolean setupUpright, boolean phone) {
        return controller ? Orientation.SENSOR_LANDSCAPE : setupUpright && phone ? Orientation.SENSOR_PORTRAIT : Orientation.FULL_SENSOR;
    }

    public static void main(String[] args) {
        // The RG Rotate: 720x720, its own pad. Android's landscape on a square panel is a quarter turn against the pad.
        boolean rotate = Orientation.square(720, 720);
        require(rotate, "720x720 is square");
        require(oldRule(true, false, true) == Orientation.SENSOR_LANDSCAPE, "red: the old rule forced landscape on it");
        require(Orientation.wanted(true, false, true, rotate) == Orientation.UNSPECIFIED, "a square panel with its pad follows the system");
        require(Orientation.wanted(false, true, true, rotate) == Orientation.UNSPECIFIED, "a square panel during the first start follows the system");
        require(Orientation.square(1080, 1080) && Orientation.square(720, 700), "near-square panels");
        // Everything else keeps its rule, on every combination.
        int[][] panels = {{1240, 1080}, {1920, 1080}, {1080, 2400}, {1600, 2560}, {1280, 960}, {640, 480}};
        for (int[] p : panels) {
            require(!Orientation.square(p[0], p[1]) && !Orientation.square(p[1], p[0]), p[0] + "x" + p[1] + " is not square");
            for (int bits = 0; bits < 8; bits++) {
                boolean controller = (bits & 1) != 0, upright = (bits & 2) != 0, phone = (bits & 4) != 0;
                require(Orientation.wanted(controller, upright, phone, false) == oldRule(controller, upright, phone), "unchanged off square panels");
            }
        }
        require(!Orientation.square(0, 0) && !Orientation.square(0, 720), "no panel is no square");
        System.out.println("OrientationTest ok");
    }
}
