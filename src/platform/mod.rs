//! OS adapters. Keep target selection here instead of in shared UI and matching code.
#[cfg(any(target_os = "linux", target_os = "emscripten"))]
#[path = "linux_http.rs"]
pub(crate) mod http;

// Android's handheld view runs the Linux app: its adapters, and the Activity bridge below beside them.
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
pub use linux::*;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(all(target_os = "android", not(feature = "handheld")))]
pub use android::{data_dir, is_speaker, sink_volume, wiki_json, console_buttons, screenshot_hint};
// The handheld view keeps the Linux app's own adapters and the rest (`linux`), but what Android answers for itself is
// Android's: whether the sound goes to the speaker and the media volume (`AudioManager`), not ROCKNIX's PipeWire.
#[cfg(all(target_os = "android", feature = "handheld"))]
pub use android::{data_dir, is_speaker, sink_volume};
// Android, the handheld view too: HTTPS through the Activity (covers, pictures, descriptions), no curl.
#[cfg(target_os = "android")]
#[path = "android_http.rs"]
pub(crate) mod http;

#[cfg(any(target_os = "linux", target_os = "emscripten"))]
#[cfg_attr(not(target_os = "emscripten"), path = "linux_audio.rs")] #[cfg_attr(target_os = "emscripten", path = "../../web/web_audio.rs")]
pub(crate) mod audio_backend;
// Android's handheld view sounds through AAudio like the Mobile view: PulseAudio/PipeWire don't exist there.
#[cfg(target_os="android")]
#[path="android_audio.rs"]
pub(crate) mod audio_backend;

#[cfg(any(target_os = "android", test))]
pub mod android_pointer;

#[cfg(any(target_os = "android", test))]
pub mod android_earlier;

#[cfg(any(target_os = "android", all(test, target_os = "linux")))]
pub mod android_content;

#[cfg(test)]
mod tests {
    /// Android has no curl (review 19.11. #1: the starter pack failed there with a raw "No such file or directory").
    /// Every place that runs curl is listed with why Android's handheld view never gets there, or which bridge it takes
    /// there. A new curl needs the Activity's HTTPS on Android (`http`, `update::Tools::bridge`) and a line here.
    const CURL_PLACES: &[(&str, usize, &str)] = &[
        ("achievements/http.rs", 2, "not(target_os = \"android\"): Android asks RaHttp"),
        ("j2me_java.rs", 1, "ROCKNIX's freej2me script only (`needed`); Android's phone games are the player's"),
        ("net_check.rs", 1, "the Wi-Fi step's sign-in check; Android's setup has no Wi-Fi step, `refine` only follows a curl that ran"),
        ("platform/linux.rs", 1, "Linux adapters"),
        ("platform/linux_http.rs", 2, "Linux's http; Android has android_http.rs"),
        ("portal.rs", 1, "the Wi-Fi sign-in sheet (setup's Wi-Fi step, Linux)"),
        ("ports.rs", 1, "PortMaster: `ports::offered` is false on Android"),
        ("romm_net.rs", 2, "RomM's Curl is Linux's; Android's `romm_net::system` is AndroidNet (RommHttp.java)"),
        ("store.rs", 2, "the Store's catalogue and packages; Android installs cores from bytes (`android_cores`)"),
        ("sync.rs", 1, "Sync: ANDROID_HIDES \"sync\""),
        ("update.rs", 5, "3 on Linux (Android's list is `update_channel::android_fetch`; files go through `Tools::bridge`), 2 test probes"),
        ("update/brick_assets.rs", 1, "a test of the Brick's bundled curl"),
    ];

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() { rust_files(&path, out) } else if path.extension().is_some_and(|e| e == "rs") { out.push(path) }
        }
    }

    /// The places whose curl has no `--connect-timeout` of its own, and why a silent network still ends it soon.
    const SHORT_ANYWAY: &[(&str, &str)] = &[
        ("net_check.rs", "the sign-in check: --max-time 4 in all"),
        ("platform/linux.rs", "the wiki text: --max-time 10 in all"),
        ("portal.rs", "the sign-in sheet: its config file sets connect-timeout 5 and max-time 10 (`Session::run`)"),
        ("sync.rs", "Syncthing on this Pocket (localhost): -m with each call's 2 to 15 s"),
        ("update/brick_assets.rs", "a test that the bundled curl starts"),
    ];

    /// Every curl that reaches out gives up within seconds on a network that drops the packets (a hotel's before the
    /// sign-in): `--connect-timeout`, the downloads' own `download_args` (tested there), or a short limit in all
    /// (`SHORT_ANYWAY`).
    /// The covers' and the Store catalogue's had only `--max-time 60`, so three covers held the worker for 3 minutes
    /// (review 21.11.2026).
    #[test]
    fn every_curl_gives_up_soon_on_a_silent_network() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let mut slow = Vec::new();
        for f in &files {
            let rel = f.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
            if SHORT_ANYWAY.iter().any(|(file, _)| *file == rel) {
                continue;
            }
            let text = std::fs::read_to_string(f).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let is_curl = l.contains("Command::new(") && l.split("Command::new(").nth(1).is_some_and(|a| a.split(')').next().is_some_and(|a| a.contains("curl")));
                if !is_curl {
                    continue;
                }
                // The command up to where it runs.
                let mut call = String::new();
                for l in &lines[i..(i + 40).min(lines.len())] {
                    call.push_str(l);
                    if l.contains(".output()") || l.contains(".spawn()") || l.contains(".status()") {
                        break;
                    }
                }
                // `--version` asks the program, not the network (the tests' probes for a curl).
                let soon = call.contains("--connect-timeout") || call.contains("download_args(") || call.contains("\"--version\"");
                if !soon {
                    slow.push(format!("{rel}:{}", i + 1));
                }
            }
        }
        assert!(slow.is_empty(), "curl without --connect-timeout waits out its whole --max-time on a silent network: {slow:?}");
        for (file, why) in SHORT_ANYWAY {
            assert!(src.join(file).exists() && !why.is_empty(), "{file} is gone: take it off SHORT_ANYWAY");
        }
    }

    #[test]
    fn every_curl_has_an_android_way() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let mut found: Vec<(String, usize)> = files.iter().filter_map(|f| {
            let text = std::fs::read_to_string(f).ok()?;
            let n = text.lines().filter(|l| l.contains("Command::new(") && l.split("Command::new(").nth(1).is_some_and(|a| a.split(')').next().is_some_and(|a| a.contains("curl")))).count();
            (n > 0).then(|| (f.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/"), n))
        }).collect();
        found.sort();
        let listed: Vec<(String, usize)> = CURL_PLACES.iter().map(|(f, n, _)| (f.to_string(), *n)).collect();
        assert_eq!(found, listed, "a curl was added or moved: on Android it needs the Activity's HTTPS (platform::http, \
            update::Tools::bridge); then list it in CURL_PLACES with why Android is fine");
    }
}
