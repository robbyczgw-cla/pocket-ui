//! "The case knows where you are" (docs/user/library.md, "Where you are in the game"): one short line read out of a
//! game's own battery save ("8 badges · Charizard Lv. 45 · game clock 31:12", "62% items · game clock 4:31", "43 of 96 exits"), shown
//! on the back of its box, beside Continue and on the "Where was I?" card. No setting, no account, no network.
//!
//! This module is the reading: which releases are known ([`RELEASES`], by the CRC-32 of the ROM as No-Intro lists
//! it), and for each the save's layout, checked with the game's own checksum before a single value is believed.
//! Anything else (a hack, a translation, a revision not listed, a save the game itself would call damaged, a value
//! the game can't have) reads as nothing, never as a wrong line. Only battery saves (`.srm`/`.sav`), never save
//! states: the same bytes on every device and every emulator. Files, the cache and the thread are
//! `case_knows_read`'s.
//!
//! Where the layouts come from (read 2026-11-13; facts only, no code taken):
//! * Pokémon, generations 1 to 3: PKHeX (`SAV1Offsets`, `SAV2Offsets`, `SAV3`, `SaveBlock3*`, `PK3`,
//!   `StringConverter1/2/3`), the pret disassemblies for the sector footer.
//! * Super Metroid: the community disassembly of the US ROM (bank logs): `$81:8000` (save), `$81:812B` (slot offsets), `$81:A23C` (selected slot),
//!   `$8B:E627` (the ending's item percentage).
//! * Metroid: Zero Mission: the metroidret decomp (`struct Sram`, `SramWrite_HeaderAndGameInfo`,
//!   `ChozodiaEscapeGetItemCountAndEndingNumber`, `sTankIncreaseAmount`), offsets taken by compiling its headers with
//!   every struct on 4 bytes, as the game's compiler lays them out.
//! * Super Mario 64: n64decomp/sm64 (`struct SaveBuffer`, `verify_save_block_signature`, `save_file_get_total_star_count`).
//! * Ocarina of Time: zeldaret/oot (`include/save.h` with its offsets, `include/item.h`, `z_sram.c`).
//! * Majora's Mask (USA): zeldaret/mm (`include/z64save.h` with its offsets, `z_sram_NES.c`: flash pages, sizes,
//!   `Sram_CalcChecksum`, the file select's reading; `z64item.h`).
//! * Paper Mario: pmret/papermario (`fio.c`, `struct SaveData`, `SaveGlobals`, `SaveMetadata`, `filemenu_main.c`).
//! * Mario Kart 64: n64decomp/mk64 (`include/save_data.h`, `save.c` with its two check bytes).
//! * Star Fox 64: sonicdcer/sf64 (`include/sf64save.h`, `engine/fox_save.c`).
//! * Diddy Kong Racing: DavidSM64/Diddy-Kong-Racing (`include/save_layout.h`, `src/save_data.c`).
//! * Final Fantasy III (USA): everything8215/ff6 (`menu/save.asm`: slots, checksum, chosen slot), Data Crystal's RAM
//!   map (characters, parties, play time).
//! * EarthBound: Herringway/ebsrc (`include/structs.asm` with its offsets, `system/saves/` checksums).
//! * Link's Awakening DX: zladx/LADX-Disassembly (`constants/memory/sram.asm`, `wram.asm` with its addresses).
//! * Oracle of Ages and Oracle of Seasons (USA): Stewmath/oracles-disasm (`code/fileManagement.s`: places, checksum,
//!   the games' strings; `include/wram.s` with its addresses for each game).
//! * The Minish Cap: zeldaret/tmc (`save.c`: places, status, checksum; `save.h`: `SaveFile`, `SaveHeader`;
//!   `playerUtils.c`: `GetInventoryValue`; `item.h`).
//! * Pokémon, generations 4 and 5 (DS): PKHeX (`SAV4`, `SAV4DP`, `SAV4Pt`, `SAV4HGSS`, `SAV4BlockDetection`,
//!   `SAV5`, `SaveBlockAccessor5BW`/`B2W2`, `PlayerData5`, `Misc5`, `PokeCrypto`, `PK4`, `PK5`, `StringConverter4Util`,
//!   `StringConverter5`, `Checksums.CRC16_CCITT`); the block footers and CRC checked against a real HeartGold save.
//! * Super Mario World: SMWDisX, `SaveTheGame`, `VerifySaveFile`, `DrawFileExitCount`; the same offsets in U, E, J.
//! * Fire Emblem (USA) and The Sacred Stones (USA): the matching decomps 7ucina/fireemblem7u and
//!   FireEmblemUniverse/fireemblem8u (`sram-layout.h`/`gbafe/save.h`, `bmsave-lib.c`: header, block entries, magics,
//!   `Checksum16`; `Checksum32` in `crt0.s`/`arm.s`; `struct PlaySt`, `constants/chapters.h`, `savemenu.c` for the
//!   modes, `time.c` for the clock).
//! * Donkey Kong Country 1, 2, 3 (USA): Yoshifanatic1's DKC1 disassembly (`CODE_B896AC`/`B896D6`: the check,
//!   the file built at $7E:79FC with "ARER" and the share at +0x0E), p4plus2/DKC2-disassembly (`sram.asm`,
//!   `structs.asm` `save_file`, `validate_save_file`, the save at `$BB:C651`), BlueImp78/DKC3-Disassembly (`bank_80.asm`
//!   `CODE_809141`/`809177`, the file's clock and share at `$80:9250`).
//! * Chrono Trigger (USA): Data Crystal's RAM map (its SRAM part: slots, "last save", party, play time), the save
//!   editors mcred/chrono-trigger-save-editor and mikearnos/snessum (the sum from the slot's end with the carry, the
//!   used mark 0xE41B, the names' letters, the characters' blocks); the sum checked against their real test saves.
//! * Golden Sun and The Lost Age: Shmargus/GSRecomp, built from the games' own routines (`save_files_picker.h`,
//!   `gs_save.cpp`, `save_files_tbs.h`, `tla_save_files.cpp`: "CAMELOT" records, the byte sum, the newer copy, the
//!   file screen's leader and level); the clock where the games' own file screens read it (a test save in the games).
//! * Kirby & the Amazing Mirror (USA): the matching decomp jiangzhengwenjz/katam (`save.c`: the two kinds of block,
//!   their places, the copies and `CalculateSaveChecksum`; `save.h`, `treasures.h`/`treasures.c`: the shards' bits; the
//!   blocks' parts by their sizes in the ROM's own table `gWorldProps`).
//! * Super Mario RPG (USA): Yoshifanatic1's disassembly (`BWRAM_Map_SMRPG.asm`, the save routine and its sum), checked
//!   against the ROM and against a save the game wrote in a headless emulator.
//! * Lufia II (USA): the game's save, load and random-number routines read from the ROM (`$80:82E7`, `$80:914B`),
//!   Cellenseres/Lufia2Decomp; checked against saves the game wrote.
//! * Pokémon Mystery Dungeon: Red Rescue Team (USA): pret/pmd-red (`save.c`: the copies, `ValidateChecksum`, the
//!   magic; `data_serializer.c`, `pokemon_3.c`, `play_time.c`: the bit streams); checked against a save the game wrote.
//! * Final Fantasy VI Advance (USA): Janne Hellas' FF6 save importer (its checksum updater and the slot it writes) and
//!   the SNES editor FF3h for the shared slot; the party and the slot saved last from saves the game wrote.
//! * Tactics Ogre: The Knight of Lodis (USA): jiangzhengwenjz/totkol (`sub_08002694`, `sub_08002A24`, `sub_080030E0`:
//!   the sectors, the CRC, the newest save; `sub_0815267C`: the file screen's summary); checked against a save the game wrote.
//! * Breath of Fire II (USA): the game's save and load routines read from the ROM (`$C0:3847`: the three slots, the
//!   "SAN!BOF2" mark, the word sum; the file select's hero); checked against a save the game wrote.
//! * Mario & Luigi: Superstar Saga (USA): jellees/mlss (`common.h`: `SaveSlot`, `SaveSlotData`; the load and save
//!   routines `sub_81233B4`, `sub_8123BA8`: "MALRPG", the files' marks and sums; the play time); the brothers' levels
//!   where the game's LOAD screen reads them, found with a save the game wrote.
//! * Mother 3 (Japan, played with the English fan translation): Data Crystal's SRAM map and the Kurausukun/mother3
//!   decomp (`sub_08052184`: the banks, `sub_08001DC8`: the sum); the Load screen's summaries in the game's own bank,
//!   the translation's letters and the file saved last found with saves the game wrote.
//! * Banjo-Kazooie: banjo.decomp/banjo-kazooie (`savedata_init`, `gameFile_8033CE40`, `jiggyscore.c`, the notes' and
//!   clock's fields in `code_BEF20.c`, `gameSelect.c`), bryc/rare-n64-chksm for the sum; checked against saves the game
//!   wrote (USA Rev 1) and its game select.
//! * Metroid Fusion: metroidret/mf (`sSaveDataSramPointers`, `struct Equipment`, `struct InGameTimer`, the ending's
//!   `completionPercentage`); the file's checks, places and the file chosen last from saves the game wrote (its own save
//!   routine, `Sram_QuickSave`, called from the pause screen's debug page), as in Zero Mission.
//! * GoldenEye 007: n64decomp/007 (`file2.c`: `save_data`, the slots at `joyGamePakLongWrite`, `fileValidateSaves`,
//!   `fileGetSaveStageDifficultyTime`, `fileGetHighestStageDifficultyCompletedForFolder`, `fileIs007ModeUnlocked`;
//!   `front.c`: the folder select's text and `mission_folder_setup_entries`); checked against saves the game wrote.
//! * A Link to the Past: snesrev/zelda3, `select_file.c` (`Intro_CheckCksum`, the `0x55AA` mark), `variables.h`.

/// The games this reads, by the layout of their save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Pokémon Red, Blue and Yellow.
    Gen1,
    /// Pokémon Gold and Silver.
    GoldSilver,
    Crystal,
    RubySapphire,
    Emerald,
    FireRedLeafGreen,
    SuperMetroid,
    /// Metroid: Zero Mission (GBA).
    ZeroMission,
    MarioWorld,
    /// Super Mario All-Stars + Super Mario World: Super Mario World's files.
    AllStarsWorld,
    LinkToThePast,
    /// Super Mario 64 (its EEPROM).
    Mario64,
    /// The Legend of Zelda: Ocarina of Time (its SRAM).
    Ocarina,
    /// The Legend of Zelda: Majora's Mask.
    MajorasMask,
    /// Paper Mario (its flash).
    PaperMario,
    /// Mario Kart 64 (its EEPROM).
    MarioKart64,
    /// Star Fox 64 / Lylat Wars (its EEPROM).
    StarFox64,
    /// Diddy Kong Racing (its EEPROM).
    DiddyKongRacing,
    /// Final Fantasy III (USA), the sixth Final Fantasy.
    FinalFantasy6,
    EarthBound,
    /// The Legend of Zelda: Link's Awakening DX.
    LinksAwakening,
    /// The Legend of Zelda: The Minish Cap.
    MinishCap,
    /// The Legend of Zelda: Oracle of Ages, Oracle of Seasons.
    OracleOfAges,
    OracleOfSeasons,
    /// Fire Emblem (GBA, the seventh: Lyn's, Eliwood's and Hector's stories).
    FireEmblem7,
    /// Fire Emblem: The Sacred Stones (the eighth).
    FireEmblem8,
    /// Donkey Kong Country 1, 2 and 3 (SNES).
    DonkeyKong1,
    DonkeyKong2,
    DonkeyKong3,
    ChronoTrigger,
    /// Golden Sun and Golden Sun: The Lost Age (GBA).
    GoldenSun,
    GoldenSunLostAge,
    /// Kirby & the Amazing Mirror (GBA).
    KirbyMirror,
    /// Super Mario RPG: Legend of the Seven Stars (SNES).
    MarioRpg,
    /// Lufia II: Rise of the Sinistrals (SNES).
    Lufia2,
    /// Pokémon Mystery Dungeon: Red Rescue Team (GBA).
    MysteryDungeonRed,
    /// Final Fantasy VI Advance (GBA).
    FinalFantasy6Advance,
    /// Tactics Ogre: The Knight of Lodis (GBA).
    TacticsOgre,
    /// Breath of Fire II (SNES).
    BreathOfFire2,
    /// Mario & Luigi: Superstar Saga (GBA).
    SuperstarSaga,
    /// Mother 3 (GBA), with the fan translation into English.
    Mother3,
    /// Banjo-Kazooie (N64, its EEPROM).
    BanjoKazooie,
    /// Metroid Fusion (GBA).
    MetroidFusion,
    /// GoldenEye 007 (N64, its EEPROM).
    GoldenEye,
    /// Pokémon Diamond and Pearl (DS).
    DiamondPearl,
    Platinum,
    /// Pokémon HeartGold and SoulSilver.
    HeartGoldSoulSilver,
    /// Pokémon Black and White.
    BlackWhite,
    /// Pokémon Black 2 and White 2.
    BlackWhite2,
}

/// A release's language, for the letters of a Pokémon's name (the Game Boy games' tables differ a little).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    De,
    Fr,
    Es,
    It,
}

/// One ROM pocket-ui reads the save of: its CRC-32 as No-Intro lists it (the file without a copier header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    pub crc: u32,
    pub kind: Kind,
    pub lang: Lang,
    /// The No-Intro name, for the docs and the tests.
    pub name: &'static str,
}

impl Release {
    /// Whether pocket-ui shows a line for this release. Only games whose reader was checked against saves the game
    /// itself wrote (docs/CASE-KNOWS.md, "Checked against the games"), and only the region checked: a USA ROM. One
    /// region's versions and revisions share their save code. The others keep their reading and their tests and come
    /// back here once a save the game made confirms them: a wrong line is worse than none.
    pub fn verified(&self) -> bool {
        // Every kind but the Japanese-only ones is here now (15.11.): checked against saves the games wrote, Paper Mario,
        // Majora's Mask, Diddy Kong Racing and Golden Sun with the games made to save headless, Super Mario World and the
        // DS games with saves real players shared, read as the games' own screens show them. A game added later shows
        // nothing until its row is here.
        let checked = matches!(
            self.kind,
            Gen1 | GoldSilver
                | Crystal
                | RubySapphire
                | Emerald
                | FireRedLeafGreen
                | SuperMetroid
                | ZeroMission
                | AllStarsWorld
                | LinkToThePast
                | Mario64
                | Ocarina
                | MarioKart64
                | StarFox64
                | FinalFantasy6
                | EarthBound
                | LinksAwakening
                | MinishCap
                | OracleOfAges
                | OracleOfSeasons
                | FireEmblem7
                | FireEmblem8
                | DonkeyKong1
                | DonkeyKong2
                | DonkeyKong3
                | ChronoTrigger
                | KirbyMirror
                | MarioRpg
                | Lufia2
                | MysteryDungeonRed
                | FinalFantasy6Advance
                | TacticsOgre
                | BreathOfFire2
                | SuperstarSaga
                | Mother3
                | MarioWorld
                | PaperMario
                | MajorasMask
                | DiddyKongRacing
                | GoldenSun
                | GoldenSunLostAge
                | DiamondPearl
                | Platinum
                | HeartGoldSoulSilver
                | BlackWhite
                | BlackWhite2
                | BanjoKazooie
                | MetroidFusion
                | GoldenEye
        );
        // Mother 3 never came out in the USA: the fan translation's ROM that was checked is its release here. Diddy Kong
        // Racing's European cartridge (Rev 1) was checked too, against a save it wrote: the same EEPROM layout.
        checked && (self.name.contains("USA") || self.kind == Mother3 || (self.kind == DiddyKongRacing && self.name.contains("(Europe)")))
    }
}

const fn r(crc: u32, kind: Kind, lang: Lang, name: &'static str) -> Release {
    Release { crc, kind, lang, name }
}

/// A DS release, by the four-letter game code of its cartridge header instead of a CRC (see [`ds_key`]).
const fn ds(code: &[u8; 4], kind: Kind, lang: Lang, name: &'static str) -> Release {
    r(u32::from_be_bytes(*code), kind, lang, name)
}

/// A DS game's key in [`RELEASES`]: its game code ("CPUE"), from the first 0x160 bytes of the ROM, when the header's
/// own CRC-16 is right. DS Pokémon ROMs are 64 to 512 MiB, more than the identify cache reads through (`rom_id::MAX_HASH`),
/// and trimmed dumps never match No-Intro's sum; the code is the same in every revision and every dump.
pub fn ds_key(header: &[u8]) -> Option<u32> {
    let h = header.get(..0x160)?;
    let sum = h[..0x15E].iter().fold(0xFFFFu16, |mut c, &b| {
        c ^= b as u16;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
        }
        c
    });
    (sum == u16le(h, 0x15E) && h[0x0C..0x10].iter().all(u8::is_ascii_alphanumeric)).then(|| u32::from_be_bytes([h[0x0C], h[0x0D], h[0x0E], h[0x0F]]))
}

use Kind::*;
use Lang::*;

