package xyz.pocketui.app;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

/** pocket-ui's own font files (the Chinese ones, `scripts/shipped-fonts.sh`), from the APK's assets/fonts into the app's
 *  files/fonts, where the native code reads them (`text_fallback::shipped_dir`). Copied before the native code starts,
 *  again whenever the APK was installed or updated since (its lastUpdateTime in files/fonts/.installed), each file
 *  written beside and renamed into place: a reader never sees half a font. Without them Chinese is not offered. */
final class ShippedFonts {
    static synchronized void install(android.content.Context context) {
        try {
            String stamp = Long.toString(context.getPackageManager().getPackageInfo(context.getPackageName(), 0).lastUpdateTime);
            File dir = new File(context.getFilesDir(), "fonts");
            File mark = new File(dir, ".installed");
            if (stamp.equals(read(mark))) return;
            if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("no fonts folder");
            android.content.res.AssetManager assets = context.getAssets();
            String[] names = assets.list("fonts");
            if (names == null || names.length == 0) return;
            for (String name : names) {
                File part = new File(dir, name + ".pocket-part");
                try (InputStream in = assets.open("fonts/" + name); FileOutputStream out = new FileOutputStream(part)) {
                    byte[] buffer = new byte[65536];
                    int n;
                    while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
                    out.getFD().sync();
                }
                if (!part.renameTo(new File(dir, name))) { part.delete(); throw new IOException("cannot place " + name); }
            }
            try (FileOutputStream out = new FileOutputStream(mark)) { out.write(stamp.getBytes("UTF-8")); }
        } catch (Exception error) {
            android.util.Log.w("PocketMvp", "fonts not installed: " + error);
        }
    }

    private static String read(File file) {
        try (InputStream in = new FileInputStream(file)) {
            byte[] b = new byte[64];
            int n = in.read(b);
            return n > 0 ? new String(b, 0, n, "UTF-8") : "";
        } catch (IOException e) {
            return "";
        }
    }
}
