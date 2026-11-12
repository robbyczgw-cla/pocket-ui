package xyz.pocketui.app;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.util.*;

/**
 * The drawer under the shelf: the apps Android would put on any home screen, and starting one.
 * Visibility comes from the manifest's {@code <queries>} intent for MAIN/LAUNCHER, not QUERY_ALL_PACKAGES:
 * a home screen needs exactly the apps that have a start icon (docs/ANDROID-MVP.md "Android apps").
 */
final class AppDrawer {
    /** Icon side in pixels; the launcher paints it at about 30 dp, muted. */
    private static final int ICON = 96;

    private AppDrawer() {}

    /** Every app with a start icon but this one: package, label, Android's own "game" category (a hint only) and the
     *  path of its icon as raw ARGB (two big-endian ints w, h, then the pixels). Icons are cached by update time. */
    static String list(Activity activity) {
        PackageManager pm = activity.getPackageManager();
        Intent main = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> found = pm.queryIntentActivities(main, 0);
        File dir = new File(activity.getCacheDir(), "app-icons");
        if (!dir.isDirectory() && !dir.mkdirs()) dir = null;
        Set<String> seen = new HashSet<>();
        Set<String> keep = new HashSet<>();
        JSONArray out = new JSONArray();
        for (ResolveInfo r : found) {
            try {
                String pkg = r.activityInfo.packageName;
                if (pkg.equals(activity.getPackageName()) || !seen.add(pkg)) continue;
                ApplicationInfo info = r.activityInfo.applicationInfo;
                @SuppressWarnings("deprecation") boolean game = info.category == ApplicationInfo.CATEGORY_GAME || (info.flags & ApplicationInfo.FLAG_IS_GAME) != 0;
                String icon = "";
                if (dir != null) {
                    long stamp = pm.getPackageInfo(pkg, 0).lastUpdateTime;
                    File file = new File(dir, pkg + "-" + stamp + ".argb");
                    if (!file.isFile()) writeIcon(r.loadIcon(pm), file);
                    if (file.isFile()) { icon = file.getAbsolutePath(); keep.add(file.getName()); }
                }
                out.put(new JSONObject().put("package", pkg).put("label", String.valueOf(r.loadLabel(pm))).put("game", game).put("icon", icon));
            } catch (Exception e) {
                android.util.Log.w("PocketMvp", "app drawer: " + e);
            }
        }
        // Icons of apps that went or were updated.
        File[] old = dir == null ? null : dir.listFiles();
        if (old != null) for (File f : old) if (!keep.contains(f.getName()) && !f.delete()) android.util.Log.w("PocketMvp", "app drawer: stale icon stays");
        return out.toString();
    }

    private static void writeIcon(Drawable icon, File file) throws IOException {
        if (icon == null) return;
        Bitmap bitmap = Bitmap.createBitmap(ICON, ICON, Bitmap.Config.ARGB_8888);
        icon.setBounds(0, 0, ICON, ICON);
        icon.draw(new Canvas(bitmap));
        int[] px = new int[ICON * ICON];
        bitmap.getPixels(px, 0, ICON, 0, 0, ICON, ICON);
        bitmap.recycle();
        File part = new File(file.getPath() + ".part");
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(new FileOutputStream(part)))) {
            out.writeInt(ICON);
            out.writeInt(ICON);
            for (int p : px) out.writeInt(p);
        }
        if (!part.renameTo(file)) { part.delete(); }
    }

    /** Starts the app the way any home screen does; "" when Android took it, else why not. */
    static String launch(Activity activity, String pkg) {
        Intent intent = activity.getPackageManager().getLaunchIntentForPackage(pkg);
        if (intent == null) return "missing";
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        try {
            activity.startActivity(intent);
            return "";
        } catch (Exception e) {
            android.util.Log.w("PocketMvp", "app drawer: start " + pkg + ": " + e);
            return "failed";
        }
    }
}
