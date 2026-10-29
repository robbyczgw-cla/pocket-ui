//! SM8550 Pocket 6 (including TOP-DPAD), Nova and the AYN Odin 2 family are experimental too: see docs/DEVICES.md for their
//! screen-off-only power path and ROCKNIX-owned fan. `POCKETUI_DEVICE=rp6|nova|odin2mini|odin2|odin2portal` selects them.
//!
//! Which handheld pocket-ui runs on (docs/DEVICES.md): the one place that tells the Retroid Pocket Mini V2 (the
//! device pocket-ui is made and tested on) from its SM8250 siblings that run the same ROCKNIX image: the Retroid
//! Pocket 5, the Retroid Pocket Flip 2 and the first Retroid Pocket Mini. Those are **experimental**: nobody on the
//! project has one, so the About page says so and first start says it once.
//!
//! **Where it comes from.** The device tree: `/sys/firmware/devicetree/base/compatible` (NUL-separated, the board's
//! own entry first) and `model`, the name ROCKNIX's quirks are keyed by (`QUIRK_DEVICE` in `rocknix-info` is the
//! model with "/" made "-", and `quirks/devices/<model>/` is the folder). The strings, from ROCKNIX's
//! `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/sm8250-retroidpocket-*.dts` (`next`, 2026-10-26):
//!
//! | Device | `compatible` (first) | `model` |
//! |---|---|---|
//! | Mini V2 | `retroidpocket,rpocketv2` | `Retroid Pocket Mini V2` |
//! | Mini (V1) | `retroidpocket,rpocket` | `Retroid Pocket Mini` |
//! | Pocket 5 | `retroidpocket,rp5` | `Retroid Pocket 5`, `Retroid Pocket 5 Visionox` |
//! | Flip 2 | `retroidpocket,rpflip2` | `Retroid Pocket Flip2`, `Retroid Pocket Flip2 Visionox` |
//!
//! The Visionox variants keep their board's `compatible` and only differ in `model` (and the panel). `HW_DEVICE`
//! in os-release is the platform (`SM8250`) for all four, so it tells nothing apart.
//!
//! **Anything else** (another SM8250 handheld on the same image, a computer, the virtual device): `Device::model`
//! is None, and every caller behaves exactly as before this module existed (the Mini V2's numbers), with nothing
//! said about the device.
//!
//! **H700** (Anbernic RG28XX, RG35XX SP; ROCKNIX's H700 image, `anbernic,rg28xx` / `anbernic,rg35xx-sp`): experimental
//! like the others, 640x480, no fan, no touch, no sticks, 1 GB; the SP has a lid, the RG28XX no Wi-Fi (docs/DEVICES.md).
//!
//! `POCKETUI_DEVICE` stands in for the device tree (snapshots, tests, the virtual device): a short name (`miniv2`,
//! `mini`, `rp5`, `rp5-visionox`, `flip2`, `flip2-visionox`) or a `model` string.

use std::path::Path;
use std::sync::OnceLock;

/// The handhelds pocket-ui knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    /// Retroid Pocket Mini V2: 1240x1080, the device pocket-ui is tested on.
    MiniV2,
    /// The first Retroid Pocket Mini: 1280x960 (4:3; the panel scales internally, see docs/DEVICES.md).
    MiniV1,
    /// Retroid Pocket 5: 1920x1080.
    Pocket5,
    /// Retroid Pocket Flip 2 (the SD865 one): the Pocket 5's board and screen in a clamshell, with a lid switch.
    Flip2,
    /// SM8550: portrait panel, screen-off only, ROCKNIX controls the fan.
    Pocket6,
    /// SM8550: landscape-native 1280x960, screen-off only.
    Nova,
    /// SM8550: 1080p, 90-degree portrait panel, Nintendo labels.
    Odin2,
    /// SM8550: 1080p Mini LED, 90-degree portrait panel, Xbox labels.
    Odin2Mini,
    /// SM8550: 1080p OLED, 270-degree portrait panel, Xbox labels.
    Odin2Portal,
    /// An Allwinner H700 handheld of ROCKNIX's H700 image: one profile (1 GB, no fan, no touch), the quirks of each
    /// model are one row of `H700_BOARDS`.
    H700(&'static H700Board),
}

