//! "PC streaming" on Home: ROCKNIX's Moonlight (moonlight-embedded) as an app. No pause menu, no save slots;
//! pocket-ui starts it like a game (own process group, sway makes its window fullscreen), and the shelf comes
//! back when it ends. Settings → PC streaming pairs a PC on the device (`pair_command`, the PIN read from Moonlight's
//! own output), lists and unpairs paired PCs, and gets a PC's games (`list_apps`, `write_games`), as
//! EmulationStation's Moonlight menu did (docs/user/pc-streaming.md).
//!
//! How ROCKNIX does it, read on the device (20261001): `/usr/bin/moonlight` is Moonlight Embedded 2.7.0; its
//! config is `/storage/.config/moonlight/moonlight.conf` (copied from `/usr/config/moonlight` at boot by
//! autostart `010-moonlight`); keys live in `/storage/.cache/moonlight`; EmulationStation pairs with
//! `moonlight pair` and writes one `.sh` per PC game into `/storage/roms/moonlight`, each ending in
//! `moonlight stream -app "<name>" -platform sdl`. We start `moonlight stream -platform sdl` without an app, so
//! the `app =` line of moonlight.conf (or Moonlight's own default) decides what is streamed.

use std::path::Path;
use std::process::Command;

/// `Running::emulator` of the app, and the system id of its recents-free entry.
pub const EMULATOR: &str = "moonlight";
pub const BIN: &str = "/usr/bin/moonlight";
pub const CONF: &str = "/storage/.config/moonlight/moonlight.conf";
const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";

/// How long the PC lookup may take before the stream is not even tried.
pub const LOOKUP_SECS: u32 = 12;
/// A stream that ends non-zero this soon after it started never got going.
pub const NEVER_STARTED: u64 = 30;

