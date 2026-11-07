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
//! 1024x768 panels (pocket-ui on the upper one, the selected game on the lower one: `second_screen`), touch on the lower
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
            Model::Flip2 => "Retroid Pocket Flip 2",
            Model::Pocket6 => "Retroid Pocket 6",
            Model::Nova => "Retroid Pocket Nova",
            Model::Odin2 => "AYN Odin 2",
            Model::Odin2Mini => "AYN Odin 2 Mini",
            Model::Odin2Portal => "AYN Odin 2 Portal",
            Model::H700(b) => b.name,
            Model::RgDsPlus => "Anbernic RG DS Plus",
            Model::SteamDeck => "Steam Deck", #[cfg(feature = "brick")] Model::Brick => crate::brick_profile::NAME,
        }
    }

    /// The screen as pocket-ui sees it (landscape, after sway's turn): the panel's portrait size the other way round.
    pub fn screen(self) -> (u32, u32) {
        match self {
            Model::MiniV2 => MINI_V2_SCREEN,
            Model::MiniV1 | Model::Nova => (1280, 960),
            Model::RgDsPlus => (1024, 768),
            Model::H700(b) => b.screen,
            Model::SteamDeck => (1280, 800), #[cfg(feature = "brick")] Model::Brick => crate::brick_profile::SCREEN,
            Model::Pocket5 | Model::Flip2 | Model::Pocket6 | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal => (1920, 1080),
        }
    }

    /// Not tested on real hardware by the project (everything but the Mini V2, and the TrimUI Brick: About said "TrimUI Brick
    /// (experimental)" on the handheld pocket-ui is released for).
    pub fn experimental(self) -> bool {
        self != Model::MiniV2 && { #[cfg(feature = "brick")] { self != Model::Brick } #[cfg(not(feature = "brick"))] { true } }
    }

    /// The project has played on it (🎮 in docs/DEVICES.md): the first start doesn't call it untested.
    pub fn played_on(self) -> bool {
        match self {
            Model::MiniV2 | Model::Odin2Mini => true,
            Model::H700(b) => matches!(b.key, "rg35xx-plus" | "rg35xx-sp"),
            // Played on since 02.11. (docs/TRIMUI-BRICK.md): the first start called it untested.
            #[cfg(feature = "brick")]
            Model::Brick => true,
            _ => false,
        }
    }

    /// The Flip 2's hall sensor: `SW_LID` on a `gpio-keys` device (docs/SLEEP.md "Lid"); the RG DS Plus's too.
    pub fn has_lid(self) -> bool {
        match self {
            Model::Flip2 | Model::RgDsPlus => true,
            Model::H700(b) => b.lid,
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
        self.is_h700() || self == Model::RgDsPlus
    }

    /// A desktop Linux whose own system owns Wi-Fi, Bluetooth, sleep, fan and updates (the Steam Deck): pocket-ui hides those pages.
    pub fn is_desktop(self) -> bool {
        self == Model::SteamDeck
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
            Model::RgDsPlus => "rgdsplus",
            Model::SteamDeck => "steamdeck", #[cfg(feature = "brick")] Model::Brick => "brick",
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
        self.is_sm8550() || { #[cfg(feature = "brick")] { self.model == Some(Model::Brick) } #[cfg(not(feature = "brick"))] { false } }
    }

    /// The Snapdragon 8 Gen 2 handhelds (Adreno 740).
    pub fn is_sm8550(&self) -> bool {
        matches!(self.model, Some(Model::Pocket6 | Model::Nova | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal))
    }

    /// SteamOS (or another desktop) owns the radios, sleep, fan and updates: Settings leaves those pages out.
    pub fn host_owned(&self) -> bool {
        self.model.is_some_and(Model::is_desktop)
    }

    /// A fan the system can run. None on the H700 handhelds and the RG DS Plus: the fan page says so instead of offering a fan.
    #[cfg(not(feature = "brick"))]
    pub fn has_fan(&self) -> bool {
        !self.model.is_some_and(|m| m.is_h700() || m.is_desktop() || m == Model::RgDsPlus)
    }

    #[cfg(feature = "brick")]
    pub fn has_fan(&self) -> bool {
        !self.model.is_some_and(|m| m.is_h700() || m.is_desktop() || m == Model::RgDsPlus || { #[cfg(feature = "brick")] { m == Model::Brick } #[cfg(not(feature = "brick"))] { false } })
    }

    /// A touch screen. None on the H700 handhelds (the kernel has no touch driver there).
    #[cfg(not(feature = "brick"))]
    pub fn has_touch(&self) -> bool {
        !self.model.is_some_and(Model::is_h700)
    }

    #[cfg(feature = "brick")]
    pub fn has_touch(&self) -> bool {
        !self.model.is_some_and(|m| m.is_h700() || { #[cfg(feature = "brick")] { m == Model::Brick } #[cfg(not(feature = "brick"))] { false } })
    }

    /// A second card slot, for a games card ROCKNIX reads by itself: every H700 handheld has one (TF2; every H700 device
    /// tree in ROCKNIX's image has `mmc@4020000` and `mmc@4022000`, both with `cd-gpios`), and the RG DS Plus (two TF
    /// slots). The SM8250 and SM8550 trees have one card slot (`sdhc_2`).
    pub fn has_second_card_slot(&self) -> bool {
        self.model.is_some_and(|m| m.is_h700() || m == Model::RgDsPlus)
    }

    /// Analog sticks, and so the lights around them. Per model on the H700 handhelds.
    #[cfg(not(feature = "brick"))]
    pub fn has_sticks(&self) -> bool {
        !self.model.is_some_and(Model::is_desktop) && !matches!(self.model, Some(Model::H700(b)) if !b.sticks)
    }

    #[cfg(feature = "brick")]
    pub fn has_sticks(&self) -> bool {
        !self.model.is_some_and(Model::is_desktop) && !{ #[cfg(feature = "brick")] { self.model == Some(Model::Brick) } #[cfg(not(feature = "brick"))] { false } } && !matches!(self.model, Some(Model::H700(b)) if !b.sticks)
    }

    /// A Wi-Fi chip. Not on the RG28XX and RG35XX 2024 (their device trees enable one anyway, the Plus tree they
    /// come from, so a chip that does answer still wins: `system_ctl::AutoHw` only calls it absent when none shows up).
    #[cfg(not(feature = "brick"))]
    pub fn has_wifi(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b)) if !b.wifi) && !self.host_owned()
    }

    #[cfg(feature = "brick")]
    pub fn has_wifi(&self) -> bool {
        !matches!(self.model, Some(Model::H700(b)) if !b.wifi) && !self.host_owned() && !{ #[cfg(feature = "brick")] { self.model == Some(Model::Brick) && !crate::brick_profile::standalone() } #[cfg(not(feature = "brick"))] { false } }
    }

    /// Memory in MB where the device is known to have less than the Mini V2's (1 GB on the H700 and the RG DS Plus, no swap).
    #[cfg(not(feature = "brick"))]
    pub fn ram_mb(&self) -> Option<u32> {
        self.model.filter(|m| m.small_chip()).map(|_| 1024)
    }

    #[cfg(feature = "brick")]
    pub fn ram_mb(&self) -> Option<u32> {
        self.model.filter(|m| m.small_chip() || { #[cfg(feature = "brick")] { *m == Model::Brick } #[cfg(not(feature = "brick"))] { false } }).map(|_| 1024)
    }

    /// Printed face letters on AYN's product images; saved menu choices still win.
    pub fn menu_layout(&self) -> crate::button_layout::ButtonLayout {
        use crate::button_layout::ButtonLayout;
        match self.model {
            // The Deck's A is the bottom button (Steam Input's Gamepad template: BTN_SOUTH), as on an Xbox pad.
            Some(Model::Odin2Mini | Model::Odin2Portal | Model::SteamDeck) => ButtonLayout::Xbox,
            _ => ButtonLayout::Nintendo,
        }
    }

    /// Settings → Playing → Buttons → Button symbols when unset (`button_prompts`): Xbox-style caps on
    /// Retroid's and AYN's handhelds, plain letters on Anbernic's and anything unknown. A saved choice wins.
    pub fn prompt_style(&self) -> crate::button_prompts::PromptStyle {
        use crate::button_prompts::PromptStyle;
        match self.model {
            Some(Model::H700(_) | Model::RgDsPlus) | None => PromptStyle::Nintendo,
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
        "Anbernic RG DS Plus" => Some(Model::RgDsPlus),
        other => h700_board(other, &[]).map(Model::H700),
    });
    let found = found.or_else(|| (model == "Steam Deck").then_some(Model::SteamDeck));
    let panel = if model.ends_with(" Visionox") { Panel::Visionox } else { Panel::Original };
    Device { model: found, panel, reported: model.to_string() }
}

/// `POCKETUI_DEVICE`'s value: a short name or a `model` string.
pub fn from_override(v: &str) -> Device {
    let v = v.trim(); #[cfg(feature = "brick")] if v.eq_ignore_ascii_case("brick") { return Device { model: Some(Model::Brick), panel: Panel::Original, reported: format!("{v} (POCKETUI_DEVICE)") }; }
    let lower = v.to_ascii_lowercase();
    let (key, panel) = match lower.strip_suffix("-visionox") {
        Some(k) => (k, Panel::Visionox),
        None => (lower.as_str(), Panel::Original),
    };
    match [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova, Model::Odin2, Model::Odin2Mini, Model::Odin2Portal, Model::RgDsPlus, Model::SteamDeck]
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
    // A Steam Deck is an x86 PC: no device tree, but DMI says Valve and Jupiter (LCD) or Galileo (OLED).
    let dmi = |f: &str| std::fs::read_to_string(root.join("sys/class/dmi/id").join(f)).unwrap_or_default().trim().to_string();
    if dmi("sys_vendor") == "Valve" && matches!(dmi("product_name").as_str(), "Jupiter" | "Galileo") {
        return Device { model: Some(Model::SteamDeck), panel: Panel::Original, reported: format!("Valve {}", dmi("product_name")) };
    }
    let base = root.join("sys/firmware/devicetree/base"); #[cfg(feature = "brick")] if crate::brick_profile::detected(root) { return Device { model: Some(Model::Brick), panel: Panel::Original, reported: crate::brick_profile::REPORTED.to_string() }; }
    let model = std::fs::read_to_string(base.join("model")).unwrap_or_default();
    let compatible = std::fs::read(base.join("compatible")).unwrap_or_default();
    identify(&model, &compatible)
}

/// Cooling safety also recognises an otherwise unknown SM8550 board; do not inherit SM8250 cooling there.
#[cfg(not(feature = "brick"))]
pub fn rocknix_cooling(root: &Path) -> bool {
    (root == Path::new("/") && current().screen_off_only())
        || std::fs::read(root.join("sys/firmware/devicetree/base/compatible")).is_ok_and(|b| b.split(|v| *v == 0).any(|c| c == b"qcom,sm8550"))
        || std::fs::read_to_string(root.join("etc/os-release")).is_ok_and(|s| s.lines().any(|l| l.starts_with("HW_DEVICE=") && l.contains("SM8550")))
}

#[cfg(feature = "brick")]
pub fn rocknix_cooling(root: &Path) -> bool {
    (root == Path::new("/") && current().is_sm8550())
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

/// Settings pages and feature cards a device lacks, by key: the one list behind the Settings rows (`app_settings_groups`)
/// and the Tour / first-start cards (`features::available`). `leds` is the lights around the sticks; the rest are what
/// SteamOS owns or has nothing to work with (docs/STEAMDECK.md). Keys are `features::Feature::id` where there is a card.
pub fn hides(key: &str, sticks: bool, host_owned: bool) -> bool {
    match key {
        "leds" => !sticks,
        "haptics" | "usb_transfer" | "sync" | "tailscale" | "tools" | "perf_fan" | "radios" | "time" | "updates" | "store" | "kids" => host_owned,
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
    hides(key, d.has_sticks(), d.host_owned())
}

#[cfg(feature = "brick")]
pub fn hides_here(key: &str) -> bool {
    let d = current();
    #[cfg(feature = "brick")]
    if d.model == Some(Model::Brick) && brick_card_hidden(key, crate::brick_profile::standalone()) {
        return true;
    }
    // The Brick's own card (no NextUI): the Wi-Fi page stays, there is a radio and a client of ours; so do the clock (`brick_standalone_keeps`),
    // Tailscale, Updates (its private channel) and Kids mode, which is pocket-ui's alone (docs/TRIMUI-BRICK-STANDALONE.md, docs/TRIMUI-BRICK-MORE.md).
    // On every Brick the motor and the lights are its own (`brick_hw`): Haptics and LEDs and their Tour cards stay, though it has no sticks.
    #[cfg(feature = "brick")]
    if d.model == Some(Model::Brick)
        && (brick_standalone_keeps(key, crate::brick_profile::standalone())
            || crate::brick_profile::standalone() && matches!(key, "tailscale" | "updates" | "kids" | "perf_fan")
            || matches!(key, "haptics" | "leds"))
    {
        return false;
    }
    hides(key, d.has_sticks(), d.host_owned() || { #[cfg(feature = "brick")] { d.model == Some(Model::Brick) } #[cfg(not(feature = "brick"))] { false } })
}

/// The pages the Brick's own card (no NextUI) keeps though the Brick counts as host-owned for the rest: the Wi-Fi page
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

    /// The Steam Deck is an x86 PC: DMI says Valve / Jupiter (LCD) or Galileo (OLED), the override names it, and no other
    /// device is host-owned.
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
        assert!(Model::Brick.played_on());
        let doc = include_str!("../docs/DEVICES.md");
        assert!(doc.lines().any(|l| l.starts_with("| 🎮") && l.contains(Model::Brick.name()) && l.contains("1024 × 768")));
    }

    #[test]
    fn the_steam_deck_is_told_by_dmi_and_owns_nothing_of_the_handhelds() {
        let dir = TestDir::new("device-deck");
        let id = dir.join("sys/class/dmi/id");
        std::fs::create_dir_all(&id).unwrap();
        std::fs::write(id.join("sys_vendor"), "Valve\n").unwrap();
        std::fs::write(id.join("product_name"), "Galileo\n").unwrap();
        let d = read(&dir);
        assert_eq!(d.model, Some(Model::SteamDeck));
        assert_eq!(d.screen(), (1280, 800));
        assert_eq!(d.menu_layout(), crate::button_layout::ButtonLayout::Xbox, "the Deck's A is at the bottom");
        assert!(d.host_owned() && !d.has_fan() && !d.has_wifi() && !d.has_sticks());
        std::fs::write(id.join("sys_vendor"), "Dell Inc.\n").unwrap();
        assert_eq!(read(&dir).model, None, "another PC is not a Deck");
        assert_eq!(from_override("steamdeck").model, Some(Model::SteamDeck));
        for m in [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova, Model::Odin2, Model::H700(&H700_BOARDS[0])] {
            assert!(!Device { model: Some(m), panel: Panel::Original, reported: String::new() }.host_owned());
        }
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
        for key in ["store", "updates", "tools", "perf_fan", "tailscale", "sync", "usb_transfer"] {
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
        // Only the 720x720 CubeXX is held back; the installers read the same flag.
        assert_eq!(H700_BOARDS.iter().filter(|b| b.not_yet).map(|b| b.key).collect::<Vec<_>>(), ["rgcubexx"]);
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
        assert!(include_str!("../deploy/package/install.sh").contains("MODELS_RK3566=\"Anbernic RG DS Plus\""));
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

/// The TrimUI Brick under NextUI (docs/TRIMUI-BRICK.md): Settings leaves out what NextUI owns, as on the Steam Deck, without
/// the Deck's other host_owned behaviour (windowing through `pocket-fb`, no Steam texts).
#[cfg(feature = "brick")]
pub fn brick() -> bool {
    current().model == Some(Model::Brick)
}
