package xyz.pocketui.app;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;

/**
 * The handheld view's way to give another emulator app a game it holds by its real path: a read-only content URI for the file
 * where it lies, nothing copied. Only files below /storage (internal storage and cards) are served, only to an app the view
 * grants the URI to with the start's intent, and the URI ends in the file's own name so an app that reads its type from it can.
 * Not exported; the view has All files access, which is what lets this process read the file at all.
 */
public final class RomProvider extends ContentProvider {
    static String authority(android.content.Context context) { return context.getPackageName() + ".roms"; }
    /** content://<package>.roms/<absolute path> */
    static Uri uriOf(android.content.Context context, File file) {
        return new Uri.Builder().scheme("content").authority(authority(context)).encodedPath(Uri.encode(file.getAbsolutePath(), "/")).build();
    }
    private File fileOf(Uri uri) throws FileNotFoundException {
        try {
            File f = new File(uri.getPath()).getCanonicalFile();
            if (!f.getPath().startsWith("/storage/") || !f.isFile() || !f.canRead()) throw new FileNotFoundException("not a game file");
            return f;
        } catch (java.io.IOException e) { throw new FileNotFoundException(String.valueOf(e.getMessage())); }
    }
    @Override public boolean onCreate() { return true; }
    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) throw new FileNotFoundException("read only");
        return ParcelFileDescriptor.open(fileOf(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }
    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        File f;
        try { f = fileOf(uri); } catch (FileNotFoundException e) { return null; }
        String[] columns = projection != null ? projection : new String[]{OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        MatrixCursor cursor = new MatrixCursor(columns);
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) row[i] = OpenableColumns.DISPLAY_NAME.equals(columns[i]) ? f.getName() : OpenableColumns.SIZE.equals(columns[i]) ? (Object) f.length() : null;
        cursor.addRow(row);
        return cursor;
    }
    @Override public String getType(Uri uri) { return "application/octet-stream"; }
    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException("read only"); }
    @Override public int delete(Uri uri, String selection, String[] args) { return 0; }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { return 0; }
}
