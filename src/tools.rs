//! Settings → Tools: ROCKNIX's tools and pocket-ui's Store tools, without EmulationStation.
//!
//! ROCKNIX's tools are the `.sh` files EmulationStation lists as its system "tools": `/storage/.config/modules`, which
//! ROCKNIX mirrors from `/usr/config/modules` at every boot (`001-sync-modules`), with names and descriptions in the
//! `gamelist.xml` next to them. EmulationStation starts one as `/usr/bin/foot %ROM%` (es_systems.cfg, ROCKNIX
//! 20261001): a terminal around the script, so scripts that print or ask see a screen, and scripts that open a
//! window (the emulators' own menus, the gamepad test) get it fullscreen. The scripts set up gptokeyb themselves
//! where a program needs keyboard keys (Azahar, FEXConfig), and name their process for ROCKNIX's kill combination
//! (`set_kill`, L1 + SELECT + START in `input_sense`). pocket-ui starts them the same way (`command`).
//!
//! Store tools (`store::tools_dir`) are signed scripts that finish by themselves; they run without a window and their
//! last line of output is shown (`run_quiet`), as the Store's Run does.
//!
//! What is risky: anything that can update the system or its bootloader, wipe a drive or reset the Pocket is never
//! listed (`hidden`); tools that delete, upload, download a lot or lay something over the games folder ask for a
//! second A with a warning (`Risk::Warn`); a script nobody knows does too. Nothing here runs on the UI thread.

use std::path::{Path, PathBuf};
use std::process::Command;

/// ROCKNIX's Tools folder (EmulationStation's), then the image's original it is mirrored from: a tool is looked for in
/// both, so the list is complete also before the first mirror.
pub const ROCKNIX_DIRS: [&str; 2] = [crate::store::ROCKNIX_MODULES, "/usr/config/modules"];
/// The terminal EmulationStation runs tools in (es_systems.cfg: `<command>/usr/bin/foot %ROM%</command>`).
pub const TERMINAL: &str = "/usr/bin/foot";
/// `Running::emulator` of a tool started from Settings → Tools; `system` is "tools".
pub const EMULATOR: &str = "tool";
/// How long HOME has to be held to stop a running tool (`App::tool_home`).
pub const HOLD_TO_STOP_MS: u128 = 1000;
/// A tool that ends with an error this soon after its start never got going.
pub const NEVER_STARTED_SECS: u64 = 3;

/// Where a tool comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// An emulator's own menus (its settings, controllers, graphics), or RetroArch's.
    Emulator,
    /// Gamepad test and calibration, the touch screen test.
    Controls,
    /// File manager, terminal, scans, installers and the other system tools of ROCKNIX.
    System,
    /// Installed from pocket-ui's Store.
    Store,
    /// A script in ROCKNIX's folder that pocket-ui does not know.
    Unknown,
}

impl Group {
    /// The quiet word at the right end of a row.
    pub fn label(self) -> &'static str {
        match self {
            Group::Emulator => "Emulator",
            Group::Controls => "Controls",
            Group::System => "System",
            Group::Store => "Store",
            Group::Unknown => "Unknown",
        }
    }
}

/// Whether a tool needs a second A first.
#[derive(Clone, Debug, PartialEq)]
pub enum Risk {
    Safe,
    /// Changes, deletes, uploads or downloads something: the warning shown before the second A.
    Warn(String),
}

/// How a tool runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Run {
    /// Like EmulationStation: in the terminal, as an app (own process group, its windows fullscreen, back to
    /// pocket-ui when it ends).
    App,
    /// A Store tool: `sh <script>` without a window, its last line of output shown on the page.
    Quiet,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    /// One or two short sentences: what it is for.
    pub about: String,
    pub script: PathBuf,
    pub group: Group,
    pub risk: Risk,
    pub run: Run,
    /// How to leave it once it runs, when ROCKNIX's script says so (its kill combination), else the generic way.
    pub leave: &'static str,
}

