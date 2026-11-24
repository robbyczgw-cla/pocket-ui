//! Android core packages use the store's catalog signature, freshness guard, hash and installer.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorePackage {
    #[serde(flatten)]
    pub package: crate::store::Package,
    pub core: String,
    systems: Vec<String>,
    library_sha256: String,
    corresponding_source: String,
    source_sha256: String,
}

#[derive(Debug)]
pub enum Error {
    InvalidPackage, Damaged, UnsafeArchive, WrongLibrary, WrongSource,
    MissingPspFiles, InvalidPspFile, PspHash,
    Signature(String), Catalog(String), Archive(String), Store(String),
    Io(std::io::Error), Json(serde_json::Error),
}
impl From<std::io::Error> for Error { fn from(error: std::io::Error) -> Self { Self::Io(error) } }
impl From<serde_json::Error> for Error { fn from(error: serde_json::Error) -> Self { Self::Json(error) } }
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidPackage => "Invalid Android core package",
            Self::Damaged => "Download failed verification. Nothing was installed.",
            Self::UnsafeArchive => "Invalid Android core archive",
            Self::WrongLibrary => "Core checksum does not match",
            Self::WrongSource => "Core source reference does not match",
            Self::MissingPspFiles => "Missing PSP system files",
            Self::InvalidPspFile => "Invalid PSP system file",
            Self::PspHash => "PSP system file checksum does not match",
            Self::Signature(_) => "Store catalogue failed signature check, so it was skipped. Nothing changed.",
            Self::Catalog(message) | Self::Archive(message) | Self::Store(message) => message,
            Self::Io(error) => return error.fmt(f),
            Self::Json(error) => return error.fmt(f),
        })
    }
}
impl std::error::Error for Error {}

/// What a player reads when the download channel can't be reached. The Android side reports the transport's own words
/// ("HTTP 404", a host name, a TLS text); only its full sentences (they end in a full stop) reach the screen as they are.
pub fn download_failure(reason: &str) -> String {
    let reason = reason.trim();
    if let Some(code) = reason.strip_prefix("HTTP ") {
        return if matches!(code.trim(), "404" | "403" | "410") {
            "The extra systems can't be downloaded yet. Try again later; every system already in the app works as it is."
        } else {
            "The download server isn't answering right now. Try again later."
        }.to_string();
    }
    let sentence = reason.ends_with('.') && reason.len() < 120 && reason.starts_with(|c: char| c.is_uppercase()) && !reason.contains(['"', '/', ':']);
    if sentence { reason.to_string() } else { "Download didn't finish. Check Wi-Fi and try again.".to_string() }
}

fn destination(data: &Path, core: &str) -> PathBuf { data.join("android-cores").join(core) }
#[cfg(test)]
fn library(data: &Path, core: &str) -> PathBuf { destination(data, core).join(format!("lib{core}_libretro.so")) }

pub fn packages(json: &[u8]) -> Result<Vec<CorePackage>, Error> {
    #[derive(Deserialize)] struct List { packages: Vec<CorePackage> }
    let list: List = serde_json::from_slice(json)?;
    let mut cores = std::collections::HashSet::new();
    let hash = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    for p in &list.packages {
        if p.core.is_empty() || p.core.len() > 40 || !p.core.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
            || !cores.insert(&p.core) || p.systems.is_empty() || p.package.category != "android-cores" || p.package.run.is_some() || !p.core.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            || p.package.id != format!("android-core-{}", p.core) || p.package.kind != crate::store::Kind::Tar
            || p.package.size == 0 || p.package.size > 128 * 1024 * 1024
            || !p.corresponding_source.ends_with(".tar.gz") || p.corresponding_source.contains('/') || p.corresponding_source.contains('\\')
            || !hash(&p.source_sha256) || !hash(&p.library_sha256) || !hash(&p.package.sha256) {
            return Err(Error::InvalidPackage);
        }
    }
    Ok(list.packages)
}

