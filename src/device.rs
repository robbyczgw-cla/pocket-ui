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
//! **RG DS Plus** (Anbernic, ROCKNIX's RK3566 image, `anbernic,rg-ds-plus` / `Anbernic RG DS Plus`): experimental, two
//! 1024x768 panels (pocket-ui uses the main output), touch on the lower
//! one only, a lid, one stick, no fan, 1 GB, two card slots. The heavy systems stay on ROCKNIX's emulators as on an H700.
//!
//! `POCKETUI_DEVICE` stands in for the device tree (snapshots, tests, the virtual device): a short name (`miniv2`,
//! `mini`, `rp5`, `rp5-visionox`, `flip2`, `flip2-visionox`) or a `model` string.

use std::path::Path;
use std::sync::OnceLock;

pub use crate::device_model::*;

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
            Model::PocketClassic => "Retroid Pocket Classic",
            Model::Flip2 => "Retroid Pocket Flip 2",
            Model::Pocket6 => "Retroid Pocket 6",
            Model::Nova => "Retroid Pocket Nova",
            Model::Odin2 => "AYN Odin 2",
            Model::Odin2Mini => "AYN Odin 2 Mini",
            Model::Odin2Portal => "AYN Odin 2 Portal",
            Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b) => b.name,
            Model::RgDsPlus => "Anbernic RG DS Plus",
            #[cfg(feature = "brick")] Model::Brick(b) => b.name(),
        }
    }

    /// The screen as pocket-ui sees it (landscape, after sway's turn): the panel's portrait size the other way round.
    pub fn screen(self) -> (u32, u32) {
        match self {
            Model::MiniV2 | Model::PocketClassic => MINI_V2_SCREEN,
            Model::MiniV1 | Model::Nova => (1280, 960),
            Model::RgDsPlus => (1024, 768),
            Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b) => b.screen,
            #[cfg(feature = "brick")] Model::Brick(b) => b.screen(),
            Model::Pocket5 | Model::Flip2 | Model::Pocket6 | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal => (1920, 1080),
        }
    }

    /// Not tested on real hardware by the project (everything but the Mini V2, and the TrimUI Brick: About said "TrimUI Brick
    /// (experimental)" on the handheld pocket-ui is released for).
    pub fn experimental(self) -> bool {
        self != Model::MiniV2 && { #[cfg(feature = "brick")] { !matches!(self, Model::Brick(b) if !b.experimental()) } #[cfg(not(feature = "brick"))] { true } }
    }

    /// The project has played on it (🎮 in docs/DEVICES.md): the first start doesn't call it untested.
    pub fn played_on(self) -> bool {
        match self {
            Model::MiniV2 | Model::Odin2Mini => true,
            Model::H700(b) => matches!(b.key, "rg35xx-plus" | "rg35xx-sp"),
            // Played on since 02.11. (docs/TRIMUI-BRICK.md): the first start called it untested.
            #[cfg(feature = "brick")]
            Model::Brick(b) => b.played_on(),
            _ => false,
        }
    }

    /// The Flip 2's hall sensor: `SW_LID` on a `gpio-keys` device (docs/SLEEP.md "Lid"); the RG DS Plus's too.
    pub fn has_lid(self) -> bool {
        match self {
            Model::Flip2 | Model::RgDsPlus => true,
            Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b) => b.lid,
            _ => false,
        }
    }

    /// The Allwinner H700 handhelds (ROCKNIX's H700 image): 1 GB, no fan, no touch; sticks and Wi-Fi per model.
    pub fn is_h700(self) -> bool {
        matches!(self, Model::H700(_))
    }

    /// Systems whose pocket-core cores are too heavy for the chip stay on ROCKNIX's own emulators
    /// (`hostsys::H700_STAYS_ON_ROCKNIX`): the H700's Cortex-A53, and the RG DS Plus's Cortex-A55 until it is measured
    /// (its DS games need DraStic anyway, which puts them on both screens).
    pub fn small_chip(self) -> bool {
        self.is_h700() || self.is_rk3566() || self.is_rk3326() || self == Model::RgDsPlus
    }

    /// The one-screen RK3566 handhelds (`RK3566_BOARDS`): Cortex-A55, no fan; sticks, Wi-Fi and HOME per model.
    pub fn is_rk3566(self) -> bool {
        matches!(self, Model::Rk3566(_))
    }

    /// The R36S on ROCKNIX's RK3326 image (`RK3326_BOARDS`): Cortex-A35, no fan.
    pub fn is_rk3326(self) -> bool {
        matches!(self, Model::Rk3326(_))
    }

    /// The slowest chips, where games keep the full clock in Auto and Battery (`power`), GBA plays on the bundled gpSP
    /// (`h700_gba`) and the native audio keeps the H700's longer ring and PipeWire quantum (`platform::linux_audio`): the
    /// H700's Cortex-A53 (measured) and the RK3326's Cortex-A35 (slower per clock, not measured yet).
    pub fn h700_class(self) -> bool {
        self.is_h700() || self.is_rk3326()
    }


    /// The short names `POCKETUI_DEVICE` takes.
    fn key(self) -> &'static str {
        match self {
            Model::MiniV2 => "miniv2",
            Model::MiniV1 => "mini",
            Model::Pocket5 => "rp5",
            Model::PocketClassic => "rp-classic",
            Model::Flip2 => "flip2",
            Model::Pocket6 => "rp6",
            Model::Nova => "nova",
            Model::Odin2 => "odin2",
            Model::Odin2Mini => "odin2mini",
            Model::Odin2Portal => "odin2portal",
            Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b) => b.key,
            Model::RgDsPlus => "rgdsplus",
            #[cfg(feature = "brick")] Model::Brick(b) => b.key(),
        }
    }
}

