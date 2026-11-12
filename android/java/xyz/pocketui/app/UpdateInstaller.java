package xyz.pocketui.app;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.Uri;
import android.os.Build;
import android.os.SystemClock;
import android.provider.Settings;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;
import java.util.*;

/** Signed offers come from Rust's shared channel parser. Android owns Wi-Fi and package replacement. */
public final class UpdateInstaller {
    static final int ALLOW_INSTALL = 44;
    private static final String WIFI = "Connect to Wi-Fi to download updates. Nothing was installed.";
    private static final String DAMAGED = "The download is damaged or was changed. Nothing was installed.";
    private static final String SIGNER = "This APK has a different signing key. Nothing was installed.";
    private static final String CANCELLED = "Cancelled. Nothing was installed.";
    private static final String NETWORK = "Couldn't reach the update server. Check Wi-Fi.";
    private static UpdateInstaller instance;
    private final Context context;
    private volatile String phase = "idle", message = "";
    private volatile long bytes, total;
    private volatile boolean cancelled;
    private volatile HttpURLConnection connection;
    private volatile File ready;
    private volatile int sessionId = -1;
    private volatile boolean committed;
    private final File installedNotice;
    private String readyVersion;

    private UpdateInstaller(Context context) {
        this.context = context.getApplicationContext();
        installedNotice = new File(context.getCacheDir(), "pocket-update-version");
    }
    static synchronized UpdateInstaller get(Context context) {
        if (instance == null) instance = new UpdateInstaller(context);
        return instance;
    }
    static boolean qa(Context c) {
        return (c.getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0 && c.getPackageName().startsWith("xyz.pocketui.qa.");
    }
    static String channel(Activity a) {
        String url = qa(a) ? a.getIntent().getStringExtra("updateUrl") : null;
        return url == null || url.isEmpty() ? "https://pocketui.xyz/updates/android/update.json" : url;
    }
    synchronized String takeInstalledVersion() {
        if (!installedNotice.isFile() || installedNotice.length() > 64) return "";
        try (DataInputStream in = new DataInputStream(new FileInputStream(installedNotice))) {
            byte[] value = new byte[(int)installedNotice.length()];
            in.readFully(value);
            String offered = new String(value, "UTF-8");
            if (!offered.equals(version(context))) return "";
            return installedNotice.delete() ? offered : "";
        } catch (IOException e) { return ""; }
    }
    @SuppressWarnings("deprecation") static String version(Context c) {
        try { return c.getPackageManager().getPackageInfo(c.getPackageName(), 0).versionName; }
        catch (Exception e) { return ""; }
    }
    private Network wifi() throws IOException { return wifi(false); }
    private Network wifi(boolean offlineFixture) throws IOException {
        ConnectivityManager cm = (ConnectivityManager)context.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (!offlineFixture) for (Network n : cm.getAllNetworks()) if (onWifi(n)) return n;
        throw new IOException(WIFI);
    }
    private boolean onWifi(Network n) {
        ConnectivityManager cm = (ConnectivityManager)context.getSystemService(Context.CONNECTIVITY_SERVICE);
        NetworkCapabilities c = cm.getNetworkCapabilities(n);
        return c != null && c.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) && !c.hasTransport(NetworkCapabilities.TRANSPORT_VPN);
    }
    private void check(Network n, boolean cancellable) throws IOException {
        if (cancellable && cancelled) throw new IOException(CANCELLED);
        if (!onWifi(n)) throw new IOException(WIFI);
    }
    private URL address(String value) throws IOException {
        URL u = new URL(value);
        if (!("https".equals(u.getProtocol()) || (qa(context) && "http".equals(u.getProtocol()))) || u.getHost().isEmpty() || u.getUserInfo() != null || u.getRef() != null)
            throw new IOException("The update server sent an address that isn't https. Nothing was changed.");
        return u;
    }
    private HttpURLConnection open(Network n, String value, long limit, boolean cancellable) throws IOException {
        URL u = address(value);
        for (int hop = 0; hop < 6; hop++) {
            check(n, cancellable);
            HttpURLConnection c = (HttpURLConnection)n.openConnection(u);
            if (cancellable) connection = c;
            c.setConnectTimeout(10000); c.setReadTimeout(10000); c.setInstanceFollowRedirects(false);
            c.setRequestProperty("Accept-Encoding", "identity"); c.setRequestProperty("Cache-Control", "no-cache");
            c.setRequestProperty("User-Agent", "pocket-ui-android");
            try {
                int status = c.getResponseCode();
                if (status >= 300 && status < 400) {
                    String next = c.getHeaderField("Location");
                    if (next == null) throw new IOException(NETWORK);
                    u = address(new URL(u, next).toString()); c.disconnect(); continue;
                }
                if (status == 404) throw new IOException("No updates published yet. A new version installs like the first one; your games and settings stay.");
                if (status != 200) throw new IOException(NETWORK);
                if (c.getContentLengthLong() > limit) throw new IOException(DAMAGED);
                return c;
            } catch (IOException e) { c.disconnect(); throw e; }
        }
        throw new IOException(NETWORK);
    }
    /** Worker-only small signed manifest / signature read, bounded before allocating. */
    String read(String url, String maximum) {
        HttpURLConnection c = null;
        try {
            long limit = Long.parseLong(maximum);
            if (limit <= 0 || limit > 1048576) throw new IOException(DAMAGED);
            Network n = wifi(); c = open(n, url, limit, false);
            long deadline = SystemClock.elapsedRealtime() + 60000;
            try (InputStream in = c.getInputStream(); ByteArrayOutputStream out = new ByteArrayOutputStream()) {
                byte[] buf = new byte[8192]; int count;
                while ((count = in.read(buf)) != -1) {
                    check(n, false);
                    if (out.size() + count > limit || SystemClock.elapsedRealtime() > deadline) throw new IOException(DAMAGED);
                    out.write(buf, 0, count);
                }
                return "OK\n" + out.toString("UTF-8");
            }
        } catch (Exception e) { return "ERR\n" + reason(e); }
        finally { if (c != null) c.disconnect(); }
    }
    String status() {
        try { return new JSONObject().put("phase", phase).put("bytes", bytes).put("total", total).put("message", message).put("cancellable", busy() && !committed).toString(); }
        catch (Exception e) { return "{}"; }
    }
    boolean busy() { return Arrays.asList("connecting", "downloading", "checking", "permission", "installing").contains(phase); }
    synchronized String start(Activity a, String request) {
        if (busy()) return "";
        cancelled = false; committed = false; bytes = 0; message = ""; phase = "connecting";
        new Thread(() -> {
            try {
                JSONObject r = new JSONObject(request);
                long size = r.getLong("size"); String hash = r.getString("sha256");
                JSONArray urls = r.getJSONArray("urls");
                if (size <= 0 || size > 100000000 || !hash.matches("[0-9a-fA-F]{64}") || urls.length() == 0 || urls.length() > 4) throw new IOException(DAMAGED);
                total = size;
                // A debuggable QA identity can exercise the same no-Wi-Fi failure without disconnecting ADB.
                Network n = wifi(qa(a) && a.getIntent().getBooleanExtra("updateOffline", false));
                if (context.getCacheDir().getUsableSpace() < size * 2 + 32 * 1024 * 1024) throw new IOException("There isn't enough space for this update. Nothing was installed.");
                File file = new File(context.getCacheDir(), "pocket-update.apk.part");
                ready = file;
                Exception failure = null;
                for (int i = 0; i < urls.length(); i++) {
                    check(n, true); phase = "connecting"; bytes = 0;
                    try { download(n, urls.getString(i), file, size, hash); failure = null; break; }
                    catch (Exception e) { failure = e; file.delete(); check(n, true); }
                }
                if (failure != null) throw failure;
                phase = "checking"; check(n, true);
                validate(file, r.getString("version"));
                readyVersion = r.getString("version");
                check(n, true);
                if (!context.getPackageManager().canRequestPackageInstalls()) {
                    phase = "permission";
                    message = "Allow pocket-ui to install updates, then return here.";
                    a.runOnUiThread(() -> {
                        if (cancelled) return;
                        try { a.startActivityForResult(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + context.getPackageName())), ALLOW_INSTALL); }
                        catch (Exception e) { fail("Android couldn't open the update installer. Nothing was installed."); }
                    });
                } else install(a);
            } catch (Exception e) { fail(cancelled ? CANCELLED : reason(e)); }
        }, "pocket-update").start();
        return "";
    }
    private void download(Network n, String url, File file, long size, String hash) throws Exception {
        HttpURLConnection c = open(n, url, size, true);
        try {
            MessageDigest sha = MessageDigest.getInstance("SHA-256");
            long deadline = SystemClock.elapsedRealtime() + 300000 + size * 1000 / 10240;
            phase = "downloading";
            try (InputStream in = c.getInputStream(); FileOutputStream out = new FileOutputStream(file, false)) {
                byte[] buf = new byte[65536]; int count;
                while ((count = in.read(buf)) != -1) {
                    check(n, true);
                    if (bytes + count > size) throw new IOException(DAMAGED);
                    if (SystemClock.elapsedRealtime() > deadline) throw new IOException("The download is too slow on this network. Nothing was installed. Try again on a faster Wi-Fi.");
                    out.write(buf, 0, count); sha.update(buf, 0, count); bytes += count;
                }
                out.getFD().sync();
            }
            phase = "checking";
            if (bytes != size || !hex(sha.digest()).equalsIgnoreCase(hash)) throw new IOException(DAMAGED);
        } finally { c.disconnect(); connection = null; }
    }
    static String hex(byte[] value) {
        StringBuilder out = new StringBuilder(); for (byte b : value) out.append(String.format(java.util.Locale.ROOT, "%02x", b & 255)); return out.toString();
    }
    @SuppressWarnings("deprecation") private void validate(File file, String version) throws Exception {
        PackageManager pm = context.getPackageManager();
        int flags = Build.VERSION.SDK_INT >= 28 ? PackageManager.GET_SIGNING_CERTIFICATES : PackageManager.GET_SIGNATURES;
        PackageInfo old = pm.getPackageInfo(context.getPackageName(), flags);
        PackageInfo apk = pm.getPackageArchiveInfo(file.getAbsolutePath(), flags);
        if (apk == null || !context.getPackageName().equals(apk.packageName) || !version.equals(apk.versionName))
            throw new IOException("This APK isn't the offered pocket-ui update. Nothing was installed.");
        Signature[] a = signers(old), b = signers(apk);
        if (a == null || b == null || a.length == 0 || a.length != b.length || !new HashSet<>(Arrays.asList(a)).equals(new HashSet<>(Arrays.asList(b)))) throw new IOException(SIGNER);
        long current = Build.VERSION.SDK_INT >= 28 ? old.getLongVersionCode() : old.versionCode;
        long candidate = Build.VERSION.SDK_INT >= 28 ? apk.getLongVersionCode() : apk.versionCode;
        if (candidate <= current) throw new IOException("This update is older than the installed app. Nothing was installed.");
    }
    @SuppressWarnings("deprecation") private static Signature[] signers(PackageInfo p) {
        return Build.VERSION.SDK_INT >= 28 ? p.signingInfo == null ? null : p.signingInfo.getApkContentsSigners() : p.signatures;
    }
    void permissionResult(Activity a) {
        if (!"permission".equals(phase)) return;
        if (!a.getPackageManager().canRequestPackageInstalls()) { fail("Update permission wasn't granted. Nothing was installed."); return; }
        new Thread(() -> { try { install(a); } catch (Exception e) { fail(cancelled ? CANCELLED : reason(e)); } }, "pocket-install").start();
    }
    private void install(Activity a) throws Exception {
        synchronized (this) {
            if (cancelled || ready == null) throw new IOException(CANCELLED);
            phase = "installing"; message = "Confirm in Android. pocket-ui closes afterwards; open it again.";
        }
        PackageInstaller installer = context.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(context.getPackageName()); params.setSize(total);
        if (Build.VERSION.SDK_INT >= 31) params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED);
        synchronized (this) { if (cancelled) throw new IOException(CANCELLED); sessionId = installer.createSession(params); }
        try (PackageInstaller.Session session = installer.openSession(sessionId)) {
            try (InputStream in = new FileInputStream(ready); OutputStream out = session.openWrite("base.apk", 0, total)) {
                byte[] buf = new byte[65536]; int count;
                while ((count = in.read(buf)) != -1) { if (cancelled) throw new IOException(CANCELLED); out.write(buf, 0, count); }
                session.fsync(out);
            }
            synchronized (this) {
                if (cancelled) throw new IOException(CANCELLED);
                try (FileOutputStream marker = new FileOutputStream(installedNotice)) { marker.write(readyVersion.getBytes("UTF-8")); marker.getFD().sync(); }
                Intent result = new Intent(context, ResultReceiver.class).setAction(context.getPackageName() + ".UPDATE_RESULT").putExtra("updateSession", sessionId);
                int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
                PendingIntent callback = PendingIntent.getBroadcast(context, sessionId, result, flags);
                session.commit(callback.getIntentSender()); committed = true;
                ready.delete(); ready = null;
            }
        }
    }
    synchronized String cancel() {
        if (!busy()) return "";
        // Android owns a committed install. Its confirmation has its own Cancel button.
        if (committed) return "";
        cancelled = true;
        HttpURLConnection c = connection; if (c != null) c.disconnect();
        if ("permission".equals(phase)) fail(CANCELLED);
        return "";
    }
    synchronized void fail(String why) {
        if (sessionId >= 0) { try { context.getPackageManager().getPackageInstaller().abandonSession(sessionId); } catch (Exception ignored) {} sessionId = -1; }
        if (ready != null) { ready.delete(); ready = null; }
        installedNotice.delete(); phase = CANCELLED.equals(why) ? "cancelled" : "failed"; message = why;
    }
    private String reason(Exception e) {
        String s = e.getMessage();
        // Only known static words enter the UI. Transport exceptions may contain URLs or local paths.
        Set<String> safe = new HashSet<>(Arrays.asList(WIFI, DAMAGED, SIGNER, CANCELLED, NETWORK,
            "The update server sent an address that isn't https. Nothing was changed.",
            "No updates published yet. A new version installs like the first one; your games and settings stay.",
            "There isn't enough space for this update. Nothing was installed.",
            "The download is too slow on this network. Nothing was installed. Try again on a faster Wi-Fi.",
            "This APK isn't the offered pocket-ui update. Nothing was installed.",
            "This update is older than the installed app. Nothing was installed."));
        return safe.contains(s) ? s : "installing".equals(phase) ? "Android couldn't install this update. Nothing was installed." : NETWORK;
    }
    public static final class ResultReceiver extends BroadcastReceiver {
        @Override public void onReceive(Context c, Intent intent) {
            UpdateInstaller u = get(c);
            int requested = intent.getIntExtra("updateSession", -1);
            int actual = intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, -1);
            if (requested < 0 || actual != requested || (u.sessionId >= 0 && u.sessionId != requested)) return;
            int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
            if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
                try { if (confirm == null) throw new IOException(); confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK); c.startActivity(confirm); }
                catch (Exception e) { u.fail("Android couldn't open the update installer. Nothing was installed."); }
            } else if (status == PackageInstaller.STATUS_SUCCESS) {
                u.phase = "done";
            } else { u.fail(status == PackageInstaller.STATUS_FAILURE_ABORTED ? CANCELLED : "Android couldn't install this update. Nothing was installed."); }
        }
    }
}
