package xyz.pocketui.app;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;

/** Which view the app opens in (`Views.chosen`), on the JVM with the reader rule's Android doubles: the first start opens the
 *  Handheld view with or without a pad (19.11.2026: the Mobile view is frozen; a phone whose Bluetooth pad was off or asleep
 *  at the first start stayed in it for good), and a view chosen before (Settings → View) is kept. */
public final class ViewsTest {
    private static File dir(String name) throws Exception {
        File d = Files.createTempDirectory("views-" + name).toFile();
        d.deleteOnExit();
        return d;
    }

    private static String chosen(String name, boolean pad, String saved) throws Exception {
        JavaReaderRule.ContextFixture c = new JavaReaderRule.ContextFixture(dir(name));
        android.view.InputDevice.fixtureGamepad = pad;
        if (saved != null) Files.write(new File(c.getFilesDir(), "view").toPath(), saved.getBytes(StandardCharsets.UTF_8));
        String got = Views.chosen(c);
        String kept = new String(Files.readAllBytes(new File(c.getFilesDir(), "view").toPath()), StandardCharsets.UTF_8).trim();
        if (!kept.equals(got)) throw new AssertionError(name + ": opened " + got + " but kept " + kept);
        return got;
    }

    private static void expect(String name, String want, String got) {
        if (!want.equals(got)) throw new AssertionError(name + ": want " + want + ", got " + got);
    }

    public static void main(String[] args) throws Exception {
        expect("first start, no pad (a phone, its pad off)", Views.HANDHELD, chosen("phone", false, null));
        expect("first start, a pad", Views.HANDHELD, chosen("handheld", true, null));
        expect("Mobile chosen before is kept", Views.MOBILE, chosen("kept-mobile", false, "mobile"));
        expect("Mobile chosen before is kept with a pad", Views.MOBILE, chosen("kept-mobile-pad", true, "mobile\n"));
        expect("Handheld chosen before is kept", Views.HANDHELD, chosen("kept-handheld", false, "handheld"));
        expect("an unreadable choice opens Handheld", Views.HANDHELD, chosen("junk", false, "tablet"));
        expect("an empty choice opens Handheld", Views.HANDHELD, chosen("empty", false, ""));
        System.out.println("ViewsTest ok");
    }
}
