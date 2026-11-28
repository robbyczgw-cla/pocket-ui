package xyz.pocketui.app;

import android.content.Intent;
import android.net.Uri;

/** The handheld view (android/handheld): the Linux app's pages in libpocket_handheld, over the same Android I/O as the Mobile
 *  view (PocketBase), in a process of its own (the app ends its process when it ends, as on Linux). It reads the games where
 *  they are, so it asks for All files access once, explained, with the Mobile view (the folder picker) as the other way. */
public class HandheldActivity extends PocketBase {
    static { System.loadLibrary("pocket_handheld"); }
    @Override protected boolean ownsBrightness() { return true; }
    /** The Linux pages fill their window edge to edge (no safe insets): on a phone with a camera hole the window keeps out
     *  of the hole's edge, so nothing of a page is under it (19.11.2026, 20:9 phones: the hole stood over the pause menu
     *  and the game info). A handheld without one (Retroid Pocket Mini V2) keeps its whole panel. */
    @Override protected int cutoutMode() { return android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_NEVER; }
    @Override protected native boolean nativeBack();
    @Override protected native void nativeConfigurationChanged();
    @Override protected native String nativePrepareDocument(int fd,String key,String name);

    private native void nativeEnding();
    /** "Choose cover…" on a game's card (src/app_cover_pick.rs): picked, cancelled, too-big, no-picker or failed. */
    private native void nativeCoverPicked(String outcome);

    private static final int PICK_COVER = 52;
    private volatile String coverDest;
    private volatile long coverLimit;

    /** Android's photo picker for a game's own cover: no storage permission (Android 11+ with the picker's update, else
     *  the system's image chooser). The chosen picture is copied to `dest` (at most `limit` bytes) on a thread; Rust reads
     *  it and checks it (`covers::custom::from_picture`). The answer comes through nativeCoverPicked. */
    public String pickCover(String dest, String limit) {
        coverDest = dest;
        coverLimit = Long.parseLong(limit);
        runOnUiThread(() -> {
            Intent intent = photoPicker()
                ? new Intent(android.provider.MediaStore.ACTION_PICK_IMAGES).setType("image/*")
                : new Intent(Intent.ACTION_GET_CONTENT).addCategory(Intent.CATEGORY_OPENABLE).setType("image/*");
            try { startActivityForResult(intent, PICK_COVER); }
            catch (android.content.ActivityNotFoundException e) {
                android.util.Log.w("PocketHandheld", "no picture picker", e);
                coverOutcome("no-picker");
            }
        });
        return "";
    }

    private static boolean photoPicker() {
        if (android.os.Build.VERSION.SDK_INT >= 33) return true;
        return android.os.Build.VERSION.SDK_INT >= 30 && android.os.ext.SdkExtensions.getExtensionVersion(android.os.Build.VERSION_CODES.R) >= 2;
    }

    private void coverOutcome(String outcome) {
        try { nativeCoverPicked(outcome); } catch (Throwable t) { android.util.Log.w("PocketHandheld", "cover answer", t); }
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        if (request == ASK_HOME) { android.util.Log.i("PocketHandheld", "home app: Android's question answered " + (result == RESULT_OK ? "yes" : "no")); return; }
        if (request != PICK_COVER) { super.onActivityResult(request, result, data); return; }
        Uri uri = data == null ? null : data.getData();
        if (result != RESULT_OK || uri == null) { coverOutcome("cancelled"); return; }
        String dest = coverDest;
        long limit = coverLimit;
        new Thread(() -> coverOutcome(copyPicture(uri, dest, limit)), "cover-pick").start();
    }

    /** The picture's bytes into `dest`: "picked", or "too-big" past `limit` (nothing is kept then), or "failed". */
    private String copyPicture(Uri uri, String dest, long limit) {
        java.io.File out = new java.io.File(dest);
        try (java.io.InputStream in = getContentResolver().openInputStream(uri); java.io.OutputStream os = new java.io.FileOutputStream(out)) {
            if (in == null) throw new java.io.IOException("no stream");
            byte[] buf = new byte[64 * 1024];
            long total = 0;
            for (int n; (n = in.read(buf)) > 0; ) {
                total += n;
                if (total > limit) { os.close(); out.delete(); return "too-big"; }
                os.write(buf, 0, n);
            }
            android.util.Log.i("PocketHandheld", "cover: picture copied, " + total + " bytes");
            return "picked";
        } catch (Exception e) {
            android.util.Log.w("PocketHandheld", "cover: picture not read", e);
            out.delete();
            return "failed";
        }
    }

    /** The view ends, finished or taken down by Android (a configuration it doesn't follow, memory): what the app keeps is
     *  written, then its process ends, as the app ends on Linux. Its loop does not end with the Activity, and NativeActivity's
     *  onDestroy and any new HandheldActivity in this process would wait for it (an ANR, 12.11.2026; review #2). */
    @Override protected void onDestroy() {
        try { nativeEnding(); } catch (Throwable t) { android.util.Log.w("PocketHandheld", "ending", t); }
        Views.endProcess(this);
        super.onDestroy();
    }

