package dev.pocket.spike;

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
    private volatile int safeLeft,safeTop,safeRight,safeBottom;
    private native String nativeCreate(String core,String rom,String system,String key,String title);
    private native int nativeFrame(int width,int height,float density,int left,int top,int right,int bottom);
    private native boolean nativeKey(int key,boolean down,boolean controller);
    private native boolean nativeAnalog(float x,float y,float rx,float ry,float hatX,float hatY);
    private native boolean nativeTouch(int id,float x,float y,int phase);
    private native void nativeSuspend();
    public String dataPath(){return getFilesDir().getAbsolutePath();}
    public String fontScale(){return Float.toString(getResources().getConfiguration().fontScale);}
    public String playerSettings(){String value=getIntent().getStringExtra("settings");return value==null?"":value;}
    /** A short tick for an on-screen button (the system's haptic setting decides whether it is felt). */
    public String haptic(){runOnUiThread(()->getWindow().getDecorView().performHapticFeedback(android.view.HapticFeedbackConstants.VIRTUAL_KEY));return "";}
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
    public String diagnosticAudio(){return Boolean.toString(getIntent().getBooleanExtra("diagnosticAudio",false));}
    @SuppressWarnings("deprecation") public String speakerState(){return Boolean.toString(!((AudioManager)getSystemService(AUDIO_SERVICE)).isBluetoothA2dpOn());}
    public String volumeState(){AudioManager a=(AudioManager)getSystemService(AUDIO_SERVICE);int max=a.getStreamMaxVolume(AudioManager.STREAM_MUSIC);return (max==0?0:a.getStreamVolume(AudioManager.STREAM_MUSIC)*100/max)+","+a.isStreamMute(AudioManager.STREAM_MUSIC);}
    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved); setVolumeControlStream(AudioManager.STREAM_MUSIC);
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        immersive();
        surface=new GLSurfaceView(this); surface.setEGLContextClientVersion(3); surface.setPreserveEGLContextOnPause(true);
        surface.setRenderer(new GLSurfaceView.Renderer() {
            public void onSurfaceCreated(GL10 unused,EGLConfig config) {
                String error=nativeCreate(getIntent().getStringExtra("core"),getIntent().getStringExtra("rom"),getIntent().getStringExtra("system"),getIntent().getStringExtra("key"),getIntent().getStringExtra("title"));
                ready=error.isEmpty();
                if (!ready) runOnUiThread(() -> new AlertDialog.Builder(PlayerActivity.this).setTitle("Game couldn't start").setMessage(error).setPositiveButton("OK",(d,w)->finish()).setOnCancelListener(d->finish()).show());
            }
            public void onSurfaceChanged(GL10 unused,int width,int height) { }
            public void onDrawFrame(GL10 unused) {
                if (!ready) return;
                int state=nativeFrame(surface.getWidth(),surface.getHeight(),getResources().getDisplayMetrics().density,safeLeft,safeTop,safeRight,safeBottom);
                if (state==2) {ready=false;runOnUiThread(()->finish());}
                else if (state==1) surface.setRenderMode(GLSurfaceView.RENDERMODE_WHEN_DIRTY);
            }
        });
        surface.setOnTouchListener((view,event)->{
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
        semantics=new SemanticView(this);addContentView(semantics,new android.view.ViewGroup.LayoutParams(-1,-1));
        if(android.os.Build.VERSION.SDK_INT>=33)getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,this::backRequested);
    }
    public void publishSemantics(String json){runOnUiThread(()->{if(semantics!=null)semantics.publish(json);});}
    @SuppressWarnings("deprecation") private void immersive(){
        getWindow().getDecorView().setSystemUiVisibility(5894|1024|512|256);
        if(android.os.Build.VERSION.SDK_INT>=30){android.view.WindowInsetsController c=getWindow().getInsetsController();if(c!=null){c.hide(android.view.WindowInsets.Type.systemBars());// Transient-bar mode suppresses edge Back until the bars are revealed on Android 15.
            c.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_DEFAULT);}}
    }
    @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);if(focused)immersive();}
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        int key=event.getKeyCode();
        if(key==KeyEvent.KEYCODE_VOLUME_UP||key==KeyEvent.KEYCODE_VOLUME_DOWN)return super.dispatchKeyEvent(event);
        if(event.getRepeatCount()>0&&event.getAction()==KeyEvent.ACTION_DOWN&&!(key>=KeyEvent.KEYCODE_DPAD_UP&&key<=KeyEvent.KEYCODE_DPAD_RIGHT))return true;
        final boolean down=event.getAction()==KeyEvent.ACTION_DOWN;
        final int sources=event.getSource()|(event.getDevice()==null?0:event.getDevice().getSources());
        final boolean controller=(sources&android.view.InputDevice.SOURCE_GAMEPAD)==android.view.InputDevice.SOURCE_GAMEPAD || (sources&android.view.InputDevice.SOURCE_JOYSTICK)==android.view.InputDevice.SOURCE_JOYSTICK;
        surface.queueEvent(()->{if(nativeKey(key,down,controller))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();return true;
    }
    private float axis(MotionEvent event,int axis){
        android.view.InputDevice device=event.getDevice();
        android.view.InputDevice.MotionRange range=device==null?null:device.getMotionRange(axis,event.getSource());
        float value=event.getAxisValue(axis);return Math.abs(value)<Math.max(.12f,range==null?0:range.getFlat())?0:Math.max(-1,Math.min(1,value));
    }
    @Override public boolean onGenericMotionEvent(MotionEvent event){
        if((event.getSource()&android.view.InputDevice.SOURCE_JOYSTICK)!=0&&event.getAction()==MotionEvent.ACTION_MOVE){
            final float x=axis(event,MotionEvent.AXIS_X),y=axis(event,MotionEvent.AXIS_Y),rx=axis(event,MotionEvent.AXIS_Z),ry=axis(event,MotionEvent.AXIS_RZ),hx=axis(event,MotionEvent.AXIS_HAT_X),hy=axis(event,MotionEvent.AXIS_HAT_Y);
            surface.queueEvent(()->{if(nativeAnalog(x,y,rx,ry,hx,hy))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();return true;
        }
        return super.onGenericMotionEvent(event);
    }
    @Override public void onBackPressed(){backRequested();}
    private void backRequested(){android.util.Log.i("PocketMvp","player system back");surface.queueEvent(()->{if(nativeKey(KeyEvent.KEYCODE_BACK,true,false))surface.setRenderMode(GLSurfaceView.RENDERMODE_CONTINUOUSLY);});surface.requestRender();}
    @Override protected void onResume(){super.onResume();surface.onResume();immersive();
        AudioManager audio=(AudioManager)getSystemService(AUDIO_SERVICE);
        focus=new android.media.AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_GAME).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build()).setOnAudioFocusChangeListener(change->{if(change<0){surface.queueEvent(()->nativeSuspend());surface.requestRender();}}).build();audio.requestAudioFocus(focus);
    }
    @Override protected void onPause(){surface.queueEvent(()->nativeSuspend());surface.onPause();if(focus!=null)((AudioManager)getSystemService(AUDIO_SERVICE)).abandonAudioFocusRequest(focus);super.onPause();}
}
