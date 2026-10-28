//! PortMaster ports, installed through PortMaster's own `harbourmaster` CLI (it checks md5s, fetches
//! runtimes and writes the launch script). Ready-to-run ports and ports that need the player's own game
//! files (`needs_files`; what to copy where is `port_files`).
//! Catalog: PortMaster-New `ports.json`, cached in the data directory for a day.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

pub const PORTS_DIR: &str = "/storage/roms/ports";
const CATALOG_URL: &str = "https://github.com/PortsMaster/PortMaster-New/releases/latest/download/ports.json";
const PM_DIR: &str = "/storage/roms/ports/PortMaster";

#[derive(Clone, Debug, Default)]
pub struct Port {
    /// Catalog id, e.g. "2048.zip".
    pub name: String,
    pub title: String,
    pub desc: String,
    pub genres: Vec<String>,
    pub size: u64,
    /// First installed item, e.g. "2048.sh"; its presence means installed.
    pub launcher: String,
    /// The port's folder in roms/ports, e.g. "sprout" (its first item that is not a script).
    pub dir: String,
    /// Not ready to run: the player has to copy files from their own copy of the game.
    pub needs_files: bool,
    /// PortMaster's instructions for those files (`attr.inst`), as they are.
    pub instructions: String,
}

#[derive(Deserialize)]
struct Catalog {
    ports: std::collections::HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    items: Vec<String>,
    attr: Attr,
    source: Source,
}

#[derive(Deserialize)]
struct Attr {
    title: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    rtr: bool,
    #[serde(default)]
    arch: Vec<String>,
    #[serde(default)]
    exp: bool,
    #[serde(default)]
    inst: Option<String>,
}

#[derive(Deserialize)]
struct Source {
    #[serde(default)]
    size: u64,
}

/// Non-experimental aarch64 ports, ready to run or not, sorted by title.
pub fn parse_catalog(json: &[u8]) -> Vec<Port> {
    let Ok(catalog) = serde_json::from_slice::<Catalog>(json) else { return Vec::new() };
    let mut ports: Vec<Port> = catalog
        .ports
        .into_values()
        .filter(|e| !e.attr.exp && e.attr.arch.iter().any(|a| a == "aarch64"))
        .filter_map(|e| {
            let launcher = e.items.iter().find(|i| i.ends_with(".sh"))?.clone();
            let dir = port_dir(&e.items)?;
            Some(Port {
                name: e.name,
                title: e.attr.title,
                desc: e.attr.desc.split_whitespace().collect::<Vec<_>>().join(" "),
                genres: e.attr.genres,
                size: e.source.size,
                launcher,
                dir,
                needs_files: !e.attr.rtr,
                instructions: e.attr.inst.unwrap_or_default().replace("\r\n", "\n").trim().to_string(),
            })
        })
        .collect();
    ports.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    ports
}

/// Loads the catalog, downloading it again when the cached copy is older than a day.
pub fn catalog(data_dir: &Path) -> Result<Vec<Port>, String> {
    let cache = data_dir.join("ports.json");
    let fresh = std::fs::metadata(&cache)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < Duration::from_secs(86_400));
    if !fresh {
        let _ = std::fs::create_dir_all(data_dir);
        let tmp = cache.with_extension("part");
        let ok = Command::new("curl")
            .args(["-fsSL", "--max-time", "120", "-o"])
            .arg(&tmp)
            .arg(CATALOG_URL)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            let _ = std::fs::rename(&tmp, &cache);
        }
    }
    let bytes = std::fs::read(&cache).map_err(|_| crate::net_check::refine("Couldn't load the PortMaster catalogue. Check Wi-Fi."))?;
    Ok(parse_catalog(&bytes))
}

/// The port's folder among its items: the first that is not a script ("sprout" of ["SPROUT.sh", "sprout"]).
fn port_dir(items: &[String]) -> Option<String> {
    items.iter().find(|i| !i.ends_with(".sh")).map(|i| i.trim_end_matches('/').to_string()).filter(|d| !d.is_empty() && !d.contains('/'))
}

