package xyz.pocketui.app;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.opengl.GLSurfaceView;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.media.AudioManager;
import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.opengles.GL10;

/** pocket-core's own EGL/GLES 3 Activity. Every core call stays on the GL thread. */
public final class PlayerActivity extends Activity {
    static { System.loadLibrary("pocket_core_android"); }
    private GLSurfaceView surface;
    private SemanticView semantics;
    private android.media.AudioFocusRequest focus;
    private boolean ready;
    private volatile boolean paused;
    private boolean hapticsEnabled;
    private android.view.ScaleGestureDetector pinch;
    private android.hardware.input.InputManager inputs;
    private final android.hardware.input.InputManager.InputDeviceListener inputListener = new android.hardware.input.InputManager.InputDeviceListener(){
        public void onInputDeviceAdded(int id){controllersChanged(false);}
        public void onInputDeviceChanged(int id){controllersChanged(true,id);}
        public void onInputDeviceRemoved(int id){controllersChanged(true,id);}
    };
    private final java.util.Set<Integer> gamepads=new java.util.HashSet<>();
    private void controllersChanged(boolean interrupted){controllersChanged(interrupted,-1);}
    private void controllersChanged(boolean interrupted,int changedId){
        java.util.Set<Integer> now=new java.util.HashSet<>();
        for(int id:android.view.InputDevice.getDeviceIds()){
            android.view.InputDevice d=android.view.InputDevice.getDevice(id);
            if(d!=null&&!d.isVirtual()&&((d.getSources()&android.view.InputDevice.SOURCE_GAMEPAD)==android.view.InputDevice.SOURCE_GAMEPAD||(d.getSources()&android.view.InputDevice.SOURCE_JOYSTICK)==android.view.InputDevice.SOURCE_JOYSTICK))now.add(id);
        }
        final boolean changed=!gamepads.equals(now);
        if(interrupted&&!changed&&!gamepads.contains(changedId))return;
        gamepads.clear();gamepads.addAll(now);
        final boolean connected=!now.isEmpty();
        orientFor(connected);
        sendRoster();
        surface.queueEvent(()->nativeControllers(connected,interrupted));surface.requestRender();
    }
    /** One line a gamepad (`id, descriptor, name, vendor, product`, tab-separated): who is which player is Rust's business (`android_pads`). */
    static String rosterLine(android.view.InputDevice d){
        return d.getId()+"\t"+d.getDescriptor().replace('\t',' ')+"\t"+d.getName().replace('\t',' ').replace('\n',' ')+"\t"+d.getVendorId()+"\t"+d.getProductId();
    }
    /** `diagnosticInput` launch extra: the keys and triggers as Android delivers them go to the log (tag PocketInput). */
    private boolean diagnosticInput,lastTriggers;
    private volatile String rosterText="";
    /** A pad that appears is reported as added, then changed a moment later: the roster goes out once things are quiet. */
    private final android.os.Handler rosterHandler=new android.os.Handler(android.os.Looper.getMainLooper());
    private final Runnable rosterJob=()->{
        StringBuilder text=new StringBuilder();
        for(int id:new java.util.TreeSet<>(gamepads)){android.view.InputDevice d=android.view.InputDevice.getDevice(id);if(d!=null)text.append(rosterLine(d)).append('\n');}
        rosterText=text.toString();
        surface.queueEvent(()->nativeRoster(rosterText));
    };
    private void sendRoster(){rosterHandler.removeCallbacks(rosterJob);rosterHandler.postDelayed(rosterJob,250);}
    private native void nativeRoster(String roster);
    private native void nativeControllers(boolean connected,boolean interrupted);
    private int orientation=android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
    /** As the launcher's `orientFor`: with a controller (a handheld's own counts) the game stays the long way round. UI thread. */
    private void orientFor(boolean controller){
        int wanted=controller?android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE:android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
        if(wanted!=orientation){orientation=wanted;setRequestedOrientation(wanted);}
    }
    private native void nativeZoom(float factor);
    private volatile int safeLeft,safeTop,safeRight,safeBottom;
    private native String nativeCreate(String core,String rom,String system,String key,String title);
    private native int nativeFrame(int width,int height,float density,float fontScale,int left,int top,int right,int bottom);
    private native boolean nativeKey(int key,boolean down,boolean controller,int device);
    private native boolean nativeAnalog(float x,float y,float rx,float ry,float hatX,float hatY,float lt,float rt,int device);
    private native boolean nativeTouch(int id,float x,float y,int phase);
    private native void nativeSuspend();
    private native void nativeForeground();
    /** Open in :player itself: fd numbers belong to a process and Activity intents cannot carry file descriptors. */
    private android.os.ParcelFileDescriptor gameDocument;
    public String gameDocumentFd(){
        if(!getIntent().hasExtra("document"))return "";
        try{
            if(gameDocument==null){
                gameDocument=getContentResolver().openFileDescriptor(android.net.Uri.parse(getIntent().getStringExtra("document")),"r");
            }
            return gameDocument==null?"-1":Integer.toString(gameDocument.dup().detachFd());
        }catch(java.io.IOException|RuntimeException error){android.util.Log.w("PocketMvp","direct document unavailable",error);return "-1";}
    }
    @Override protected void onDestroy(){
        rosterHandler.removeCallbacks(rosterJob);
        if(gameDocument!=null){try{gameDocument.close();}catch(java.io.IOException ignored){}gameDocument=null;}
        super.onDestroy();
    }
    /** RetroAchievements (`achievements::http::AndroidHttp`): one HTTPS request, one answer; worker threads only. */
    public String raRequest(String address, String post, String contentType, String userAgent) { return RaHttp.request(address, post, contentType, userAgent); }
    public String raDownload(String address, String destination, String userAgent) { return RaHttp.download(address, destination, userAgent); }
    public String dataPath(){return getFilesDir().getAbsolutePath();}
    public String preparePspAssets(){
        try{
            final android.content.res.AssetManager assets=getAssets();
            int count=SystemAssets.install(new SystemAssets.Source(){
                public String[] list(String path)throws java.io.IOException{return assets.list(path);}
                public java.io.InputStream open(String path)throws java.io.IOException{return assets.open(path);}
            },"system/PPSSPP",new java.io.File(getFilesDir(),"bios/PPSSPP"));
            android.util.Log.i("PocketMvp","PPSSPP system assets prepared: copied="+count);
            return "";
        }catch(java.io.IOException error){return error.toString();}
    }
    public String batterySaver(){return Boolean.toString(((android.os.PowerManager)getSystemService(POWER_SERVICE)).isPowerSaveMode());}
    public String fontScale(){return Float.toString(getResources().getConfiguration().fontScale);}
    public String playerSettings(){String value=getIntent().getStringExtra("settings");return value==null?"":value;}
    /** A short tick for an on-screen button (the system's haptic setting decides whether it is felt). */
    public String haptic(){if(hapticsEnabled)runOnUiThread(()->getWindow().getDecorView().performHapticFeedback(android.view.HapticFeedbackConstants.VIRTUAL_KEY));return "";}
    public String hasGamepad(){
        for(int id:android.view.InputDevice.getDeviceIds()){
            android.view.InputDevice device=android.view.InputDevice.getDevice(id);if(device==null)continue;
            int sources=device.getSources();
            if((sources&android.view.InputDevice.SOURCE_GAMEPAD)==android.view.InputDevice.SOURCE_GAMEPAD ||
               (sources&android.view.InputDevice.SOURCE_JOYSTICK)==android.view.InputDevice.SOURCE_JOYSTICK)return "true";
        }
        return "false";
    }
    public String language(){return java.util.Locale.getDefault().getLanguage();}
    @SuppressWarnings("deprecation") public String displayRefreshRate(){return Float.toString(getWindowManager().getDefaultDisplay().getRefreshRate());}
    /** Where a diagnostic launch leaves its audio capture: the app's own folder on shared storage, which adb reads without root. */
    public String diagnosticsPath(){java.io.File shared=getExternalFilesDir(null);return shared==null?"":shared.getAbsolutePath();}
    public String diagnosticAudio(){return Boolean.toString(getIntent().getBooleanExtra("diagnosticAudio",false));}
    public String diagnosticAudioSeconds(){return Integer.toString(Math.max(1,Math.min(240,getIntent().getIntExtra("diagnosticAudioSeconds",30))));}
    @SuppressWarnings("deprecation") public String speakerState(){return Boolean.toString(!((AudioManager)getSystemService(AUDIO_SERVICE)).isBluetoothA2dpOn());}
    public String localAudioOutput(String device){return AudioRoute.localOutput(this,device);}
    public String volumeState(){AudioManager a=(AudioManager)getSystemService(AUDIO_SERVICE);int max=a.getStreamMaxVolume(AudioManager.STREAM_MUSIC);return (max==0?0:a.getStreamVolume(AudioManager.STREAM_MUSIC)*100/max)+","+a.isStreamMute(AudioManager.STREAM_MUSIC);}
    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved); setVolumeControlStream(AudioManager.STREAM_MUSIC);
        diagnosticInput=getIntent().getBooleanExtra("diagnosticInput",false);
        orientFor("true".equals(hasGamepad()));
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        if(android.os.Build.VERSION.SDK_INT>=28){android.view.WindowManager.LayoutParams attrs=getWindow().getAttributes();attrs.layoutInDisplayCutoutMode=android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;getWindow().setAttributes(attrs);}
        try{hapticsEnabled=new org.json.JSONObject(playerSettings()).optBoolean("haptics",false);}catch(org.json.JSONException ignored){hapticsEnabled=false;}
        immersive();
        surface=new GLSurfaceView(this); surface.setEGLContextClientVersion(3); surface.setPreserveEGLContextOnPause(true);
        surface.setRenderer(new GLSurfaceView.Renderer() {
            public void onSurfaceCreated(GL10 unused,EGLConfig config) {
                String error=nativeCreate(getIntent().getStringExtra("core"),getIntent().getStringExtra("rom"),getIntent().getStringExtra("system"),getIntent().getStringExtra("key"),getIntent().getStringExtra("title"));
                ready=error.isEmpty();
                if(ready)nativeRoster(rosterText);
                if (!ready) runOnUiThread(() -> new AlertDialog.Builder(PlayerActivity.this).setTitle("Game couldn't start").setMessage(error).setPositiveButton("OK",(d,w)->finish()).setOnCancelListener(d->finish()).show());
            }
            public void onSurfaceChanged(GL10 unused,int width,int height) { }
            public void onDrawFrame(GL10 unused) {
                if (!ready) return;
                int state=nativeFrame(surface.getWidth(),surface.getHeight(),getResources().getDisplayMetrics().density,getResources().getConfiguration().fontScale,safeLeft,safeTop,safeRight,safeBottom);
                paused=state==1;
                if (state==2) {ready=false;runOnUiThread(()->finish());}
                else if (state==1) surface.setRenderMode(GLSurfaceView.RENDERMODE_WHEN_DIRTY);
            }
        });
        pinch=new android.view.ScaleGestureDetector(this,new android.view.ScaleGestureDetector.SimpleOnScaleGestureListener(){
            @Override public boolean onScaleBegin(android.view.ScaleGestureDetector detector){if(!paused)return false;surface.queueEvent(()->nativeZoom(1));surface.requestRender();return true;}
            @Override public boolean onScale(android.view.ScaleGestureDetector detector){final float factor=detector.getScaleFactor();surface.queueEvent(()->nativeZoom(factor));surface.requestRender();return true;}
        });
        surface.setOnTouchListener((view,event)->{
            if(paused){pinch.onTouchEvent(event);if(pinch.isInProgress())return true;}
            int action=event.getActionMasked();
            if(action==MotionEvent.ACTION_CANCEL) {surface.queueEvent(()->nativeTouch(-1,0,0,3));}
            else if(action==MotionEvent.ACTION_MOVE) {
                for(int i=0;i<event.getPointerCount();i++){final int id=event.getPointerId(i);final float x=event.getX(i),y=event.getY(i);surface.queueEvent(()->{if(nativeTouch(id,x,y,1))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});}
            } else {int i=event.getActionIndex();final int id=event.getPointerId(i);final float x=event.getX(i),y=event.getY(i);final int phase=(action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_POINTER_UP)?2:0;
                surface.queueEvent(()->{if(nativeTouch(id,x,y,phase))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});}
            surface.requestRender();return true;
        });
        if(android.os.Build.VERSION.SDK_INT>=30)getWindow().setDecorFitsSystemWindows(false);
        surface.setOnApplyWindowInsetsListener((view,insets)->{
            int[] safe=SafeInsets.read(this,insets);safeLeft=safe[0];safeTop=safe[1];safeRight=safe[2];safeBottom=safe[3];
            return insets;
        });
        setContentView(surface);
        inputs=(android.hardware.input.InputManager)getSystemService(INPUT_SERVICE);
        semantics=new SemanticView(this);addContentView(semantics,new android.view.ViewGroup.LayoutParams(-1,-1));
        if(android.os.Build.VERSION.SDK_INT>=33)getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,this::backRequested);
    }
    public void publishSemantics(String json){runOnUiThread(()->{if(semantics!=null)semantics.publish(json);});}
    @SuppressWarnings("deprecation") private void immersive(){
        getWindow().getDecorView().setSystemUiVisibility(5894|1024|512|256);
        if(android.os.Build.VERSION.SDK_INT>=30){android.view.WindowInsetsController c=getWindow().getInsetsController();if(c!=null){c.hide(android.view.WindowInsets.Type.systemBars());// Transient-bar mode suppresses edge Back until the bars are revealed on Android 15.
            c.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_DEFAULT);}}
    }
    @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);
        if(focused)immersive();}
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        int key=event.getKeyCode();
        if(key==KeyEvent.KEYCODE_VOLUME_UP||key==KeyEvent.KEYCODE_VOLUME_DOWN)return super.dispatchKeyEvent(event);
        if(event.getRepeatCount()>0&&event.getAction()==KeyEvent.ACTION_DOWN&&!(key>=KeyEvent.KEYCODE_DPAD_UP&&key<=KeyEvent.KEYCODE_DPAD_RIGHT))return true;
        final boolean down=event.getAction()==KeyEvent.ACTION_DOWN;
        final int sources=event.getSource()|(event.getDevice()==null?0:event.getDevice().getSources());
        final boolean controller=(sources&android.view.InputDevice.SOURCE_GAMEPAD)==android.view.InputDevice.SOURCE_GAMEPAD || (sources&android.view.InputDevice.SOURCE_JOYSTICK)==android.view.InputDevice.SOURCE_JOYSTICK;
        final int device=event.getDeviceId(),scanCode=event.getScanCode();
        if(diagnosticInput&&controller)android.util.Log.i("PocketInput","key code="+key+" scan="+event.getScanCode()+" down="+down+" device="+device+" name="+(event.getDevice()==null?"":event.getDevice().getName()));
        surface.queueEvent(()->{if(nativeKey(key,down,controller,device))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();return true;
    }
    private float axis(MotionEvent event,int axis){
        android.view.InputDevice device=event.getDevice();
        android.view.InputDevice.MotionRange range=device==null?null:device.getMotionRange(axis,event.getSource());
        float value=event.getAxisValue(axis);return Math.abs(value)<Math.max(.12f,range==null?0:range.getFlat())?0:Math.max(-1,Math.min(1,value));
    }
    @Override public boolean onGenericMotionEvent(MotionEvent event){
        if((event.getSource()&android.view.InputDevice.SOURCE_JOYSTICK)!=0&&event.getAction()==MotionEvent.ACTION_MOVE){
            final float x=axis(event,MotionEvent.AXIS_X),y=axis(event,MotionEvent.AXIS_Y),rx=axis(event,MotionEvent.AXIS_Z),ry=axis(event,MotionEvent.AXIS_RZ),hx=axis(event,MotionEvent.AXIS_HAT_X),hy=axis(event,MotionEvent.AXIS_HAT_Y);
            // Analog L2/R2 (Android names them LTRIGGER/RTRIGGER or BRAKE/GAS); Rust turns them into buttons.
            final float lt=Math.max(axis(event,MotionEvent.AXIS_LTRIGGER),axis(event,MotionEvent.AXIS_BRAKE)),rt=Math.max(axis(event,MotionEvent.AXIS_RTRIGGER),axis(event,MotionEvent.AXIS_GAS));
            final int device=event.getDeviceId();
            if(diagnosticInput&&(lt>0||rt>0||lastTriggers))android.util.Log.i("PocketInput","triggers lt="+lt+" rt="+rt+" device="+device);
            lastTriggers=lt>0||rt>0;
            surface.queueEvent(()->{if(nativeAnalog(x,y,rx,ry,hx,hy,lt,rt,device))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();return true;
        }
        return super.onGenericMotionEvent(event);
    }
    @Override public void onBackPressed(){backRequested();}
    private void backRequested(){android.util.Log.i("PocketMvp","player system back");surface.queueEvent(()->{if(nativeKey(KeyEvent.KEYCODE_BACK,true,false,-1))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();}
    @Override public void onConfigurationChanged(android.content.res.Configuration config){super.onConfigurationChanged(config);surface.requestApplyInsets();surface.requestRender();}
    @Override protected void onResume(){super.onResume();surface.onResume();immersive();
        inputs.registerInputDeviceListener(inputListener,null);surface.queueEvent(()->nativeForeground());controllersChanged(false);
        AudioManager audio=(AudioManager)getSystemService(AUDIO_SERVICE);
        focus=new android.media.AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_GAME).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build()).setOnAudioFocusChangeListener(change->{if(change<0){surface.queueEvent(()->nativeSuspend());surface.requestRender();}}).build();audio.requestAudioFocus(focus);
    }
    @Override protected void onPause(){inputs.unregisterInputDeviceListener(inputListener);
        // GLSurfaceView.onPause waits for the GL thread: the queued save completes before its context is parked.
        surface.queueEvent(()->nativeSuspend());surface.onPause();if(focus!=null)((AudioManager)getSystemService(AUDIO_SERVICE)).abandonAudioFocusRequest(focus);super.onPause();}
}