/// How to leave a tool that names its process for ROCKNIX's kill combination (`set_kill`).
pub const LEAVE_KILL: &str = "To leave: its own Quit, L1 + SELECT + START, or hold HOME for a second.";
/// How to leave any other app-like tool: pocket-ui stops it (`App::tool_home`).
pub const LEAVE_HOME: &str = "Ends by itself when it is done; hold HOME for a second to stop it.";
/// Store tools finish by themselves.
pub const LEAVE_QUIET: &str = "Runs without a window and ends by itself; what it says shows here.";

/// One known tool of ROCKNIX: file name, name in the list, group, what it is for, and the warning if it is risky.
struct Known {
    file: &'static str,
    name: &'static str,
    group: Group,
    about: &'static str,
    warn: Option<&'static str>,
}

const fn k(file: &'static str, name: &'static str, group: Group, about: &'static str) -> Known {
    Known { file, name, group, about, warn: None }
}

const fn w(file: &'static str, name: &'static str, group: Group, about: &'static str, warn: &'static str) -> Known {
    Known { file, name, group, about, warn: Some(warn) }
}

/// ROCKNIX 20261001's tools on the SM8250 image (`/usr/config/modules`), in the order the list shows them within a
/// group. Descriptions say what each is for in pocket-ui's words; the files and what they do were read in the image.
const KNOWN: &[Known] = &[
    k("Start 64bit Retroarch.sh", "RetroArch", Group::Emulator, "RetroArch's own menu: cores, shaders, input and its settings, for the games RetroArch runs."),
    k("Start 32bit Retroarch.sh", "RetroArch (32-bit)", Group::Emulator, "The 32-bit RetroArch's menu, for the few cores that only exist as 32-bit."),
    k("Start Dolphin.sh", "Dolphin", Group::Emulator, "GameCube and Wii: Dolphin's own settings, graphics, controllers and hotkeys."),
    k("Start PPSSPP.sh", "PPSSPP", Group::Emulator, "PSP: PPSSPP's own menu, settings and controls."),
    k("Start Azahar.sh", "Azahar", Group::Emulator, "Nintendo 3DS: Azahar's settings and controls. ROCKNIX turns the buttons into keys and the stick into a mouse."),
    k("Start ARMSX2.sh", "ARMSX2", Group::Emulator, "PlayStation 2: ARMSX2's own settings, controllers and memory cards."),
    k("Start AetherSX2.sh", "AetherSX2", Group::Emulator, "PlayStation 2, the older emulator, in its big-picture menu."),
    k("Start Duckstation.sh", "DuckStation", Group::Emulator, "PlayStation: DuckStation's big-picture menu and settings."),
    k("Start MelonDS.sh", "melonDS", Group::Emulator, "Nintendo DS: melonDS's settings and controls."),
    k("Start Flycast.sh", "Flycast", Group::Emulator, "Dreamcast, NAOMI and Atomiswave: Flycast's settings."),
    k("Start ScummVM.sh", "ScummVM", Group::Emulator, "ScummVM's own launcher: add, set up and start adventure games."),
    k("Start SkyEmu.sh", "SkyEmu", Group::Emulator, "Game Boy, Game Boy Advance and DS: SkyEmu's settings."),
    k("Start DaedalusX64.sh", "DaedalusX64", Group::Emulator, "Nintendo 64: DaedalusX64's own menu."),
    k("Start HATARISA.sh", "Hatari", Group::Emulator, "Atari ST: Hatari's settings."),
    k("Start BigPEmu.sh", "BigPEmu", Group::Emulator, "Atari Jaguar: BigPEmu's settings."),
    k("Start CEMU.sh", "Cemu", Group::Emulator, "Wii U: Cemu's settings."),
    k("Start RPCS3.sh", "RPCS3", Group::Emulator, "PlayStation 3: RPCS3's settings."),
    k("Start Vita3K.sh", "Vita3K", Group::Emulator, "PlayStation Vita: Vita3K. Installs firmware files found in bios/vita3k first."),
    k("Start Xemu.sh", "xemu", Group::Emulator, "Original Xbox: xemu's settings."),
    k("Start touchHLE.sh", "touchHLE", Group::Emulator, "Early iPhone OS apps and games."),
    k("Start FEXConfig.sh", "FEX settings", Group::Emulator, "Settings of FEX, which runs x86 programs such as Steam on this Pocket."),
    k("Start Moonlight.sh", "Moonlight app", Group::Emulator, "Moonlight's own app for PC streaming. The PC streaming page in Settings pairs and streams without it."),
    k("Test Gamepad.sh", "Test gamepad", Group::Controls, "Shows every button, stick and trigger as you press it."),
    w(
        "GPcal.sh",
        "Calibrate gamepad",
        Group::Controls,
        "Measures the sticks and triggers again: the fix for a drifting stick. Follow the steps on screen.",
        "Writes a new calibration for the sticks and triggers. pocket-ui's controls pause while it runs.",
    ),
    k("Test Touchscreen.sh", "Test touch screen", Group::Controls, "Shows where the screen feels your fingers."),
    k("commander.sh", "File manager", Group::System, "Browse, copy and move files, including on a USB stick."),
    k("qterminal.sh", "Terminal", Group::System, "A Linux terminal, as root. Needs a USB keyboard; an on-screen keyboard comes up too."),
    k("Start PortMaster.sh", "PortMaster", Group::System, "PortMaster's own app: install and update ports. Extras in Settings does the same in pocket-ui."),
    k("Scan ScummVM Games.sh", "Scan ScummVM games", Group::System, "Adds the games in roms/scummvm to the shelf. pocket-ui starts again afterwards, as ROCKNIX's script asks."),
    k("Scan id Tech Games.sh", "Scan id Tech games", Group::System, "Writes start files for Doom, Quake and other id Tech games found on the SD card."),
    k("Scan Heroic Games.sh", "Scan Heroic games", Group::System, "Adds the games installed in Heroic to the shelf."),
    k("Start M8C.sh", "M8C", Group::System, "Screen and sound of a Dirtywave M8 tracker connected by USB."),
    w(
        "Mount NFS.sh",
        "Mount NFS",
        Group::System,
        "Puts a NAS folder (NFS) over your games folder. Set it up in /storage/.nfs-mount first.",
        "Lays a network folder over /storage/roms: the shelf then shows the games there. Undo it with the same tool.",
    ),
    w(
        "Remove ._ Files.sh",
        "Remove ._ files",
        Group::System,
        "Deletes the hidden ._ files a Mac leaves next to every file it copies.",
        "Deletes every file starting with ._ in your games folder.",
    ),
    w("Install Steam.sh", "Install Steam", Group::System, "Installs Steam for ARM and x86 games through FEX.", "Downloads several gigabytes from the internet and installs them on the SD card."),
    w("Uninstall Steam.sh", "Uninstall Steam", Group::System, "Removes the Steam client.", "Removes the Steam client. Your Steam games and saves stay."),
    w(
        "Install Heroic Games Launcher.sh",
        "Install Heroic",
        Group::System,
        "Installs the Heroic launcher for Epic, GOG and Amazon games.",
        "Downloads Heroic from the internet and installs it on the SD card.",
    ),
    w(
        "Uninstall Heroic Games Launcher.sh",
        "Uninstall Heroic",
        Group::System,
        "Removes the Heroic launcher.",
        "Deletes Heroic's folder and everything in it.",
    ),
    w(
        "cloud_backup.sh",
        "Cloud backup",
        Group::System,
        "Copies your saves to a cloud drive set up for rclone (see rocknix.org).",
        "Uploads your saves to the cloud drive set up for rclone.",
    ),
    w(
        "cloud_restore.sh",
        "Cloud restore",
        Group::System,
        "Copies your saves back from the cloud drive set up for rclone.",
        "Replaces the saves on this Pocket with the ones on your cloud drive.",
    ),
];

