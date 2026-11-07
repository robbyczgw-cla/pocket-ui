//! The TrimUI Brick under NextUI, as one place to change (docs/TRIMUI-BRICK.md). A DRAFT from files read off the
//! device (2026-11-02, read only) and Knulli's `es_input.cfg`; the kit's `read` and `input` stages must still confirm
//! every line marked CONFIRM. A wrong value is a one-line change here.

use std::path::Path;

pub const NAME: &str = "TrimUI Brick";
/// What the device says about itself in the log and the report.
pub const REPORTED: &str = "TrimUI Brick (NextUI)";
/// fb0 as read on the device: 1024x768, 32 bits per pixel (`fb0/virtual_size` is 1024x16384: pages to flip through).
/// Confirmed 2026-11-02: the shown page is `fb0/pan`; `pocket-fb` follows the kernel's `yoffset` and the read-back matches the picture.
pub const SCREEN: (u32, u32) = (1024, 768);

/// The built-in pad as the kernel names it (`/proc/bus/input/devices`; trimui_inputd makes it from the keys).
pub const PAD: &str = "TRIMUI Player1";

/// The Brick and the Brick Pro both: the stock system has `/usr/trimui/bin/MainUI`, NextUI's card has `.system`.
/// CONFIRM (`read`, the string in `MainUI`): the Brick Pro. NextUI tells them apart by a string inside MainUI; the profile is the same here.
pub fn detected(root: &Path) -> bool {
    root.join("usr/trimui/bin/MainUI").is_file() && root.join("mnt/SDCARD/.system").is_dir()
}

/// The Brick running from its own card, without NextUI (`POCKETUI_BRICK_STANDALONE=1`, set by the card's start script): the
/// Brick has Wi-Fi, and nothing else owns it, so the profile that leaves it out for the NextUI pak does not apply.
pub fn standalone() -> bool {
    std::env::var_os("POCKETUI_BRICK_STANDALONE").is_some_and(|v| !v.is_empty())
}

/// Whether the card carries an SMB server for file sharing: its `smbd` stand-in lies in the fake root (`POCKETUI_HW_ROOT`, `usr/bin/smbd`).
/// Without it Add games offers no file sharing tile (a switch that cannot work is worse than none); the phone page does not need it.
pub fn file_sharing() -> bool {
    std::env::var_os("POCKETUI_HW_ROOT").is_some_and(|r| Path::new(&r).join("usr/bin/smbd").is_file())
}

/// Whether evdev device `name` is the Brick's pad. **Read on the Brick with the owner (2026-11-02, `input` stage):** A 305, B 304,
/// X 308 and Y 307 (printed X reports BTN_WEST and printed Y BTN_NORTH, the H700 pad's quirk: the keys cross over here, so printed X
/// is `North` like on every other pad), L 310, R 311, Select 314, Start 315, Menu 316 (`Mode`), the D-pad as hat axes 16 and 17.
/// **L2 and R2 are not keys** (312 and 313 never come): they are analog axes, ABS_Z (code 2) and ABS_RZ (code 5), 0 to 255, which
/// `pad.rs` already reads as `Axis::L2` and `Axis::R2`. Knulli's `es_input.cfg` listed 312/313 as buttons: wrong for NextUI's pad.
pub fn is_pad(name: &str) -> bool {
    name.trim() == PAD
}

/// Additional software cores built for our standalone card. The NextUI pak keeps its existing systems.
pub fn additional_core(system: &str) -> Option<&'static str> {
    if !crate::device::brick() || !standalone() {
        return None;
    }
    additional_core_on_card(system)
}

fn additional_core_on_card(system: &str) -> Option<&'static str> {
    match system {
        "fds" => Some("fceumm"),
        "sg-1000" => Some("gearsystem"),
        "segacd" => Some("picodrive"),
        "sega32x" => Some("picodrive"),
        "pcenginecd" => Some("beetle_pce_fast"),
        "supergrafx" => Some("beetle_supergrafx"),
        "pokemini" => Some("pokemini"),
        "virtualboy" => Some("beetle_vb"),
        "pico-8" => Some("fake08"),
        "tic-80" => Some("tic80"),
        "nds" => Some("melondsds"),
        "pc" => Some("dosbox_pure"),
        "scummvm" => Some("scummvm"),
        "saturn" => Some("yabause"),
        "ngp" | "ngpc" => Some("beetle_ngp"),
        "wonderswan" | "wonderswancolor" => Some("beetle_wswan"),
        "pcengine" => Some("beetle_pce_fast"),
        "atarilynx" => Some("handy"),
        "atari2600" => Some("stella"),
        "atari7800" => Some("prosystem"),
        "fbn" | "neogeo" | "cps1" | "cps2" | "cps3" => Some("fbneo"),
        // The 3D cores (GLES 3, no Vulkan): they draw through pocket-core's own context (`brick_gles`).
        "n64" => Some("mupen64plus_next"),
        "psp" => Some("ppsspp"),
        "dreamcast" => Some("flycast"),
        _ => None,
    }
}

