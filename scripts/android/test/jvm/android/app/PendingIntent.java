package android.app;import android.content.*;
public final class PendingIntent {public static final int FLAG_UPDATE_CURRENT=1,FLAG_MUTABLE=2;private final Intent intent;private PendingIntent(Intent i){intent=i;}public static PendingIntent getBroadcast(Context c,int r,Intent i,int f){return new PendingIntent(i);}public IntentSender getIntentSender(){return new IntentSender(intent);}}
