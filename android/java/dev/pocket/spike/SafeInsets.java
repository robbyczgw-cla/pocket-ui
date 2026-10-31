package dev.pocket.spike;

import android.content.Context;
import android.os.Build;
import android.view.WindowInsets;

/** One inset policy for the CPU launcher and EGL player, expressed in physical pixels. */
final class SafeInsets {
    /** The player keeps its D-pad and buttons out of the side Back-gesture zones. */
    static int[] read(Context context,WindowInsets insets){return read(context,insets,true);}
    /**
     * The launcher only needs taps there, so with {@code gestureSides} false it keeps the cutout, the rounded
     * corners and the bottom home-gesture bar clear and uses the full width; corners then only move the short
     * edges (top and bottom in portrait), which is where a phone's rounded corners actually cut content.
     */
    @SuppressWarnings("deprecation") static int[] read(Context context,WindowInsets insets,boolean gestureSides){
        int padding=(int)((gestureSides?12:8)*context.getResources().getDisplayMetrics().density);
        int[] safe={padding,padding,padding,padding};
        if(Build.VERSION.SDK_INT>=30){
            int types=WindowInsets.Type.displayCutout()|(gestureSides?WindowInsets.Type.systemGestures():WindowInsets.Type.mandatorySystemGestures());
            android.graphics.Insets edges=insets.getInsets(types);
            safe[0]=Math.max(safe[0],edges.left);safe[1]=Math.max(safe[1],edges.top);safe[2]=Math.max(safe[2],edges.right);safe[3]=Math.max(safe[3],edges.bottom);
        }else{
            safe[0]=Math.max(safe[0],insets.getStableInsetLeft());safe[1]=Math.max(safe[1],insets.getStableInsetTop());safe[2]=Math.max(safe[2],insets.getStableInsetRight());safe[3]=Math.max(safe[3],insets.getStableInsetBottom());
            if(Build.VERSION.SDK_INT>=28&&insets.getDisplayCutout()!=null){
                android.view.DisplayCutout cutout=insets.getDisplayCutout();
                safe[0]=Math.max(safe[0],cutout.getSafeInsetLeft());safe[1]=Math.max(safe[1],cutout.getSafeInsetTop());safe[2]=Math.max(safe[2],cutout.getSafeInsetRight());safe[3]=Math.max(safe[3],cutout.getSafeInsetBottom());
            }
        }
        if(Build.VERSION.SDK_INT>=31){
            // Radius-based bounds are conservative; unlike a fixed dp guess they follow the panel/OEM.
            for(int position=0;position<4;position++){
                android.view.RoundedCorner corner=insets.getRoundedCorner(position);if(corner==null)continue;
                int r=corner.getRadius();
                if(!gestureSides){
                    // The bars span the long side; a corner's radius only has to clear their ends (half the radius
                    // inside the short edge leaves the corner's curve outside every label).
                    boolean portrait=context.getResources().getConfiguration().orientation!=android.content.res.Configuration.ORIENTATION_LANDSCAPE;
                    int edge=portrait?((position==android.view.RoundedCorner.POSITION_TOP_LEFT||position==android.view.RoundedCorner.POSITION_TOP_RIGHT)?1:3)
                            :((position==android.view.RoundedCorner.POSITION_TOP_LEFT||position==android.view.RoundedCorner.POSITION_BOTTOM_LEFT)?0:2);
                    safe[edge]=Math.max(safe[edge],r/2);
                    continue;
                }
                int horizontal=(position==android.view.RoundedCorner.POSITION_TOP_LEFT||position==android.view.RoundedCorner.POSITION_BOTTOM_LEFT)?0:2;
                int vertical=(position==android.view.RoundedCorner.POSITION_TOP_LEFT||position==android.view.RoundedCorner.POSITION_TOP_RIGHT)?1:3;
                safe[horizontal]=Math.max(safe[horizontal],r);safe[vertical]=Math.max(safe[vertical],r);
            }
        }
        return safe;
    }
    private SafeInsets(){}
}