/// File names never listed: ROCKNIX's installer wipes the drive it installs to.
const NEVER: &[&str] = &["Install ROCKNIX.sh"];

/// Words in a script that update the system or its bootloader, wipe or partition a drive, or reset the Pocket: such a
/// script is never listed (ROCKNIX's `rocknix-update` flashes the ABL on this device at the next boot, `factoryreset`
/// deletes /storage with pocket-ui in it).
const NEVER_IN_SCRIPT: &[&str] = &["rocknix-update", "updateabl", "factoryreset", "mkfs", "of=/dev/", "wipefs", "sgdisk", "sfdisk", "fdisk ", "parted ", "/storage/.update"];

/// Words in a file name that say the same.
const NEVER_IN_NAME: &[&str] = &["factory", "bootloader", "flash", "install rocknix", "update rocknix", "rocknix update", "system update", "reset"];

/// The warning for a script pocket-ui does not know.
pub const UNKNOWN_WARNING: &str = "Not one of ROCKNIX's known tools. It runs as root with full access: start it only if you know what it does.";

/// True when a tool must not be listed: its name or its script says it updates, flashes, wipes or resets.
pub fn hidden(file: &str, script: &str) -> bool {
    let name = file.to_lowercase();
    NEVER.contains(&file) || NEVER_IN_NAME.iter().any(|w| name.contains(w)) || NEVER_IN_SCRIPT.iter().any(|w| script.contains(w))
}

