package xyz.pocketui.app;

import android.content.ContentResolver;
import android.content.UriPermission;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import org.json.JSONArray;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

/**
 * Saves copied into the player's chosen folder (src/save_copy.rs decides; this only moves bytes). The copies sit in
 * `pocket-ui saves/<game>/…` inside one picked tree, written through the Storage Access Framework, so they stay when the
 * app is removed. Worker threads only. Every call answers "ok[\n…]", "no-access", "gone" or "error\n<why>": the Rust
 * side turns those into words for the player.
 */
final class SaveCopy {
    static final String FOLDER = "pocket-ui saves";
    private static final String TAG = "PocketMvp";
    private static final String BINARY = "application/octet-stream";
    private final ContentResolver resolver;
    private final Uri tree;
    /** Folder path ("pocket-ui saves/Zelda [..]/saves") → document id, for this run. */
    private final Map<String, String> ids = new HashMap<>();

    SaveCopy(ContentResolver resolver, String root) { this.resolver = resolver; this.tree = Uri.parse(root); }

    /** "ok" when pocket-ui may write into the tree (and it answers), "read-only", "lost" (no grant) or "gone". */
    static String state(ContentResolver resolver, String root) {
        Uri tree = Uri.parse(root);
        boolean read = false, write = false;
        for (UriPermission p : resolver.getPersistedUriPermissions()) {
            if (!p.getUri().equals(tree)) continue;
            read |= p.isReadPermission();
            write |= p.isWritePermission();
        }
        if (!read && !write) return "lost";
        try (Cursor c = resolver.query(DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree)),
                new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID}, null, null, null)) {
            if (c == null) return "gone";
        } catch (SecurityException e) { return "lost"; }
        catch (RuntimeException e) { return "gone"; }
        return write ? "ok" : "read-only";
    }

    private static String failure(Exception e) {
        android.util.Log.w(TAG, "save copy", e);
        if (e instanceof SecurityException) return "no-access";
        if (e instanceof FileNotFoundException || e instanceof IllegalArgumentException) return "gone";
        return "error\n" + e;
    }

    private Uri doc(String id) { return DocumentsContract.buildDocumentUriUsingTree(tree, id); }
    private Uri children(String id) { return DocumentsContract.buildChildDocumentsUriUsingTree(tree, id); }

    /** The id of `name` below `parent` and whether it is a folder; null when there is none. */
    private String[] child(String parent, String name) throws IOException {
        try (Cursor c = resolver.query(children(parent), new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE}, null, null, null)) {
            if (c == null) throw new FileNotFoundException("The folder gave no list");
            while (c.moveToNext()) if (name.equals(c.getString(1))) return new String[]{c.getString(0), DocumentsContract.Document.MIME_TYPE_DIR.equals(c.getString(2)) ? "dir" : "file"};
        }
        return null;
    }

    /** The folder at `parts` below the tree (created when `create`); null when it is not there. */
    private String folder(String[] parts, boolean create) throws IOException {
        String id = DocumentsContract.getTreeDocumentId(tree), path = "";
        for (String part : parts) {
            path = path.isEmpty() ? part : path + "/" + part;
            String known = ids.get(path);
            if (known != null) { id = known; continue; }
            String[] found = child(id, part);
            if (found != null && !found[1].equals("dir")) throw new IOException(part + " is a file, not a folder");
            if (found == null) {
                if (!create) return null;
                Uri made = DocumentsContract.createDocument(resolver, doc(id), DocumentsContract.Document.MIME_TYPE_DIR, part);
                if (made == null) throw new IOException("The folder " + part + " could not be made");
                found = new String[]{DocumentsContract.getDocumentId(made), "dir"};
                if (path.equals(FOLDER)) readme(found[0]);
            }
            id = found[0];
            ids.put(path, id);
        }
        return id;
    }
    /** What the folder is, for whoever finds it. Best effort. */
    private void readme(String folder) {
        try {
            put(folder, "README.txt", ("These are copies of your pocket-ui saves and save states, one folder per game.\n"
                + "They stay when pocket-ui is removed. Install pocket-ui again and choose the folder that holds this one:\n"
                + "the saves come back by themselves. Do not rename or edit the files in here.\n").getBytes(StandardCharsets.UTF_8));
            put(folder, ".nomedia", new byte[0]); // the pictures of the save slots stay out of the gallery
        } catch (IOException | RuntimeException e) { android.util.Log.i(TAG, "save copy: no readme: " + e); }
    }

    private void put(String folderId, String name, byte[] bytes) throws IOException {
        try (OutputStream out = open(folderId, name)) { out.write(bytes); }
    }
    /** An output stream that replaces the file `name` in the folder, made when there is none. */
    private OutputStream open(String folderId, String name) throws IOException {
        String[] found = child(folderId, name);
        Uri file;
        if (found == null) {
            file = DocumentsContract.createDocument(resolver, doc(folderId), BINARY, name);
            if (file == null) throw new IOException(name + " could not be made");
            String[] back = child(folderId, name);
            if (back == null) { try { DocumentsContract.deleteDocument(resolver, file); } catch (Exception ignored) {} throw new IOException("The folder renamed " + name); }
        } else if (found[1].equals("dir")) throw new IOException(name + " is a folder");
        else file = doc(found[0]);
        OutputStream out;
        try { out = resolver.openOutputStream(file, "wt"); } catch (Exception e) { out = resolver.openOutputStream(file, "w"); }
        if (out == null) throw new IOException("The folder gave no file to write");
        return out;
    }

    /** The names of the folders in `pocket-ui saves`; "ok\n[…]". Starts a run: forgets what was looked up. */
    String games() {
        ids.clear();
        try {
            String root = folder(new String[]{FOLDER}, false);
            JSONArray names = new JSONArray();
            if (root != null) try (Cursor c = resolver.query(children(root), new String[]{DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                    DocumentsContract.Document.COLUMN_MIME_TYPE}, null, null, null)) {
                if (c != null) while (c.moveToNext()) if (DocumentsContract.Document.MIME_TYPE_DIR.equals(c.getString(1))) names.put(c.getString(0));
            }
            return "ok\n" + names;
        } catch (Exception e) { return failure(e); }
    }

    /** "ok\n<text>" or "ok" with nothing after it when the game has no list. */
    String manifest(String dir) {
        try {
            String folder = folder(new String[]{FOLDER, dir}, false);
            String[] found = folder == null ? null : child(folder, "copy.json");
            if (found == null) return "ok";
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            try (InputStream in = resolver.openInputStream(doc(found[0]))) {
                if (in == null) throw new FileNotFoundException("copy.json can't be opened");
                byte[] block = new byte[1 << 14];
                for (int n; (n = in.read(block)) > 0; ) {
                    if (out.size() + n > (8 << 20)) throw new IOException("The list is too long");
                    out.write(block, 0, n);
                }
            }
            return "ok\n" + new String(out.toByteArray(), StandardCharsets.UTF_8);
        } catch (Exception e) { return failure(e); }
    }

    /** Copies the local file into `pocket-ui saves/<dir>/<rel>`. */
    String put(String dir, String rel, String local) {
        try {
            String[] parts = rel.split("/");
            String[] path = new String[2 + parts.length - 1];
            path[0] = FOLDER; path[1] = dir;
            System.arraycopy(parts, 0, path, 2, parts.length - 1);
            String folder = folder(path, true);
            byte[] block = new byte[1 << 16];
            try (InputStream in = new FileInputStream(local); OutputStream out = open(folder, parts[parts.length - 1])) {
                for (int n; (n = in.read(block)) > 0; ) out.write(block, 0, n);
            }
            return "ok";
        } catch (Exception e) { return failure(e); }
    }

    String putManifest(String dir, String text) {
        try {
            String folder = folder(new String[]{FOLDER, dir}, true);
            put(folder, "copy.json", text.getBytes(StandardCharsets.UTF_8));
            return "ok";
        } catch (Exception e) { return failure(e); }
    }

    /** Copies `pocket-ui saves/<dir>/<rel>` into the local file. */
    String get(String dir, String rel, String local) {
        try {
            String[] parts = rel.split("/");
            String[] path = new String[2 + parts.length - 1];
            path[0] = FOLDER; path[1] = dir;
            System.arraycopy(parts, 0, path, 2, parts.length - 1);
            String folder = folder(path, false);
            String[] found = folder == null ? null : child(folder, parts[parts.length - 1]);
            if (found == null || found[1].equals("dir")) throw new FileNotFoundException(rel + " is not in the copy");
            byte[] block = new byte[1 << 16];
            try (InputStream in = resolver.openInputStream(doc(found[0])); OutputStream out = new FileOutputStream(local)) {
                if (in == null) throw new FileNotFoundException(rel + " can't be opened");
                for (int n; (n = in.read(block)) > 0; ) out.write(block, 0, n);
            }
            return "ok";
        } catch (Exception e) { return failure(e); }
    }
}
