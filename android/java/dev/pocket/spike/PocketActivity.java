package dev.pocket.spike;

import android.app.NativeActivity;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.ClipData;
import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.DocumentsContract;
import android.media.AudioManager;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.net.*;
import java.util.*;

/** Framework I/O only: the Rust library owns matching, drawing and navigation. */
public class PocketActivity extends NativeActivity {
    // NativeActivity dlopen alone does not register JNI methods with this Java class loader.
    static { System.loadLibrary("pocket_android"); }
    private static final int PICK_TREE = 41;
    private static final String TAG = "PocketMvp";
    private int stickDirection;
    private volatile boolean audioFocus;
    private android.media.AudioFocusRequest focusRequest;
    private volatile String launchStatus = "";
    private final java.util.concurrent.atomic.AtomicBoolean launching = new java.util.concurrent.atomic.AtomicBoolean();

    private SemanticView semantics;
    private native boolean nativeBack();
    private void backRequested(){android.util.Log.i(TAG,"launcher system back");if(!nativeBack())backRequests.incrementAndGet();}
    private final java.util.concurrent.atomic.AtomicInteger backRequests=new java.util.concurrent.atomic.AtomicInteger();
    private volatile String metrics = "1,1,0,0,0,0";
    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        semantics = new SemanticView(this);
        addContentView(semantics, new android.view.ViewGroup.LayoutParams(-1,-1));
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        android.view.Window window = getWindow();
        if (android.os.Build.VERSION.SDK_INT >= 28) {
            android.view.WindowManager.LayoutParams attrs = window.getAttributes();
            attrs.layoutInDisplayCutoutMode = android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(attrs);
        }
        if (android.os.Build.VERSION.SDK_INT >= 30) window.setDecorFitsSystemWindows(false);
        window.getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            int[] safe=SafeInsets.read(this,insets,false);
            metrics=getResources().getDisplayMetrics().density+","+getResources().getConfiguration().fontScale+","+safe[0]+","+safe[1]+","+safe[2]+","+safe[3];
            return insets;
        });
        if(android.os.Build.VERSION.SDK_INT>=33)getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,this::backRequested);
        immersive();
    }
    @SuppressWarnings("deprecation") private void immersive() {
        getWindow().getDecorView().setSystemUiVisibility(5894 | 1024 | 512 | 256);
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            android.view.WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.hide(android.view.WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_DEFAULT);
            }
        }
    }
    @Override public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused); if (focused) immersive();
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event){
        // NativeActivity can finish on an unhandled physical Back before its legacy callback runs.
        // Consume both halves here; gestures still arrive through OnBackInvokedDispatcher.
        if(event.getKeyCode()==KeyEvent.KEYCODE_BACK){if(event.getAction()==KeyEvent.ACTION_DOWN&&event.getRepeatCount()==0)backRequested();return true;}
        return super.dispatchKeyEvent(event);
    }
    @Override public void onBackPressed(){backRequested();}
    public String navigationEvents(){return backRequests.getAndSet(0)>0?"back":"";}
    public String publishSemantics(String json) { runOnUiThread(() -> semantics.publish(json)); return ""; }
    public String semanticActions() { return semantics == null ? "" : semantics.actions(); }
    public String windowMetrics() { return metrics; }
    /** The idle slideshow keeps the screen on while it can run (it ends after ten minutes by itself). */
    public String keepScreenOn(String on) {
        runOnUiThread(() -> {
            if ("true".equals(on)) getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            else getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        });
        return "";
    }
    public String externalPlayer() { return Boolean.toString(getIntent().getBooleanExtra("externalPlayer",false)); }
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);setIntent(intent);}
    public String diagnosticAudio() { return Boolean.toString(getIntent().getBooleanExtra("diagnosticAudio",false)); }
    public String language() { return Locale.getDefault().getLanguage(); }
    public String dataPath() { return getFilesDir().getAbsolutePath(); }
    /** Ordered roots; upgrade the original single-root preference without touching its grant. */
    private synchronized JSONArray roots() {
        String saved = getPreferences(0).getString("trees", "");
        if (!saved.isEmpty()) {
            try { return new JSONArray(saved); }
            catch (org.json.JSONException e) { android.util.Log.w(TAG, "Invalid saved roots; recovering original selection", e); }
        }
        JSONArray roots = new JSONArray();
        String legacy = getPreferences(0).getString("tree", "");
        if (!legacy.isEmpty()) roots.put(legacy);
        if (!getPreferences(0).edit().putString("trees", roots.toString()).commit())
            android.util.Log.w(TAG, "Could not persist root migration; original selection remains intact");
        return roots;
    }
    public String selectedRoots() { return roots().toString(); }
    public String deviceControls() {
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice device = InputDevice.getDevice(id);
            if (device == null || device.isVirtual()) continue;
            int source = device.getSources();
            if ((source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                    || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) return "controller";
        }
        return "touch";
    }
    public String unmeteredWifi() {
        android.net.ConnectivityManager manager = (android.net.ConnectivityManager)getSystemService(Context.CONNECTIVITY_SERVICE);
        if (manager == null) return "false";
        android.net.NetworkCapabilities caps = manager.getNetworkCapabilities(manager.getActiveNetwork());
        return Boolean.toString(caps != null && caps.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI)
                && caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
                && caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_INTERNET));
    }
    public String librarySelection() {
        return getPreferences(0).getString("tree", "") + "\n" + getPreferences(0).getInt("revision", 0);
    }
    public String chooseFolder() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            String selected = getPreferences(0).getString("tree", "");
            if (!selected.isEmpty()) intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI, Uri.parse(selected));
            startActivityForResult(intent, PICK_TREE);
        });
        return "";
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_TREE || result != RESULT_OK || data == null || data.getData() == null) return;
        Uri tree = data.getData();
        try {
            int flags = data.getFlags() & Intent.FLAG_GRANT_READ_URI_PERMISSION;
            if (flags == 0) throw new SecurityException("The folder did not grant read access");
            getContentResolver().takePersistableUriPermission(tree, flags);
            JSONArray roots = roots();
            boolean exists = false;
            for (int i = 0; i < roots.length(); i++) if (tree.toString().equals(roots.getString(i))) exists = true;
            if (!exists) roots.put(tree.toString());
            // Add atomically, retaining all older persisted permissions. Cancelling changes nothing.
            if (!getPreferences(0).edit().putString("tree", tree.toString()).putString("trees", roots.toString())
                    .putInt("revision", getPreferences(0).getInt("revision", 0) + 1).commit())
                throw new IOException("Could not save the folder selection");
            android.util.Log.i(TAG, "persisted tree=" + tree);
        } catch (Exception e) { error("Folder unavailable", e); }
    }
    private void error(String title, Exception e) {
        android.util.Log.e(TAG, title, e);
        runOnUiThread(() -> new AlertDialog.Builder(this).setTitle(title).setMessage(e.getMessage()).setPositiveButton("OK", null).show());
    }
    /** Worker-only multi-root scan. Stable virtual prefixes isolate identically named disc sheets across roots. */
    public String scanLibraries(String ignored) throws Exception {
        JSONArray roots = roots(), files = new JSONArray();
        Exception failure = null;
        int scanned = 0;
        for (int i = 0; i < roots.length(); i++) {
            String root = roots.getString(i);
            try {
                JSONArray entries = new JSONArray(scanLibrary(root));
                String prefix = "";
                if (i > 0) {
                    byte[] digest = java.security.MessageDigest.getInstance("SHA-256").digest(root.getBytes(java.nio.charset.StandardCharsets.UTF_8));
                    StringBuilder hex = new StringBuilder("@pocket-root-");
                    for (int j = 0; j < 8; j++) hex.append(String.format(Locale.ROOT, "%02x", digest[j] & 255));
                    prefix = hex.toString() + "/";
                }
                for (int j = 0; j < entries.length(); j++) {
                    JSONObject entry = entries.getJSONObject(j);
                    entry.put("relative", prefix + entry.getString("relative"));
                    files.put(entry);
                }
                scanned++;
            } catch (Exception e) {
                failure = e;
                android.util.Log.w(TAG, "SAF root unavailable; other roots remain available", e);
            }
        }
        if (scanned == 0 && failure != null) throw failure;
        android.util.Log.i(TAG, "SAF multi-root scan roots=" + scanned + " entries=" + files.length());
        return files.toString();
    }
    /** Called on a Rust worker with the captured tree, never traverses providers on the UI thread. */
    public String scanLibrary(String selected) throws Exception {
        JSONArray files = new JSONArray();
        if (selected.isEmpty()) return files.toString();
        Uri tree = Uri.parse(selected);
        if (!"content".equals(tree.getScheme()) || !DocumentsContract.isTreeUri(tree)) throw new IOException("Choose a ROM folder again");
        ArrayDeque<String[]> pending = new ArrayDeque<>();
        pending.add(new String[]{DocumentsContract.getTreeDocumentId(tree), "", "0"});
        HashSet<String> visited = new HashSet<>();
        int seen = 0;
        while (!pending.isEmpty()) {
            String[] folder = pending.removeFirst();
            if (!visited.add(folder[0])) continue;
            int depth = Integer.parseInt(folder[2]);
            Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, folder[0]);
            String[] projection = {DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED};
            try (Cursor cursor = getContentResolver().query(children, projection, null, null, null)) {
                if (cursor == null) throw new IOException("Provider did not return the folder");
                while (cursor.moveToNext()) {
                    if (++seen > 50000) throw new IOException("Library exceeds 50,000 entries; choose a smaller folder");
                    String id = cursor.getString(0), name = cursor.getString(1), mime = cursor.getString(2);
                    if (name == null || name.equals(".") || name.equals("..") || name.contains("/") || name.contains("\\")) continue;
                    String relative = folder[1] + name;
                    if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mime)) {
                        if (depth >= 32) throw new IOException("Library exceeds 32 folder levels");
                        pending.add(new String[]{id, relative + "/", Integer.toString(depth + 1)});
                    } else {
                        JSONObject entry = new JSONObject();
                        entry.put("uri", DocumentsContract.buildDocumentUriUsingTree(tree, id).toString());
                        entry.put("relative", relative);
                        entry.put("id", id);
                        entry.put("authority", tree.getAuthority());
                        if (name.toLowerCase(Locale.ROOT).matches(".*\\.(m3u|cue|gdi)$")) {
                            Uri document = DocumentsContract.buildDocumentUriUsingTree(tree, id);
                            try (InputStream in = getContentResolver().openInputStream(document)) {
                                ByteArrayOutputStream out = new ByteArrayOutputStream();
                                byte[] bytes = new byte[4096]; int n;
                                while (out.size() <= 262144 && (n = in.read(bytes)) != -1) out.write(bytes, 0, n);
                                if (out.size() > 262144) throw new IOException("Disc sheet too large: " + relative);
                                entry.put("sheet", out.toString("UTF-8"));
                            }
                        }
                        entry.put("size", cursor.isNull(3) ? -1 : cursor.getLong(3));
                        entry.put("modified", cursor.isNull(4) ? 0 : cursor.getLong(4));
                        files.put(entry);
                    }
                }
            }
        }
        android.util.Log.i(TAG, "SAF scan entries=" + files.length());
        return files.toString();
    }
    public String volumeState() {
        AudioManager audio = (AudioManager)getSystemService(Context.AUDIO_SERVICE);
        int max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
        int value = audio.getStreamVolume(AudioManager.STREAM_MUSIC);
        return (max == 0 ? 0 : value * 100 / max) + "," + (audio.isStreamMute(AudioManager.STREAM_MUSIC) || !audioFocus);
    }
    @SuppressWarnings("deprecation") public String speakerState() {
        AudioManager audio = (AudioManager)getSystemService(Context.AUDIO_SERVICE);
        return Boolean.toString(!audio.isBluetoothA2dpOn());
    }
    /** Android's TLS trust store; redirects remain HTTPS; bounded responses and worker-only disk writes. */
    public String httpGet(String address, String destination, String maximum) throws Exception {
        long limit = Long.parseLong(maximum);
        if (limit <= 0 || limit > 32 * 1024 * 1024) throw new IOException("Invalid download limit");
        URL url = new URL(address);
        for (int hop = 0; hop < 6; hop++) {
            if (!"https".equals(url.getProtocol())) throw new IOException("HTTPS required");
            HttpURLConnection connection = (HttpURLConnection)url.openConnection();
            connection.setConnectTimeout(10000); connection.setReadTimeout(15000); connection.setInstanceFollowRedirects(false);
            connection.setRequestProperty("User-Agent", "pocket-ui-android/0.1");
            try {
                int status = connection.getResponseCode();
                if (status >= 300 && status < 400) {
                    String location = connection.getHeaderField("Location");
                    if (location == null) throw new IOException("Redirect without a destination");
                    url = new URL(url, location); continue;
                }
                if (status != 200) throw new IOException("HTTP " + status);
                if (connection.getContentLengthLong() > limit) throw new IOException("Download too large");
                ByteArrayOutputStream text = destination.isEmpty() ? new ByteArrayOutputStream() : null;
                try (InputStream in = connection.getInputStream(); OutputStream out = text != null ? text : new FileOutputStream(destination)) {
                    byte[] buffer = new byte[16384]; long total = 0; long deadline = android.os.SystemClock.elapsedRealtime() + 60000;
                    int n;
                    while ((n = in.read(buffer)) != -1) {
                        if ((total += n) > limit || android.os.SystemClock.elapsedRealtime() > deadline) throw new IOException("Download limit reached");
                        out.write(buffer, 0, n);
                    }
                }
                return "OK\n" + (text == null ? "" : text.toString("UTF-8"));
            } finally { connection.disconnect(); }
        }
        throw new IOException("Too many redirects");
    }
    /** Stage one cartridge on a worker, then enter our own EGL player. URI identity owns its saves. */
    public String launchInternal(String document,String relative,String core,String key,String title,String system,String settings) {
        if (!launching.compareAndSet(false,true)) return "Already opening a game";
        new Thread(()->{
            try {
                if (!core.matches("[a-z][a-z0-9_]{0,39}") || !new File(getApplicationInfo().nativeLibraryDir,"lib"+core+"_libretro.so").isFile())
                    throw new IOException("Core not bundled: " + core);
                if (!key.matches("[a-f0-9]{16}")) throw new IOException("Invalid game identity");
                File folder=new File(getFilesDir(),"content/"+key); if(!folder.mkdirs()&&!folder.isDirectory())throw new IOException("Cannot prepare content");
                String name=relative.substring(relative.lastIndexOf('/')+1);
                if(name.isEmpty()||name.equals(".")||name.equals("..")||name.contains("\\"))throw new IOException("Invalid ROM name");
                File rom=new File(folder,name), part=new File(folder,name+".part");
                try(InputStream in=getContentResolver().openInputStream(Uri.parse(document));OutputStream out=new FileOutputStream(part)) {
                    byte[] bytes=new byte[65536];long total=0;int n;
                    while((n=in.read(bytes))!=-1){if((total+=n)>128*1024*1024)throw new IOException("Cartridge exceeds 128 MiB");out.write(bytes,0,n);}
                }
                if(!part.renameTo(rom))throw new IOException("Cannot finish content copy");
                Intent player=new Intent(this,PlayerActivity.class).putExtra("core",getApplicationInfo().nativeLibraryDir+"/lib"+core+"_libretro.so").putExtra("rom",rom.getAbsolutePath()).putExtra("system",system).putExtra("key",key).putExtra("title",title).putExtra("diagnosticAudio",getIntent().getBooleanExtra("diagnosticAudio",false)).putExtra("settings",settings);
                runOnUiThread(()->{try{startActivity(player);}finally{launching.set(false);}});
            }catch(Exception e){launching.set(false);error("Game couldn't start",e);}
        },"pocket-content").start();return "";
    }
    public String launchState() { return launchStatus; }
    /** Called by Rust with a profile id and document URI. No URI is converted into a fake filesystem path. */
    public String launchGame(String document, String relative, String profile) {
        if (!launching.compareAndSet(false, true)) return "Already opening a game";
        launchStatus = "Opening game...";
        new Thread(() -> {
            try {
                Intent intent = EmulatorIntents.create(this, Uri.parse(document), relative, profile);
                runOnUiThread(() -> {
                    try { startActivity(intent); launchStatus = ""; android.util.Log.i(TAG, "launch profile=" + profile + " ROM=" + document); }
                    catch (Exception e) { launchStatus = e.getMessage(); error("Emulator couldn't start", e); }
                    finally { launching.set(false); }
                });
            } catch (Exception e) { launchStatus = e.getMessage(); launching.set(false); error("Emulator couldn't start", e); }
        }, "pocket-launch").start();
        return "";
    }
    // Convert stick/hat transitions once, leaving repeat and navigation to Android key dispatch.
    @Override public boolean onGenericMotionEvent(MotionEvent event) {
        if ((event.getSource() & InputDevice.SOURCE_JOYSTICK) != 0 && event.getAction() == MotionEvent.ACTION_MOVE) {
            float y = event.getAxisValue(MotionEvent.AXIS_HAT_Y);
            if (Math.abs(y) < .5f) y = event.getAxisValue(MotionEvent.AXIS_Y);
            float x = event.getAxisValue(MotionEvent.AXIS_HAT_X);
            if (Math.abs(x) < .5f) x = event.getAxisValue(MotionEvent.AXIS_X);
            int next = Math.abs(x) > Math.abs(y) ? (x < -.55f ? KeyEvent.KEYCODE_DPAD_LEFT : x > .55f ? KeyEvent.KEYCODE_DPAD_RIGHT : 0) : (y < -.55f ? KeyEvent.KEYCODE_DPAD_UP : y > .55f ? KeyEvent.KEYCODE_DPAD_DOWN : 0);
            if (next != stickDirection) {
                if (stickDirection != 0) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, stickDirection));
                stickDirection = next;
                if (next != 0) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN, next));
            }
            return true;
        }
        return super.onGenericMotionEvent(event);
    }
    @Override protected void onResume() {
        super.onResume();
        immersive();
        AudioManager audio = (AudioManager)getSystemService(Context.AUDIO_SERVICE);
        focusRequest = new android.media.AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
            .setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_GAME)
                .setContentType(android.media.AudioAttributes.CONTENT_TYPE_SONIFICATION).build())
            .setOnAudioFocusChangeListener(change -> audioFocus = change > 0).build();
        audioFocus = audio.requestAudioFocus(focusRequest) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
    }
    @Override protected void onPause() {
        audioFocus = false;
        if (focusRequest != null) ((AudioManager)getSystemService(Context.AUDIO_SERVICE)).abandonAudioFocusRequest(focusRequest);
        if (stickDirection != 0) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, stickDirection));
        stickDirection = 0;
        super.onPause();
    }
}