/// Names and descriptions from ROCKNIX's `gamelist.xml`: `(file name, name, description)` per `<game>`.
pub fn parse_gamelist(xml: &str) -> Vec<(String, String, String)> {
    let tag = |block: &str, t: &str| -> String {
        let (open, close) = (format!("<{t}>"), format!("</{t}>"));
        block.find(&open).and_then(|s| block[s + open.len()..].find(&close).map(|e| unescape(block[s + open.len()..s + open.len() + e].trim()))).unwrap_or_default()
    };
    xml.split("<game>")
        .skip(1)
        .filter_map(|block| {
            let block = block.split("</game>").next()?;
            let path = tag(block, "path");
            let file = path.strip_prefix("./").unwrap_or(&path).to_string();
            (!file.is_empty() && !file.contains('/')).then(|| (file, tag(block, "name"), tag(block, "desc")))
        })
        .collect()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'").replace("&amp;", "&")
}

/// A script of ROCKNIX's folder as a `Tool`, or None when it is never listed. `script` is the text of the file (read up
/// to a size limit), `gamelist` the folder's names.
pub fn rocknix_tool(path: &Path, script: &str, gamelist: &[(String, String, String)]) -> Option<Tool> {
    let file = path.file_name()?.to_str()?;
    if hidden(file, script) {
        return None;
    }
    let leave = if script.contains("set_kill set") { LEAVE_KILL } else { LEAVE_HOME };
    let tool = match KNOWN.iter().find(|k| k.file == file) {
        Some(k) => Tool {
            name: k.name.to_string(),
            about: k.about.to_string(),
            script: path.to_path_buf(),
            group: k.group,
            risk: k.warn.map_or(Risk::Safe, |w| Risk::Warn(w.to_string())),
            run: Run::App,
            leave,
        },
        None => {
            let listed = gamelist.iter().find(|(f, _, _)| f == file);
            let stem = file.strip_suffix(".sh").unwrap_or(file).to_string();
            let name = listed.map(|(_, n, _)| n.clone()).filter(|n| !n.is_empty()).unwrap_or(stem);
            let about = listed.map(|(_, _, d)| d.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| "A script in ROCKNIX's Tools folder.".into());
            Tool { name, about, script: path.to_path_buf(), group: Group::Unknown, risk: Risk::Warn(UNKNOWN_WARNING.into()), run: Run::App, leave }
        }
    };
    Some(tool)
}