/// Whether the Picture rows (scaling, sharpness, effect, frame, orientation) stay out: on a Brick whose bridge has no GPU path
/// (`pocket-fb` without `--gl`, or one that lost it) the CPU copy ignores them, and a row that does nothing is worse than none.
/// Settings > Screen frames follows the same answer. With `POCKET_BRICK_GL=1` and a working GPU they all come back.
pub fn picture_rows_out() -> bool {
    crate::device::brick() && !crate::brick_picture::shaders_available()
}

/// Picture choices that are heavy for the Brick's GPU (GE8300, 786 432 pixels a picture; docs/BRICK-SHADERS.md has the counts and the
/// host timings): the menu says so beside the choice. (setting key, value). Light ones (Sharp, Soft, Crisp, the plain frames) are not listed.
pub const HEAVY: [(&str, &str); 3] = [("effect", "scanlines"), ("effect", "crt"), ("frame", "ambient")];

/// `choice` of the setting `key` with " (heavy)" after it when the Brick's GPU path draws it and it is one of `HEAVY`; otherwise as is.
pub fn labelled(key: &str, value: &str, label: &str) -> String {
    if crate::device::brick() && HEAVY.contains(&(key, value)) { format!("{label} (heavy)") } else { label.to_string() }
}

/// PCE Fast reads this exact filename; the broader ROCKNIX check accepts other System Cards.
pub fn missing_bios(system: &str, directory: &Path) -> Option<String> {
    if crate::device::brick() && standalone() {
        card_missing_bios(system, directory)
    } else {
        None
    }
}

fn card_missing_bios(system: &str, directory: &Path) -> Option<String> {
    if system == "pcenginecd" && !directory.join("syscard3.pce").is_file() {
        Some("PC Engine CD needs a BIOS file in Roms/bios: syscard3.pce".into())
    } else {
        None
    }
}

/// Use the card's bundled engine data instead of ROCKNIX's /usr/share directory.
pub fn prepare_scummvm(bios: &Path) -> bool {
    if !crate::device::brick() || !standalone() { return false; }
    let path = bios.join("scummvm.ini");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let saves = Path::new("/storage/roms/scummvm/.saves");
    let _ = std::fs::create_dir_all(saves);
    if let Some(updated) = crate::hostsys::scummvm_ini(&text, Path::is_dir) {
        let updated = card_scummvm_ini(&updated, bios);
        let _ = std::fs::write(path, updated);
    }
    true
}

fn card_scummvm_ini(text: &str, bios: &Path) -> String {
    text.replace("extrapath=/usr/share/scummvm", &format!("extrapath={}/scummvm/extra", bios.display()))
        .replace("themepath=/usr/share/scummvm", &format!("themepath={}/scummvm/theme", bios.display()))
        .replace("savepath=/storage/.local/share/scummvm/saves", "savepath=/storage/roms/scummvm/.saves")
}

