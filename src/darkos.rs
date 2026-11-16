//! pocket-ui as a port on dArkOS (prototype, docs/DARKOS.md): the `brick` build (pocket-fb on `/dev/fb0`, the CPU copy of a
//! game's picture) started from dArkOS's Ports by `deploy/darkos/start.sh`, which sets `POCKETUI_HOST=darkos` and
//! `POCKETUI_DEVICE=darkos-<board>`. dArkOS (christianhaitian/dArkOS, Debian 13) stays as it is: nothing is installed outside
//! the port's folder, and leaving pocket-ui brings EmulationStation back.
//!
//! The boards are the H700 profile's rows (1 GB is less than these have, no fan, no touch), read from dArkOS's build
//! (`~ark/.config/.DEVICE`, which start.sh reads first, and the kernels' device trees: `rk3566-353m.dts`,
//! `rk3326-rg351mp-linux.dts` and the others). Only the 640x480 ones run; the others are listed with their screens and
//! `not_yet()`, and start.sh names them when it refuses.
//!
//! **Keys.** ogage keeps volume and brightness. The UI claims only a separate power-only evdev node with
//! EVIOCGRAB; short power saves and suspends, holding it opens the power menu. Exit/crash closes the fd and restores
//! host event delivery without changing ogage, logind or any host file (docs/DARKOS.md).
use crate::device_model::H700Board;

/// `POCKETUI_HOST`'s value that start.sh sets.
pub const HOST: &str = "darkos";

const fn board(key: &'static str, name: &'static str, screen: (u32, u32), sticks: bool) -> H700Board {
    // Wi-Fi stays dArkOS's (its own NetworkManager): pocket-ui offers no Wi-Fi page there. No `compatible`: the boards
    // are found by start.sh (several Powkiddy trees say "anbernic,rg353m"), never by `device::identify`.
    H700Board { key, name, compatible: &[], screen, sticks, wifi: false, lid: false }
}

/// A screen the test version has no layout check for (not 640x480): start.sh refuses these boards and names them.
pub const fn not_yet(b: &H700Board) -> bool { b.screen.0 != 640 || b.screen.1 != 480 }

/// Every handheld of dArkOS's build (its Makefile's targets, v08272026), after `darkos-` in `POCKETUI_DEVICE`; the screens
/// are the device trees' panel timings. `not_yet()`: a screen the test version has no layout check for.
pub const BOARDS: &[H700Board] = &[
    // RK3566, kernel 5.10, pad `retrogame_joypad`.
    board("darkos-rg353m", "Anbernic RG353M", (640, 480), true),
    board("darkos-rg353v", "Anbernic RG353V/VS", (640, 480), true),
    board("darkos-rk2023", "Powkiddy RK2023", (640, 480), true),
    board("darkos-rg503", "Anbernic RG503", (960, 544), true),
    board("darkos-rgb30", "Powkiddy RGB30", (720, 720), true),
    board("darkos-rgb20pro", "Powkiddy RGB20 Pro", (1024, 768), true),
    // The tree's panel is 720x960, turned: not a landscape screen of ours.
    board("darkos-miniloong", "MiniLoong Pocket 1", (720, 960), true),
    // RK3326, kernel 4.4, pad `GO-Super Gamepad` (`pad_button`). Their trees call themselves "Hardkernel ODROID-GO3".
    board("darkos-rg351mp", "Anbernic RG351MP", (640, 480), true),
    board("darkos-g350", "BatleXP G350", (640, 480), true),
    board("darkos-a10mini", "A10 Mini", (640, 480), true),
    // The R36S family (R36S, R36XX, R36H, clones) on a community build of dArkOS (southoz's dArkOSRE-R36): the same RK3326,
    // the same `GO-Super Gamepad`. start.sh finds it by the device tree's `model`, which these builds set per board and panel.
    board("darkos-r36s", "R36S", (640, 480), true),
    // The ODROID-GO Advance's 320x480 panel, turned by fbcon.
    board("darkos-rgb10", "Powkiddy RGB10", (320, 480), false),
    // Any other dArkOS handheld with a 640x480 screen (start.sh checks the size): a clone, a board not listed yet.
    board("darkos-generic", "dArkOS handheld", (640, 480), true),
];

/// dArkOS has a separate signed port-update channel; its Settings pages remain available.
pub fn hides(_key: &str) -> bool { false }

/// Started from dArkOS's Ports (start.sh).
pub fn host() -> bool {
    std::env::var("POCKETUI_HOST").is_ok_and(|h| h.trim() == HOST)
}

/// The board for `POCKETUI_DEVICE`'s value.
pub fn board_by_key(key: &str) -> Option<&'static H700Board> {
    BOARDS.iter().find(|b| b.key == key.trim().to_ascii_lowercase())
}

/// dArkOS's hotkey daemon runs (`/proc/<pid>/comm`; `root` is "/" except in tests): it owns Vol+/Vol- then.
pub fn ogage_runs(root: &std::path::Path) -> bool {
    crate::fn_keys::process_runs(root, "ogage")
}

