package dev.pocket.spike;

import android.app.Service;
import android.content.Intent;
import android.os.IBinder;

/** Brings up the `:player` process and its library ahead of PlayerActivity, while the launcher's start animation plays. */
public final class PlayerWarm extends Service {
    @Override public IBinder onBind(Intent intent){return null;}
    @Override public int onStartCommand(Intent intent,int flags,int id){
        try{Class.forName("dev.pocket.spike.PlayerActivity");}catch(ClassNotFoundException|LinkageError e){android.util.Log.w("PocketMvp","player warm-up",e);}
        stopSelf(id);
        return START_NOT_STICKY;
    }
}
