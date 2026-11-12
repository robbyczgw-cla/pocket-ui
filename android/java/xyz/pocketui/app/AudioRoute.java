package xyz.pocketui.app;

import android.content.Context;
import android.media.AudioDeviceInfo;
import android.media.AudioManager;

/** Identify the stream's actual output, not merely the absence of a Bluetooth connection. */
final class AudioRoute {
    static String localOutput(Context context, String streamDevice) {
        final int id;
        try { id = Integer.parseInt(streamDevice); } catch (NumberFormatException invalid) { return "false"; }
        AudioManager audio = (AudioManager)context.getSystemService(Context.AUDIO_SERVICE);
        for (AudioDeviceInfo device : audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)) {
            if (device.getId() != id) continue;
            int type = device.getType();
            return Boolean.toString(type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER
                || type == AudioDeviceInfo.TYPE_WIRED_HEADSET || type == AudioDeviceInfo.TYPE_WIRED_HEADPHONES);
        }
        return "false";
    }
}