/// Moonlight is on this system: the program is there and may be run.
pub fn installed_at(bin: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn installed() -> bool {
    installed_at(Path::new(&std::env::var("POCKETUI_MOONLIGHT").unwrap_or_else(|_| BIN.to_string())))
}

/// The PC's address ROCKNIX's menu stored (`moonlight.host` in system.cfg), if it is one we can pass on.
pub fn host_setting(system_cfg: &str) -> Option<String> {
    let host = system_cfg.lines().find_map(|l| l.strip_prefix("moonlight.host="))?.trim();
    valid_host(host).then(|| host.to_string())
}

/// A name or an address Moonlight may be given: never something that reads like an option or like shell.
pub fn valid_host(host: &str) -> bool {
    let plain = host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_'));
    !host.is_empty() && host.len() <= 253 && plain && host.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// The PC to stream from, if ROCKNIX's menu was ever used to set one; otherwise Moonlight finds it itself
/// (`address =` in moonlight.conf, or mDNS).
pub fn host() -> Option<String> {
    host_setting(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default())
}

/// What `sh -c` runs: look the PC up first (so a PC that is off or not paired costs a few seconds and a
/// message, not a hung screen), then become the stream: of the app named in `$2` (one of the PC's games, Settings →
/// PC streaming), or of Moonlight's default when it is empty.
pub fn script(bin: &str) -> String {
    format!(
        r#". /etc/profile >/dev/null 2>&1
HOST="$1"
if ! timeout {LOOKUP_SECS} {bin} list $HOST >/dev/null 2>&1; then
  echo "pocket-ui: no PC answered the lookup" >&2
  exit 3
fi
if [ -n "$2" ]; then
  exec {bin} stream -platform sdl -app "$2" $HOST
fi
exec {bin} stream -platform sdl $HOST
"#
    )
}

/// The command that starts Moonlight; `host` and `app` are passed as arguments of the script, never spliced into it.
pub fn command(bin: &str, host: Option<&str>) -> Command {
    command_for(bin, host, None)
}

/// `command`, streaming the PC's game `app` instead of Moonlight's default.
pub fn command_for(bin: &str, host: Option<&str>, app: Option<&str>) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(script(bin)).arg("sh").arg(host.unwrap_or("")).arg(app.unwrap_or(""));
    c
}

/// The program to run: `POCKETUI_MOONLIGHT` names another one, for the virtual device and tests.
pub fn bin() -> String {
    std::env::var("POCKETUI_MOONLIGHT").unwrap_or_else(|_| BIN.to_string())
}

/// The command for this device.
pub fn start() -> Command {
    command(&bin(), host().as_deref())
}

/// The command for one of the PC's games (Settings → Games → PC streaming), on `host`.
pub fn start_app(host: &str, app: &str) -> Command {
    command_for(&bin(), Some(host), Some(app))
}

// ---------------------------------------------------------------------------------------------------
// Pairing and the PC's games (Settings → Games → PC streaming).

/// Moonlight Embedded keeps no list of the PCs it is paired with (the pairing lives on the PC, the client keys in
/// `/storage/.cache/moonlight`), so pocket-ui keeps one: `<data>/moonlight-pcs.json`.
pub const PCS_FILE: &str = "moonlight-pcs.json";
/// Where ROCKNIX's menu wrote one `.sh` per PC game (and a gamelist.xml); pocket-ui writes the same.
pub const GAMES_DIR: &str = "/storage/roms/moonlight";
/// How long a pairing may wait for the PIN to be typed on the PC.
pub const PAIR_SECS: u64 = 180;
/// How long `list` and `unpair` may take.
pub const ASK_SECS: u64 = 20;

/// The PCs pocket-ui paired (or saw ROCKNIX's `moonlight.host` for), in the order they were paired.
pub fn load_pcs(data_dir: &Path) -> Vec<String> {
    let list: Vec<String> = crate::fsutil::read_json_or_backup(&data_dir.join(PCS_FILE)).unwrap_or_default();
    list.into_iter().filter(|h| valid_host(h)).collect()
}

pub fn save_pcs(data_dir: &Path, pcs: &[String]) -> std::io::Result<()> {
    crate::fsutil::write_json(&data_dir.join(PCS_FILE), &pcs.to_vec())
}

/// What a line of `moonlight pair` says.
#[derive(Clone, Debug, PartialEq)]
pub enum PairNews {
    /// Moonlight looks for a PC on the network (no address given).
    Searching,
    /// It talks to this PC.
    Connecting(String),
    /// The PIN to type on the PC (Moonlight Embedded makes it up itself; it has no option to be given one).
    Pin(String),
    Paired,
    Failed(String),
}

/// One line of `moonlight pair` (Moonlight Embedded 2.7.0 on ROCKNIX 20261001; its spelling "Succesfully").
pub fn parse_pair_line(line: &str) -> Option<PairNews> {
    let line = line.trim();
    if line.starts_with("Searching for server") {
        return Some(PairNews::Searching);
    }
    if let Some(host) = line.strip_prefix("Connecting to ").and_then(|r| r.strip_suffix("...")) {
        return Some(PairNews::Connecting(host.trim().to_string()));
    }
    if let Some(pin) = line.strip_prefix("Please enter the following PIN on the target PC:") {
        let pin = pin.trim();
        return (!pin.is_empty() && pin.chars().all(|c| c.is_ascii_digit())).then(|| PairNews::Pin(pin.to_string()));
    }
    if line.starts_with("Succesfully paired") || line.starts_with("Successfully paired") {
        return Some(PairNews::Paired);
    }
    if let Some(why) = line.strip_prefix("Failed to pair to server:") {
        return Some(PairNews::Failed(format!("The PC refused the pairing ({}).", why.trim())));
    }
    if line.starts_with("Can't connect to server") || line.starts_with("Can not connect") {
        return Some(PairNews::Failed("No PC answered. Is it on, with Sunshine or GeForce Experience running?".into()));
    }
    None
}

/// `moonlight pair [host]`, line by line: stdout and stderr together, line-buffered with coreutils' `stdbuf` (the
/// PIN line would otherwise wait in a pipe buffer until Moonlight ends). `host` is an argument, never spliced.
pub fn pair_command(bin: &str, host: Option<&str>) -> Command {
    let mut c = Command::new("/bin/sh");
    let run = r#"if command -v stdbuf >/dev/null 2>&1; then exec stdbuf -oL -eL "$0" pair $1 2>&1; fi; exec "$0" pair $1 2>&1"#;
    c.arg("-c").arg(run).arg(bin).arg(host.unwrap_or(""));
    c
}

/// The app names in `moonlight list <host>` ("1. Steam", "2. Desktop").
pub fn parse_app_list(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| {
            let (n, name) = l.trim().split_once(". ")?;
            (n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() && !name.trim().is_empty()).then(|| name.trim().to_string())
        })
        .collect()
}

