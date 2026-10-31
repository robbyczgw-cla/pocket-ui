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
    /// Valve Steam Deck (SteamOS, Gamescope): 1280x800, a desktop Linux that owns Wi-Fi, Bluetooth, sleep and fan itself
    /// (docs/STEAMDECK.md). Found by `POCKETUI_DEVICE=steamdeck` or DMI (`Valve`: `Jupiter`, `Galileo`).
    SteamDeck,
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
