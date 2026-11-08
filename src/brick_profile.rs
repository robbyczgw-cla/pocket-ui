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

/// Which TrimUI handheld of the A133P family this is: the `brick` build and its card run on both (docs/TRIMUI-SMART-PRO.md).
/// Everything the two share (the firmware, the card, NextUI's way, 1 GB, no fan, no touch) is `device::brick()`; what differs is here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Board {
    /// The TrimUI Brick: 3.2" 1024x768, no sticks, F1 and F2 beside the screen, the mute slider, the lights `brick_hw` drives.
    Brick,
    /// The TrimUI Smart Pro: 4.96" 1280x720, two sticks with L3 and R3, no F keys, no slider. Not played on by the project
    /// (no device); its lights are a ring around each stick that nobody has mapped, so they stay dark (`leds`).
    SmartPro,
}

impl Board {
    pub const ALL: [Board; 2] = [Board::Brick, Board::SmartPro];

    pub fn name(self) -> &'static str {
        match self {
            Board::Brick => NAME,
            Board::SmartPro => "TrimUI Smart Pro",
        }
    }

    /// The handheld's own word where a line calls it "the Pocket" (`i18n::brick_words`).
    pub fn word(self) -> &'static str {
        match self {
            Board::Brick => "Brick",
            Board::SmartPro => "Smart Pro",
        }
    }

    /// What the device says about itself in the log and the report.
    pub fn reported(self) -> &'static str {
        match self {
            Board::Brick => REPORTED,
            Board::SmartPro => "TrimUI Smart Pro (NextUI)",
        }
    }

    /// `POCKETUI_DEVICE`'s name (start.sh sets it after `board` in deploy/brick/start.sh).
    pub fn key(self) -> &'static str {
        match self {
            Board::Brick => "brick",
            Board::SmartPro => "smartpro",
        }
    }

    pub fn by_key(key: &str) -> Option<Board> {
        Board::ALL.into_iter().find(|b| b.key().eq_ignore_ascii_case(key.trim()))
    }

    /// The screen as fb0 has it. The Smart Pro's from its specification and every project that runs on it (no rotation).
    pub fn screen(self) -> (u32, u32) {
        match self {
            Board::Brick => SCREEN,
            Board::SmartPro => SMART_PRO_SCREEN,
        }
    }

    /// Two analog sticks (and their presses, L3 and R3).
    pub fn sticks(self) -> bool {
        self == Board::SmartPro
    }

    /// F1 and F2 beside the screen: on the Brick they arrive as 317 and 318, which are the stick presses on the Smart Pro.
    pub fn fkeys(self) -> bool {
        self == Board::Brick
    }

    /// The mute slider (gpio243 on the Brick). The Smart Pro has none: nothing may read that pin as "muted" there.
    pub fn slider(self) -> bool {
        self == Board::Brick
    }

    /// Lights pocket-ui drives (`brick_hw`'s zones, read on the Brick). The Smart Pro's rings are the same driver with other zones;
    /// until a tester has mapped them they stay off (start.sh writes them black) and Settings has no lights page.
    pub fn leds(self) -> bool {
        self == Board::Brick
    }

    /// The small type a step larger (`type_scale::brick_base`): the Brick's panel is 400 ppi (15.75 px a mm). The Smart Pro's
    /// 4.96" at 1280x720 is about 296 ppi (11.7 px a mm): the design's Body 28 is 2.4 mm there, larger than the Brick's 31 (2.0 mm),
    /// so it keeps the design's sizes.
    pub fn larger_small_type(self) -> bool {
        self == Board::Brick
    }

    /// The project has played on it (the first start doesn't call it untested; About says no "(experimental)").
    pub fn played_on(self) -> bool {
        self == Board::Brick
    }

    pub fn experimental(self) -> bool {
        !self.played_on()
    }
}

/// Settings › Sound & feel › Lights & buttons (the lights, F1 and F2, the mute slider): only where one of them is ours to drive.
pub fn lights_page() -> bool {
    board().is_some_and(|b| b.leds() || b.fkeys() || b.slider())
}

/// F1 and F2 on this device (`Board::fkeys`).
pub fn fkeys() -> bool {
    board().is_some_and(Board::fkeys)
}