/// A shell word for `s`: single quotes, a quote inside as `'\''`. App names come from the PC; a PC on the network
/// must not be able to put commands into a script the Pocket runs.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The file a PC game is written to: its name without `/` (as ROCKNIX does), control characters or a leading dot.
pub fn game_file_name(title: &str) -> Option<String> {
    let clean: String = title.chars().map(|c| if c == '/' || c.is_control() { ' ' } else { c }).collect();
    let clean = clean.trim_start_matches(|c: char| c == '.' || c.is_whitespace()).trim_end().chars().take(120).collect::<String>();
    (!clean.is_empty()).then(|| format!("{clean}.sh"))
}

/// The start file of one PC game, as ROCKNIX's menu writes it for Moonlight Embedded, with its name and the PC
/// quoted for the shell.
pub fn game_script(title: &str, host: &str, file: &str) -> String {
    format!(
        "#!/bin/sh\n# Written by pocket-ui (Settings → Games → PC streaming), as ROCKNIX's Moonlight menu does.\n/usr/bin/controller-layout moonlight {} /storage/.config/moonlight/gamecontrollerdb.txt\nmoonlight stream -app {} -platform sdl {}\n",
        shell_quote(file),
        shell_quote(title),
        shell_quote(host)
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// ROCKNIX's `gamelist.xml` for the PC's games (names; no box art, which pocket-ui does not fetch).
pub fn gamelist_xml(games: &[(String, String)]) -> String {
    let mut x = String::from("<?xml version=\"1.0\"?>\n<gameList>\n");
    for (title, file) in games {
        x.push_str(&format!("  <game>\n    <path>./{}</path>\n    <name>{}</name>\n  </game>\n", xml_escape(file), xml_escape(title)));
    }
    x.push_str("</gameList>\n");
    x
}

/// Writes the PC's games into `dir` like ROCKNIX's menu: the `.sh` files and gamelist.xml there before are replaced
/// (other files stay). Returns how many were written.
pub fn write_games(dir: &Path, host: &str, titles: &[String]) -> std::io::Result<usize> {
    use std::os::unix::fs::PermissionsExt;
    if !valid_host(host) {
        return Err(std::io::Error::other("not a PC address"));
    }
    std::fs::create_dir_all(dir)?;
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "sh") && p.is_file() {
            std::fs::remove_file(&p)?;
        }
    }
    let mut written: Vec<(String, String)> = Vec::new();
    for title in titles {
        let Some(file) = game_file_name(title) else { continue };
        if written.iter().any(|(_, f)| *f == file) {
            continue;
        }
        let path = dir.join(&file);
        crate::fsutil::write_atomic(&path, game_script(title, host, &file).as_bytes())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        written.push((title.clone(), file));
    }
    crate::fsutil::write_atomic(&dir.join("gamelist.xml"), gamelist_xml(&written).as_bytes())?;
    Ok(written.len())
}