/// What differs between the Anbernic H700 models (`H700_BOARDS`, docs/DEVICES.md has the same table). The
/// device tree's `model` and `compatible` are ROCKNIX's (`sun50i-h700-anbernic-*.dts`); sizes are what sway gives
/// pocket-ui (landscape, after the turn), from the panel files ROCKNIX loads.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct H700Board {
    /// `POCKETUI_DEVICE` and the vdev's name.
    pub key: &'static str,
    /// The name people know it by; also the device tree's `model`.
    pub name: &'static str,
    /// `compatible` entries that name this board (compared in lower case; the panel variants' device trees keep them).
    pub compatible: &'static [&'static str],
    pub screen: (u32, u32),
    /// Analog sticks (ROCKNIX's `amux-count`; the stick lights page needs them).
    pub sticks: bool,
    /// A Wi-Fi chip by the maker's specification. A chip that answers wins either way (`system_ctl::AutoHw`).
    pub wifi: bool,
    /// A lid switch (`gpio-keys-lid`, the clamshells).
    pub lid: bool,
    /// pocket-ui's frame cost is too high on this panel for now (the 720x720 CubeXX): the installers refuse it.
    pub not_yet: bool,
}

const fn board(key: &'static str, name: &'static str, compatible: &'static [&'static str], screen: (u32, u32), sticks: bool, wifi: bool, lid: bool) -> H700Board {
    H700Board { key, name, compatible, screen, sticks, wifi, lid, not_yet: false }
}

/// Every H700 model in ROCKNIX's image (`projects/ROCKNIX/config.xml`, H700). The two panel revisions of a model
/// share a row. Experimental, all of them; the RG28XX has no Wi-Fi (owner), the RG35XX 2024 none either (maker).
/// Sticks as ROCKNIX's device trees have them: the RG34XX's has none (`amux-count` 0 from the Plus tree).
pub const H700_BOARDS: &[H700Board] = &[
    board("rg28xx", "Anbernic RG28XX", &["anbernic,rg28xx"], (640, 480), false, false, false),
    board("rg35xx-2024", "Anbernic RG35XX 2024", &["anbernic,rg35xx-2024"], (640, 480), false, false, false),
    board("rg35xx-plus", "Anbernic RG35XX Plus", &["anbernic,rg35xx-plus"], (640, 480), false, true, false),
    board("rg35xx-h", "Anbernic RG35XX H", &["anbernic,rg35xx-h"], (640, 480), true, true, false),
    board("rg35xx-pro", "Anbernic RG35XX Pro", &["anbernic,rg35xx-pro"], (640, 480), true, true, false),
    // The second panel revision's device tree (`...-rg35xx-sp-v2-panel.dtb`) names the same board.
    board("rg35xx-sp", "Anbernic RG35XX SP", &["anbernic,rg35xx-sp", "anbernic,rg35xx-sp-v2", "anbernic,rg35xx-sp-v2-panel"], (640, 480), false, true, true),
    board("rg40xx-h", "Anbernic RG40XX H", &["anbernic,rg40xx-h"], (640, 480), true, true, false),
    board("rg40xx-v", "Anbernic RG40XX V", &["anbernic,rg40xx-v"], (640, 480), true, true, false),
    board("rg34xx", "Anbernic RG34XX", &["anbernic,rg34xx"], (720, 480), false, true, false),
    board("rg34xx-sp", "Anbernic RG34XX-SP", &["anbernic,rg34xx-sp"], (720, 480), true, true, true),
    board("rg-sp", "Anbernic RG-SP", &["anbernic,rg-sp"], (720, 480), false, true, true),
    H700Board { not_yet: true, ..board("rgcubexx", "Anbernic RG CubeXX", &["anbernic,rgcubexx"], (720, 720), true, true, false) },
];

/// The H700 model with this `compatible` entry or device tree `model`, if the profile table has it.
pub fn h700_board(model: &str, compatible: &[&str]) -> Option<&'static H700Board> {
    let by_compatible = compatible.iter().find_map(|c| {
        let c = c.trim().to_ascii_lowercase();
        H700_BOARDS.iter().find(|b| b.compatible.contains(&c.as_str()))
    });
    by_compatible.or_else(|| H700_BOARDS.iter().find(|b| b.name.eq_ignore_ascii_case(model)))
}

/// Which panel the Pocket 5 or Flip 2 has (ROCKNIX's "Visionox" device trees): the touch axes differ, which the
/// kernel's touch driver handles (`touchscreen-inverted-*` in the device tree), so it is only shown and logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Original,
    Visionox,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// None: an unknown handheld (the generic fallback).
    pub model: Option<Model>,
    pub panel: Panel,
    /// The device tree's `model` as read (empty when there is none), for the log and the report.
    pub reported: String,
}

