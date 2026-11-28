package xyz.pocketui.app;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;
import android.os.Binder;
import android.os.Bundle;
import moe.shizuku.api.BinderContainer;

/**
 * Android extras (docs/ANDROID-EXTRAS.md), in the `:extras` process with `PadWatch`: Shizuku's server hands its binder here
 * ("sendBinder", authority `<package>.shizuku`, as Shizuku-API's ShizukuProvider), and pocket-ui's other processes (the
 * handheld view, the bug report) ask through it for what needs that binder. Exported for Shizuku's server, which runs as the
 * shell or root; the permission in the manifest keeps other apps out, and our own methods answer only our own uid.
 */
public class ExtrasProvider extends ContentProvider {
    static final String SEND_BINDER = "sendBinder";
    private static final String EXTRA_BINDER = "moe.shizuku.privileged.api.intent.extra.BINDER";

    static Uri uri(android.content.Context c) {
        return Uri.parse("content://" + c.getPackageName() + ".shizuku");
    }

    @Override public boolean onCreate() {
        return true;
    }

    @Override public Bundle call(String method, String arg, Bundle extras) {
        Bundle reply = new Bundle();
        if (SEND_BINDER.equals(method)) {
            if (extras == null) return reply;
            extras.setClassLoader(BinderContainer.class.getClassLoader());
            BinderContainer container = extras.getParcelable(EXTRA_BINDER);
            if (container != null && container.binder != null) ShizukuLink.received(container.binder, getContext().getPackageName());
            return reply;
        }
        if (Binder.getCallingUid() != android.os.Process.myUid()) return reply;
        switch (method) {
            case "shizuku-state": reply.putString("v", ShizukuLink.state(getContext())); break;
            case "shizuku-request": reply.putString("v", ShizukuLink.request()); break;
            case "shell": {
                String[] cmd = extras == null ? null : extras.getStringArray("cmd");
                long timeout = extras == null ? 5000 : extras.getLong("timeout", 5000);
                reply.putString("v", cmd == null ? null : ShizukuLink.run(cmd, timeout));
                break;
            }
            default: break;
        }
        return reply;
    }

    @Override public Cursor query(Uri u, String[] p, String s, String[] a, String o) { return null; }
    @Override public String getType(Uri u) { return null; }
    @Override public Uri insert(Uri u, ContentValues v) { return null; }
    @Override public int delete(Uri u, String s, String[] a) { return 0; }
    @Override public int update(Uri u, ContentValues v, String s, String[] a) { return 0; }
}
