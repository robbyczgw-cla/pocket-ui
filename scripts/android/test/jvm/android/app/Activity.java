package android.app;import android.content.*;
public abstract class Activity extends Context {public Intent getIntent(){return new Intent();}public void runOnUiThread(Runnable r){r.run();}public void startActivityForResult(Intent i,int request){throw new UnsupportedOperationException("Activity runtime");}}
