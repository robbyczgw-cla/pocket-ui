package xyz.pocketui.app;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): the keys our pause menu presses in another emulator app to save and load a state.
 * Pressing a key in another app needs Shizuku or root and Android 13+ (`TargetKey`: the press reaches this app's window or
 * nothing); the emulator must have the key on its save and load.
 *
 * One scheme for every app: F2 saves, F4 loads (RetroArch's own defaults). RetroArch has them from the start. The others have
 * mappable hotkeys and get them once, in their own controls page, with the menu's "Teach" rows (the key is pressed for them
 * while their mapping waits for a key). Checked against the sources on 26.11.2026: PPSSPP maps no save key by default
 * (Core/KeyMapDefaults.cpp), so it must be taught; nothing here is checked on a device yet.
 *
 * A file of the player's own can change a package's keys: `files/extras/keys.txt`, a line `<package> <save> <load>` with key
 * names F1..F12, or `-` for none.
 * No Android types, so `scripts/android/test/ExtrasKeysTest.java` runs it on a plain JVM.
 */
final class EmulatorKeys {
    static final int KEYCODE_F1 = 131;
    static final String SAVE = "F2", LOAD = "F4";

    /** Apps our pause menu offers itself over (the handheld view starts them: EmulatorIntents, plus DraStic and NetherSX2). */
    static final String[] APPS = {
        "com.retroarch", "com.retroarch.aarch64", "com.retroarch.ra32",
        "com.armsx2", "come.nanodata.armsx2", "xyz.aethersx2.android", "xyz.nethersx2.android",
        "org.dolphinemu.dolphinemu", "org.ppsspp.ppsspp", "org.ppsspp.ppssppgold",
        "com.github.stenzek.duckstation", "org.azahar_emu.azahar", "io.github.lime3ds.android",
        "com.dsemu.drastic", "com.rfandango.haku_x",
    };

    final String save, load;

    private EmulatorKeys(String save, String load) {
        this.save = save;
        this.load = load;
    }

    static boolean watched(String pkg) {
        for (String app : APPS) if (app.equals(pkg)) return true;
        return false;
    }

    /** The keys of `pkg`, with the lines of keys.txt (may be null or empty). */
    static EmulatorKeys of(String pkg, String file) {
        String save = SAVE, load = LOAD;
        if (file != null) {
            for (String line : file.split("\n")) {
                String[] f = line.trim().split("\\s+");
                if (f.length == 3 && f[0].equals(pkg) && name(f[1]) && name(f[2])) {
                    save = f[1];
                    load = f[2];
                }
            }
        }
        return new EmulatorKeys(save, load);
    }

    private static boolean name(String key) {
        return key.equals("-") || code(key) > 0;
    }

    /**
     * Where `pkg` writes its save states, under the shared storage `root` (`/storage/emulated/<user>`), and the file names
     * (`statePattern`): `TargetKey` calls a save "saved" only when a file there is newer after the press. The folders are
     * the apps' defaults (their sources, and src/standalone.rs for the names on pocket-ui OS); a player's own folder is not
     * found, then the menu says only that the key was sent. Empty for an app whose states we don't know (hakuX).
     */
    static String[] stateDirs(String pkg, String root) {
        String own = root + "/Android/data/" + pkg + "/files";
        if (pkg.startsWith("org.ppsspp.")) return new String[]{root + "/PSP/PPSSPP_STATE", own + "/PSP/PPSSPP_STATE"};
        if (pkg.equals("org.dolphinemu.dolphinemu")) return new String[]{own + "/StateSaves", root + "/dolphin-emu/StateSaves"};
        if (pkg.contains("armsx2") || pkg.contains("aethersx2") || pkg.contains("nethersx2")) return new String[]{own + "/sstates"};
        if (pkg.equals("org.azahar_emu.azahar") || pkg.equals("io.github.lime3ds.android")) {
            return new String[]{own + "/states", root + "/Azahar/states", root + "/Lime3DS/states", root + "/citra-emu/states"};
        }
        if (pkg.equals("com.github.stenzek.duckstation")) return new String[]{own + "/savestates", root + "/duckstation/savestates"};
        if (pkg.equals("com.dsemu.drastic")) return new String[]{root + "/DraStic/savestates"};
        if (pkg.startsWith("com.retroarch")) return new String[]{root + "/RetroArch/states", own + "/states"};
        return new String[0];
    }

    /** The names of `pkg`'s state files (a Java regex over the whole name), "-" when unknown. */
    static String statePattern(String pkg) {
        if (pkg.startsWith("org.ppsspp.")) return ".*\\.ppst";
        if (pkg.equals("org.dolphinemu.dolphinemu")) return ".*\\.s[0-9][0-9]";
        if (pkg.contains("armsx2") || pkg.contains("aethersx2") || pkg.contains("nethersx2")) return ".*\\.p2s";
        if (pkg.equals("org.azahar_emu.azahar") || pkg.equals("io.github.lime3ds.android")) return ".*\\.cst";
        if (pkg.equals("com.github.stenzek.duckstation")) return ".*\\.sav";
        if (pkg.equals("com.dsemu.drastic")) return ".*\\.dss";
        if (pkg.startsWith("com.retroarch")) return ".*\\.state[0-9]*";
        return "-";
    }

    /**
     * A key our pause menu presses for an emulator (`TargetKey`: F1..F12 from Android's virtual keyboard, device -1) that
     * reached one of pocket-ui's own windows instead: on Android 13 a press whose window lost the focus can be delivered to
     * whatever is in front (ANDROID-EXTRAS.md), so the shelf and the player drop such keys (lead, 27.11.2026). A real
     * keyboard's F-keys (device 0 or more) still reach a computer core.
     */
    static boolean strayEmulatorKey(int code, int deviceId) {
        return deviceId == -1 && code >= KEYCODE_F1 && code <= KEYCODE_F1 + 11;
    }

    /** F1..F12 as Android key codes; 0 for anything else (and "-"). */
    static int code(String key) {
        if (key == null || !key.matches("F([1-9]|1[0-2])")) return 0;
        return KEYCODE_F1 + Integer.parseInt(key.substring(1)) - 1;
    }
}
