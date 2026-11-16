//! The same signed release format and numeric version ordering on Linux and Android.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Release {
    pub version: String,
    /// Absolute, or relative to update.json.
    pub url: String,
    /// Additional signed sources, tried in order after `url`, with exactly the same size and SHA-256.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<String>,
    /// Signed sources preferred by new clients. Older clients keep using `url`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred_sources: Vec<String>,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}

impl Release {
    /// The distinct signed sources in attempt order. Resolve before deduplicating so
    /// an absolute mirror of the relative primary does not consume another attempt.
    pub fn download_urls(&self, manifest_url: &str) -> Vec<String> {
        let mut urls = Vec::new();
        for source in self.preferred_sources.iter().chain(std::iter::once(&self.url)).chain(&self.mirrors) {
            let url = resolve(manifest_url, source);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    #[serde(default)]
    pub app: Option<Release>,
    #[serde(default)]
    pub os: Option<Release>,
    /// The SD card image and the install package for a ROCKNIX card, for people (the website's download page, a
    /// checksum to compare); a device ignores both. docs/UPDATE-CHANNEL.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sd_image: Option<Release>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_zip: Option<Release>,
}

/// The APK's own version, as Android's app info shows it: `android:versionName` of android/AndroidManifest.xml, which
/// scripts/android/build-apk.sh writes before it builds the libraries (so the release's "0.4.0-android.3", not the crates'
/// Cargo versions). About and the RetroAchievements User-Agent on Android say this.
pub fn android_version_name() -> &'static str {
    const MANIFEST: &str = include_str!("../../android/AndroidManifest.xml");
    const KEY: &str = "android:versionName=\"";
    MANIFEST.split_once(KEY).and_then(|(_, rest)| rest.split_once('"')).map_or("unknown", |(v, _)| v)
}

/// True when `candidate` is a later version than `current`: number groups compared in order
/// ("0.10.0" > "0.9.3", "20261101" > "20261001"); anything else in the string is ignored.
pub fn newer(candidate: &str, current: &str) -> bool {
    let nums = |s: &str| -> Vec<u64> { s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect() };
    nums(candidate) > nums(current)
}

/// `url` as given when absolute, otherwise next to `manifest_url`.
pub fn resolve(manifest_url: &str, url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        let base = manifest_url.rsplit_once('/').map_or(manifest_url, |(b, _)| b);
        format!("{base}/{url}")
    }
}

pub fn parse_manifest(json: &[u8], sig_b64: &str, key: &[u8; 32]) -> Result<Manifest, String> {
    crate::signed_data::verify_signed(json, sig_b64, key).map_err(|_| "The update list isn't signed by us. Nothing was changed.".to_string())?;
    serde_json::from_slice(json).map_err(|e| format!("update list: {e}"))
}

/// Android's signed list (`updates/android/update.json` and its `.sig`), read through the app's Wi-Fi-only reader
/// (`UpdateInstaller.read`): the Mobile view's Updates page and the handheld view's quiet look share it.
#[cfg(target_os = "android")]
pub fn android_fetch(url: &str) -> Result<Manifest, String> {
    let read = |url: &str, limit: &str| -> Result<String, String> {
        let result = crate::platform::android::strings("updateRead", &[url, limit])?;
        result.strip_prefix("OK\n").map(String::from).ok_or_else(|| result.strip_prefix("ERR\n").unwrap_or("Couldn't reach the update server. Check Wi-Fi.").to_string())
    };
    let json = read(url, "1048576")?;
    let signature = read(&format!("{url}.sig"), "1024")?;
    let manifest = parse_manifest(json.as_bytes(), &signature, &crate::signed_data::PUBLIC_KEY)?;
    android_offer(&manifest, url)?;
    Ok(manifest)
}

/// Android can install only one bounded APK from this separate channel. Never interpret an OS entry as an APK.
pub fn android_offer(manifest: &Manifest, url: &str) -> Result<(), String> {
    let bad = || "The Android update list is invalid. Nothing was installed.".to_string();
    if manifest.format != 1 { return Err(bad()); }
    if let Some(app) = &manifest.app {
        if app.size == 0 || app.size > 100_000_000 || app.sha256.len() != 64 || !app.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || app.version.is_empty() || app.version.len() > 64 || !app.version.bytes().all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
            || app.notes.len() > 4096 || app.download_urls(url).len() > 4 { return Err(bad()); }
    }
    Ok(())
}
