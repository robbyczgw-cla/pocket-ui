package xyz.pocketui.app;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;

/**
 * The one rule for big downloads (app updates, cores): the app's own network, as long as it is Wi-Fi or Ethernet that
 * costs nothing (unmetered) and reaches the internet; never mobile data. A VPN on it is fine: Android reports a VPN over
 * Wi-Fi with the Wi-Fi transport and the unmetered capability of the network under it, and the app's connections go
 * through it as every other app's do.
 *
 * Until 0.5.0-android.6 the update installer looked for the raw Wi-Fi network under any VPN and opened its connections
 * there. Under a VPN that doesn't allow apps around it (Tailscale, on the Retroid Pocket Mini V2, 17.11.2026) that is not
 * permitted, and every update check failed with "Couldn't reach the update server". Test: scripts/android/test/NetworkRuleTest.java.
 */
public final class NetworkRule {
    private NetworkRule() {}

    /** Whether a network with these properties may carry a big download. */
    public static boolean allows(boolean wifi, boolean ethernet, boolean unmetered, boolean internet) {
        return (wifi || ethernet) && unmetered && internet;
    }

    /** {@link #allows} for Android's capabilities of a network (null: no network). */
    static boolean allows(NetworkCapabilities c) {
        return c != null && allows(c.hasTransport(NetworkCapabilities.TRANSPORT_WIFI), c.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                c.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED), c.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET));
    }

    /** The app's own network when it may carry a big download, else null. */
    static Network network(Context context) {
        ConnectivityManager cm = (ConnectivityManager)context.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (cm == null) return null;
        Network n = cm.getActiveNetwork();
        return n != null && allows(cm.getNetworkCapabilities(n)) ? n : null;
    }

    /** Whether network {@code n} still may carry it (checked while a download runs: Wi-Fi gone, metered now). */
    static boolean still(Context context, Network n) {
        ConnectivityManager cm = (ConnectivityManager)context.getSystemService(Context.CONNECTIVITY_SERVICE);
        return cm != null && n != null && allows(cm.getNetworkCapabilities(n));
    }
}