impl Device {
    /// ROCKNIX SM8550 disables suspend and owns its userspace fan loop. The Brick's own card does not suspend either:
    /// suspend is unproven on its firmware, so "sleep" is the screen off with the game paused (docs/TRIMUI-BRICK-STANDALONE.md).
    #[cfg(not(feature = "brick"))]
    pub fn screen_off_only(&self) -> bool {
        self.is_sm8550()
    }

    #[cfg(feature = "brick")]
    pub fn screen_off_only(&self) -> bool {
        self.is_sm8550() || { #[cfg(feature = "brick")] { matches!(self.model, Some(Model::Brick(_))) } #[cfg(not(feature = "brick"))] { false } }
    }

    /// The Snapdragon 8 Gen 2 handhelds (Adreno 740).
    pub fn is_sm8550(&self) -> bool {
        matches!(self.model, Some(Model::Pocket6 | Model::Nova | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal))
    }

    /// Fan control belongs to ROCKNIX, independently of hardware suspend support.
    pub fn rocknix_fan(&self) -> bool {
        self.is_sm8550() || self.model == Some(Model::PocketClassic)
    }


    /// A fan the system can run. None on the H700 handhelds and the RG DS Plus: the fan page says so instead of offering a fan.
    #[cfg(not(feature = "brick"))]
    pub fn has_fan(&self) -> bool {
        !self.model.is_some_and(|m| m.small_chip())
    }

    #[cfg(feature = "brick")]
    pub fn has_fan(&self) -> bool {
        !self.model.is_some_and(|m| m.small_chip() || { #[cfg(feature = "brick")] { matches!(m, Model::Brick(_)) } #[cfg(not(feature = "brick"))] { false } })
    }

    /// A touch screen. None on the H700 handhelds (the kernel has no touch driver there) and the R36S (no touch panel).
    #[cfg(not(feature = "brick"))]
    pub fn has_touch(&self) -> bool {
        !self.model.is_some_and(|m| m.is_h700() || m.is_rk3326())
    }

    #[cfg(feature = "brick")]
    pub fn has_touch(&self) -> bool {
        !self.model.is_some_and(|m| m.is_h700() || m.is_rk3326() || { #[cfg(feature = "brick")] { matches!(m, Model::Brick(_)) } #[cfg(not(feature = "brick"))] { false } })
    }

    /// A second card slot, for a games card ROCKNIX reads by itself: every H700 handheld has one (TF2; every H700 device
    /// tree in ROCKNIX's image has `mmc@4020000` and `mmc@4022000`, both with `cd-gpios`), and the RG DS Plus (two TF
    /// slots) and the R36S (TF1 for the system, TF2 for games). The SM8250 and SM8550 trees have one card slot (`sdhc_2`).
    pub fn has_second_card_slot(&self) -> bool {
        self.model.is_some_and(|m| m.is_h700() || m.is_rk3326() || m == Model::RgDsPlus || matches!(m, Model::Rk3566(b) if b.key.starts_with("rg")))
    }

    /// The card's games folder shows up on Windows and macOS as it is: the TrimUI Brick's own FAT card. On the ROCKNIX
    /// handhelds the card's STORAGE part is ext4, which a computer can't read (and Windows offers to format it).
    pub fn card_reads_in_computer(&self) -> bool {
        #[cfg(feature = "brick")]
        { matches!(self.model, Some(Model::Brick(_))) }
        #[cfg(not(feature = "brick"))]
        { false }
    }

    /// Analog sticks, and so the lights around them. Per model on the H700 handhelds.
    #[cfg(not(feature = "brick"))]
    pub fn has_sticks(&self) -> bool {
        !self.model.is_some_and(|m| m == Model::PocketClassic) && !matches!(self.model, Some(Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b)) if !b.sticks)
    }

    #[cfg(feature = "brick")]
    pub fn has_sticks(&self) -> bool {
        !self.model.is_some_and(|m| m == Model::PocketClassic) && !{ #[cfg(feature = "brick")] { matches!(self.model, Some(Model::Brick(b)) if !b.sticks()) } #[cfg(not(feature = "brick"))] { false } } && !matches!(self.model, Some(Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b)) if !b.sticks)
    }

    /// A Wi-Fi chip. Not on the RG28XX and RG35XX 2024 (their device trees enable one anyway, the Plus tree they
    /// come from, so a chip that does answer still wins: `system_ctl::AutoHw` only calls it absent when none shows up).
    #[cfg(not(feature = "brick"))]
    pub fn has_wifi(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b)) if !b.wifi)
    }

    #[cfg(feature = "brick")]
    pub fn has_wifi(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b) | Model::Rk3566(b) | Model::Rk3326(b)) if !b.wifi) && !{ #[cfg(feature = "brick")] { matches!(self.model, Some(Model::Brick(_))) && !crate::brick_profile::standalone() } #[cfg(not(feature = "brick"))] { false } }
    }

    /// Memory in MB where the device is known to have less than the Mini V2's (1 GB on the H700 and the RG DS Plus, no swap).
    #[cfg(not(feature = "brick"))]
    pub fn ram_mb(&self) -> Option<u32> {
        self.model.filter(|m| m.small_chip()).map(|_| 1024)
    }

