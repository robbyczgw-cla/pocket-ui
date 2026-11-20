package android.database;
public interface Cursor extends AutoCloseable { boolean moveToFirst(); boolean moveToNext(); boolean isNull(int i); long getLong(int i); String getString(int i); void close(); }