/// What an installed port's own port.json says (PortMaster writes it into the port's folder).
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledPort {
    pub title: String,
    pub dir: String,
    pub rtr: bool,
    pub instructions: String,
    /// When port.json was written: PortMaster writes it last when it installs.
    pub written: Option<SystemTime>,
}

#[derive(Deserialize)]
struct PortJson {
    #[serde(default)]
    items: Vec<String>,
    attr: PortJsonAttr,
}

#[derive(Deserialize)]
struct PortJsonAttr {
    #[serde(default)]
    title: String,
    // A port.json without it is an old, hand-made one: treat it as ready to run, never block it.
    #[serde(default = "yes")]
    rtr: bool,
    #[serde(default)]
    inst: Option<String>,
}

fn yes() -> bool {
    true
}

/// The installed port that `launcher` (/storage/roms/ports/SPROUT.sh) starts: the port.json in a folder of
/// `ports_dir` whose items name that script.
pub fn installed_port(ports_dir: &Path, launcher: &Path) -> Option<InstalledPort> {
    let script = launcher.file_name()?.to_str()?;
    for entry in std::fs::read_dir(ports_dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("PortMaster") || !entry.path().is_dir() {
            continue;
        }
        let json = entry.path().join("port.json");
        let Ok(bytes) = std::fs::read(&json) else { continue };
        let Ok(info) = serde_json::from_slice::<PortJson>(&bytes) else { continue };
        if !info.items.iter().any(|i| i == script) {
            continue;
        }
        return Some(InstalledPort {
            title: if info.attr.title.is_empty() { script.trim_end_matches(".sh").to_string() } else { info.attr.title },
            dir: name,
            rtr: info.attr.rtr,
            instructions: info.attr.inst.unwrap_or_default().trim().to_string(),
            written: std::fs::metadata(&json).and_then(|m| m.modified()).ok(),
        });
    }
    None
}

/// Every installed port that says it is not ready to run (`rtr: false`): the ports that need the player's files, for
/// the web companion's upload page (`web::files`). Sorted by title.
pub fn installed_needing_files(ports_dir: &Path) -> Vec<InstalledPort> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(ports_dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("PortMaster") || !entry.path().is_dir() {
            continue;
        }
        let json = entry.path().join("port.json");
        let Ok(bytes) = std::fs::read(&json) else { continue };
        let Ok(info) = serde_json::from_slice::<PortJson>(&bytes) else { continue };
        if info.attr.rtr {
            continue;
        }
        out.push(InstalledPort {
            title: if info.attr.title.is_empty() { name.clone() } else { info.attr.title },
            dir: name,
            rtr: false,
            instructions: info.attr.inst.unwrap_or_default().trim().to_string(),
            written: std::fs::metadata(&json).and_then(|m| m.modified()).ok(),
        });
    }
    out.sort_by_key(|p| p.title.to_lowercase());
    out
}

pub fn installed(port: &Port) -> bool {
    Path::new(PORTS_DIR).join(&port.launcher).exists()
}

/// Screenshot cached by PortMaster (`config/images_pm`), or the one inside an installed port.
pub fn preview(port: &Port) -> Option<PathBuf> {
    let stem = port.name.trim_end_matches(".zip");
    let cached = Path::new(PM_DIR).join("config/images_pm");
    ["jpg", "png"]
        .iter()
        .map(|ext| cached.join(format!("{stem}.screenshot.{ext}")))
        .chain(launcher_images(&port.launcher))
        .find(|p| p.exists())
}

/// Images a port ships next to its launch script: `<dir>/screenshot.jpg|png`, `<dir>/cover.png`.
pub fn launcher_images(launcher: &str) -> Vec<PathBuf> {
    let dir = Path::new(PORTS_DIR).join(launcher.trim_end_matches(".sh"));
    ["screenshot.jpg", "screenshot.png", "cover.png", "cover.jpg"].iter().map(|f| dir.join(f)).collect()
}