/// The Store's installed tools: those with a script to run (`Installed::run`, or the only `.sh` file of a package
/// installed before the inventory kept it), in `tools_dir`.
pub fn store_tools(data_dir: &Path) -> Vec<Tool> {
    let store = crate::store::Store::open(data_dir);
    let dir = crate::store::tools_dir(data_dir);
    let mut out = Vec::new();
    for (id, i) in &store.installed {
        let in_dir: Vec<&PathBuf> = i.files.iter().filter(|f| f.starts_with(&dir)).collect();
        if in_dir.is_empty() {
            continue;
        }
        let script = match i.run.as_deref() {
            Some(run) if !run.contains("..") && !run.starts_with('/') => dir.join(run),
            Some(_) => continue,
            None => match in_dir.iter().filter(|f| f.extension().is_some_and(|e| e == "sh")).collect::<Vec<_>>()[..] {
                [one] => (*one).clone(),
                _ => continue,
            },
        };
        if !script.is_file() {
            continue;
        }
        out.push(Tool {
            // Lists from before 2026-10-23 have no name: the script's, as the Store named it on the card.
            name: i.name.clone().or_else(|| script.file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(|| id.clone()),
            about: format!("From the Store, version {}.", i.version),
            script,
            group: Group::Store,
            risk: Risk::Safe,
            run: Run::Quiet,
            leave: LEAVE_QUIET,
        });
    }
    out
}

/// The biggest script read to decide whether it is listed; a longer file is read only this far.
const READ_AT_MOST: u64 = 256 * 1024;

fn read_head(path: &Path) -> String {
    use std::io::Read;
    let mut text = Vec::new();
    let _ = std::fs::File::open(path).and_then(|f| f.take(READ_AT_MOST).read_to_end(&mut text));
    String::from_utf8_lossy(&text).into_owned()
}

/// Every tool: ROCKNIX's from `dirs` (a file in an earlier folder wins over the same name in a later one), then the
/// Store's from `data_dir`; sorted by group, ROCKNIX's known ones in their table order, the rest by name.
pub fn scan(dirs: &[&Path], data_dir: &Path) -> Vec<Tool> {
    let mut seen: Vec<String> = Vec::new();
    let mut tools = Vec::new();
    for dir in dirs {
        let gamelist = parse_gamelist(&std::fs::read_to_string(dir.join("gamelist.xml")).unwrap_or_default());
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "sh")).collect();
        files.sort();
        for path in files {
            let Some(file) = path.file_name().and_then(|f| f.to_str()).map(str::to_string) else { continue };
            // A link is fine (cloud_backup.sh points into /usr/bin), as long as it leads to a file.
            if seen.contains(&file) || !path.is_file() {
                continue;
            }
            seen.push(file);
            tools.extend(rocknix_tool(&path, &read_head(&path), &gamelist));
        }
    }
    tools.extend(store_tools(data_dir));
    sort(&mut tools);
    tools
}

/// Group order; within ROCKNIX's groups the table's order, then by name.
pub fn sort(tools: &mut [Tool]) {
    let rank = |t: &Tool| {
        let file = t.script.file_name().and_then(|f| f.to_str()).unwrap_or("");
        (t.group, KNOWN.iter().position(|k| k.file == file).unwrap_or(usize::MAX), t.name.to_lowercase())
    };
    tools.sort_by_key(rank);
}

/// The locale tools get: foot refuses to run anything in the C locale ("'C' is not a UTF-8 locale, and failed to find a
/// fallback"; seen in the virtual device), and pocket-ui, unlike EmulationStation's `es_settings`, exports none.
pub const LOCALE: &str = "en_US.UTF-8";

/// What `sh -c` runs before the tool: compile `LOCALE` into `$1` once (as `es_settings` does with `localedef`, under a
/// second), point `LOCPATH` and `LANG` at it, then become the rest of the arguments.
const WITH_LOCALE: &str = r#"L="$1"; shift
if [ ! -e "$L/en_US.UTF-8/LC_CTYPE" ] && command -v localedef >/dev/null 2>&1; then
  mkdir -p "$L" && localedef -i en_US -c -f UTF-8 "$L/en_US.UTF-8" >/dev/null 2>&1
fi
if [ -e "$L/en_US.UTF-8/LC_CTYPE" ]; then export LOCPATH="$L" LANG=en_US.UTF-8 LANGUAGE=en_US.UTF-8; fi
exec "$@"
"#;

/// The command that starts an app-like tool as EmulationStation does: `foot /bin/bash <script>` (bash: ROCKNIX's
/// scripts are bash, and a script that lost its x bit still runs), in `/storage`, the home folder, with a UTF-8 locale
/// kept in `locales` (`<data>/locale`). Without a terminal (`terminal` missing) the script runs on its own. Paths are
/// arguments, never part of the shell line.
pub fn command(tool: &Tool, terminal: &Path, locales: &Path) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(WITH_LOCALE).arg("sh").arg(locales);
    if terminal.is_file() {
        c.arg(terminal);
    }
    c.arg("/bin/bash").arg(&tool.script);
    c.current_dir("/storage");
    c
}

