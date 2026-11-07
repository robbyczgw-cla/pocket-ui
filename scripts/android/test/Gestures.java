package dev.pocket.qa;

import android.os.SystemClock;
import android.view.InputEvent;
import android.view.InputDevice;
import android.view.MotionEvent;

/** Shell-only emulator probe. Never packaged in the APK. */
public final class Gestures {
    public static void main(String[] args) throws Exception {
        Class<?> manager=Class.forName("android.hardware.input.InputManagerGlobal");
        Object input=manager.getDeclaredMethod("getInstance").invoke(null);
        java.lang.reflect.Method inject=manager.getMethod("injectInputEvent",InputEvent.class,int.class);
        float cx=Float.parseFloat(args[0]),cy=Float.parseFloat(args[1]);
        long down=SystemClock.uptimeMillis();
        MotionEvent.PointerProperties[] props=new MotionEvent.PointerProperties[2];
        MotionEvent.PointerCoords[] points=new MotionEvent.PointerCoords[2];
        for(int i=0;i<2;i++) {props[i]=new MotionEvent.PointerProperties();props[i].id=i;props[i].toolType=MotionEvent.TOOL_TYPE_FINGER;points[i]=new MotionEvent.PointerCoords();points[i].y=cy;points[i].pressure=1;points[i].size=0.1f;}
        for(int step=0;step<15;step++) {
            float distance=30+step*9;points[0].x=cx-distance;points[1].x=cx+distance;
            int count=step==0||step==14?1:2;
            int action=step==0?MotionEvent.ACTION_DOWN:step==1?MotionEvent.ACTION_POINTER_DOWN|(1<<MotionEvent.ACTION_POINTER_INDEX_SHIFT):step==13?MotionEvent.ACTION_POINTER_UP|(1<<MotionEvent.ACTION_POINTER_INDEX_SHIFT):step==14?MotionEvent.ACTION_UP:MotionEvent.ACTION_MOVE;
            MotionEvent event=MotionEvent.obtain(down,SystemClock.uptimeMillis(),action,count,props,points,0,0,1,1,0,0,InputDevice.SOURCE_TOUCHSCREEN,0);
            if(!Boolean.TRUE.equals(inject.invoke(input,event,2)))throw new AssertionError("Pinch injection refused");
            event.recycle();Thread.sleep(35);
        }
    }
}