/// PortMaster needs its setup when it isn't unpacked yet, or when its helpers can't run. PortMaster's own
/// GUI makes everything in its folder executable each time it opens (`chmod -R +x .` in PortMaster.sh);
/// pocket-ui never opens that GUI, so without this a port starts but `gptokeyb` doesn't, and every port that
/// reads the keyboard (Celeste Classic, 2048, ...) gets no controls.
fn needs_setup(pm_dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runnable = |name: &str| std::fs::metadata(pm_dir.join(name)).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    !pm_dir.join("harbourmaster").exists() || !runnable("gptokeyb")
}

/// Unpacks PortMaster or repairs it when needed (see `needs_setup`); cheap when nothing is to do.
/// Called before PortMaster installs and before a port starts.
pub fn ensure_ready() -> Result<(), String> {
    if needs_setup(Path::new(PM_DIR)) {
        // This mirrors ROCKNIX's start_portmaster.sh without the GUI.
        let ok = Command::new("sh").arg("/storage/pocket-ui/portmaster-setup.sh").status().is_ok_and(|s| s.success());
        if !ok {
            return Err("Couldn't prepare PortMaster.".into());
        }
    }
    Ok(())
}

fn harbourmaster(args: &[&str]) -> Result<(), String> {
    ensure_ready()?;
    let out = Command::new("python3")
        .arg("harbourmaster")
        .args(["--quiet", "--no-check", "--no-colour"])
        .args(args)
        .current_dir(PM_DIR)
        .env("HOME", "/storage")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stdout);
        Err(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("PortMaster reported an error").trim().to_string())
    }
}

pub fn install(port: &Port) -> Result<(), String> {
    harbourmaster(&["install", &port.name])?;
    if installed(port) { Ok(()) } else { Err("PortMaster finished, but the game is missing.".into()) }
}

pub fn uninstall(port: &Port) -> Result<(), String> {
    harbourmaster(&uninstall_args(port))
}

/// harbourmaster knows installed ports by their full catalog name ("cceleste.zip", from the port's
/// port.json); without ".zip" it answers "Unknown port".
fn uninstall_args(port: &Port) -> [&str; 2] {
    ["uninstall", &port.name]
}