/// What a report (Settings › System › About › Report a problem) adds on a board the project has not played on (the Smart Pro):
/// the input devices as the kernel lists them (`input_devices`, the file below the report's root) and the end of the card's
/// `boot.log` (which board start.sh found, the GPU's answer). Nothing on the Brick, whose reports stay as they were.
pub fn report_section(input_devices: &Path) -> String {
    let card = std::env::var_os("POCKETUI_BRICK_CARD").map(std::path::PathBuf::from);
    report_section_for(board(), input_devices, card.as_deref())
}

fn report_section_for(board: Option<Board>, input_devices: &Path, card: Option<&Path>) -> String {
    if !board.is_some_and(Board::experimental) {
        return String::new();
    }
    let devices = std::fs::read_to_string(input_devices).unwrap_or_else(|_| "(missing)\n".into());
    let boot = card.and_then(|d| std::fs::read_to_string(d.join("logs/boot.log")).ok());
    let boot = boot.map_or_else(|| "(missing)\n".to_string(), |t| {
        let lines: Vec<&str> = t.lines().collect();
        lines[lines.len().saturating_sub(60)..].join("\n") + "\n"
    });
    format!("\n[input_devices]\n{devices}\n[boot_log]\n{boot}")
}

/// The Smart Pro's screen: 1280x720 (16:9), landscape as fb0 gives it.
pub const SMART_PRO_SCREEN: (u32, u32) = (1280, 720);

/// This device's board, when it is one of the family.
pub fn board() -> Option<Board> {
    match crate::device::current().model {
        Some(crate::device::Model::Brick(b)) => Some(b),
        _ => None,
    }
}

/// The board, read below `root` (`/` on the device) when start.sh did not say (`POCKETUI_DEVICE`; NextUI's pak, the kit). By the
/// screen first (fb0's `virtual_size`: 1024 wide on the Brick, 1280 on the Smart Pro), then the name TrimUI's own MainUI carries
/// (`Trimui Brick`, `Trimui Smart Pro`). Nothing that says Smart Pro: the Brick, as before this existed, so a firmware that changes
/// a string can't turn a Brick into something else.
pub fn board_at(root: &Path) -> Board {
    let fb = std::fs::read_to_string(root.join("sys/class/graphics/fb0/virtual_size")).unwrap_or_default();
    board_from(&fb, || std::fs::read(root.join("usr/trimui/bin/MainUI")).unwrap_or_default())
}