/// Runs a Store tool's script with `sh` in `/tmp` and returns its last line of output (of its errors if it fails).
pub fn run_quiet(script: &Path) -> Result<String, String> {
    let out = Command::new("sh").arg(script).current_dir("/tmp").output().map_err(|e| format!("Couldn't start the tool: {e}"))?;
    let last = |b: &[u8]| String::from_utf8_lossy(b).lines().map(str::trim).rfind(|l| !l.is_empty()).map(str::to_string);
    if out.status.success() {
        Ok(last(&out.stdout).unwrap_or_else(|| "Done.".into()))
    } else {
        Err(last(&out.stderr).or_else(|| last(&out.stdout)).unwrap_or_else(|| "The tool stopped with an error.".into()))
    }
}

/// ROCKNIX's input service on the SM8250: GPcal (Calibrate gamepad) stops it and starts it again only when its script
/// ends normally, so a tool stopped with HOME (SIGTERM to the whole group) would leave the Pocket without its controls.
pub const INPUT_SERVICE: &str = "/usr/lib/systemd/system/inputplumber.service";

/// After a tool ended, however: the input service runs again (`systemctl start` does nothing when it runs). On a thread.
pub fn after_tool() {
    if !Path::new(INPUT_SERVICE).is_file() {
        return;
    }
    std::thread::spawn(|| {
        let ok = crate::proc::status_with_timeout(Command::new("systemctl").args(["start", "inputplumber"]), std::time::Duration::from_secs(10)).is_ok_and(|s| s.success());
        if !ok {
            eprintln!("tools: inputplumber did not start again");
        }
    });
}

/// What to tell the user when a tool ended at once with an error.
pub fn never_started(name: &str, secs: u64, ok: bool, stop_asked: bool) -> Option<String> {
    (!ok && !stop_asked && secs < NEVER_STARTED_SECS).then(|| format!("{name} ended at once with an error. The log has the details (Settings › Help & tools › About shows where it is)."))
}

/// A few tools with made-up paths, for `--snapshot tools` and the tests.
pub fn demo() -> Vec<Tool> {
    let gamelist = [("Hello.sh".to_string(), "Hello script".to_string(), "Says hello.".to_string())];
    let mut tools: Vec<Tool> = ["Start 64bit Retroarch.sh", "Start Dolphin.sh", "Start PPSSPP.sh", "Start Azahar.sh", "Start ARMSX2.sh", "Test Gamepad.sh", "GPcal.sh", "commander.sh", "qterminal.sh", "Remove ._ Files.sh", "cloud_backup.sh", "Hello.sh"]
        .iter()
        .filter_map(|f| rocknix_tool(&Path::new(crate::store::ROCKNIX_MODULES).join(f), "set_kill set \"x\"", &gamelist))
        .collect();
    tools.push(Tool {
        name: "Storage report".into(),
        about: "From the Store, version 1.0.".into(),
        script: PathBuf::from("/storage/.config/pocket-ui/tools/Storage report.sh"),
        group: Group::Store,
        risk: Risk::Safe,
        run: Run::Quiet,
        leave: LEAVE_QUIET,
    });
    sort(&mut tools);
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("tools-{name}"))
    }

    /// ROCKNIX's gamelist.xml (20261001), shortened: names, descriptions, escapes.
    const GAMELIST: &str = r#"<?xml version="1.0"?>
<gameList>
    <game>
        <path>./commander.sh</path>
        <name>File Manager</name>
        <desc>Enables you to browse the ROCKNIX filesystem &amp; manage files.</desc>
    </game>
    <game>
        <path>./My tool.sh</path>
        <name>My &quot;own&quot; tool</name>
        <desc>Does things.</desc>
    </game>
    <game><path>../escape.sh</path><name>x</name></game>