/// The Mini V2's screen, landscape: every fallback size before this module was this.
pub const MINI_V2_SCREEN: (u32, u32) = (1240, 1080);

impl Model {
    /// The name people know it by, for the About page and first start (stays English in every language: a name).
    pub fn name(self) -> &'static str {
        match self {
            Model::MiniV2 => "Retroid Pocket Mini V2",
            Model::MiniV1 => "Retroid Pocket Mini V1",
            Model::Pocket5 => "Retroid Pocket 5",
            Model::Flip2 => "Retroid Pocket Flip 2",
            Model::Pocket6 => "Retroid Pocket 6",
            Model::Nova => "Retroid Pocket Nova",
            Model::Odin2 => "AYN Odin 2",
            Model::Odin2Mini => "AYN Odin 2 Mini",
            Model::Odin2Portal => "AYN Odin 2 Portal",
            Model::H700(b) => b.name,
        }
    }

    /// The screen as pocket-ui sees it (landscape, after sway's turn): the panel's portrait size the other way round.
    pub fn screen(self) -> (u32, u32) {
        match self {
            Model::MiniV2 => MINI_V2_SCREEN,
            Model::MiniV1 | Model::Nova => (1280, 960),
            Model::H700(b) => b.screen,
            Model::Pocket5 | Model::Flip2 | Model::Pocket6 | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal => (1920, 1080),
        }
    }

    /// Not tested on real hardware by the project (everything but the Mini V2).
    pub fn experimental(self) -> bool {
        self != Model::MiniV2
    }

    /// The Flip 2's hall sensor: `SW_LID` on a `gpio-keys` device (docs/SLEEP.md "Lid").
    pub fn has_lid(self) -> bool {
        match self {
            Model::Flip2 => true,
            Model::H700(b) => b.lid,
            _ => false,
        }
    }

    /// The Allwinner H700 handhelds (ROCKNIX's H700 image): 1 GB, no fan, no touch; sticks and Wi-Fi per model.
    pub fn is_h700(self) -> bool {
        matches!(self, Model::H700(_))
    }

    /// The short names `POCKETUI_DEVICE` takes.
    fn key(self) -> &'static str {
        match self {
            Model::MiniV2 => "miniv2",
            Model::MiniV1 => "mini",
            Model::Pocket5 => "rp5",
            Model::Flip2 => "flip2",
            Model::Pocket6 => "rp6",
            Model::Nova => "nova",
            Model::Odin2 => "odin2",
            Model::Odin2Mini => "odin2mini",
            Model::Odin2Portal => "odin2portal",
            Model::H700(b) => b.key,
        }
    }
}

impl Device {
    /// ROCKNIX SM8550 disables suspend and owns its userspace fan loop.
    pub fn screen_off_only(&self) -> bool {
        self.is_sm8550()
    }

    /// The Snapdragon 8 Gen 2 handhelds (Adreno 740).
    pub fn is_sm8550(&self) -> bool {
        matches!(self.model, Some(Model::Pocket6 | Model::Nova | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal))
    }

    /// A fan the system can run. None on the H700 handhelds: the fan page says so instead of offering a fan.
    pub fn has_fan(&self) -> bool {
        !self.model.is_some_and(Model::is_h700)
    }

    /// A touch screen. None on the H700 handhelds (the kernel has no touch driver there).
    pub fn has_touch(&self) -> bool {
        !self.model.is_some_and(Model::is_h700)
    }