pub fn verified_catalog(json: &[u8], sig: &str, url: &str, data: &Path) -> Result<Vec<CorePackage>, Error> {
    let catalog = crate::store::verify_catalog(json, sig, &crate::store::PUBLIC_KEY)
        .map_err(|reason| if reason.starts_with("catalog signature") { Error::Signature(reason) } else { Error::Catalog(reason) })?;
    let packages = packages(json)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |t| t.as_secs());
    crate::store::check_fresh(&catalog, url, data, now).map_err(Error::Catalog)?;
    Ok(packages)
}

static INSTALL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A core the APK carries now that was a download before (PSP, DS, N64 and Dreamcast until 0.5.0-android.5): the APK's
/// library wins (`corePath` asks for it first), and the downloaded copy goes, its files, its folder and its store record,
/// so nothing is there twice. Saves, states, core options and the PSP system folder live elsewhere (`saves/`, `states/`,
/// `core-options/`, `bios/`) and stay. With no download offered at all (`offers_left` false), unfinished downloads
/// (`store/partials`) go too. At every start of either view; nothing is written when there is nothing to retire.
/// Returns the cores retired.
pub fn retire_bundled(data: &Path, bundled: impl Fn(&str) -> bool, offers_left: bool) -> Vec<String> {
    let _install = INSTALL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut retired = Vec::new();
    let downloaded: Vec<String> = std::fs::read_dir(data.join("android-cores")).into_iter().flatten().flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|core| !core.is_empty() && core.len() <= 40 && core.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'))
        .collect();
    for core in downloaded {
        if !bundled(&core) { continue; }
        // The store removes exactly the files it installed; a damaged inventory refuses, and the folder goes anyway.
        if let Err(e) = crate::store::Store::open(data).uninstall(&format!("android-core-{core}")) {
            eprintln!("retire downloaded {core}: store: {e}");
        }
        match std::fs::remove_dir_all(destination(data, &core)) {
            Ok(()) => retired.push(core),
            Err(e) => eprintln!("retire downloaded {core}: {e}"),
        }
    }
    if !offers_left {
        let _ = std::fs::remove_dir_all(data.join("store/partials"));
    }
    retired
}

