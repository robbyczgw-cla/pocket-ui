package xyz.pocketui.app;
import android.content.*;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.IOException;

/** Explicit, version-sensitive recipes. Presence is checked before granting or starting anything. */
final class EmulatorIntents {
    private static final String RA_ACTIVITY = "com.retroarch.browser.retroactivity.RetroActivityFuture";
    private static final String MAIN = Intent.ACTION_MAIN, VIEW = Intent.ACTION_VIEW;
    /**
     * The apps that take a profile, in the order they are tried: package, Activity, action, and the extra that carries the
     * document URI (null: the URI is the intent's data). Empty: none can be started from here yet. Checked on the Odin 2 Mini
     * on 10.11.2026 (docs/ANDROID-MVP.md "Emulator recipes"); each goes straight into the game.
     */
    private static String[][] apps(String profile) {
        if (profile.startsWith("retroarch:")) return new String[][]{{"com.retroarch.aarch64", RA_ACTIVITY, MAIN, "ROM"}, {"com.retroarch", RA_ACTIVITY, MAIN, "ROM"}, {"com.retroarch.ra32", RA_ACTIVITY, MAIN, "ROM"}};
        switch (profile) {
            // ARMSX2 2.9.3: Main reads the intent's data (handleExternalLaunchIntent) and goes back to us on Close.
            case "ps2": return new String[][]{{"com.armsx2", "com.armsx2.Main", VIEW, null},
                {"xyz.aethersx2.android", "xyz.aethersx2.android.EmulationActivity", MAIN, "bootPath"}};
            // Dolphin 2603a: StartupHandler boots the ClipData's or the data's URI before AutoStartFile.
            case "dolphin": return new String[][]{{"org.dolphinemu.dolphinemu", "org.dolphinemu.dolphinemu.ui.main.MainActivity", MAIN, "AutoStartFile"}};
            case "psx": return new String[][]{{"com.github.stenzek.duckstation", "com.github.stenzek.duckstation.EmulationActivity", MAIN, "bootPath"}};
            case "psp": return new String[][]{{"org.ppsspp.ppsspp", "org.ppsspp.ppsspp.PpssppActivity", VIEW, null}, {"org.ppsspp.ppssppgold", "org.ppsspp.ppsspp.PpssppActivity", VIEW, null}};
            case "3ds": return new String[][]{{"org.azahar_emu.azahar", "org.citra.citra_emu.activities.EmulationActivity", VIEW, null},
                {"io.github.lime3ds.android", "org.citra.citra_emu.activities.EmulationActivity", VIEW, null},
                {"io.github.lime3ds.android", "io.github.lime3ds.android.activities.EmulationActivity", VIEW, null}};
            // hakuX 1.3.0: LauncherActivity starts the core with the data's URI once a games folder is chosen in it.
            case "xbox": return new String[][]{{"com.rfandango.haku_x", "com.rfandango.haku_x.LauncherActivity", VIEW, null}};
            default: return new String[0][];
        }
    }
    /** The Game check: is an app that takes this profile on the phone? The components `create` would start. */
    static boolean available(Context context, String profile) {
        for (String[] pair : apps(profile)) {
            Intent intent = new Intent(Intent.ACTION_MAIN).setClassName(pair[0], pair[1]);
            if (context.getPackageManager().resolveActivity(intent, 0) != null) return true;
        }
        return false;
    }
    /**
     * Settings → Plays with: for each profile in the comma list, the apps that take it and are on the phone (package and
     * name as the phone shows it), as {"psp":[{"p":"org.ppsspp.ppsspp","l":"PPSSPP"}]}. A profile with none is left out.
     */
    static String installedApps(Context context, String profiles) {
        org.json.JSONObject all = new org.json.JSONObject();
        android.content.pm.PackageManager pm = context.getPackageManager();
        for (String profile : profiles.split(",")) {
            if (profile.isEmpty()) continue;
            org.json.JSONArray found = new org.json.JSONArray();
            java.util.HashSet<String> seen = new java.util.HashSet<>();
            for (String[] app : apps(profile)) {
                if (seen.contains(app[0]) || pm.resolveActivity(new Intent(Intent.ACTION_MAIN).setClassName(app[0], app[1]), 0) == null) continue;
                seen.add(app[0]);
                String name = app[0];
                try { name = String.valueOf(pm.getApplicationLabel(pm.getApplicationInfo(app[0], 0))); } catch (Exception ignored) { }
                try { found.put(new org.json.JSONObject().put("p", app[0]).put("l", name)); } catch (org.json.JSONException ignored) { }
            }
            try { if (found.length() > 0) all.put(profile, found); } catch (org.json.JSONException ignored) { }
        }
        return all.toString();
    }
    /** The recipes with the app Settings → Plays with chose first; the others stay behind it, as before. */
    private static String[][] prefer(String[][] candidates, String preferred) {
        if (preferred == null || preferred.isEmpty()) return candidates;
        java.util.ArrayList<String[]> ordered = new java.util.ArrayList<>();
        for (String[] app : candidates) if (app[0].equals(preferred)) ordered.add(app);
        for (String[] app : candidates) if (!app[0].equals(preferred)) ordered.add(app);
        return ordered.toArray(new String[0][]);
    }
    static Intent create(Context context, Uri document, String relative, String profile, String preferred) throws Exception {
        if (!"content".equals(document.getScheme()) || !DocumentsContract.isDocumentUri(context, document))
            throw new IOException("A SAF document is required");
        Intent intent = new Intent(Intent.ACTION_MAIN);
        String pkg;
        Uri grant = document;
        if (profile.startsWith("retroarch:")) {
            String[] parts = profile.split(":", -1);
            if (parts.length != 2 || !parts[1].matches("[a-z0-9_]+")) throw new IOException("Invalid RetroArch core");
            pkg = installed(context, prefer(apps(profile), preferred), intent);
            // RetroArch 1.22.2's VFS uses saf://<escaped tree>/<relative path>, not a content:// ROM path.
            grant = DocumentsContract.buildTreeDocumentUri(document.getAuthority(), DocumentsContract.getTreeDocumentId(document));
            if (relative.startsWith("/") || relative.contains("\\") || java.util.Arrays.asList(relative.split("/")).contains(".."))
                throw new IOException("Invalid relative ROM name");
            String saf = "saf://" + grant.toString().replace("%", "%25").replace("/", "%2F") + "/" + relative;
            intent.putExtra("ROM", saf);
            android.content.pm.ApplicationInfo app = context.getPackageManager().getApplicationInfo(pkg, 0);
            String external = android.os.Environment.getExternalStorageDirectory().getAbsolutePath() + "/Android/data/" + pkg + "/files";
            intent.putExtra("LIBRETRO", app.dataDir + "/cores/" + parts[1] + "_libretro_android.so");
            // The same defaults RetroArch's own MainMenuActivity supplies. Never override SRAM/state directories.
            intent.putExtra("CONFIGFILE", external + "/retroarch.cfg");
            intent.putExtra("DATADIR", app.dataDir);
            intent.putExtra("APK", app.sourceDir);
            intent.putExtra("SDCARD", android.os.Environment.getExternalStorageDirectory().getAbsolutePath());
            intent.putExtra("EXTERNAL", external);
            intent.putExtra("IME", android.provider.Settings.Secure.getString(context.getContentResolver(), android.provider.Settings.Secure.DEFAULT_INPUT_METHOD));
            android.util.Log.i("PocketMvp", "RetroArch ROM=" + saf);
        } else {
            String[] app = chosen(context, prefer(apps(profile), preferred), intent);
            if (app == null) throw new IOException("No emulator profile for " + profile);
            pkg = app[0];
            intent.setAction(app[2]);
            if (app[3] == null) intent.setDataAndType(document, "application/octet-stream");
            else intent.putExtra(app[3], document.toString());
        }
        if (context.getPackageManager().resolveActivity(intent, 0) == null)
            throw new IOException("Install and configure the emulator first: " + pkg);
        // Azahar: its own screens (the first-start setup, its game list) stay in the task it was opened in, and the game is
        // pushed on top of them: Close Game then lands on Azahar's list, not here. A clean task ends with the game, and the
        // phone goes back to pocket-ui. Checked on the Odin 2 Mini on 11.11.2026 (Azahar 2126.2, twice, play time counted).
        if (profile.equals("3ds")) intent.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK);
        intent.setClipData(ClipData.newRawUri("ROM access", grant));
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_NEW_TASK);
        return intent;
    }
    private static String installed(Context context, String[][] candidates, Intent intent) throws IOException {
        return chosen(context, candidates, intent)[0];
    }
    /** The first candidate on the phone, its component set on `intent`. Null: the profile has none. */
    private static String[] chosen(Context context, String[][] candidates, Intent intent) throws IOException {
        if (candidates.length == 0) return null;
        for (String[] app : candidates) {
            intent.setClassName(app[0], app[1]);
            if (context.getPackageManager().resolveActivity(intent, 0) != null) return app;
        }
        throw new IOException("No compatible emulator installed (" + candidates[0][0] + ")");
    }
}