    /// Analog sticks, and so the lights around them. Per model on the H700 handhelds.
    pub fn has_sticks(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b)) if !b.sticks)
    }

    /// A Wi-Fi chip. Not on the RG28XX and RG35XX 2024 (their device trees enable one anyway, the Plus tree they
    /// come from, so a chip that does answer still wins: `system_ctl::AutoHw` only calls it absent when none shows up).
    pub fn has_wifi(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b)) if !b.wifi)
    }

    /// Memory in MB where the device is known to have less than the Mini V2's (1 GB on the H700, no swap).
    pub fn ram_mb(&self) -> Option<u32> {
        self.model.filter(|m| m.is_h700()).map(|_| 1024)
    }

    /// Printed face letters on AYN's product images; saved menu choices still win.
    pub fn menu_layout(&self) -> crate::button_layout::ButtonLayout {
        use crate::button_layout::ButtonLayout;
        match self.model {
            Some(Model::Odin2Mini | Model::Odin2Portal) => ButtonLayout::Xbox,
            _ => ButtonLayout::Nintendo,
        }
    }

    /// Touch ranges if evdev has none. The kernel already applies DT axis inversions.
    pub fn panel_size(&self) -> (u32, u32) {
        match self.model {
            Some(Model::Nova) => (1280, 960),
            _ => {
                let (w, h) = self.screen();
                (h, w)
            }
        }
    }

    /// An unknown handheld.
    pub fn unknown(reported: &str) -> Device {
        Device { model: None, panel: Panel::Original, reported: reported.to_string() }
    }

    /// Experimental: one of the Mini V2's siblings (an unknown device is not called anything).
    pub fn experimental(&self) -> bool {
        self.model.is_some_and(Model::experimental)
    }

    /// The screen's landscape size where no window says it (fallbacks, previews): the Mini V2's when unknown.
    pub fn screen(&self) -> (u32, u32) {
        self.model.map_or(MINI_V2_SCREEN, Model::screen)
    }

    pub fn has_lid(&self) -> bool {
        self.model.is_some_and(Model::has_lid)
    }

    /// Settings › System › About's Device row: "Retroid Pocket 5 (experimental)". None on the Mini V2 and on an
    /// unknown device, whose About page stays as it was.
    pub fn about_line(&self) -> Option<String> {
        self.model.filter(|m| m.experimental()).map(|m| format!("{} (experimental)", m.name()))
    }

    /// The lowest running PWM Smooth fan starts from (`fan_curve`, docs/FAN-CURVE.md): the Mini V2's measured 40
    /// there and on an unknown device (today's behaviour); on the experimental ones the kernel's own lowest cooling
    /// level, 51, which ROCKNIX runs every one of these fans at: the same fan driver and cooling device tree, but a
    /// different fan in a different case, never measured below that.
    pub fn fan_min_run(&self) -> u8 {
        if self.experimental() {
            crate::fan_curve::START_PWM
        } else {
            crate::fan_curve::MIN_RUN_PWM
        }
    }

    /// One line for the log at start.
    pub fn log_line(&self) -> String {
        let reported = if self.reported.is_empty() { "no device tree model".to_string() } else { format!("\"{}\"", self.reported) };
        match self.model {
            Some(m) if m.experimental() => format!("device: {} ({reported}), experimental", m.name()),
            Some(m) => format!("device: {} ({reported})", m.name()),
            None => format!("device: unknown ({reported}), generic"),
        }
    }
}

/// The device from the device tree's `model` and `compatible` (`compatible` as read: NUL-separated). The board's
/// `compatible` decides; `model` alone when there is no `compatible`.
pub fn identify(model: &str, compatible: &[u8]) -> Device {
    let model = model.trim_end_matches('\0').trim();
    let by_compatible = compatible
        .split(|b| *b == 0)
        .filter_map(|c| std::str::from_utf8(c).ok())
        .find_map(|c| match c.trim() {
            "retroidpocket,rpocketv2" => Some(Model::MiniV2),
            "retroidpocket,rpocket" => Some(Model::MiniV1),
            "retroidpocket,rp5" => Some(Model::Pocket5),
            "retroidpocket,rpflip2" => Some(Model::Flip2),
            "retroidpocket,rp6" => Some(Model::Pocket6),
            "retroidpocket,rpnova" => Some(Model::Nova),
            "ayn,odin2" => Some(Model::Odin2),
            "ayn,odin2mini" => Some(Model::Odin2Mini),
            "ayn,odin2portal" => Some(Model::Odin2Portal),
            _ => None,
        })
        .or_else(|| {
            let names: Vec<&str> = compatible.split(|b| *b == 0).filter_map(|c| std::str::from_utf8(c).ok()).collect();
            h700_board("", &names).map(Model::H700)
        });
    let found = by_compatible.or(match model.strip_suffix(" Visionox").unwrap_or(model) {
        "Retroid Pocket Mini V2" => Some(Model::MiniV2),
        "Retroid Pocket Mini" => Some(Model::MiniV1),
        "Retroid Pocket 5" => Some(Model::Pocket5),
        "Retroid Pocket Flip2" => Some(Model::Flip2),
        "Retroid Pocket 6" | "Retroid Pocket 6 TOP-DPAD" => Some(Model::Pocket6),
        "Retroid Pocket Nova" => Some(Model::Nova),
        "AYN Odin 2" => Some(Model::Odin2),
        "AYN Odin 2 Mini" => Some(Model::Odin2Mini),
        "AYN Odin 2 Portal" => Some(Model::Odin2Portal),
        other => h700_board(other, &[]).map(Model::H700),
    });
    let panel = if model.ends_with(" Visionox") { Panel::Visionox } else { Panel::Original };
    Device { model: found, panel, reported: model.to_string() }
}

