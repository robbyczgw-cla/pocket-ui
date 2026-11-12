package xyz.pocketui.app;

import android.content.ContentResolver;
import android.net.Uri;
import android.provider.DocumentsContract;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import java.util.function.BooleanSupplier;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

/**
 * Settings → Saves: moves bytes between the app's files and one zip the player picked. Which files, under which names and
 * where they land is decided in Rust (src/save_transfer.rs) and arrives as a plan; nothing here chooses.
 * Worker threads only. A cancelled or failed export leaves no file, a cancelled or failed import changes nothing.
 */
final class SaveBackup {
    static final String MANIFEST = "pocket-ui-saves.json";
    private static final String TAG = "PocketMvp";
    private static final int MOST_ENTRIES = 20000;
    private SaveBackup() {}

    /** "exported" or "export-failed". */
    static String write(ContentResolver resolver, Uri target, String planJson, BooleanSupplier cancelled) {
        boolean done = false;
        try {
            JSONObject plan = new JSONObject(planJson);
            JSONArray files = plan.getJSONArray("files");
            try (OutputStream raw = open(resolver, target); ZipOutputStream zip = new ZipOutputStream(new BufferedOutputStream(raw, 1 << 16))) {
                zip.setLevel(1); // moments are compressed already; this is about not waiting
                zip.putNextEntry(new ZipEntry(MANIFEST));
                zip.write(plan.getString("manifest").getBytes(StandardCharsets.UTF_8));
                zip.closeEntry();
                byte[] buffer = new byte[1 << 16];
                for (int i = 0; i < files.length(); i++) {
                    JSONArray file = files.getJSONArray(i);
                    File source = new File(file.getString(1));
                    if (!source.isFile()) continue;
                    ZipEntry entry = new ZipEntry(file.getString(0));
                    entry.setTime(source.lastModified());
                    zip.putNextEntry(entry);
                    try (InputStream in = new FileInputStream(source)) {
                        for (int n; (n = in.read(buffer)) > 0; ) {
                            if (cancelled.getAsBoolean()) throw new IOException("cancelled");
                            zip.write(buffer, 0, n);
                        }
                    }
                    zip.closeEntry();
                }
            }
            done = !cancelled.getAsBoolean();
        } catch (Exception e) {
            android.util.Log.w(TAG, "saves export", e);
        }
        if (!done) {
            // No half backup that looks like a whole one.
            try { DocumentsContract.deleteDocument(resolver, target); } catch (Exception ignored) {}
        }
        return done ? "exported" : "export-failed";
    }
    private static OutputStream open(ContentResolver resolver, Uri target) throws IOException {
        OutputStream out;
        try { out = resolver.openOutputStream(target, "wt"); }
        catch (Exception e) { out = resolver.openOutputStream(target, "w"); }
        if (out == null) throw new IOException("The folder gave no file to write");
        return out;
    }

