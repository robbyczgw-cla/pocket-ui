//! Pure device profiles used by shared art; Linux hardware detection is in device.rs.
/// The handhelds pocket-ui knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    /// Retroid Pocket Mini V2: 1240x1080, the device pocket-ui is tested on.
    MiniV2,
    /// The first Retroid Pocket Mini: 1280x960 (4:3; the panel scales internally, see docs/DEVICES.md).
    MiniV1,
    /// Retroid Pocket 5: 1920x1080.
    Pocket5,
    /// SM4450 Classic Rev Q: 1240x1080, touch, no sticks; ROCKNIX owns cooling.
    PocketClassic,
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
    /// Anbernic RG DS Plus (ROCKNIX's RK3566 image; an RK3568, 1 GB): two 1024x768 panels, one above the other, touch only
    /// on the lower one, a lid, one stick, no fan. pocket-ui uses the main output.
    RgDsPlus,
    /// A one-screen handheld of ROCKNIX's RK3566 image on the Generic U-Boot (RG353 family, RG503, RGB30, RK2023): one
    /// profile like the H700's (1 GB counted, no fan; Cortex-A55), one row per model in `RK3566_BOARDS`.
    Rk3566(&'static H700Board),
    /// A handheld of ROCKNIX's RK3326 image (the R36S): four Cortex-A35, 1 GB, no fan, our slowest chip. The card boots the
    /// R36S tree by name (ROCKNIX's U-Boot script has no R36S branch, docs/SD-IMAGE-RK3326.md); one row in `RK3326_BOARDS`.
    Rk3326(&'static H700Board),
    #[cfg(feature = "brick")] Brick(crate::brick_profile::Board),
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
}

const fn board(key: &'static str, name: &'static str, compatible: &'static [&'static str], screen: (u32, u32), sticks: bool, wifi: bool, lid: bool) -> H700Board {
    H700Board { key, name, compatible, screen, sticks, wifi, lid }
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
    // Square: laid out on 1440x1440 and halved like the others (`scaled_layout::logical_size`).
    board("rgcubexx", "Anbernic RG CubeXX", &["anbernic,rgcubexx"], (720, 720), true, true, false),
];

/// The H700 model with this `compatible` entry or device tree `model`, if the profile table has it.
pub fn h700_board(model: &str, compatible: &[&str]) -> Option<&'static H700Board> {
    let by_compatible = compatible.iter().find_map(|c| {
        let c = c.trim().to_ascii_lowercase();
        H700_BOARDS.iter().find(|b| b.compatible.contains(&c.as_str()))
    });
    by_compatible.or_else(|| H700_BOARDS.iter().find(|b| b.name.eq_ignore_ascii_case(model)))
}

/// The one-screen handhelds of ROCKNIX's RK3566 image that its Generic U-Boot tells apart (`doc/board/anbernic/rgxx3.rst` in
/// U-Boot; ROCKNIX 20261101's `device_trees/`), in the same shape as `H700_BOARDS` (docs/DEVICES.md, "RK3566"). Sizes are the
/// panels' (rocknix.org's device pages). The RG353M has no tree of its own: U-Boot gives it the RG353P's and only changes
/// `model`, so it is found by `model` first. Wi-Fi: the RK2023's chip is optional (a chip that answers wins either way).
/// The RG353M/P/V start a ROCKNIX card only with Android wiped from their eMMC (ROCKCHIP-NATIVE.md); the profile is the same.
pub const RK3566_BOARDS: &[H700Board] = &[
    board("rg353m", "Anbernic RG353M", &[], (640, 480), true, true, false),
    board("rg353p", "Anbernic RG353P", &["anbernic,rg353p"], (640, 480), true, true, false),
    board("rg353ps", "Anbernic RG353PS", &["anbernic,rg353ps"], (640, 480), true, true, false),
    board("rg353v", "Anbernic RG353V", &["anbernic,rg353v"], (640, 480), true, true, false),
    board("rg353vs", "Anbernic RG353VS", &["anbernic,rg353vs"], (640, 480), true, true, false),
    // 5", 960x544 (OLED): no layout of its own, the canvas is scaled down from 1694x960 (`scaled_layout::logical_size`).
    board("rg503", "Anbernic RG503", &["anbernic,rg503"], (960, 544), true, true, false),
    // Square like the CubeXX: laid out on 1440x1440 and halved. ROCKNIX's quirk boots a second RGB30 revision (another
    // Wi-Fi chip) on the RGB20SX's tree, so that row is here too.
    board("rgb30", "Powkiddy RGB30", &["powkiddy,rgb30"], (720, 720), true, true, false),
    board("rgb20sx", "Powkiddy RGB20SX", &["powkiddy,rgb20sx"], (720, 720), true, true, false),
    board("rk2023", "Powkiddy RK2023", &["powkiddy,rk2023"], (640, 480), true, false, false),
];