</gameList>"#;

    #[test]
    fn reads_names_and_descriptions_from_rocknixs_gamelist() {
        let g = parse_gamelist(GAMELIST);
        assert_eq!(g.len(), 2, "a path outside the folder is not a tool: {g:?}");
        assert_eq!(g[0], ("commander.sh".into(), "File Manager".into(), "Enables you to browse the ROCKNIX filesystem & manage files.".into()));
        assert_eq!(g[1].1, "My \"own\" tool");
    }

    #[test]
    fn updates_installers_and_resets_are_never_listed() {
        assert!(hidden("Install ROCKNIX.sh", ""), "wipes the drive it installs to");
        assert!(hidden("Factory Reset.sh", ""));
        assert!(hidden("Update ROCKNIX.sh", ""));
        assert!(hidden("x.sh", "#!/bin/bash\nrocknix-update\n"), "rocknix-update flashes the ABL on this device");
        assert!(hidden("x.sh", "updateabl /dev/sda\n"));
        assert!(hidden("x.sh", "factoryreset ALL\n"));
        assert!(hidden("x.sh", "dd if=img of=/dev/mmcblk0\n"));
        assert!(hidden("x.sh", "cp x.tar /storage/.update/\n"), "an update dropped for the next boot");
        // The real tools of the image are not caught by accident (RPCS3's script says "dev_flash").
        assert!(!hidden("Start RPCS3.sh", "FOLDER_LINKS=(\"dev_flash\" \"dev_hdd0\")\n"));
        assert!(!hidden("Uninstall Steam.sh", "rm -rf \"${entry}\"\n"));
        assert!(!hidden("Start Dolphin.sh", "/usr/bin/dolphin-emu\n"));
    }

    #[test]
    fn known_tools_get_our_words_unknown_ones_a_warning() {
        let g = parse_gamelist(GAMELIST);
        let at = |f: &str| Path::new("/m").join(f);
        let dolphin = rocknix_tool(&at("Start Dolphin.sh"), "set_kill set \"dolphin-emu\"", &g).unwrap();
        assert_eq!((dolphin.name.as_str(), dolphin.group, &dolphin.risk, dolphin.run), ("Dolphin", Group::Emulator, &Risk::Safe, Run::App));
        assert_eq!(dolphin.leave, LEAVE_KILL);
        let backup = rocknix_tool(&at("cloud_backup.sh"), "", &g).unwrap();
        assert!(matches!(&backup.risk, Risk::Warn(w) if w.contains("Uploads")), "cloud backup uploads: {backup:?}");
        assert_eq!(backup.leave, LEAVE_HOME, "no kill combination named");
        let own = rocknix_tool(&at("My tool.sh"), "echo hi", &g).unwrap();
        assert_eq!((own.name.as_str(), own.about.as_str(), own.group), ("My \"own\" tool", "Does things.", Group::Unknown));
        assert_eq!(own.risk, Risk::Warn(UNKNOWN_WARNING.into()));
        let bare = rocknix_tool(&at("Other.sh"), "", &[]).unwrap();
        assert_eq!(bare.name, "Other");
        assert!(rocknix_tool(&at("Install ROCKNIX.sh"), "", &g).is_none());
    }

    #[test]
    fn every_known_tool_has_words_and_a_warning_says_why() {
        for k in KNOWN {
            assert!(k.file.ends_with(".sh") && !k.name.is_empty() && k.about.ends_with('.'), "{}", k.file);
            assert!(k.warn.is_none_or(|w| w.ends_with('.')), "{}", k.file);
            assert!(!hidden(k.file, ""), "{} is listed", k.file);
        }
    }

    #[test]
    fn scans_both_folders_once_per_name_and_sorts_by_group() {
        let d = dir("scan");
        let (storage, usr, data) = (d.join("storage"), d.join("usr"), d.join("data"));
        for f in [&storage, &usr] {
            std::fs::create_dir_all(f).unwrap();
        }
        std::fs::write(storage.join("gamelist.xml"), GAMELIST).unwrap();
        std::fs::write(storage.join("commander.sh"), "set_kill set \"commander\"").unwrap();
        std::fs::write(storage.join("Test Gamepad.sh"), "").unwrap();
        std::fs::write(storage.join("Install ROCKNIX.sh"), "").unwrap();
        std::fs::write(storage.join("notes.txt"), "").unwrap();
        std::fs::write(usr.join("commander.sh"), "older copy").unwrap();
        std::fs::write(usr.join("Start Dolphin.sh"), "").unwrap();
        std::fs::write(usr.join("Zed.sh"), "").unwrap();
        std::os::unix::fs::symlink(d.join("missing"), storage.join("dangling.sh")).unwrap();
        let tools = scan(&[&storage, &usr], &data);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Dolphin", "Test gamepad", "File manager", "Zed"]);
        assert_eq!(tools[2].script, storage.join("commander.sh"), "ROCKNIX's mirror wins over the image's copy");
    }

    #[test]
    fn store_tools_are_listed_from_the_inventory_and_run_quietly() {
        use crate::store::{Installed, Store};
        let d = dir("store");
        let tools_dir = crate::store::tools_dir(&d);
        std::fs::create_dir_all(&tools_dir).unwrap();
        std::fs::write(tools_dir.join("report.sh"), "echo first\necho 'Free: 12 GB'\n").unwrap();
        std::fs::write(tools_dir.join("old.sh"), "exit 3").unwrap();
        let mut store = Store::open(&d);
        store.installed.insert(
            "tool-report".into(),
            Installed { version: "1.1".into(), files: vec![tools_dir.join("report.sh")], name: Some("Storage report".into()), run: Some("report.sh".into()), ..Default::default() },
        );
        store.installed.insert("tool-old".into(), Installed { version: "1.0".into(), files: vec![tools_dir.join("old.sh")], ..Default::default() });
        store.installed.insert("frames".into(), Installed { version: "1.0".into(), files: vec![d.join("frames/a.png")], ..Default::default() });
        // Written the way the Store writes it.
        std::fs::create_dir_all(d.join("store")).unwrap();
        std::fs::write(d.join("store/installed.json"), serde_json::to_vec(&store.installed).unwrap()).unwrap();

        let tools = store_tools(&d);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["old", "Storage report"], "an old entry without a name shows its script's");
        assert!(tools.iter().all(|t| t.run == Run::Quiet && t.group == Group::Store));
        assert_eq!(run_quiet(&tools[1].script), Ok("Free: 12 GB".into()));
        assert!(run_quiet(&tools[0].script).is_err());
    }

    #[test]
    fn tools_start_in_the_terminal_like_emulationstation_does() {
        let d = dir("cmd");
        let foot = d.join("foot");
        std::fs::write(&foot, "").unwrap();
        let tool = rocknix_tool(Path::new("/m/Test Gamepad.sh"), "", &[]).unwrap();
        let c = command(&tool, &foot, Path::new("/data/locale"));
        assert_eq!(c.get_program(), "/bin/sh");
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("localedef -i en_US -c -f UTF-8") && args[1].contains("exec \"$@\""), "{}", args[1]);
        assert_eq!(args[2..], ["sh", "/data/locale", &foot.to_string_lossy(), "/bin/bash", "/m/Test Gamepad.sh"], "paths are arguments, never spliced into the shell line");
        assert_eq!(c.get_current_dir(), Some(Path::new("/storage")));
        let bare: Vec<_> = command(&tool, &d.join("no-terminal"), Path::new("/l")).get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(bare[2..], ["sh", "/l", "/bin/bash", "/m/Test Gamepad.sh"]);
    }

    #[test]
    fn a_tool_that_ended_at_once_is_explained() {
        assert!(never_started("Dolphin", 1, false, false).is_some_and(|m| m.starts_with("Dolphin ended at once")));
        assert_eq!(never_started("Dolphin", 1, true, false), None);
        assert_eq!(never_started("Dolphin", 1, false, true), None, "stopped with HOME");
        assert_eq!(never_started("Dolphin", NEVER_STARTED_SECS, false, false), None);
    }

    #[test]
    fn the_demo_has_every_group_and_a_warning() {
        let d = demo();
        for g in [Group::Emulator, Group::Controls, Group::System, Group::Store, Group::Unknown] {
            assert!(d.iter().any(|t| t.group == g), "{g:?}");
        }
        assert!(d.iter().any(|t| matches!(t.risk, Risk::Warn(_))));
    }
}