pub fn install(data: &Path, p: &CorePackage, archive: &[u8]) -> Result<(), Error> {
    let _install = INSTALL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Never trust a catalog's desktop destination on Android. Each core owns one private folder.
    packages(&serde_json::json!({"packages":[p]}).to_string().into_bytes())?;
    if archive.len() as u64 != p.package.size || crate::store::sha256_hex(archive) != p.package.sha256 {
        return Err(Error::Damaged);
    }
    let plain = crate::untar::decompress(archive).map_err(Error::Archive)?;
    let entries = crate::untar::entries(&plain).map_err(Error::Archive)?;
    drop(plain);
    let filename = format!("lib{}_libretro.so", p.core);
    if !entries.iter().any(|e| !e.dir && e.path == filename)
        || entries.iter().any(|e| !e.dir && e.path != filename && e.path != "core.json"
            && !e.path.starts_with("system/PPSSPP/") && !e.path.starts_with("licences/")) {
        return Err(Error::UnsafeArchive);
    }
    // Stage and validate the library and PPSSPP assets before the store replaces an installed version.
    let check = data.join("store/core-check");
    std::fs::create_dir_all(data.join("store"))?;
    if check.exists() { std::fs::remove_dir_all(&check)?; }
    let result = (|| {
        crate::untar::unpack(archive, &check).map_err(Error::Archive)?;
        if crate::store::sha256_file(&check.join(&filename))? != p.library_sha256 {
            return Err(Error::WrongLibrary);
        }
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(check.join("core.json"))?)?;
        if manifest["id"] != p.core || manifest["corresponding_source"] != p.corresponding_source
            || manifest["source_sha256"] != p.source_sha256 { return Err(Error::WrongSource); }
        if p.core == "ppsspp" {
            let assets = manifest["system_assets"].as_array().filter(|a| !a.is_empty()).ok_or(Error::MissingPspFiles)?;
            for a in assets {
                let path = a["path"].as_str().ok_or(Error::InvalidPspFile)?;
                let file = check.join("system/PPSSPP").join(path);
                if Path::new(path).is_absolute() || Path::new(path).components().any(|c| matches!(c, std::path::Component::ParentDir))
                    || crate::store::sha256_file(&file)? != a["sha256"].as_str().unwrap_or("") {
                    return Err(Error::PspHash);
                }
            }
        }
        let mut package = p.package.clone();
        package.dest = destination(data, &p.core).to_string_lossy().into_owned();
        crate::store::Store::open(data).install_bytes(&package, archive).map_err(Error::Store)
    })();
    // Only disposable verification staging lives here; a later attempt clears any residue again.
    let _ = std::fs::remove_dir_all(&check);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The catalog is not on the website yet (or the site is down): the player said "HTTP 404" or a host name.
    #[test]
    fn a_missing_catalog_or_a_failing_network_reads_as_plain_words_in_every_language() {
        use crate::i18n::{tr_in, Lang};
        let cases = [
            ("HTTP 404", "The extra systems can't be downloaded yet. Try again later; every system already in the app works as it is."),
            ("HTTP 503", "The download server isn't answering right now. Try again later."),
            ("Unable to resolve host \"pocketui.xyz\": No address associated with hostname", "Download didn't finish. Check Wi-Fi and try again."),
            ("HTTPS required", "Download didn't finish. Check Wi-Fi and try again."),
            ("Downloads wait for unmetered Wi-Fi.", "Downloads wait for unmetered Wi-Fi."),
            ("Download cancelled. Choose it again to continue.", "Download cancelled. Choose it again to continue."),
        ];
        for (raw, said) in cases {
            let shown = download_failure(raw);
            assert_eq!(shown, said, "{raw}");
            assert!(!shown.contains("HTTP") && !shown.contains("404") && !shown.contains("pocketui.xyz"), "{raw}: {shown}");
            for lang in [Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko] { assert_ne!(tr_in(lang, &shown), shown, "{lang:?}: {shown}"); }
        }
    }
    #[test]
    fn checked_core_installs_privately_and_a_changed_download_keeps_the_working_core() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("libtest_libretro.so"), b"working core").unwrap();
        std::fs::write(source.join("core.json"), br#"{"id":"test","corresponding_source":"source.tar.gz","source_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#).unwrap();
        let archive = root.path().join("core.tar.gz");
        assert!(std::process::Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&source).arg(".").status().unwrap().success());
        let bytes = std::fs::read(archive).unwrap();
        let json = serde_json::json!({"packages":[{
            "id":"android-core-test", "name":"Test", "category":"android-cores", "version":"1", "description":"", "size":bytes.len(),
            "url":"test.tar.gz", "sha256":crate::store::sha256_hex(&bytes), "kind":"tar", "dest":"/never-write-here",
            "core":"test", "systems":["test"], "library_sha256":crate::store::sha256_hex(b"working core"),
            "corresponding_source":"source.tar.gz", "source_sha256":"a".repeat(64)
        }]}).to_string();
        let package = packages(json.as_bytes()).unwrap().remove(0);
        let data = root.path().join("data");
        install(&data, &package, &bytes).unwrap();
        assert_eq!(std::fs::read(library(&data, "test")).unwrap(), b"working core");
        let mut damaged = bytes.clone(); damaged[20] ^= 1;
        assert!(install(&data, &package, &damaged).is_err());
        let mut wrong_library = package.clone(); wrong_library.library_sha256 = "0".repeat(64);
        assert!(install(&data, &wrong_library, &bytes).is_err());
        assert_eq!(std::fs::read(library(&data, "test")).unwrap(), b"working core", "refused updates leave the playable core intact");
        let mut redirect = package.clone(); redirect.package.category = "tools".into();
        assert!(install(&data, &redirect, &bytes).is_err(), "a core must not change into a tool and move its playable library");
        assert_eq!(std::fs::read(library(&data, "test")).unwrap(), b"working core");
        assert!(verified_catalog(json.as_bytes(), "not signed", "file:///test/catalog.json", &data).is_err());
        assert!(crate::store::Store::open(&data).installed.contains_key("android-core-test"));
    }

    /// 0.5.x: the four download cores are in the APK. A phone that downloaded N64 and Dreamcast before keeps one copy of each, the
    /// APK's: the download's files, folder and store record go, an unfinished download too; saves, states, core options
    /// and the PSP system files stay where they are.
    #[test]
    fn a_core_the_apk_now_carries_replaces_its_download_and_keeps_saves_and_options() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let mut kept = Vec::new();
        for core in ["mupen64plus_next", "flycast", "future"] {
            let source = root.path().join(core);
            std::fs::create_dir_all(source.join("licences")).unwrap();
            std::fs::write(source.join(format!("lib{core}_libretro.so")), core.as_bytes()).unwrap();
            std::fs::write(source.join("licences/LICENSE"), b"licence").unwrap();
            std::fs::write(source.join("core.json"), format!(r#"{{"id":"{core}","corresponding_source":"s.tar.gz","source_sha256":"{}"}}"#, "a".repeat(64))).unwrap();
            let archive = root.path().join(format!("{core}.tar.gz"));
            assert!(std::process::Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&source).arg(".").status().unwrap().success());
            let bytes = std::fs::read(archive).unwrap();
            let json = serde_json::json!({"packages":[{
                "id":format!("android-core-{core}"), "name":core, "category":"android-cores", "version":"1", "description":"", "size":bytes.len(),
                "url":"c.tar.gz", "sha256":crate::store::sha256_hex(&bytes), "kind":"tar", "dest":"/x",
                "core":core, "systems":[core], "library_sha256":crate::store::sha256_hex(core.as_bytes()),
                "corresponding_source":"s.tar.gz", "source_sha256":"a".repeat(64)
            }]}).to_string();
            install(&data, &packages(json.as_bytes()).unwrap().remove(0), &bytes).unwrap();
        }
        for (path, body) in [("saves/0123456789abcdef/game.srm", "save"), ("states/0123456789abcdef/game.state1", "state"),
                             ("core-options/n64.json", "{}"), ("bios/PPSSPP/compat.ini", "assets"), ("store/partials/ab.part", "half")] {
            std::fs::create_dir_all(data.join(path).parent().unwrap()).unwrap();
            std::fs::write(data.join(path), body).unwrap();
            kept.push((path, body));
        }
        // PSP's download also brought its system files, and the core may have written beside them: the whole folder goes.
        std::fs::create_dir_all(data.join("android-cores/ppsspp/system/PPSSPP/flash0")).unwrap();
        std::fs::write(data.join("android-cores/ppsspp/libppsspp_libretro.so"), b"psp").unwrap();
        std::fs::write(data.join("android-cores/ppsspp/system/PPSSPP/flash0/written-by-the-core"), b"cache").unwrap();
        let bundled = |core: &str| ["mupen64plus_next", "ppsspp", "melondsds", "flycast"].contains(&core);
        let mut retired = retire_bundled(&data, bundled, false);
        retired.sort();
        assert_eq!(retired, ["flycast", "mupen64plus_next", "ppsspp"]);
        assert!(!data.join("android-cores/ppsspp").exists());
        assert!(!data.join("android-cores/mupen64plus_next").exists() && !data.join("android-cores/flycast").exists(), "no second copy");
        assert!(library(&data, "future").is_file(), "a core the APK doesn't carry stays downloaded");
        let store = crate::store::Store::open(&data);
        assert!(!store.installed.contains_key("android-core-mupen64plus_next") && !store.installed.contains_key("android-core-flycast"));
        assert!(store.installed.contains_key("android-core-future"));
        assert!(!data.join("store/partials").exists(), "an unfinished download of a core nobody offers goes");
        for (path, body) in kept.iter().filter(|(p, _)| !p.starts_with("store/")) {
            assert_eq!(std::fs::read_to_string(data.join(path)).unwrap(), *body, "{path} stays");
        }
        // The next start finds nothing to do and writes nothing.
        let before = std::fs::metadata(data.join("store/installed.json")).unwrap().modified().unwrap();
        assert!(retire_bundled(&data, bundled, false).is_empty());
        assert_eq!(std::fs::metadata(data.join("store/installed.json")).unwrap().modified().unwrap(), before);
    }
}
