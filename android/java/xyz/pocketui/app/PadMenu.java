package xyz.pocketui.app;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): the rows of our pause menu over another emulator app and what a key does on it.
 * `PadWatch` paints it and carries out the row. No Android types (`scripts/android/test/ExtrasKeysTest.java`).
 */
final class PadMenu {
    static final int CONTINUE = 0, SAVE = 1, LOAD = 2, SHELF = 4, EMULATOR_MENU = 5, TEACH_SAVE = 6, TEACH_LOAD = 7;
    /** What a key asks: nothing, close the menu, or carry out the selected row (`row()`). */
    static final int NOTHING = -1, CLOSE = -2;

    /** The English line of each row; `PadWatch` paints the translation Rust wrote (`files/extras/texts.tsv`). */
    static String label(int row) {
        switch (row) {
            case CONTINUE: return "Continue";
            case SAVE: return "Save state";
            case LOAD: return "Load state";
            case SHELF: return "Back to the shelf";
            case EMULATOR_MENU: return "Emulator's own menu";
            case TEACH_SAVE: return "Teach the save key (F2)";
            case TEACH_LOAD: return "Teach the load key (F4)";
            default: return "";
        }
    }

    final int[] rows;
    int sel;

    /**
     * `press`: keys can be pressed in the app (Shizuku or root). Without that the menu still brings back the shelf and the
     * emulator's own menu. No "Save and quit": a pressed key says nothing about whether the emulator saved (Sol's review
     * 26.11.2026), so ending the app after it could lose the game; the emulator's own menu ends it.
     */
    PadMenu(boolean press) {
        rows = press ? new int[]{CONTINUE, SAVE, LOAD, SHELF, EMULATOR_MENU, TEACH_SAVE, TEACH_LOAD} : new int[]{CONTINUE, SHELF, EMULATOR_MENU};
    }

    int row() {
        return rows[sel];
    }

    /** A key's press (`PadChord` codes): Up/Down walk round, A or Enter carries out the row, B, Back, HOME and START close. */
    int press(int code) {
        switch (code) {
            case PadChord.DPAD_UP: sel = (sel + rows.length - 1) % rows.length; return NOTHING;
            case PadChord.DPAD_DOWN: sel = (sel + 1) % rows.length; return NOTHING;
            case PadChord.BUTTON_A: case PadChord.DPAD_CENTER: case PadChord.ENTER: return row();
            case PadChord.BUTTON_B: case PadChord.BACK: case PadChord.HOME: case PadChord.MODE: case PadChord.START: return CLOSE;
            default: return NOTHING;
        }
    }
}
