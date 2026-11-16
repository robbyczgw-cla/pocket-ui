package xyz.pocketui.app;

public final class PlayerExitKeysTest {
    public static void main(String[] args) {
        for (int key : new int[]{4,82,110,97,111,23,66,96}) {
            if (!PlayerExitKeys.closesError(key)) throw new AssertionError("Failed start must return to the library for key " + key);
        }
        for (int key : new int[]{19,20,21,22,24,25,99,100,102,103}) {
            if (PlayerExitKeys.closesError(key)) throw new AssertionError("Navigation/volume must not dismiss: " + key);
        }
        System.out.println("PASS: failed-start Back/HOME and confirmation keys");
    }
}