/// The RK3566 model with this device tree `model` or `compatible` entry: `model` first (the RG353M), as `h700_board` the other way round.
pub fn rk3566_board(model: &str, compatible: &[&str]) -> Option<&'static H700Board> {
    let by_model = RK3566_BOARDS.iter().find(|b| !model.trim().is_empty() && b.name.eq_ignore_ascii_case(model.trim()));
    by_model.or_else(|| {
        compatible.iter().find_map(|c| {
            let c = c.trim().to_ascii_lowercase();
            RK3566_BOARDS.iter().find(|b| b.compatible.contains(&c.as_str()))
        })
    })
}

/// Which key is HOME on an RK3566 board under ROCKNIX's InputPlumber (its `anbernic_gen1` and `powkiddy` maps, 20261101):
/// the F key where there is one (it comes as the InputPlumber keyboard's F1, or `BTN_BACK` without InputPlumber), else the
/// right stick's click, as pocket-ui's dArkOS port does on boards without a menu key. InputPlumber's own Guide on these
/// boards is a copy of SELECT (`pad_linux_decode`'s echo filter drops it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rk3566Home {
    FKey,
    RightStickClick,
}

pub fn rk3566_home(b: &H700Board) -> Rk3566Home {
    match b.key {
        "rgb30" | "rgb20sx" | "rk2023" => Rk3566Home::RightStickClick,
        _ => Rk3566Home::FKey,
    }
}

/// The handhelds of ROCKNIX's RK3326 image pocket-ui has a profile for (docs/DEVICES.md, "RK3326"): the R36S, read from
/// ROCKNIX's `rk3326-gameconsole-r36s.dts` (tag 20261001; model "Game Console R36S", compatible `gameconsole,r35s`,
/// `gameconsole,r36s`, pad `r36s_Gamepad` with two sticks, L2/R2 as keys and FN as BTN_MODE). 640x480 on every panel
/// revision (the 2024 panels only need another overlay, the screen size stays). No Wi-Fi by the maker's specification;
/// a chip that answers (some later boards, a USB dongle) still wins (`system_ctl::AutoHw`). The R35S shares the tree.
pub const RK3326_BOARDS: &[H700Board] = &[
    board("r36s", "Game Console R36S", &["gameconsole,r36s"], (640, 480), true, false, false),
];

/// The RK3326 model with this device tree `model` or `compatible` entry.
pub fn rk3326_board(model: &str, compatible: &[&str]) -> Option<&'static H700Board> {
    let by_compatible = compatible.iter().find_map(|c| {
        let c = c.trim().to_ascii_lowercase();
        RK3326_BOARDS.iter().find(|b| b.compatible.contains(&c.as_str()))
    });
    by_compatible.or_else(|| RK3326_BOARDS.iter().find(|b| !model.trim().is_empty() && b.name.eq_ignore_ascii_case(model.trim())))
}

/// The Android player (`android_modules.rs`, where this file is `device`): phones and Android handhelds have Wi-Fi, so
/// the words for a handheld without it (`device::no_wifi_chip` on Linux) are never chosen there.
#[cfg(all(target_os = "android", not(feature = "handheld")))]
pub fn no_wifi_chip() -> bool {
    false
}

#[cfg(all(target_os = "android", not(feature = "handheld")))]
pub const NEEDS_NETWORK: &str = "Needs a network, and this handheld has no Wi-Fi.";

#[cfg(all(target_os = "android", not(feature = "handheld")))]
pub fn wifi_or<'a>(wifi: &'a str, _none: &'a str) -> &'a str {
    wifi
}

/// The Mobile view's player (Android, this module as `device`): Android cools itself, there is no fan to set
/// (`device::no_fan` on the Linux side, review 19.11. #2).
#[cfg(all(target_os = "android", not(feature = "handheld")))]
pub fn no_fan() -> bool {
    true
}
