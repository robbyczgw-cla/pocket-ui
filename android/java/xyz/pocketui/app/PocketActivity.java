package xyz.pocketui.app;

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
    private static final int PICK_TREE = 41, SAVES_OUT = 42, SAVES_IN = 43;
    private static final String TAG = "PocketMvp";
    private final StickKeys stickKeys = new StickKeys();
    private volatile boolean audioFocus;
    private android.media.AudioFocusRequest focusRequest;
    private volatile String launchStatus = "";
    private final java.util.concurrent.atomic.AtomicBoolean launching = new java.util.concurrent.atomic.AtomicBoolean();

    private SemanticView semantics;
    private native boolean nativeBack();
    private native void nativeConfigurationChanged();
    private void backRequested(){android.util.Log.i(TAG,"launcher system back");if(!nativeBack())backRequests.incrementAndGet();}
    private final java.util.concurrent.atomic.AtomicInteger backRequests=new java.util.concurrent.atomic.AtomicInteger();
    private volatile String metrics = "1,1,0,0,0,0,0";
    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        deviceControls(); // before the first picture: a handheld starts the long way round
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
            String next=getResources().getDisplayMetrics().density+","+getResources().getConfiguration().fontScale+","+safe[0]+","+safe[1]+","+safe[2]+","+safe[3]+","+(statusBarShown(view,insets)?1:0);
            if(!next.equals(metrics)){metrics=next;nativeConfigurationChanged();}
            return insets;
        });
        if(android.os.Build.VERSION.SDK_INT>=33)getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,this::backRequested);
        immersive();
    }
    /** Android's own status bar is on screen (not the immersive mode this launcher asks for): the launcher's clock stays out then. */
    @SuppressWarnings("deprecation") private static boolean statusBarShown(android.view.View view, android.view.WindowInsets insets) {
        if (android.os.Build.VERSION.SDK_INT >= 30) return insets.isVisible(android.view.WindowInsets.Type.statusBars());
        return insets.getSystemWindowInsetTop() > 0 && (view.getWindowSystemUiVisibility() & android.view.View.SYSTEM_UI_FLAG_FULLSCREEN) == 0;
    }
    /** The status corner's facts in one line, "is24,hour,minute,percent,charging" (phone_corner.rs): the system's 12/24-hour choice, the local time, the battery from the sticky ACTION_BATTERY_CHANGED (no permission, no receiver). percent is -1 without a battery. */
    public String statusCorner() {
        boolean is24 = android.text.format.DateFormat.is24HourFormat(this);
        java.util.Calendar now = java.util.Calendar.getInstance();
        int percent = -1, charging = 0;
        Intent battery = registerReceiver(null, new android.content.IntentFilter(Intent.ACTION_BATTERY_CHANGED));
        if (battery != null && battery.getBooleanExtra(android.os.BatteryManager.EXTRA_PRESENT, true)) {
            int level = battery.getIntExtra(android.os.BatteryManager.EXTRA_LEVEL, -1), scale = battery.getIntExtra(android.os.BatteryManager.EXTRA_SCALE, -1);
            if (level >= 0 && scale > 0) percent = Math.min(100, level * 100 / scale);
            charging = battery.getIntExtra(android.os.BatteryManager.EXTRA_STATUS, -1) == android.os.BatteryManager.BATTERY_STATUS_CHARGING ? 1 : 0;
        }
        return (is24 ? 1 : 0) + "," + now.get(java.util.Calendar.HOUR_OF_DAY) + "," + now.get(java.util.Calendar.MINUTE) + "," + percent + "," + charging;
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
    public String updateRead(String url, String limit) { return UpdateInstaller.get(this).read(url, limit); }
    public String updateStart(String request) { return UpdateInstaller.get(this).start(this, request); }
    public String updateStatus() { return UpdateInstaller.get(this).status(); }
    public String updateCancel() { return UpdateInstaller.get(this).cancel(); }
    public String updateChannel() { return UpdateInstaller.channel(this); }
    public String updateVersion() { return UpdateInstaller.version(this); }
    public String updateNotice() { return UpdateInstaller.get(this).takeInstalledVersion(); }
    public String externalPlayer() { return Boolean.toString(getIntent().getBooleanExtra("externalPlayer",false)); }
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);setIntent(intent);}
    public String insertRehearsal() { return Boolean.toString(getIntent().getBooleanExtra("insertRehearsal",false)); }
    /** Where a diagnostic launch leaves its audio capture: the app's own folder on shared storage, which adb reads without root. */
    public String diagnosticsPath(){java.io.File shared=getExternalFilesDir(null);return shared==null?"":shared.getAbsolutePath();}
    public String diagnosticAudio() { return Boolean.toString(getIntent().getBooleanExtra("diagnosticAudio",false)); }
    public String diagnosticAudioSeconds(){return Integer.toString(Math.max(1,Math.min(240,getIntent().getIntExtra("diagnosticAudioSeconds",30))));}
    public String batterySaver(){return Boolean.toString(((android.os.PowerManager)getSystemService(POWER_SERVICE)).isPowerSaveMode());}
    @Override public void onConfigurationChanged(android.content.res.Configuration config){super.onConfigurationChanged(config);getWindow().getDecorView().requestApplyInsets();}
    public String language() { return Locale.getDefault().getLanguage(); }
    private static CoreDownloads coreDownloads;
    private synchronized CoreDownloads coreDownloads(){if(coreDownloads==null)coreDownloads=new CoreDownloads(getApplicationContext());return coreDownloads;}
    public String coreOffers() throws Exception {try(InputStream in=getAssets().open("core-downloads.json");ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] b=new byte[16384];int n;while((n=in.read(b))!=-1)out.write(b,0,n);return out.toString("UTF-8");}}
    /** assets/licences/CORES.tsv: id, systems, licence, upstream, commit, apk|download, patches. Empty when it cannot be read (the page says so). */
    public String licenceList(){try(InputStream in=getAssets().open("licences/CORES.tsv");ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] b=new byte[16384];int n;while((n=in.read(b))!=-1)out.write(b,0,n);return out.toString("UTF-8");}catch(Exception e){return "";}}
    private String requestedCore="";
    public synchronized String coreRequested(){String value=requestedCore;requestedCore="";return value;}
    public String offerCoreDownload(String core,String name,String size,String settings){
        runOnUiThread(()->new AlertDialog.Builder(this)
            .setTitle(inLanguage(settings,"System not installed","System noch nicht installiert","Sistema no instalado"))
            .setMessage(inLanguage(settings,name+" needs an extra download over Wi-Fi. Your games and saves stay where they are.",name+" braucht einen zusätzlichen Download über WLAN. Deine Spiele und Spielstände bleiben erhalten.",name+" necesita una descarga adicional por wifi. Tus juegos y partidas guardadas se conservan."))
            .setPositiveButton(inLanguage(settings,"Download now","Jetzt laden","Descargar ahora")+" · "+size,(dialog,which)->{synchronized(this){requestedCore=core;}})
            .setNegativeButton(inLanguage(settings,"Not now","Nicht jetzt","Ahora no"),null).show());
        return "";
    }
    /** The system download channel: an intent's address (tests), else the one built into the APK (assets/core-channel.url, from android/core-channel.url). Empty: no public channel yet. */
    public String coreCatalogUrl(){String url=getIntent().getStringExtra("coreCatalogUrl");if(url!=null&&!url.isEmpty())return url;
        try(InputStream in=getAssets().open("core-channel.url");ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] b=new byte[1024];int n;while((n=in.read(b))!=-1)out.write(b,0,n);
            for(String line:out.toString("UTF-8").split("\n")){String t=line.trim();if(!t.isEmpty()&&!t.startsWith("#"))return t;}}catch(Exception e){}
        return "";}
    public String coreRead(String url){try{return "OK\n"+coreDownloads().read(url);}catch(Exception e){return "ERR\n"+e.getMessage();}}
    public String coreFetch(String url,String hash,String size){try{return "OK\n"+coreDownloads().fetch(url,hash,size);}catch(Exception e){return "ERR\n"+e.getMessage();}}
    public String coreBegin(){return coreDownloads().begin();}
    public String coreProgress(){return coreDownloads().progress();}
    public String coreCancel(){return coreDownloads().cancel();}
    public String corePath(String core){
        if(!core.matches("[a-z][a-z0-9_]{0,39}"))return "";
        File installed=new File(getFilesDir(),"android-cores/"+core+"/lib"+core+"_libretro.so");
        File bundled=new File(getApplicationInfo().nativeLibraryDir,"lib"+core+"_libretro.so");
        return installed.isFile()?installed.getAbsolutePath():bundled.isFile()?bundled.getAbsolutePath():"";
    }
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
        String controls = "touch";
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice device = InputDevice.getDevice(id);
            if (device == null || device.isVirtual()) continue;
            int source = device.getSources();
            if ((source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                    || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) controls = "controller";
        }
        orientFor(controls.equals("controller"));
        return controls;
    }
    private volatile int orientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
    /**
     * With a controller the picture stays the long way round, either way up. A handheld's own pad counts (Android calls
     * it external too): its panel is an upright one, and following the sensor turned the app upright on an Odin 2 Mini
     * whenever it stood on its side, with Android's auto-rotate off. A phone without a pad still turns freely.
     */
    private void orientFor(boolean controller) {
        int wanted = controller ? android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE : android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
        if (wanted == orientation) return;
        orientation = wanted;
        runOnUiThread(() -> setRequestedOrientation(wanted));
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
            // Write as well: the saves' copies go into a folder of their own in this one (src/save_copy.rs). Reading is what matters most.
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            String selected = getPreferences(0).getString("tree", "");
            if (!selected.isEmpty()) intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI, Uri.parse(selected));
            startActivityForResult(intent, PICK_TREE);
        });
        return "";
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == UpdateInstaller.ALLOW_INSTALL) { UpdateInstaller.get(this).permissionResult(this); return; }
        if (request == SAVES_OUT || request == SAVES_IN) { savesPicked(request, result, data); return; }
        if (request != PICK_TREE || result != RESULT_OK || data == null || data.getData() == null) return;
        Uri tree = data.getData();
        try {
            int flags = data.getFlags() & (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            if ((flags & Intent.FLAG_GRANT_READ_URI_PERMISSION) == 0) throw new SecurityException("The folder did not grant read access");
            getContentResolver().takePersistableUriPermission(tree, flags);
            android.util.Log.i(TAG, "folder grant: " + ((flags & Intent.FLAG_GRANT_WRITE_URI_PERMISSION) != 0 ? "read and write" : "read only"));
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
    // Settings → Saves. What goes into the zip and where its files land is Rust's plan (src/save_transfer.rs).
    private final java.util.concurrent.atomic.AtomicReference<String> savesStatus = new java.util.concurrent.atomic.AtomicReference<>("");
    private final java.util.concurrent.atomic.AtomicInteger savesTurn = new java.util.concurrent.atomic.AtomicInteger();
    private volatile String savesPlan = "";
    private volatile Uri savesBackup;
    /** Settings → Report a problem (`ReportSupport`): the device, the app's log, and the two ways out of the app. */
    public String deviceReport() { return ReportSupport.device(this); }
    public String recentLog() { return ReportSupport.log(this); }
    public String shareReport(String text, String title) { return ReportSupport.share(this, text, title); }
    public String copyReport(String text, String title) { return ReportSupport.copy(this, text); }
    public String localDay() { return Long.toString(java.time.LocalDate.now().toEpochDay()); }
    /** What the export or import on its way has come to; said once. Empty while it is still going. */
    public String savesStatus() { return savesStatus.getAndSet(""); }
    public String savesCancel() { savesTurn.incrementAndGet(); savesBackup = null; savesStatus.set(""); return ""; }
    public String exportSaves(String plan, String name) {
        savesPlan = plan; savesStatus.set("");
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("application/zip").putExtra(Intent.EXTRA_TITLE, name);
            try { startActivityForResult(intent, SAVES_OUT); }
            catch (android.content.ActivityNotFoundException e) { android.util.Log.w(TAG, "No file picker", e); savesStatus.set("export-failed"); }
        });
        return "";
    }
    public String importSaves() {
        savesBackup = null; savesStatus.set("");
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*")
                .putExtra(Intent.EXTRA_MIME_TYPES, new String[]{"application/zip", "application/x-zip-compressed", "application/octet-stream"});
            try { startActivityForResult(intent, SAVES_IN); }
            catch (android.content.ActivityNotFoundException e) { android.util.Log.w(TAG, "No file picker", e); savesStatus.set("import-failed"); }
        });
        return "";
    }
    private void savesPicked(int request, int result, Intent data) {
        if (result != RESULT_OK || data == null || data.getData() == null) { savesStatus.set("cancelled"); return; }
        Uri uri = data.getData();
        int turn = savesTurn.incrementAndGet();
        if (request == SAVES_OUT) {
            String plan = savesPlan;
            new Thread(() -> savesDone(turn, SaveBackup.write(getContentResolver(), uri, plan, () -> savesTurn.get() != turn)), "saves-export").start();
        } else {
            savesBackup = uri;
            new Thread(() -> savesDone(turn, SaveBackup.list(getContentResolver(), uri)), "saves-list").start();
        }
    }
    /** After the page's answer: unpack what the plan takes. The picked file's grant lasts while this Activity lives. */
    public String importApply(String plan) {
        Uri uri = savesBackup;
        if (uri == null) { savesStatus.set("import-failed"); return ""; }
        int turn = savesTurn.incrementAndGet();
        new Thread(() -> { String outcome = SaveBackup.apply(getContentResolver(), uri, getFilesDir(), plan, () -> savesTurn.get() != turn); savesBackup = null; savesDone(turn, outcome); }, "saves-import").start();
        return "";
    }
    private void savesDone(int turn, String outcome) {
        android.util.Log.i(TAG, "saves: " + outcome.split("\n", 2)[0]);
        if (savesTurn.get() == turn) savesStatus.set(outcome);
    }
    // Copies of the saves in the chosen folder (src/save_copy.rs decides, SaveCopy.java moves the bytes). Worker threads only.
    private final Map<String, SaveCopy> copyStores = new HashMap<>();
    private synchronized SaveCopy copyStore(String root) {
        SaveCopy store = copyStores.get(root);
        if (store == null) copyStores.put(root, store = new SaveCopy(getContentResolver(), root));
        return store;
    }
    /** Every chosen folder with what pocket-ui may do in it: [{"uri","name","state"}], state ok / read-only / lost / gone. */
    public String copyTargets() {
        JSONArray out = new JSONArray();
        try {
            JSONArray roots = roots();
            for (int i = 0; i < roots.length(); i++) {
                String root = roots.getString(i);
                out.put(new JSONObject().put("uri", root).put("name", rootName(root)).put("state", SaveCopy.state(getContentResolver(), root)));
            }
        } catch (Exception e) { android.util.Log.w(TAG, "save copy: targets", e); }
        return out.toString();
    }
    public String copyGames(String root) { return copyStore(root).games(); }
    public String copyManifest(String root, String dir) { return copyStore(root).manifest(dir); }
    public String copyPut(String root, String dir, String rel, String local) { return copyStore(root).put(dir, rel, local); }
    public String copyPutManifest(String root, String dir, String text) { return copyStore(root).putManifest(dir, text); }
    public String copyGet(String root, String dir, String rel, String local) { return copyStore(root).get(dir, rel, local); }
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
                String prefix = rootPrefix(i, root);
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
    /** Worker-only: decode a granted provider picture into a bounded private PNG. No writes to the card. */
    public String readPicture(String document) {
        android.graphics.Bitmap bitmap = null;
        try {
            Uri uri = Uri.parse(document);
            if (!"content".equals(uri.getScheme())) return "";
            byte[] bytes;
            try (InputStream in = getContentResolver().openInputStream(uri)) {
                ByteArrayOutputStream out = new ByteArrayOutputStream();
                byte[] block = new byte[16384]; int n;
                while ((n = in.read(block)) != -1) {
                    if (out.size() + n > 8388608) return "";
                    out.write(block, 0, n);
                }
                bytes = out.toByteArray();
            }
            android.graphics.BitmapFactory.Options options = new android.graphics.BitmapFactory.Options();
            options.inJustDecodeBounds = true;
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
            if (options.outWidth <= 0 || options.outHeight <= 0 || options.outWidth > 16384 || options.outHeight > 16384) return "";
            options.inSampleSize = 1;
            while (options.outWidth / options.inSampleSize > 1024 || options.outHeight / options.inSampleSize > 1024) options.inSampleSize *= 2;
            options.inJustDecodeBounds = false;
            bitmap = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
            if (bitmap == null) return "";
            File folder = new File(getCacheDir(), "document-art");
            if (!folder.isDirectory() && !folder.mkdirs()) return "";
            // The one media worker consumes this file before its next bridge call; no growing artwork cache.
            File file = new File(folder, "current.png");
            try (OutputStream out = new FileOutputStream(file)) {
                if (!bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)) return "";
            }
            return file.getAbsolutePath();
        } catch (Exception e) { android.util.Log.w(TAG, "Provider picture unavailable", e); return ""; }
        finally { if (bitmap != null) bitmap.recycle(); }
    }

    private static String rootPrefix(int index, String root) throws Exception {
        if (index == 0) return "";
        byte[] digest = java.security.MessageDigest.getInstance("SHA-256").digest(root.getBytes(java.nio.charset.StandardCharsets.UTF_8));
        StringBuilder hex = new StringBuilder("@pocket-root-");
        for (int j = 0; j < 8; j++) hex.append(String.format(Locale.ROOT, "%02x", digest[j] & 255));
        return hex.toString() + "/";
    }
    /** "GameCheck" of primary:Download/GameCheck, "Phone storage" for the top of a volume. */
    private String rootName(String root) {
        try {
            Uri tree = Uri.parse(root);
            String id = DocumentsContract.getTreeDocumentId(tree);
            // Document ids can be opaque. The provider's display name is what the player selected.
            try (Cursor cursor = getContentResolver().query(DocumentsContract.buildDocumentUriUsingTree(tree, id),
                    new String[]{DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
                if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0) && !cursor.getString(0).isEmpty()) return cursor.getString(0);
            } catch (RuntimeException e) { android.util.Log.w(TAG, "SAF folder name unavailable", e); }
            String name = id.substring(Math.max(id.lastIndexOf('/'), id.lastIndexOf(':')) + 1);
            return name.isEmpty() ? "Phone storage" : name;
        } catch (RuntimeException e) { return "Your folder"; }
    }
    /** Worker-only, for the Game check: every chosen folder as it is now, or why it can't be read. Android's grants can vanish. */
    public String checkListing() throws Exception {
        HashSet<String> granted = new HashSet<>();
        for (android.content.UriPermission p : getContentResolver().getPersistedUriPermissions()) if (p.isReadPermission()) granted.add(p.getUri().toString());
        JSONArray roots = roots(), out = new JSONArray();
        for (int i = 0; i < roots.length(); i++) {
            String root = roots.getString(i), prefix = rootPrefix(i, root);
            JSONObject entry = new JSONObject().put("uri", root).put("name", rootName(root)).put("prefix", prefix);
            String state = "ok";
            JSONArray files = new JSONArray();
            if (!granted.contains(root)) state = "lost";
            else try {
                files = new JSONArray(scanLibrary(root));
                for (int j = 0; j < files.length(); j++) { JSONObject f = files.getJSONObject(j); f.put("relative", prefix + f.getString("relative")); }
            } catch (SecurityException e) { state = "lost"; files = new JSONArray(); }
            catch (LibraryTooBig e) { state = "big"; files = new JSONArray(); }
            catch (Exception e) { android.util.Log.w(TAG, "Game check: folder unreadable", e); state = "gone"; files = new JSONArray(); }
            out.put(entry.put("state", state).put("files", files));
        }
        return new JSONObject().put("roots", out).toString();
    }
    /** Worker-only: a document open for reading, its descriptor handed over to the caller. Empty: it can't be opened. */
    public String openDocument(String document) {
        try (android.os.ParcelFileDescriptor file = getContentResolver().openFileDescriptor(Uri.parse(document), "r")) {
            return file == null ? "" : Integer.toString(file.dup().detachFd());
        } catch (Exception e) { return ""; }
    }
    public String emulatorInstalled(String profile) { return Boolean.toString(EmulatorIntents.available(this, profile)); }
    /** Settings → Plays with: the apps on the phone that take each of these profiles (comma list), as JSON. */
    public String installedApps(String profiles) { return EmulatorIntents.installedApps(this, profiles); }
    /** Stops reading a chosen folder that can't be read any more; its files aren't touched. */
    public String forgetFolder(String root) throws Exception {
        JSONArray roots = roots(), left = new JSONArray();
        for (int i = 0; i < roots.length(); i++) if (!root.equals(roots.getString(i))) left.put(roots.getString(i));
        try { getContentResolver().releasePersistableUriPermission(Uri.parse(root), Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION); } catch (SecurityException | IllegalArgumentException e) { android.util.Log.i(TAG, "No grant left to release"); }
        String tree = left.length() == 0 ? "" : left.getString(left.length() - 1);
        getPreferences(0).edit().putString("trees", left.toString()).putString("tree", tree).putInt("revision", getPreferences(0).getInt("revision", 0) + 1).commit();
        return "";
    }
    /** A message that stays until it is read: a title, the text and OK (an emulator app to install). */
    public String notice(String title, String text) { runOnUiThread(() -> new AlertDialog.Builder(this).setTitle(title).setMessage(text).setPositiveButton("OK", null).show()); return ""; }
    /** The drawer under the shelf (`AppDrawer`): the apps with a start icon, and starting one. */
    public String launcherApps() { return AppDrawer.list(this); }
    public String launchApp(String pkg) { return AppDrawer.launch(this, pkg); }
    public String toast(String text) { runOnUiThread(() -> android.widget.Toast.makeText(this, text, android.widget.Toast.LENGTH_LONG).show()); return ""; }
    private static final class LibraryTooBig extends IOException { LibraryTooBig(String message) { super(message); } }
    /** A sheet's names as they were written: many cue sheets are Latin-1, not UTF-8. */
    private static String sheetText(byte[] bytes) {
        try { return java.nio.charset.StandardCharsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(bytes)).toString(); }
        catch (java.nio.charset.CharacterCodingException e) { return new String(bytes, java.nio.charset.StandardCharsets.ISO_8859_1); }
    }
    /** Called on a Rust worker with the captured tree, never traverses providers on the UI thread. */
    public String scanLibrary(String selected) throws Exception {
        JSONArray files = new JSONArray();
        if (selected.isEmpty()) return files.toString();
        Uri tree = Uri.parse(selected);
        if (!"content".equals(tree.getScheme()) || !DocumentsContract.isTreeUri(tree)) throw new IOException("Choose a ROM folder again");
        String rootName = rootName(selected);
        ArrayDeque<String[]> pending = new ArrayDeque<>();
        pending.add(new String[]{DocumentsContract.getTreeDocumentId(tree), "", "0"});
        HashSet<String> visited = new HashSet<>();
        int seen = 0, metadataBytes = 0;
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
                    if (++seen > 50000) throw new LibraryTooBig("Library exceeds 50,000 entries; choose a smaller folder");
                    String id = cursor.getString(0), name = cursor.getString(1), mime = cursor.getString(2);
                    if (name == null || name.equals(".") || name.equals("..") || name.contains("/") || name.contains("\\")) continue;
                    String relative = folder[1] + name;
                    if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mime)) {
                        if (name.equals(SaveCopy.FOLDER)) continue; // the saves' copies: no game in there
                        if (depth >= 32) throw new IOException("Library exceeds 32 folder levels");
                        pending.add(new String[]{id, relative + "/", Integer.toString(depth + 1)});
                    } else {
                        JSONObject entry = new JSONObject();
                        entry.put("uri", DocumentsContract.buildDocumentUriUsingTree(tree, id).toString());
                        entry.put("relative", relative);
                        entry.put("root_name", rootName);
                        entry.put("id", id);
                        entry.put("authority", tree.getAuthority());
                        boolean optionalText = name.equalsIgnoreCase("gamelist.xml") || name.toLowerCase(Locale.ROOT).endsWith(".cht");
                        if ((optionalText && metadataBytes < 8388608) || name.toLowerCase(Locale.ROOT).matches(".*\\.(m3u|cue|gdi)$")) {
                            Uri document = DocumentsContract.buildDocumentUriUsingTree(tree, id);
                            try (InputStream in = getContentResolver().openInputStream(document)) {
                                ByteArrayOutputStream out = new ByteArrayOutputStream();
                                byte[] bytes = new byte[4096]; int n;
                                while (out.size() <= 262144 && (n = in.read(bytes)) != -1) out.write(bytes, 0, n);
                                if (out.size() > 262144) {
                                    if (!optionalText) throw new IOException("Disc sheet too large: " + relative);
                                } else { entry.put("sheet", sheetText(out.toByteArray())); if (optionalText) metadataBytes += out.size(); }
                            } catch (IOException | SecurityException e) {
                                if (!optionalText) throw e;
                                android.util.Log.w(TAG, "Optional game metadata unavailable", e);
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
    public String localAudioOutput(String device) { return AudioRoute.localOutput(this, device); }
    /** RetroAchievements (`achievements::http::AndroidHttp`): one HTTPS request, one answer; worker threads only. */
    public String raRequest(String address, String post, String contentType, String userAgent) { return RaHttp.request(address, post, contentType, userAgent); }
    public String raDownload(String address, String destination, String userAgent) { return RaHttp.download(address, destination, userAgent); }
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
    /** The launcher's start animation (`insert.rs`): the game is made ready at once and comes when the animation lets it. */
    private final Object handOver=new Object();
    private boolean holding;
    private Runnable heldStart;
    /** Which start is on its way: `cancelLaunch` moves on, and a worker of an earlier turn stops, starts nothing and says nothing. */
    private int launchTurn;
    private volatile boolean launchFailed;
    private boolean launchCurrent(int turn){synchronized(handOver){return turn==launchTurn;}}
    public String holdLaunch(){
        synchronized(handOver){holding=true;heldStart=null;launchFailed=false;}
        // The player's process comes up while the animation plays: shorter on black afterwards, and Android shows no splash for a running process.
        try{startService(new Intent(this,PlayerWarm.class));}catch(RuntimeException e){android.util.Log.w(TAG,"player warm-up",e);}
        return "";
    }
    public String releaseLaunch(){Runnable start;synchronized(handOver){holding=false;start=heldStart;heldStart=null;}if(start!=null)runOnUiThread(start);return "";}
    /** Left the launcher before the game came, or the shelf came back: nothing comes up later. "true": a start was stopped. */
    public String cancelLaunch(){synchronized(handOver){launchTurn++;holding=false;heldStart=null;return Boolean.toString(launching.getAndSet(false));}}
    /** A worker is still making the game ready, or its start hasn't been handed to Android yet. */
    public String launchOnItsWay(){return Boolean.toString(launching.get());}
    public String launchFailed(){boolean failed=launchFailed;launchFailed=false;return Boolean.toString(failed);}
    private void whenReleased(int turn,Runnable start){
        Runnable checked=()->{if(launchCurrent(turn))start.run();else android.util.Log.i(TAG,"launch: cancelled start dropped");};
        boolean now;synchronized(handOver){if(turn!=launchTurn){android.util.Log.i(TAG,"launch: cancelled start dropped");return;}now=!holding;if(!now)heldStart=checked;}
        if(now)runOnUiThread(checked);
    }
    private void launchError(int turn,String title,Exception e){
        synchronized(handOver){if(turn!=launchTurn){android.util.Log.i(TAG,"launch: cancelled start ended: "+e);return;}holding=false;heldStart=null;launching.set(false);}
        launchFailed=true;error(title,e);
    }
    /**
     * Check direct access or stage a game on a worker, then enter our own EGL player. URI identity owns its saves. `parts`: a disc sheet's files as
     * `[{"uri","name"}]` (`document_library::Part`), copied beside it; `bios`: the supported BIOS files among the chosen
     * folders, the same way, for the player's private BIOS folder. Both "[]" for a cartridge.
     */
    private native String nativePrepareDocument(int fd,String key,String name);
    /** Large-file CRC work stays on the content worker while the insertion animation runs. */
    private void prepareDirectDocument(String document,String key,String name) throws IOException {
        try(android.os.ParcelFileDescriptor fd=getContentResolver().openFileDescriptor(Uri.parse(document),"r")){
            if(fd==null)throw new FileNotFoundException("no descriptor");
            if(fd.getStatSize()>(64L<<20)){
                String error=nativePrepareDocument(fd.dup().detachFd(),key,name);
                if(!error.isEmpty())throw new IOException(error);
            }
        }
    }
    public String launchInternal(String document,String relative,String core,String key,String title,String system,String settings,String parts,String bios) {
        final int turn;
        synchronized(handOver){if(!launching.compareAndSet(false,true))return "Already opening a game";turn=launchTurn;}
        new Thread(()->{
            try {
                if (!core.matches("[a-z][a-z0-9_]{0,39}") || corePath(core).isEmpty())
                    throw new IOException("System not downloaded: " + core);
                if (!key.matches("[a-f0-9]{16}")) throw new IOException("Invalid game identity");
                String name=relative.substring(relative.lastIndexOf('/')+1);
                if(name.isEmpty()||name.equals(".")||name.equals("..")||name.contains("\\"))throw new IOException("Invalid ROM name");
                List<GameCopy.Part> set=new ArrayList<>();
                JSONArray listed=new JSONArray(parts);
                for(int i=0;i<listed.length();i++)set.add(new GameCopy.Part(Uri.parse(listed.getJSONObject(i).getString("uri")),listed.getJSONObject(i).getString("name")));
                CopyWindow window=new CopyWindow(title,settings,turn);
                File rom;
                String copyReason=GameDirect.probe(this,Uri.parse(document),core,name,!set.isEmpty());
                final boolean direct=copyReason==null;
                android.util.Log.i(TAG,"content: "+(direct?"direct descriptor":"copy: "+copyReason));
                try{
                    copyBios(new JSONArray(bios));
                    if(core.equals("yabasanshiro")&&!BiosFiles.hasSaturn(new File(getFilesDir(),"bios")))
                        throw new IOException(inLanguage(settings,"Saturn needs a BIOS. Put saturn_bios.bin, sega_101.bin or mpr-17933.bin in a selected folder, then run Game check.","Saturn braucht ein BIOS. Lege saturn_bios.bin, sega_101.bin oder mpr-17933.bin in einen gewählten Ordner und starte Spiele prüfen.","Saturn necesita una BIOS. Añade saturn_bios.bin, sega_101.bin o mpr-17933.bin a una carpeta seleccionada y ejecuta Revisar juegos."));
                    // A provider can have listed a BIOS that it no longer lets us read. Explain the actual fallback too.
                    if(core.equals("flycast")&&!BiosFiles.hasDreamcast(new File(getFilesDir(),"bios"))&&new JSONArray(bios).length()>0)
                        runOnUiThread(()->android.widget.Toast.makeText(this,inLanguage(settings,"No Dreamcast BIOS. Flycast uses its built-in replacement. For better compatibility, add dc_boot.bin and dc_flash.bin to a selected bios folder.","Kein Dreamcast-BIOS. Flycast nutzt seinen eingebauten Ersatz. Für bessere Kompatibilität lege dc_boot.bin und dc_flash.bin in einen gewählten bios-Ordner.","No hay BIOS de Dreamcast. Flycast usa su reemplazo integrado. Para mayor compatibilidad, añade dc_boot.bin y dc_flash.bin a una carpeta bios seleccionada."),android.widget.Toast.LENGTH_LONG).show());
                    if(direct)prepareDirectDocument(document,key,name);
                    rom=direct?new File(name):set.isEmpty()?new GameCopy(this).prepare(Uri.parse(document),key,name,name+"."+turn+".part",core,copyWords(settings),window)
                        :new GameCopy(this).prepareSet(Uri.parse(document),name,set,key,turn,core,copyWords(settings),window);
                }
                catch(SecurityException e){throw new IOException(cannotRead(settings,false),e);}
                catch(FileNotFoundException|IllegalArgumentException e){throw new IOException(cannotRead(settings,true),e);} // The storage provider's way of saying it is gone.
                finally{window.close();}
                Intent player=new Intent(this,PlayerActivity.class).putExtra("core",corePath(core)).putExtra("rom",rom.getAbsolutePath()).putExtra("system",system).putExtra("key",key).putExtra("title",title).putExtra("diagnosticAudio",getIntent().getBooleanExtra("diagnosticAudio",false)).putExtra("diagnosticAudioSeconds",getIntent().getIntExtra("diagnosticAudioSeconds",30)).putExtra("settings",settings).putExtra("diagnosticInput",getIntent().getBooleanExtra("diagnosticInput",false)).addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION);
                if(direct)player.putExtra("rom",name).putExtra("document",document);
                whenReleased(turn,()->{try{startActivity(player);}finally{launching.set(false);}});
            }catch(GameCopy.Cancelled e){android.util.Log.i(TAG,"launch: cancelled start ended");if(launchCurrent(turn)){cancelLaunch();launchFailed=true;} // Its Cancel: back to the shelf. Home has moved on already.
            }catch(Exception e){launchError(turn,inLanguage(settings,"Game couldn't start","Das Spiel konnte nicht starten","No se pudo iniciar el juego"),e);}
        },"pocket-content").start();return "";
    }
    private static GameCopy.Words copyWords(String settings){
        return new GameCopy.Words(){
            @Override public String tooBig(String size,String most){return inLanguage(settings,"This game is "+size+". This app's player loads it into memory whole and takes games up to "+most+" on this system.","Dieses Spiel hat "+size+". Der Player dieser App lädt es ganz in den Speicher und nimmt auf diesem System Spiele bis "+most+".","Este juego pesa "+size+". El emulador de esta app lo carga entero en la memoria y admite juegos de hasta "+most+" en este sistema.");}
            @Override public String noRoom(String size,String free){return inLanguage(settings,"This game needs "+size+" free on your phone for its first start, and "+free+" is free. Make some room and start it again. Your saves are safe.","Für den ersten Start braucht dieses Spiel "+size+" freien Speicher auf deinem Handy, frei sind "+free+". Schaff Platz und starte es noch einmal. Deine Spielstände bleiben erhalten.","Para su primer inicio este juego necesita "+size+" libres en tu móvil, y hay "+free+" libres. Libera espacio e inícialo de nuevo. Tus partidas guardadas están a salvo.");}
            @Override public String outside(){return inLanguage(settings,"This game's .cue or .m3u names a file outside its folder. Put it and all its files in one folder and start the game again.","Die .cue- oder .m3u-Datei dieses Spiels nennt eine Datei außerhalb ihres Ordners. Leg sie mit allen ihren Dateien in einen Ordner und starte das Spiel noch einmal.","El .cue o .m3u de este juego nombra un archivo fuera de su carpeta. Ponlo con todos sus archivos en una carpeta e inicia el juego de nuevo.");}
            @Override public String full(){return inLanguage(settings,"Your phone's storage is full. Make some room and start the game again. Your saves are safe.","Der Speicher deines Handys ist voll. Schaff Platz und starte das Spiel noch einmal. Deine Spielstände bleiben erhalten.","El almacenamiento de tu móvil está lleno. Libera espacio e inicia el juego de nuevo. Tus partidas guardadas están a salvo.");}
        };
    }
    /** Install only known BIOS names, to the private system folder. Unreadable copies preserve any kept BIOS. */
    private void copyBios(JSONArray bios) throws org.json.JSONException {
        File folder=new File(getFilesDir(),"bios");
        for(int i=0;i<bios.length();i++){
            String name=bios.getJSONObject(i).getString("name");
            try(InputStream in=getContentResolver().openInputStream(Uri.parse(bios.getJSONObject(i).getString("uri")))){
                BiosFiles.install(folder,name,in);
                android.util.Log.i(TAG,"bios: "+name+" ready");
            }catch(IOException|RuntimeException e){android.util.Log.w(TAG,"bios: "+name+" not copied: "+e.getMessage());}
        }
    }
    /** The bar of a big game's first start: Cancel or Back stops the copy, the screen stays on meanwhile. Shown at its first report. */
    private final class CopyWindow implements GameCopy.Progress {
        private final String title,settings;
        private final int turn;
        // Test only (`slowCopy` launch extra, milliseconds a block, as `insertRehearsal`): a provider that takes its time.
        private final int slowCopy=getIntent().getIntExtra("slowCopy",0);
        private final long began=android.os.SystemClock.elapsedRealtime();
        private volatile boolean stop,open;
        private long drawn;
        private AlertDialog dialog;
        private android.widget.ProgressBar bar;
        private android.widget.TextView line;
        CopyWindow(String title,String settings,int turn){this.title=title;this.settings=settings;this.turn=turn;}
        @Override public boolean current(){return launchCurrent(turn);}
        @Override public Object lock(){return handOver;}
        @Override public boolean at(long done,long total,boolean big){
            if(slowCopy>0)android.os.SystemClock.sleep(slowCopy);
            if(!big)return !stop&&launchCurrent(turn);
            long now=android.os.SystemClock.elapsedRealtime();
            boolean first=!open;
            if(first||now-drawn>=250){
                drawn=now;open=true;
                long left=total>0&&done>0&&now-began>=2000?(total-done)*(now-began)/done/1000+1:-1;
                String of=total>0?GameCopy.shown(done)+inLanguage(settings," of "," von "," de ")+GameCopy.shown(total):GameCopy.shown(done);
                String text=left>=0?of+inLanguage(settings," · about "," · noch etwa "," · unos ")+(left>=90?(left+30)/60+" min":left+" s"):of;
                int percent=total>0?(int)(done*1000/total):0;
                runOnUiThread(()->draw(first,total>0,percent,text));
            }
            return !stop&&launchCurrent(turn);
        }
        private void draw(boolean first,boolean known,int percent,String text){
            if(first&&dialog==null&&!isFinishing()){
                bar=new android.widget.ProgressBar(PocketActivity.this,null,android.R.attr.progressBarStyleHorizontal);
                bar.setMax(1000);bar.setIndeterminate(!known);
                line=new android.widget.TextView(PocketActivity.this);
                android.widget.TextView about=new android.widget.TextView(PocketActivity.this);
                about.setText(inLanguage(settings,"The first start copies this big game into the app's own storage. After that it starts without this wait.","Beim ersten Start kommt dieses große Spiel in den Speicher der App, danach startet es ohne Warten.","El primer inicio copia este juego grande al almacenamiento de la app. Después arranca sin esta espera."));
                android.widget.LinearLayout box=new android.widget.LinearLayout(PocketActivity.this);
                box.setOrientation(android.widget.LinearLayout.VERTICAL);
                int pad=(int)(20*getResources().getDisplayMetrics().density);
                box.setPadding(pad,pad/2,pad,0);
                box.addView(about);box.addView(bar);box.addView(line);
                dialog=new AlertDialog.Builder(PocketActivity.this).setTitle(title).setView(box)
                    .setNegativeButton(inLanguage(settings,"Cancel","Abbrechen","Cancelar"),(d,w)->stop=true).setOnCancelListener(d->stop=true).create();
                dialog.setCanceledOnTouchOutside(false);
                dialog.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                dialog.show();
                android.util.Log.i(TAG,"copy: bar shown");
            }
            if(bar!=null){bar.setProgress(percent);line.setText(text);}
        }
        void close(){runOnUiThread(()->{if(dialog!=null&&dialog.isShowing())dialog.dismiss();});android.util.Log.i(TAG,"copy: "+(open?"window ":"")+(android.os.SystemClock.elapsedRealtime()-began)+" ms");}
    }
    public String directGameReason(String document,String core,String name,String parts){
        String reason=GameDirect.probe(this,Uri.parse(document),core,name,Boolean.parseBoolean(parts));
        return reason==null?"":reason;
    }
    /** Room for a first copy of this game (`document_check::Host`); -1: it has one. */
    public String roomForCopy(String key){return Long.toString(new GameCopy(this).room(key));}
    /** The player's language: Settings' choice ("language", an older version's "german"), else the phone's. English for any other. */
    private static String inLanguage(String settings,String english,String german,String spanish) {
        String system=java.util.Locale.getDefault().getLanguage();
        boolean de=settings.contains("\"language\":\"de\"")||settings.contains("\"german\":true");
        boolean es=settings.contains("\"language\":\"es\"");
        boolean chosen=de||es||settings.contains("\"language\":\"en\"")||settings.contains("\"german\":false");
        if(de||(!chosen&&system.equals("de")))return german;
        return es||(!chosen&&system.equals("es"))?spanish:english;
    }
    /** Nothing was replaced: the copy before this start, the moments and the in-game saves are as they were. */
    private static String cannotRead(String settings,boolean gone) {
        if(gone)return inLanguage(settings,"This game's file isn't in its folder any more. Your moment and your saves are safe; put the file back to carry on.","Die Datei des Spiels ist nicht mehr in seinem Ordner. Dein Moment und deine Spielstände bleiben erhalten; leg die Datei zurück, dann geht es weiter.","El archivo de este juego ya no está en su carpeta. Tu momento y tus partidas guardadas siguen a salvo; vuelve a poner el archivo para continuar.");
        return inLanguage(settings,"Android no longer lets pocket-ui read this file. Choose its folder again in Settings. Your moment and your saves are safe.","Android lässt pocket-ui diese Datei nicht mehr lesen. Wähle den Ordner in den Einstellungen unter „ROM-Ordner“ noch einmal. Dein Moment und deine Spielstände bleiben erhalten.","Android ya no deja que pocket-ui lea este archivo. Vuelve a elegir su carpeta en Ajustes. Tu momento y tus partidas guardadas siguen a salvo.");
    }
    public String launchState() { return launchStatus; }
    /**
     * Called by Rust with a profile id and document URI. No URI is converted into a fake filesystem path. `preferred` is
     * the app chosen in Settings → Plays with ("": the first one on the phone).
     */
    public String launchGame(String document, String relative, String profile, String preferred) {
        final int turn;
        synchronized (handOver) { if (!launching.compareAndSet(false, true)) return "Already opening a game"; turn = launchTurn; }
        launchStatus = "Opening game...";
        new Thread(() -> {
            try {
                Intent intent = EmulatorIntents.create(this, Uri.parse(document), relative, profile, preferred);
                intent.addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION);
                whenReleased(turn, () -> {
                    try { startActivity(intent); launchStatus = ""; android.util.Log.i(TAG, "launch profile=" + profile + " ROM=" + document); }
                    catch (Exception e) { launchStatus = e.getMessage(); launchError(turn, "Emulator couldn't start", e); }
                    finally { launching.set(false); }
                });
            } catch (Exception e) { launchStatus = e.getMessage(); launchError(turn, "Emulator couldn't start", e); }
        }, "pocket-launch").start();
        return "";
    }
    // Convert stick/hat transitions once, leaving repeat and navigation to Android key dispatch.
    @Override public boolean onGenericMotionEvent(MotionEvent event) {
        if ((event.getSource() & InputDevice.SOURCE_JOYSTICK) != 0 && event.getAction() == MotionEvent.ACTION_MOVE) {
            int next = StickKeys.direction(event.getAxisValue(MotionEvent.AXIS_HAT_X), event.getAxisValue(MotionEvent.AXIS_HAT_Y), event.getAxisValue(MotionEvent.AXIS_X), event.getAxisValue(MotionEvent.AXIS_Y));
            int[] change = stickKeys.update(event.getDeviceId(), next);
            if (change[0] != 0) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, change[0]));
            if (change[1] != 0) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN, change[1]));
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
        for (int key : stickKeys.releaseAll()) dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, key));
        super.onPause();
    }
}