    #[cfg(feature = "brick")]
    pub fn ram_mb(&self) -> Option<u32> {
        self.model.filter(|m| m.small_chip() || { #[cfg(feature = "brick")] { matches!(m, Model::Brick(_)) } #[cfg(not(feature = "brick"))] { false } }).map(|_| 1024)
    }

    /// Printed face letters on AYN's product images; saved menu choices still win.
    pub fn menu_layout(&self) -> crate::button_layout::ButtonLayout {
        use crate::button_layout::ButtonLayout;
        match self.model {
            Some(Model::Odin2Mini | Model::Odin2Portal) => ButtonLayout::Xbox,
            _ => ButtonLayout::Nintendo,
        }
    }

    /// Settings → Controls → Buttons → Button symbols when unset (`button_prompts`): Xbox-style caps on
    /// Retroid's and AYN's handhelds, plain letters on Anbernic's and anything unknown. A saved choice wins.
    pub fn prompt_style(&self) -> crate::button_prompts::PromptStyle {
        use crate::button_prompts::PromptStyle;
        match self.model {
            Some(Model::H700(_) | Model::Rk3566(_) | Model::Rk3326(_) | Model::RgDsPlus) | None => PromptStyle::Nintendo,
            Some(_) => PromptStyle::Xbox,
        }
    }

    /// Touch ranges if evdev has none. The kernel already applies DT axis inversions.
    pub fn panel_size(&self) -> (u32, u32) {
        match self.model {
            // Landscape like the screen: the Goodix's `touchscreen-size` is 1024x768 (the lower panel's).
            Some(Model::Nova) => (1280, 960),
            Some(Model::RgDsPlus) => (1024, 768),
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
            "retroid,pocket-classic" => Some(Model::PocketClassic),
            "retroidpocket,rpflip2" => Some(Model::Flip2),
            "retroidpocket,rp6" => Some(Model::Pocket6),
            "retroidpocket,rpnova" => Some(Model::Nova),
            "ayn,odin2" => Some(Model::Odin2),
            "ayn,odin2mini" => Some(Model::Odin2Mini),
            "ayn,odin2portal" => Some(Model::Odin2Portal),
            "anbernic,rg-ds-plus" => Some(Model::RgDsPlus),
            _ => None,
        })
        .or_else(|| {
            let names: Vec<&str> = compatible.split(|b| *b == 0).filter_map(|c| std::str::from_utf8(c).ok()).collect();
            // The RG353M carries the RG353P's tree with only `model` changed: `model` first there.
            rk3566_board(model, &names).map(Model::Rk3566).or_else(|| h700_board("", &names).map(Model::H700)).or_else(|| rk3326_board("", &names).map(Model::Rk3326))
        });
    let found = by_compatible.or(match model.strip_suffix(" Visionox").unwrap_or(model) {
        "Retroid Pocket Mini V2" => Some(Model::MiniV2),
        "Retroid Pocket Mini" => Some(Model::MiniV1),
        "Retroid Pocket 5" => Some(Model::Pocket5),
        "Retroid Pocket Classic" | "Retroid Pocket Classic Rev Q" => Some(Model::PocketClassic),
        "Retroid Pocket Flip2" => Some(Model::Flip2),
        "Retroid Pocket 6" | "Retroid Pocket 6 TOP-DPAD" => Some(Model::Pocket6),
        "Retroid Pocket Nova" => Some(Model::Nova),
        "AYN Odin 2" => Some(Model::Odin2),
        "AYN Odin 2 Mini" => Some(Model::Odin2Mini),
        "AYN Odin 2 Portal" => Some(Model::Odin2Portal),
        "Anbernic RG DS Plus" => Some(Model::RgDsPlus),
        other => h700_board(other, &[]).map(Model::H700).or_else(|| rk3566_board(other, &[]).map(Model::Rk3566)).or_else(|| rk3326_board(other, &[]).map(Model::Rk3326)),
    });
    let panel = if model.ends_with(" Visionox") { Panel::Visionox } else { Panel::Original };
    Device { model: found, panel, reported: model.to_string() }
}

/// `POCKETUI_DEVICE`'s value: a short name or a `model` string.
pub fn from_override(v: &str) -> Device {
    let v = v.trim(); #[cfg(feature = "brick")] if let Some(b) = crate::brick_profile::Board::by_key(v) { return Device { model: Some(Model::Brick(b)), panel: Panel::Original, reported: format!("{v} (POCKETUI_DEVICE)") }; } #[cfg(feature = "brick")] if let Some(b) = crate::darkos::board_by_key(v) { return Device { model: Some(Model::H700(b)), panel: Panel::Original, reported: format!("{} (dArkOS)", b.name) }; }
    let lower = v.to_ascii_lowercase();
    let (key, panel) = match lower.strip_suffix("-visionox") {
        Some(k) => (k, Panel::Visionox),
        None => (lower.as_str(), Panel::Original),
    };
    match [Model::PocketClassic, Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova, Model::Odin2, Model::Odin2Mini, Model::Odin2Portal, Model::RgDsPlus]
        .into_iter()
        .chain(H700_BOARDS.iter().map(Model::H700))
        .chain(RK3566_BOARDS.iter().map(Model::Rk3566))
        .chain(RK3326_BOARDS.iter().map(Model::Rk3326))
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
    let base = root.join("sys/firmware/devicetree/base"); #[cfg(feature = "brick")] if crate::brick_profile::detected(root) { let b = crate::brick_profile::board_at(root); return Device { model: Some(Model::Brick(b)), panel: Panel::Original, reported: b.reported().to_string() }; }
    let model = std::fs::read_to_string(base.join("model")).unwrap_or_default();
    let compatible = std::fs::read(base.join("compatible")).unwrap_or_default();
    identify(&model, &compatible)
}

/// ROCKNIX owns cooling on SM8550 and SM4450, including boards without a UI profile.
#[cfg(not(feature = "brick"))]
pub fn rocknix_cooling(root: &Path) -> bool {
    (root == Path::new("/") && current().rocknix_fan())
        || std::fs::read(root.join("sys/firmware/devicetree/base/compatible")).is_ok_and(|b| b.split(|v| *v == 0).any(|c| c == b"qcom,sm8550" || c == b"qcom,sm4450"))
        || std::fs::read_to_string(root.join("etc/os-release")).is_ok_and(|s| s.lines().any(|l| l.starts_with("HW_DEVICE=") && (l.contains("SM8550") || l.contains("SM4450"))))
}

#[cfg(feature = "brick")]
pub fn rocknix_cooling(root: &Path) -> bool {
    (root == Path::new("/") && current().rocknix_fan())
        || std::fs::read(root.join("sys/firmware/devicetree/base/compatible")).is_ok_and(|b| b.split(|v| *v == 0).any(|c| c == b"qcom,sm8550" || c == b"qcom,sm4450"))
        || std::fs::read_to_string(root.join("etc/os-release")).is_ok_and(|s| s.lines().any(|l| l.starts_with("HW_DEVICE=") && (l.contains("SM8550") || l.contains("SM4450"))))
}

/// The platform's suspend-off policy, independent of who controls its fan.
/// SM4450 retains hardware suspend while SM8550 only turns the screen off.
pub fn rocknix_screen_off_only(root: &Path) -> bool {
    (root == Path::new("/") && current().screen_off_only())
        || std::fs::read(root.join("sys/firmware/devicetree/base/compatible")).is_ok_and(|b| b.split(|v| *v == 0).any(|c| c == b"qcom,sm8550"))
        || std::fs::read_to_string(root.join("etc/os-release")).is_ok_and(|s| s.lines().any(|l| l.starts_with("HW_DEVICE=") && l.contains("SM8550")))
}

static DEVICE: OnceLock<Device> = OnceLock::new();

/// No Wi-Fi chip on this handheld (the RG28XX, the RG35XX 2024, the R36S: `Device::has_wifi`). A page that needs a
/// network says so (`NEEDS_NETWORK`) or names the way without one, never "Connect to Wi-Fi" (tests/no_wifi_walk.rs).
pub fn no_wifi_chip() -> bool {
    !current().has_wifi()
}

/// What a page that needs a network says on a handheld without Wi-Fi.
pub const NEEDS_NETWORK: &str = "Needs a network, and this handheld has no Wi-Fi.";

/// `wifi` where the handheld has Wi-Fi, `none` where it has none (`no_wifi_chip`).
pub fn wifi_or<'a>(wifi: &'a str, none: &'a str) -> &'a str {
    if no_wifi_chip() { none } else { wifi }
}

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

/// Android's handheld view (android/handheld): on Android, and on Linux for the tests and snapshots with
/// `POCKETUI_ANDROID_PROFILE=1` (tests/nav_explorer.rs walks the pages as the Odin's Android shows them).
pub fn android_profile() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    cfg!(all(target_os = "android", feature = "handheld")) || *ON.get_or_init(|| std::env::var_os(ANDROID_PROFILE_ENV).is_some_and(|v| v == "1"))
}

/// The variable that puts the Linux build in Android's profile ([`android_profile`]).
pub const ANDROID_PROFILE_ENV: &str = "POCKETUI_ANDROID_PROFILE";

/// What Android's handheld view (android/handheld) leaves to Android or doesn't have: the radios (Wi-Fi, Bluetooth, remote
/// access), sleep and power, the fan, the LEDs and the motor, the clock, the card and its backup, ROCKNIX's updates, and
/// the services of a Linux handheld (Syncthing, Tailscale, the web share, PC streaming, tools), the screen's own colour,
/// EmulationStation's gamelists, the backup to the card and the button test (it reads Linux's input devices).
pub const ANDROID_HIDES: &[&str] = &["leds", "haptics", "sync", "tailscale", "tools", "perf_fan", "radios", "time", "updates", "sleep",
    "storage", "web", "pc_streaming", "night_light", "screen_colour", "battery", "gamelist", "backup", "test_buttons",
    // The report is written to the card's reports folder; Android's way is the Mobile view's Report a problem (Settings → View).
    "report",
    // What only pocket-core's pause menu and overlay have: the Android player's menu has Zoom always, one Save & quit, its own
    // Over the game (Off/FPS, Options in the game, kept with its picture choices: no default here), no Pixel Path line and no run-ahead,
    // no console boot screen (it has no BIOS folder to read), and its Screenshot button saves into the player's folder, not the one this view reads.
    "zoom_pause", "boot_screen", "over_game", "remember_quit", "quit_without_saving", "pixel_path", "pixel_path_live", "run_ahead", "screenshots"];

/// What a game's "This game" page offers on Android's handheld view: the settings of the picture the Android player draws
/// (scaling, the Look, the frame; its four effects are the Look's four values, so Custom with Sharpness and Effect is not there). Everything else of Linux's list is pocket-core's: the
/// player has no rewind or Earlier setting, no fast-forward, rumble, fan or run-ahead, no screen turn and no HOME shortcuts, so a
/// row for them would be a switch that does nothing. Over the game (FPS) is in the player's own Options, kept by the player.
pub const ANDROID_PLAYER_SETTINGS: &[&str] = &["scaling", "look", "frame"];

/// Whether a game setting is out of the page, given Android's profile.
pub fn game_setting_out_in(key: &str, android: bool) -> bool {
    android && !ANDROID_PLAYER_SETTINGS.contains(&key)
}

/// Settings pages and feature cards a device lacks, by key: the one list behind the Settings rows (`app_settings_groups`)
/// and the Tour / first-start cards (`features::available`). `leds` is the lights around the sticks; the rest are what
pub fn hides(key: &str, sticks: bool, nextui_owned: bool) -> bool {
    hides_in(key, sticks, nextui_owned, android_profile())
}

/// [`hides`] with Android's profile given.
pub fn hides_in(key: &str, sticks: bool, nextui_owned: bool, android: bool) -> bool {
    if android && ANDROID_HIDES.contains(&key) {
        return true;
    }
    // Settings → View (handheld or Mobile) is Android's alone.
    if key == "android_view" {
        return !android;
    }
    // Settings → System → Back to EmulationStation is dArkOS's alone (src/darkos.rs).
    if key == "darkos_es" {
        #[cfg(feature = "brick")]
        return !crate::darkos::host();
        #[cfg(not(feature = "brick"))]
        return true;
    }
    // dArkOS keeps its own system; the port has a separate signed app-only update channel.
    #[cfg(feature = "brick")]
    if crate::darkos::hides(key) {
        return true;
    }
    match key {
        "leds" => !sticks,
        "haptics" | "sync" | "tailscale" | "tools" | "perf_fan" | "radios" | "time" | "updates" | "kids" => nextui_owned,
        _ => false,
    }
}

/// The Tour cards a TrimUI Brick must not show because the page they send to is not there (the gap list: "the Tour promises switches that
/// do nothing"): Screen frames without the GPU's picture path, RetroAchievements with NextUI's card, Players (no second controller), and on
/// NextUI's card Over the game. Settings rows come from `app_settings_groups::brick_hides_with`; this is the Tour's side of it.
#[cfg(feature = "brick")]
pub fn brick_card_hidden(key: &str, standalone: bool) -> bool {
    match key {
        // Screen frames come with the GPU's picture path (`brick_profile::picture_rows_out`); RetroAchievements on the card's own Settings.
        "frames" => crate::brick_profile::picture_rows_out(),
        "achievements" => !standalone,
        "players" => true,
        "pixel_path_live" => !standalone,
        _ => false,
    }
}

/// `hides` for this device.
#[cfg(not(feature = "brick"))]
pub fn hides_here(key: &str) -> bool {
    let d = current();
    hides(key, d.has_sticks(), false)
}

#[cfg(feature = "brick")]
pub fn hides_here(key: &str) -> bool {
    let d = current();
    #[cfg(feature = "brick")]
    if matches!(d.model, Some(Model::Brick(_))) && brick_card_hidden(key, crate::brick_profile::standalone()) {
        return true;
    }
    // The Brick's own card (no NextUI): the Wi-Fi page stays, there is a radio and a client of ours; so do the clock (`brick_standalone_keeps`),
    // Tailscale, Updates (its private channel) and Kids mode, which is pocket-ui's alone (docs/TRIMUI-BRICK-STANDALONE.md, docs/TRIMUI-BRICK-MORE.md).
    // On every Brick the motor and the lights are its own (`brick_hw`): Haptics and LEDs and their Tour cards stay, though it has no sticks.
    #[cfg(feature = "brick")]
    if let Some(Model::Brick(b)) = d.model {
        // The Smart Pro's lights are not mapped yet (`Board::leds`): no page, no card, until they are.
        if key == "leds" && !b.leds() { return true; }
        if brick_standalone_keeps(key, crate::brick_profile::standalone()) || crate::brick_profile::standalone() && matches!(key, "tailscale" | "updates" | "kids" | "perf_fan") || matches!(key, "haptics" | "leds") {
            return false;
        }
    }
    hides(key, d.has_sticks(), { #[cfg(feature = "brick")] { matches!(d.model, Some(Model::Brick(_))) } #[cfg(not(feature = "brick"))] { false } })
}

/// The pages the Brick's own card (no NextUI) keeps though NextUI normally owns the rest: the Wi-Fi page
/// (a radio and a client of ours) and the clock page (the card carries its own zone data and time helpers,
/// docs/TRIMUI-BRICK-STANDALONE.md). With NextUI's card both stay out: NextUI owns them there.
#[cfg(feature = "brick")]
fn brick_standalone_keeps(key: &str, standalone: bool) -> bool {
    standalone && matches!(key, "radios" | "time")
}

/// Settles the device before anything has read it (`--snapshot` with a `:device=`). False when it was read already.
pub fn force(d: Device) -> bool {
    DEVICE.set(d).is_ok()
}

#[cfg(test)]
mod tests {
    /// Android's handheld view leaves out what Android owns (Wi-Fi, sleep, the fan ...) and only it has Settings → View; a
    /// Linux handheld keeps every row and has no View row.
    #[test]
    fn android_hides_what_android_owns_and_only_android_has_the_view_row() {
        for key in super::ANDROID_HIDES {
            assert!(super::hides_in(key, true, false, true), "{key} on Android");
        }
        for key in ["radios", "sleep", "updates", "gamelist", "backup"] {
            assert!(!super::hides_in(key, true, false, false), "{key} on Linux");
        }
        assert!(super::hides_in("android_view", true, false, false), "no View row on Linux");
        assert!(!super::hides_in("android_view", true, false, true), "the View row on Android");
    }

    /// docs/TRIMUI-BRICK-MORE.md: the Tour leaves out the cards whose page the Brick has no row for, and every card it keeps names a real id.
    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_tour_leaves_out_cards_without_a_page() {
        for id in ["frames", "achievements", "players"] {
            assert!(crate::features::FEATURES.iter().any(|f| f.id == id), "{id} is a card");
        }
        assert!(brick_card_hidden("players", true) && brick_card_hidden("players", false), "no second controller");
        assert!(brick_card_hidden("achievements", false) && !brick_card_hidden("achievements", true), "RetroAchievements on the card's own Settings");
        assert_eq!(brick_card_hidden("frames", true), crate::brick_profile::picture_rows_out(), "Screen frames with the GPU's picture path");
        assert!(brick_card_hidden("pixel_path_live", false) && !brick_card_hidden("pixel_path_live", true), "Over the game is on the card's own Settings");
        for id in ["tailscale", "kids", "sounds", "lens"] {
            assert!(!brick_card_hidden(id, true), "{id}");
        }
    }

    use super::*;
    use crate::testutil::TestDir;

    /// device is running under NextUI.
    /// New-player walk 2026-11-02: the welcome called an RG35XX SP untested while docs/DEVICES.md has played on it.
    #[test]
    fn the_handhelds_the_project_has_played_on_are_the_ones_docs_devices_says() {
        assert!(Model::MiniV2.played_on() && Model::Odin2Mini.played_on());
        assert!(!Model::Pocket5.played_on());
        let of = |tree: &[u8]| identify("", tree).model.expect("known").played_on();
        assert!(of(b"anbernic,rg35xx-sp\0allwinner,sun50i-h700\0"));
        assert!(of(b"anbernic,rg35xx-plus\0allwinner,sun50i-h700\0"));
        assert!(!of(b"anbernic,rg28xx\0allwinner,sun50i-h700\0"));
        let doc = include_str!("../docs/DEVICES.md");
        for name in ["Retroid Pocket Mini V2", "AYN Odin 2 Mini", "Anbernic RG35XX Plus", "Anbernic RG35XX SP"] {
            assert!(doc.lines().any(|l| l.starts_with("| 🎮") && l.contains(name)), "{name}");
        }
    }

    /// A real Brick, 2026-11-06: the first start's welcome told the owner pocket-ui "hasn't been tested on a TrimUI
    /// Brick yet" on a Brick the project plays on.
    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_is_played_on_and_docs_devices_says_so() {
        let brick = Model::Brick(crate::brick_profile::Board::Brick);
        assert!(brick.played_on());
        let doc = include_str!("../docs/DEVICES.md");
        assert!(doc.lines().any(|l| l.starts_with("| 🎮") && l.contains(brick.name()) && l.contains("1024 × 768")));
    }


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
            assert_eq!(d.prompt_style(), crate::button_prompts::PromptStyle::Xbox, "AYN: Xbox-style caps whatever is printed");
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
    fn retroids_get_xbox_style_caps_and_an_unknown_device_plain_letters() {
        use crate::button_prompts::PromptStyle;
        for m in [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova] {
            let d = Device { model: Some(m), panel: Panel::Original, reported: String::new() };
            assert_eq!(d.prompt_style(), PromptStyle::Xbox, "{m:?}");
        }
        assert_eq!(Device::unknown("Some Handheld").prompt_style(), PromptStyle::Nintendo);
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

    /// The Brick's own card keeps the Wi-Fi and clock pages (its own radio client, zone data and time helpers); with
    /// NextUI's card both stay NextUI's, and nothing else the Brick lacks comes back with them.
    #[cfg(feature = "brick")]
    #[test]
    fn the_bricks_own_card_keeps_wifi_and_the_clock_only() {
        for key in ["radios", "time"] {
            assert!(brick_standalone_keeps(key, true), "{key} on the card");
            assert!(!brick_standalone_keeps(key, false), "{key} with NextUI");
        }
        for key in ["updates", "tools", "perf_fan", "tailscale", "sync"] {
            assert!(!brick_standalone_keeps(key, true), "{key} stays out");
        }
    }

    /// Sleep on the Brick's card is the screen off with the game waiting (suspend is unproven on its firmware); the SM8550
    /// cooling rules are not the Brick's, and a ROCKNIX device keeps what it had.
    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_sleeps_dark_but_does_not_borrow_the_snapdragon_cooling_rules() {
        let brick = from_override("brick");
        assert!(brick.screen_off_only() && !brick.is_sm8550());
        assert!(!from_override("miniv2").screen_off_only());
        assert!(from_override("odin2mini").screen_off_only() && from_override("odin2mini").is_sm8550());
    }

    #[test]
    fn classic_has_touch_and_safe_cooling_without_stick_lights() {
        let d = identify("Retroid Pocket Classic Rev Q\0", b"retroid,pocket-classic\0qcom,sm4450\0");
        assert_eq!(d.model, Some(Model::PocketClassic));
        assert_eq!(from_override("rp-classic").model, d.model);
        assert_eq!(identify("Retroid Pocket Classic Rev Q", b"").model, d.model);
        assert_eq!((d.screen(), d.panel_size()), ((1240, 1080), (1080, 1240)));
        assert!(d.experimental() && !d.model.unwrap().played_on());
        assert!(d.has_touch() && d.has_fan() && d.has_wifi() && d.rocknix_fan());
        assert!(!d.has_sticks() && !d.has_lid() && !d.has_second_card_slot());
        assert!(!d.screen_off_only(), "SM4450 has no suspend-off platform quirk");
        assert!(hides("leds", d.has_sticks(), false), "no stick-light settings");
        let dir = TestDir::new("classic-cooling");
        std::fs::create_dir_all(dir.join("etc")).unwrap();
        std::fs::write(dir.join("etc/os-release"), "HW_DEVICE=\"SM4450\"\n").unwrap();
        assert!(rocknix_cooling(&dir), "pocket-ui must not write fan PWM");
        assert!(!rocknix_screen_off_only(&dir), "fan ownership must not block suspend");
        std::fs::write(dir.join("etc/os-release"), "HW_DEVICE=\"SM8550\"\n").unwrap();
        assert!(rocknix_screen_off_only(&dir), "SM8550 remains screen-off-only");
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
            assert_eq!(d.prompt_style(), crate::button_prompts::PromptStyle::Nintendo, "{key}: Anbernic, plain letters");
            assert_eq!(d.ram_mb(), Some(1024));
            assert_eq!(d.menu_layout(), crate::button_layout::ButtonLayout::Nintendo);
            assert_eq!(d.about_line(), Some(format!("{} (experimental)", m.name())));
            assert_eq!(from_override(key).model, Some(m), "{key}");
        }
        assert_eq!(rows.iter().map(|r| r.2).collect::<std::collections::BTreeSet<_>>().len(), H700_BOARDS.len(), "every row of the table is tested");
        // Another Anbernic (not an H700 model in the table) stays unknown.
        assert_eq!(identify("Anbernic RG99XX\0", b"anbernic,rg99xx\0allwinner,sun50i-h700\0").model, None);
        // Every other device keeps its fan, touch, sticks and Wi-Fi; an unknown one too.
        for d in [from_override("miniv2"), from_override("flip2"), from_override("odin2"), identify("", b"")] {
            assert!(d.has_fan() && d.has_touch() && d.has_sticks() && d.has_wifi() && d.ram_mb().is_none());
        }
    }

    /// ROCKNIX's `rk3568-anbernic-rg-ds-plus.dts` (20261001): its strings, and what the profile says the device has. The
    /// first RG DS (no Plus, 640x480, Android) is another device and stays unknown.
    #[test]
    fn the_rg_ds_plus_is_told_by_its_device_tree() {
        let d = identify("Anbernic RG DS Plus\0", b"anbernic,rg-ds-plus\0rockchip,rk3568\0");
        assert_eq!(d.model, Some(Model::RgDsPlus));
        assert_eq!(identify("Anbernic RG DS Plus\0", b"").model, Some(Model::RgDsPlus), "the model alone");
        assert_eq!(from_override("rgdsplus").model, Some(Model::RgDsPlus));
        assert_eq!(identify("Anbernic RG DS\0", b"anbernic,rg-ds\0rockchip,rk3568\0").model, None);
        assert!(d.experimental() && !d.screen_off_only() && !d.is_sm8550());
        assert_eq!((d.screen(), d.panel_size()), ((1024, 768), (1024, 768)));
        assert!(d.has_lid() && d.has_touch() && d.has_sticks() && d.has_wifi() && d.has_second_card_slot() && !d.has_fan());
        assert_eq!(d.ram_mb(), Some(1024));
        assert!(Model::RgDsPlus.small_chip() && !Model::RgDsPlus.is_h700() && !Model::MiniV2.small_chip());
        assert_eq!(d.prompt_style(), crate::button_prompts::PromptStyle::Nintendo, "Anbernic: plain letters");
        assert_eq!(d.menu_layout(), crate::button_layout::ButtonLayout::Nintendo);
        assert_eq!(d.about_line().as_deref(), Some("Anbernic RG DS Plus (experimental)"));
        assert!(include_str!("../deploy/package/install.sh").contains("MODELS_RK3566=\"Anbernic RG DS Plus|"));
    }

    /// ROCKNIX 20261101's RK3566 trees (`device_trees/` of its Generic image, read 2026-11-15): each one-screen model by its own
    /// strings, the RG353M by `model` on the RG353P's tree (U-Boot changes only `model`), and what each has.
    #[test]
    fn rk3566_profiles_are_told_by_their_trees_and_say_what_each_has() {
        // (model, compatible, key, screen, wifi, HOME)
        use crate::device::Rk3566Home::{FKey, RightStickClick};
        let rows: &[(&str, &[u8], &str, (u32, u32), bool, Rk3566Home)] = &[
            ("Anbernic RG353M\0", b"anbernic,rg353p\0rockchip,rk3566\0", "rg353m", (640, 480), true, FKey),
            ("Anbernic RG353P\0", b"anbernic,rg353p\0rockchip,rk3566\0", "rg353p", (640, 480), true, FKey),
            ("Anbernic RG353PS\0", b"anbernic,rg353ps\0rockchip,rk3566\0", "rg353ps", (640, 480), true, FKey),
            ("Anbernic RG353V\0", b"anbernic,rg353v\0rockchip,rk3566\0", "rg353v", (640, 480), true, FKey),
            ("Anbernic RG353VS\0", b"anbernic,rg353vs\0rockchip,rk3566\0", "rg353vs", (640, 480), true, FKey),
            ("Anbernic RG503\0", b"anbernic,rg503\0rockchip,rk3566\0", "rg503", (960, 544), true, FKey),
            ("Powkiddy RGB30\0", b"powkiddy,rgb30\0rockchip,rk3566\0", "rgb30", (720, 720), true, RightStickClick),
            ("Powkiddy RGB20SX\0", b"powkiddy,rgb20sx\0rockchip,rk3566\0", "rgb20sx", (720, 720), true, RightStickClick),
            ("Powkiddy RK2023\0", b"powkiddy,rk2023\0rockchip,rk3566\0", "rk2023", (640, 480), false, RightStickClick),
        ];
        for &(model, compatible, key, screen, wifi, home) in rows {
            let d = identify(model, compatible);
            let m = d.model.unwrap_or_else(|| panic!("{key} not found"));
            let Model::Rk3566(b) = m else { panic!("{key}: {m:?}") };
            assert_eq!(m.key(), key);
            assert_eq!(identify(model, b"").model, Some(m), "{key}: the model alone says the same");
            assert_eq!(from_override(key).model, Some(m), "{key}");
            assert!(m.small_chip() && m.is_rk3566() && !m.is_h700() && d.experimental() && !d.screen_off_only() && !d.is_sm8550());
            assert_eq!(d.screen(), screen, "{key}");
            assert_eq!(d.has_wifi(), wifi, "{key}");
            assert!(d.has_sticks() && !d.has_lid() && !d.has_fan(), "{key}");
            assert_eq!(crate::device::rk3566_home(b), home, "{key}");
            assert_eq!(d.prompt_style(), crate::button_prompts::PromptStyle::Nintendo, "{key}: plain letters");
            assert_eq!(d.about_line(), Some(format!("{} (experimental)", m.name())));
        }
        assert_eq!(rows.iter().map(|r| r.2).collect::<std::collections::BTreeSet<_>>().len(), RK3566_BOARDS.len(), "every row of the table is tested");
        // The RG353P's own tree stays the RG353P; trees of the image without a profile stay unknown.
        assert_eq!(identify("Anbernic RG353P\0", b"anbernic,rg353p\0").model.map(|m| m.key()), Some("rg353p"));
        for (model, compatible) in [("Powkiddy x35s\0", &b"powkiddy,x35s\0rockchip,rk3566\0"[..]), ("Powkiddy RGB10MAX3\0", b"powkiddy,rgb10max3\0rockchip,rk3566\0"), ("Anbernic RG ARC-D\0", b"anbernic,rg-arc-d\0rockchip,rk3566\0")] {
            assert_eq!(identify(model, compatible).model, None, "{model}");
        }
        // The RG DS Plus keeps its own profile.
        assert_eq!(identify("Anbernic RG DS Plus\0", b"anbernic,rg-ds-plus\0rockchip,rk3568\0").model, Some(Model::RgDsPlus));
    }

    /// The installers keep their own lists of the models (shell, no Rust): the same names as `H700_BOARDS`, in the same order.
    #[test]
    fn the_installers_accept_exactly_the_h700_models_of_the_table() {
        fn list(text: &str, var: &str) -> Vec<String> {
            let line = text.lines().find(|l| l.trim_start().starts_with(var)).unwrap_or_else(|| panic!("{var} missing"));
            let value = line.split_once('=').unwrap().1.trim().trim_matches('"');
            value.split('|').filter(|n| !n.is_empty()).map(str::to_string).collect()
        }
        let want: Vec<String> = H700_BOARDS.iter().map(|b| b.name.to_string()).collect();
        assert!(want.iter().any(|n| n == "Anbernic RG CubeXX"), "the square CubeXX is installed like the others");
        let install = include_str!("../deploy/package/install.sh");
        assert_eq!(list(install, "MODELS_H700="), want, "deploy/package/install.sh MODELS_H700");
        let hook = include_str!("../deploy/fat/mount-storage.sh.in");
        assert_eq!(list(hook, "pu_h700_models="), want, "deploy/fat/mount-storage.sh.in pu_h700_models");
        // RK3566: the RG DS Plus, then `RK3566_BOARDS`.
        let want: Vec<String> = std::iter::once(Model::RgDsPlus.name()).chain(RK3566_BOARDS.iter().map(|b| b.name)).map(str::to_string).collect();
        assert_eq!(list(install, "MODELS_RK3566="), want, "deploy/package/install.sh MODELS_RK3566");
        assert_eq!(list(hook, "pu_rk3566_models="), want, "deploy/fat/mount-storage.sh.in pu_rk3566_models");
        // RK3326: `RK3326_BOARDS`.
        let want: Vec<String> = RK3326_BOARDS.iter().map(|b| b.name.to_string()).collect();
        assert_eq!(list(install, "MODELS_RK3326="), want, "deploy/package/install.sh MODELS_RK3326");
        assert_eq!(list(hook, "pu_rk3326_models="), want, "deploy/fat/mount-storage.sh.in pu_rk3326_models");
    }

    /// ROCKNIX's R36S tree (`rk3326-gameconsole-r36s.dts`, tag 20261001), which the RK3326 card boots by name: the R36S
    /// profile with what a player notices (640x480, two sticks, a second card slot, no fan, no touch, no Wi-Fi promised,
    /// plain letters), the slow chip's game rules; the R33S tree (ROCKNIX's U-Boot script's guess for an R36S) stays unknown.
    #[test]
    fn the_r36s_is_told_by_rocknix_r36s_tree_and_says_what_it_has() {
        let d = identify("Game Console R36S\0", b"gameconsole,r35s\0gameconsole,r36s\0rockchip,rk3326\0");
        let m = d.model.expect("the R36S tree has a profile");
        assert!(matches!(m, Model::Rk3326(b) if b.key == "r36s"), "{m:?}");
        assert_eq!(identify("Game Console R36S\0", b"").model, Some(m), "the model alone says the same");
        assert_eq!(identify("", b"gameconsole,r36s\0rockchip,rk3326\0").model, Some(m), "the compatible alone too");
        assert_eq!(from_override("r36s").model, Some(m));
        assert!(m.is_rk3326() && m.small_chip() && m.h700_class() && !m.is_h700() && !m.is_rk3566());
        assert!(d.experimental() && !d.screen_off_only() && !d.is_sm8550());
        assert_eq!(d.screen(), (640, 480));
        assert!(d.has_sticks() && d.has_second_card_slot() && !d.has_fan() && !d.has_lid() && !d.has_touch());
        assert!(!d.has_wifi(), "no Wi-Fi by the maker's specification: no page promises it");
        assert_eq!(d.prompt_style(), crate::button_prompts::PromptStyle::Nintendo);
        assert_eq!(d.about_line().as_deref(), Some("Game Console R36S (experimental)"));
        for (model, compatible) in [("Game Console R33S\0", &b"gameconsole,r33s\0rockchip,rk3326\0"[..]), ("Anbernic RG351M\0", b"anbernic,rg351m\0rockchip,rk3326\0")] {
            assert_eq!(identify(model, compatible).model, None, "{model}");
        }
        // dArkOS's R36S row stays the port's (an H700-shaped row, found by start.sh), not this one.
        assert_ne!(from_override("darkos-r36s").model, Some(m));
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

#[cfg(feature = "brick")]
pub fn brick() -> bool {
    matches!(current().model, Some(Model::Brick(_)))
}

/// Running as the dArkOS port, whose native games and signed updater belong to its own host.
pub fn darkos_port() -> bool {
    #[cfg(feature = "brick")]
    { crate::darkos::host() }
    #[cfg(not(feature = "brick"))]
    { false }
}
