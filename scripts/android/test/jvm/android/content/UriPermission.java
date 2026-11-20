package android.content;import android.net.Uri;
public final class UriPermission {private final Uri uri;public UriPermission(Uri uri){this.uri=uri;}public Uri getUri(){return uri;}public boolean isReadPermission(){return true;}public boolean isWritePermission(){return true;}}