/// The core NextUI's card has for a system whose default core (`hostsys::HANDHELD_CORES`) it does not have: gpSP for mGBA, FCEUmm for
/// Nestopia (2026-11-02: GBA asked for mgba_libretro.so, which is not on the card). The other systems keep their cores (gambatte,
/// snes9x are there), and PicoDrive for every Sega system. One place to change when the card's cores change.
pub fn core_for(core: &'static str) -> &'static str {
    if !crate::device::brick() {
        return core;
    }
    core_on_brick(core, standalone())
}

/// `core_for` on a Brick, `standalone` or with NextUI's card.
fn core_on_brick(core: &'static str, standalone: bool) -> &'static str {
    // PicoDrive plays what the Mega Drive core and Gearsystem play elsewhere (Mega Drive, Master System, Game Gear), on NextUI's card
    // and on the standalone card alike: it is the one Sega core either has.
    if matches!(core, "genesis_plus_gx" | "gearsystem") {
        return "picodrive";
    }
    // The standalone card has gpSP too (the one that ran 59.98 fps on the real Brick) and keeps Nestopia for NES; FCEUmm is on the card to choose.
    if standalone {
        return if core == "mgba" { "gpsp" } else { core };
    }
    match core {
        "mgba" => "gpsp",
        "nestopia" => "fceumm",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{self, Model};
    use crate::testutil::TestDir;

    #[test]
    fn the_brick_is_found_by_its_stock_system_and_the_nextui_card() {
        let d = TestDir::new("brick-detect");
        assert!(!detected(&*d));
        std::fs::create_dir_all(&*d.join("usr/trimui/bin")).unwrap();
        std::fs::write(&*d.join("usr/trimui/bin/MainUI"), b"x").unwrap();
        assert!(!detected(&*d), "stock system alone is not NextUI");
        std::fs::create_dir_all(&*d.join("mnt/SDCARD/.system")).unwrap();
        assert!(detected(&*d));
        let dev = device::read(&*d);
        assert_eq!(dev.model, Some(Model::Brick));
        assert_eq!((dev.screen(), dev.has_fan(), dev.has_touch(), dev.has_sticks(), dev.has_wifi()), ((1024, 768), false, false, false, false));
        assert_eq!(dev.ram_mb(), Some(1024));
        assert!(!dev.host_owned(), "the Brick is not the Deck: its windowing goes through pocket-fb");
    }

    #[test]
    fn the_override_names_it_and_settings_leave_out_what_nextui_owns() {
        assert_eq!(device::from_override("brick").model, Some(Model::Brick));
        for k in ["radios", "store", "updates", "tools", "perf_fan", "time", "tailscale"] {
            assert!(device::hides(k, false, true), "{k}");
        }
        assert!(!device::hides("looks", false, true) && !device::hides("sound", false, true));
    }

    #[test]
    fn the_brick_plays_sega_games_with_picodrive_and_gba_with_gpsp() {
        for standalone in [false, true] {
            for core in ["genesis_plus_gx", "gearsystem"] {
                assert_eq!(core_on_brick(core, standalone), "picodrive", "{core}");
            }
            assert_eq!((core_on_brick("mgba", standalone), core_on_brick("pcsx_rearmed", standalone), core_on_brick("snes9x", standalone)), ("gpsp", "pcsx_rearmed", "snes9x"));
        }
        assert_eq!((core_on_brick("nestopia", false), core_on_brick("nestopia", true)), ("fceumm", "nestopia"));
    }

    #[test]
    fn additional_card_cores_are_the_card_systems_only() {
        for (system, core) in [("fds", "fceumm"), ("sg-1000", "gearsystem"), ("segacd", "picodrive"), ("sega32x", "picodrive"),
            ("pcenginecd", "beetle_pce_fast"), ("supergrafx", "beetle_supergrafx"), ("pokemini", "pokemini"), ("virtualboy", "beetle_vb"),
            ("pico-8", "fake08"), ("tic-80", "tic80"), ("nds", "melondsds"), ("pc", "dosbox_pure"), ("scummvm", "scummvm"), ("saturn", "yabause"),
             ("ngp", "beetle_ngp"), ("ngpc", "beetle_ngp"), ("wonderswan", "beetle_wswan"),
            ("wonderswancolor", "beetle_wswan"), ("pcengine", "beetle_pce_fast"), ("atarilynx", "handy"),
            ("atari2600", "stella"), ("atari7800", "prosystem"), ("fbn", "fbneo"), ("neogeo", "fbneo"),
            ("cps1", "fbneo"), ("cps2", "fbneo"), ("cps3", "fbneo"),
            ("n64", "mupen64plus_next"), ("psp", "ppsspp"), ("dreamcast", "flycast")] {
            assert_eq!(additional_core_on_card(system), Some(core));
        }
        for system in ["naomi", "atomiswave"] {
            assert_eq!(additional_core_on_card(system), None);
        }
    }

    #[test]
    fn pce_fast_requires_the_filename_it_actually_opens() {
        let dir = TestDir::new("brick-pce-bios");
        std::fs::write(dir.join("syscard2.pce"), b"test").unwrap();
        assert!(card_missing_bios("pcenginecd", &dir).unwrap().contains("syscard3.pce"));
        std::fs::write(dir.join("syscard3.pce"), b"test").unwrap();
        assert!(card_missing_bios("pcenginecd", &dir).is_none());
        assert!(card_missing_bios("pcengine", &dir).is_none());
    }

    #[test]
    fn scummvm_uses_card_data_and_preserves_game_sections() {
        let text = "[scummvm]\nextrapath=/usr/share/scummvm\nthemepath=/usr/share/scummvm\nsavepath=/storage/.local/share/scummvm/saves\n[game]\npath=/storage/roms/scummvm/game\n";
        let updated = card_scummvm_ini(text, Path::new("/storage/roms/bios"));
        assert!(updated.contains("extrapath=/storage/roms/bios/scummvm/extra"));
        assert!(updated.contains("themepath=/storage/roms/bios/scummvm/theme"));
        assert!(updated.contains("savepath=/storage/roms/scummvm/.saves"));
        assert!(updated.ends_with("[game]\npath=/storage/roms/scummvm/game\n"));
    }

    #[test]
    fn only_its_own_pad_crosses_x_and_y() {
        assert!(is_pad("TRIMUI Player1") && is_pad(" TRIMUI Player1 "));
        assert!(!is_pad("Xbox Wireless Controller") && !is_pad("sunxi-keyboard"));
    }
}