/// "83 KB", "12 MB", "1.4 GB".
pub fn size_label(bytes: u64) -> String {
    match bytes {
        0..=1_048_575 => format!("{} KB", (bytes / 1024).max(1)),
        1_048_576..=1_073_741_823 => format!("{} MB", bytes / 1_048_576),
        _ => format!("{:.1} GB", bytes as f64 / 1_073_741_824.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn uninstall_names_the_port_as_harbourmaster_knows_it() {
        let port = Port { name: "cceleste.zip".into(), ..Port::default() };
        assert_eq!(uninstall_args(&port), ["uninstall", "cceleste.zip"]);
    }

    #[test]
    fn setup_is_needed_until_portmaster_is_unpacked_and_its_helpers_can_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("pmsetup");
        assert!(needs_setup(&dir), "nothing unpacked");
        std::fs::write(dir.join("harbourmaster"), "#!/usr/bin/env python3").unwrap();
        std::fs::write(dir.join("gptokeyb"), "ELF").unwrap();
        std::fs::set_permissions(dir.join("gptokeyb"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(needs_setup(&dir), "gptokeyb as unpacked from PortMaster.zip: not executable");
        std::fs::set_permissions(dir.join("gptokeyb"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!needs_setup(&dir), "unpacked and runnable");
    }

    #[test]
    fn keeps_arm64_ports_and_marks_the_ones_that_need_game_files() {
        let json = br#"{"ports":{
            "2048.zip":{"name":"2048.zip","items":["2048.sh","2048/"],"attr":{"title":"2048","desc":"The 2048\n puzzle","genres":["puzzle"],"rtr":true,"arch":["aarch64"],"exp":false},"source":{"size":83788}},
            "sprout.zip":{"name":"sprout.zip","items":["SPROUT.sh","sprout"],"attr":{"title":"-SPROUT-","desc":"","inst":"Purchase the game from Steam. Download the game and place the game files in sprout/assets.","inst_md":null,"rtr":false,"arch":["aarch64"]},"source":{"size":8159843}},
            "bermuda_syndrome.zip":{"name":"bermuda_syndrome.zip","items":["bermuda/","Bermuda Syndrome.sh"],"attr":{"title":"Bermuda Syndrome","inst":null,"rtr":false,"arch":["aarch64"]},"source":{"size":1}},
            "beta.zip":{"name":"beta.zip","items":["Beta.sh","beta"],"attr":{"title":"Beta","rtr":false,"exp":true,"arch":["aarch64"]},"source":{"size":1}},
            "x86.zip":{"name":"x86.zip","items":["X.sh"],"attr":{"title":"X86","rtr":true,"arch":["x86_64"]},"source":{"size":1}}
        },"utils":{}}"#;
        let ports = parse_catalog(json);
        let names: Vec<&str> = ports.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["sprout.zip", "2048.zip", "bermuda_syndrome.zip"], "by title; experimental and x86 left out");
        assert_eq!(ports[1].launcher, "2048.sh");
        assert_eq!(ports[1].dir, "2048");
        assert_eq!(ports[1].desc, "The 2048 puzzle");
        assert!(!ports[1].needs_files);
        assert!(ports[0].needs_files);
        assert_eq!(ports[0].dir, "sprout");
        assert_eq!(ports[0].instructions, "Purchase the game from Steam. Download the game and place the game files in sprout/assets.");
        assert_eq!((ports[2].dir.as_str(), ports[2].launcher.as_str(), ports[2].instructions.as_str()), ("bermuda", "Bermuda Syndrome.sh", ""));
        assert_eq!(size_label(83_788), "81 KB");
    }

    /// The real catalog, when one has been downloaded (`POCKETUI_PORTS_JSON=tmp/ports.json cargo test`):
    /// it still parses, and about half of the arm64 ports need the player's files.
    #[test]
    fn the_real_catalog_parses_when_given() {
        let Some(path) = std::env::var_os("POCKETUI_PORTS_JSON") else { return };
        let ports = parse_catalog(&std::fs::read(path).unwrap());
        let own = ports.iter().filter(|p| p.needs_files).count();
        assert!(ports.len() > 900 && own > 400, "{} ports, {own} need files", ports.len());
    }

    #[test]
    fn finds_an_installed_port_by_its_launcher() {
        let dir = TestDir::new("installed");
        for d in ["sprout", "PortMaster", "2048", "empty"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        // As harbourmaster writes it: the catalog entry plus "files" and "status".
        std::fs::write(
            dir.join("sprout/port.json"),
            r#"{"version":4,"name":"sprout.zip","items":["SPROUT.sh","sprout"],"items_opt":null,"attr":{"title":"-SPROUT-","inst":"Place the game files in sprout/assets.","rtr":false,"exp":false,"arch":["aarch64"]},"status":{"status":"Installed"},"files":{"port.json":"sprout/port.json","SPROUT.sh":"SPROUT.sh","sprout":"sprout"}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("2048/port.json"), r#"{"items":["2048.sh","2048/"],"attr":{"title":"2048","rtr":true}}"#).unwrap();
        std::fs::write(dir.join("PortMaster/port.json"), r#"{"items":["SPROUT.sh"],"attr":{"title":"not a port"}}"#).unwrap();
        let p = installed_port(&dir, &dir.join("SPROUT.sh")).unwrap();
        assert_eq!((p.title.as_str(), p.dir.as_str(), p.rtr), ("-SPROUT-", "sprout", false));
        assert_eq!(p.instructions, "Place the game files in sprout/assets.");
        assert!(p.written.is_some());
        assert!(installed_port(&dir, &dir.join("2048.sh")).unwrap().rtr);
        assert_eq!(installed_port(&dir, &dir.join("Other.sh")), None);
        // A hand-made port.json without "rtr" is never treated as needing files.
        std::fs::write(dir.join("empty/port.json"), r#"{"items":["Empty.sh","empty"],"attr":{"title":"Empty"}}"#).unwrap();
        assert!(installed_port(&dir, &dir.join("Empty.sh")).unwrap().rtr);
    }
}