/// The PC games ROCKNIX's menu (or pocket-ui) wrote into `dir`, by name: what Settings → PC streaming lists.
pub fn written_games(dir: &Path) -> Vec<String> {
    let xml = std::fs::read_to_string(dir.join("gamelist.xml")).unwrap_or_default();
    let mut names: Vec<String> = crate::tools::parse_gamelist(&xml)
        .into_iter()
        .filter(|(file, _, _)| dir.join(file).is_file())
        .map(|(file, name, _)| if name.is_empty() { file.trim_end_matches(".sh").to_string() } else { name })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Runs `moonlight <action> <host>` with a time limit and returns its output (stdout and stderr).
fn ask(bin: &str, action: &str, host: &str) -> Result<String, String> {
    if !valid_host(host) {
        return Err("Not a PC address.".into());
    }
    let out = crate::proc::run_with_timeout(Command::new(bin).arg(action).arg(host), std::time::Duration::from_secs(ASK_SECS))
        .map_err(|e| format!("Moonlight didn't answer ({e})."))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if text.contains("Can't connect to server") {
        return Err("The PC didn't answer. Is it on?".into());
    }
    if text.contains("You must pair") {
        return Err("This PC isn't paired with the Pocket any more. Pair it again.".into());
    }
    Ok(text)
}

/// The PC's games (`moonlight list <host>`).
pub fn list_apps(bin: &str, host: &str) -> Result<Vec<String>, String> {
    let text = ask(bin, "list", host)?;
    let apps = parse_app_list(&text);
    if apps.is_empty() {
        Err("The PC listed no games.".into())
    } else {
        Ok(apps)
    }
}

/// Unpairs the Pocket from the PC (`moonlight unpair <host>`): the PC forgets this Pocket.
pub fn unpair(bin: &str, host: &str) -> Result<(), String> {
    let text = ask(bin, "unpair", host)?;
    if text.contains("Failed to unpair") {
        return Err(text.lines().find(|l| l.contains("Failed")).unwrap_or("The PC refused.").trim().to_string());
    }
    Ok(())
}

/// What to tell the user when the stream ended without ever running.
pub fn never_started(secs: u64, ok: bool, quit_asked: bool) -> Option<&'static str> {
    (!ok && !quit_asked && secs < NEVER_STARTED)
        .then_some("Couldn't start the stream. Is your PC on, and paired with this Pocket? See PC streaming in the user guide.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::os::unix::fs::PermissionsExt;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("moonlight-{name}"))
    }

    #[test]
    fn moonlight_counts_as_installed_only_when_the_program_is_there_and_runnable() {
        let d = dir("installed");
        let bin = d.join("moonlight");
        assert!(!installed_at(&bin), "missing");
        std::fs::write(&bin, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!installed_at(&bin), "there but not executable");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(installed_at(&bin));
        assert!(!installed_at(&d), "a folder is not a program");
    }

    #[test]
    fn the_pc_address_from_rocknixs_setting_is_used_only_when_it_is_safe_to_pass_on() {
        assert_eq!(host_setting("a=1\nmoonlight.host=192.168.1.20\nb=2\n").as_deref(), Some("192.168.1.20"));
        assert_eq!(host_setting("moonlight.host=gaming-pc.local\n").as_deref(), Some("gaming-pc.local"));
        assert_eq!(host_setting("moonlight.host=fe80::1\n").as_deref(), Some("fe80::1"));
        assert_eq!(host_setting("moonlight.host=\n"), None);
        assert_eq!(host_setting("other=1\n"), None);
        assert_eq!(host_setting("moonlight.host=1.2.3.4; reboot\n"), None, "no shell syntax");
        assert_eq!(host_setting("moonlight.host=-config\n"), None, "not an option");
        assert_eq!(host_setting("moonlight.host=$(id)\n"), None);
    }

    #[test]
    fn the_script_looks_the_pc_up_before_it_streams_and_does_not_hang_forever() {
        let s = script("/usr/bin/moonlight");
        assert!(s.contains(&format!("timeout {LOOKUP_SECS} /usr/bin/moonlight list")), "{s}");
        assert!(s.contains("exit 3"), "{s}");
        assert!(s.trim_end().ends_with("exec /usr/bin/moonlight stream -platform sdl $HOST"), "{s}");
        assert!(!s.contains("pair"), "pairing is a separate step, never a side effect of starting");
    }

    #[test]
    fn the_command_is_sh_with_the_host_as_an_argument() {
        let c = command("/usr/bin/moonlight", Some("192.0.2.9"));
        assert_eq!(c.get_program(), "/bin/sh");
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args[0], "-c");
        assert_eq!(args[1], script("/usr/bin/moonlight"));
        assert_eq!(&args[2..], ["sh", "192.0.2.9", ""]);
        let none = command("/usr/bin/moonlight", None);
        assert_eq!(none.get_args().count(), 5, "an empty host, so $HOST is nothing, and no app");
    }

    #[test]
    fn a_chosen_game_is_streamed_by_name_passed_as_an_argument() {
        let s = script("/usr/bin/moonlight");
        assert!(s.contains(r#"exec /usr/bin/moonlight stream -platform sdl -app "$2" $HOST"#), "{s}");
        let c = command_for("/usr/bin/moonlight", Some("192.0.2.9"), Some("Steam; reboot"));
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(&args[2..], ["sh", "192.0.2.9", "Steam; reboot"], "the name is an argument, not shell");
    }

    #[test]
    fn pairing_reads_the_pin_moonlight_makes_up_and_how_it_ended() {
        // Moonlight Embedded 2.7.0 (ROCKNIX 20261001) has no -pin option: it prints its own PIN.
        assert_eq!(parse_pair_line("Searching for server..."), Some(PairNews::Searching));
        assert_eq!(parse_pair_line("Connecting to 192.168.1.20..."), Some(PairNews::Connecting("192.168.1.20".into())));
        assert_eq!(parse_pair_line("Please enter the following PIN on the target PC: 4712"), Some(PairNews::Pin("4712".into())));
        assert_eq!(parse_pair_line("Please enter the following PIN on the target PC: x"), None);
        assert_eq!(parse_pair_line("Succesfully paired"), Some(PairNews::Paired));
        assert!(matches!(parse_pair_line("Failed to pair to server: Wrong PIN"), Some(PairNews::Failed(w)) if w.contains("Wrong PIN")));
        assert!(matches!(parse_pair_line("Can't connect to server 192.0.2.1"), Some(PairNews::Failed(_))));
        assert_eq!(parse_pair_line("Generating certificate...done"), None);
        let c = pair_command("/usr/bin/moonlight", Some("192.0.2.9"));
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args[1].contains("stdbuf -oL -eL"), "line-buffered, or the PIN waits in the pipe");
        assert_eq!(&args[2..], ["/usr/bin/moonlight", "192.0.2.9"]);
    }

    #[test]
    fn the_pcs_games_are_written_like_rocknix_does_but_safely_quoted() {
        assert_eq!(parse_app_list("1. Steam\n2. Desktop\nnoise\n3. \n"), ["Steam", "Desktop"]);
        assert_eq!(game_file_name("Half-Life 2/Ep. 1"), Some("Half-Life 2 Ep. 1.sh".into()));
        assert_eq!(game_file_name("../.."), None, "no hidden file, no way up");
        assert_eq!(game_file_name("..."), None);
        let s = game_script("It's \"x\"; reboot", "192.0.2.9", "It's.sh");
        assert!(s.contains(r#"moonlight stream -app 'It'\''s "x"; reboot' -platform sdl '192.0.2.9'"#), "{s}");
        let d = dir("games");
        std::fs::write(d.join("Old game.sh"), "old").unwrap();
        std::fs::write(d.join("notes.txt"), "mine").unwrap();
        let n = write_games(&d, "192.0.2.9", &["Steam".into(), "Desktop".into(), "Steam".into(), "<Tag> & co".into()]).unwrap();
        assert_eq!(n, 3, "one file per name");
        assert!(!d.join("Old game.sh").exists() && d.join("notes.txt").exists());
        assert_eq!(written_games(&d), ["<Tag> & co", "Desktop", "Steam"]);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(d.join("Steam.sh")).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(write_games(&d, "-x; rm", &[]).is_err(), "never an address that reads like an option or shell");
    }

    #[test]
    fn paired_pcs_are_remembered_and_only_valid_ones_read() {
        let d = dir("pcs");
        assert!(load_pcs(&d).is_empty());
        save_pcs(&d, &["192.168.1.20".into(), "gaming-pc.local".into()]).unwrap();
        assert_eq!(load_pcs(&d), ["192.168.1.20", "gaming-pc.local"]);
        std::fs::write(d.join(PCS_FILE), r#"["ok.lan", "-bad", "$(x)"]"#).unwrap();
        assert_eq!(load_pcs(&d), ["ok.lan"]);
    }

    #[test]
    fn a_stream_that_never_got_going_is_explained_one_that_ran_or_was_quit_is_not() {
        assert!(never_started(2, false, false).is_some_and(|m| m.contains("paired")));
        assert!(never_started(NEVER_STARTED - 1, false, false).is_some());
        assert_eq!(never_started(NEVER_STARTED, false, false), None, "it ran for a while");
        assert_eq!(never_started(3600, false, false), None);
        assert_eq!(never_started(2, true, false), None, "exit code 0");
        assert_eq!(never_started(2, false, true), None, "asked to stop");
    }
}
