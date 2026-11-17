package xyz.pocketui.app;

/**
 * Big downloads go over the app's own unmetered Wi-Fi or Ethernet, a VPN on it included, never over mobile data
 * (NetworkRule). Runs on the JVM: javac -cp android.jar -d out android/java/xyz/pocketui/app/NetworkRule.java
 * scripts/android/test/NetworkRuleTest.java && java -cp out:android.jar xyz.pocketui.app.NetworkRuleTest
 */
public final class NetworkRuleTest {
    static void require(boolean value, String message) { if (!value) throw new AssertionError(message); }

    /** A network as Android describes it to the app. */
    static final class Net {
        final String name; final boolean wifi, ethernet, cellular, vpn, unmetered, internet;
        Net(String name, boolean wifi, boolean ethernet, boolean cellular, boolean vpn, boolean unmetered, boolean internet) {
            this.name = name; this.wifi = wifi; this.ethernet = ethernet; this.cellular = cellular; this.vpn = vpn; this.unmetered = unmetered; this.internet = internet;
        }
    }

    /** The update installer's rule until 0.5.0-android.6: the raw Wi-Fi network, never one with a VPN. */
    static boolean oldUpdateRule(Net n) { return n.wifi && !n.vpn; }

    public static void main(String[] args) {
        // A VPN over home Wi-Fi (Tailscale, WireGuard, an ad blocker): Android reports WIFI|VPN, unmetered, internet. It is
        // the app's own network; the raw Wi-Fi under it is not one the app may use.
        Net vpnOverWifi = new Net("VPN over Wi-Fi", true, false, false, true, true, true);
        require(!oldUpdateRule(vpnOverWifi), "red: the old rule refused the app's own network under a VPN");
        require(NetworkRule.allows(vpnOverWifi.wifi, vpnOverWifi.ethernet, vpnOverWifi.unmetered, vpnOverWifi.internet), "a VPN over Wi-Fi carries updates");
        Net[] yes = {
            new Net("home Wi-Fi", true, false, false, false, true, true),
            vpnOverWifi,
            new Net("Ethernet dock", false, true, false, false, true, true),
            new Net("VPN over Ethernet", false, true, false, true, true, true),
        };
        Net[] no = {
            new Net("mobile data", false, false, true, false, false, true),
            new Net("VPN over mobile data", false, false, true, true, false, true),
            new Net("phone hotspot (metered Wi-Fi)", true, false, false, false, false, true),
            new Net("Wi-Fi without internet (captive portal)", true, false, false, false, true, false),
            new Net("unmetered mobile plan", false, false, true, false, true, true),
        };
        for (Net n : yes) require(NetworkRule.allows(n.wifi, n.ethernet, n.unmetered, n.internet), "allowed: " + n.name);
        for (Net n : no) require(!NetworkRule.allows(n.wifi, n.ethernet, n.unmetered, n.internet), "refused: " + n.name);
        System.out.println("NetworkRuleTest: ok");
    }
}