/// The RK3326 pad of dArkOS's ODROID-GO3 trees (RG351MP, G350, A10 Mini, the R36S family), `GO-Super Gamepad`: Select, Start
/// and the stick presses are BTN_TRIGGER_HAPPY1 to 4 (704 to 707), which no other pad of ours sends. The order is the one
/// dArkOS itself uses (`es_input.cfg.rg351mp`: button 12 Select, 13 Start, 14 left thumb, 15 right thumb and hotkey;
/// RetroArch's `GO-Super Gamepad.cfg` the same; ogage's rg351mp branch: hotkey BTN_TRIGGER_HAPPY4). So R3 is HOME here, as it
/// is dArkOS's hotkey. 708 (F5 in the trees) is the R36S family's FN/Menu button: dArkOSRE-R36's RetroArch config gives it
/// `input_menu_toggle_btn = "16"` (the 17th button, 708), so it is HOME too; on a pad without that button it never arrives.
/// 709 (F6) stays unmapped. NOT checked on a device.
pub fn pad_button(name: &str, code: u16) -> Option<crate::pad::Button> {
    use crate::pad::Button;
    if name.trim() != "GO-Super Gamepad" {
        return None;
    }
    match code {
        704 => Some(Button::Select),
        705 => Some(Button::Start),
        706 => Some(Button::L3),
        707 | 708 => Some(Button::Mode),
        _ => None,
    }
}

/// The leave dialog's title on dArkOS: SELECT + START held, Settings → About's EmulationStation row.
pub const LEAVE_TITLE: &str = "Back to EmulationStation?";
/// Its one paragraph.
pub const LEAVE_BODY: &str = "pocket-ui closes and EmulationStation comes back. Your games, saves and settings stay as they are.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_darkos_board_is_found_by_its_key_and_only_640x480_landscape_ones_run() {
        for b in BOARDS {
            assert!(b.key.starts_with("darkos-") && !b.wifi, "{}", b.key);
            assert_eq!(not_yet(b), b.screen != (640, 480), "{}", b.key);
            assert_eq!(board_by_key(&b.key.to_ascii_uppercase()), Some(b));
        }
        assert_eq!(board_by_key("rg35xx-plus"), None, "the H700 rows stay ROCKNIX's");
        let d = crate::device::from_override("darkos-rg353m");
        assert_eq!((d.screen(), d.model.map(|m| m.name())), ((640, 480), Some("Anbernic RG353M")));
        // An H700 row: the H700's GBA rule (gpSP by default, `h700_gba`) applies, with dArkOS's gpSP (start.sh).
        assert!(d.model.is_some_and(|m| m.is_h700()));
        let d = crate::device::from_override("darkos-rgb30");
        assert_eq!(d.screen(), (720, 720));
    }

    /// start.sh decides the board before pocket-ui runs; its table must name exactly these rows, and refuse the ones
    /// that are `not_yet`.
    #[test]
    fn start_sh_knows_every_board_and_refuses_the_not_yet_screens() {
        let sh = include_str!("../deploy/darkos/start.sh");
        for b in BOARDS.iter().filter(|b| b.key != "darkos-generic") {
            let line = sh.lines().find(|l| l.contains(&format!("BOARD={}", b.key))).unwrap_or_else(|| panic!("start.sh has no {}", b.key));
            assert_eq!(line.contains("REFUSE="), not_yet(b), "{line}");
            assert!(line.contains(&format!("{}x{}", b.screen.0, b.screen.1)), "{line}: {:?}", b.screen);
        }
        assert!(sh.contains("BOARD=darkos-generic"));
    }

    #[test]
    fn the_rk3326_pad_gives_select_start_l3_and_home_as_dark_os_does_and_the_r36s_fn_button_is_home_too() {
        use crate::pad::Button;
        let got: Vec<_> = (704..=709).map(|c| pad_button("GO-Super Gamepad", c)).collect();
        assert_eq!(got, [Some(Button::Select), Some(Button::Start), Some(Button::L3), Some(Button::Mode), Some(Button::Mode), None]);
        assert_eq!(pad_button("retrogame_joypad", 704), None, "the RK3566 pad sends the usual codes");
    }

    #[test]
    fn a_port_on_dark_os_uses_the_signed_port_updates_page() {
        // The signed port channel makes Updates available without replacing dArkOS.
        assert!(!hides("updates") && !hides("kids"));
        let state = crate::update_hint::State::default();
        assert!(crate::update_hint::looks_on_for_tests(true, &state, false));
        assert!(crate::update_hint::looks_on_for_tests(true, &state, true), "dArkOS uses its own update channel");
        assert!(!crate::update_hint::looks_on_for_tests(false, &state, false), "no look from a development build, as before");
    }

    #[test]
    fn ogage_is_seen_by_its_process_name() {
        let root = crate::testutil::TestDir::new("darkos-ogage");
        std::fs::create_dir_all(root.join("proc/41")).unwrap();
        std::fs::write(root.join("proc/41/comm"), "emulationstatio\n").unwrap();
        assert!(!ogage_runs(&root));
        std::fs::create_dir_all(root.join("proc/77")).unwrap();
        std::fs::write(root.join("proc/77/comm"), "ogage\n").unwrap();
        assert!(ogage_runs(&root));
    }
}
