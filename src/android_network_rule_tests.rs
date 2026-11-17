//! Android's big downloads (app updates, cores) and the "unmetered Wi-Fi" answer the Rust side asks for share one network
//! rule, `android/java/xyz/pocketui/app/NetworkRule.java`: the app's own unmetered Wi-Fi or Ethernet, a VPN on it included,
//! never mobile data. Until 0.5.0-android.6 the update installer picked the raw Wi-Fi network under a VPN
//! (`getAllNetworks`, `!TRANSPORT_VPN`) and opened its connections there, which a VPN without a bypass forbids: on the
//! Retroid Pocket Mini V2 with Tailscale every update check failed (17.11.2026). The rule's own cases are a JVM test
//! (`scripts/android/test/NetworkRuleTest.java`); this test keeps every Java file on it.

use std::path::Path;

fn java_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("android/java/xyz/pocketui/app");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("android java folder")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "java"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// No Java file looks for a network of its own beside the app's (the raw Wi-Fi under a VPN) or turns a VPN away; only
/// NetworkRule asks Android which network the app has and what it is.
#[test]
fn android_downloads_use_the_app_s_own_network_through_one_rule() {
    let files = java_files();
    assert!(files.iter().any(|(n, _)| n == "NetworkRule.java"), "the shared rule is there");
    for (name, text) in &files {
        assert!(!text.contains("getAllNetworks"), "{name}: picks a network beside the app's own (a VPN forbids that)");
        assert!(!text.contains("TRANSPORT_VPN"), "{name}: turns a VPN away; a VPN over Wi-Fi is the app's own network");
        if name != "NetworkRule.java" {
            assert!(!text.contains("getActiveNetwork") && !text.contains("NET_CAPABILITY_NOT_METERED"), "{name}: decides about the network itself instead of NetworkRule");
        }
    }
    // The three users: app updates, cores, and the Rust side's question.
    for (name, call) in [("UpdateInstaller.java", "NetworkRule.network("), ("CoreDownloads.java", "NetworkRule.network("), ("PocketActivity.java", "NetworkRule.network(")] {
        let text = &files.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}")).1;
        assert!(text.contains(call), "{name} asks NetworkRule");
    }
}