/// `board_at` on what was read: fb0's `virtual_size` ("1280,2880") and, only when that says nothing, MainUI's bytes.
pub fn board_from(fb_virtual_size: &str, main_ui: impl FnOnce() -> Vec<u8>) -> Board {
    match fb_virtual_size.trim().split(',').next().and_then(|w| w.trim().parse::<u32>().ok()) {
        Some(w) if w == SMART_PRO_SCREEN.0 => return Board::SmartPro,
        Some(w) if w == SCREEN.0 => return Board::Brick,
        _ => {}
    }
    let bytes = main_ui();
    if bytes.windows(b"Trimui Smart Pro".len()).any(|w| w.eq_ignore_ascii_case(b"Trimui Smart Pro")) {
        Board::SmartPro
    } else {
        Board::Brick
    }
}

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
        assert_eq!(dev.model, Some(Model::Brick(Board::Brick)));
        assert_eq!((dev.screen(), dev.has_fan(), dev.has_touch(), dev.has_sticks(), dev.has_wifi()), ((1024, 768), false, false, false, false));
        assert_eq!(dev.ram_mb(), Some(1024));
        assert!(!dev.host_owned(), "the Brick is not the Deck: its windowing goes through pocket-fb");
    }

    #[test]
    fn the_override_names_it_and_settings_leave_out_what_nextui_owns() {
        assert_eq!(device::from_override("brick").model, Some(Model::Brick(Board::Brick)));
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

    /// The same card on a Smart Pro (docs/TRIMUI-SMART-PRO.md): found by fb0's width, then by MainUI's name; anything else stays the
    /// Brick it was before the Smart Pro existed, so no firmware string can turn a Brick into something it is not.
    #[test]
    fn the_smart_pro_is_told_from_the_brick_by_its_screen_then_mainui() {
        let never = || -> Vec<u8> { panic!("fb0 decided, MainUI is not read") };
        assert_eq!(board_from("1280,2880\n", never), Board::SmartPro);
        assert_eq!(board_from("1024,16384", never), Board::Brick);
        assert_eq!(board_from("", || b"\0Trimui Smart Pro\0".to_vec()), Board::SmartPro);
        assert_eq!(board_from("", || b"\0Trimui Brick\0".to_vec()), Board::Brick);
        assert_eq!(board_from("800,600", Vec::new), Board::Brick);
        let d = TestDir::new("smartpro-detect");
        std::fs::create_dir_all(&*d.join("usr/trimui/bin")).unwrap();
        std::fs::write(&*d.join("usr/trimui/bin/MainUI"), b"x").unwrap();
        std::fs::create_dir_all(&*d.join("mnt/SDCARD/.system")).unwrap();
        std::fs::create_dir_all(&*d.join("sys/class/graphics/fb0")).unwrap();
        std::fs::write(&*d.join("sys/class/graphics/fb0/virtual_size"), "1280,2880").unwrap();
        let dev = device::read(&*d);
        assert_eq!((dev.model, dev.reported.as_str()), (Some(Model::Brick(Board::SmartPro)), "TrimUI Smart Pro (NextUI)"));
    }

    /// What a Smart Pro owner meets: its own name and screen, sticks (Settings and the Tour have the stick pages, menus move with the
    /// left stick), no F keys or slider (317 and 318 are no F1 and F2 there), no lights page until the rings are mapped, the design's
    /// type sizes (its panel is coarser than the Brick's), and it is called experimental. The Brick keeps every answer it had.
    #[test]
    fn the_smart_pro_profile_differs_from_the_brick_only_where_the_hardware_does() {
        let sp = device::from_override("smartpro");
        assert_eq!(sp.model, Some(Model::Brick(Board::SmartPro)));
        assert_eq!((sp.screen(), sp.has_sticks(), sp.has_fan(), sp.has_touch(), sp.ram_mb()), ((1280, 720), true, false, false, Some(1024)));
        let m = Model::Brick(Board::SmartPro);
        assert_eq!((m.name(), m.experimental(), m.played_on()), ("TrimUI Smart Pro", true, false));
        let sp = Board::SmartPro;
        assert!(!sp.fkeys() && !sp.slider() && !sp.leds() && !sp.larger_small_type() && sp.sticks());
        let brick = device::from_override("brick");
        assert_eq!(brick.model, Some(Model::Brick(Board::Brick)));
        assert_eq!((brick.screen(), brick.has_sticks()), ((1024, 768), false));
        let b = Board::Brick;
        assert!(b.fkeys() && b.slider() && b.leds() && b.larger_small_type() && !b.sticks() && b.played_on());
        assert_eq!((Model::Brick(b).name(), Model::Brick(b).experimental()), ("TrimUI Brick", false));
        assert_eq!((Board::by_key("SmartPro"), Board::by_key("brick"), Board::by_key("smart pro")), (Some(Board::SmartPro), Some(Board::Brick), None));
    }

    /// A Smart Pro owner's report (Report a problem) carries what we could not read ourselves: its input devices and the card's
    /// boot.log (the board start.sh found, the GPU's answer), the last 60 lines. A Brick's report has none of it, as before.
    #[test]
    fn a_smart_pro_report_carries_its_input_devices_and_boot_log_and_a_bricks_does_not() {
        let d = TestDir::new("smartpro-report");
        std::fs::write(d.join("devices"), "N: Name=\"TRIMUI Player1\"\n").unwrap();
        std::fs::create_dir_all(d.join("card/logs")).unwrap();
        let boot: String = (0..100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(d.join("card/logs/boot.log"), boot + "board: smartpro (fb0 1280,1440)\n").unwrap();
        let s = report_section_for(Some(Board::SmartPro), &d.join("devices"), Some(&d.join("card")));
        assert!(s.contains("[input_devices]\nN: Name=\"TRIMUI Player1\"") && s.contains("[boot_log]\n") && s.contains("board: smartpro"), "{s}");
        assert!(!s.contains("line 40\n") && s.contains("line 41\n"), "only the end of boot.log: {s}");
        assert_eq!(report_section_for(Some(Board::Brick), &d.join("devices"), Some(&d.join("card"))), "");
        assert_eq!(report_section_for(None, &d.join("devices"), None), "");
        assert!(report_section_for(Some(Board::SmartPro), &d.join("none"), None).contains("[input_devices]\n(missing)"));
    }

    #[test]
    fn only_its_own_pad_crosses_x_and_y() {
        assert!(is_pad("TRIMUI Player1") && is_pad(" TRIMUI Player1 "));
        assert!(!is_pad("Xbox Wireless Controller") && !is_pad("sunxi-keyboard"));
    }
}
