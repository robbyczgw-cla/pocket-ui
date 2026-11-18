package xyz.pocketui.app;

import android.content.Context;
import android.os.Build;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.os.VibratorManager;
import android.view.InputDevice;

/** The player's game rumble: a controller's own vibrator, else (a built-in pad has none of its own) the handheld's. */
final class Rumble {
    private Rumble() {}

    static Vibrator handheld(Context context) {
        if (Build.VERSION.SDK_INT >= 31) {
            VibratorManager m = (VibratorManager) context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE);
            return m == null ? null : m.getDefaultVibrator();
        }
        return (Vibrator) context.getSystemService(Context.VIBRATOR_SERVICE);
    }

    static Vibrator of(Context context, String target) {
        if (!"motor".equals(target)) {
            try {
                InputDevice d = InputDevice.getDevice(Integer.parseInt(target));
                if (d != null) {
                    Vibrator v = Build.VERSION.SDK_INT >= 31 ? d.getVibratorManager().getDefaultVibrator() : d.getVibrator();
                    if (v != null && v.hasVibrator()) return v;
                    // The handheld's own pad: its motor is the handheld's.
                    if (Build.VERSION.SDK_INT >= 29 && d.isExternal()) return null;
                }
            } catch (NumberFormatException e) {
                return null;
            }
        }
        return handheld(context);
    }

    static String play(Context context, String target, int magnitude, long ms) {
        Vibrator v = of(context, target);
        if (v == null || !v.hasVibrator()) return "no vibrator for " + target;
        try {
            if (magnitude <= 0 || ms <= 0) {
                v.cancel();
                return "";
            }
            int amplitude = v.hasAmplitudeControl() ? Math.max(1, Math.min(255, (int) ((long) magnitude * 255 / 65535))) : VibrationEffect.DEFAULT_AMPLITUDE;
            v.vibrate(VibrationEffect.createOneShot(ms, amplitude));
            return "";
        } catch (RuntimeException e) {
            return e.toString();
        }
    }
}
