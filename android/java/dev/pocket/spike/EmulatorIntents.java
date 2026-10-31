package dev.pocket.spike;
import android.content.*;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.IOException;

/** Explicit, version-sensitive recipes. Presence is checked before granting or starting anything. */
final class EmulatorIntents {
    private static final String RA_ACTIVITY = "com.retroarch.browser.retroactivity.RetroActivityFuture";
    static Intent create(Context context, Uri document, String relative, String profile) throws Exception {
        if (!"content".equals(document.getScheme()) || !DocumentsContract.isDocumentUri(context, document))
            throw new IOException("A SAF document is required");
        Intent intent = new Intent(Intent.ACTION_MAIN);
        String pkg;
        Uri grant = document;
        if (profile.startsWith("retroarch:")) {
            String[] parts = profile.split(":", -1);
            if (parts.length != 2 || !parts[1].matches("[a-z0-9_]+")) throw new IOException("Invalid RetroArch core");
            pkg = installed(context, new String[][]{{"com.retroarch.aarch64", RA_ACTIVITY}, {"com.retroarch", RA_ACTIVITY}, {"com.retroarch.ra32", RA_ACTIVITY}}, intent);
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
            String activity, extra = null;
            switch (profile) {
                case "ps2": pkg = "xyz.aethersx2.android"; activity = pkg + ".EmulationActivity"; extra = "bootPath"; break;
                case "dolphin": pkg = "org.dolphinemu.dolphinemu"; activity = pkg + ".ui.main.MainActivity"; extra = "AutoStartFile"; break;
                case "psp":
                    pkg = installed(context, new String[][]{{"org.ppsspp.ppsspp", "org.ppsspp.ppsspp.PpssppActivity"},
                        {"org.ppsspp.ppssppgold", "org.ppsspp.ppsspp.PpssppActivity"}}, intent);
                    activity = intent.getComponent().getClassName(); intent.setAction(Intent.ACTION_VIEW); break;
                case "psx": pkg = "com.github.stenzek.duckstation"; activity = pkg + ".EmulationActivity"; extra = "bootPath"; break;
                case "3ds":
                    pkg = installed(context, new String[][]{{"org.azahar_emu.azahar", "org.citra.citra_emu.activities.EmulationActivity"},
                        {"io.github.lime3ds.android", "org.citra.citra_emu.activities.EmulationActivity"},
                        {"io.github.lime3ds.android", "io.github.lime3ds.android.activities.EmulationActivity"}}, intent);
                    activity = intent.getComponent().getClassName(); intent.setAction(Intent.ACTION_VIEW); break;
                case "switch":
                    pkg = installed(context, new String[][]{{"dev.eden.eden_emulator", "org.yuzu.yuzu_emu.activities.EmulationActivity"},
                        {"org.citron.citron_emu", "org.citron.citron_emu.activities.EmulationActivity"}}, intent);
                    activity = intent.getComponent().getClassName(); intent.setAction("android.nfc.action.TECH_DISCOVERED"); break;
                default: throw new IOException("No emulator profile for " + profile);
            }
            intent.setClassName(pkg, activity);
            if (extra == null) intent.setDataAndType(document, "application/octet-stream");
            else intent.putExtra(extra, document.toString());
        }
        if (context.getPackageManager().resolveActivity(intent, 0) == null)
            throw new IOException("Install and configure the emulator first: " + pkg);
        intent.setClipData(ClipData.newRawUri("ROM access", grant));
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_NEW_TASK);
        return intent;
    }
    private static String installed(Context context, String[][] candidates, Intent intent) throws IOException {
        for (String[] pair : candidates) {
            intent.setClassName(pair[0], pair[1]);
            if (context.getPackageManager().resolveActivity(intent, 0) != null) return pair[0];
        }
        throw new IOException("No compatible emulator installed (" + candidates[0][0] + ")");
    }
}