/// `POCKETUI_DEVICE`'s value: a short name or a `model` string.
pub fn from_override(v: &str) -> Device {
    let v = v.trim();
    let lower = v.to_ascii_lowercase();
    let (key, panel) = match lower.strip_suffix("-visionox") {
        Some(k) => (k, Panel::Visionox),
        None => (lower.as_str(), Panel::Original),
    };
    match [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova, Model::Odin2, Model::Odin2Mini, Model::Odin2Portal]
        .into_iter()
        .chain(H700_BOARDS.iter().map(Model::H700))
        .find(|m| m.key() == key)
    {
        Some(m) => Device {
            model: Some(m),
            panel,
            reported: format!("{v} (POCKETUI_DEVICE)"),
        },
        None => identify(v, b""),
    }
}

/// The device tree below `root` (`/` on the device).
pub fn read(root: &Path) -> Device {
    let base = root.join("sys/firmware/devicetree/base");
    let model = std::fs::read_to_string(base.join("model")).unwrap_or_default();
    let compatible = std::fs::read(base.join("compatible")).unwrap_or_default();
    identify(&model, &compatible)
}

/// Cooling safety also recognises an otherwise unknown SM8550 board; do not inherit SM8250 cooling there.
pub fn rocknix_cooling(root: &Path) -> bool {
    (root == Path::new("/") && current().screen_off_only())
        || std::fs::read(root.join("sys/firmware/devicetree/base/compatible")).is_ok_and(|b| b.split(|v| *v == 0).any(|c| c == b"qcom,sm8550"))
        || std::fs::read_to_string(root.join("etc/os-release")).is_ok_and(|s| s.lines().any(|l| l.starts_with("HW_DEVICE=") && l.contains("SM8550")))
}

static DEVICE: OnceLock<Device> = OnceLock::new();

/// This device, read once (two small sysfs files).
pub fn current() -> &'static Device {
    DEVICE.get_or_init(|| match std::env::var("POCKETUI_DEVICE") {
        Ok(v) if !v.trim().is_empty() => from_override(&v),
        _ => read(Path::new("/")),
    })
}

/// `--snapshot <state>:device=<name>`: the state without its device, and the device (`from_override`'s names), so the
/// snapshot suite paints what an experimental device shows (tests/i18n-states.txt).
pub fn snapshot_state(state: &str) -> (&str, Option<Device>) {
    match state.split_once(":device=") {
        Some((rest, name)) => (rest, Some(from_override(name))),
        None => (state, None),
    }
}

