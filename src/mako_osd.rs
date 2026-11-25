//! ROCKNIX's own volume and brightness box. `input_sense` answers every Vol and HOME + Vol press with
//! `mako-notify "Volume: 25%" -no-es`: a black monospace box at the top of the screen, drawn by the `mako` daemon over
//! everything. `mako-notify` leaves it out only when EmulationStation is in front, and pocket-ui's `app_id` is not
//! that, so the box landed on top of pocket-ui's own display (`osd.rs`).
//!
//! The config block hides the stock box on the shelf and wherever pocket-core draws its own OSD.
//! On H700, the game mode lets through only ROCKNIX's volume notification, in a smaller box.
//! ROCKNIX sends it on key release, with no additional volume polling or hidden-shelf redraw.
//! Other standalone games get the regular ROCKNIX notifications.
//!
//! Config updates preserve the rest of the user's file. Mode changes run off the UI thread, and the last
//! request wins. The daemon keeps its modes across a reload. The uninstaller removes both modes and the block.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Measurement override only: set POCKETUI_H700_VOLUME_OSD=1 before start to retain the old display path.
pub fn quiet_h700_games() -> bool {
    quiet_for(
        crate::device::current().model.is_some_and(|m| m.is_h700()),
        crate::device::darkos_port(),
        std::env::var("POCKETUI_H700_VOLUME_OSD").as_deref() == Ok("1"),
    )
}

/// The quiet game display is for the Allwinner H700 under ROCKNIX, where it was measured (docs/H700-VOLUME.md). The dArkOS port
/// borrows the H700 profile's rows for its RK3566/RK3326 boards (`darkos::BOARDS`), but it is not that chip or that audio path:
/// it keeps the volume bar over a game, as every other device does.
fn quiet_for(h700_profile: bool, darkos: bool, old_display: bool) -> bool {
    h700_profile && !darkos && !old_display
}

pub const CONFIG: &str = "/storage/.config/mako/config";
/// `mako-notify` is what `input_sense` calls; without it there is no box to hide.
const NOTIFY: &str = "/usr/bin/mako-notify";
const MODE: &str = "pocket-ui";
const GAME_VOLUME_MODE: &str = "pocket-ui-game-volume";
const BEGIN: &str = "# BEGIN pocket-ui";
const END: &str = "# END pocket-ui";
/// The notification's `app_name` (what `mako-notify` sends over D-Bus).
const APP_NAME: &str = "mako-notify";
/// Where the session bus is on ROCKNIX when the environment doesn't say (`/etc/profile.d/050-sway.conf`).
const RUNTIME_DIR: &str = "/var/run/0-runtime-dir";

/// What `mako-notify` writes when the config is missing (ROCKNIX 20261001, H700 and the others alike). A card that
/// never had a volume press has no file yet: the box of the first press would come before our block, so we write the
/// same default with the block, and `mako-notify` finds the file there and leaves it.
const MAKO_DEFAULT: &str = "max-visible=1\nlayer=overlay\nfont=monospace 30\ntext-color=#ffffff\ntext-alignment=center\nbackground-color=#000000\nborder-size=0\nborder-radius=10\ndefault-timeout=1500\nanchor=top-center\nwidth=500\n";

fn block() -> String {
    format!(
        "{BEGIN} (routes ROCKNIX's OSD while pocket-ui is in front; removed by uninstall.sh)\n\
         [mode={MODE} app-name={APP_NAME}]\ninvisible=1\n\
         [mode={GAME_VOLUME_MODE} app-name={APP_NAME} summary~=\"^Volume: [0-9]+%$\"]\n\
         invisible=0\nlayer=overlay\nanchor=top-center\nformat=%s\nfont=monospace 16\nwidth=176\nheight=42\n\
         padding=6\ndefault-timeout=1000\nignore-timeout=1\n{END}\n"
    )
}

/// `text` without pocket-ui's block (a block without its end is cut to the end of the file).
pub fn without_block(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with(BEGIN) {
            inside = true;
        }
        if !inside {
            out.push_str(line);
        } else if line.starts_with(END) {
            inside = false;
        }
    }
    out
}

/// `text` with pocket-ui's block as it is now at the end; None when it already is exactly that (idempotent).
pub fn with_block(text: &str) -> Option<String> {
    let mut base = without_block(text);
    if !base.is_empty() && !base.ends_with('\n') {
        base.push('\n');
    }
    base.push_str(&block());
    (base != text).then_some(base)
}