/// Every release read, from libretro-database's No-Intro lists (2026-11-13). Japanese releases are left out: their
/// saves are laid out differently. Re-releases (Switch, Virtual Console), betas, demos, pirates, hacks: not here.
pub const RELEASES: &[Release] = &[
    r(0x9F7FDD53, Gen1, En, "Pokemon - Red Version (USA, Europe) (SGB Enhanced)"),
    r(0xD6DA8A1A, Gen1, En, "Pokemon - Blue Version (USA, Europe) (SGB Enhanced)"),
    r(0x7D527D62, Gen1, En, "Pokemon - Yellow Version - Special Pikachu Edition (USA, Europe) (CGB+SGB Enhanced)"),
    r(0x89197825, Gen1, De, "Pokemon - Rote Edition (Germany) (SGB Enhanced)"),
    r(0x9C336307, Gen1, De, "Pokemon - Blaue Edition (Germany) (SGB Enhanced)"),
    r(0x7A01E45A, Gen1, De, "Pokemon - Gelbe Edition - Special Pikachu Edition (Germany) (CGB+SGB Enhanced)"),
    r(0xD8507D8A, Gen1, Es, "Pokemon - Edicion Roja (Spain) (SGB Enhanced)"),
    r(0xD95416F9, Gen1, Es, "Pokemon - Edicion Azul (Spain) (SGB Enhanced)"),
    r(0x964B7A10, Gen1, Es, "Pokemon - Edicion Amarilla - Edicion Especial Pikachu (Spain) (CGB+SGB Enhanced)"),
    r(0x337FCE11, Gen1, Fr, "Pokemon - Version Rouge (France) (SGB Enhanced)"),
    r(0x50E2FC1D, Gen1, Fr, "Pokemon - Version Bleue (France) (SGB Enhanced)"),
    r(0xD03426E9, Gen1, Fr, "Pokemon - Version Jaune - Edition Speciale Pikachu (France) (CGB+SGB Enhanced)"),
    r(0x2945ACEB, Gen1, It, "Pokemon - Versione Rossa (Italy) (SGB Enhanced)"),
    r(0x4D0984A9, Gen1, It, "Pokemon - Versione Blu (Italy) (SGB Enhanced)"),
    r(0x8B56FE33, Gen1, It, "Pokemon - Versione Gialla - Speciale Edizione Pikachu (Italy) (CGB+SGB Enhanced)"),
    r(0x6BDE3C3E, GoldSilver, En, "Pokemon - Gold Version (USA, Europe) (SGB Enhanced) (GB Compatible)"),
    r(0x8AD48636, GoldSilver, En, "Pokemon - Silver Version (USA, Europe) (SGB Enhanced) (GB Compatible)"),
    r(0x4889DFAA, GoldSilver, De, "Pokemon - Goldene Edition (Germany) (SGB Enhanced) (GB Compatible)"),
    r(0x96C9DB95, GoldSilver, De, "Pokemon - Silberne Edition (Germany) (SGB Enhanced) (GB Compatible)"),
    r(0x3434A92B, GoldSilver, Es, "Pokemon - Edicion Oro (Spain) (SGB Enhanced) (GB Compatible)"),
    r(0x1D9FAAC5, GoldSilver, Es, "Pokemon - Edicion Plata (Spain) (SGB Enhanced) (GB Compatible)"),
    r(0x37A70702, GoldSilver, Fr, "Pokemon - Version Or (France) (SGB Enhanced) (GB Compatible)"),
    r(0xE0C216EA, GoldSilver, Fr, "Pokemon - Version Argent (France) (SGB Enhanced) (GB Compatible)"),
    r(0x4C184CE3, GoldSilver, It, "Pokemon - Versione Oro (Italy) (SGB Enhanced) (GB Compatible)"),
    r(0xCBA6D2D4, GoldSilver, It, "Pokemon - Versione Argento (Italy) (SGB Enhanced) (GB Compatible)"),
    r(0xEE6F5188, Crystal, En, "Pokemon - Crystal Version (USA)"),
    r(0x3358E30A, Crystal, En, "Pokemon - Crystal Version (USA, Europe) (Rev 1)"),
    r(0xBB6DD80C, Crystal, En, "Pokemon - Crystal Version (Australia)"),
    r(0x616D85DE, Crystal, De, "Pokemon - Kristall-Edition (Germany)"),
    r(0xFF0A6F8A, Crystal, Es, "Pokemon - Edicion Cristal (Spain)"),
    r(0x878B2AA7, Crystal, Fr, "Pokemon - Version Cristal (France)"),
    r(0xD45AC039, Crystal, It, "Pokemon - Versione Cristallo (Italy)"),
    r(0xF0815EE7, RubySapphire, En, "Pokemon - Ruby Version (USA, Europe)"),
    r(0x61641576, RubySapphire, En, "Pokemon - Ruby Version (USA, Europe) (Rev 1)"),
    r(0xAEAC73E6, RubySapphire, En, "Pokemon - Ruby Version (USA, Europe) (Rev 2)"),
    r(0x554DEDC4, RubySapphire, En, "Pokemon - Sapphire Version (USA, Europe)"),
    r(0xBAFEDAE5, RubySapphire, En, "Pokemon - Sapphire Version (USA, Europe) (Rev 1)"),
    r(0x9CC4410E, RubySapphire, En, "Pokemon - Sapphire Version (USA, Europe) (Rev 2)"),
    r(0x15E1E280, RubySapphire, De, "Pokemon - Rubin-Edition (Germany)"),
    r(0xCAE89464, RubySapphire, De, "Pokemon - Rubin-Edition (Germany) (Rev 1)"),
    r(0x6FCD7A98, RubySapphire, De, "Pokemon - Saphir-Edition (Germany)"),
    r(0xB0C40C7C, RubySapphire, De, "Pokemon - Saphir-Edition (Germany) (Rev 1)"),
    r(0xEB0729CF, RubySapphire, Es, "Pokemon - Edicion Rubi (Spain)"),
    r(0xB980FFF5, RubySapphire, Es, "Pokemon - Edicion Rubi (Spain) (Rev 1)"),
    r(0xA04F5F0B, RubySapphire, Es, "Pokemon - Edicion Zafiro (Spain)"),
    r(0xF2C88931, RubySapphire, Es, "Pokemon - Edicion Zafiro (Spain) (Rev 1)"),
    r(0x690FD310, RubySapphire, Fr, "Pokemon - Version Rubis (France)"),
    r(0x9F981F72, RubySapphire, Fr, "Pokemon - Version Rubis (France) (Rev 1)"),
    r(0x3581A05F, RubySapphire, Fr, "Pokemon - Version Saphir (France)"),
    r(0x2AE49146, RubySapphire, Fr, "Pokemon - Version Saphir (France) (Rev 1)"),
    r(0xC18231A9, RubySapphire, It, "Pokemon - Versione Rubino (Italy)"),
    r(0x9305E793, RubySapphire, It, "Pokemon - Versione Rubino (Italy) (Rev 1)"),
    r(0x0C7992A9, RubySapphire, It, "Pokemon - Versione Zaffiro (Italy)"),
    r(0x5EFE4493, RubySapphire, It, "Pokemon - Versione Zaffiro (Italy) (Rev 1)"),
    r(0x1F1C08FB, Emerald, En, "Pokemon - Emerald Version (USA, Europe)"),
    r(0x34C9DF89, Emerald, De, "Pokemon - Smaragd-Edition (Germany)"),
    r(0x8C4D3108, Emerald, Es, "Pokemon - Edicion Esmeralda (Spain)"),
    r(0xA3FDCCB1, Emerald, Fr, "Pokemon - Version Emeraude (France)"),
    r(0xA0AEC80A, Emerald, It, "Pokemon - Versione Smeraldo (Italy)"),
    r(0xDD88761C, FireRedLeafGreen, En, "Pokemon - FireRed Version (USA, Europe)"),
    r(0x84EE4776, FireRedLeafGreen, En, "Pokemon - FireRed Version (USA, Europe) (Rev 1)"),
    r(0xD69C96CC, FireRedLeafGreen, En, "Pokemon - LeafGreen Version (USA, Europe)"),
    r(0xDAFFECEC, FireRedLeafGreen, En, "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)"),
    r(0x1A81EEDF, FireRedLeafGreen, De, "Pokemon - Feuerrote Edition (Germany)"),
    r(0xD12F1FDD, FireRedLeafGreen, De, "Pokemon - Blattgruene Edition (Germany)"),
    r(0x9F08064E, FireRedLeafGreen, Es, "Pokemon - Edicion Rojo Fuego (Spain)"),
    r(0x2CA11D59, FireRedLeafGreen, Es, "Pokemon - Edicion Verde Hoja (Spain)"),
    r(0x5DC668F6, FireRedLeafGreen, Fr, "Pokemon - Version Rouge Feu (France)"),
    r(0xBA3285E3, FireRedLeafGreen, Fr, "Pokemon - Version Vert Feuille (France)"),
    r(0x73A72167, FireRedLeafGreen, It, "Pokemon - Versione Rosso Fuoco (Italy)"),
    r(0x16974506, FireRedLeafGreen, It, "Pokemon - Versione Verde Foglia (Italy)"),
    r(0xD63ED5F8, SuperMetroid, En, "Super Metroid (Japan, USA) (En,Ja)"),
    r(0xAD2CBF9C, SuperMetroid, En, "Super Metroid (Europe) (En,Fr,De)"),
    r(0x5C61A844, ZeroMission, En, "Metroid - Zero Mission (USA, Australia)"),
    r(0xF1D92E63, ZeroMission, En, "Metroid - Zero Mission (Europe) (En,Fr,De,Es,It)"),
    r(0xB19ED489, MarioWorld, En, "Super Mario World (USA)"),
    r(0x3C41070F, MarioWorld, En, "Super Mario World (Europe)"),
    r(0xB47F5F20, MarioWorld, En, "Super Mario World (Europe) (Rev 1)"),
    r(0x0EC0DDAC, MarioWorld, En, "Super Mario World - Super Mario Bros. 4 (Japan)"),
    r(0xF84305B1, AllStarsWorld, En, "Super Mario All-Stars + Super Mario World (USA)"),
    // The same 2.5 MiB with zeros to 3 MiB, as older dumps have it.
    r(0x8089624C, AllStarsWorld, En, "Super Mario All-Stars + Super Mario World (USA) (padded to 3 MiB)"),
    r(0x3CE60709, Mario64, En, "Super Mario 64 (USA)"),
    r(0x42C43204, Mario64, En, "Super Mario 64 (USA) (byte-swapped)"),
    r(0x03048DE6, Mario64, En, "Super Mario 64 (Europe) (En,Fr,De)"),
    r(0x11FB579B, Mario64, En, "Super Mario 64 (Europe) (En,Fr,De) (byte-swapped)"),
    r(0xDD801954, Mario64, En, "Super Mario 64 (Japan)"),
    r(0x5CF7952A, Mario64, En, "Super Mario 64 (Japan) (byte-swapped)"),
    r(0xA1E15117, Mario64, En, "Super Mario 64 (Japan) (Rev 3) (Shindou Edition)"),
    r(0xBC9FF5F2, Mario64, En, "Super Mario 64 (Japan) (Rev 3) (Shindou Edition) (byte-swapped)"),
    r(0xCD16C529, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA)"),
    r(0xEC95702D, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA) (byte-swapped)"),
    r(0x3FD2151E, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA) (Rev 1)"),
    r(0xAA07C8BA, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA) (Rev 1) (byte-swapped)"),
    r(0x32120C23, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA) (Rev 2)"),
    r(0x3C7DBF5B, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (USA) (Rev 2) (byte-swapped)"),
    r(0x946FD0F7, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (Europe) (En,Fr,De)"),
    r(0x9852A62F, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (Europe) (En,Fr,De) (byte-swapped)"),
    r(0xA108F6E3, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (Europe) (En,Fr,De) (Rev 1)"),
    r(0xAB0FFBAE, Ocarina, En, "Legend of Zelda, The - Ocarina of Time (Europe) (En,Fr,De) (Rev 1) (byte-swapped)"),
    r(0xB428D8A7, MajorasMask, En, "Legend of Zelda, The - Majora's Mask (USA)"),
    r(0x3EE3F2D1, MajorasMask, En, "Legend of Zelda, The - Majora's Mask (USA) (byte-swapped)"),
    r(0x434389C1, MarioKart64, En, "Mario Kart 64 (USA)"),
    r(0x2BCEE11C, MarioKart64, En, "Mario Kart 64 (USA) (byte-swapped)"),
    r(0xFAA6B083, MarioKart64, En, "Mario Kart 64 (Europe)"),
    r(0x6787E212, MarioKart64, En, "Mario Kart 64 (Europe) (byte-swapped)"),
    r(0x0248F6C3, MarioKart64, En, "Mario Kart 64 (Europe) (Rev 1)"),
    r(0x032625B0, MarioKart64, En, "Mario Kart 64 (Europe) (Rev 1) (byte-swapped)"),
    r(0x5D9696DF, MarioKart64, En, "Mario Kart 64 (Japan)"),
    r(0x4711A9DC, MarioKart64, En, "Mario Kart 64 (Japan) (byte-swapped)"),
    r(0x6CED6472, MarioKart64, En, "Mario Kart 64 (Japan) (Rev 1)"),
    r(0x26450AAB, MarioKart64, En, "Mario Kart 64 (Japan) (Rev 1) (byte-swapped)"),
    r(0xB1FCAA9C, StarFox64, En, "Star Fox 64 (USA)"),
    r(0x363E7EE8, StarFox64, En, "Star Fox 64 (USA) (byte-swapped)"),
    r(0xB1B5FC46, StarFox64, En, "Star Fox 64 (USA) (Rev 1)"),
    r(0xD71902B0, StarFox64, En, "Star Fox 64 (USA) (Rev 1) (byte-swapped)"),
    r(0x411142A7, StarFox64, En, "Star Fox 64 (Japan)"),
    r(0x879EDB89, StarFox64, En, "Star Fox 64 (Japan) (byte-swapped)"),
    r(0x9A3425DA, StarFox64, En, "Lylat Wars (Australia) (En,Fr,De)"),
    r(0xED1249A5, StarFox64, En, "Lylat Wars (Australia) (En,Fr,De) (byte-swapped)"),
    r(0xEB759206, DiddyKongRacing, En, "Diddy Kong Racing (USA) (En,Fr)"),
    r(0x9D2935A1, DiddyKongRacing, En, "Diddy Kong Racing (USA) (En,Fr) (byte-swapped)"),
    r(0x5ACCA298, DiddyKongRacing, En, "Diddy Kong Racing (USA) (En,Fr) (Rev 1)"),
    r(0x96C32BB6, DiddyKongRacing, En, "Diddy Kong Racing (USA) (En,Fr) (Rev 1) (byte-swapped)"),
    r(0x4A13323C, DiddyKongRacing, En, "Diddy Kong Racing (Europe) (En,Fr,De)"),
    r(0x883DF1EA, DiddyKongRacing, En, "Diddy Kong Racing (Europe) (En,Fr,De) (byte-swapped)"),
    r(0xB1E87639, DiddyKongRacing, En, "Diddy Kong Racing (Europe) (En,Fr,De) (Rev 1)"),
    r(0xF05D414F, DiddyKongRacing, En, "Diddy Kong Racing (Europe) (En,Fr,De) (Rev 1) (byte-swapped)"),
    r(0xB566FB94, DiddyKongRacing, En, "Diddy Kong Racing (Japan)"),
    r(0x95931356, DiddyKongRacing, En, "Diddy Kong Racing (Japan) (byte-swapped)"),
    r(0xB6330846, GoldenEye, En, "GoldenEye 007 (USA)"),
    r(0x8B70CB5B, GoldenEye, En, "GoldenEye 007 (USA) (byte-swapped)"),
    r(0x9EC14AEB, GoldenEye, En, "GoldenEye 007 (Europe)"),
    r(0x7425AE2D, GoldenEye, En, "GoldenEye 007 (Europe) (byte-swapped)"),
    r(0xAD429961, BanjoKazooie, En, "Banjo-Kazooie (USA)"),
    r(0x4FD43199, BanjoKazooie, En, "Banjo-Kazooie (USA) (byte-swapped)"),
    r(0xFB7FFB10, BanjoKazooie, En, "Banjo-Kazooie (USA) (Rev 1)"),
    r(0x90B38651, BanjoKazooie, En, "Banjo-Kazooie (USA) (Rev 1) (byte-swapped)"),
    r(0x525899C9, BanjoKazooie, En, "Banjo-Kazooie (Europe) (En,Fr,De)"),
    r(0x4AF7C16B, BanjoKazooie, En, "Banjo-Kazooie (Europe) (En,Fr,De) (byte-swapped)"),
    r(0xA7F5CD7E, PaperMario, En, "Paper Mario (USA)"),
    r(0xD56D1C89, PaperMario, En, "Paper Mario (USA) (byte-swapped)"),
    r(0x9FC00CE3, PaperMario, En, "Paper Mario (Europe) (En,Fr,De,Es)"),
    r(0x406CE874, PaperMario, En, "Paper Mario (Europe) (En,Fr,De,Es) (byte-swapped)"),
    r(0xDC9BB451, EarthBound, En, "EarthBound (USA)"),
    r(0xA27F1C7A, FinalFantasy6, En, "Final Fantasy III (USA)"),
    r(0xC0FA0464, FinalFantasy6, En, "Final Fantasy III (USA) (Rev 1)"),
    r(0x97822948, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (SGB Enhanced) (GB Compatible)"),
    r(0xB38EB9DE, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 1) (SGB Enhanced) (GB Compatible)"),
    r(0x06887A34, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced) (GB Compatible)"),
    r(0xFED5959B, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (Germany) (SGB Enhanced) (GB Compatible)"),
    r(0xEFB76777, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (Germany) (Rev 1) (SGB Enhanced) (GB Compatible)"),
    r(0xF48824FE, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (France) (SGB Enhanced) (GB Compatible)"),
    r(0x4E2B75E7, LinksAwakening, En, "Legend of Zelda, The - Link's Awakening DX (France) (Rev 1) (SGB Enhanced) (GB Compatible)"),
    r(0x3800A387, OracleOfAges, En, "Legend of Zelda, The - Oracle of Ages (USA, Australia)"),
    r(0xD7E9F5D7, OracleOfSeasons, En, "Legend of Zelda, The - Oracle of Seasons (USA, Australia)"),
    r(0xABCEBBB1, MinishCap, En, "Legend of Zelda, The - The Minish Cap (USA)"),
    r(0xE8637292, MinishCap, En, "Legend of Zelda, The - The Minish Cap (Europe) (En,Fr,De,Es,It)"),
    r(0xC946DCA0, DonkeyKong1, En, "Donkey Kong Country (USA)"),
    r(0x3EAA5697, DonkeyKong1, En, "Donkey Kong Country (USA) (Rev 1)"),
    r(0x006364DB, DonkeyKong2, En, "Donkey Kong Country 2 - Diddy's Kong Quest (USA) (En,Fr)"),
    r(0x4E2D90F4, DonkeyKong2, En, "Donkey Kong Country 2 - Diddy's Kong Quest (USA) (En,Fr) (Rev 1)"),
    r(0x448EEC19, DonkeyKong3, En, "Donkey Kong Country 3 - Dixie Kong's Double Trouble! (USA) (En,Fr)"),
    r(0x2D206BF7, ChronoTrigger, En, "Chrono Trigger (USA)"),
    r(0xE1FB68E8, GoldenSun, En, "Golden Sun (USA, Europe)"),
    r(0x1053DCE4, GoldenSun, De, "Golden Sun (Germany)"),
    r(0xF6521161, GoldenSun, Fr, "Golden Sun (France)"),
    r(0xC63008D6, GoldenSun, Es, "Golden Sun (Spain)"),
    r(0xF3128812, GoldenSun, It, "Golden Sun (Italy)"),
    r(0x606A1C4D, GoldenSunLostAge, En, "Golden Sun - The Lost Age (USA, Europe)"),
    r(0xD981D889, GoldenSunLostAge, De, "Golden Sun - Die Vergessene Epoche (Germany)"),
    r(0x1090BD33, GoldenSunLostAge, Fr, "Golden Sun - L'Age Perdu (France)"),
    r(0xEE38BA94, GoldenSunLostAge, Es, "Golden Sun - La Edad Perdida (Spain)"),
    r(0xA3E49743, GoldenSunLostAge, It, "Golden Sun - L'Era Perduta (Italy)"),
    r(0x9F2A3048, KirbyMirror, En, "Kirby & The Amazing Mirror (USA)"),
    r(0x1B8A0625, MarioRpg, En, "Super Mario RPG - Legend of the Seven Stars (USA)"),
    r(0x20F2AC29, Lufia2, En, "Lufia II - Rise of the Sinistrals (USA)"),
    r(0xDD0AC86C, MysteryDungeonRed, En, "Pokemon Mystery Dungeon - Red Rescue Team (USA, Australia)"),
    r(0xD708F5AB, FinalFantasy6Advance, En, "Final Fantasy VI Advance (USA)"),
    r(0x6649D2CE, TacticsOgre, En, "Tactics Ogre - The Knight of Lodis (USA)"),
    r(0x67CDACC5, BreathOfFire2, En, "Breath of Fire II (USA)"),
    r(0xE718D850, SuperstarSaga, En, "Mario & Luigi - Superstar Saga (USA)"),
    // The fan translation's v1.3a with the HQ sound patch (the save code is the Japanese game's, byte for byte); the
    // Japanese ROM itself writes its names in its own letters, so it isn't listed.
    r(0x789605CD, Mother3, En, "Mother 3 (Japan) [T-En v1.3a, HQ Sound]"),
    r(0x6C75479C, MetroidFusion, En, "Metroid Fusion (USA)"),
    r(0x974E46AB, MetroidFusion, En, "Metroid Fusion (Europe) (En,Fr,De,Es,It)"),
    r(0x2A524221, FireEmblem7, En, "Fire Emblem (USA, Australia)"),
    r(0xA47246AE, FireEmblem8, En, "Fire Emblem - The Sacred Stones (USA, Australia)"),
    r(0x777AAC2F, LinkToThePast, En, "Legend of Zelda, The - A Link to the Past (USA)"),
    r(0x04D55E12, LinkToThePast, En, "Legend of Zelda, The - A Link to the Past (Europe)"),
    r(0x6A6C2091, LinkToThePast, De, "Legend of Zelda, The - A Link to the Past (Germany)"),
    r(0x1F8C30E8, LinkToThePast, Fr, "Legend of Zelda, The - A Link to the Past (France)"),
    r(0x47760381, LinkToThePast, Fr, "Legend of Zelda, The - A Link to the Past (Canada) (Fr)"),
    // The DS games by their game code (`ds_key`); one code covers USA and Europe and every revision.
    ds(b"ADAE", DiamondPearl, En, "Pokemon - Diamond Version (USA) / (Europe)"),
    ds(b"APAE", DiamondPearl, En, "Pokemon - Pearl Version (USA) / (Europe)"),
    ds(b"ADAD", DiamondPearl, De, "Pokemon - Diamant-Edition (Germany)"),
    ds(b"APAD", DiamondPearl, De, "Pokemon - Perl-Edition (Germany)"),
    ds(b"ADAS", DiamondPearl, Es, "Pokemon - Edicion Diamante (Spain)"),
    ds(b"APAS", DiamondPearl, Es, "Pokemon - Edicion Perla (Spain)"),
    ds(b"ADAF", DiamondPearl, Fr, "Pokemon - Version Diamant (France)"),
    ds(b"APAF", DiamondPearl, Fr, "Pokemon - Version Perle (France)"),
    ds(b"ADAI", DiamondPearl, It, "Pokemon - Versione Diamante (Italy)"),
    ds(b"APAI", DiamondPearl, It, "Pokemon - Versione Perla (Italy)"),
    ds(b"CPUE", Platinum, En, "Pokemon - Platinum Version (USA) / (Europe)"),
    ds(b"CPUD", Platinum, De, "Pokemon - Platin-Edition (Germany)"),
    ds(b"CPUS", Platinum, Es, "Pokemon - Edicion Platino (Spain)"),
    ds(b"CPUF", Platinum, Fr, "Pokemon - Version Platine (France)"),
    ds(b"CPUI", Platinum, It, "Pokemon - Versione Platino (Italy)"),
    ds(b"IPKE", HeartGoldSoulSilver, En, "Pokemon - HeartGold Version (USA) / (Europe)"),
    ds(b"IPGE", HeartGoldSoulSilver, En, "Pokemon - SoulSilver Version (USA, Australia) / (Europe)"),
    ds(b"IPKD", HeartGoldSoulSilver, De, "Pokemon - Goldene Edition HeartGold (Germany)"),
    ds(b"IPGD", HeartGoldSoulSilver, De, "Pokemon - Silberne Edition SoulSilver (Germany)"),
    ds(b"IPKS", HeartGoldSoulSilver, Es, "Pokemon - Edicion Oro HeartGold (Spain)"),
    ds(b"IPGS", HeartGoldSoulSilver, Es, "Pokemon - Edicion Plata SoulSilver (Spain)"),
    ds(b"IPKF", HeartGoldSoulSilver, Fr, "Pokemon - Version Or HeartGold (France)"),
    ds(b"IPGF", HeartGoldSoulSilver, Fr, "Pokemon - Version Argent SoulSilver (France)"),
    ds(b"IPKI", HeartGoldSoulSilver, It, "Pokemon - Versione Oro HeartGold (Italy)"),
    ds(b"IPGI", HeartGoldSoulSilver, It, "Pokemon - Versione Argento SoulSilver (Italy)"),
    ds(b"IRBO", BlackWhite, En, "Pokemon - Black Version (USA, Europe)"),
    ds(b"IRAO", BlackWhite, En, "Pokemon - White Version (USA, Europe)"),
    ds(b"IRBD", BlackWhite, De, "Pokemon - Schwarze Edition (Germany)"),
    ds(b"IRAD", BlackWhite, De, "Pokemon - Weisse Edition (Germany)"),
    ds(b"IRBS", BlackWhite, Es, "Pokemon - Edicion Negra (Spain)"),
    ds(b"IRAS", BlackWhite, Es, "Pokemon - Edicion Blanca (Spain)"),
    ds(b"IRBF", BlackWhite, Fr, "Pokemon - Version Noire (France)"),
    ds(b"IRAF", BlackWhite, Fr, "Pokemon - Version Blanche (France)"),
    ds(b"IRBI", BlackWhite, It, "Pokemon - Versione Nera (Italy)"),
    ds(b"IRAI", BlackWhite, It, "Pokemon - Versione Bianca (Italy)"),
    ds(b"IREO", BlackWhite2, En, "Pokemon - Black Version 2 (USA, Europe)"),
    ds(b"IRDO", BlackWhite2, En, "Pokemon - White Version 2 (USA, Europe)"),
    ds(b"IRED", BlackWhite2, De, "Pokemon - Schwarze Edition 2 (Germany)"),
    ds(b"IRDD", BlackWhite2, De, "Pokemon - Weisse Edition 2 (Germany)"),
    ds(b"IRES", BlackWhite2, Es, "Pokemon - Edicion Negra 2 (Spain)"),
    ds(b"IRDS", BlackWhite2, Es, "Pokemon - Edicion Blanca 2 (Spain)"),
    ds(b"IREF", BlackWhite2, Fr, "Pokemon - Version Noire 2 (France)"),
    ds(b"IRDF", BlackWhite2, Fr, "Pokemon - Version Blanche 2 (France)"),
    ds(b"IREI", BlackWhite2, It, "Pokemon - Versione Nera 2 (Italy)"),
    ds(b"IRDI", BlackWhite2, It, "Pokemon - Versione Bianca 2 (Italy)"),
];

/// The release a ROM's CRC-32 is (either of a file's two sums: as it is, or without its copier header), or None.
pub fn release(crc: u32, alt: Option<u32>) -> Option<&'static Release> {
    RELEASES.iter().find(|r| r.crc == crc || Some(r.crc) == alt)
}

/// Whether a ROM of `len` bytes in `system`'s folder could be one of [`RELEASES`] at all, so that only those are read
/// through for their checksum (a zip states its own; it is always worth asking).
pub fn worth_checking(system: &str, len: u64, zip: bool) -> bool {
    const MB: u64 = 1 << 20;
    let len = if len % 1024 == 512 { len - 512 } else { len };
    match system {
        "gb" | "gbc" | "gbh" | "gbch" => zip || len == MB || len == 2 * MB,
        "gba" | "gbah" => zip || len == 8 * MB || len == 16 * MB || len == 32 * MB,
        "snes" | "sfc" | "snesh" => zip || len == MB / 2 || len == MB || len == 5 * MB / 2 || len == 3 * MB || len == 4 * MB,
        "n64" => zip || len == 8 * MB || len == 12 * MB || len == 16 * MB || len == 32 * MB || len == 40 * MB || len == 48 * MB,
        _ => false,
    }
}

/// Where the player is, as the save says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Badges (Gold, Silver and Crystal count Kanto's too), the first Pokémon of the team that isn't an egg (its
    /// name as the game shows it, its level; None when it can't be read or there is none yet), the game's clock.
    Pokemon { badges: u8, lead: Option<(String, u8)>, hours: u16, minutes: u8, art: TeamArt },
    /// Super Metroid and Metroid: Zero Mission: the item percentage the ending shows, and the game's clock.
    Items { percent: u8, hours: u16, minutes: u8 },
    /// Super Mario World: the exits the file select counts, of 96.
    Exits { done: u8, of: u8 },
    /// Super Mario 64: the stars the file select counts, of 120.
    Stars { done: u8, of: u8 },
    /// Mario Kart 64: the gold cups of the Grand Prix (4 cups in 4 classes).
    GoldCups { done: u8, of: u8 },
    /// Star Fox 64: the planets' medals won, and in Expert mode.
    Medals { normal: u8, expert: u8 },
    /// Diddy Kong Racing: the golden balloons (of 47).
    Balloons { done: u8, of: u8 },
    /// Ocarina of Time: the spiritual stones (of 3) until the first medallion, then the medallions (of 6), and the heart
    /// containers.
    Spirit { stones: u8, medallions: u8, hearts: u8 },
    /// Paper Mario: the Star Spirits rescued (of 7), Mario's level and the game's clock, as its file select shows them.
    StarSpirits { rescued: u8, level: u8, hours: u16, minutes: u8 },
    /// A role-playing game: the leader of the party (name as the game writes it, level; None when its name can't be
    /// read) and the game's clock.
    Hero { lead: Option<(String, u8)>, hours: u16, minutes: u8 },
    /// A role-playing game that keeps no clock to show: the party's leader and how many friends travel along.
    Party { lead: (String, u8), friends: u8 },
    /// A Link to the Past: the pendants (Light World) or, once Agahnim is beaten, the crystals, and the heart
    /// containers.
    Quest { pendants: u8, crystals: u8, hearts: u8, dark_world: bool },
    /// Link's Awakening DX: the instruments of the Sirens (of 8) and the heart containers.
    Instruments { done: u8, hearts: u8 },
    /// The Minish Cap: the elements (of 4) and the hearts.
    Elements { done: u8, hearts: u8 },
    /// Majora's Mask: the boss remains (of 4) and the hearts.
    Remains { done: u8, hearts: u8 },
    /// Oracle of Ages and Seasons: the essences (of 8) and the hearts.
    Essences { done: u8, hearts: u8 },
    /// Fire Emblem: the chapter as the save menu names it (None when it names a place of the world map instead), the
    /// lord whose story it is, the mode and the game's clock.
    Chapter { chapter: Option<String>, lord: Option<&'static str>, mode: Option<&'static str>, hours: u16, minutes: u8 },
    /// Donkey Kong Country: the share its file select shows (of `of`, the most the game counts: 101, 102, 105) and
    /// the game's clock where the save keeps one.
    Complete { percent: u8, of: u8, clock: Option<(u16, u8)> },
    /// Kirby & the Amazing Mirror: the pieces of the mirror found (of 8).
    Shards { done: u8 },
    /// Banjo-Kazooie: the Jiggies (of `of`), the notes and the game's clock, as its game select shows them.
    Jiggies { done: u8, of: u8, notes: u16, hours: u16, minutes: u8 },
    /// GoldenEye 007: the furthest mission done at the hardest difficulty done ("Mission 2.i: Surface", "Secret Agent"),
    /// as its folder select shows them; only "007" once that mode is open (the folder names no mission then).
    Mission { mission: Option<String>, difficulty: &'static str },
}

fn clock_part(h: u16, m: u8) -> String {
    format!("game clock {h}:{m:02}")
}

impl Progress {
    /// The line's parts in English ("8 badges", "Charizard Lv. 45", "game clock 31:12": the game's own clock, never to
    /// be taken for the time pocket-ui counted); `case_knows_read::line` puts them in
    /// the interface's language and joins them with " · ".
    pub fn parts(&self) -> Vec<String> {
        let clock = clock_part;
        match self {
            Progress::Pokemon { badges, lead, hours, minutes, .. } => {
                let mut out = vec![match badges {
                    0 => "No badges yet".to_string(),
                    1 => "1 badge".to_string(),
                    n => format!("{n} badges"),
                }];
                out.extend(lead.as_ref().map(|(name, level)| format!("{name} Lv. {level}")));
                out.push(clock(*hours, *minutes));
                out
            }
            Progress::Items { percent, hours, minutes } => vec![format!("{percent}% items"), clock(*hours, *minutes)],
            Progress::Exits { done, of } => vec![format!("{done} of {of} exits")],
            Progress::Stars { done, of } => vec![format!("{done} of {of} stars")],
            Progress::GoldCups { done, of } => vec![format!("{done} of {of} gold cups")],
            Progress::Balloons { done, of } => vec![format!("{done} of {of} golden balloons")],
            Progress::Medals { normal, expert } => {
                let mut out = vec![if *normal == 1 { "1 medal".to_string() } else { format!("{normal} medals") }];
                if *expert > 0 {
                    out.push(format!("{expert} in Expert"));
                }
                out
            }
            Progress::StarSpirits { rescued, level, hours, minutes } => vec![format!("{rescued} of {} Star Spirits", 7), format!("Mario Lv. {level}"), clock(*hours, *minutes)],
            Progress::Spirit { stones, medallions, hearts } => vec![
                if *medallions > 0 { format!("{medallions} of {} medallions", 6) } else { format!("{stones} of {} spiritual stones", 3) },
                format!("{hearts} hearts"),
            ],
            Progress::Party { lead: (name, level), friends } => {
                let mut out = vec![format!("{name} Lv. {level}")];
                match friends {
                    0 => {}
                    1 => out.push("1 friend".to_string()),
                    n => out.push(format!("{n} friends")),
                }
                out
            }
            Progress::Hero { lead, hours, minutes } => {
                let mut out: Vec<String> = lead.iter().map(|(name, level)| format!("{name} Lv. {level}")).collect();
                out.push(clock(*hours, *minutes));
                out
            }
            Progress::Instruments { done, hearts } => vec![format!("{done} of {} instruments", 8), format!("{hearts} hearts")],
            Progress::Elements { done, hearts } => vec![format!("{done} of {} elements", 4), format!("{hearts} hearts")],
            Progress::Remains { done, hearts } => vec![format!("{done} of {} remains", 4), format!("{hearts} hearts")],
            Progress::Essences { done, hearts } => vec![format!("{done} of {} essences", 8), format!("{hearts} hearts")],
            Progress::Chapter { chapter, lord, mode, hours, minutes } => {
                let mut out: Vec<String> = chapter.iter().cloned().collect();
                out.extend(lord.map(str::to_string));
                out.extend(mode.map(str::to_string));
                out.push(clock(*hours, *minutes));
                out
            }
            Progress::Complete { percent, clock, .. } => {
                let mut out = vec![format!("{percent}% complete")];
                out.extend(clock.map(|(h, m)| clock_part(h, m)));
                out
            }
            Progress::Shards { done } => vec![format!("{done} of {} mirror shards", 8)],
            Progress::Mission { mission, difficulty } => mission.iter().cloned().chain([difficulty.to_string()]).collect(),
            Progress::Jiggies { done, notes, hours, minutes, .. } => vec![
                if *done == 1 { "1 Jiggy".to_string() } else { format!("{done} Jiggies") },
                if *notes == 1 { "1 note".to_string() } else { format!("{notes} notes") },
                clock(*hours, *minutes),
            ],
            Progress::Quest { pendants, crystals, hearts, dark_world } => vec![
                if *dark_world { format!("{crystals} of {} crystals", 7) } else { format!("{pendants} of {} pendants", 3) },
                format!("{hearts} hearts"),
            ],
        }
    }
}

/// A small measure drawn under the line instead of a picture from the game: a bar (Super Metroid's items, Super Mario
/// World's exits) or a row of marks (A Link to the Past's pendants or crystals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meter {
    Share { done: u16, of: u16 },
    Marks { done: u8, of: u8 },
}

impl Progress {
    /// The measure under the line, or None (Pokémon: its pictures, `case_knows_art`).
    pub fn meter(&self) -> Option<Meter> {
        match self {
            Progress::Pokemon { .. } | Progress::Medals { .. } | Progress::Hero { .. } | Progress::Party { .. } | Progress::Chapter { .. } | Progress::Mission { .. } => None,
            Progress::Items { percent, .. } => Some(Meter::Share { done: *percent as u16, of: 100 }),
            Progress::Complete { percent, of, .. } => Some(Meter::Share { done: *percent as u16, of: *of as u16 }),
            Progress::Exits { done, of } | Progress::Stars { done, of } | Progress::GoldCups { done, of } | Progress::Balloons { done, of } => Some(Meter::Share { done: *done as u16, of: *of as u16 }),
            Progress::StarSpirits { rescued, .. } => Some(Meter::Marks { done: *rescued, of: 7 }),
            Progress::Spirit { stones, medallions, .. } => Some(if *medallions > 0 { Meter::Marks { done: *medallions, of: 6 } } else { Meter::Marks { done: *stones, of: 3 } }),
            Progress::Instruments { done, .. } => Some(Meter::Marks { done: *done, of: 8 }),
            Progress::Elements { done, .. } => Some(Meter::Marks { done: *done, of: 4 }),
            Progress::Essences { done, .. } => Some(Meter::Marks { done: *done, of: 8 }),
            Progress::Shards { done } => Some(Meter::Marks { done: *done, of: 8 }),
            Progress::Jiggies { done, of, .. } => Some(Meter::Share { done: *done as u16, of: *of as u16 }),
            Progress::Remains { done, .. } => Some(Meter::Marks { done: *done, of: 4 }),
            Progress::Quest { pendants, crystals, dark_world, .. } => Some(if *dark_world { Meter::Marks { done: *crystals, of: 7 } } else { Meter::Marks { done: *pendants, of: 3 } }),
        }
    }
}

/// What the saves add for the pictures read from the player's own ROM (`case_knows_art`): the team by species (the
/// GBA games' internal numbers, 412 an egg; Gold, Silver and Crystal's 1 to 251, 0xFD an egg) and which badges are won
/// (Gold, Silver and Crystal: Johto's, the ones on the trainer card). Empty for Red, Blue and Yellow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamArt {
    pub team: Vec<u16>,
    /// Bit n: badge n + 1 is won.
    pub badges: u8,
}

/// What a read remembers for the next one, for games whose save holds several files and doesn't say which was
/// played last (Super Mario World, A Link to the Past): a fingerprint of each valid file and the one played. The file
/// whose bytes changed since is the one being played.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Per file: a fingerprint of its bytes, 0 for an empty or damaged one.
    pub prints: Vec<u32>,
    pub current: Option<usize>,
}

/// Reads `save` as `release`'s battery save. None when it isn't one the game would load as it is, or says nothing
/// to show; with it what to remember for the next read.
pub fn read(release: &Release, save: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    match release.kind {
        Gen1 => (gen1(save, release.lang), Memory::default()),
        GoldSilver | Crystal => (gen2(save, release.lang, release.kind == Crystal), Memory::default()),
        RubySapphire | Emerald | FireRedLeafGreen => (gen3(save, release.kind), Memory::default()),
        SuperMetroid => super_metroid(save, before),
        ZeroMission => zero_mission(save, before),
        MarioWorld => mario_world(save, (0, 3 * 0x8F, 3), before),
        AllStarsWorld => mario_world(save, (0x1000, 0x23C, 4), before),
        LinkToThePast => link_to_the_past(save, before),
        Mario64 => mario_64(save, before),
        Ocarina => ocarina(save, before),
        PaperMario => (paper_mario(save), Memory::default()),
        MarioKart64 => (mario_kart_64(save), Memory::default()),
        StarFox64 => (star_fox_64(save), Memory::default()),
        DiddyKongRacing => diddy_kong_racing(save, before),
        FinalFantasy6 => final_fantasy_6(save, before),
        EarthBound => earthbound(save, before),
        LinksAwakening => links_awakening(save, before),
        MinishCap => minish_cap(save, before),
        MajorasMask => majoras_mask(save, before),
        OracleOfAges => oracle(save, true, before),
        OracleOfSeasons => oracle(save, false, before),
        FireEmblem7 => fire_emblem(save, false, before),
        FireEmblem8 => fire_emblem(save, true, before),
        DonkeyKong1 | DonkeyKong2 | DonkeyKong3 => donkey_kong(save, release.kind, before),
        ChronoTrigger => chrono_trigger(save, before),
        GoldenSun => golden_sun(save, false, before),
        GoldenSunLostAge => golden_sun(save, true, before),
        KirbyMirror => kirby_mirror(save, before),
        MarioRpg => mario_rpg(save, before),
        Lufia2 => lufia_2(save, before),
        MysteryDungeonRed => (mystery_dungeon_red(save), Memory::default()),
        FinalFantasy6Advance => final_fantasy_6_advance(save, before),
        TacticsOgre => (tactics_ogre(save), Memory::default()),
        BreathOfFire2 => breath_of_fire_2(save, before),
        SuperstarSaga => superstar_saga(save, before),
        Mother3 => mother_3(save, before),
        BanjoKazooie => banjo_kazooie(save, before),
        MetroidFusion => metroid_fusion(save, before),
        GoldenEye => goldeneye(save, before),
        DiamondPearl | Platinum | HeartGoldSoulSilver => (gen4(save, release.kind), Memory::default()),
        BlackWhite | BlackWhite2 => (gen5(save, release.kind), Memory::default()),
    }
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A small fingerprint of a file's bytes (FNV-1a), to tell which changed between two reads.
fn print(b: &[u8]) -> u32 {
    b.iter().fold(0x811C_9DC5u32, |h, &x| (h ^ x as u32).wrapping_mul(0x0100_0193)).max(1)
}

/// The file being played among `valid` (index, fingerprint), going by what the last read saw.
fn pick(valid: &[(usize, u32)], files: usize, before: Option<&Memory>) -> Memory {
    let mut prints = vec![0; files];
    for (i, p) in valid {
        prints[*i] = *p;
    }
    let current = if valid.len() == 1 {
        Some(valid[0].0)
    } else if let Some(m) = before.filter(|m| m.prints.len() == files) {
        let changed: Vec<usize> = valid.iter().filter(|(i, p)| m.prints[*i] != *p).map(|(i, _)| *i).collect();
        match changed.as_slice() {
            [one] => Some(*one),
            // Nothing changed: the one played before, if it is still there.
            // (A damaged cache line may name a file past the last.)
            [] => m.current.filter(|c| prints.get(*c).is_some_and(|p| *p != 0)),
            // Several at once (a file copied in the game's own menu): the one played before if it is among them.
            many => m.current.filter(|c| many.contains(c)),
        }
    } else {
        None
    };
    Memory { prints, current }
}

/// A Pokémon's name as the game shows it, from its letters in the Game Boy games' table (`gen2`: Gold, Silver and
/// Crystal's). None for a letter that isn't a plain one (Japanese, a glitch), so a name is never shown half.
fn gb_name(bytes: &[u8], lang: Lang, gen2: bool) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0x50 => break,
            0x80..=0x99 => (b'A' + (b - 0x80)) as char,
            0xA0..=0xB9 => (b'a' + (b - 0xA0)) as char,
            0xF6..=0xFF => (b'0' + (b - 0xF6)) as char,
            0x7F => ' ',
            0x9A => '(',
            0x9B => ')',
            0x9C => ':',
            0xE0 => '\'',
            0xE3 => '-',
            0xE6 => '?',
            0xE7 => '!',
            0xE8 | 0xF2 => '.',
            0xF3 => '/',
            0xF4 => ',',
            0xEF | 0xF5 => continue, // ♂ ♀: the interface's faces have no such letters; "Nidoran" is still right.
            0xBA => 'à',
            0xBB => 'è',
            0xBC => 'é',
            0xBD => 'ù',
            0xC0 => 'Ä',
            0xC1 => 'Ö',
            0xC2 => 'Ü',
            0xC3 => 'ä',
            0xC4 => 'ö',
            0xC5 => 'ü',
            _ => gb_accent(b, lang, gen2)?,
        };
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

/// The letters the Game Boy tables give each language beyond the shared ones (PKHeX's `StringConverter1/2`).
fn gb_accent(b: u8, lang: Lang, gen2: bool) -> Option<char> {
    if gen2 && matches!(lang, Fr | De) {
        return Some(match b {
            0xBE => 'ß',
            0xBF => 'ç',
            0xC6 => 'ë',
            0xC7 => 'ï',
            0xC8 => 'â',
            0xC9 => 'ô',
            0xCA => 'û',
            0xCB => 'ê',
            0xCC => 'î',
            0xCD => 'Ù',
            0xCE => 'Ú',
            0xCF => 'á',
            _ => return None,
        });
    }
    Some(match b {
        0xBE => 'À',
        0xBF => 'Á',
        0xC6 => 'È',
        0xC7 => 'É',
        0xC8 => 'Ì',
        0xC9 => 'Í',
        0xCA => 'Ñ',
        0xCB => 'Ò',
        0xCC => 'Ó',
        0xCD => 'Ù',
        0xCE => 'Ú',
        0xCF => 'á',
        0xD0 => 'ì',
        0xD1 => 'í',
        0xD2 => 'ñ',
        0xD3 => 'ò',
        0xD4 => 'ó',
        0xD5 => 'ú',
        _ => return None,
    })
}

/// The GBA games' letters (PKHeX's `G3_EN`, the Western table): None for any other.
fn gba_name(bytes: &[u8]) -> Option<String> {
    const LATIN: &str = " ÀÁÂÇÈÉÊËÌ\0ÎÏÒÓÔŒÙÚÛÑßàá\0çèéêëì\0îïòóôœùúûñºª\0&+";
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0xFF => break,
            0x00..=0x2E => LATIN.chars().nth(b as usize).filter(|c| *c != '\0')?,
            0xA1..=0xAA => (b'0' + (b - 0xA1)) as char,
            0xAB => '!',
            0xAC => '?',
            0xAD => '.',
            0xAE => '-',
            0xB3 => '\'',
            0xB4 => '\'',
            0xB5 | 0xB6 => continue, // ♂ ♀
            0xB8 => ',',
            0xBA => '/',
            0xBB..=0xD4 => (b'A' + (b - 0xBB)) as char,
            0xD5..=0xEE => (b'a' + (b - 0xD5)) as char,
            0xF0 => ':',
            0xF1 => 'Ä',
            0xF2 => 'Ö',
            0xF3 => 'Ü',
            0xF4 => 'ä',
            0xF5 => 'ö',
            0xF6 => 'ü',
            _ => return None,
        };
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

/// A name in capitals as the old games write every Pokémon ("CHARIZARD", "MR.MIME", "FARFETCH'D") in the case a
/// sentence uses ("Charizard", "Mr.Mime", "Farfetch'd"). A name with a small letter in it is the player's own spelling
/// and stays as it is.
fn shown_name(name: &str) -> String {
    if name.chars().any(char::is_lowercase) {
        return name.to_string();
    }
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in name.chars() {
        let starts = prev.is_none_or(|p| !p.is_alphabetic() && p != '\'');
        if starts { out.push(c) } else { out.extend(c.to_lowercase()) }
        prev = Some(c);
    }
    out
}

/// Red, Blue and Yellow (Western): the bank 1 block from the player's name to its checksum byte (PKHeX `SAV1Offsets::INT`).
fn gen1(s: &[u8], lang: Lang) -> Option<Progress> {
    const OT: usize = 0x2598;
    const SUM: usize = 0x3523;
    const BADGES: usize = 0x2602;
    const TIME: usize = 0x2CED;
    const PARTY: usize = 0x2F2C;
    if s.len() < 0x8000 {
        return None;
    }
    let sum = s[OT..SUM].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    if !sum != s[SUM] {
        return None;
    }
    let (hours, maxed, minutes) = (s[TIME] as u16, s[TIME + 1] != 0, s[TIME + 2]);
    if minutes > 59 || s[TIME + 3] > 59 {
        return None;
    }
    let (hours, minutes) = if maxed { (255, 59) } else { (hours, minutes) };
    let count = s[PARTY] as usize;
    if count > 6 || s[PARTY + 1 + count] != 0xFF {
        return None;
    }
    let lead = if count == 0 {
        None
    } else {
        let level = s[PARTY + 8 + 0x21];
        if !(1..=100).contains(&level) {
            return None;
        }
        gb_name(&s[PARTY + 0x152..PARTY + 0x152 + 11], lang, false).map(|n| (n, level))
    };
    Some(Progress::Pokemon { badges: s[BADGES].count_ones() as u8, lead, hours, minutes, art: TeamArt::default() })
}

/// Gold, Silver and Crystal (Western): the block from 0x2009 to the end the checksum covers (PKHeX `SAV2Offsets`).
fn gen2(s: &[u8], lang: Lang, crystal: bool) -> Option<Progress> {
    let (time, badges, party, end, sum_at) = if crystal { (0x2052, 0x23E5, 0x2865, 0x2B82, 0x2D0D) } else { (0x2053, 0x23E4, 0x288A, 0x2D68, 0x2D69) };
    if s.len() < 0x8000 {
        return None;
    }
    let sum = s[0x2009..=end].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
    if sum != u16le(s, sum_at) {
        return None;
    }
    let hours = u16::from_be_bytes([s[time], s[time + 1]]);
    let (minutes, seconds) = (s[time + 2], s[time + 3]);
    if minutes > 59 || seconds > 59 || hours > 999 {
        return None;
    }
    let count = s[party] as usize;
    if count > 6 || s[party + 1 + count] != 0xFF {
        return None;
    }
    // The team list: count, species (0xFD an egg) and 0xFF, six of 48 bytes, six trainer names, six names (11 each).
    let mut lead = None;
    for i in 0..count {
        if s[party + 1 + i] == 0xFD {
            continue;
        }
        let level = s[party + 8 + i * 48 + 0x1F];
        if !(1..=100).contains(&level) {
            return None;
        }
        let at = party + 8 + 6 * 48 + 6 * 11 + i * 11;
        lead = gb_name(&s[at..at + 11], lang, true).map(|n| (n, level));
        break;
    }
    let art = TeamArt { team: s[party + 1..party + 1 + count].iter().map(|&sp| sp as u16).collect(), badges: s[badges] };
    let badges = (s[badges].count_ones() + s[badges + 1].count_ones()) as u8;
    Some(Progress::Pokemon { badges, lead, hours, minutes, art })
}

/// The DS games' CRC-16 (CCITT, from 0xFFFF; PKHeX `Checksums.CRC16_CCITT`) over a block of the save, a byte at a time
/// by a table (the blocks are up to 62 KiB, read on a handheld).
pub(crate) fn crc16_ccitt(b: &[u8]) -> u16 {
    const TABLE: [u16; 256] = {
        let mut t = [0u16; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = (i as u16) << 8;
            let mut k = 0;
            while k < 8 {
                c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    b.iter().fold(0xFFFFu16, |c, &x| (c << 8) ^ TABLE[((c >> 8) as u8 ^ x) as usize])
}

/// The egg's and Manaphy's egg's numbers among the DS icons (the games' own: after species 493).
pub const DS_EGG: u16 = 494;
pub const DS_MANAPHY_EGG: u16 = 495;

/// One Pokémon of a DS team, unscrambled: its species, whether it is an egg, its level, its name (None when a letter
/// isn't a plain one).
struct DsMon {
    species: u16,
    egg: bool,
    level: u8,
    name: Option<String>,
}

/// Where each of the four 32-byte parts is stored, for each of the 24 orders (`PokeCrypto.BlockPosition`).
const DS_ORDER: [[u8; 4]; 24] = [
    [0, 1, 2, 3], [0, 1, 3, 2], [0, 2, 1, 3], [0, 3, 1, 2], [0, 2, 3, 1], [0, 3, 2, 1], [1, 0, 2, 3], [1, 0, 3, 2],
    [2, 0, 1, 3], [3, 0, 1, 2], [2, 0, 3, 1], [3, 0, 2, 1], [1, 2, 0, 3], [1, 3, 0, 2], [2, 1, 0, 3], [3, 1, 0, 2],
    [2, 3, 0, 1], [3, 2, 0, 1], [1, 2, 3, 0], [1, 3, 2, 0], [2, 1, 3, 0], [3, 1, 2, 0], [2, 3, 1, 0], [3, 2, 1, 0],
];

/// The DS games' scrambling: each 16-bit word XORed with the next number of their random sequence from `seed`.
fn ds_crypt(b: &[u8], mut seed: u32) -> Vec<u8> {
    b.chunks(2)
        .flat_map(|w| {
            seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x6073);
            (u16::from_le_bytes([w[0], w[1]]) ^ (seed >> 16) as u16).to_le_bytes()
        })
        .collect()
}

/// A party Pokémon of a DS game (236 bytes in generation 4, 220 in 5): 128 bytes in four parts, scrambled with the
/// Pokémon's checksum and stored in one of 24 orders by its personality; the battle stats after them scrambled with the
/// personality. None when the checksum (the sum of the unscrambled words) fails, or a value is one the game can't have.
fn ds_mon(m: &[u8], gen5: bool) -> Option<DsMon> {
    let (pid, sum) = (u32le(m, 0), u16le(m, 6));
    let data = ds_crypt(m.get(8..136)?, sum as u32);
    if (0..64).fold(0u16, |a, i| a.wrapping_add(u16le(&data, i * 2))) != sum {
        return None;
    }
    let order = DS_ORDER[(((pid >> 13) & 31) % 24) as usize];
    let part = |i: usize| &data[order[i] as usize * 32..order[i] as usize * 32 + 32];
    let species = u16le(part(0), 0);
    let egg = u32le(part(1), 0x10) & (1 << 30) != 0;
    let level = *ds_crypt(m.get(136..)?, pid).get(4)?;
    let last = if gen5 { 649 } else { 493 };
    if !(1..=last).contains(&species) || !(1..=100).contains(&level) {
        return None;
    }
    let name = if gen5 { g5_name(&part(2)[..22]) } else { g4_name(&part(2)[..22]) };
    Some(DsMon { species, egg, level, name })
}

/// The team of a DS save's party block: `count` Pokémon of `size` bytes from `at`. The lead is the first that isn't an
/// egg (None when one before it can't be read: it might have been the lead); the team for the pictures is left empty
/// when any Pokémon fails its checksum.
fn ds_team(b: &[u8], at: usize, count: usize, size: usize, gen5: bool) -> Option<(Option<(String, u8)>, Vec<u16>)> {
    let mut lead = None;
    let mut team = Some(Vec::new());
    let mut unsure = false;
    for i in 0..count {
        let mon = ds_mon(b.get(at + i * size..at + (i + 1) * size)?, gen5);
        match (&mon, &mut team) {
            (Some(m), Some(t)) => t.push(match (m.egg, m.species) {
                (true, 490) => DS_MANAPHY_EGG,
                (true, _) => DS_EGG,
                (false, sp) => sp,
            }),
            (None, t) => *t = None,
            _ => {}
        }
        match mon {
            None => unsure = true,
            Some(m) if !m.egg && lead.is_none() && !unsure => lead = Some(m.name.map(|n| (n, m.level))),
            _ => {}
        }
    }
    Some((lead.flatten(), team.unwrap_or_default()))
}

/// Diamond, Pearl, Platinum, HeartGold and SoulSilver: the general block (badges, clock, team) is kept twice, at 0 and
/// 0x40000, each with a footer of its save's counters, its size and the CRC-16 of the block (Diamond, Pearl, Platinum:
/// all before the footer's 0x14 bytes; HeartGold and SoulSilver: before its last 0x10). The newer valid copy counts,
/// by the save counter, then the block's own counter, as the game decides (PKHeX `SAV4BlockDetection`).
fn gen4(s: &[u8], kind: Kind) -> Option<Progress> {
    const PARTITION: usize = 0x40000;
    let (size, covered, trainer, party) = match kind {
        DiamondPearl => (0xC100, 0xC100 - 0x14, 0x64, 0x98),
        Platinum => (0xCF2C, 0xCF2C - 0x14, 0x68, 0xA0),
        _ => (0xF628, 0xF628 - 0x10, 0x64, 0x98),
    };
    let copy = |base: usize| -> Option<((u32, u32), &[u8])> {
        let b = s.get(base..base + size)?;
        let foot = size - 0x14;
        (u32le(b, foot + 8) as usize == size && crc16_ccitt(&b[..covered]) == u16le(b, size - 2)).then(|| ((u32le(b, foot), u32le(b, foot + 4)), b))
    };
    // The second first: of two copies the same age the first counts.
    let (_, g) = [PARTITION, 0].into_iter().filter_map(copy).max_by(|a, b| newer(a.0 .0, b.0 .0).then(newer(a.0 .1, b.0 .1)))?;
    let t = trainer;
    let (hours, minutes, seconds) = (u16le(g, t + 0x22), g[t + 0x24], g[t + 0x25]);
    if minutes > 59 || seconds > 59 || hours > 999 {
        return None;
    }
    let johto = g[t + 0x1A];
    let kanto = if kind == HeartGoldSoulSilver { g[t + 0x1F] } else { 0 };
    let count = g[party - 4] as usize;
    if count > 6 {
        return None;
    }
    let (lead, team) = ds_team(g, party, count, 236, false)?;
    let badges = (johto.count_ones() + kanto.count_ones()) as u8;
    Some(Progress::Pokemon { badges, lead, hours, minutes, art: TeamArt { team, badges: johto } })
}

/// Black, White, Black 2 and White 2: blocks at fixed places, each followed by its CRC-16 (two bytes after its end)
/// and the same sum again in the table of all blocks' sums (PKHeX `SaveBlockAccessor5BW`/`B2W2`): the party, the
/// trainer's data (clock at 0x24) and the block with the badges' bits (0x04).
fn gen5(s: &[u8], kind: Kind) -> Option<Progress> {
    let (trainer_len, misc, misc_len, sums) = if kind == BlackWhite { (0x68, 0x21200, 0xEC, 0x23F00) } else { (0xB0, 0x21100, 0xF0, 0x25F00) };
    let block = |at: usize, len: usize, index: usize| -> Option<&[u8]> {
        let b = s.get(at..at + len)?;
        let sum = crc16_ccitt(b);
        (u16le(s.get(at + len + 2..at + len + 4)?, 0) == sum && u16le(s.get(sums + 2 * index..sums + 2 * index + 2)?, 0) == sum).then_some(b)
    };
    let party = block(0x18E00, 0x534, 26)?;
    let trainer = block(0x19400, trainer_len, 27)?;
    let misc = block(misc, misc_len, 52)?;
    let (hours, minutes, seconds) = (u16le(trainer, 0x24), trainer[0x26], trainer[0x27]);
    if minutes > 59 || seconds > 59 || hours > 999 {
        return None;
    }
    let count = party[4] as usize;
    if count > 6 {
        return None;
    }
    let (lead, team) = ds_team(party, 8, count, 220, true)?;
    let bits = misc[4];
    Some(Progress::Pokemon { badges: bits.count_ones() as u8, lead, hours, minutes, art: TeamArt { team, badges: bits } })
}

/// Generation 4's letters (PKHeX `StringConverter4Util`, the international table): digits, A–Z, a–z, the Latin-1
/// letters and some signs; None for any other (a Japanese or Korean name), so a name is never shown half.
fn g4_name(b: &[u8]) -> Option<String> {
    let mut out = String::new();
    for w in b.chunks(2) {
        let v = u16::from_le_bytes([w[0], w[1]]);
        let c = match v {
            0xFFFF => break,
            0x0121..=0x012A => (b'0' + (v - 0x0121) as u8) as char,
            0x012B..=0x0144 => (b'A' + (v - 0x012B) as u8) as char,
            0x0145..=0x015E => (b'a' + (v - 0x0145) as u8) as char,
            // À to ÿ in Latin-1's order, but for × and ÷ (two other signs there).
            0x015F..=0x019E if v != 0x0176 && v != 0x0196 => char::from_u32(0xC0 + (v - 0x015F) as u32)?,
            0x019F => 'Œ',
            0x01A0 => 'œ',
            0x01AB => '!',
            0x01AC => '?',
            0x01AD => ',',
            0x01AE => '.',
            0x01B1 => '/',
            0x01B3 => '\'',
            0x01BB | 0x01BC => continue, // ♂ ♀
            0x01BD => '+',
            0x01BE => '-',
            0x01C4 => ':',
            0x01DE => ' ',
            _ => return None,
        };
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

/// Generation 5's letters: UTF-16 (PKHeX `StringConverter5`); plain Latin letters only, ♂ and ♀ left out.
fn g5_name(b: &[u8]) -> Option<String> {
    let mut out = String::new();
    for w in b.chunks(2) {
        let v = u16::from_le_bytes([w[0], w[1]]);
        match v {
            0xFFFF | 0 => break,
            0x2640 | 0x2642 | 0xE08E | 0xE08F => continue,
            0x20..=0x7E | 0xA0..=0x17F => out.push(char::from_u32(v as u32)?),
            _ => return None,
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

const SECTOR: usize = 0x1000;
const SECTOR_DATA: usize = 0xF80;
const SECTORS: usize = 14;

/// Ruby, Sapphire, Emerald, FireRed and LeafGreen: two copies of 14 sectors of 4 KiB, each with its own checksum;
/// the newer whole copy counts, as in the game.
fn gen3(s: &[u8], kind: Kind) -> Option<Progress> {
    // The half-size file some emulators write for a 64 KiB flash has one copy.
    if s.len() < SECTORS * SECTOR {
        return None;
    }
    let slots: Vec<(u32, Vec<&[u8]>)> = (0..2).filter_map(|slot| gen3_slot(s, slot)).collect();
    let (_, sectors) = slots.iter().max_by(|a, b| newer(a.0, b.0))?;
    let small = sectors[0];
    let large: Vec<u8> = sectors[1..5].iter().flat_map(|d| d.iter().copied()).collect();
    let hours = u16le(small, 0x0E);
    let (minutes, seconds) = (small[0x10], small[0x11]);
    if minutes > 59 || seconds > 59 || hours > 999 {
        return None;
    }
    let (count_at, flags, first_badge) = match kind {
        Emerald => (0x234, 0x1270, 0x867),
        RubySapphire => (0x234, 0x1220, 0x807),
        _ => (0x034, 0x0EE0, 0x820),
    };
    let count = u32le(&large, count_at) as usize;
    if count > 6 {
        return None;
    }
    let mut lead = None;
    let mut team = Some(Vec::new());
    for i in 0..count {
        let mon = &large[count_at + 4 + i * 100..count_at + 4 + (i + 1) * 100];
        // The team's pictures: every Pokémon's species, from its data the game keeps scrambled; one that fails the
        // Pokémon's own checksum leaves the whole row out.
        match (species(mon), &mut team) {
            (Some(sp), Some(t)) => t.push(sp),
            (None, t) => *t = None,
            _ => {}
        }
        let f = mon[0x13];
        // Bit 0: a bad egg; bit 1: a Pokémon is there; bit 2: an egg.
        if lead.is_some() || f & 1 != 0 || f & 2 == 0 || f & 4 != 0 {
            continue;
        }
        let level = mon[0x54];
        if !(1..=100).contains(&level) {
            return None;
        }
        lead = Some(gba_name(&mon[0x08..0x12]).map(|n| (n, level)));
    }
    let lead = lead.flatten();
    let won = (0..8u8).filter(|i| {
        let flag = first_badge + *i as usize;
        large[flags + flag / 8] & (1 << (flag % 8)) != 0
    });
    let bits = won.fold(0u8, |b, i| b | 1 << i);
    Some(Progress::Pokemon { badges: bits.count_ones() as u8, lead, hours, minutes, art: TeamArt { team: team.unwrap_or_default(), badges: bits } })
}

/// The highest species number of the GBA games (412 is the egg).
const LAST_SPECIES: u16 = 412;

/// A party Pokémon's species: its 48 bytes of data are scrambled with its personality and trainer number and stored in
/// one of 24 orders; the growth part comes first with the species. None when the Pokémon's own checksum (the sum of the
/// unscrambled words) doesn't match, or the number is none a game has.
fn species(mon: &[u8]) -> Option<u16> {
    let key = u32le(mon, 0) ^ u32le(mon, 4);
    let mut data = [0u8; 48];
    for w in 0..12 {
        data[w * 4..w * 4 + 4].copy_from_slice(&(u32le(mon, 0x20 + w * 4) ^ key).to_le_bytes());
    }
    let sum = (0..24).fold(0u16, |a, i| a.wrapping_add(u16le(&data, i * 2)));
    if sum != u16le(mon, 0x1C) {
        return None;
    }
    // Where the growth part (G) is among growth, attacks, EVs and misc in each of the 24 orders (`GetSubstruct`).
    const ORDERS: [&[u8; 4]; 24] = [
        b"GAEM", b"GAME", b"GEAM", b"GEMA", b"GMAE", b"GMEA", b"AGEM", b"AGME", b"AEGM", b"AEMG", b"AMGE", b"AMEG", b"EGAM", b"EGMA", b"EAGM",
        b"EAMG", b"EMGA", b"EMAG", b"MGAE", b"MGEA", b"MAGE", b"MAEG", b"MEGA", b"MEAG",
    ];
    let at = ORDERS[(u32le(mon, 0) % 24) as usize].iter().position(|c| *c == b'G')? * 12;
    let sp = u16le(&data, at);
    (1..=LAST_SPECIES).contains(&sp).then_some(sp)
}

/// The newer of two save counters, as the game decides (a fresh flash's 0xFFFFFFFF is the oldest).
fn newer(a: u32, b: u32) -> std::cmp::Ordering {
    match (a, b) {
        (u32::MAX, x) if x != u32::MAX - 1 => std::cmp::Ordering::Less,
        (x, u32::MAX) if x != u32::MAX - 1 => std::cmp::Ordering::Greater,
        _ => a.cmp(&b),
    }
}

/// One copy of a GBA Pokémon save: its counter and its 14 sectors' data in order of their numbers, or None when one
/// is missing, doubled, unsigned or fails its checksum.
fn gen3_slot(s: &[u8], slot: usize) -> Option<(u32, Vec<&[u8]>)> {
    let mut by_id: Vec<Option<&[u8]>> = vec![None; SECTORS];
    let mut counter = None;
    for k in 0..SECTORS {
        // A file of one copy (64 KiB) has no second one: none, not past the end.
        let sector = s.get((slot * SECTORS + k) * SECTOR..(slot * SECTORS + k + 1) * SECTOR)?;
        let id = u16le(sector, 0xFF4) as usize;
        if id >= SECTORS || by_id[id].is_some() || u32le(sector, 0xFF8) != 0x0801_2025 {
            return None;
        }
        let data = &sector[..SECTOR_DATA];
        let sum = (0..SECTOR_DATA / 4).fold(0u32, |a, w| a.wrapping_add(u32le(data, w * 4)));
        if (sum as u16).wrapping_add((sum >> 16) as u16) != u16le(sector, 0xFF6) {
            return None;
        }
        let c = u32le(sector, 0xFFC);
        if counter.is_some_and(|x| x != c) {
            return None;
        }
        counter = Some(c);
        by_id[id] = Some(data);
    }
    Some((counter?, by_id.into_iter().collect::<Option<Vec<_>>>()?))
}

/// Super Metroid: three files of 0x65C bytes at 0x10, 0x66C, 0xCC8, each with its sum and that sum's complement twice;
/// the file chosen last on the file select at 0x1FEC (its complement at 0x1FEE).
fn super_metroid(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x65C;
    const AT: [usize; 3] = [0x10, 0x66C, 0xCC8];
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let valid: Vec<(usize, u32)> = (0..3)
        .filter(|&i| {
            let f = &s[AT[i]..AT[i] + SIZE];
            let sum = (0..SIZE / 2).fold(0u16, |a, w| a.wrapping_add(u16le(f, w * 2)));
            (sum == u16le(s, 2 * i) && !sum == u16le(s, 8 + 2 * i)) || (sum == u16le(s, 0x1FF0 + 2 * i) && !sum == u16le(s, 0x1FF8 + 2 * i))
        })
        .map(|i| (i, print(&s[AT[i]..AT[i] + SIZE])))
        .collect();
    let mut memory = pick(&valid, 3, before);
    let chosen = u16le(s, 0x1FEC) as usize;
    if chosen < 3 && chosen as u16 & u16le(s, 0x1FEE) == 0 && memory.prints[chosen] != 0 {
        memory.current = Some(chosen);
    }
    let Some(f) = memory.current.map(|i| &s[AT[i]..AT[i] + SIZE]) else { return (None, memory) };
    // The file starts with Samus's data from $7E:09A2 on.
    let at = |ram: usize| u16le(f, ram - 0x09A2);
    let (energy, reserve, missiles, supers, bombs) = (at(0x09C4), at(0x09D4), at(0x09C8), at(0x09CC), at(0x09D0));
    let (hours, minutes) = (at(0x09E0), at(0x09DE));
    if !(99..=1499).contains(&energy) || reserve > 400 || missiles > 230 || supers > 50 || bombs > 50 || minutes > 59 || hours > 99 {
        return (None, memory);
    }
    // The ending's sum (`$8B:E627`): tanks by their step, then each of the 11 items and 5 beams.
    const ITEMS: u16 = 0x0001 | 0x0020 | 0x0004 | 0x1000 | 0x0002 | 0x0008 | 0x0100 | 0x0200 | 0x2000 | 0x4000 | 0x8000;
    const BEAMS: u16 = 0x0001 | 0x0002 | 0x0004 | 0x0008 | 0x1000;
    let percent = energy / 100 + reserve / 100 + missiles / 5 + supers / 5 + bombs / 5 + (at(0x09A4) & ITEMS).count_ones() as u16 + (at(0x09A8) & BEAMS).count_ones() as u16;
    if percent > 100 {
        return (None, memory);
    }
    (Some(Progress::Items { percent: percent as u8, hours, minutes: minutes as u8 }), memory)
}

/// Metroid: Zero Mission (the metroidret decomp, `struct Sram`; offsets from its headers laid out as the game's compiler
/// does, every struct on 4 bytes, which puts "Planet Zebes... " at 79 and " - Samus Aran - " at 592 of a file, as in a
/// save the game itself wrote, 15.11.): three files of 4640 bytes from 128 on and a copy of each from 14048 on, each a sum
/// of its 32-bit words (taken with the sum's own words as 0 and ~0, so a whole file's words add up to the stored sum) and
/// the sum's complement; the file saved last in a record at 28032 (marked 0x1E, its 16-bit words adding up to its sum).
fn zero_mission(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 4640;
    const AT: usize = 128;
    const COPIES: usize = 14048;
    const LAST: usize = 28032;
    if s.len() < LAST + 64 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        [AT + i * SIZE, COPIES + i * SIZE].into_iter().map(|at| &s[at..at + SIZE]).find(|f| {
            let sum = (0..SIZE / 4).fold(0u32, |a, w| a.wrapping_add(u32le(f, w * 4)));
            &f[..16] == b"ZERO_MISSION_010" && sum == u32le(f, 16) && !sum == u32le(f, 20)
        })
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    let rec = &s[LAST..LAST + 64];
    let rec_sum = (0..32).fold(0u16, |a, w| a.wrapping_add(u16le(rec, w * 2)));
    let last = rec[22] as usize;
    if rec[0] == 0x1E && rec_sum == u16le(rec, 2) && !rec_sum == u16le(rec, 4) && last < 3 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    // Samus's equipment at 412: the most of each, then beams and bombs, suit and misc; the clock at 508.
    let e = &f[412..432];
    let (energy, missiles, supers, bombs) = (u16le(e, 0), u16le(e, 2), e[4] as u16, e[5] as u16);
    let (beams, suit) = (e[12], e[14]);
    let difficulty = f[60];
    let (hours, minutes) = (f[508] as u16, f[509]);
    // What a tank adds on Easy, Normal, Hard (`sTankIncreaseAmount`).
    let Some(&(te, tm, ts, tp)) = [(100, 5, 2, 2), (100, 5, 2, 2), (50, 2, 1, 1)].get(difficulty as usize) else { return (None, memory) };
    if energy < 99 || minutes > 59 {
        return (None, memory);
    }
    // The ending's count (`ChozodiaEscapeGetItemCountAndEndingNumber`): tanks, then 8 suit and misc bits, 5 beam bits and
    // the bombs' bit.
    let percent = (energy - 99) / te + missiles / tm + supers / ts + bombs / tp + suit.count_ones() as u16 + (beams & 0x1F).count_ones() as u16 + (beams >> 7) as u16;
    if percent > 100 {
        return (None, memory);
    }
    (Some(Progress::Items { percent: percent as u8, hours, minutes }), memory)
}

/// Metroid Fusion (metroidret/mf `sSaveDataSramPointers`, `struct Equipment`, the ending's count; the rest from saves the
/// game wrote, 17.11.): three files of 0x1200 bytes from 0x200 on, each followed by its copy (A at 0x200 and 0x1400, B
/// at 0x2600 and 0x3800, C at 0x4A00 and 0x5C00), each "MetroidEpisode4A" and its 32-bit words adding up to the sum at
/// 16, with the sum's complement at 20, as in Zero Mission; the file chosen last in the same kind of record at 0x7000
/// ("SELECTSAVENUMBER", the file at 22). Samus's equipment (`struct Equipment`) at 0xC4, the clock at 0xE4. The share is
/// the ending's: tanks only, energy over 99 by 100, missiles over 10 by 5, power bombs over 10 by 2, at most 100. A file
/// the game made at a new start and never saved holds no equipment and says nothing.
fn metroid_fusion(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x1200;
    const AT: usize = 0x200;
    const LAST: usize = 0x7000;
    if s.len() < LAST + 64 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        [AT + 2 * i * SIZE, AT + (2 * i + 1) * SIZE].into_iter().map(|at| &s[at..at + SIZE]).find(|f| {
            let sum = (0..SIZE / 4).fold(0u32, |a, w| a.wrapping_add(u32le(f, w * 4)));
            &f[..16] == b"MetroidEpisode4A" && sum == u32le(f, 16) && !sum == u32le(f, 20)
        })
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    let rec = &s[LAST..LAST + 64];
    let rec_sum = (0..32).fold(0u16, |a, w| a.wrapping_add(u16le(rec, w * 2)));
    let last = rec[22] as usize;
    if rec[0] == 0x1E && rec_sum == u16le(rec, 2) && !rec_sum == u16le(rec, 4) && &rec[6..22] == b"SELECTSAVENUMBER" && last < 3 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let e = &f[0xC4..0xD0];
    let (max_energy, max_missiles, max_power_bombs) = (u16le(e, 2), u16le(e, 6), e[9] as u16);
    let (hours, minutes) = (f[0xE4] as u16, f[0xE5]);
    if max_energy < 99 || minutes > 59 {
        return (None, memory);
    }
    let percent = ((max_energy - 99) / 100 + max_missiles.saturating_sub(10) / 5 + max_power_bombs.saturating_sub(10) / 2).min(100);
    (Some(Progress::Items { percent: percent as u8, hours, minutes }), memory)
}

/// GoldenEye 007's missions in the order the save counts them (`LEVEL_SOLO_SEQUENCE`): the chapter, the part, a name.
const GOLDENEYE_MISSIONS: [(u8, &str, &str); 20] = [
    (1, "i", "Dam"),
    (1, "ii", "Facility"),
    (1, "iii", "Runway"),
    (2, "i", "Surface"),
    (2, "ii", "Bunker"),
    (3, "i", "Silo"),
    (4, "i", "Frigate"),
    (5, "i", "Surface"),
    (5, "ii", "Bunker"),
    (6, "i", "Statue"),
    (6, "ii", "Archives"),
    (6, "iii", "Streets"),
    (6, "iv", "Depot"),
    (6, "v", "Train"),
    (7, "i", "Jungle"),
    (7, "ii", "Control"),
    (7, "iii", "Caverns"),
    (7, "iv", "Cradle"),
    (8, "i", "Aztec"),
    (9, "i", "Egyptian"),
];

/// GoldenEye 007 (n64decomp/007 `file2.c`, `front.c`): a 32-byte block at the EEPROM's start, then five saves of 0x60
/// bytes from 0x20, each Rare's two sums (big-endian, type A) over its bytes from 8 to its end; byte 8 holds the folder
/// (bits 0-2), the save's turn (3-4) and "free" (7); a folder whose two saves are both good uses the one whose turn
/// follows the other's. Byte 9 bit 0 opens 007; from 0x12 the best times, 10 bits each, high bit first, at bit
/// (difficulty × 20 + mission) × 10 for Agent, Secret Agent and 00 Agent, 0 for not done. The folder select names the
/// hardest difficulty done and the furthest mission done at it ("Mission 2.i"), or "007" alone once that is open (by
/// the bit or every mission done at 00 Agent). Which folder was played last isn't kept: the one that changed.
fn goldeneye(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const AT: usize = 0x20;
    const SIZE: usize = 0x60;
    if s.len() < AT + 5 * SIZE {
        return (None, Memory::default());
    }
    let good = |i: usize| -> Option<&[u8]> {
        let f = &s[AT + i * SIZE..AT + (i + 1) * SIZE];
        let (a, b) = rare_sums(&f[8..]);
        (u32::from_be_bytes([f[0], f[1], f[2], f[3]]) == a && u32::from_be_bytes([f[4], f[5], f[6], f[7]]) == b && f[8] & 0x80 == 0).then_some(f)
    };
    let mut folders: [Option<&[u8]>; 4] = [None; 4];
    for i in 0..5 {
        let Some(f) = good(i) else { continue };
        let folder = (f[8] & 7) as usize;
        if folder >= 4 {
            continue;
        }
        match folders[folder] {
            None => folders[folder] = Some(f),
            Some(old) if (f[8] >> 3 & 3) == ((old[8] >> 3 & 3) + 1) % 4 => folders[folder] = Some(f),
            Some(_) => {}
        }
    }
    let time = |f: &[u8], difficulty: usize, mission: usize| -> u16 {
        let at = (difficulty * 20 + mission) * 10;
        (0..10).fold(0u16, |v, k| v << 1 | (f[0x12 + (at + k) / 8] >> (7 - (at + k) % 8) & 1) as u16)
    };
    let done = |f: &[u8]| -> Option<(usize, usize)> { (0..3).rev().find_map(|d| (0..20).rev().find(|&m| time(f, d, m) != 0).map(|m| (d, m))) };
    let valid: Vec<(usize, u32)> = (0..4).filter_map(|i| folders[i].filter(|f| done(f).is_some() || f[9] & 1 != 0).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 4, before);
    let Some(f) = memory.current.and_then(|i| folders[i]) else { return (None, memory) };
    if f[9] & 1 != 0 || (0..20).all(|m| time(f, 2, m) != 0) {
        return (Some(Progress::Mission { mission: None, difficulty: "007" }), memory);
    }
    let Some((difficulty, mission)) = done(f) else { return (None, memory) };
    let (chapter, part, name) = GOLDENEYE_MISSIONS[mission];
    let difficulty = ["Agent", "Secret Agent", "00 Agent"][difficulty];
    (Some(Progress::Mission { mission: Some(format!("Mission {}: {name}", format_args!("{chapter}.{part}"))), difficulty }), memory)
}

/// Super Mario World: three files of 143 bytes at 0, 0x8F, 0x11E and a copy of each 0x1AD further on, used when the
/// first fails its sum (bytes plus the stored word make 0x5A5A, `VerifySaveFile`). The exits at byte 0x8C. In Super
/// Mario All-Stars + Super Mario World the cartridge keeps four files (A to D) from 0x1000 with the copies 0x23C on, and
/// the file chosen last at 0x0E (0 to 3; the file select's cursor starts there), as saves the game wrote show (15.11.).
fn mario_world(s: &[u8], (base, copy, files): (usize, usize, usize), before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x8F;
    if s.len() < (base + 0x800).max(base + copy + files * SIZE) {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        [base + i * SIZE, base + i * SIZE + copy].into_iter().map(|at| &s[at..at + SIZE]).find(|f| {
            let sum = f[..SIZE - 2].iter().fold(u16le(f, SIZE - 2), |a, b| a.wrapping_add(*b as u16));
            sum == 0x5A5A
        })
    };
    let valid: Vec<(usize, u32)> = (0..files).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, files, before);
    if files == 4 && (s[0x0E] as usize) < 4 && memory.prints[s[0x0E] as usize] != 0 {
        memory.current = Some(s[0x0E] as usize);
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let done = f[0x8C];
    if done > 96 {
        return (None, memory);
    }
    (Some(Progress::Exits { done, of: 96 }), memory)
}

/// Super Mario 64 (n64decomp/sm64, `struct SaveBuffer`): the EEPROM's first 512 bytes (also the first part of
/// mupen64plus's `.srm`), four files of 56 bytes with two copies each, big-endian; a copy is good when it ends in the
/// magic "DA" and the sum of its bytes before the checksum. Stars: 7 bits per course (25) and the castle's secret ones
/// in bits 24-30 of the flags, as `save_file_get_total_star_count` counts them.
fn mario_64(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 56;
    if s.len() < 512 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        [2 * i, 2 * i + 1].into_iter().map(|c| &s[c * SIZE..(c + 1) * SIZE]).find(|f| {
            let sum = f[..SIZE - 2].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
            f[52..54] == *b"DA" && u16::from_be_bytes([f[54], f[55]]) == sum && f[11] & 1 != 0
        })
    };
    let valid: Vec<(usize, u32)> = (0..4).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 4, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let flags = u32::from_be_bytes([f[8], f[9], f[10], f[11]]);
    let stars = ((flags >> 24) & 0x7F).count_ones() + f[12..37].iter().map(|c| (c & 0x7F).count_ones()).sum::<u32>();
    if stars > 120 {
        return (None, memory);
    }
    (Some(Progress::Stars { done: stars as u8, of: 120 }), memory)
}

/// Ocarina of Time (zeldaret/oot, `include/save.h`, `z_sram.c`): the SRAM (in mupen64plus's `.srm` from 0x20800; a
/// `.sra` alone), three slots of 0x1450 bytes from 0x20 on and their backups after them, each `Save` starting with its
/// big-endian data: "ZELDAZ" at 0x1C, the 16-bit words up to 0x1354 adding up (the sum's own word counted as 0) to the
/// sum at 0x1352. Heart containers 0x2E (16 a heart), quest items 0xA4 (medallions bits 0-5, stones 18-20).
fn ocarina(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SLOT: usize = 0x1450;
    const SAVE: usize = 0x1354;
    // The SRAM alone (0x8000) or inside mupen64plus's file; some emulators keep its 32-bit words the other way round.
    let sram: Vec<u8> = match s.len() {
        n if n >= 0x28800 => s[0x20800..0x28800].to_vec(),
        n if n >= 0x8000 => s[..0x8000].to_vec(),
        _ => return (None, Memory::default()),
    };
    let flipped: Vec<u8> = sram.chunks(4).flat_map(|w| w.iter().rev().copied()).collect();
    let sram = if sram[0x20 + 0x1C..0x20 + 0x22] != *b"ZELDAZ" && flipped.windows(6).any(|w| w == b"ZELDAZ") { flipped } else { sram };
    let file = |i: usize| -> Option<&[u8]> {
        [0x20 + i * SLOT, 0x20 + (i + 3) * SLOT].into_iter().map(|at| &sram[at..at + SAVE]).find(|f| {
            let sum = (0..SAVE / 2).filter(|w| w * 2 != 0x1352).fold(0u16, |a, w| a.wrapping_add(u16::from_be_bytes([f[w * 2], f[w * 2 + 1]])));
            f[0x1C..0x22] == *b"ZELDAZ" && sum == u16::from_be_bytes([f[0x1352], f[0x1353]])
        })
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let health = i16::from_be_bytes([f[0x2E], f[0x2F]]);
    let quest = u32::from_be_bytes([f[0xA4], f[0xA5], f[0xA6], f[0xA7]]);
    if health % 16 != 0 || !(48..=320).contains(&health) {
        return (None, memory);
    }
    let spirit = Progress::Spirit { stones: ((quest >> 18) & 7).count_ones() as u8, medallions: (quest & 0x3F).count_ones() as u8, hearts: (health / 16) as u8 };
    (Some(spirit), memory)
}

/// Paper Mario (pmret/papermario `fio.c`, `struct SaveData`, `SaveGlobals`): the flash (in mupen64plus's `.srm` from
/// 0x28800, or a `.fla`), six saves of 0x1380 bytes at 16 KiB steps and the globals at the 7th and 8th, big-endian. A
/// save is good with its "Mario Story 006", its two sums each other's complement and its 32-bit words adding up to
/// the first; the globals (the same checks over 0x80 bytes) name the file last chosen, and that file's newest save (by
/// its count) is read. What the file select shows is kept at 0x12EC: frames played (60 a second), spirits, level.
fn paper_mario(s: &[u8]) -> Option<Progress> {
    const PAGE: usize = 0x4000;
    let flash: Vec<u8> = match s.len() {
        n if n >= 0x48800 => s[0x28800..0x48800].to_vec(),
        n if n >= 0x20000 => s[..0x20000].to_vec(),
        _ => return None,
    };
    let flipped: Vec<u8> = flash.chunks(4).flat_map(|w| w.iter().rev().copied()).collect();
    let flash = if flash.starts_with(b"Mario Story 006") || !flipped.windows(15).any(|w| w == b"Mario Story 006") { flash } else { flipped };
    let be = |b: &[u8], at: usize| u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    let good = |b: &[u8]| {
        let sum = (0..b.len() / 4).fold(0u32, |a, w| a.wrapping_add(be(b, w * 4)));
        &b[..16] == b"Mario Story 006\0" && be(b, 0x30) == !be(b, 0x34) && sum == be(b, 0x30)
    };
    let last = [6usize, 7].into_iter().map(|p| &flash[p * PAGE..p * PAGE + 0x80]).find(|g| good(g)).map(|g| be(g, 0x3C) as usize);
    let saves: Vec<&[u8]> = (0..6).map(|p| &flash[p * PAGE..p * PAGE + 0x1380]).filter(|f| good(f)).collect();
    // The chosen file's newest save; without good globals, the newest save of all.
    let f = saves.iter().filter(|f| last.is_none_or(|l| be(f, 0x38) as usize == l)).max_by_key(|f| be(f, 0x3C))?;
    let meta = &f[0x12EC..0x12EC + 0x18];
    // A file never played (its clock 0): the file menu says "First Play" and shows nothing else, nor does pocket-ui.
    if be(meta, 0) == 0 {
        return None;
    }
    let frames = be(meta, 0).min(100 * 216_000 - 1);
    let (rescued, level) = (meta[4], meta[6]);
    if rescued > 7 || !(1..=27).contains(&level) {
        return None;
    }
    Some(Progress::StarSpirits { rescued, level, hours: (frames / 216_000) as u16, minutes: ((frames / 3600) % 60) as u8 })
}

/// Mario Kart 64 (n64decomp/mk64 `include/save_data.h`, `save.c`): the EEPROM's Grand Prix points at 0x180, one byte
/// per class (50cc, 100cc, 150cc, Extra) with two bits per cup (3 a gold cup), the sound mode at 0x184, and two check
/// bytes at 0x186 (`compute_save_data_checksum_1`) and 0x187 (the first plus 90).
fn mario_kart_64(s: &[u8]) -> Option<Progress> {
    if s.len() < 512 {
        return None;
    }
    let info = &s[0x180..0x185];
    let c1 = info.iter().enumerate().fold(0u32, |a, (i, b)| a + (*b as u32 + 1) * (i as u32 + 1) + i as u32) as u8;
    if s[0x186] != c1 || s[0x187] != c1.wrapping_add(90) || info[4] > 2 {
        return None;
    }
    let gold = info[..4].iter().map(|b| (0..4).filter(|cup| (b >> (2 * cup)) & 3 == 3).count()).sum::<usize>();
    Some(Progress::GoldCups { done: gold as u8, of: 16 })
}

/// Star Fox 64 (sonicdcer/sf64 `include/sf64save.h`, `fox_save.c`): the EEPROM's save of 0x100 bytes and its backup,
/// each its data (0xFE) and a 16-bit check (`Save_Checksum`); one byte of flags per planet at the start. The flags are
/// C bit fields, laid out by the N64's compiler from the high bit down: normal clear 0x01, normal medal 0x02, played
/// 0x04, expert clear 0x08, expert medal 0x10. A medal without its clear, or a clear never played, can't be: then
/// nothing is shown.
fn star_fox_64(s: &[u8]) -> Option<Progress> {
    if s.len() < 512 {
        return None;
    }
    let check = |d: &[u8]| -> u16 {
        let mut c: u16 = 0;
        for &b in &d[..0xFE] {
            c ^= b as u16;
            c <<= 1;
            c = (c & 0xFE) | ((c >> 8) & 1);
        }
        (c & 0xFF) | 0x9500
    };
    let save = [&s[..0x100], &s[0x100..0x200]].into_iter().find(|d| u16::from_be_bytes([d[0xFE], d[0xFF]]) == check(d))?;
    let planets = &save[..16];
    let sound_ok = save[0x14] < 4 && save[0x15] <= 99 && save[0x16] <= 99 && save[0x17] <= 99;
    let consistent = planets.iter().all(|p| (p & 0x02 == 0 || p & 0x01 != 0) && (p & 0x10 == 0 || p & 0x08 != 0) && (p & 0x09 == 0 || p & 0x04 != 0));
    if !sound_ok || !consistent {
        return None;
    }
    let normal = planets.iter().filter(|p| *p & 0x02 != 0).count() as u8;
    let expert = planets.iter().filter(|p| *p & 0x10 != 0).count() as u8;
    Some(Progress::Medals { normal, expert })
}

/// Diddy Kong Racing (DavidSM64/Diddy-Kong-Racing `save_layout.h`, `save_data.c`): three files of 40 bytes at the
/// EEPROM's start, each a stream of bit fields, high bit first; the first 16 bits minus 5 minus the bytes after them
/// make 0 (`populate_settings_from_save_data`), else the file is new. After 68 bits of courses, 6 of Taj, 10 of trophies
/// and 12 of bosses, 7 bits count the golden balloons of all worlds.
fn diddy_kong_racing(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 512 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        let f = &s[i * 40..(i + 1) * 40];
        let sum = f[2..].iter().fold(u16::from_be_bytes([f[0], f[1]]).wrapping_sub(5), |a, b| a.wrapping_sub(*b as u16));
        (sum == 0 && f[2..].iter().any(|b| *b != 0)).then_some(f)
    };
    let bits = |f: &[u8], from: usize, n: usize| (from..from + n).fold(0u32, |v, b| v << 1 | ((f[b / 8] >> (7 - b % 8)) & 1) as u32);
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let total = bits(f, 16 + 68 + 6 + 10 + 12, 7);
    let worlds: u32 = (1..6).map(|w| bits(f, 112 + 7 * w, 7)).sum();
    if total > 47 || worlds > total {
        return (None, memory);
    }
    (Some(Progress::Balloons { done: total as u8, of: 47 }), memory)
}

/// Rare's save checksum (bryc/rare-n64-chksm, the games' `glcrc_calcChecksum`): a 33-bit state stirred by every byte,
/// forwards then backwards, the two passes' results kept apart. Banjo-Kazooie stores them XORed, Banjo-Tooie side by side.
pub(crate) fn rare_sums(data: &[u8]) -> (u32, u32) {
    fn stir(state: &mut u64, b: u8, shift: u32) -> u32 {
        let s = state.wrapping_add((b as u64) << (shift & 15));
        let mixed = ((s >> 1 | s << 32) ^ ((s << 44) >> 32)) & 0x1_FFFF_FFFF;
        *state = mixed ^ ((mixed >> 20) & 0xFFF);
        *state as u32
    }
    let (mut state, mut shift) = (0x1_3108_B3C1u64, 0u32);
    let (mut forwards, mut backwards) = (0u32, 0u32);
    for &b in data {
        forwards ^= stir(&mut state, b, shift);
        shift += 7;
    }
    for &b in data.iter().rev() {
        backwards ^= stir(&mut state, b, shift);
        shift += 3;
    }
    (forwards, backwards)
}

/// Banjo-Kazooie (banjo.decomp/banjo-kazooie: `savedata_init`, `gameFile_8033CE40`, `gamenum`'s text in `gameSelect.c`;
/// the sum is Rare's): the EEPROM's first 480 bytes are four files of 0x78, each 0x11, the game it holds (1 to 3), then
/// the Jiggies' bits from 2 (Jiggy n in byte (n - 1) / 8, bit n & 7; 100 in the ten worlds), the honeycombs and Mumbo
/// tokens, the best notes of the nine note worlds as 7-bit fields of the big-endian u64 at 0x22 (Mumbo's Mountain in the
/// highest), the seconds spent in the eleven places as big-endian u16 from 0x2A, and the sum at 0x74. Saving a game
/// writes it to the spare file and wipes the old one, so the files move about; the game select counts the first good
/// file of each game, and so does this. Which game was played last isn't kept: the one whose file changed.
fn banjo_kazooie(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x78;
    if s.len() < 512 {
        return (None, Memory::default());
    }
    let mut games: [Option<&[u8]>; 3] = [None; 3];
    for i in 0..4 {
        let f = &s[i * SIZE..(i + 1) * SIZE];
        let (a, b) = rare_sums(&f[..0x74]);
        let game = f[1] as usize;
        if f[0] == 0x11 && (1..=3).contains(&game) && a ^ b == u32::from_be_bytes([f[0x74], f[0x75], f[0x76], f[0x77]]) && games[game - 1].is_none() {
            games[game - 1] = Some(f);
        }
    }
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|g| games[g].map(|f| (g, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(|g| games[g]) else { return (None, memory) };
    let jiggies = (1..=100usize).filter(|n| f[2 + (n - 1) / 8] >> (n & 7) & 1 != 0).count() as u8;
    let notes_field = u64::from_be_bytes(f[0x22..0x2A].try_into().unwrap());
    let notes: Vec<u16> = (0..9).map(|w| (notes_field >> (7 * w) & 0x7F) as u16).collect();
    let seconds: u32 = (0..11).map(|p| u16::from_be_bytes([f[0x2A + 2 * p], f[0x2B + 2 * p]]) as u32).sum();
    // A world has 100 notes; the top bit of the field is never set.
    if notes.iter().any(|n| *n > 100) || notes_field >> 63 != 0 {
        return (None, memory);
    }
    let progress = Progress::Jiggies { done: jiggies, of: 100, notes: notes.iter().sum(), hours: (seconds / 3600) as u16, minutes: (seconds / 60 % 60) as u8 };
    (Some(progress), memory)
}

/// Final Fantasy III (USA), the sixth (everything8215/ff6 `save.asm`, Data Crystal's RAM map): three slots of 0xA00
/// bytes, each a copy of the game's $7E:1600 on, whose bytes up to 0x9FD add up (16 bits) to the word at 0x9FE; the
/// slot chosen last at 0x1FF0 (1 to 3). In a slot: 16 characters of 0x25 bytes (name at +2, level at +8), from 0x250
/// one byte each (party in bits 0-2, battle row in bits 3-4, 0x40 in the party at all), the active party at 0x46D, the
/// play time at 0x263. The leader is who the save screen shows: of the active party's characters, the one in the first
/// row (`InitCharProp`, menu_init.asm; checked in saves the game wrote after a change of order, 15.11.).
fn final_fantasy_6(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0xA00;
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        let f = &s[i * SIZE..(i + 1) * SIZE];
        let sum = f[..0x9FE].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        (sum == u16le(f, 0x9FE) && f.iter().any(|b| *b != f[0])).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    let chosen = s[0x1FF0] as usize;
    if (1..=3).contains(&chosen) && memory.prints[chosen - 1] != 0 {
        memory.current = Some(chosen - 1);
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let (hours, minutes) = (f[0x263] as u16, f[0x264]);
    if minutes > 59 || f[0x265] > 59 {
        return (None, memory);
    }
    // The leader of the active party: of its characters (0x40, party bits 0-2), the one in the first row (bits 3-4).
    let party = f[0x46D] & 7;
    let lead = (0..16usize).filter(|c| f[0x250 + c] & 0x40 != 0 && f[0x250 + c] & 7 == party && party != 0).min_by_key(|c| (f[0x250 + c] >> 3) & 3).and_then(|c| {
        let ch = &f[c * 0x25..(c + 1) * 0x25];
        let level = ch[8];
        let name = ff6_name(&ch[2..8])?;
        (1..=99).contains(&level).then_some((name, level))
    });
    (Some(Progress::Hero { lead, hours, minutes }), memory)
}

/// EarthBound (Herringway/ebsrc `include/structs.asm`, `system/saves/`): six blocks of 1280 bytes, a file in two
/// (blocks 2n and 2n+1, the second a copy), each starting with "HAL Laboratory, inc.", a 16-bit sum of the bytes from
/// 32 on (at 28) and the XOR of their 16-bit words (at 30); the game writes the header into all six at its first start,
/// so a file is there when it holds more than that. From 32 the game state (the party's character numbers at +122, 1 to
/// 4 the four friends), from 505 the characters, 95 bytes each: name (5, ended by 0), level (+5) (checked in a save the
/// game wrote, 15.11.).
fn earthbound(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 1280;
    if s.len() < 6 * SIZE {
        return (None, Memory::default());
    }
    let good = |b: &[u8]| {
        let sum = b[32..].iter().fold(0u16, |a, x| a.wrapping_add(*x as u16));
        let xor = (32..SIZE).step_by(2).fold(0u16, |a, w| a ^ u16le(b, w));
        // The game writes the header into all six blocks at its first start, the empty ones with nothing after it.
        b.starts_with(b"HAL Laboratory, inc.") && sum == u16le(b, 28) && xor == u16le(b, 30) && b[32..].iter().any(|x| *x != 0)
    };
    let file = |i: usize| -> Option<&[u8]> { [2 * i, 2 * i + 1].into_iter().map(|k| &s[k * SIZE..(k + 1) * SIZE]).find(|b| good(b)) };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let party = &f[32 + 122..32 + 128];
    let leader = party[0] as usize;
    if !(1..=4).contains(&leader) {
        return (None, memory);
    }
    let ch = &f[505 + (leader - 1) * 95..505 + leader * 95];
    let level = ch[5];
    let Some(name) = earthbound_name(&ch[..5]) else { return (None, memory) };
    if !(1..=99).contains(&level) {
        return (None, memory);
    }
    let friends = party[1..].iter().filter(|c| (1..=4).contains(*c)).count() as u8;
    (Some(Progress::Party { lead: (name, level), friends }), memory)
}

/// A name in EarthBound's letters (USA: A-Z from 0x71, a-z from 0x91, as its own name table has "Ness" and "Paula"),
/// 0 the end; None for any other byte.
fn earthbound_name(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0 => break,
            0x71..=0x8A => (b'A' + (b - 0x71)) as char,
            0x91..=0xAA => (b'a' + (b - 0x91)) as char,
            _ => return None,
        };
        out.push(c);
    }
    (!out.is_empty()).then_some(out)
}

/// A name in Final Fantasy III's letters (USA): A-Z from 0x80, a-z from 0x9A, digits from 0xB4, 0xFF the end. None for
/// any other byte.
fn ff6_name(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0xFF => break,
            0x80..=0x99 => (b'A' + (b - 0x80)) as char,
            0x9A..=0xB3 => (b'a' + (b - 0x9A)) as char,
            0xB4..=0xBD => (b'0' + (b - 0xB4)) as char,
            0xFE => ' ',
            _ => return None,
        };
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

/// Link's Awakening DX (zladx/LADX-Disassembly `constants/memory/sram.asm`, `wram.asm`): from 0x100 on three files of
/// 0x3AD bytes, each starting with 1, 3, 5, 7, 9 (it has no sum), then its $D800-$DB7F: the name at $DB4F-$DB53, heart
/// containers at $DB5B, the eight instruments at $DB65-$DB6C (bit 1 each). The game writes the 1, 3, 5, 7, 9 into all
/// three places when it first starts; a file is there when its name is (the player select's count, `func_5DC0`).
fn links_awakening(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x3AD;
    if s.len() < 0x100 + 3 * SIZE {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        let f = &s[0x100 + i * SIZE..0x100 + (i + 1) * SIZE];
        let name = &f[5 + 0xDB4F - 0xD800..5 + 0xDB54 - 0xD800];
        (f[..5] == [1, 3, 5, 7, 9] && name.iter().any(|&b| b != 0)).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let at = |ram: usize| f[5 + ram - 0xD800];
    let hearts = at(0xDB5B);
    let instruments: Vec<u8> = (0xDB65..=0xDB6C).map(at).collect();
    // Nothing here has a sum, so only what the game can hold: 3 to 14 hearts, instrument bytes of the flags it sets.
    if !(3..=14).contains(&hearts) || instruments.iter().any(|b| b & !0x07 != 0) {
        return (None, memory);
    }
    let done = instruments.iter().filter(|b| *b & 0x02 != 0).count() as u8;
    (Some(Progress::Instruments { done, hearts }), memory)
}

/// Majora's Mask (USA; zeldaret/mm `z_sram_NES.c`, `include/z64save.h`): the flash (128 KiB; in mupen64plus's `.srm`
/// from 0x28800), pages of 128 bytes. Each of the two files has its new-cycle save at page 0 or 0x80 (backup 0x40
/// further on) and its owl save at page 0x100 or 0x200 (backup at 0x180, 0x280); the owl save, when there is one, is
/// where the player stopped. A save is good when it says "ZELDA3" at 0x24 and its bytes, its sum's own word as 0, add up
/// to that word at 0x100A (over 0x100C bytes, an owl save over 0x3CA0), big-endian; the most health at 0x34 (16 a heart),
/// the quest items at 0xBC (the remains bits 0-3, `QUEST_REMAINS_*`). Some emulators keep the 32-bit words the other way
/// round: whichever order shows "ZELDA3" is read.
fn majoras_mask(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    let flash: Vec<u8> = match s.len() {
        n if n >= 0x48800 => s[0x28800..0x48800].to_vec(),
        n if n >= 0x20000 => s[..0x20000].to_vec(),
        _ => return (None, Memory::default()),
    };
    let flipped: Vec<u8> = flash.chunks(4).flat_map(|w| w.iter().rev().copied()).collect();
    let starts = [0usize, 0x2000, 0x4000, 0x6000, 0x8000, 0xC000, 0x10000, 0x14000];
    let marked = |f: &[u8]| starts.iter().any(|&at| f[at + 0x24..at + 0x2A] == *b"ZELDA3");
    let flash = if !marked(&flash) && marked(&flipped) { flipped } else { flash };
    let good = |at: usize, size: usize| -> Option<&[u8]> {
        let f = &flash[at..at + size];
        let sum = f.iter().enumerate().filter(|(i, _)| !(0x100A..0x100C).contains(i)).fold(0u16, |a, (_, b)| a.wrapping_add(*b as u16));
        (f[0x24..0x2A] == *b"ZELDA3" && sum == u16::from_be_bytes([f[0x100A], f[0x100B]])).then_some(f)
    };
    // File i: its owl save (main, then backup), else its new-cycle save.
    let file = |i: usize| -> Option<&[u8]> {
        let owl = [0x8000 + 0x8000 * i, 0xC000 + 0x8000 * i].into_iter().find_map(|at| good(at, 0x3CA0));
        owl.or_else(|| [0x4000 * i, 0x4000 * i + 0x2000].into_iter().find_map(|at| good(at, 0x100C)))
    };
    let valid: Vec<(usize, u32)> = (0..2).filter_map(|i| file(i).map(|f| (i, print(&f[..0x100C])))).collect();
    let memory = pick(&valid, 2, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let max = i16::from_be_bytes([f[0x34], f[0x35]]);
    if max % 16 != 0 || !(48..=320).contains(&max) {
        return (None, memory);
    }
    let remains = (f[0xBF] & 0x0F).count_ones() as u8;
    (Some(Progress::Remains { done: remains, hearts: (max / 16) as u8 }), memory)
}

/// Oracle of Ages and Seasons (Stewmath/oracles-disasm `code/fileManagement.s`): three files of 0x550 bytes at 0x10,
/// 0x560, 0xAB0 with copies at 0x1000, 0x1550, 0x1AA0; each starts with the 16-bit sum of its words after the first and
/// the game's string ("Z21216-0" Ages, "Z11216-0" Seasons). The file is the game's $C5B0 on (`include/wram.s`): the most
/// health (4 a heart) at $C6AB in Ages, $C6A3 in Seasons; the essences, a bit each, at $C6BF and $C6BB.
fn oracle(s: &[u8], ages: bool, before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x550;
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let string: &[u8] = if ages { b"Z21216-0" } else { b"Z11216-0" };
    let file = |i: usize| -> Option<&[u8]> {
        [[0x10, 0x560, 0xAB0][i], [0x1000, 0x1550, 0x1AA0][i]].into_iter().map(|at| &s[at..at + SIZE]).find(|f| {
            let sum = (1..SIZE / 2).fold(0u16, |a, w| a.wrapping_add(u16le(f, 2 * w)));
            sum == u16le(f, 0) && &f[2..10] == string
        })
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let (max, essences) = if ages { (f[0xC6AB - 0xC5B0], f[0xC6BF - 0xC5B0]) } else { (f[0xC6A3 - 0xC5B0], f[0xC6BB - 0xC5B0]) };
    if max % 4 != 0 || !(12..=80).contains(&max) {
        return (None, memory);
    }
    (Some(Progress::Essences { done: essences.count_ones() as u8, hearts: max / 4 }), memory)
}

/// The Minish Cap's EEPROM (8 KiB; zeldaret/tmc `save.c`): three files of 0x500 bytes at 0x80, 0x580, 0xA80, each with
/// a copy 0x1000 further on and an 8-byte status (0x30, 0x40, 0x50; copies at 0x1030…): the sum, its negative and
/// "MCZ3", the sum being `CalculateChecksum` of the status word and the file. The header (status 0x20, 16 bytes at 0x70)
/// says which file was chosen last. Emulators keep the EEPROM in 8-byte blocks as the chip has them, which some store
/// byte for byte reversed: whichever order shows the game's "MCZ3" is read.
fn minish_cap(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let straight = &s[..0x2000];
    let reversed: Vec<u8> = straight.chunks(8).flat_map(|b| b.iter().rev().copied()).collect();
    let mcz3 = |e: &[u8]| (0..4).any(|i| (0..2).any(|c| &e[0x20 + 0x10 * i + 0x1000 * c + 4..0x20 + 0x10 * i + 0x1000 * c + 8] == b"3ZCM"));
    let e: &[u8] = if mcz3(straight) { straight } else if mcz3(&reversed) { &reversed } else { return (None, Memory::default()) };
    // `CalculateChecksum`: each 16-bit word XOR the bytes still to go, added up.
    let sum = |b: &[u8]| (0..b.len() / 2).fold(0u32, |a, w| a.wrapping_add((u16le(b, 2 * w) ^ (b.len() - 2 * w) as u16) as u32)) as u16;
    // A place's data when one of its two copies has a valid status (`ReadSaveFileStatus` tries the 8 bytes after too)
    // and the sum.
    let read = |status: usize, data: usize, size: usize| -> Option<&[u8]> {
        [0, 0x1000].into_iter().find_map(|c| {
            let d = &e[data + c..data + c + size];
            [status + c, status + c + 8].into_iter().find_map(|at| {
                let (one, two) = (u16le(e, at), u16le(e, at + 2));
                let want = sum(&e[at + 4..at + 8]).wrapping_add(sum(d));
                (&e[at + 4..at + 8] == b"3ZCM" && one == want && two == want.wrapping_neg()).then_some(d)
            })
        })
    };
    let file = |i: usize| read(0x30 + 0x10 * i, 0x80 + 0x500 * i, 0x500).filter(|f| f[0] == 0 && f[1] == 1);
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    // The header: "the signature", then the file chosen last.
    if let Some(h) = read(0x20, 0x70, 0x10) {
        if (h[4] as usize) < 3 && memory.prints[h[4] as usize] != 0 {
            memory.current = Some(h[4] as usize);
        }
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    // `Stats` at 0xA8: the most health at 3 (8 a heart, 3 to 20 hearts); the inventory at 0xF2, 2 bits an item
    // (`GetInventoryValue`), the elements items 0x40 to 0x43.
    let max = f[0xA8 + 3];
    if max % 8 != 0 || !(24..=160).contains(&max) {
        return (None, memory);
    }
    let elements = f[0xF2 + 0x10];
    let done = (0..4).filter(|k| (elements >> (2 * k)) & 3 != 0).count() as u8;
    (Some(Progress::Elements { done, hearts: max / 8 }), memory)
}

/// A Link to the Past: three files of 0x500 bytes, each with a mirror 0xF00 further on, marked 0x55AA at 0x3E5, whose
/// words add up to 0x5A5A; the file is the game's $7E:F000 on.
fn link_to_the_past(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0x500;
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        [i * SIZE, i * SIZE + 0xF00].into_iter().map(|at| &s[at..at + SIZE]).find(|f| {
            let sum = (0..SIZE / 2).fold(0u16, |a, w| a.wrapping_add(u16le(f, w * 2)));
            sum == 0x5A5A && u16le(f, 0x3E5) == 0x55AA
        })
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let (health, progress) = (f[0x36C], f[0x3C5]);
    if health % 8 != 0 || !(24..=160).contains(&health) || progress > 3 {
        return (None, memory);
    }
    let quest = Progress::Quest { pendants: (f[0x374] & 0x07).count_ones() as u8, crystals: (f[0x37A] & 0x7F).count_ones() as u8, hearts: health / 8, dark_world: progress >= 3 };
    (Some(quest), memory)
}

/// Fire Emblem (USA, the seventh; FireEmblemUniverse-style decomp 7ucina/fireemblem7u) and The Sacred Stones (USA, the
/// eighth; FireEmblemUniverse/fireemblem8u), the same 32 KiB layout (`sram-layout.h`, `bmsave-lib.c`, `bmsave.c`): at 0
/// the global header (magics at 8 and 0xC, `Checksum16` of its first 0x50 bytes at 0x60, the file saved last at 0x62),
/// at 0x64 one block entry of 0x10 bytes per save (magics, the block's place and size, `Checksum32` of its bytes), the
/// three games first. A game block starts with the play state (`struct PlaySt`): the clock in frames at 0, the chapter
/// at 0xE, the state bits at 0x14 (0x04 after the end, 0x20 finished, 0x40 the hard mode), the story at 0x1B, in The
/// Sacred Stones the options at 0x40 (bit 21: Normal rather than Easy) and the save menu's kind at 0x4A.
fn fire_emblem(s: &[u8], eighth: bool, before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const MAGIC16: u16 = 0x200A;
    let (head_magic, game_magic) = if eighth { (0x40624, 0x40624) } else { (0x30317, 0x11217) };
    if s.len() < 0x8000 {
        return (None, Memory::default());
    }
    let sum16 = |b: &[u8]| {
        let (add, xor) = (0..b.len() / 2).fold((0u32, 0u32), |(a, x), w| (a + u16le(b, 2 * w) as u32, x ^ u16le(b, 2 * w) as u32));
        add.wrapping_add(xor) as u16
    };
    if u32le(s, 8) != head_magic || u16le(s, 0xC) != MAGIC16 || sum16(&s[..0x50]) != u16le(s, 0x60) {
        return (None, Memory::default());
    }
    let block = |i: usize| -> Option<&[u8]> {
        let e = &s[0x64 + i * 0x10..0x64 + (i + 1) * 0x10];
        let (at, size) = (u16le(e, 8) as usize, u16le(e, 10) as usize);
        if u32le(e, 0) != game_magic || u16le(e, 4) != MAGIC16 || size < 0x4C || at + size > s.len() {
            return None;
        }
        let b = &s[at..at + size];
        let (add, xor) = (0..size / 2).fold((0u32, 0u32), |(a, x), w| (a + u16le(b, 2 * w) as u32, x ^ u16le(b, 2 * w) as u32));
        ((add & 0xFFFF) | (xor << 16) == u32le(e, 12)).then_some(b)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| block(i).map(|b| (i, print(b)))).collect();
    let mut memory = pick(&valid, 3, before);
    let last = s[0x62] as usize;
    if last < 3 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(b) = memory.current.and_then(block) else { return (None, memory) };
    let frames = u32le(b, 0);
    let (hours, minutes) = (frames / 216_000, (frames / 3600 % 60) as u8);
    let (index, bits, story) = (b[0xE], b[0x14], b[0x1B]);
    if hours > 9999 || !(1..=3).contains(&story) {
        return (None, memory);
    }
    let chapter = if bits & 0x20 != 0 {
        Some("Epilogue".to_string())
    } else if eighth && (bits & 0x04 != 0 || (b[0x4A] >> 1) & 7 == 4) {
        // After the end (the creature campaign) or saved on a skirmish or tower: the menu names a place of the world map.
        None
    } else if eighth {
        fe8_chapter(index)
    } else {
        fe7_chapter(index, story)
    };
    let lord = match (eighth, story) {
        (false, 1) => Some("Lyn"),
        (false, 2) => Some("Eliwood"),
        (false, 3) => Some("Hector"),
        (true, 2) => Some("Eirika"),
        (true, 3) => Some("Ephraim"),
        _ => None,
    };
    // The mode (the hard bit, The Sacred Stones' Normal bit) is on no screen of the games' save menus, so no save the
    // game wrote could confirm it (15.11.): not shown (`Release::verified`'s rule: a wrong line is worse than none).
    (Some(Progress::Chapter { chapter, lord, mode: None, hours: hours as u16, minutes }), memory)
}

/// A chapter's name as a part of the line: "Prologue", "Chapter 9", "Chapter 7x", "Final chapter".
fn chapter_name(n: &str) -> String {
    match n {
        "0" => "Prologue".into(),
        "F" => "Final chapter".into(),
        n if n.ends_with('x') => format!("Chapter {}x", &n[..n.len() - 1]),
        n => format!("Chapter {n}"),
    }
}

/// The seventh's chapters by number (7ucina/fireemblem7u `constants/chapters.h`) as each story counts them: Lyn's
/// tale, then Eliwood's and Hector's ("" where that story doesn't have it).
fn fe7_chapter(index: u8, story: u8) -> Option<String> {
    const LYN: [&str; 12] = ["0", "1", "2", "3", "4", "5", "6", "7", "7x", "8", "9", "10"];
    const LORDS: [(&str, &str); 36] = [
        ("11", ""), ("", "11"), ("12", "12"), ("13", "13"), ("13x", "13x"), ("14", "14"), ("", "15"), ("15", "16"),
        ("16", "17"), ("16x", "17x"), ("17", "18"), ("18", "19"), ("18x", "19x"), ("", "19x"), ("19", "20"),
        ("20", "21"), ("21", "22"), ("22", "23"), ("22x", "23x"), ("23", "24"), ("23", "24"), ("", "25"), ("24", "26"),
        ("25", "27"), ("25", "27"), ("26", "28"), ("26x", "28x"), ("27", "29"), ("28", ""), ("", "30"), ("29", "31"),
        ("29x", "31x"), ("30", "32"), ("", "32x"), ("F", "F"), ("F", "F"),
    ];
    let n = match story {
        1 => *LYN.get(index as usize)?,
        2 => LORDS.get((index as usize).checked_sub(0x0C)?)?.0,
        _ => LORDS.get((index as usize).checked_sub(0x0C)?)?.1,
    };
    (!n.is_empty()).then(|| chapter_name(n))
}

/// The Sacred Stones' chapters by number (fireemblem8u `constants/chapters.h`): the common part, then Eirika's and
/// Ephraim's routes (each with its chapter 11 apart). The towers, ruins and skirmishes have none.
fn fe8_chapter(index: u8) -> Option<String> {
    const COMMON: [&str; 10] = ["0", "1", "2", "3", "4", "5x", "5", "6", "7", "8"];
    const ROUTE: [&str; 13] = ["9", "10", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "21x"];
    let n = match index {
        0x00..=0x09 => COMMON[index as usize],
        0x0A..=0x16 => ROUTE[index as usize - 0x0A],
        0x17..=0x23 => ROUTE[index as usize - 0x17],
        0x3D | 0x3E => "11",
        _ => return None,
    };
    Some(chapter_name(n))
}

/// Donkey Kong Country 1, 2 and 3 (USA), three files in 2 KiB, none saying it was played last (the one whose bytes
/// changed is). The first: six slots of 0x154 bytes from 8, a file the first of each pair (the second is the contest's
/// other player), "ARER" at 4, the XOR of the words from 4 to 0x114 at 0, their sum at 2; the share at 0x0E. The
/// second and third: files of 0x2A8 bytes from 8 (0x28A bytes from 0x62 in the third), the sum and XOR of the words
/// from 6 up to 0x2A0 (0x288) at 0 and 2, 0x52/0x53 and the file's kind (0-2) at 4; the first player's part from 6:
/// its clock in frames at +0 (the third +0x0A), its share at +4 (+0x0E). The third stores a sum of 0 as 1.
fn donkey_kong(s: &[u8], kind: Kind, before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x800 {
        return (None, Memory::default());
    }
    let words = |f: &[u8], from: usize, to: usize| (from..=to).step_by(2).fold((0u16, 0u16), |(a, x), w| (a.wrapping_add(u16le(f, w)), x ^ u16le(f, w)));
    let kong = |f: &[u8]| f[4] & 0xFE == 0x52 && f[5] <= 2;
    let file = |i: usize| -> Option<&[u8]> {
        match kind {
            DonkeyKong1 => {
                let slot = |k: usize| &s[8 + k * 0x154..8 + (k + 1) * 0x154];
                let good = |f: &[u8]| {
                    let (sum, xor) = words(f, 4, 0x114);
                    &f[4..8] == b"ARER" && xor == u16le(f, 0) && sum == u16le(f, 2)
                };
                let f = slot(2 * i);
                // A contest file is only there with its second player's slot.
                (good(f) && (f[8] != 2 || good(slot(2 * i + 1)))).then_some(f)
            }
            DonkeyKong2 => {
                let f = &s[8 + i * 0x2A8..8 + (i + 1) * 0x2A8];
                let (sum, xor) = words(f, 6, 0x2A0);
                (kong(f) && sum == u16le(f, 0) && xor == u16le(f, 2)).then_some(f)
            }
            _ => {
                let f = &s[0x62 + i * 0x28A..0x62 + (i + 1) * 0x28A];
                let (sum, xor) = words(f, 6, 0x288);
                (kong(f) && u16le(f, 0) != 0 && sum.max(1) == u16le(f, 0) && xor == u16le(f, 2)).then_some(f)
            }
        }
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let (percent, of, frames) = match kind {
        DonkeyKong1 => (f[0x0E], 101, None),
        DonkeyKong2 => (f[6 + 4], 102, Some(u32le(f, 6))),
        _ => (f[6 + 0x0E], 105, Some(u32le(f, 6 + 0x0A))),
    };
    if percent > of {
        return (None, memory);
    }
    let clock = frames.map(|f| ((f / 216_000) as u16, (f / 3600 % 60) as u8)).filter(|(h, _)| *h < 1000);
    (Some(Progress::Complete { percent, of, clock }), memory)
}

/// Chrono Trigger (USA): three slots of 0xA00 bytes, each with its sum at 0x1FF0 + 2n (the 16-bit words added from the
/// slot's end to its start, the carry of each addition going into the next, the last one dropped) and 0xE41B at
/// 0x1FF8 + 2n when it holds a game; the slot saved last at 0x1FE0. In a slot: the party (character numbers 0-6) at
/// 0x580, the names (6 bytes each) at 0x5B0, a character's level at 0x200 + 0x50n + 0x12, the clock from 0x5E3 as the game
/// keeps it in RAM ($7E0400): frames, seconds, then one byte per digit, minute units, minute tens, hour units, hour tens (its
/// menu's TIME is tens * 10 + units; checked against the game's own saves and display, not Data Crystal's map).
fn chrono_trigger(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let slot = |i: usize| -> Option<&[u8]> {
        let f = &s[i * 0xA00..(i + 1) * 0xA00];
        let mut sum = 0u32;
        for w in (0..0x500).rev() {
            if sum > 0xFFFF {
                sum -= 0xFFFF;
            }
            sum += u16le(f, 2 * w) as u32;
        }
        (sum as u16 == u16le(s, 0x1FF0 + 2 * i) && u16le(s, 0x1FF8 + 2 * i) == 0xE41B).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| slot(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    let last = s[0x1FE0] as usize;
    if last < 3 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(f) = memory.current.and_then(slot) else { return (None, memory) };
    let [seconds, min_units, min_tens, hour_units, hour_tens] = [f[0x5E4], f[0x5E5], f[0x5E6], f[0x5E7], f[0x5E8]];
    if seconds > 59 || min_units > 9 || min_tens > 5 || hour_units > 9 || hour_tens > 9 {
        return (None, memory);
    }
    let (hours, minutes) = (hour_tens as u16 * 10 + hour_units as u16, min_tens * 10 + min_units);
    let leader = f[0x580] as usize;
    let lead = if leader < 7 {
        let level = f[0x200 + 0x50 * leader + 0x12];
        chrono_name(&f[0x5B0 + 6 * leader..0x5B0 + 6 * leader + 6]).filter(|_| (1..=99).contains(&level)).map(|n| (n, level))
    } else {
        None
    };
    (Some(Progress::Hero { lead, hours, minutes }), memory)
}

/// A name in Chrono Trigger's letters (USA): A-Z from 0xA0, a-z from 0xBA, digits from 0xD4, 0xFF a space, 0 the end.
fn chrono_name(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0 => break,
            0xA0..=0xB9 => (b'A' + (b - 0xA0)) as char,
            0xBA..=0xD3 => (b'a' + (b - 0xBA)) as char,
            0xD4..=0xDD => (b'0' + (b - 0xD4)) as char,
            0xFF => ' ',
            _ => return None,
        };
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then(|| shown_name(&out))
}

/// Golden Sun (`lost_age`: The Lost Age), 64 KiB of flash in records that start "CAMELOT", their number (the three
/// files are 0-2) at 7, the sum of their bytes from 0x10 (16 bits) at 8 and a count that grows with each save at 0xA;
/// of two records with one number the higher count is the newer. The first game's records are 0x1000 bytes (the sum
/// leaving out the last 8), The Lost Age's 0x3000. From 0x10 the file: the leader's name (12 bytes, 0 the end) and
/// level (0: no file) and the clock in frames at 0x10, as the file screen shows them (the games' file screens put a
/// clock written at 0x10 of a test's save on screen and leave the one at 0x244 aside, 15.11.).
fn golden_sun(s: &[u8], lost_age: bool, before: Option<&Memory>) -> (Option<Progress>, Memory) {
    let (size, summed) = if lost_age { (0x3000, 0x3000) } else { (0x1000, 0xFF8) };
    if s.len() < 0x10000 {
        return (None, Memory::default());
    }
    let mut newest: [Option<(u16, usize)>; 3] = [None; 3];
    for at in (0..0x10000 - size + 1).step_by(size) {
        let r = &s[at..at + size];
        let number = r[7] as usize;
        if &r[..7] != b"CAMELOT" || number > 2 {
            continue;
        }
        let sum = r[0x10..summed].iter().fold(0u32, |a, b| a + *b as u32) as u16;
        if sum != u16le(r, 8) {
            continue;
        }
        let count = u16le(r, 0xA);
        if newest[number].is_none_or(|(c, _)| count > c) {
            newest[number] = Some((count, at));
        }
    }
    let file = |i: usize| -> Option<&[u8]> { newest[i].map(|(_, at)| &s[at + 0x10..at + size]).filter(|f| f[0x0C] != 0) };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let level = f[0x0C];
    let frames = u32le(f, 0x10);
    let (hours, minutes) = (frames / 216_000, (frames / 3600 % 60) as u8);
    if level > 99 || hours > 999 {
        return (None, memory);
    }
    let lead = golden_sun_name(&f[..12]).map(|n| (n, level));
    (Some(Progress::Hero { lead, hours: hours as u16, minutes }), memory)
}

/// A name in Golden Sun's letters: plain Latin-1 (the Western releases), 0 the end; None for anything but letters.
fn golden_sun_name(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        match b {
            0 => break,
            b'A'..=b'Z' | b'a'..=b'z' | b' ' | b'-' | b'.' | b'\'' | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0xFF => out.push(b as char),
            _ => return None,
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// Kirby & the Amazing Mirror (USA; katam `save.c`): a block is an 8-byte check (the sum of its 16-bit words, its
/// complement, its negative, 0) and then its bytes; each comes twice, the first copy believed when it is good. First the
/// game's own block (4 bytes, twice), from 24 the three files' world, 0x30C bytes each (twice: copies 2n and 2n+1):
/// 0x20 + 0x80 bytes of the world's state, the doors visited (0x23E), the treasures (0x24: the chests' 16 bytes, then
/// the shards' bits as a 32-bit word), the two encounters (4 each) and the helpers' state (2), the sizes of the ROM's own
/// table (`gWorldProps`; checked in a save the game wrote, 15.11.). A world never played is all zeros. The
/// file select's percentage is worked out from tables in the ROM at each start and never saved: the shards are what
/// the save itself says.
fn kirby_mirror(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const WORLD: usize = 0x30C;
    const AT: usize = 2 * (8 + 4);
    if s.len() < AT + 6 * (8 + WORLD) {
        return (None, Memory::default());
    }
    let copy = |k: usize| -> Option<&[u8]> {
        let b = &s[AT + k * (8 + WORLD)..AT + (k + 1) * (8 + WORLD)];
        let a = (0..WORLD / 2).fold(0u16, |a, w| a.wrapping_add(u16le(b, 8 + 2 * w)));
        let check = [a, !a, (!a).wrapping_add(1), 0];
        (0..4).all(|i| u16le(b, 2 * i) == check[i]).then_some(&b[8..])
    };
    let world = |i: usize| -> Option<&[u8]> { copy(2 * i).or_else(|| copy(2 * i + 1)).filter(|w| w.iter().any(|b| *b != 0)) };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| world(i).map(|w| (i, print(w)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(w) = memory.current.and_then(world) else { return (None, memory) };
    let shards = u32le(w, 0x20 + 0x80 + 0x23E + 0x10);
    if shards & !0xFF != 0 {
        return (None, memory);
    }
    (Some(Progress::Shards { done: shards.count_ones() as u8 }), memory)
}

/// Super Mario RPG (USA; Yoshifanatic1's disassembly, checked against the ROM): four slots of 0x800 bytes in the first
/// 8 KiB (snes9x writes 32 KiB), each a copy of $7FF800-$7FFFFB: its bytes added up (16 bits), the complement of that
/// sum at 0x7FC; 0xFF in the first byte of a free slot; the slot chosen last in slot 0 at 0x7FE. Mario's level is a
/// slot's first byte, as the file select shows it; the game keeps no clock. Checked against a save the game wrote.
fn mario_rpg(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let slot = |i: usize| -> Option<&[u8]> {
        let f = &s[i * 0x800..(i + 1) * 0x800];
        let sum = f[..0x7FC].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        (f[0] != 0xFF && !sum == u16le(f, 0x7FC)).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..4).filter_map(|i| slot(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 4, before);
    let last = s[0x7FE] as usize;
    if last < 4 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(f) = memory.current.and_then(slot) else { return (None, memory) };
    let level = f[0];
    if !(1..=30).contains(&level) {
        return (None, memory);
    }
    (Some(Progress::Party { lead: ("Mario".to_string(), level), friends: 0 }), memory)
}

/// Lufia II's random numbers (the game's `$80:82E7`/`$80:832D`/`$80:914B`): a subtractive generator of 55 bytes seeded
/// with one byte, each number scaled to 0-254.
struct Lufia2Random {
    t: [u8; 55],
    i: usize,
}

impl Lufia2Random {
    fn new(seed: u8) -> Self {
        let mut t = [0u8; 55];
        t[54] = seed;
        let (mut mj, mut mk, mut x) = (seed, 1u8, 0usize);
        for _ in 0..55 {
            x = (x + 21) % 55;
            let d = mj.wrapping_sub(mk);
            mj = mk;
            t[x] = mk;
            mk = d;
        }
        let mut r = Lufia2Random { t, i: 54 };
        for _ in 0..3 {
            r.refill();
        }
        r
    }
    fn refill(&mut self) {
        for x in 0..24 {
            self.t[x] ^= self.t[x + 31];
        }
        for x in 24..55 {
            self.t[x] ^= self.t[x - 24];
        }
    }
    fn next(&mut self) -> u8 {
        self.i += 1;
        if self.i >= 55 {
            self.refill();
            self.i = 0;
        }
        ((0xFF * self.t[self.i] as u32) >> 8) as u8
    }
}

/// Lufia II (USA; the game's save and load routines read from the ROM, Cellenseres/Lufia2Decomp): four slots of 0x800
/// bytes; a slot whose first byte has its high four bits set is free, the low two bits of slot 0's first byte name the
/// slot saved last. From 4 on a slot is scrambled with the game's random numbers, seeded by its second byte; its
/// 16-bit words from 4 added to 0x6502 make the word at 2. Unscrambled: the hero's name (5 bytes, back to front) at
/// 0x3C8, the party's leader at 0x3D1 (0 the hero), the clock at 0x3F6 (seconds, minutes, hours), the characters'
/// levels at 0x5F1 + 0x3D n (doubled). Checked against saves the game wrote.
fn lufia_2(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let slot = |i: usize| -> Option<Vec<u8>> {
        let f = &s[i * 0x800..(i + 1) * 0x800];
        let sum = (2..0x400).fold(0x6502u16, |a, w| a.wrapping_add(u16le(f, 2 * w)));
        if f[0] & 0xF0 != 0 || sum != u16le(f, 2) {
            return None;
        }
        let mut random = Lufia2Random::new(f[1]);
        let mut b = f.to_vec();
        for x in b.iter_mut().skip(4) {
            *x = x.wrapping_sub(random.next());
        }
        Some(b)
    };
    let valid: Vec<(usize, u32)> = (0..4).filter_map(|i| slot(i).map(|f| (i, print(&f)))).collect();
    let mut memory = pick(&valid, 4, before);
    let last = (s[0] & 3) as usize;
    if memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(b) = memory.current.and_then(slot) else { return (None, memory) };
    let (hours, minutes) = (b[0x3F8] as u16, b[0x3F7]);
    if minutes > 59 || b[0x3F6] > 59 {
        return (None, memory);
    }
    // Only the hero's name is saved; another leader keeps the line to the clock.
    let level = b[0x5F1] >> 1;
    let name: Vec<u8> = b[0x3C8..0x3CD].iter().rev().copied().collect();
    let lead = (b[0x3D1] == 0 && (1..=99).contains(&level)).then(|| golden_sun_name(&name)).flatten().map(|n| (n, level));
    (Some(Progress::Hero { lead, hours, minutes }), memory)
}

/// Pokémon Mystery Dungeon: Red Rescue Team (USA; pret/pmd-red `save.c`): 128 KiB of flash, the game's save twice (at 0
/// and 0x6000, 0x57D4 bytes): the sum of its 32-bit words from 4 at 0, the magic 0x05071412 at 0x414; the second copy
/// only when the first one's sum fails, as the game does. From 0x448 the recruited Pokémon as a stream of bits (each
/// value's lowest bit first): 413 records of 323 bits (level 7 bits first, the name 10 bytes from bit 243), and at bit
/// 134791 the leader's record number (16 bits). The clock in the adventure's bits at 0x4EE0: seconds from bit 20,
/// minutes from 26, hours the 14 bits of the word at 0x4EE4, as the game's Continue screen shows it. Checked against a
/// save the game wrote.
fn mystery_dungeon_red(s: &[u8]) -> Option<Progress> {
    const SIZE: usize = 0x57D4;
    let copy = |at: usize| -> Option<&[u8]> {
        let c = s.get(at..at + SIZE)?;
        let sum = (1..SIZE / 4).fold(0u32, |a, w| a.wrapping_add(u32le(c, 4 * w)));
        (sum == u32le(c, 0)).then_some(c)
    };
    let c = copy(0).or_else(|| copy(0x6000))?;
    if u32le(c, 0x414) != 0x0507_1412 {
        return None;
    }
    let stream = &c[0x448..];
    let bits = |at: usize, n: usize| -> u32 { (0..n).fold(0, |v, j| v | (((stream[(at + j) >> 3] >> ((at + j) & 7)) & 1) as u32) << j) };
    let w = u32le(c, 0x4EE0);
    let (hours, minutes, seconds) = (u16le(c, 0x4EE4) & 0x3FFF, (w >> 26 & 63) as u8, w >> 20 & 63);
    if hours > 9999 || minutes > 59 || seconds > 59 {
        return None;
    }
    let leader = bits(134_791, 16) as usize;
    let lead = (leader < 413).then(|| {
        let level = bits(323 * leader, 7) as u8;
        let name: Vec<u8> = (0..10).map(|k| bits(323 * leader + 243 + 8 * k, 8) as u8).collect();
        golden_sun_name(&name).filter(|_| (1..=100).contains(&level)).map(|n| (n, level))
    });
    Some(Progress::Hero { lead: lead.flatten(), hours, minutes })
}

/// Final Fantasy VI Advance (USA; Janne Hellas' FF6 save importer and checksum updater, FF3h for the slot the SNES game
/// shares): 32 KiB, from 0 three slots of 0xA00 bytes, each with its byte sum (16 bits) at 0x9FE, an unused slot all
/// zeros; "FINAL FANTASY VI     ADVANCE" at 0x1F00 and the slot saved last (1-3) at 0x1FF0. In a slot the 16 characters
/// of 0x25 bytes (the name at +2 in the game's letters, the level at +8), their party bits at 0x250 (bits 0-1 the party,
/// 3-4 the place in it, 0 the leader), the active party at 0x6F5, the clock at 0x263 (hours, minutes, seconds). Checked
/// against saves the game wrote and a made one its Load Game screen shows as written.
fn final_fantasy_6_advance(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0xA00;
    if s.len() < 0x2000 || &s[0x1F00..0x1F1C] != b"FINAL FANTASY VI     ADVANCE" {
        return (None, Memory::default());
    }
    let file = |i: usize| -> Option<&[u8]> {
        let f = &s[i * SIZE..(i + 1) * SIZE];
        let sum = f[..0x9FE].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        (sum == u16le(f, 0x9FE) && f.iter().any(|b| *b != 0)).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 3, before);
    let chosen = s[0x1FF0] as usize;
    if (1..=3).contains(&chosen) && memory.prints[chosen - 1] != 0 {
        memory.current = Some(chosen - 1);
    }
    let Some(f) = memory.current.and_then(file) else { return (None, memory) };
    let (hours, minutes) = (f[0x263] as u16, f[0x264]);
    if minutes > 59 || f[0x265] > 59 {
        return (None, memory);
    }
    let party = f[0x6F5];
    let lead = (0..16).find(|&c| (1..=3).contains(&party) && f[0x250 + c] & 3 == party && f[0x250 + c] >> 3 & 3 == 0).and_then(|c| {
        let ch = &f[c * 0x25..(c + 1) * 0x25];
        let level = ch[8];
        let name = ff6_advance_name(&ch[2..8])?;
        (1..=99).contains(&level).then_some((name, level))
    });
    (Some(Progress::Hero { lead, hours, minutes }), memory)
}

/// A name in Final Fantasy VI Advance's letters: A-Z from 0x20, a-z from 0x3A, digits from 0x54, 0 the end. A character
/// not named yet (the game shows "??????") has no name.
fn ff6_advance_name(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    for &b in bytes {
        let c = match b {
            0 => break,
            0x20..=0x39 => (b'A' + (b - 0x20)) as char,
            0x3A..=0x53 => (b'a' + (b - 0x3A)) as char,
            0x54..=0x5D => (b'0' + (b - 0x54)) as char,
            _ => return None,
        };
        out.push(c);
    }
    (!out.is_empty()).then(|| shown_name(&out))
}

/// Tactics Ogre: The Knight of Lodis (USA; jiangzhengwenjz/totkol `sub_08002694`, `sub_08002A24`, `sub_080030E0`): 64 KiB
/// of flash in 16 sectors; a save starts a sector with "TOGQST02", its count at 8, its CRC-32 at 0xC, its file at 0x11
/// (0-2; 3 is a suspended battle) and the sector of its second part (0x420 bytes) at 0x12. The CRC is the standard one
/// over both parts with 0xC-0xF, 0x12 and 0x13 read as zeros. Of a file's saves the highest count is the one the game
/// loads, of the three files the highest count was saved last. The file screen's summary from 0x14: the hero's name
/// (8 letters: 0-25 A-Z, 26-51 a-z, 52-61 digits, 0xFF the end), the level at 0x24, the clock at 0x2A (hours, minutes).
/// Checked against a save the game wrote and a made one its file screen shows as written.
fn tactics_ogre(s: &[u8]) -> Option<Progress> {
    if s.len() < 0x10000 {
        return None;
    }
    let good = |k: usize| -> Option<(u32, &[u8])> {
        let h = &s[k * 0x1000..(k + 1) * 0x1000];
        let second = h[0x12] as usize;
        if &h[..8] != b"TOGQST02" || h[0x11] > 2 || second > 15 || second == k {
            return None;
        }
        let mut crc = crc32fast::Hasher::new();
        crc.update(&h[..0xC]);
        crc.update(&[0; 4]);
        crc.update(&h[0x10..0x12]);
        crc.update(&[0, 0]);
        crc.update(&h[0x14..]);
        crc.update(&s[second * 0x1000..second * 0x1000 + 0x420]);
        (crc.finalize() == u32le(h, 0xC)).then_some((u32le(h, 8), h))
    };
    let (_, h) = (0..16).filter_map(good).max_by_key(|(count, _)| *count)?;
    let (level, hours, minutes) = (h[0x24], h[0x2A] as u16, h[0x2B]);
    if minutes > 59 || hours > 99 {
        return None;
    }
    let mut name = String::new();
    for &b in &h[0x14..0x1C] {
        name.push(match b {
            0xFF => break,
            0..=25 => (b'A' + b) as char,
            26..=51 => (b'a' + b - 26) as char,
            52..=61 => (b'0' + b - 52) as char,
            0x89 => ' ',
            _ => return Some(Progress::Hero { lead: None, hours, minutes }),
        });
    }
    let name = name.trim().to_string();
    let lead = (!name.is_empty() && (1..=50).contains(&level)).then_some((name, level));
    Some(Progress::Hero { lead, hours, minutes })
}

/// Breath of Fire II (USA; the game's routines at `$C0:3847`, read from the ROM): three slots of 0xAA0 bytes (copies of
/// $7E:5000), each starting with the sum of its 16-bit words from 2 and "SAN!BOF2"; a slot without the mark is free. From
/// 0x1E0 the characters, 0x40 bytes each: the name (4 ASCII letters, 0 or 0xFF the end), the character at +6 (0 Ryu, 9 the young Ryu of
/// the prologue), the level at +7. The file select shows the first Ryu's name and level, and so does the line; the
/// game's clock isn't shown, as no save made by the game has checked it yet. Checked against a save the game wrote.
fn breath_of_fire_2(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    const SIZE: usize = 0xAA0;
    if s.len() < 3 * SIZE {
        return (None, Memory::default());
    }
    let slot = |i: usize| -> Option<&[u8]> {
        let f = &s[i * SIZE..(i + 1) * SIZE];
        let sum = (1..SIZE / 2).fold(0u16, |a, w| a.wrapping_add(u16le(f, 2 * w)));
        (&f[2..10] == b"SAN!BOF2" && sum == u16le(f, 0)).then_some(f)
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| slot(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(f) = memory.current.and_then(slot) else { return (None, memory) };
    let hero = (0..9).map(|k| &f[0x1E0 + 0x40 * k..0x1E0 + 0x40 * (k + 1)]).find(|c| c[6] == 0 || c[6] == 9);
    let lead = hero.and_then(|c| {
        let end = c[..4].iter().position(|b| *b == 0 || *b == 0xFF).unwrap_or(4);
        let name = &c[..end];
        let plain = !name.is_empty() && name.iter().all(|b| b.is_ascii_alphanumeric() || *b == b' ');
        (plain && (1..=99).contains(&c[7])).then(|| (String::from_utf8_lossy(name).trim().to_string(), c[7]))
    });
    (lead.map(|lead| Progress::Party { lead, friends: 0 }), memory)
}

/// Mario & Luigi: Superstar Saga (USA; jellees/mlss `common.h`, `sub_81233B4`): 8 KiB of EEPROM, its 8-byte blocks kept
/// back to front by mGBA and others (both orders are read): "MALRPG", the files in use as bits at 6, three files of
/// 0x6F8 bytes from 0x10, each its number at 0, the 8-bit sum of the 0x6F0 bytes from 4 at 1 and "AD" at 2. From 4 on:
/// which brother leads (bit 3: Luigi), Mario's level at 0x4C and Luigi's at 0x88 (where the LOAD screen reads them), the
/// play time in frames at 0x6EC. Checked against saves the game wrote and one with changed levels its LOAD screen
/// shows as written.
fn superstar_saga(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x2000 {
        return (None, Memory::default());
    }
    let turned: Vec<u8> = s[..0x2000].chunks(8).flat_map(|b| b.iter().rev().copied()).collect();
    let e: &[u8] = if &turned[..6] == b"MALRPG" {
        &turned
    } else if &s[..6] == b"MALRPG" {
        &s[..0x2000]
    } else {
        return (None, Memory::default());
    };
    let file = |i: usize| -> Option<&[u8]> {
        let f = &e[0x10 + 0x6F8 * i..0x10 + 0x6F8 * (i + 1)];
        let sum = f[4..0x6F4].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        (e[6] >> i & 1 == 1 && f[0] as usize == i && f[1] == sum && &f[2..4] == b"AD").then_some(&f[4..0x6F4])
    };
    let valid: Vec<(usize, u32)> = (0..3).filter_map(|i| file(i).map(|f| (i, print(f)))).collect();
    let memory = pick(&valid, 3, before);
    let Some(d) = memory.current.and_then(file) else { return (None, memory) };
    let frames = u32le(d, 0x6EC);
    let (hours, minutes) = ((frames / 216_000) as u16, (frames / 3600 % 60) as u8);
    let luigi = d[0] & 8 != 0;
    let level = if luigi { d[0x88] } else { d[0x4C] };
    if hours > 99 {
        return (None, memory);
    }
    let lead = (1..=99).contains(&level).then(|| ((if luigi { "Luigi" } else { "Mario" }).to_string(), level));
    (Some(Progress::Hero { lead, hours, minutes }), memory)
}

/// Mother 3 (the Japanese game's save code, kept byte for byte by the English fan translation; Data Crystal, the
/// Kurausukun/mother3 decomp): 64 KiB of flash in banks of 0x1000 bytes, each valid when it starts 2, 0x334D and its
/// 16-bit words add up to 0xFFFF. Bank 0 (or its mirror, bank 1) is the game's own: the file saved last at 4 (0 or 1),
/// per file a summary of 0x64 bytes from 8 + 0x64 n, which the Load screen shows (the play time in sixtieths of a
/// second at +0, the leader's name at +0x36 as 16-bit letters, the translation's ASCII less 0x20 with 0xFFFF the end,
/// the level at +0x46), and after it the file's mark (bit 0 at 0x68 + 0x64 n). The file itself is banks 6n + 4 to
/// 6n + 6 with a copy in the next three; one of them must be whole, or the game calls the file damaged. Checked against
/// saves the game wrote and one with a changed summary its Load screen shows as written.
fn mother_3(s: &[u8], before: Option<&Memory>) -> (Option<Progress>, Memory) {
    if s.len() < 0x10000 {
        return (None, Memory::default());
    }
    let bank = |n: usize| -> Option<&[u8]> {
        let b = &s[n * 0x1000..(n + 1) * 0x1000];
        let sum = (0..0x800).fold(0u16, |a, w| a.wrapping_add(u16le(b, 2 * w)));
        (u16le(b, 0) == 2 && u16le(b, 2) == 0x334D && sum == 0xFFFF).then_some(b)
    };
    let Some(system) = bank(0).or_else(|| bank(1)) else { return (None, Memory::default()) };
    let summary = |i: usize| -> Option<&[u8]> {
        let whole = [0, 3].iter().any(|copy| (0..3).all(|k| bank(6 * i + 4 + copy + k).is_some()));
        (system[0x68 + 0x64 * i] & 1 == 1 && whole).then(|| &system[8 + 0x64 * i..0x68 + 0x64 * i])
    };
    let valid: Vec<(usize, u32)> = (0..2).filter_map(|i| summary(i).map(|f| (i, print(f)))).collect();
    let mut memory = pick(&valid, 2, before);
    let last = u32le(system, 4) as usize;
    if last < 2 && memory.prints[last] != 0 {
        memory.current = Some(last);
    }
    let Some(f) = memory.current.and_then(summary) else { return (None, memory) };
    let time = u32le(f, 0);
    let (hours, minutes) = (time / 216_000, (time / 3600 % 60) as u8);
    if hours > 999 {
        return (None, memory);
    }
    let level = f[0x46];
    let mut name = String::new();
    for k in 0..8 {
        match u16le(f, 0x36 + 2 * k) {
            0xFFFF => break,
            l @ (0x00 | 0x10..=0x19 | 0x21..=0x3A | 0x41..=0x5A) => name.push((l as u8 + 0x20) as char),
            _ => {
                name.clear();
                break;
            }
        }
    }
    let name = name.trim().to_string();
    let lead = (!name.is_empty() && (1..=99).contains(&level)).then_some((name, level));
    (Some(Progress::Hero { lead, hours: hours as u16, minutes }), memory)
}

#[cfg(test)]
pub(crate) mod tests {
    //! Saves made here byte by byte, as each game writes them (no one's real save is in the repository).
    use super::*;

    fn release_of(kind: Kind, lang: Lang) -> &'static Release {
        RELEASES.iter().find(|r| r.kind == kind && r.lang == lang).unwrap()
    }

    /// Letters in the Game Boy table: capitals, small letters and the terminator.
    fn gb_text(s: &str, len: usize) -> Vec<u8> {
        let mut out: Vec<u8> = s
            .chars()
            .map(|c| match c {
                'A'..='Z' => 0x80 + (c as u8 - b'A'),
                'a'..='z' => 0xA0 + (c as u8 - b'a'),
                '0'..='9' => 0xF6 + (c as u8 - b'0'),
                'Ä' => 0xC0,
                'Ü' => 0xC2,
                '.' => 0xE8,
                '\'' => 0xE0,
                '-' => 0xE3,
                ' ' => 0x7F,
                _ => panic!("{c}"),
            })
            .collect();
        out.push(0x50);
        out.resize(len, 0x50);
        out
    }

    fn gba_text(s: &str, len: usize) -> Vec<u8> {
        let mut out: Vec<u8> = s
            .chars()
            .map(|c| match c {
                'A'..='Z' => 0xBB + (c as u8 - b'A'),
                'a'..='z' => 0xD5 + (c as u8 - b'a'),
                'Ä' => 0xF1,
                'Ü' => 0xF3,
                '.' => 0xAD,
                '-' => 0xAE,
                _ => panic!("{c}"),
            })
            .collect();
        out.push(0xFF);
        out.resize(len, 0xFF);
        out
    }

    /// A Red/Blue/Yellow save: `team` as (name, level), the badges' bits, the clock.
    pub(crate) fn gen1_save(team: &[(&str, u8)], badges: u8, (h, m): (u8, u8)) -> Vec<u8> {
        let mut s = vec![0u8; 0x8000];
        s[0x2598..0x2598 + 11].copy_from_slice(&gb_text("ASH", 11));
        s[0x2602] = badges;
        s[0x2CED] = h;
        s[0x2CEF] = m;
        s[0x2CF0] = 30;
        let p = 0x2F2C;
        s[p] = team.len() as u8;
        for (i, (name, level)) in team.iter().enumerate() {
            s[p + 1 + i] = 0xB4; // Charizard's index; the line doesn't need it
            s[p + 8 + i * 44 + 0x21] = *level;
            s[p + 8 + i * 44 + 0x03] = *level;
            s[p + 0x152 + i * 11..p + 0x152 + (i + 1) * 11].copy_from_slice(&gb_text(name, 11));
        }
        s[p + 1 + team.len()] = 0xFF;
        let sum = s[0x2598..0x3523].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        s[0x3523] = !sum;
        s
    }

    /// A Gold/Silver or Crystal save; `team` as (name, level, egg).
    pub(crate) fn gen2_save(crystal: bool, team: &[(&str, u8, bool)], (johto, kanto): (u8, u8), (h, m): (u16, u8)) -> Vec<u8> {
        let (time, badges, party, end, sum_at) = if crystal { (0x2052, 0x23E5, 0x2865, 0x2B82, 0x2D0D) } else { (0x2053, 0x23E4, 0x288A, 0x2D68, 0x2D69) };
        let mut s = vec![0u8; 0x8000];
        s[0x200B..0x200B + 11].copy_from_slice(&gb_text("GOLD", 11));
        s[time..time + 2].copy_from_slice(&h.to_be_bytes());
        s[time + 2] = m;
        s[badges] = johto;
        s[badges + 1] = kanto;
        s[party] = team.len() as u8;
        for (i, (name, level, egg)) in team.iter().enumerate() {
            s[party + 1 + i] = if *egg { 0xFD } else { 155 + i as u8 };
            s[party + 8 + i * 48 + 0x1F] = *level;
            let at = party + 8 + 6 * 48 + 6 * 11 + i * 11;
            s[at..at + 11].copy_from_slice(&gb_text(name, 11));
        }
        s[party + 1 + team.len()] = 0xFF;
        let sum = s[0x2009..=end].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        s[sum_at..sum_at + 2].copy_from_slice(&sum.to_le_bytes());
        s
    }

    /// A GBA Pokémon flash of 128 KiB: both copies written, the second (counter + 1) the newer unless `older_second`;
    /// `team` as (name, level, egg).
    pub(crate) fn gen3_save(kind: Kind, team: &[(&str, u8, bool)], badges: u8, (h, m): (u16, u8), counter: u32) -> Vec<u8> {
        let (count_at, flags, first_badge) = match kind {
            Emerald => (0x234, 0x1270, 0x867),
            RubySapphire => (0x234, 0x1220, 0x807),
            _ => (0x034, 0x0EE0, 0x820),
        };
        let mut small = vec![0u8; SECTOR_DATA];
        small[..8].copy_from_slice(&gba_text("MAY", 8));
        small[0x0E..0x10].copy_from_slice(&h.to_le_bytes());
        small[0x10] = m;
        let mut large = vec![0u8; 4 * SECTOR_DATA];
        large[count_at..count_at + 4].copy_from_slice(&(team.len() as u32).to_le_bytes());
        for (i, (name, level, egg)) in team.iter().enumerate() {
            let at = count_at + 4 + i * 100;
            let (pid, otid) = (0x1234_5678u32.wrapping_mul(i as u32 + 1), 0x0BAD_CAFEu32);
            large[at..at + 4].copy_from_slice(&pid.to_le_bytes());
            large[at + 4..at + 8].copy_from_slice(&otid.to_le_bytes());
            // The species in the growth part, the data scrambled as the game keeps it, with its checksum.
            let species: u16 = if *egg { 412 } else { 280 + i as u16 };
            let mut data = [0u8; 48];
            let g = [b"GAEM", b"GAME", b"GEAM", b"GEMA", b"GMAE", b"GMEA", b"AGEM", b"AGME", b"AEGM", b"AEMG", b"AMGE", b"AMEG", b"EGAM", b"EGMA", b"EAGM", b"EAMG", b"EMGA", b"EMAG", b"MGAE", b"MGEA", b"MAGE", b"MAEG", b"MEGA", b"MEAG"][(pid % 24) as usize]
                .iter()
                .position(|c| *c == b'G')
                .unwrap();
            data[g * 12..g * 12 + 2].copy_from_slice(&species.to_le_bytes());
            let sum = (0..24).fold(0u16, |a, k| a.wrapping_add(u16le(&data, k * 2)));
            large[at + 0x1C..at + 0x1E].copy_from_slice(&sum.to_le_bytes());
            for w in 0..12 {
                let v = u32le(&data, w * 4) ^ pid ^ otid;
                large[at + 0x20 + w * 4..at + 0x24 + w * 4].copy_from_slice(&v.to_le_bytes());
            }
            large[at + 0x08..at + 0x12].copy_from_slice(&gba_text(name, 10));
            large[at + 0x12] = 2;
            large[at + 0x13] = if *egg { 2 | 4 } else { 2 };
            large[at + 0x54] = *level;
        }
        for b in 0..badges as usize {
            let flag = first_badge + b;
            large[flags + flag / 8] |= 1 << (flag % 8);
        }
        let mut s = vec![0xFFu8; 0x20000];
        for (slot, c) in [(0usize, counter), (1, counter + 1)] {
            for k in 0..SECTORS {
                // The game rotates the sectors in a copy; the reader must follow their numbers.
                let id = (k + slot * 3) % SECTORS;
                let sector = &mut s[(slot * SECTORS + k) * SECTOR..(slot * SECTORS + k + 1) * SECTOR];
                sector.fill(0);
                match id {
                    0 => sector[..SECTOR_DATA].copy_from_slice(&small),
                    1..=4 => sector[..SECTOR_DATA].copy_from_slice(&large[(id - 1) * SECTOR_DATA..id * SECTOR_DATA]),
                    _ => {}
                }
                let sum = (0..SECTOR_DATA / 4).fold(0u32, |a, w| a.wrapping_add(u32le(sector, w * 4)));
                let sum = (sum as u16).wrapping_add((sum >> 16) as u16);
                sector[0xFF4..0xFF6].copy_from_slice(&(id as u16).to_le_bytes());
                sector[0xFF6..0xFF8].copy_from_slice(&sum.to_le_bytes());
                sector[0xFF8..0xFFC].copy_from_slice(&0x0801_2025u32.to_le_bytes());
                sector[0xFFC..0x1000].copy_from_slice(&c.to_le_bytes());
            }
        }
        s
    }

    /// Super Metroid's file `slot` among `files` (the others left empty), chosen last on the file select.
    pub(crate) fn metroid_save(slot: usize, (energy, reserve, missiles, supers, bombs): (u16, u16, u16, u16, u16), items: u16, beams: u16, (h, m): (u16, u16)) -> Vec<u8> {
        let mut s = vec![0u8; 0x2000];
        let at = [0x10, 0x66C, 0xCC8][slot];
        let mut put = |ram: usize, v: u16| s[at + ram - 0x09A2..at + ram - 0x09A2 + 2].copy_from_slice(&v.to_le_bytes());
        put(0x09A4, items);
        put(0x09A8, beams);
        put(0x09C4, energy);
        put(0x09C2, energy);
        put(0x09D4, reserve);
        put(0x09C8, missiles);
        put(0x09CC, supers);
        put(0x09D0, bombs);
        put(0x09DE, m);
        put(0x09E0, h);
        let sum = (0..0x65C / 2).fold(0u16, |a, w| a.wrapping_add(u16le(&s[at..], w * 2)));
        for (place, v) in [(2 * slot, sum), (8 + 2 * slot, !sum), (0x1FF0 + 2 * slot, sum), (0x1FF8 + 2 * slot, !sum)] {
            s[place..place + 2].copy_from_slice(&v.to_le_bytes());
        }
        s[0x1FEC..0x1FEE].copy_from_slice(&(slot as u16).to_le_bytes());
        s[0x1FEE..0x1FF0].copy_from_slice(&(!(slot as u16)).to_le_bytes());
        s
    }

    /// Metroid: Zero Mission's 32 KiB with `files` (slot, (energy, missiles, supers, bombs), beams, suit, difficulty,
    /// (h, m)); the record of the file saved last says `last`.
    pub(crate) fn zero_mission_save(files: &[(usize, (u16, u16, u8, u8), u8, u8, u8, (u8, u8))], last: Option<u8>) -> Vec<u8> {
        let mut s = vec![0xFFu8; 0x8000];
        for &(slot, (energy, missiles, supers, bombs), beams, suit, difficulty, (h, m)) in files {
            for at in [128 + slot * 4640, 14048 + slot * 4640] {
                let f = &mut s[at..at + 4640];
                f.fill(0);
                f[..16].copy_from_slice(b"ZERO_MISSION_010");
                f[60] = difficulty;
                f[79..95].copy_from_slice(b"Planet Zebes... ");
                f[412..414].copy_from_slice(&energy.to_le_bytes());
                f[414..416].copy_from_slice(&missiles.to_le_bytes());
                f[416] = supers;
                f[417] = bombs;
                f[424] = beams;
                f[426] = suit;
                f[508] = h;
                f[509] = m;
                f[592..608].copy_from_slice(b" - Samus Aran - ");
                f[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
                let sum = (0..4640 / 4).fold(0u32, |a, w| a.wrapping_add(u32le(f, w * 4)));
                f[16..20].copy_from_slice(&sum.to_le_bytes());
                f[20..24].copy_from_slice(&(!sum).to_le_bytes());
            }
        }
        if let Some(l) = last {
            let r = &mut s[28032..28032 + 64];
            r.fill(0);
            r[0] = 0x1E;
            r[22] = l;
            r[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
            let sum = (0..32).fold(0u16, |a, w| a.wrapping_add(u16le(r, w * 2)));
            r[2..4].copy_from_slice(&sum.to_le_bytes());
            r[4..6].copy_from_slice(&(!sum).to_le_bytes());
        }
        s
    }

    #[test]
    fn zero_mission_counts_items_as_its_ending_does() {
        let r = release_of(ZeroMission, En);
        // Normal: 3 energy tanks, 15 missiles (3 tanks), 4 supers (2), 2 bombs (1); varia + hi-jump (2 suit bits);
        // long beam + charge (2 beam bits) and the bombs' bit: 3 + 3 + 2 + 1 + 2 + 2 + 1 = 14.
        let save = zero_mission_save(&[(1, (399, 15, 4, 2), 0b1000_0011, 0b0000_0101, 1, (2, 7))], Some(1));
        assert_eq!(line(&read(r, &save, None).0), "14% items · game clock 2:07");
        // Hard: an energy tank is 50 there.
        let hard = zero_mission_save(&[(0, (199, 4, 2, 1), 0, 0, 2, (0, 30))], Some(0));
        assert_eq!(line(&read(r, &hard, None).0), "7% items · game clock 0:30");
        // Two files: the record of the file saved last decides, not a guess.
        let two = zero_mission_save(&[(0, (99, 0, 0, 0), 0, 0, 1, (0, 5)), (2, (1299, 250, 30, 18), 0x9F, 0xFF, 1, (3, 0))], Some(2));
        assert_eq!(line(&read(r, &two, None).0), "100% items · game clock 3:00");
        let mut broken = two.clone();
        broken[28032 + 22] = 0;
        assert_eq!(read(r, &broken, None).0, None, "a record that fails its sum, and two files: nothing");
    }

    /// Super Mario 64's EEPROM with `files` (file, castle star bits, stars per course as given), both copies written.
    pub(crate) fn mario_64_save(files: &[(usize, u8, &[u8])]) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        for &(i, castle, courses) in files {
            for c in [2 * i, 2 * i + 1] {
                let f = &mut s[c * 56..(c + 1) * 56];
                f[8..12].copy_from_slice(&((castle as u32) << 24 | 1).to_be_bytes());
                f[12..12 + courses.len()].copy_from_slice(courses);
                f[52..54].copy_from_slice(b"DA");
                let sum = f[..54].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
                f[54..56].copy_from_slice(&sum.to_be_bytes());
            }
        }
        s
    }

    #[test]
    fn mario_64_counts_stars_as_its_file_select_does() {
        let r = release_of(Mario64, En);
        // Bob-omb Battlefield all 7, Whomp's Fortress 3 and its cannon bit (not a star), 2 castle stars.
        let save = mario_64_save(&[(0, 0b11, &[0x7F, 0x80 | 0b111])]);
        assert_eq!(line(&read(r, &save, None).0), "12 of 120 stars");
        // A damaged first copy: the second is used, as the game does.
        let mut one = save.clone();
        one[3] ^= 1;
        assert_eq!(line(&read(r, &one, None).0), "12 of 120 stars");
        // Both copies damaged: nothing.
        one[56 + 3] ^= 1;
        assert_eq!(read(r, &one, None).0, None);
        // The mupen64plus .srm starts with the EEPROM: the same.
        let mut srm = save.clone();
        srm.resize(0x48800, 0xFF);
        assert_eq!(line(&read(r, &srm, None).0), "12 of 120 stars");
    }

    /// Ocarina of Time's SRAM with `files` (slot, quest item bits, heart containers), each written to its slot and backup.
    pub(crate) fn ocarina_save(files: &[(usize, u32, u8)]) -> Vec<u8> {
        let mut s = vec![0u8; 0x8000];
        for &(i, quest, hearts) in files {
            for at in [0x20 + i * 0x1450, 0x20 + (i + 3) * 0x1450] {
                let f = &mut s[at..at + 0x1354];
                f[0x1C..0x22].copy_from_slice(b"ZELDAZ");
                f[0x2E..0x30].copy_from_slice(&((hearts as i16) * 16).to_be_bytes());
                f[0xA4..0xA8].copy_from_slice(&quest.to_be_bytes());
                f[0x24 + i] = b'L';
                let sum = (0..0x1354 / 2).filter(|w| w * 2 != 0x1352).fold(0u16, |a, w| a.wrapping_add(u16::from_be_bytes([f[w * 2], f[w * 2 + 1]])));
                f[0x1352..0x1354].copy_from_slice(&sum.to_be_bytes());
            }
        }
        s
    }

    #[test]
    fn ocarina_counts_stones_then_medallions() {
        let r = release_of(Ocarina, En);
        // Kokiri's Emerald and Goron's Ruby, 5 hearts.
        let child = ocarina_save(&[(1, 0b11 << 18, 5)]);
        assert_eq!(line(&read(r, &child, None).0), "2 of 3 spiritual stones · 5 hearts");
        // Adult: all stones, forest and fire medallions, 11 hearts; inside mupen64plus's .srm.
        let adult = ocarina_save(&[(0, 0b111 << 18 | 0b11, 11)]);
        let mut srm = vec![0xFFu8; 0x48800];
        srm[0x20800..0x28800].copy_from_slice(&adult);
        assert_eq!(line(&read(r, &srm, None).0), "2 of 6 medallions · 11 hearts");
        // An emulator that keeps the SRAM's words the other way round.
        let flipped: Vec<u8> = adult.chunks(4).flat_map(|w| w.iter().rev().copied()).collect();
        assert_eq!(line(&read(r, &flipped, None).0), "2 of 6 medallions · 11 hearts");
        // A damaged slot falls back to its backup; both damaged: nothing.
        let mut broken = adult.clone();
        broken[0x20 + 0x40] ^= 1;
        assert_eq!(line(&read(r, &broken, None).0), "2 of 6 medallions · 11 hearts");
        broken[0x20 + 3 * 0x1450 + 0x40] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
        // Alex's untouched SRAM (all 0xFF): nothing.
        assert_eq!(read(r, &vec![0xFFu8; 0x48800], None).0, None);
    }

    /// Paper Mario's flash: for each (physical save, file, count, frames, spirits, level) a save; the globals choose
    /// `last`.
    pub(crate) fn paper_mario_save(saves: &[(usize, u32, u32, u32, u8, u8)], last: Option<u32>) -> Vec<u8> {
        fn sign(b: &mut [u8]) {
            b[..16].copy_from_slice(b"Mario Story 006\0");
            b[0x30..0x34].copy_from_slice(&0u32.to_be_bytes());
            b[0x34..0x38].copy_from_slice(&u32::MAX.to_be_bytes());
            let sum = (0..b.len() / 4).fold(0u32, |a, w| a.wrapping_add(u32::from_be_bytes([b[w * 4], b[w * 4 + 1], b[w * 4 + 2], b[w * 4 + 3]])));
            b[0x30..0x34].copy_from_slice(&sum.to_be_bytes());
            b[0x34..0x38].copy_from_slice(&(!sum).to_be_bytes());
        }
        let mut s = vec![0xFFu8; 0x20000];
        for &(p, file, count, frames, spirits, level) in saves {
            let f = &mut s[p * 0x4000..p * 0x4000 + 0x1380];
            f.fill(0);
            f[0x38..0x3C].copy_from_slice(&file.to_be_bytes());
            f[0x3C..0x40].copy_from_slice(&count.to_be_bytes());
            f[0x12EC..0x12F0].copy_from_slice(&frames.to_be_bytes());
            f[0x12F0] = spirits;
            f[0x12F2] = level;
            sign(f);
        }
        if let Some(l) = last {
            for p in [6, 7] {
                let g = &mut s[p * 0x4000..p * 0x4000 + 0x80];
                g.fill(0);
                g[0x3C..0x40].copy_from_slice(&l.to_be_bytes());
                sign(g);
            }
        }
        s
    }

    #[test]
    fn paper_mario_reads_what_its_file_select_shows() {
        let r = release_of(PaperMario, En);
        // File 1 saved twice (the newer counts), file 0 once; the globals chose file 1. 12 h 03 min at 60 a second.
        let frames = (12 * 3600 + 3 * 60) * 60;
        let save = paper_mario_save(&[(0, 0, 5, 1000, 1, 3), (1, 1, 6, frames - 999_999, 2, 8), (2, 1, 9, frames, 3, 9)], Some(1));
        assert_eq!(line(&read(r, &save, None).0), "3 of 7 Star Spirits · Mario Lv. 9 · game clock 12:03");
        // Without good globals: the newest save of all (count 9), the same here.
        assert_eq!(line(&read(r, &paper_mario_save(&[(0, 0, 5, 1000, 1, 3), (2, 1, 9, frames, 3, 9)], None), None).0), "3 of 7 Star Spirits · Mario Lv. 9 · game clock 12:03");
        // A file never played: the game's file menu says "First Play" and nothing else (a save the game wrote, 15.11.).
        assert_eq!(read(r, &paper_mario_save(&[(0, 0, 1, 0, 0, 1)], None), None).0, None);
        // Inside mupen64plus's .srm.
        let mut srm = vec![0xFFu8; 0x48800];
        srm[0x28800..0x48800].copy_from_slice(&save);
        assert_eq!(line(&read(r, &srm, None).0), "3 of 7 Star Spirits · Mario Lv. 9 · game clock 12:03");
        let mut broken = save.clone();
        broken[2 * 0x4000 + 0x200] ^= 1;
        broken[0x4000 + 0x200] ^= 1;
        assert_eq!(read(r, &broken, None).0, None, "file 1's saves both damaged");
    }

    /// Mario Kart 64's EEPROM with these Grand Prix points (one byte per class).
    pub(crate) fn mario_kart_64_save(points: [u8; 4]) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        s[0x180..0x184].copy_from_slice(&points);
        let c1 = s[0x180..0x185].iter().enumerate().fold(0u32, |a, (i, b)| a + (*b as u32 + 1) * (i as u32 + 1) + i as u32) as u8;
        s[0x186] = c1;
        s[0x187] = c1.wrapping_add(90);
        s
    }

    #[test]
    fn mario_kart_64_counts_gold_cups() {
        let r = release_of(MarioKart64, En);
        // 50cc: all four gold; 100cc: gold, silver, bronze, none; 150cc: one gold.
        let save = mario_kart_64_save([0xFF, 0b00_01_10_11, 0b11, 0]);
        assert_eq!(line(&read(r, &save, None).0), "6 of 16 gold cups");
        let mut broken = save.clone();
        broken[0x181] ^= 0x40;
        assert_eq!(read(r, &broken, None).0, None, "points that don't match the check bytes");
    }

    /// Star Fox 64's EEPROM: planet flags as given, both copies signed.
    pub(crate) fn star_fox_64_save(planets: &[u8]) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        for at in [0, 0x100] {
            let d = &mut s[at..at + 0x100];
            d[..planets.len()].copy_from_slice(planets);
            d[0x14] = 1;
            d[0x15..0x18].copy_from_slice(&[99, 99, 99]);
            let mut c: u16 = 0;
            for &b in &d[..0xFE] {
                c ^= b as u16;
                c <<= 1;
                c = (c & 0xFE) | ((c >> 8) & 1);
            }
            d[0xFE..0x100].copy_from_slice(&((c & 0xFF) | 0x9500).to_be_bytes());
        }
        s
    }

    #[test]
    fn star_fox_64_counts_medals() {
        let r = release_of(StarFox64, En);
        // Corneria: played, cleared, medal; Meteo: played, cleared; Katina: medal and Expert medal.
        let save = star_fox_64_save(&[0x07, 0x05, 0x1F]);
        assert_eq!(line(&read(r, &save, None).0), "2 medals · 1 in Expert");
        assert_eq!(line(&read(r, &star_fox_64_save(&[0x07]), None).0), "1 medal");
        assert_eq!(read(r, &star_fox_64_save(&[0x02]), None).0, None, "a medal without its clear: the bits are not what we think");
        let mut broken = save.clone();
        broken[1] ^= 1;
        broken[0x101] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
    }

    /// Diddy Kong Racing's EEPROM: file `i` with `total` golden balloons (and a world's few), signed as the game does.
    pub(crate) fn dkr_save(i: usize, total: u8) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        let f = &mut s[i * 40..(i + 1) * 40];
        let mut put = |from: usize, n: usize, v: u32| {
            for k in 0..n {
                let b = from + k;
                if v >> (n - 1 - k) & 1 != 0 {
                    f[b / 8] |= 1 << (7 - b % 8);
                }
            }
        };
        put(16, 2, 2); // a course cleared
        put(112, 7, total as u32);
        put(119, 7, total.min(4) as u32);
        let sum = f[2..].iter().fold(5u16, |a, b| a.wrapping_add(*b as u16));
        f[..2].copy_from_slice(&sum.to_be_bytes());
        s
    }

    #[test]
    fn diddy_kong_racing_counts_golden_balloons() {
        let r = release_of(DiddyKongRacing, En);
        assert_eq!(line(&read(r, &dkr_save(2, 34), None).0), "34 of 47 golden balloons");
        let mut broken = dkr_save(0, 10);
        broken[20] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
        assert_eq!(read(r, &dkr_save(0, 60), None).0, None, "60 balloons don't exist");
    }

    /// Final Fantasy III's 8 KiB with one slot: `slot` (0-2) holds `leader` (name, level) leading the active party.
    pub(crate) fn ff6_save(slot: usize, leader: (&str, u8), (h, m): (u8, u8)) -> Vec<u8> {
        let mut s = vec![0x60u8; 0x2000];
        let f = &mut s[slot * 0xA00..(slot + 1) * 0xA00];
        f.fill(0);
        let name: Vec<u8> = leader.0.bytes().map(|c| if c.is_ascii_uppercase() { c - b'A' + 0x80 } else { c - b'a' + 0x9A }).chain(std::iter::repeat(0xFF)).take(6).collect();
        // Character 3 leads party 1 (first row); character 0 is in it too, second row, and marked 0x80 as the game
        // leaves it after a change of order (it shows the first row).
        f[3 * 0x25 + 2..3 * 0x25 + 8].copy_from_slice(&name);
        f[3 * 0x25 + 8] = leader.1;
        f[0x250] = 0x80 | 0x40 | 1 << 3 | 1;
        f[0x253] = 0x40 | 1;
        f[0x46D] = 1;
        f[0x263] = h;
        f[0x264] = m;
        let sum = f[..0x9FE].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        f[0x9FE..0xA00].copy_from_slice(&sum.to_le_bytes());
        s[0x1FF0] = slot as u8 + 1;
        s
    }

    /// EarthBound's 8 KiB: file `i` (both blocks) with `party` (character numbers) and the leader's name and level.
    pub(crate) fn earthbound_save(i: usize, party: &[u8], name: &str, level: u8) -> Vec<u8> {
        let mut s = vec![0u8; 0x2000];
        // As the game writes it from its first start: the header in all six blocks, the empty files' sums 0.
        for k in 0..6 {
            s[k * 1280..k * 1280 + 20].copy_from_slice(b"HAL Laboratory, inc.");
        }
        for k in [2 * i, 2 * i + 1] {
            let b = &mut s[k * 1280..(k + 1) * 1280];
            b[32 + 122..32 + 122 + party.len()].copy_from_slice(party);
            let ch = 505 + (party[0] as usize - 1) * 95;
            for (j, c) in name.bytes().enumerate() {
                b[ch + j] = if c.is_ascii_uppercase() { c - b'A' + 0x71 } else { c - b'a' + 0x91 };
            }
            b[ch + 5] = level;
            let sum = b[32..].iter().fold(0u16, |a, x| a.wrapping_add(*x as u16));
            let xor = (32..1280).step_by(2).fold(0u16, |a, w| a ^ u16::from_le_bytes([b[w], b[w + 1]]));
            b[28..30].copy_from_slice(&sum.to_le_bytes());
            b[30..32].copy_from_slice(&xor.to_le_bytes());
        }
        s
    }

    #[test]
    fn earthbound_names_the_leader_and_the_friends() {
        let r = release_of(EarthBound, En);
        assert_eq!(line(&read(r, &earthbound_save(1, &[1, 2, 3, 0, 0, 0], "Ness", 23), None).0), "Ness Lv. 23 · 2 friends");
        assert_eq!(line(&read(r, &earthbound_save(0, &[1, 0, 0, 0, 0, 0], "Lucas", 4), None).0), "Lucas Lv. 4");
        let mut broken = earthbound_save(2, &[1, 2, 0, 0, 0, 0], "Ness", 9);
        broken[4 * 1280 + 600] ^= 1;
        assert_eq!(line(&read(r, &broken, None).0), "Ness Lv. 9 · 1 friend", "the copy");
        broken[5 * 1280 + 600] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
    }

    #[test]
    fn final_fantasy_6_names_the_party_leader() {
        let r = release_of(FinalFantasy6, En);
        assert_eq!(line(&read(r, &ff6_save(1, ("TERRA", 32), (18, 44)), None).0), "Terra Lv. 32 · game clock 18:44");
        // A name the player gave keeps its spelling.
        assert_eq!(line(&read(r, &ff6_save(0, ("Tina", 5), (0, 50)), None).0), "Tina Lv. 5 · game clock 0:50");
        // Alex's never-saved SRAM (all 0x60): nothing.
        assert_eq!(read(r, &vec![0x60u8; 0x2000], None).0, None);
        let mut broken = ff6_save(2, ("CELES", 40), (30, 0));
        broken[2 * 0xA00 + 0x100] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
    }

    /// Link's Awakening DX's 8 KiB with `files` (file, instruments bits, hearts), as the game writes it: 1, 3, 5, 7, 9 in
    /// all three places from its first start on, a name only in the files played.
    pub(crate) fn ladx_save(files: &[(usize, u8, u8)]) -> Vec<u8> {
        let mut s = vec![0u8; 0x2000];
        for i in 0..3 {
            s[0x100 + i * 0x3AD..0x100 + i * 0x3AD + 5].copy_from_slice(&[1, 3, 5, 7, 9]);
        }
        for &(i, inst, hearts) in files {
            let f = &mut s[0x100 + i * 0x3AD..0x100 + (i + 1) * 0x3AD];
            f[5 + 0xDB4F - 0xD800..5 + 0xDB54 - 0xD800].copy_from_slice(&[0x42; 5]);
            f[5 + 0xDB5B - 0xD800] = hearts;
            for k in 0..8 {
                if inst & (1 << k) != 0 {
                    f[5 + 0xDB65 - 0xD800 + k] = 0x02;
                }
            }
        }
        s
    }

    /// The Minish Cap's 8 KiB EEPROM with `files` (file, elements bits as 2-bit values, most health), the header
    /// choosing `chosen`; `reversed`: each 8-byte block stored back to front, as some emulators keep it.
    pub(crate) fn minish_save(files: &[(usize, u8, u8)], chosen: Option<u8>, reversed: bool) -> Vec<u8> {
        let mut e = vec![0xFFu8; 0x2000];
        let sum = |b: &[u8]| (0..b.len() / 2).fold(0u32, |a, w| a.wrapping_add((u16le(b, 2 * w) ^ (b.len() - 2 * w) as u16) as u32)) as u16;
        let put = |e: &mut Vec<u8>, status: usize, data: usize, d: &[u8]| {
            for c in [0, 0x1000] {
                e[data + c..data + c + d.len()].copy_from_slice(d);
                let total = sum(b"3ZCM").wrapping_add(sum(d));
                e[status + c..status + c + 2].copy_from_slice(&total.to_le_bytes());
                e[status + c + 2..status + c + 4].copy_from_slice(&total.wrapping_neg().to_le_bytes());
                e[status + c + 4..status + c + 8].copy_from_slice(b"3ZCM");
            }
        };
        for &(i, elements, max) in files {
            let mut f = vec![0u8; 0x500];
            f[1] = 1;
            f[0x80..0x86].copy_from_slice(b"LINK\0\0");
            f[0x80 + 5] = i as u8;
            f[0xA8 + 3] = max;
            f[0xF2 + 0x10] = elements;
            put(&mut e, 0x30 + 0x10 * i, 0x80 + 0x500 * i, &f);
        }
        if let Some(c) = chosen {
            let mut h = vec![0u8; 0x10];
            h[..4].copy_from_slice(b"AGBZ");
            h[4] = c;
            put(&mut e, 0x20, 0x70, &h);
        }
        if reversed {
            e = e.chunks(8).flat_map(|b| b.iter().rev().copied()).collect();
        }
        e
    }

    /// Majora's Mask's 128 KiB flash with `files` (file, remains bits, most health, as an owl save), each written to its
    /// place and backup; `flipped`: 32-bit words the other way round.
    pub(crate) fn majora_save(files: &[(usize, u8, i16, bool)], flipped: bool) -> Vec<u8> {
        let mut s = vec![0xFFu8; 0x20000];
        for &(i, remains, max, owl) in files {
            let size = if owl { 0x3CA0 } else { 0x100C };
            let mut f = vec![0u8; size];
            f[0x24..0x2A].copy_from_slice(b"ZELDA3");
            f[0x2C] = b'L' + i as u8;
            f[0x23] = owl as u8;
            f[0x34..0x36].copy_from_slice(&max.to_be_bytes());
            f[0xBF] = remains;
            let sum = f.iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
            f[0x100A..0x100C].copy_from_slice(&sum.to_be_bytes());
            let at = if owl { 0x8000 + 0x8000 * i } else { 0x4000 * i };
            for at in [at, at + if owl { 0x4000 } else { 0x2000 }] {
                s[at..at + size].copy_from_slice(&f);
            }
        }
        if flipped {
            s = s.chunks(4).flat_map(|w| w.iter().rev().copied()).collect();
        }
        s
    }

    #[test]
    fn majoras_mask_counts_remains_and_hearts_owl_save_first() {
        let r = release_of(MajorasMask, En);
        assert_eq!(line(&read(r, &majora_save(&[(0, 0b0011, 0x90, false)], false), None).0), "2 of 4 remains · 9 hearts");
        let both = majora_save(&[(1, 0b0001, 0x50, false), (1, 0b0111, 0xC0, true)], true);
        assert_eq!(line(&read(r, &both, None).0), "3 of 4 remains · 12 hearts", "the owl save, words flipped");
        let mut srm = vec![0u8; 0x48800];
        srm[0x28800..].copy_from_slice(&majora_save(&[(0, 0b1111, 0x140, false)], false));
        assert_eq!(line(&read(r, &srm, None).0), "4 of 4 remains · 20 hearts", "inside mupen64plus's file");
        let mut bad = majora_save(&[(0, 0b0011, 0x90, false)], false);
        bad[0xBF] = 0;
        bad[0x20BF] = 0;
        assert_eq!(read(r, &bad, None).0, None, "the sum no longer adds up");
        assert_eq!(read(r, &majora_save(&[(0, 0, 0x55, false)], false), None).0, None, "health isn't whole hearts");
    }

    /// An Oracle game's 8 KiB with `files` (file, essences bits, most health), each written to both places.
    pub(crate) fn oracle_save(ages: bool, files: &[(usize, u8, u8)]) -> Vec<u8> {
        let mut s = vec![0u8; 0x2000];
        for &(i, essences, max) in files {
            let mut f = vec![0u8; 0x550];
            f[2..10].copy_from_slice(if ages { b"Z21216-0" } else { b"Z11216-0" });
            f[0x52 + i] = b'A';
            let (at_max, at_ess) = if ages { (0xFB, 0x10F) } else { (0xF3, 0x10B) };
            f[at_max] = max;
            f[at_ess] = essences;
            let sum = (1..0x550 / 2).fold(0u16, |a, w| a.wrapping_add(u16le(&f, 2 * w)));
            f[..2].copy_from_slice(&sum.to_le_bytes());
            for at in [[0x10, 0x560, 0xAB0][i], [0x1000, 0x1550, 0x1AA0][i]] {
                s[at..at + 0x550].copy_from_slice(&f);
            }
        }
        s
    }

    #[test]
    fn the_oracles_count_essences_and_hearts() {
        let ages = release_of(OracleOfAges, En);
        let seasons = release_of(OracleOfSeasons, En);
        assert_eq!(line(&read(ages, &oracle_save(true, &[(1, 0b0001_1111, 36)]), None).0), "5 of 8 essences · 9 hearts");
        assert_eq!(line(&read(seasons, &oracle_save(false, &[(0, 0b11, 16)]), None).0), "2 of 8 essences · 4 hearts");
        assert_eq!(read(ages, &oracle_save(false, &[(0, 0b11, 16)]), None).0, None, "a Seasons save isn't Ages'");
        let mut damaged = oracle_save(true, &[(2, 0b1, 12)]);
        damaged[0xAB0 + 0x20] ^= 1;
        assert_eq!(line(&read(ages, &damaged, None).0), "1 of 8 essences · 3 hearts", "the copy");
        damaged[0x1AA0 + 0x20] ^= 1;
        assert_eq!(read(ages, &damaged, None).0, None, "both copies damaged");
        assert_eq!(read(ages, &oracle_save(true, &[(0, 0, 13)]), None).0, None, "health isn't whole hearts");
    }

    #[test]
    fn minish_cap_counts_elements_and_hearts_in_the_file_chosen_last() {
        let r = release_of(MinishCap, En);
        let two = minish_save(&[(0, 0b0101, 48), (2, 0b0101_0101, 120)], Some(2), false);
        assert_eq!(line(&read(r, &two, None).0), "4 of 4 elements · 15 hearts");
        let two = minish_save(&[(0, 0b0101, 48), (2, 0b0101_0101, 120)], Some(0), true);
        assert_eq!(line(&read(r, &two, None).0), "2 of 4 elements · 6 hearts", "an emulator that keeps the blocks reversed");
        let one = minish_save(&[(1, 0, 24)], None, false);
        assert_eq!(line(&read(r, &one, None).0), "0 of 4 elements · 3 hearts");
    }

    #[test]
    fn minish_cap_says_nothing_for_a_damaged_or_odd_file() {
        let r = release_of(MinishCap, En);
        let mut bad = minish_save(&[(0, 0b01, 48)], Some(0), false);
        bad[0x80 + 0xA8 + 3] = 56;
        bad[0x1080 + 0xA8 + 3] = 56;
        assert_eq!(read(r, &bad, None).0, None, "the sum no longer adds up in either copy");
        let mut first = minish_save(&[(0, 0b01, 48)], Some(0), false);
        first[0x80 + 0xA8 + 3] = 56;
        assert_eq!(line(&read(r, &first, None).0), "1 of 4 elements · 6 hearts", "the copy when the first is damaged");
        assert_eq!(read(r, &minish_save(&[(0, 0b01, 50)], Some(0), false), None).0, None, "health isn't whole hearts");
        assert_eq!(read(r, &vec![0xFF; 0x2000], None).0, None, "a fresh EEPROM");
    }

    #[test]
    fn only_releases_checked_against_the_games_show_a_line() {
        let shown = |name: &str| RELEASES.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("{name}")).verified();
        // Checked: a save the game itself wrote, read as the game shows it (CASE-KNOWS-VERIFY.md, 15.11.).
        for name in [
            "Pokemon - Emerald Version (USA, Europe)",
            "Super Metroid (Japan, USA) (En,Ja)",
            "Super Mario All-Stars + Super Mario World (USA)",
            "Legend of Zelda, The - A Link to the Past (USA)",
            "Donkey Kong Country (USA) (Rev 1)",
            "Pokemon - Yellow Version - Special Pikachu Edition (USA, Europe) (CGB+SGB Enhanced)",
        ] {
            assert!(shown(name), "{name}");
        }
        // Not checked: no ROM, another region, or no save the game could be made to write yet.
        for name in [
            "Pokemon - Smaragd-Edition (Germany)",
            "Super Metroid (Europe) (En,Fr,De)",
            "Legend of Zelda, The - A Link to the Past (Canada) (Fr)",
        ] {
            assert!(!shown(name), "{name}");
        }
        assert!(shown("Pokemon - Platinum Version (USA) / (Europe)") && !shown("Pokemon - Platin-Edition (Germany)"));
        assert!(shown("Diddy Kong Racing (Europe) (En,Fr,De) (Rev 1)") && !shown("Diddy Kong Racing (Japan)"));
    }

    #[test]
    fn links_awakening_counts_instruments_and_hearts() {
        let r = release_of(LinksAwakening, En);
        // The game marks all three places on its first start (seen in a save the game itself wrote, 15.11.): the one
        // named file is the one played, with nothing remembered yet.
        assert_eq!(line(&read(r, &ladx_save(&[(1, 0b0001_1111, 9)]), None).0), "5 of 8 instruments · 9 hearts");
        assert_eq!(read(r, &ladx_save(&[]), None).0, None, "places marked, no file named");
        assert_eq!(read(r, &ladx_save(&[(0, 1, 30)]), None).0, None, "30 heart containers don't exist");
        assert_eq!(read(r, &vec![0u8; 0x2000], None).0, None, "no file there");
    }

    /// Super Mario World's 2 KiB with `exits` for each file that is there (None: never saved).
    /// Letters of a DS game: generation 4's table or generation 5's UTF-16, ended with 0xFFFF, in `len` bytes.
    fn ds_text(s: &str, gen5: bool, len: usize) -> Vec<u8> {
        let mut out: Vec<u8> = s
            .chars()
            .flat_map(|c| {
                let v: u16 = if gen5 {
                    c as u16
                } else {
                    match c {
                        '0'..='9' => 0x0121 + (c as u16 - '0' as u16),
                        'A'..='Z' => 0x012B + (c as u16 - 'A' as u16),
                        'a'..='z' => 0x0145 + (c as u16 - 'a' as u16),
                        'À'..='ÿ' => 0x015F + (c as u16 - 0xC0),
                        '.' => 0x01AE,
                        '-' => 0x01BE,
                        ' ' => 0x01DE,
                        'ア' => 0x0052, // a katakana letter
                        _ => panic!("{c}"),
                    }
                };
                v.to_le_bytes()
            })
            .collect();
        out.extend([0xFF, 0xFF]);
        out.resize(len, 0xFF);
        out
    }

    /// A DS party Pokémon (236 bytes in generation 4, 220 in 5), scrambled as the game keeps it, with its checksum.
    pub(crate) fn ds_mon_bytes(pid: u32, species: u16, name: &str, level: u8, egg: bool, gen5: bool) -> Vec<u8> {
        let mut parts = [[0u8; 32]; 4];
        parts[0][..2].copy_from_slice(&species.to_le_bytes());
        let iv: u32 = 0x1234_5678 & !(3 << 30) | if egg { 1 << 30 } else { 0 };
        parts[1][0x10..0x14].copy_from_slice(&iv.to_le_bytes());
        parts[2][..22].copy_from_slice(&ds_text(name, gen5, 22));
        let plain: Vec<u8> = parts.concat();
        let sum = (0..64).fold(0u16, |a, i| a.wrapping_add(u16le(&plain, i * 2)));
        let order = DS_ORDER[(((pid >> 13) & 31) % 24) as usize];
        let mut stored = vec![0u8; 128];
        for (i, part) in parts.iter().enumerate() {
            stored[order[i] as usize * 32..order[i] as usize * 32 + 32].copy_from_slice(part);
        }
        let mut m = vec![0u8; if gen5 { 220 } else { 236 }];
        m[..4].copy_from_slice(&pid.to_le_bytes());
        m[6..8].copy_from_slice(&sum.to_le_bytes());
        m[8..136].copy_from_slice(&ds_crypt(&stored, sum as u32));
        let mut stats = vec![0u8; m.len() - 136];
        stats[4] = level;
        m[136..].copy_from_slice(&ds_crypt(&stats, pid));
        m
    }

    /// A DS team for the saves below: (name, level, egg), species 387 on (Turtwig …), an egg Pokémon 175.
    fn ds_party(team: &[(&str, u8, bool)], gen5: bool) -> Vec<u8> {
        team.iter()
            .enumerate()
            .flat_map(|(i, (name, level, egg))| {
                let pid = 0x9E37_79B9u32.wrapping_mul(i as u32 + 3);
                ds_mon_bytes(pid, if *egg { 175 } else { 387 + i as u16 }, name, *level, *egg, gen5)
            })
            .collect()
    }

    /// A Diamond/Pearl, Platinum or HeartGold/SoulSilver save of 512 KiB: the general block in both halves, the first
    /// with counters (`first`), the second with (`second`) and its own team and clock; a `None` half stays blank flash.
    pub(crate) fn gen4_save(kind: Kind, halves: [Option<((u32, u32), &[(&str, u8, bool)], (u8, u8), (u16, u8))>; 2]) -> Vec<u8> {
        let (size, covered, trainer, party) = match kind {
            DiamondPearl => (0xC100, 0xC100 - 0x14, 0x64, 0x98),
            Platinum => (0xCF2C, 0xCF2C - 0x14, 0x68, 0xA0),
            _ => (0xF628, 0xF628 - 0x10, 0x64, 0x98),
        };
        let mut s = vec![0xFFu8; 0x80000];
        for (half, h) in halves.iter().enumerate() {
            let Some(((save_no, block_no), team, (johto, kanto), (hours, minutes))) = h else { continue };
            let mut g = vec![0u8; size];
            g[trainer..trainer + 16].copy_from_slice(&ds_text("LUCAS", false, 16));
            g[trainer + 0x1A] = *johto;
            g[trainer + 0x1F] = *kanto;
            g[trainer + 0x22..trainer + 0x24].copy_from_slice(&hours.to_le_bytes());
            g[trainer + 0x24] = *minutes;
            g[trainer + 0x25] = 7;
            g[party - 4] = team.len() as u8;
            let p = ds_party(team, false);
            g[party..party + p.len()].copy_from_slice(&p);
            let foot = size - 0x14;
            g[foot..foot + 4].copy_from_slice(&save_no.to_le_bytes());
            g[foot + 4..foot + 8].copy_from_slice(&block_no.to_le_bytes());
            g[foot + 8..foot + 12].copy_from_slice(&(size as u32).to_le_bytes());
            g[foot + 12..foot + 16].copy_from_slice(&0x2006_0623u32.to_le_bytes());
            let sum = crc16_ccitt(&g[..covered]);
            g[size - 2..].copy_from_slice(&sum.to_le_bytes());
            s[half * 0x40000..half * 0x40000 + size].copy_from_slice(&g);
        }
        s
    }

    /// A Black/White or Black 2/White 2 save of 512 KiB: the party, the trainer's clock and the badges, each block with
    /// its CRC after it and in the table of sums.
    pub(crate) fn gen5_save(kind: Kind, team: &[(&str, u8, bool)], badges: u8, (hours, minutes): (u16, u8)) -> Vec<u8> {
        let (trainer_len, misc, misc_len, sums) = if kind == BlackWhite { (0x68, 0x21200, 0xEC, 0x23F00) } else { (0xB0, 0x21100, 0xF0, 0x25F00) };
        let mut s = vec![0u8; 0x80000];
        s[0x18E00 + 4] = team.len() as u8;
        let p = ds_party(team, true);
        s[0x18E08..0x18E08 + p.len()].copy_from_slice(&p);
        s[0x19404..0x19404 + 16].copy_from_slice(&ds_text("Hilda", true, 16));
        s[0x19424..0x19426].copy_from_slice(&hours.to_le_bytes());
        s[0x19426] = minutes;
        s[misc + 4] = badges;
        for (at, len, index) in [(0x18E00, 0x534, 26), (0x19400, trainer_len, 27), (misc, misc_len, 52)] {
            let sum = crc16_ccitt(&s[at..at + len]).to_le_bytes();
            s[at + len + 2..at + len + 4].copy_from_slice(&sum);
            s[sums + 2 * index..sums + 2 * index + 2].copy_from_slice(&sum);
        }
        s
    }

    /// The CRC the DS games use: CCITT from 0xFFFF, no final XOR ("123456789" gives 0x29B1, the standard check value).
    #[test]
    fn the_ds_crc_is_ccitt_from_ffff() {
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
        assert_eq!(crc16_ccitt(b""), 0xFFFF);
    }

    #[test]
    fn the_ds_games_say_badges_the_first_pokemon_and_the_clock() {
        let team: &[(&str, u8, bool)] = &[("TURTWIG", 14, false)];
        let save = gen4_save(DiamondPearl, [Some(((3, 1), team, (0b11, 0), (4, 5))), None]);
        assert_eq!(line(&read(release_of(DiamondPearl, En), &save, None).0), "2 badges · Turtwig Lv. 14 · game clock 4:05");
        let team: &[(&str, u8, bool)] = &[("EGG", 1, true), ("PLINFA", 31, false)];
        let save = gen4_save(Platinum, [None, Some(((9, 2), team, (0xFF, 0), (120, 59)))]);
        assert_eq!(line(&read(release_of(Platinum, De), &save, None).0), "8 badges · Plinfa Lv. 31 · game clock 120:59");
        // HeartGold and SoulSilver count Kanto's badges too.
        let team: &[(&str, u8, bool)] = &[("Blaze", 50, false)];
        let save = gen4_save(HeartGoldSoulSilver, [Some(((40, 3), team, (0xFF, 0b111), (55, 0))), None]);
        assert_eq!(line(&read(release_of(HeartGoldSoulSilver, En), &save, None).0), "11 badges · Blaze Lv. 50 · game clock 55:00");
        let save = gen5_save(BlackWhite, &[("Tepig", 9, false)], 1, (2, 30));
        assert_eq!(line(&read(release_of(BlackWhite, Fr), &save, None).0), "1 badge · Tepig Lv. 9 · game clock 2:30");
        let save = gen5_save(BlackWhite2, &[("Oshawott", 22, false), ("Éclair", 20, false)], 0b1111, (10, 1));
        assert_eq!(line(&read(release_of(BlackWhite2, It), &save, None).0), "4 badges · Oshawott Lv. 22 · game clock 10:01");
        // Black's blocks are not Black 2's.
        assert_eq!(read(release_of(BlackWhite, En), &save, None).0, None);
    }

    /// The general block is saved in turns to its two places; the newer one counts, by the save counter, then the
    /// block's own; a damaged newer one (a save cut short) leaves the older one, as the game loads it.
    #[test]
    fn the_ds_games_read_the_newer_valid_block() {
        let old: &[(&str, u8, bool)] = &[("CHIMCHAR", 10, false)];
        let new: &[(&str, u8, bool)] = &[("MONFERNO", 18, false)];
        let r = release_of(Platinum, En);
        let save = gen4_save(Platinum, [Some(((5, 1), old, (1, 0), (3, 0))), Some(((6, 1), new, (0b11, 0), (5, 0)))]);
        assert_eq!(line(&read(r, &save, None).0), "2 badges · Monferno Lv. 18 · game clock 5:00");
        let save = gen4_save(Platinum, [Some(((7, 1), old, (1, 0), (3, 0))), Some(((6, 9), new, (0b11, 0), (5, 0)))]);
        assert_eq!(line(&read(r, &save, None).0), "1 badge · Chimchar Lv. 10 · game clock 3:00", "the save counter first");
        let save = gen4_save(Platinum, [Some(((6, 2), old, (1, 0), (3, 0))), Some(((6, 1), new, (0b11, 0), (5, 0)))]);
        assert_eq!(line(&read(r, &save, None).0), "1 badge · Chimchar Lv. 10 · game clock 3:00", "then the block's");
        let mut cut = gen4_save(Platinum, [Some(((5, 1), old, (1, 0), (3, 0))), Some(((6, 1), new, (0b11, 0), (5, 0)))]);
        cut[0x40000 + 0x200] ^= 0x10;
        assert_eq!(line(&read(r, &cut, None).0), "1 badge · Chimchar Lv. 10 · game clock 3:00", "a damaged newer copy");
        // A fresh flash's counters (0xFFFFFFFF) are the oldest.
        let save = gen4_save(Platinum, [Some(((u32::MAX, u32::MAX), old, (1, 0), (3, 0))), Some(((0, 0), new, (0b11, 0), (5, 0)))]);
        assert_eq!(line(&read(r, &save, None).0), "2 badges · Monferno Lv. 18 · game clock 5:00");
    }

    /// Melon DS writes the bare 512 KiB, DeSmuME (`.dsv`) adds its footer after them; both read the same.
    #[test]
    fn a_desmume_footer_after_the_save_changes_nothing() {
        let team: &[(&str, u8, bool)] = &[("PIPLUP", 7, false)];
        let mut save = gen4_save(DiamondPearl, [Some(((1, 1), team, (0, 0), (0, 40))), None]);
        save.extend_from_slice(b"|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:");
        save.extend_from_slice(&[0u8; 40]);
        save.extend_from_slice(b"|-DESMUME SAVE-|");
        assert_eq!(line(&read(release_of(DiamondPearl, Es), &save, None).0), "No badges yet · Piplup Lv. 7 · game clock 0:40");
    }

    /// Damaged, blank, short or foreign saves show nothing; a Pokémon that fails its own checksum leaves out the team
    /// row (and the name, when it might be the lead), never the line.
    #[test]
    fn a_ds_save_that_isnt_sure_shows_nothing_or_less() {
        for kind in [DiamondPearl, Platinum, HeartGoldSoulSilver, BlackWhite, BlackWhite2] {
            let r = release_of(kind, En);
            for blank in [vec![0u8; 0x80000], vec![0xFFu8; 0x80000], vec![0u8; 0x1000], Vec::new()] {
                assert_eq!(read(r, &blank, None).0, None, "{kind:?}: an empty or short file");
            }
        }
        let team: &[(&str, u8, bool)] = &[("TURTWIG", 14, false)];
        let pearl = gen4_save(DiamondPearl, [Some(((1, 1), team, (1, 0), (1, 0))), None]);
        assert_eq!(read(release_of(Platinum, En), &pearl, None).0, None, "Pearl's block is not Platinum's");
        assert_eq!(read(release_of(HeartGoldSoulSilver, En), &pearl, None).0, None);
        let mut broken = gen4_save(HeartGoldSoulSilver, [Some(((1, 1), &[("CYNDAQUIL", 5, false), ("SENTRET", 3, false)], (1, 0), (1, 0))), None]);
        broken[0x98 + 236 + 0x40] ^= 1;
        let sum = crc16_ccitt(&broken[..0xF628 - 0x10]);
        broken[0xF626..0xF628].copy_from_slice(&sum.to_le_bytes());
        let Some(Progress::Pokemon { lead, art, .. }) = read(release_of(HeartGoldSoulSilver, En), &broken, None).0 else { panic!() };
        assert_eq!((lead, art.team), (Some(("Cyndaquil".into(), 5)), vec![]), "the second Pokémon fails: no team row, the lead stays");
        broken[0x98 + 0x40] ^= 1;
        let sum = crc16_ccitt(&broken[..0xF628 - 0x10]);
        broken[0xF626..0xF628].copy_from_slice(&sum.to_le_bytes());
        assert_eq!(line(&read(release_of(HeartGoldSoulSilver, En), &broken, None).0), "1 badge · game clock 1:00", "the lead unsure: no name");
        // A level the game can't have.
        let save = gen5_save(BlackWhite, &[("Snivy", 140, false)], 1, (1, 0));
        assert_eq!(line(&read(release_of(BlackWhite, En), &save, None).0), "1 badge · game clock 1:00");
        // A Japanese trade's name is left out, not shown half.
        let save = gen4_save(DiamondPearl, [Some(((1, 1), &[("ア", 30, false)], (1, 0), (2, 0))), None]);
        assert_eq!(line(&read(release_of(DiamondPearl, En), &save, None).0), "1 badge · game clock 2:00");
    }

    #[test]
    fn the_ds_games_tell_the_pictures_the_team() {
        let save = gen4_save(HeartGoldSoulSilver, [Some(((1, 1), &[("EGG", 1, true), ("CHIKORITA", 12, false), ("PIDGEY", 4, false)], (0b101, 1), (3, 0))), None]);
        let Some(Progress::Pokemon { art, .. }) = read(release_of(HeartGoldSoulSilver, En), &save, None).0 else { panic!("no progress") };
        assert_eq!(art, TeamArt { team: vec![DS_EGG, 388, 389], badges: 0b101 });
    }

    /// DS games are known by their cartridge's game code, read from its header when the header's own sum is right;
    /// Japanese releases (another letter), other games and a broken header are not.
    #[test]
    fn a_ds_game_is_known_by_its_header() {
        let header = |code: &[u8; 4]| {
            let mut h = vec![0u8; 0x200];
            h[..10].copy_from_slice(b"POKEMON PL");
            h[0x0C..0x10].copy_from_slice(code);
            let sum = h[..0x15E].iter().fold(0xFFFFu16, |mut c, &b| {
                c ^= b as u16;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
                }
                c
            });
            h[0x15E..0x160].copy_from_slice(&sum.to_le_bytes());
            h
        };
        let known = |h: &[u8]| ds_key(h).and_then(|k| release(k, None)).map(|r| (r.kind, r.lang));
        assert_eq!(known(&header(b"CPUE")), Some((Platinum, En)));
        assert_eq!(known(&header(b"IPGD")), Some((HeartGoldSoulSilver, De)));
        assert_eq!(known(&header(b"IRDO")), Some((BlackWhite2, En)));
        assert_eq!(known(&header(b"IPKJ")), None, "a Japanese HeartGold (and hacks made from it)");
        assert_eq!(known(&header(b"AMCE")), None, "another DS game");
        let mut bad = header(b"CPUE");
        bad[0x20] ^= 1;
        assert_eq!(known(&bad), None, "a header that fails its own sum");
        assert_eq!(known(&[0u8; 0x40]), None, "a file too short");
    }

    pub(crate) fn mario_save(exits: [Option<u8>; 3]) -> Vec<u8> {
        let mut s = vec![0u8; 0x800];
        for (i, e) in exits.iter().enumerate() {
            let Some(e) = e else { continue };
            for at in [i * 0x8F, i * 0x8F + 3 * 0x8F] {
                let f = &mut s[at..at + 0x8F];
                f[0] = 0x45 + i as u8; // a level beaten, so files differ
                f[0x8C] = *e;
                let sum = f[..0x8D].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
                f[0x8D..0x8F].copy_from_slice(&0x5A5Au16.wrapping_sub(sum).to_le_bytes());
            }
        }
        s
    }

    /// A Link to the Past's 8 KiB: for each file that is there, (pendant bits, crystal bits, heart containers, progress).
    pub(crate) fn zelda_save(files: [Option<(u8, u8, u8, u8)>; 3]) -> Vec<u8> {
        let mut s = vec![0u8; 0x2000];
        for (i, f) in files.iter().enumerate() {
            let Some((pendants, crystals, hearts, progress)) = f else { continue };
            for at in [i * 0x500, i * 0x500 + 0xF00] {
                let f = &mut s[at..at + 0x500];
                f[0x36C] = hearts * 8;
                f[0x36D] = hearts * 8;
                f[0x374] = *pendants;
                f[0x37A] = *crystals;
                f[0x3C5] = *progress;
                f[0x3E5..0x3E7].copy_from_slice(&0x55AAu16.to_le_bytes());
                let sum = (0..0x27F).fold(0u16, |a, w| a.wrapping_add(u16le(f, w * 2)));
                f[0x4FE..0x500].copy_from_slice(&0x5A5Au16.wrapping_sub(sum).to_le_bytes());
            }
        }
        s
    }

    /// Fire Emblem's 32 KiB (`eighth`: The Sacred Stones) with `games` (save, chapter, story, state bits, clock in
    /// frames, The Sacred Stones' Normal bit) and the save written last.
    pub(crate) fn fe_save(eighth: bool, games: &[(usize, u8, u8, u8, u32, bool)], last: u8) -> Vec<u8> {
        let (head, game) = if eighth { (0x40624u32, 0x40624u32) } else { (0x30317, 0x11217) };
        let mut s = vec![0xFFu8; 0x8000];
        s[..0x64].fill(0);
        s[..7].copy_from_slice(if eighth { b"AGB-FE9" } else { b"AGB-FE7" });
        s[8..12].copy_from_slice(&head.to_le_bytes());
        s[0xC..0xE].copy_from_slice(&0x200Au16.to_le_bytes());
        let (add, xor) = (0..0x28).fold((0u32, 0u32), |(a, x), w| (a + u16le(&s, 2 * w) as u32, x ^ u16le(&s, 2 * w) as u32));
        s[0x60..0x62].copy_from_slice(&(add.wrapping_add(xor) as u16).to_le_bytes());
        s[0x62] = last;
        for &(i, chapter, story, bits, frames, normal) in games {
            let (at, size) = (0x1000 + i * 0x1800, 0x1000usize);
            let b = &mut s[at..at + size];
            b.fill(0);
            b[..4].copy_from_slice(&frames.to_le_bytes());
            b[0xC] = i as u8;
            b[0xE] = chapter;
            b[0x14] = bits;
            b[0x1B] = story;
            if normal {
                b[0x42] |= 0x20;
            }
            b[0x100 + i] = 0x5A; // the army
            let (add, xor) = (0..size / 2).fold((0u32, 0u32), |(a, x), w| (a + u16le(b, 2 * w) as u32, x ^ u16le(b, 2 * w) as u32));
            let e = &mut s[0x64 + i * 0x10..0x64 + (i + 1) * 0x10];
            e[..4].copy_from_slice(&game.to_le_bytes());
            e[4..6].copy_from_slice(&0x200Au16.to_le_bytes());
            e[6] = 0;
            e[8..10].copy_from_slice(&(at as u16).to_le_bytes());
            e[10..12].copy_from_slice(&(size as u16).to_le_bytes());
            e[12..16].copy_from_slice(&((add & 0xFFFF) | (xor << 16)).to_le_bytes());
        }
        s
    }

    const HOUR: u32 = 216_000;

    #[test]
    fn fire_emblem_says_the_chapter_the_story_the_mode_and_the_clock() {
        let r = release_of(FireEmblem7, En);
        // Hector's story counts one chapter more than Eliwood's from his chapter 15 on: index 0x16 is his 18.
        let save = fe_save(false, &[(0, 0x16, 3, 0x40, 14 * HOUR + 52 * 3600 + 59 * 60, false), (1, 0x16, 2, 0, HOUR, false)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 18 · Hector · game clock 14:52");
        // The save the game wrote last, not the first.
        let save = fe_save(false, &[(0, 0x16, 3, 0x40, HOUR, false), (1, 0x16, 2, 0, 2 * HOUR + 5 * 3600, false)], 1);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 17 · Eliwood · game clock 2:05");
        // Lyn's tale: its own numbers, one mode; a side chapter keeps its x.
        let save = fe_save(false, &[(2, 8, 1, 0, 3600 * 40, false)], 2);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 7x · Lyn · game clock 0:40");
        assert_eq!(line(&read(r, &fe_save(false, &[(0, 0, 1, 0, 0, false)], 0), None).0), "Prologue · Lyn · game clock 0:00");
        // A chapter Eliwood's story doesn't have: no chapter, never another's number.
        let save = fe_save(false, &[(0, 0x12, 2, 0, HOUR, false)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Eliwood · game clock 1:00");
        let save = fe_save(false, &[(0, 0x2E, 2, 0x20, HOUR, false)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Epilogue · Eliwood · game clock 1:00");
    }

    #[test]
    fn sacred_stones_says_the_route_and_its_three_modes() {
        let r = release_of(FireEmblem8, En);
        let save = fe_save(true, &[(1, 5, 1, 0, 3 * HOUR + 7 * 3600, true)], 1);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 5x · game clock 3:07");
        let save = fe_save(true, &[(0, 0x19, 3, 0x40, 20 * HOUR, false)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 12 · Ephraim · game clock 20:00");
        let save = fe_save(true, &[(0, 0x3D, 2, 0, HOUR, false)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Chapter 11 · Eirika · game clock 1:00");
        // A tower floor or the creature campaign: the menu names a place, the line leaves the chapter out.
        let save = fe_save(true, &[(0, 0x24, 2, 0x04, HOUR, true)], 0);
        assert_eq!(line(&read(r, &save, None).0), "Eirika · game clock 1:00");
    }

    #[test]
    fn fire_emblem_believes_only_what_the_game_would_load() {
        let r = release_of(FireEmblem8, En);
        // A cartridge never saved on (all 0xFF), nothing at all, a cut-off file: nothing.
        assert_eq!(read(r, &vec![0xFFu8; 0x8000], None).0, None);
        assert_eq!(read(r, &[], None).0, None);
        let save = fe_save(true, &[(0, 3, 1, 0, HOUR, true)], 0);
        assert_eq!(read(r, &save[..0x4000], None).0, None);
        // One bit off in the game's block: its sum says damaged, and the game wouldn't load it.
        let mut broken = save.clone();
        broken[0x1000 + 0x200] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
        // A broken header: the game sees no saves at all.
        let mut broken = save.clone();
        broken[0x20] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
        // The seventh's save is not the eighth's (other magic numbers).
        assert_eq!(read(r, &fe_save(false, &[(0, 3, 2, 0, HOUR, false)], 0), None).0, None);
        // The last save damaged: the other one, as the file the game would still load.
        let mut two = fe_save(true, &[(0, 3, 1, 0, HOUR, true), (1, 7, 1, 0, 2 * HOUR, true)], 1);
        two[0x2800 + 0x300] ^= 1;
        assert_eq!(line(&read(r, &two, None).0), "Chapter 3 · game clock 1:00");
    }

    /// Donkey Kong Country's 2 KiB (`kind`: which of the three) with `files` (file, share, clock in frames), as each game
    /// writes them; the rest as an untouched cartridge has it (0x60).
    pub(crate) fn dkc_save(kind: Kind, files: &[(usize, u8, u32)]) -> Vec<u8> {
        let mut s = vec![0x60u8; 0x800];
        let words = |f: &[u8], from: usize, to: usize| (from..=to).step_by(2).fold((0u16, 0u16), |(a, x), w| (a.wrapping_add(u16le(f, w)), x ^ u16le(f, w)));
        for &(i, percent, frames) in files {
            match kind {
                DonkeyKong1 => {
                    let f = &mut s[8 + 2 * i * 0x154..8 + (2 * i + 1) * 0x154];
                    f.fill(0);
                    f[4..8].copy_from_slice(b"ARER");
                    f[8] = 0;
                    f[0x0E] = percent;
                    f[0x10 + i] = 0x7F; // levels done
                    let (sum, xor) = words(f, 4, 0x114);
                    f[0..2].copy_from_slice(&xor.to_le_bytes());
                    f[2..4].copy_from_slice(&sum.to_le_bytes());
                }
                DonkeyKong2 | DonkeyKong3 => {
                    let (at, size, last, clock, share) = if kind == DonkeyKong2 { (8, 0x2A8, 0x2A0, 6, 10) } else { (0x62, 0x28A, 0x288, 6 + 0x0A, 6 + 0x0E) };
                    let f = &mut s[at + i * size..at + (i + 1) * size];
                    f.fill(0);
                    f[4] = 0x52;
                    f[clock..clock + 4].copy_from_slice(&frames.to_le_bytes());
                    f[share] = percent;
                    f[0x40 + i] = 0x33;
                    let (sum, xor) = words(f, 6, last);
                    let sum = if kind == DonkeyKong3 { sum.max(1) } else { sum };
                    f[0..2].copy_from_slice(&sum.to_le_bytes());
                    f[2..4].copy_from_slice(&xor.to_le_bytes());
                }
                _ => unreachable!(),
            }
        }
        s
    }

    #[test]
    fn donkey_kong_country_shows_its_file_selects_share() {
        let one = release_of(DonkeyKong1, En);
        assert_eq!(line(&read(one, &dkc_save(DonkeyKong1, &[(1, 62, 0)]), None).0), "62% complete");
        let two = release_of(DonkeyKong2, En);
        assert_eq!(line(&read(two, &dkc_save(DonkeyKong2, &[(2, 102, 31 * HOUR + 12 * 3600)]), None).0), "102% complete · game clock 31:12");
        let three = release_of(DonkeyKong3, En);
        let save = dkc_save(DonkeyKong3, &[(0, 41, 6 * HOUR + 3 * 3600)]);
        let (p, _) = read(three, &save, None);
        assert_eq!(line(&p), "41% complete · game clock 6:03");
        assert_eq!(p.unwrap().meter(), Some(Meter::Share { done: 41, of: 105 }));
    }

    #[test]
    fn donkey_kong_country_follows_the_file_that_changed() {
        let r = release_of(DonkeyKong2, En);
        // Two files and no history: the save doesn't say which was played, so nothing rather than a guess.
        let before = dkc_save(DonkeyKong2, &[(0, 20, HOUR), (1, 50, HOUR)]);
        let (p, memory) = read(r, &before, None);
        assert_eq!(p, None);
        // File 1 saved again: that one.
        let after = dkc_save(DonkeyKong2, &[(0, 20, HOUR), (1, 53, 2 * HOUR)]);
        assert_eq!(line(&read(r, &after, Some(&memory)).0), "53% complete · game clock 2:00");
    }

    #[test]
    fn donkey_kong_country_believes_only_what_the_game_would_load() {
        for kind in [DonkeyKong1, DonkeyKong2, DonkeyKong3] {
            let r = release_of(kind, En);
            // Alex's never-saved cartridges (all 0x60), nothing, a cut-off file.
            assert_eq!(read(r, &vec![0x60u8; 0x800], None).0, None, "{kind:?}");
            assert_eq!(read(r, &[], None).0, None);
            let save = dkc_save(kind, &[(0, 30, HOUR)]);
            assert_eq!(read(r, &save[..0x400], None).0, None);
            // One bit off: the game calls the file damaged.
            let mut broken = save.clone();
            let at = match kind { DonkeyKong1 => 8 + 0x40, DonkeyKong2 => 8 + 0x100, _ => 0x62 + 0x100 };
            broken[at] ^= 1;
            assert_eq!(read(r, &broken, None).0, None, "{kind:?}");
            // A share the game can't count to.
            assert_eq!(read(r, &dkc_save(kind, &[(0, 200, HOUR)]), None).0, None, "{kind:?}");
        }
        // An erased first-game file ("NULL" where "ARER" was) is no file.
        let mut erased = dkc_save(DonkeyKong1, &[(0, 30, 0)]);
        erased[8 + 4..8 + 8].copy_from_slice(b"NULL");
        assert_eq!(read(release_of(DonkeyKong1, En), &erased, None).0, None);
        // The second game's file is not the third's: other places.
        assert_eq!(read(release_of(DonkeyKong3, En), &dkc_save(DonkeyKong2, &[(0, 30, HOUR)]), None).0, None);
    }

    /// Chrono Trigger's 8 KiB with `slots` (slot, the party's leader by number, its name, level, clock) and the slot
    /// saved last; the rest as an untouched cartridge has it (0x60, as Alex's pocket file).
    pub(crate) fn chrono_save(slots: &[(usize, u8, &str, u8, (u8, u8))], last: u8) -> Vec<u8> {
        let mut s = vec![0x60u8; 0x2000];
        for &(i, leader, name, level, (h, m)) in slots {
            let f = &mut s[i * 0xA00..(i + 1) * 0xA00];
            f.fill(0);
            f[0x580..0x583].copy_from_slice(&[leader, 0x80, 0x80]);
            let at = 0x5B0 + 6 * leader as usize;
            for (j, c) in name.bytes().enumerate() {
                f[at + j] = if c.is_ascii_uppercase() { c - b'A' + 0xA0 } else { c - b'a' + 0xBA };
            }
            f[0x200 + 0x50 * leader as usize + 0x12] = level;
            // The game's clock digits (`chrono_trigger`): minute units, minute tens, hour units, hour tens.
            f[0x5E5..0x5E9].copy_from_slice(&[m % 10, m / 10, h % 10, h / 10]);
            f[0x5E4] = 17;
            f[0x59C] = 3;
            let mut sum = 0u32;
            for w in (0..0x500).rev() {
                if sum > 0xFFFF {
                    sum -= 0xFFFF;
                }
                sum += u16le(f, 2 * w) as u32;
            }
            s[0x1FF0 + 2 * i..0x1FF2 + 2 * i].copy_from_slice(&(sum as u16).to_le_bytes());
            s[0x1FF8 + 2 * i..0x1FFA + 2 * i].copy_from_slice(&0xE41Bu16.to_le_bytes());
        }
        s[0x1FE0] = last;
        s
    }

    #[test]
    fn chrono_trigger_names_the_party_leader_and_the_clock() {
        let r = release_of(ChronoTrigger, En);
        assert_eq!(line(&read(r, &chrono_save(&[(0, 0, "Crono", 12, (5, 7))], 0), None).0), "Crono Lv. 12 · game clock 5:07");
        // The slot saved last; Lucca leading.
        let save = chrono_save(&[(0, 0, "Crono", 12, (5, 7)), (2, 2, "Lucca", 31, (17, 40))], 2);
        assert_eq!(line(&read(r, &save, None).0), "Lucca Lv. 31 · game clock 17:40");
        // A sum carrying over many times: big words throughout the slot.
        let mut heavy = chrono_save(&[(1, 1, "Marle", 40, (20, 0))], 1);
        for w in 0..0x100 {
            heavy[0xA00 + 0x700 + 2 * w] = 0xFF;
            heavy[0xA00 + 0x701 + 2 * w] = 0xFE;
        }
        let f = heavy[0xA00..0x1400].to_vec();
        let mut sum = 0u32;
        for w in (0..0x500).rev() {
            if sum > 0xFFFF {
                sum -= 0xFFFF;
            }
            sum += u16le(&f, 2 * w) as u32;
        }
        heavy[0x1FF2..0x1FF4].copy_from_slice(&(sum as u16).to_le_bytes());
        assert_eq!(line(&read(r, &heavy, None).0), "Marle Lv. 40 · game clock 20:00");
    }

    /// Chrono Trigger's slot `i` summed again after its bytes changed (`chrono_trigger`'s sum).
    fn chrono_resum(s: &mut [u8], i: usize) {
        let mut sum = 0u32;
        for w in (0..0x500).rev() {
            if sum > 0xFFFF {
                sum -= 0xFFFF;
            }
            sum += u16le(&s[i * 0xA00..], 2 * w) as u32;
        }
        s[0x1FF0 + 2 * i..0x1FF2 + 2 * i].copy_from_slice(&(sum as u16).to_le_bytes());
    }

    #[test]
    fn chrono_trigger_reads_the_clock_as_the_game_writes_it() {
        // The six clock bytes (0x5E3 on) of two saves the game itself wrote (snes9x, the USA ROM, 15.11.2026), with what
        // the game's own menu showed as TIME: frames, seconds, minute units, minute tens, hour units, hour tens. Read
        // as minutes at 0x5E5 and hours at 0x5E6 (Data Crystal's map), the first said "game clock 4:04".
        let r = release_of(ChronoTrigger, En);
        for (clock, shown) in [([0x02, 0x20, 4, 4, 8, 1], "Crono Lv. 32 · game clock 18:44"), ([0x2D, 0x25, 2, 0, 0, 0], "Crono Lv. 32 · game clock 0:02")] {
            let mut save = chrono_save(&[(0, 0, "Crono", 32, (0, 0))], 0);
            save[0x5E3..0x5E9].copy_from_slice(&clock);
            chrono_resum(&mut save, 0);
            assert_eq!(line(&read(r, &save, None).0), shown);
        }
        // A digit the game never writes (minute tens 6, an hour digit over 9): no line rather than a wrong clock.
        for (at, value) in [(0x5E6, 6u8), (0x5E8, 10), (0x5E5, 10)] {
            let mut save = chrono_save(&[(0, 0, "Crono", 32, (18, 44))], 0);
            save[at] = value;
            chrono_resum(&mut save, 0);
            assert_eq!(read(r, &save, None).0, None, "byte {at:#x} = {value}");
        }
        // The highest the menu shows.
        assert_eq!(line(&read(r, &chrono_save(&[(0, 0, "Crono", 99, (99, 59))], 0), None).0), "Crono Lv. 99 · game clock 99:59");
    }

    #[test]
    fn chrono_trigger_believes_only_what_the_game_would_load() {
        let r = release_of(ChronoTrigger, En);
        // Alex's pocket file, never saved on (all 0x60), and an empty one: nothing.
        assert_eq!(read(r, &vec![0x60u8; 0x2000], None).0, None);
        assert_eq!(read(r, &vec![0u8; 0x2000], None).0, None, "zeros add up to 0, but no slot is marked as used");
        assert_eq!(read(r, &[], None).0, None);
        let mut broken = chrono_save(&[(0, 0, "Crono", 12, (5, 7))], 0);
        broken[0x300] ^= 1;
        assert_eq!(read(r, &broken, None).0, None);
        // A sum taken front to back instead (the editors' test saves tell the two apart): no line.
        let mut forward = chrono_save(&[(0, 0, "Crono", 12, (5, 7))], 0);
        forward[0x9FE] = 0xFF;
        forward[0x9FF] = 0xFF;
        forward[0x10] = 0x02;
        let (mut back, mut front) = (0u32, 0u32);
        for w in (0..0x500).rev() {
            if back > 0xFFFF {
                back -= 0xFFFF;
            }
            back += u16le(&forward, 2 * w) as u32;
        }
        for w in 0..0x500 {
            if front > 0xFFFF {
                front -= 0xFFFF;
            }
            front += u16le(&forward, 2 * w) as u32;
        }
        assert_ne!(back as u16, front as u16);
        forward[0x1FF0..0x1FF2].copy_from_slice(&(front as u16).to_le_bytes());
        assert_eq!(read(r, &forward, None).0, None);
    }

    /// Golden Sun's 64 KiB flash (`lost_age`: The Lost Age) with `records` (record place, file number, count, leader's
    /// name, level, clock in frames); the rest erased flash (0xFF).
    pub(crate) fn golden_sun_save(lost_age: bool, records: &[(usize, u8, u16, &str, u8, u32)]) -> Vec<u8> {
        let (size, summed) = if lost_age { (0x3000, 0x3000) } else { (0x1000, 0xFF8) };
        let mut s = vec![0xFFu8; 0x10000];
        for &(k, number, count, name, level, frames) in records {
            let r = &mut s[k * size..(k + 1) * size];
            r.fill(0);
            r[..7].copy_from_slice(b"CAMELOT");
            r[7] = number;
            r[0xA..0xC].copy_from_slice(&count.to_le_bytes());
            r[0x10..0x10 + name.len()].copy_from_slice(&name.bytes().collect::<Vec<_>>());
            r[0x10 + 0x0C] = level;
            r[0x10 + 0x10..0x10 + 0x14].copy_from_slice(&frames.to_le_bytes());
            r[0x10 + 0x500] = 0x42;
            r[size - 4] = 0x99; // past the first game's sum
            let sum = r[0x10..summed].iter().fold(0u32, |a, b| a + *b as u32) as u16;
            r[8..10].copy_from_slice(&sum.to_le_bytes());
        }
        s
    }

    #[test]
    fn golden_sun_names_the_leader_his_level_and_the_clock() {
        let gs = release_of(GoldenSun, En);
        assert_eq!(line(&read(gs, &golden_sun_save(false, &[(0, 0, 4, "Isaac", 23, 12 * HOUR + 5 * 3600)]), None).0), "Isaac Lv. 23 · game clock 12:05");
        // Two records of one file: the higher count is the newer save, wherever it stands.
        let save = golden_sun_save(false, &[(1, 2, 7, "Isaac", 11, HOUR), (9, 2, 8, "Isaac", 12, 2 * HOUR)]);
        assert_eq!(line(&read(gs, &save, None).0), "Isaac Lv. 12 · game clock 2:00");
        // The newer damaged: the older, as the game falls back to it.
        let mut damaged = save.clone();
        damaged[9 * 0x1000 + 0x300] ^= 1;
        assert_eq!(line(&read(gs, &damaged, None).0), "Isaac Lv. 11 · game clock 1:00");
        let tla = release_of(GoldenSunLostAge, En);
        assert_eq!(line(&read(tla, &golden_sun_save(true, &[(3, 1, 2, "Felix", 37, 30 * HOUR)]), None).0), "Felix Lv. 37 · game clock 30:00");
        // The German release's own spelling, in its letters.
        let de = release_of(GoldenSun, De);
        let mut save = golden_sun_save(false, &[(0, 0, 1, "Isaac", 5, HOUR)]);
        save[0x10..0x15].copy_from_slice(b"Iv\xE1n\0");
        let sum = save[0x10..0xFF8].iter().fold(0u32, |a, b| a + *b as u32) as u16;
        save[8..10].copy_from_slice(&sum.to_le_bytes());
        assert_eq!(line(&read(de, &save, None).0), "Iván Lv. 5 · game clock 1:00");
    }

    #[test]
    fn golden_sun_believes_only_what_the_game_would_load() {
        let gs = release_of(GoldenSun, En);
        assert_eq!(read(gs, &vec![0xFFu8; 0x10000], None).0, None, "erased flash");
        assert_eq!(read(gs, &[], None).0, None);
        let save = golden_sun_save(false, &[(0, 0, 4, "Isaac", 23, HOUR)]);
        assert_eq!(read(gs, &save[..0x8000], None).0, None);
        let mut broken = save.clone();
        broken[0x400] ^= 1;
        assert_eq!(read(gs, &broken, None).0, None);
        // The last 8 bytes are outside the first game's sum; The Lost Age's sum takes its whole record.
        let mut tail = save.clone();
        tail[0xFFA] ^= 1;
        assert_eq!(line(&read(gs, &tail, None).0), "Isaac Lv. 23 · game clock 1:00");
        let mut tail = golden_sun_save(true, &[(0, 0, 4, "Felix", 23, HOUR)]);
        tail[0x2FFA] ^= 1;
        assert_eq!(read(release_of(GoldenSunLostAge, En), &tail, None).0, None);
        // A file started but empty (level 0), and the record of number 3 (not a file): nothing.
        assert_eq!(read(gs, &golden_sun_save(false, &[(0, 0, 4, "Isaac", 0, HOUR)]), None).0, None);
        assert_eq!(read(gs, &golden_sun_save(false, &[(0, 3, 4, "Isaac", 9, HOUR)]), None).0, None);
    }

    /// Kirby & the Amazing Mirror's save with `worlds` (file, shards' bits, rooms seen), each block written twice.
    pub(crate) fn kirby_save(worlds: &[(usize, u8, u8)]) -> Vec<u8> {
        // The cartridge's 32 KiB SRAM; what the game never writes stays erased.
        let mut s = vec![0xFFu8; 0x8000];
        let put = |s: &mut Vec<u8>, at: usize, data: &[u8]| {
            let a = (0..data.len() / 2).fold(0u16, |a, w| a.wrapping_add(u16le(data, 2 * w)));
            for (i, v) in [a, !a, (!a).wrapping_add(1), 0].into_iter().enumerate() {
                s[at + 2 * i..at + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
            }
            s[at + 8..at + 8 + data.len()].copy_from_slice(data);
        };
        for k in 0..2 {
            put(&mut s, k * 12, &[0, 0, 0, 0]);
        }
        for i in 0..3 {
            let mut w = vec![0u8; 0x30C];
            if let Some(&(_, shards, rooms)) = worlds.iter().find(|x| x.0 == i) {
                w[0xA0] = rooms;
                w[0x20 + 0x80 + 0x23E + 0x10] = shards;
                w[0x20 + 0x80 + 0x23E] = 0x0F; // chests
            }
            for k in [2 * i, 2 * i + 1] {
                put(&mut s, 24 + k * (8 + 0x30C), &w);
            }
        }
        s
    }

    #[test]
    fn kirby_counts_the_mirror_shards() {
        let r = release_of(KirbyMirror, En);
        let (p, _) = read(r, &kirby_save(&[(1, 0b0010_1101, 3)]), None);
        assert_eq!(line(&p), "4 of 8 mirror shards");
        assert_eq!(p.unwrap().meter(), Some(Meter::Marks { done: 4, of: 8 }));
        // Just started, no shard yet: still a file, and it says so.
        assert_eq!(line(&read(r, &kirby_save(&[(0, 0, 1)]), None).0), "0 of 8 mirror shards");
        // The first copy damaged: the second, as the game reads it.
        let mut save = kirby_save(&[(2, 0xFF, 9)]);
        save[24 + 4 * (8 + 0x30C) + 8 + 0x50] ^= 1;
        assert_eq!(line(&read(r, &save, None).0), "8 of 8 mirror shards");
        // Both damaged: nothing.
        save[24 + 5 * (8 + 0x30C) + 8 + 0x50] ^= 1;
        assert_eq!(read(r, &save, None).0, None);
    }

    #[test]
    fn kirby_shows_nothing_without_a_played_file() {
        let r = release_of(KirbyMirror, En);
        // Three fresh files the game wrote at its first start (all zeros, each with its check): nothing to say.
        assert_eq!(read(r, &kirby_save(&[]), None).0, None);
        assert_eq!(read(r, &vec![0xFFu8; 0x10000], None).0, None);
        assert_eq!(read(r, &[], None).0, None);
        // Two played files and no history: which one isn't known yet; the next save tells.
        let (p, memory) = read(r, &kirby_save(&[(0, 1, 2), (1, 3, 2)]), None);
        assert_eq!(p, None);
        assert_eq!(line(&read(r, &kirby_save(&[(0, 1, 2), (1, 7, 3)]), Some(&memory)).0), "3 of 8 mirror shards");
    }

    fn line(p: &Option<Progress>) -> String {
        p.as_ref().map_or("(nothing)".into(), |p| p.parts().join(" · "))
    }

    #[test]
    fn red_says_badges_the_first_pokemon_and_the_clock() {
        let save = gen1_save(&[("CHARIZARD", 45), ("PIKACHU", 30)], 0b0111_1111, (31, 12));
        assert_eq!(line(&read(release_of(Gen1, En), &save, None).0), "7 badges · Charizard Lv. 45 · game clock 31:12");
        // The German game's own name, umlauts and all.
        let save = gen1_save(&[("GLURAK", 45)], 0xFF, (31, 2));
        assert_eq!(line(&read(release_of(Gen1, De), &save, None).0), "8 badges · Glurak Lv. 45 · game clock 31:02");
    }

    #[test]
    fn a_name_the_player_gave_keeps_its_own_spelling() {
        let save = gen1_save(&[("Sparky", 12)], 0, (1, 5));
        assert_eq!(line(&read(release_of(Gen1, En), &save, None).0), "No badges yet · Sparky Lv. 12 · game clock 1:05");
        for (raw, shown) in [("MR.MIME", "Mr.Mime"), ("FARFETCH'D", "Farfetch'd"), ("HO-OH", "Ho-Oh"), ("PORYGON2", "Porygon2"), ("ÄRGER", "Ärger")] {
            assert_eq!(shown_name(raw), shown);
        }
    }

    #[test]
    fn one_badge_and_a_save_before_the_first_pokemon() {
        let save = gen1_save(&[], 1, (0, 9));
        assert_eq!(line(&read(release_of(Gen1, En), &save, None).0), "1 badge · game clock 0:09");
    }

    #[test]
    fn gold_and_crystal_count_kanto_badges_and_skip_an_egg() {
        let save = gen2_save(false, &[("EGG", 5, true), ("TYPHLOSION", 52, false)], (0xFF, 0b111), (40, 7));
        assert_eq!(line(&read(release_of(GoldSilver, En), &save, None).0), "11 badges · Typhlosion Lv. 52 · game clock 40:07");
        let save = gen2_save(true, &[("TOTODILE", 9, false)], (1, 0), (2, 30));
        assert_eq!(line(&read(release_of(Crystal, De), &save, None).0), "1 badge · Totodile Lv. 9 · game clock 2:30");
        // Gold's layout is not Crystal's: the same bytes read as Crystal fail its checksum.
        let gold = gen2_save(false, &[("TOTODILE", 9, false)], (1, 0), (2, 30));
        assert_eq!(read(release_of(Crystal, En), &gold, None).0, None);
    }

    #[test]
    fn gold_and_crystal_tell_the_pictures_the_team_and_johtos_badges() {
        let save = gen2_save(true, &[("EGG", 5, true), ("CYNDAQUIL", 12, false), ("PIDGEY", 4, false)], (0b101, 0xFF), (3, 0));
        let Some(Progress::Pokemon { art, .. }) = read(release_of(Crystal, En), &save, None).0 else { panic!("no progress") };
        assert_eq!(art, TeamArt { team: vec![0xFD, 156, 157], badges: 0b101 });
    }

    #[test]
    fn emerald_fire_red_and_ruby_read_the_newer_copy() {
        let save = gen3_save(Emerald, &[("BLAZIKEN", 61, false)], 6, (24, 40), 50);
        assert_eq!(line(&read(release_of(Emerald, En), &save, None).0), "6 badges · Blaziken Lv. 61 · game clock 24:40");
        let save = gen3_save(FireRedLeafGreen, &[("EI", 1, true), ("GLURAK", 45, false)], 8, (31, 12), 9);
        assert_eq!(line(&read(release_of(FireRedLeafGreen, De), &save, None).0), "8 badges · Glurak Lv. 45 · game clock 31:12");
        let save = gen3_save(RubySapphire, &[("Mudkip", 7, false)], 0, (0, 45), 3);
        assert_eq!(line(&read(release_of(RubySapphire, Es), &save, None).0), "No badges yet · Mudkip Lv. 7 · game clock 0:45");
    }

    /// The pictures' data: each Pokémon's species unscrambled (an egg is 412) and which badges are won; a Pokémon whose
    /// own checksum fails leaves the whole row out rather than show a wrong one.
    #[test]
    fn the_team_and_the_badges_for_the_pictures() {
        let save = gen3_save(Emerald, &[("EI", 1, true), ("TORCHIC", 12, false), ("PIKACHU", 9, false)], 3, (1, 0), 4);
        let Some(Progress::Pokemon { art, .. }) = read(release_of(Emerald, En), &save, None).0 else { panic!() };
        assert_eq!(art, TeamArt { team: vec![412, 281, 282], badges: 0b111 });
        let mut broken = save.clone();
        for slot in 0..2 {
            for k in 0..SECTORS {
                let base = (slot * SECTORS + k) * SECTOR;
                if u16le(&broken, base + 0xFF4) == 1 {
                    broken[base + 0x238 + 100 + 0x1C] ^= 1;
                    let sum = (0..SECTOR_DATA / 4).fold(0u32, |a, w| a.wrapping_add(u32le(&broken[base..], w * 4)));
                    let sum = (sum as u16).wrapping_add((sum >> 16) as u16);
                    broken[base + 0xFF6..base + 0xFF8].copy_from_slice(&sum.to_le_bytes());
                }
            }
        }
        let Some(Progress::Pokemon { art, lead, .. }) = read(release_of(Emerald, En), &broken, None).0 else { panic!() };
        assert!(art.team.is_empty() && lead.is_some(), "no team row, the line stays");
    }

    #[test]
    fn a_damaged_newer_copy_falls_back_to_the_older_one_as_the_game_does() {
        let older = gen3_save(Emerald, &[("TORCHIC", 10, false)], 1, (3, 0), 7);
        let mut save = gen3_save(Emerald, &[("COMBUSKEN", 20, false)], 2, (6, 0), 7);
        // The first copy keeps the older play; the newer one is cut by a write that never finished.
        save[..SECTORS * SECTOR].copy_from_slice(&older[..SECTORS * SECTOR]);
        assert_eq!(line(&read(release_of(Emerald, En), &save, None).0), "2 badges · Combusken Lv. 20 · game clock 6:00");
        save[SECTORS * SECTOR + 0x123] ^= 0x40;
        assert_eq!(line(&read(release_of(Emerald, En), &save, None).0), "1 badge · Torchic Lv. 10 · game clock 3:00");
    }

    #[test]
    fn super_metroid_counts_items_as_its_ending_does() {
        // 5 energy tanks, 1 reserve, 20 missiles, 10 supers, 5 power bombs (13), morph, bombs, charge and ice (4).
        let save = metroid_save(1, (599, 100, 20, 10, 5), 0x0004 | 0x1000, 0x1000 | 0x0002, (4, 31));
        assert_eq!(line(&read(release_of(SuperMetroid, En), &save, None).0), "17% items · game clock 4:31");
        let full = metroid_save(0, (1499, 400, 230, 50, 50), 0xF32F, 0x100F, (2, 50));
        assert_eq!(line(&read(release_of(SuperMetroid, En), &full, None).0), "100% items · game clock 2:50");
    }

    #[test]
    fn all_stars_reads_super_mario_worlds_files_where_it_keeps_them() {
        let r = release_of(AllStarsWorld, En);
        let world = mario_save([Some(6), None, None]);
        let mut s = vec![0u8; 0x2000];
        s[0x1000..0x1000 + 0x8F].copy_from_slice(&world[..0x8F]);
        s[0x123C..0x123C + 0x8F].copy_from_slice(&world[..0x8F]);
        assert_eq!(line(&read(r, &s, None).0), "6 of 96 exits");
        s[0x1000] ^= 1;
        assert_eq!(line(&read(r, &s, None).0), "6 of 96 exits", "the copy when the file is damaged");
        let mut plain = vec![0u8; 0x2000];
        plain[..0x800].copy_from_slice(&world);
        assert_eq!(read(r, &plain, None).0, None, "Super Mario World's own save isn't All-Stars'");
        assert!(worth_checking("snes", 5 << 19, false) && worth_checking("snes", (3 << 20) + 512, false));
        // The cartridge has a fourth file, D, and says at 0x0E which file was chosen last (in saves the game wrote,
        // 15.11.: the file select's cursor starts there).
        let file = |exits: u8| mario_save([Some(exits), None, None])[..0x8F].to_vec();
        let mut d = vec![0u8; 0x2000];
        d[0x11AD..0x11AD + 0x8F].copy_from_slice(&file(47));
        assert_eq!(line(&read(r, &d, None).0), "47 of 96 exits", "file D alone");
        d[0x1000..0x1000 + 0x8F].copy_from_slice(&file(6));
        d[0x0E] = 3;
        assert_eq!(line(&read(r, &d, None).0), "47 of 96 exits", "A and D, D chosen last");
        d[0x0E] = 0;
        assert_eq!(line(&read(r, &d, None).0), "6 of 96 exits", "A and D, A chosen last");
    }

    #[test]
    fn mario_world_shows_the_file_being_played() {
        let save = mario_save([Some(43), None, None]);
        let (p, m) = read(release_of(MarioWorld, En), &save, None);
        assert_eq!(line(&p), "43 of 96 exits");
        // A second file is started and saved: that one changed, so that one is being played.
        let two = mario_save([Some(43), Some(5), None]);
        let (p, m) = read(release_of(MarioWorld, En), &two, Some(&m));
        assert_eq!(line(&p), "5 of 96 exits");
        // Read again unchanged: still the second.
        assert_eq!(line(&read(release_of(MarioWorld, En), &two, Some(&m)).0), "5 of 96 exits");
        // Two files and nothing to go by: nothing rather than a guess.
        assert_eq!(read(release_of(MarioWorld, En), &two, None).0, None);
    }

    #[test]
    fn a_link_to_the_past_counts_pendants_then_crystals() {
        let save = zelda_save([None, Some((0b101, 0, 6, 2)), None]);
        assert_eq!(line(&read(release_of(LinkToThePast, En), &save, None).0), "2 of 3 pendants · 6 hearts");
        let save = zelda_save([Some((0b111, 0b0010_0101, 14, 3)), None, None]);
        assert_eq!(line(&read(release_of(LinkToThePast, De), &save, None).0), "3 of 7 crystals · 14 hearts");
    }

    /// Hacks, translations and other revisions show nothing: their ROMs aren't in the list, and a save that isn't the
    /// game's own (a hack saving elsewhere, a damaged file, a value the game can't have) fails its checks.
    #[test]
    fn anything_unsure_shows_nothing() {
        assert!(release(0x1234_5678, None).is_none(), "an unknown ROM (a hack, a translation) is not read");
        assert!(release(0x0000_0000, Some(0xB19ED489)).is_some(), "a SNES file with a copier header is known by its other sum");
        let mut save = gen1_save(&[("CHARIZARD", 45)], 3, (1, 1));
        save[0x2602] ^= 1;
        assert_eq!(read(release_of(Gen1, En), &save, None).0, None, "a changed byte fails the game's checksum");
        let mut save = gen1_save(&[("CHARIZARD", 45)], 3, (1, 1));
        save[0x2F2C + 8 + 0x21] = 140;
        let sum = save[0x2598..0x3523].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        save[0x3523] = !sum;
        assert_eq!(read(release_of(Gen1, En), &save, None).0, None, "level 140 doesn't exist, even with a right checksum");
        for kind in [Gen1, GoldSilver, Crystal, Emerald, RubySapphire, FireRedLeafGreen, SuperMetroid, ZeroMission, MarioWorld, LinkToThePast, Mario64, Ocarina, PaperMario, MarioKart64, StarFox64, DiddyKongRacing, FinalFantasy6, EarthBound, LinksAwakening, MinishCap, AllStarsWorld, OracleOfAges, OracleOfSeasons, MajorasMask] {
            let r = release_of(kind, En);
            for blank in [vec![0u8; 0x20000], vec![0xFFu8; 0x20000], vec![0u8; 100]] {
                assert_eq!(read(r, &blank, None).0, None, "{kind:?}: an empty or short file");
            }
        }
        // Every list entry's ROM is told apart from every other.
        let mut crcs: Vec<u32> = RELEASES.iter().map(|r| r.crc).collect();
        crcs.sort_unstable();
        crcs.dedup();
        assert_eq!(crcs.len(), RELEASES.len());
    }

    #[test]
    fn a_japanese_name_is_left_out_not_shown_half() {
        // A Pokémon traded in from a Japanese game: kana bytes in its name.
        let mut save = gen3_save(Emerald, &[("PIKACHU", 30, false)], 4, (9, 9), 1);
        for slot in 0..2 {
            for k in 0..SECTORS {
                let base = (slot * SECTORS + k) * SECTOR;
                if u16le(&save, base + 0xFF4) == 1 {
                    save[base + 0x238 + 0x08] = 0x50; // a katakana byte
                    let sum = (0..SECTOR_DATA / 4).fold(0u32, |a, w| a.wrapping_add(u32le(&save[base..], w * 4)));
                    let sum = (sum as u16).wrapping_add((sum >> 16) as u16);
                    save[base + 0xFF6..base + 0xFF8].copy_from_slice(&sum.to_le_bytes());
                }
            }
        }
        assert_eq!(line(&read(release_of(Emerald, En), &save, None).0), "4 badges · game clock 9:09");
    }

    /// The thread reads a save the first time a game is looked at; on a Brick or an H700 that must stay a blink, not
    /// a stall of the card. The largest save (GBA flash, both copies checked) a hundred times.
    #[test]
    fn reading_the_largest_save_is_quick() {
        let save = gen3_save(Emerald, &[("BLAZIKEN", 61, false)], 6, (24, 40), 50);
        let r = release_of(Emerald, En);
        let t = crate::testutil::clock::CpuInstant::now();
        for _ in 0..100 {
            assert!(read(r, &save, None).0.is_some());
        }
        // About 0.1 ms each on the build computer; a hundred under 200 ms leaves room for a core ten times slower.
        assert!(t.elapsed() < std::time::Duration::from_millis(200), "{:?} for 100 reads", t.elapsed());
        // The DS's 512 KiB: both general blocks (62 KiB each) through their CRC, six Pokémon unscrambled.
        let team: &[(&str, u8, bool)] = &[("CYNDAQUIL", 5, false), ("A", 5, false), ("B", 5, false), ("C", 5, false), ("D", 5, false), ("E", 5, false)];
        let save = gen4_save(HeartGoldSoulSilver, [Some(((1, 1), team, (1, 0), (1, 0))), Some(((2, 1), team, (1, 0), (1, 0)))]);
        let r = release_of(HeartGoldSoulSilver, En);
        let t = crate::testutil::clock::CpuInstant::now();
        for _ in 0..100 {
            assert!(read(r, &save, None).0.is_some());
        }
        assert!(t.elapsed() < std::time::Duration::from_millis(200), "{:?} for 100 DS reads", t.elapsed());
    }

    #[test]
    fn only_files_of_the_right_size_are_read_for_their_checksum() {
        assert!(worth_checking("gba", 16 << 20, false) && worth_checking("gba", 8 << 20, false), "Pokémon, Zero Mission");
        assert!(worth_checking("gba", 32 << 20, false), "Pokémon Mystery Dungeon, Mother 3");
        assert!(!worth_checking("gba", 64 << 20, false));
        assert!(worth_checking("snes", (3 << 20) + 512, false), "Super Metroid with a copier header");
        assert!(worth_checking("snes", 4 << 20, false), "Donkey Kong Country");
        assert!(worth_checking("gbc", 2 << 20, false) && worth_checking("gb", 1 << 20, false));
        assert!(!worth_checking("nes", 1 << 20, false) && !worth_checking("psx", 1 << 20, true));
        assert!(worth_checking("gb", 77, true), "a zip says its checksum without being read");
    }
}
