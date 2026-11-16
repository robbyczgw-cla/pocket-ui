package xyz.pocketui.app;

/** Keys that return from a failed game start. The handheld's HOME is BUTTON_MODE (110). */
final class PlayerExitKeys {
    static boolean closesError(int key) {
        return key == 4 || key == 82 || key == 110 || key == 97 || key == 111
            || key == 23 || key == 66 || key == 96;
    }
}