    private static final int ASK_HOME = 53;

    /** pocket-ui is Android's Home app now: "true" or "false" (Settings → System → Home app). */
    public String homeAppHeld() {
        try {
            if (android.os.Build.VERSION.SDK_INT >= 29) {
                android.app.role.RoleManager roles = getSystemService(android.app.role.RoleManager.class);
                return Boolean.toString(roles != null && roles.isRoleHeld(android.app.role.RoleManager.ROLE_HOME));
            }
            Intent home = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME);
            android.content.pm.ResolveInfo found = getPackageManager().resolveActivity(home, android.content.pm.PackageManager.MATCH_DEFAULT_ONLY);
            return Boolean.toString(found != null && found.activityInfo != null && getPackageName().equals(found.activityInfo.packageName));
        } catch (RuntimeException e) {
            android.util.Log.w("PocketHandheld", "home app: not read", e);
            return "false";
        }
    }

    /** Android's own question "pocket-ui as your Home app?" (Android 10+, the user answers), else (older Android, or pocket-ui
     *  already is the Home app and the row is there to choose another) Android's page of Home apps. Nothing is set by us. */
    public String askHomeApp() {
        final boolean held = "true".equals(homeAppHeld());
        runOnUiThread(() -> {
            try {
                if (android.os.Build.VERSION.SDK_INT >= 29 && !held) {
                    android.app.role.RoleManager roles = getSystemService(android.app.role.RoleManager.class);
                    if (roles != null && roles.isRoleAvailable(android.app.role.RoleManager.ROLE_HOME)) {
                        startActivityForResult(roles.createRequestRoleIntent(android.app.role.RoleManager.ROLE_HOME), ASK_HOME);
                        return;
                    }
                }
                startActivity(new Intent(android.provider.Settings.ACTION_HOME_SETTINGS));
            } catch (RuntimeException e) {
                android.util.Log.w("PocketHandheld", "home app: Android's page didn't open", e);
                try { startActivity(new Intent(android.provider.Settings.ACTION_SETTINGS)); } catch (RuntimeException ignored) {}
            }
        });
        return "";
    }

    /** pocket-ui is off Android's battery optimisation: "true" or "false" (Settings → System → Run in background). The
     *  whole app counts (the player's process too): Android judges an app, not a process. */
    public String batteryExempt() {
        try {
            android.os.PowerManager power = (android.os.PowerManager) getSystemService(POWER_SERVICE);
            return Boolean.toString(power != null && power.isIgnoringBatteryOptimizations(getPackageName()));
        } catch (RuntimeException e) {
            android.util.Log.w("PocketHandheld", "battery optimisation: not read", e);
            return "false";
        }
    }

    /** Android's own question "Let pocket-ui always run in the background?" (the user answers, nothing is set by us; the
     *  normal permission REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, no adb). Already off the optimisation: pocket-ui's own App
     *  info, where its battery use can be set back (Android 15's list of all apps would need scrolling to it); a device
     *  without either: Android's battery optimisation list. B comes back to the row. */
    @android.annotation.SuppressLint("BatteryLife")
    public String askBatteryExempt() {
        final boolean exempt = "true".equals(batteryExempt());
        final Uri self = Uri.parse("package:" + getPackageName());
        runOnUiThread(() -> {
            try {
                startActivity(exempt ? new Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, self)
                    : new Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, self));
                return;
            } catch (RuntimeException e) {
                android.util.Log.w("PocketHandheld", "battery optimisation: Android's " + (exempt ? "App info" : "question") + " didn't open", e);
            }
            try { startActivity(new Intent(android.provider.Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)); }
            catch (RuntimeException e) {
                android.util.Log.w("PocketHandheld", "battery optimisation: Android's page didn't open", e);
                try { startActivity(new Intent(android.provider.Settings.ACTION_SETTINGS)); } catch (RuntimeException ignored) {}
            }
        });
        return "";
    }

    private static final int READ_GAMES = 51;

    @Override protected void onResume() {
        super.onResume();
        // A game in another emulator app ended (Android extras: our pause menu is only over our games, PadWatch).
        Extras.gameEnded(this);
    }

    /** "Read your games" is a sheet of the view itself (src/app_android_files.rs), no Android dialog: its A asks Android
     *  here. From Android 11 Android's own page for All files access (that page can't be ours), before it the storage
     *  permission. Back in front, the view looks again and the sheet goes once the access is there. */
    public String askFilesAccess() {
        runOnUiThread(() -> {
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                try { startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName()))); }
                catch (RuntimeException e) { startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)); }
            } else requestPermissions(new String[] { android.Manifest.permission.READ_EXTERNAL_STORAGE }, READ_GAMES);
        });
        return "";
    }

}