    /** "listed\n{manifest, names}" or "import-failed". A file that is no zip lists nothing: Rust says what it is. */
    static String list(ContentResolver resolver, Uri source) {
        try (InputStream raw = resolver.openInputStream(source)) {
            if (raw == null) throw new IOException("The file can't be opened");
            ZipInputStream zip = new ZipInputStream(new BufferedInputStream(raw, 1 << 16));
            JSONArray names = new JSONArray();
            String manifest = null;
            int seen = 0;
            for (ZipEntry entry; (entry = zip.getNextEntry()) != null && seen < MOST_ENTRIES; seen++) {
                if (entry.isDirectory()) continue;
                if (MANIFEST.equals(entry.getName())) manifest = new String(bounded(zip, 8 << 20), StandardCharsets.UTF_8);
                else names.put(entry.getName());
            }
            JSONObject listing = new JSONObject();
            listing.put("manifest", manifest == null ? JSONObject.NULL : manifest);
            listing.put("names", names);
            return "listed\n" + listing;
        } catch (Exception e) {
            android.util.Log.w(TAG, "saves import: listing", e);
            return "import-failed";
        }
    }
    private static byte[] bounded(InputStream in, int most) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buffer = new byte[1 << 14];
        for (int n; (n = in.read(buffer)) > 0; ) {
            if (out.size() + n > most) throw new IOException("The backup's list is too long");
            out.write(buffer, 0, n);
        }
        return out.toByteArray();
    }

    /**
     * "imported", "no-room" or "import-failed". Everything is unpacked beside the saves first (import.part); only when
     * every listed file is there do the phone's sets go and the backup's move in, a game's folder at a time.
     */
    static String apply(ContentResolver resolver, Uri source, File root, String planJson, BooleanSupplier cancelled) {
        File part = new File(root, "import.part");
        try {
            deleteTree(part);
            JSONObject plan = new JSONObject(planJson);
            Map<String, JSONObject> take = new HashMap<>();
            JSONArray wanted = plan.getJSONArray("take");
            for (int i = 0; i < wanted.length(); i++) take.put(wanted.getJSONObject(i).getString("entry"), wanted.getJSONObject(i));
            String inside = part.getCanonicalPath() + File.separator;
            try (InputStream raw = resolver.openInputStream(source)) {
                if (raw == null) throw new IOException("The file can't be opened");
                ZipInputStream zip = new ZipInputStream(new BufferedInputStream(raw, 1 << 16));
                byte[] buffer = new byte[1 << 16];
                for (ZipEntry entry; (entry = zip.getNextEntry()) != null; ) {
                    JSONObject file = take.remove(entry.getName());
                    if (file == null) continue;
                    File target = new File(part, file.getString("to"));
                    if (!target.getCanonicalPath().startsWith(inside)) throw new IOException("A name leads outside the saves");
                    File folder = target.getParentFile();
                    if (folder == null || (!folder.isDirectory() && !folder.mkdirs())) throw new IOException("No folder for " + target.getName());
                    if (root.getUsableSpace() < (64L << 20)) return "no-room";
                    try (OutputStream out = new FileOutputStream(target)) {
                        for (int n; (n = zip.read(buffer)) > 0; ) {
                            if (cancelled.getAsBoolean()) throw new IOException("cancelled");
                            out.write(buffer, 0, n);
                        }
                    }
                    long written = file.optLong("written", 0);
                    // A moment older than the in-game save is not resumed: the files' order in time comes back with them.
                    if (written > 0 && !target.setLastModified(written)) android.util.Log.w(TAG, "saves import: time of " + target.getName());
                }
            }
            if (!take.isEmpty()) throw new IOException("The backup lacks " + take.size() + " of its listed files");
            if (cancelled.getAsBoolean()) throw new IOException("cancelled");
            JSONArray remove = plan.getJSONArray("remove"), place = plan.getJSONArray("place");
            for (int i = 0; i < remove.length(); i++) deleteTree(gameFolder(root, remove.getString(i)));
            for (int i = 0; i < place.length(); i++) {
                File from = gameFolder(part, place.getString(i)), to = gameFolder(root, place.getString(i));
                deleteTree(to);
                File parent = to.getParentFile();
                if (parent == null || (!parent.isDirectory() && !parent.mkdirs()) || !from.renameTo(to)) throw new IOException("Could not move in " + place.getString(i));
            }
            return "imported";
        } catch (Exception e) {
            android.util.Log.w(TAG, "saves import", e);
            String why = String.valueOf(e.getMessage());
            return why.contains("ENOSPC") || why.contains("No space left") ? "no-room" : "import-failed";
        } finally {
            deleteTree(part);
        }
    }
    /** `saves/<16 hex>` or `states/<16 hex>` under `base`, and nothing else. */
    private static File gameFolder(File base, String name) throws IOException {
        if (!name.matches("(saves|states)/[0-9a-f]{16}")) throw new IOException("Not a game's save folder: " + name);
        return new File(base, name);
    }
    private static void deleteTree(File file) {
        File[] inside = file.listFiles();
        if (inside != null) for (File child : inside) deleteTree(child);
        if (file.exists() && !file.delete()) android.util.Log.w(TAG, "saves: could not remove " + file.getName());
    }
}