/// Puts the block into mako's config and tells mako. Slow (a process): call it from a thread. Does nothing without
/// `mako-notify`; a missing config is made with ROCKNIX's default and the block (`MAKO_DEFAULT`). Returns whether
/// the file changed.
fn ensure() -> Result<bool, String> {
    if !Path::new(NOTIFY).exists() {
        return Ok(false);
    }
    let changed = ensure_in(Path::new(CONFIG))?;
    if changed {
        makoctl(Path::new("makoctl"), &["reload"]).map_err(|e| format!("wrote the config, but {e}"))?;
    }
    Ok(changed)
}

fn ensure_in(config: &Path) -> Result<bool, String> {
    let text = match std::fs::read_to_string(config) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = config.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
            }
            MAKO_DEFAULT.to_string()
        }
        Err(e) => return Err(format!("couldn't read {}: {e}", config.display())),
    };
    let Some(new) = with_block(&text) else { return Ok(false) };
    crate::fsutil::write_atomic(config, new.as_bytes()).map_err(|e| format!("couldn't write {}: {e}", config.display()))?;
    Ok(true)
}

fn makoctl(bin: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(bin);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        cmd.env("XDG_RUNTIME_DIR", RUNTIME_DIR);
    }
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("makoctl {} failed ({s})", args.join(" "))),
        Err(e) => Err(format!("makoctl {}: {e}", args.join(" "))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Display {
    PocketUi,
    Rocknix,
    RocknixVolume,
}

fn game_display(hosted: bool, quiet_h700: bool) -> Display {
    if quiet_h700 {
        Display::RocknixVolume
    } else if hosted {
        Display::PocketUi
    } else {
        Display::Rocknix
    }
}

fn set_mode(bin: &Path, display: Display) -> Result<(), String> {
    let hide_stock = display != Display::Rocknix;
    let show_volume = display == Display::RocknixVolume;
    makoctl(bin, &[
        "mode",
        if hide_stock { "-a" } else { "-r" }, MODE,
        if show_volume { "-a" } else { "-r" }, GAME_VOLUME_MODE,
    ])
}

static WANT: Mutex<Display> = Mutex::new(Display::PocketUi);
/// One `makoctl` at a time, so that two quick changes arrive in order.
static APPLY: Mutex<()> = Mutex::new(());
static COMPLAINED: AtomicBool = AtomicBool::new(false);

/// Install the block after the first frame, then apply the latest display, even if a game already started.
pub fn start() -> Result<bool, String> {
    let result = ensure();
    apply();
    result
}

pub fn show_shelf() {
    set_display(Display::PocketUi);
}

pub fn show_game(hosted: bool) {
    set_display(game_display(hosted, quiet_h700_games()));
}

fn set_display(display: Display) {
    *WANT.lock().unwrap_or_else(|e| e.into_inner()) = display;
    apply();
}

fn apply() {
    if !Path::new(NOTIFY).exists() {
        return;
    }
    std::thread::spawn(|| {
        let _one_at_a_time = APPLY.lock().unwrap_or_else(|e| e.into_inner());
        let display = *WANT.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = set_mode(Path::new("makoctl"), display) {
            if !COMPLAINED.swap(true, Ordering::Relaxed) {
                eprintln!("mako: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h700_rocknix_uses_native_volume_and_other_games_keep_their_display() {
        let h700 = quiet_for(true, false, false);
        for hosted in [false, true] {
            assert_eq!(game_display(hosted, h700), Display::RocknixVolume);
        }
        assert_eq!(game_display(true, quiet_for(true, true, false)), Display::PocketUi);
        assert_eq!(game_display(true, quiet_for(true, false, true)), Display::PocketUi);
        assert_eq!(game_display(false, false), Display::Rocknix);
        assert!(!quiet_for(true, true, false), "dArkOS (H700 profile rows, RK3566/RK3326 chips): the bar shows");
        assert!(!quiet_for(true, false, true), "the measurement override brings the old display back");
        assert!(!quiet_for(false, false, false));
    }
    use crate::testutil::TestDir;
    use std::os::unix::fs::PermissionsExt;

    const ROCKNIX_DEFAULT: &str = "max-visible=1\nlayer=overlay\nfont=monospace 30\nanchor=top-center\nwidth=500\n";

    #[test]
    fn the_block_is_added_once_and_goes_again() {
        let with = with_block(ROCKNIX_DEFAULT).unwrap();
        assert!(with.starts_with(ROCKNIX_DEFAULT), "ROCKNIX's lines stay as they were");
        let hidden = "[mode=pocket-ui app-name=mako-notify]\ninvisible=1\n";
        let compact_volume = concat!(
            "[mode=pocket-ui-game-volume app-name=mako-notify summary~=\"^Volume: [0-9]+%$\"]\n",
            "invisible=0\nlayer=overlay\nanchor=top-center\nformat=%s\nfont=monospace 16\nwidth=176\nheight=42\n",
            "padding=6\ndefault-timeout=1000\nignore-timeout=1\n",
        );
        assert!(with.contains(&format!("{hidden}{compact_volume}")), "only the volume summary overrides the hide rule");
        assert_eq!(with_block(&with), None, "idempotent");
        assert_eq!(without_block(&with), ROCKNIX_DEFAULT, "the undo restores the file exactly");
        assert_eq!(without_block(ROCKNIX_DEFAULT), ROCKNIX_DEFAULT);
    }

    #[test]
    fn a_file_without_final_newline_or_with_an_old_block_is_repaired() {
        let with = with_block("width=500").unwrap();
        assert!(with.starts_with("width=500\n# BEGIN pocket-ui"));
        let old = format!("a=1\n{BEGIN} old\n[mode=x]\ninvisible=0\n{END}\nb=2\n");
        let new = with_block(&old).unwrap();
        assert_eq!(new.matches(BEGIN).count(), 1);
        assert!(new.starts_with("a=1\nb=2\n") && !new.contains("[mode=x]"));
        // No block yet in an empty file.
        assert_eq!(with_block("").unwrap(), block());
    }

    #[test]
    fn the_config_is_changed_in_place_and_a_missing_one_is_made() {
        let dir = TestDir::new("mako");
        let cfg = dir.join("mako/config");
        // A fresh card: no config before the first volume press. The block must be there before that press.
        assert!(ensure_in(&cfg).unwrap());
        assert_eq!(without_block(&std::fs::read_to_string(&cfg).unwrap()), MAKO_DEFAULT);
        assert!(!ensure_in(&cfg).unwrap());
        std::fs::remove_file(&cfg).unwrap();
        std::fs::write(&cfg, ROCKNIX_DEFAULT).unwrap();
        assert!(ensure_in(&cfg).unwrap());
        assert!(!ensure_in(&cfg).unwrap(), "the second start changes nothing");
        assert_eq!(without_block(&std::fs::read_to_string(&cfg).unwrap()), ROCKNIX_DEFAULT);
    }

    #[test]
    fn the_mode_is_added_and_removed_with_makoctl() {
        let dir = TestDir::new("makoctl");
        let bin = dir.join("makoctl");
        let log = dir.join("calls");
        std::fs::write(&bin, format!("#!/bin/sh\necho \"$*\" >>{}\n[ \"$1\" != fail ]\n", log.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        set_mode(&bin, Display::RocknixVolume).unwrap();
        set_mode(&bin, Display::PocketUi).unwrap();
        set_mode(&bin, Display::Rocknix).unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), concat!(
            "mode -a pocket-ui -a pocket-ui-game-volume\n",
            "mode -a pocket-ui -r pocket-ui-game-volume\n",
            "mode -r pocket-ui -r pocket-ui-game-volume\n",
        ));
        assert!(makoctl(&bin, &["fail"]).is_err());
        assert!(set_mode(&dir.join("nothing"), Display::PocketUi).is_err(), "no makoctl: an error, no panic");
    }
    #[test]
    fn file_source_rule_notification_config() {
        let d = crate::testutil::TestDir::new("mako-file-rule");
        let file = d.join("config");
        for (label, bytes) in crate::file_faults::faults(MAKO_DEFAULT.as_bytes()) {
            let _ = std::fs::remove_file(&file);
            if let Some(bytes) = bytes { std::fs::write(&file, bytes).unwrap(); }
            match ensure_in(&file) {
                Ok(_) => { assert!(!std::fs::read_to_string(&file).unwrap().trim().is_empty(), "{label}: blank config"); }
                Err(reason) => assert!(reason.contains("couldn't read"), "{label}: {reason}"),
            }
        }
    }

}