/// Settles the device before anything has read it (`--snapshot` with a `:device=`). False when it was read already.
pub fn force(d: Device) -> bool {
    DEVICE.set(d).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// The strings exactly as ROCKNIX's device trees give them (`model` with its NUL, `compatible` NUL-separated).
    #[test]
    fn each_retroid_is_told_by_its_device_tree() {
        let cases: [(&str, &[u8], Model, Panel); 9] = [
            (
                "Retroid Pocket 6\0",
                b"retroidpocket,rp6\0qcom,qcs8550\0qcom,sm8550\0",
                Model::Pocket6,
                Panel::Original,
            ),
            (
                "Retroid Pocket 6 TOP-DPAD\0",
                b"retroidpocket,rp6\0qcom,qcs8550\0qcom,sm8550\0",
                Model::Pocket6,
                Panel::Original,
            ),
            (
                "Retroid Pocket Nova\0",
                b"retroidpocket,rpnova\0qcom,qcs8550\0qcom,sm8550\0",
                Model::Nova,
                Panel::Original,
            ),
            (
                "Retroid Pocket Mini V2\0",
                b"retroidpocket,rpocketv2\0qcom,sm8250\0",
                Model::MiniV2,
                Panel::Original,
            ),
            ("Retroid Pocket Mini\0", b"retroidpocket,rpocket\0qcom,sm8250\0", Model::MiniV1, Panel::Original),
            ("Retroid Pocket 5\0", b"retroidpocket,rp5\0qcom,sm8250\0", Model::Pocket5, Panel::Original),
            ("Retroid Pocket 5 Visionox\0", b"retroidpocket,rp5\0qcom,sm8250\0", Model::Pocket5, Panel::Visionox),
            ("Retroid Pocket Flip2\0", b"retroidpocket,rpflip2\0qcom,sm8250\0", Model::Flip2, Panel::Original),
            ("Retroid Pocket Flip2 Visionox\0", b"retroidpocket,rpflip2\0qcom,sm8250\0", Model::Flip2, Panel::Visionox),
        ];
        for (model, compatible, want, panel) in cases {
            let d = identify(model, compatible);
            assert_eq!((d.model, d.panel), (Some(want), panel), "{model}");
            assert_eq!(d.reported, model.trim_end_matches('\0'));
            // The model alone (no compatible file) says the same.
            assert_eq!(identify(model, b"").model, Some(want), "{model} without compatible");
        }
    }

    #[test]
    fn ayn_device_trees_and_printed_layouts() {
        use crate::button_layout::ButtonLayout::{Nintendo, Xbox};
        for (model, compatible, want, layout) in [
            ("AYN Odin 2\0", &b"ayn,odin2\0qcom,qcs8550\0qcom,sm8550\0"[..], Model::Odin2, Nintendo),
            ("AYN Odin 2 Mini\0", &b"ayn,odin2mini\0qcom,qcs8550\0qcom,sm8550\0"[..], Model::Odin2Mini, Xbox),
            ("AYN Odin 2 Portal\0", &b"ayn,odin2portal\0qcom,qcs8550\0qcom,sm8550\0"[..], Model::Odin2Portal, Xbox),
        ] {
            let d = identify(model, compatible);
            assert_eq!(d.model, Some(want));
            assert_eq!(identify(model, b"").model, Some(want));
            assert_eq!(d.menu_layout(), layout);
            assert!(!want.has_lid());
        }
    }

    #[test]
    fn only_the_mini_v2_is_tested_and_only_the_flip_2_has_a_lid() {
        assert!(!Model::MiniV2.experimental());
        assert!(Model::MiniV1.experimental() && Model::Pocket5.experimental() && Model::Flip2.experimental());
        assert!(Model::Flip2.has_lid());
        assert!(!Model::MiniV2.has_lid() && !Model::MiniV1.has_lid() && !Model::Pocket5.has_lid());
        assert_eq!(Model::Pocket5.screen(), (1920, 1080));
        assert_eq!(Model::Flip2.screen(), (1920, 1080));
        assert_eq!(Model::MiniV1.screen(), (1280, 960));
        assert_eq!(Model::MiniV2.screen(), (1240, 1080));
    }

    #[test]
    fn another_device_is_unknown_and_keeps_the_mini_v2s_numbers() {
        for (model, compatible) in [
            ("AYN Thor Lite\0", &b"ayn,thorlite\0qcom,sm8250\0"[..]),
            ("MANGMI Pocket Max\0", b"mangmi,pocketmax\0qcom,sm8250\0"),
            ("AYN Thor\0", b"ayn,thor\0qcom,qcs8550\0"),
            ("", b""),
        ] {
            let d = identify(model, compatible);
            assert_eq!(d.model, None, "{model}");
            assert!(!d.experimental() && !d.has_lid());
            assert_eq!(d.screen(), MINI_V2_SCREEN);
            assert_eq!(d.about_line(), None);
            assert_eq!(d.fan_min_run(), crate::fan_curve::MIN_RUN_PWM);
        }
        // A computer has no device tree at all.
        let d = read(Path::new("/nonexistent-pocketui-root"));
        assert_eq!((d.model, d.reported.as_str()), (None, ""));
        assert!(d.log_line().contains("unknown"));
    }

    #[test]
    fn the_compatible_decides_over_the_model() {
        let d = identify("Retroid Pocket 5\0", b"retroidpocket,rpflip2\0qcom,sm8250\0");
        assert_eq!(d.model, Some(Model::Flip2));
    }

    #[test]
    fn the_device_tree_is_read_below_a_root() {
        let root = TestDir::new("device");
        let base = root.join("sys/firmware/devicetree/base");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("model"), "Retroid Pocket Flip2 Visionox\0").unwrap();
        std::fs::write(base.join("compatible"), b"retroidpocket,rpflip2\0qcom,sm8250\0").unwrap();
        let d = read(&root);
        assert_eq!((d.model, d.panel), (Some(Model::Flip2), Panel::Visionox));
        assert!(d.has_lid());
    }

    #[test]
    fn about_and_the_fan_say_what_an_experimental_device_gets() {
        let rp5 = identify("Retroid Pocket 5\0", b"retroidpocket,rp5\0");
        assert_eq!(rp5.about_line().as_deref(), Some("Retroid Pocket 5 (experimental)"));
        assert_eq!(rp5.fan_min_run(), crate::fan_curve::START_PWM);
        assert!(rp5.log_line().ends_with("experimental"), "{}", rp5.log_line());
        let v2 = identify("Retroid Pocket Mini V2\0", b"retroidpocket,rpocketv2\0");
        assert_eq!(v2.about_line(), None, "the Mini V2's About page stays as it was");
        assert_eq!(v2.fan_min_run(), crate::fan_curve::MIN_RUN_PWM);
        assert_eq!(identify("Retroid Pocket Mini\0", b"retroidpocket,rpocket\0").about_line().as_deref(), Some("Retroid Pocket Mini V1 (experimental)"));
        assert_eq!(identify("Retroid Pocket Flip2\0", b"retroidpocket,rpflip2\0").about_line().as_deref(), Some("Retroid Pocket Flip 2 (experimental)"));
    }

    #[test]
    fn sm8550_profiles_are_experimental_with_safe_capabilities_and_geometry() {
        for (key, model, screen, panel) in [
            ("rp6", Model::Pocket6, (1920, 1080), (1080, 1920)),
            ("nova", Model::Nova, (1280, 960), (1280, 960)),
            ("odin2", Model::Odin2, (1920, 1080), (1080, 1920)),
            ("odin2mini", Model::Odin2Mini, (1920, 1080), (1080, 1920)),
            ("odin2portal", Model::Odin2Portal, (1920, 1080), (1080, 1920)),
        ] {
            let d = from_override(key);
            assert_eq!(d.model, Some(model));
            assert!(d.experimental() && d.screen_off_only());
            assert_eq!(d.screen(), screen);
            assert_eq!(d.panel_size(), panel);
            assert_eq!(d.about_line(), Some(format!("{} (experimental)", model.name())));
        }
        assert!(!from_override("miniv2").screen_off_only());
    }

    #[test]
    fn h700_profiles_are_experimental_and_lack_fan_and_touch() {
        // (model, compatible, key, screen, sticks, wifi, lid)
        let rows: &[(&str, &[u8], &str, (u32, u32), bool, bool, bool)] = &[
            ("Anbernic RG28XX\0", b"anbernic,rg28xx\0allwinner,sun50i-h700\0", "rg28xx", (640, 480), false, false, false),
            ("Anbernic RG35XX 2024\0", b"anbernic,rg35xx-2024\0allwinner,sun50i-h700\0", "rg35xx-2024", (640, 480), false, false, false),
            ("Anbernic RG35XX Plus\0", b"anbernic,rg35xx-plus\0allwinner,sun50i-h700\0", "rg35xx-plus", (640, 480), false, true, false),
            ("Anbernic RG35XX H\0", b"anbernic,rg35xx-h\0allwinner,sun50i-h700\0", "rg35xx-h", (640, 480), true, true, false),
            ("Anbernic RG35XX Pro\0", b"anbernic,rg35xx-pro\0allwinner,sun50i-h700\0", "rg35xx-pro", (640, 480), true, true, false),
            ("Anbernic RG35XX SP\0", b"anbernic,rg35xx-sp\0allwinner,sun50i-h700\0", "rg35xx-sp", (640, 480), false, true, true),
            ("Anbernic RG35XX SP\0", b"anbernic,rg35xx-sp-v2-panel\0anbernic,rg35xx-sp\0allwinner,sun50i-h700\0", "rg35xx-sp", (640, 480), false, true, true),
            ("Anbernic RG40XX H\0", b"anbernic,rg40xx-h\0allwinner,sun50i-h700\0", "rg40xx-h", (640, 480), true, true, false),
            ("Anbernic RG40XX V\0", b"anbernic,rg40xx-V\0allwinner,sun50i-h700\0", "rg40xx-v", (640, 480), true, true, false),
            ("Anbernic RG34XX\0", b"anbernic,rg34xx\0allwinner,sun50i-h700\0", "rg34xx", (720, 480), false, true, false),
            ("Anbernic RG34XX-SP\0", b"anbernic,rg34xx-sp\0allwinner,sun50i-h700\0", "rg34xx-sp", (720, 480), true, true, true),
            ("Anbernic RG-SP\0", b"anbernic,rg-sp\0allwinner,sun50i-h700\0", "rg-sp", (720, 480), false, true, true),
            ("Anbernic RG CubeXX\0", b"anbernic,rgcubexx\0allwinner,sun50i-h700\0", "rgcubexx", (720, 720), true, true, false),
        ];
        for &(model, compatible, key, screen, sticks, wifi, lid) in rows {
            let d = identify(model, compatible);
            let m = d.model.unwrap_or_else(|| panic!("{key} not found"));
            assert!(m.is_h700() && m.key() == key, "{key}");
            assert_eq!(identify(model, b"").model, Some(m), "{key}: the model alone says the same");
            assert!(d.experimental() && !d.screen_off_only());
            assert_eq!(d.screen(), screen, "{key}");
            assert_eq!(d.has_lid(), lid, "{key}");
            assert_eq!(d.has_wifi(), wifi, "{key}");
            assert_eq!(d.has_sticks(), sticks, "{key}");
            assert!(!d.has_fan() && !d.has_touch());
            assert_eq!(d.ram_mb(), Some(1024));
            assert_eq!(d.menu_layout(), crate::button_layout::ButtonLayout::Nintendo);
            assert_eq!(d.about_line(), Some(format!("{} (experimental)", m.name())));
            assert_eq!(from_override(key).model, Some(m), "{key}");
        }
        assert_eq!(rows.iter().map(|r| r.2).collect::<std::collections::BTreeSet<_>>().len(), H700_BOARDS.len(), "every row of the table is tested");
        // Only the 720x720 CubeXX is held back; the installers read the same flag.
        assert_eq!(H700_BOARDS.iter().filter(|b| b.not_yet).map(|b| b.key).collect::<Vec<_>>(), ["rgcubexx"]);
        // Another Anbernic (not an H700 model in the table) stays unknown.
        assert_eq!(identify("Anbernic RG99XX\0", b"anbernic,rg99xx\0allwinner,sun50i-h700\0").model, None);
        // Every other device keeps its fan, touch, sticks and Wi-Fi; an unknown one too.
        for d in [from_override("miniv2"), from_override("flip2"), from_override("odin2"), identify("", b"")] {
            assert!(d.has_fan() && d.has_touch() && d.has_sticks() && d.has_wifi() && d.ram_mb().is_none());
        }
    }

    /// The installers keep their own lists of the models (shell, no Rust): the same names as `H700_BOARDS`, in the same order.
    #[test]
    fn the_installers_accept_exactly_the_h700_models_of_the_table() {
        fn list(text: &str, var: &str) -> Vec<String> {
            let line = text.lines().find(|l| l.trim_start().starts_with(var)).unwrap_or_else(|| panic!("{var} missing"));
            let value = line.split_once('=').unwrap().1.trim().trim_matches('"');
            value.split('|').filter(|n| !n.is_empty()).map(str::to_string).collect()
        }
        let want: Vec<String> = H700_BOARDS.iter().filter(|b| !b.not_yet).map(|b| b.name.to_string()).collect();
        let not_yet: Vec<String> = H700_BOARDS.iter().filter(|b| b.not_yet).map(|b| b.name.to_string()).collect();
        let install = include_str!("../deploy/package/install.sh");
        assert_eq!(list(install, "MODELS_H700="), want, "deploy/package/install.sh MODELS_H700");
        assert_eq!(list(install, "MODELS_H700_NOT_YET="), not_yet, "deploy/package/install.sh MODELS_H700_NOT_YET");
        let hook = include_str!("../deploy/fat/mount-storage.sh.in");
        assert_eq!(list(hook, "pu_h700_models="), want, "deploy/fat/mount-storage.sh.in pu_h700_models");
    }

    #[test]
    fn a_snapshot_state_can_name_its_device() {
        let (state, d) = snapshot_state("setup:first:device=flip2");
        assert_eq!((state, d.and_then(|d| d.model)), ("setup:first", Some(Model::Flip2)));
        assert_eq!(snapshot_state("about").0, "about");
        assert!(snapshot_state("about").1.is_none());
    }

    #[test]
    fn the_override_takes_short_names_and_model_strings() {
        assert_eq!(from_override("rp5").model, Some(Model::Pocket5));
        assert_eq!(from_override("RP5-Visionox").panel, Panel::Visionox);
        assert_eq!(from_override("flip2").model, Some(Model::Flip2));
        assert_eq!(from_override("mini").model, Some(Model::MiniV1));
        assert_eq!(from_override("miniv2").model, Some(Model::MiniV2));
        assert_eq!(from_override("Retroid Pocket Flip2").model, Some(Model::Flip2));
        assert_eq!(from_override("something else").model, None);
    }
}
