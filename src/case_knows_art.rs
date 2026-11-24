//! Pictures for "the case knows where you are" (docs/CASE-KNOWS.md, "Pictures"): the team's little party icons and
//! the row of badges, read at run time from the player's own ROM of a Pokémon game (GBA, and Gold, Silver and Crystal:
//! [`find_gb`]), and A Link to the Past's pendants and crystals. Nothing of a game is in
//! pocket-ui, in its package or in this repository: only where to look, found by numbers (lengths and checksums of
//! the game's own tables), and the pictures are made in memory from the player's file each time.
//!
//! **Where.** Ruby, Sapphire, Emerald, FireRed and LeafGreen share the table of icon palette numbers (one byte per
//! species, [`ICON_PALETTES`]); right before it lie the pointers to the icons, right after it the pointers to the
//! three icon palettes. The badges are the trainer card's: 8 badges of 16 x 16 pixels in one 128 x 16 sheet, LZ77
//! packed in Emerald and FireRed, plain in Ruby and Sapphire, found by their checksum once unpacked ([`BADGES_HOENN`],
//! [`BADGES_KANTO`]), and their palette by its own ([`BADGE_PALETTE`]). Each is checked before it is used: pointers
//! inside the ROM, the right tags, the right sums. One that isn't there leaves its row out, never shows a wrong one.
//!
//! **Cost.** Finding the places reads the ROM once (16 MiB, a second from a slow card; the reading thread, at the
//! lowest priority, never while a game runs) and they are kept with the file's size and time
//! (`identify/case-knows-art.tsv`). After that a picture is a few hundred bytes read where the places say.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::case_knows::{Kind, TeamArt};

/// The GBA's address of the ROM's first byte.
const ROM: u32 = 0x0800_0000;

/// A table's or a picture's identity: its length, the CRC-32 of its bytes, and a word at an offset to look for first.
struct Mark {
    len: usize,
    crc: u32,
    probe: (usize, u32),
}

/// The icon palette numbers of species 0 to 439 (the same 440 bytes in all five games).
const ICON_PALETTES: Mark = Mark { len: 440, crc: 0xFFE5_7B45, probe: (0, 0x0101_0100) };
/// The Hoenn badges unpacked (1024 bytes, 4 bits a pixel).
const BADGES_HOENN: Mark = Mark { len: 1024, crc: 0xBDF1_FB9E, probe: (0xC, 0xFFF0_0000) };
/// The Kanto badges unpacked.
const BADGES_KANTO: Mark = Mark { len: 1024, crc: 0xED3D_9565, probe: (0x4, 0xFF00_0000) };
/// The badges' 16 colours.
const BADGE_PALETTE: Mark = Mark { len: 32, crc: 0x1DC6_B294, probe: (0, 0x7FFF_3991) };
/// The egg's icon number.
const EGG: u16 = 412;

/// Where the pictures are in one ROM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The icon pointers (one per species), the palette numbers, the three icon palettes.
    icons: u32,
    numbers: u32,
    palettes: [u32; 3],
    /// The badge sheet and whether it is LZ77 packed; its palette.
    badges: Option<(u32, bool)>,
    badge_palette: Option<u32>,
}

impl Places {
    /// One line for the cache: `icons numbers p0 p1 p2 badges lz bpal` in hex, `-` for none.
    pub fn line(&self) -> String {
        let (b, lz) = self.badges.map_or(("-".to_string(), "-"), |(o, lz)| (format!("{o:x}"), if lz { "lz" } else { "raw" }));
        let bp = self.badge_palette.map_or("-".to_string(), |o| format!("{o:x}"));
        format!("{:x} {:x} {:x} {:x} {:x} {b} {lz} {bp}", self.icons, self.numbers, self.palettes[0], self.palettes[1], self.palettes[2])
    }

    pub fn parse(s: &str) -> Option<Places> {
        let f: Vec<&str> = s.split(' ').collect();
        let [icons, numbers, p0, p1, p2, b, lz, bp] = f[..] else { return None };
        let hex = |x: &str| u32::from_str_radix(x, 16).ok();
        let badges = hex(b).map(|o| (o, lz == "lz"));
        Some(Places { icons: hex(icons)?, numbers: hex(numbers)?, palettes: [hex(p0)?, hex(p1)?, hex(p2)?], badges, badge_palette: hex(bp) })
    }
}

/// A picture made from the ROM: 0xAARRGGBB, alpha 0 or 255.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pic {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// The pictures of one save: the team's icons, then the eight badges with whether each is won.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Art {
    pub team: Vec<Pic>,
    pub badges: Vec<(Pic, bool)>,
    /// The badges are the game's own "got" and "missing" pictures, made for a dark menu window (A Link to the Past): drawn
    /// as they are, on a dark plate, none faded.
    pub plate: bool,
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// [`u32le`] for places read from the file itself (a DS archive's own sizes and counts): None past the end.
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// Whether `p` points into a ROM of `len` bytes, `need` bytes and more.
fn inside(p: u32, len: usize, need: usize) -> bool {
    p >= ROM && ((p - ROM) as usize).saturating_add(need) <= len
}

/// The GBA BIOS's LZ77 (type 0x10) unpacked, or None for anything that isn't one whole.
pub fn lz77(rom: &[u8], at: usize) -> Option<Vec<u8>> {
    let head = rom.get(at..at + 4)?;
    if head[0] != 0x10 {
        return None;
    }
    let size = head[1] as usize | (head[2] as usize) << 8 | (head[3] as usize) << 16;
    let mut out = Vec::with_capacity(size);
    let mut p = at + 4;
    while out.len() < size {
        let flags = *rom.get(p)?;
        p += 1;
        for bit in 0..8 {
            if out.len() >= size {
                break;
            }
            if flags & (0x80 >> bit) != 0 {
                let x = (*rom.get(p)? as usize) << 8 | *rom.get(p + 1)? as usize;
                p += 2;
                let (n, d) = ((x >> 12) + 3, (x & 0xFFF) + 1);
                if d > out.len() {
                    return None;
                }
                for _ in 0..n {
                    out.push(out[out.len() - d]);
                }
            } else {
                out.push(*rom.get(p)?);
                p += 1;
            }
        }
    }
    out.truncate(size);
    Some(out)
}

/// Where `mark` is in `rom`, plain (`step`-aligned) or LZ77 packed (4-aligned): (offset, packed).
fn locate(rom: &[u8], mark: &Mark, step: usize, packed_too: bool) -> Option<(u32, bool)> {
    let (po, pw) = mark.probe;
    let mut o = 0;
    while o + mark.len <= rom.len() {
        if u32le(rom, o + po) == pw && crc32fast::hash(&rom[o..o + mark.len]) == mark.crc {
            return Some((o as u32, false));
        }
        if packed_too && o % 4 == 0 && rom[o] == 0x10 && rom[o + 1..o + 4] == [(mark.len & 0xFF) as u8, (mark.len >> 8) as u8, (mark.len >> 16) as u8] {
            if lz77(rom, o).is_some_and(|d| crc32fast::hash(&d) == mark.crc) {
                return Some((o as u32, true));
            }
        }
        o += step;
    }
    None
}

/// Whether pocket-ui reads pictures from this game's ROM at all.
pub fn has_pictures(kind: Kind) -> bool {
    matches!(kind, Kind::RubySapphire | Kind::Emerald | Kind::FireRedLeafGreen | Kind::LinkToThePast | Kind::GoldSilver | Kind::Crystal | Kind::DiamondPearl | Kind::Platinum | Kind::HeartGoldSoulSilver)
}

/// Gold, Silver and Crystal: the party menu's table of icon numbers (one byte per species 1 to 251), found by the code
/// that reads it (`ReadMonMenuIcon` in pret's pokecrystal: an egg is icon 0x1C, the others look it up).
const GB_READ_ICON: &[Option<u8>] = &[Some(0xFE), Some(0xFD), Some(0x28), Some(0x0A), Some(0x3D), Some(0x21), None, None, Some(0x5F), Some(0x16), Some(0x00), Some(0x19), Some(0x7E), Some(0xC9), Some(0x3E), Some(0x1C), Some(0xC9)];
/// The pointers to the 39 icons and their bank, found by the code that loads one (`LoadOverworldMonIcon`).
const GB_LOAD_ICON: &[Option<u8>] = &[Some(0x6F), Some(0x26), Some(0x00), Some(0x29), Some(0x11), None, None, Some(0x19), Some(0x2A), Some(0x5F), Some(0x56), Some(0x06), None, Some(0x0E), Some(0x08), Some(0xC9)];
/// The trainer card's badges, found by the code that loads their 44 tiles into the sprite tiles (`ld de, BadgeGFX;
/// ld hl, vTiles0; lb bc, BANK(BadgeGFX), 44; call`).
const GB_LOAD_BADGES: &[Option<u8>] = &[Some(0x11), None, None, Some(0x21), Some(0x00), Some(0x80), Some(0x01), Some(0x2C), None, Some(0xCD)];
/// The icon numbers' 251 bytes, the 39 icons in table order (two frames of 2 x 2 tiles, 128 bytes each), the badges'
/// 44 tiles: their checksums, the same in Gold, Silver and Crystal.
const GB_ICON_NUMBERS: u32 = 0x59FF_CC7E;
const GB_ICONS: u32 = 0x196B_1CBB;
const GB_BADGES: u32 = 0xE698_1457;
/// The party menu's sprite colours and the trainer card's badge colours (4 each, BGR555).
const GB_PARTY_PALETTE: Mark = Mark { len: 8, crc: 0x09FB_C649, probe: (0, 0x2A7F_6FFB) };
const GB_BADGE_PALETTE: Mark = Mark { len: 8, crc: 0x61EC_7915, probe: (0, 0x5ABA_7FFF) };
const GB_EGG: u16 = 0xFD;

/// Where Gold's, Silver's or Crystal's pictures are: the icon numbers, the icon pointers, the icons' bank, the badges,
/// the two palettes (file offsets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GbPlaces {
    numbers: u32,
    pointers: u32,
    bank: u8,
    badges: Option<u32>,
    badge_palette: Option<u32>,
    party_palette: u32,
}

impl GbPlaces {
    /// One line for the cache: `gb numbers pointers bank badges bpal ppal` in hex, `-` for none.
    pub fn line(&self) -> String {
        let opt = |o: Option<u32>| o.map_or("-".to_string(), |o| format!("{o:x}"));
        format!("gb {:x} {:x} {:x} {} {} {:x}", self.numbers, self.pointers, self.bank, opt(self.badges), opt(self.badge_palette), self.party_palette)
    }

    pub fn parse(s: &str) -> Option<GbPlaces> {
        let f: Vec<&str> = s.strip_prefix("gb ")?.split(' ').collect();
        let [numbers, pointers, bank, badges, bpal, ppal] = f[..] else { return None };
        let hex = |x: &str| u32::from_str_radix(x, 16).ok();
        Some(GbPlaces { numbers: hex(numbers)?, pointers: hex(pointers)?, bank: u8::try_from(hex(bank)?).ok()?, badges: hex(badges), badge_palette: hex(bpal), party_palette: hex(ppal)? })
    }
}

/// Where `code` is in `rom` (None in it: any byte).
fn code_at(rom: &[u8], code: &[Option<u8>]) -> Option<usize> {
    rom.windows(code.len()).position(|w| w.iter().zip(code).all(|(b, c)| c.is_none_or(|c| c == *b)))
}

/// The file offset of a Game Boy address `a` of the bank of file offset `at` (0x4000 to 0x7FFF: that bank's; below:
/// bank 0's).
fn gb_offset(at: usize, a: u16) -> usize {
    if a >= 0x4000 { at / 0x4000 * 0x4000 + (a as usize - 0x4000) } else { a as usize }
}

/// The places of Gold's, Silver's or Crystal's pictures, from its bytes; None when the icons aren't there as the games
/// have them (a hack that changed them, another game). The badges and their colours may be missing alone.
pub fn find_gb(rom: &[u8]) -> Option<GbPlaces> {
    let at = code_at(rom, GB_READ_ICON)?;
    let numbers = gb_offset(at, u16::from_le_bytes([rom[at + 6], rom[at + 7]]));
    if crc32fast::hash(rom.get(numbers..numbers + 251)?) != GB_ICON_NUMBERS {
        return None;
    }
    let at = code_at(rom, GB_LOAD_ICON)?;
    let pointers = gb_offset(at, u16::from_le_bytes([rom[at + 5], rom[at + 6]]));
    let bank = rom[at + 12];
    if gb_icons(rom.get(pointers..pointers + 78)?, bank, |o, n| rom.get(o..o + n).map(|b| b.to_vec())).is_none() {
        return None;
    }
    let badges = code_at(rom, GB_LOAD_BADGES).and_then(|at| {
        let o = rom[at + 8] as usize * 0x4000 + u16::from_le_bytes([rom[at + 1], rom[at + 2]]).checked_sub(0x4000)? as usize;
        (crc32fast::hash(rom.get(o..o + 704)?) == GB_BADGES).then_some(o as u32)
    });
    let badge_palette = locate(rom, &GB_BADGE_PALETTE, 1, false).map(|(o, _)| o);
    let (party_palette, _) = locate(rom, &GB_PARTY_PALETTE, 1, false)?;
    Some(GbPlaces { numbers: numbers as u32, pointers: pointers as u32, bank, badges, badge_palette, party_palette })
}

/// The 39 icons (128 bytes each, in table order) by the pointer table `pointers` in `bank`, read with `read(offset,
/// len)`; None unless they are the games' own.
fn gb_icons(pointers: &[u8], bank: u8, mut read: impl FnMut(usize, usize) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut all = Vec::with_capacity(39 * 128);
    for p in pointers.chunks(2) {
        let a = u16::from_le_bytes([p[0], p[1]]);
        if !(0x4000..0x8000 - 128).contains(&a) {
            return None;
        }
        all.extend(read(bank as usize * 0x4000 + a as usize - 0x4000, 128)?);
    }
    (crc32fast::hash(&all) == GB_ICONS).then_some(all)
}

/// A picture of 2 x 2 Game Boy tiles (2 bits a pixel, the tiles left to right, top to bottom) in `pal` (colour 0 clear).
fn gb_picture(tiles: &[u8], pal: &[u32; 4]) -> Option<Pic> {
    let mut pic = Pic { w: 16, h: 16, px: vec![0; 256] };
    for (t, at) in [(0, 0), (8, 0), (0, 8), (8, 8)].into_iter().enumerate() {
        tile_2bpp(tiles, t, pal, &mut pic, at)?;
    }
    Some(pic)
}

/// Gold's, Silver's or Crystal's pictures for `team`: each Pokémon's party menu icon in the menu's colours, and the
/// trainer card's eight Johto badges (won ones bright).
pub fn gb_art(rom: &Path, places: &GbPlaces, team: &TeamArt) -> Art {
    let Ok(mut f) = crate::background_io::open(rom) else { return Art::default() };
    let mut out = Art::default();
    let four = |raw: Vec<u8>| -> [u32; 4] { let c = colours_n(&raw, 4); [0, c[1], c[2], c[3]] };
    let Some(party) = bytes_at(&mut f, places.party_palette as u64, 8).filter(|p| crc32fast::hash(p) == GB_PARTY_PALETTE.crc).map(four) else { return out };
    let numbers = bytes_at(&mut f, places.numbers as u64, 251).filter(|n| crc32fast::hash(n) == GB_ICON_NUMBERS);
    let pointers = bytes_at(&mut f, places.pointers as u64, 78);
    let icons = pointers.and_then(|p| gb_icons(&p, places.bank, |o, n| bytes_at(&mut f, o as u64, n)));
    if let (Some(numbers), Some(icons)) = (numbers, icons) {
        let team: Option<Vec<Pic>> = team
            .team
            .iter()
            .map(|&sp| {
                let icon = match sp {
                    GB_EGG => 0x1C,
                    1..=251 => numbers[sp as usize - 1] as usize,
                    _ => return None,
                };
                gb_picture(icons.get(icon * 128..icon * 128 + 64)?, &party)
            })
            .collect();
        out.team = team.unwrap_or_default();
    }
    let sheet = places.badges.and_then(|o| bytes_at(&mut f, o as u64, 704)).filter(|b| crc32fast::hash(b) == GB_BADGES);
    let pal = places.badge_palette.and_then(|o| bytes_at(&mut f, o as u64, 8)).filter(|p| crc32fast::hash(p) == GB_BADGE_PALETTE.crc).map(four);
    if let (Some(sheet), Some(pal)) = (sheet, pal) {
        out.badges = (0..8).filter_map(|b| gb_picture(&sheet[b * 64..b * 64 + 64], &pal).map(|p| (p, team.badges & (1 << b) != 0))).collect();
        if out.badges.len() != 8 {
            out.badges.clear();
        }
    }
    out
}

/// The places of a GBA Pokémon ROM's pictures, from its bytes; None when its icon tables aren't there as the games
/// have them (a hack that moved them, another game).
pub fn find(rom: &[u8], kind: Kind) -> Option<Places> {
    if !matches!(kind, Kind::RubySapphire | Kind::Emerald | Kind::FireRedLeafGreen) {
        return None;
    }
    let (numbers, _) = locate(rom, &ICON_PALETTES, 1, false)?;
    let n = ICON_PALETTES.len;
    let icons = (numbers as usize).checked_sub(4 * n)?;
    if icons % 4 != 0 || !(0..n).all(|i| inside(u32le(rom, icons + 4 * i), rom.len(), 512)) {
        return None;
    }
    // The three palettes' table: { palette, tag 0xDAC0 + i } each.
    let table = (numbers as usize + n + 3) & !3;
    let mut palettes = [0u32; 3];
    for (i, p) in palettes.iter_mut().enumerate() {
        // The table may stand at the very end of a file cut short.
        let (ptr, tag) = (u32_at(rom, table + 8 * i)?, u32_at(rom, table + 8 * i + 4)? & 0xFFFF);
        if !inside(ptr, rom.len(), 32) || tag != 0xDAC0 + i as u32 {
            return None;
        }
        *p = ptr - ROM;
    }
    let sheet = if kind == Kind::FireRedLeafGreen { &BADGES_KANTO } else { &BADGES_HOENN };
    let badges = locate(rom, sheet, 4, true);
    let badge_palette = locate(rom, &BADGE_PALETTE, 2, false).map(|(o, _)| o);
    Some(Places { icons: icons as u32, numbers, palettes, badges, badge_palette })
}

/// A Link to the Past's pause menu pictures: the HUD's third 2-bit sheet (pack 105, packed the game's own way) by its
/// first word and its unpacked checksum, and the HUD palette by its first word and checksum.
const ZELDA_SHEET: Mark = Mark { len: 0x800, crc: 0x93D2_C58D, probe: (0, 0xC00C_FF22) };
const ZELDA_PALETTE: Mark = Mark { len: 128, crc: 0x8241_65FF, probe: (0, 0x0198_0000) };

/// A Link to the Past's packing (the game's decompressor, as snesrev/zelda3's `decomp` documents it): commands of a
/// 3-bit kind and a length, 0xFF the end. None for anything malformed or longer than `max`.
pub fn zelda_unpack(rom: &[u8], at: usize, max: usize) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let mut p = at;
    let mut next = || -> Option<u8> {
        let b = *rom.get(p)?;
        p += 1;
        Some(b)
    };
    loop {
        let b = next()?;
        if b == 0xFF {
            return Some(out);
        }
        let (cmd, len) = if b & 0xE0 != 0xE0 { (b & 0xE0, (b & 0x1F) as usize) } else { ((b << 3) & 0xE0, ((b & 3) as usize) << 8 | next()? as usize) };
        let len = len + 1;
        if out.len() + len > max {
            return None;
        }
        match cmd {
            0x00 => {
                for _ in 0..len {
                    out.push(next()?);
                }
            }
            c if c & 0x80 != 0 => {
                // A copy of earlier output; its offset is little-endian in sprite sheets.
                let from = next()? as usize | (next()? as usize) << 8;
                for i in 0..len {
                    out.push(*out.get(from + i)?);
                }
            }
            0x20 => {
                let v = next()?;
                out.extend(std::iter::repeat_n(v, len));
            }
            0x40 => {
                let (a, b) = (next()?, next()?);
                out.extend((0..len).map(|i| if i % 2 == 0 { a } else { b }));
            }
            _ => {
                let mut v = next()?;
                for _ in 0..len {
                    out.push(v);
                    v = v.wrapping_add(1);
                }
            }
        }
    }
}

/// Where A Link to the Past's pause menu pictures are in `rom` (copier header left off): the packed sheet and the
/// palette, as offsets into it. None when either isn't found as the game has it.
pub fn find_zelda(rom: &[u8]) -> Option<(u32, u32)> {
    let (po, pw) = ZELDA_SHEET.probe;
    let sheet = (0..rom.len().saturating_sub(4 + po))
        .find(|&o| u32le(rom, o + po) == pw && zelda_unpack(rom, o, 0x1000).is_some_and(|d| d.len() >= ZELDA_SHEET.len && crc32fast::hash(&d[..ZELDA_SHEET.len]) == ZELDA_SHEET.crc))?;
    let (palette, _) = locate(rom, &ZELDA_PALETTE, 2, false)?;
    Some((sheet as u32, palette))
}

/// An 8 x 8 tile of 2 bits a pixel (the SNES's planar rows) in 4 colours of `pal` (colour 0 clear) at (x0, y0) of `into`.
fn tile_2bpp(sheet: &[u8], t: usize, pal: &[u32; 4], into: &mut Pic, (x0, y0): (u32, u32)) -> Option<()> {
    let d = sheet.get(t * 16..t * 16 + 16)?;
    for y in 0..8u32 {
        let (a, b) = (d[2 * y as usize], d[2 * y as usize + 1]);
        for x in 0..8u32 {
            let c = ((a >> (7 - x)) & 1) | (((b >> (7 - x)) & 1) << 1);
            if c != 0 {
                into.px[((y0 + y) * into.w + x0 + x) as usize] = pal[c as usize];
            }
        }
    }
    Some(())
}

/// A Link to the Past's pendants (three, each in its colour, or the empty one) or, once Agahnim is beaten, its seven
/// crystals, read from the ROM at `rom` where `find_zelda` found them (`header`: the copier header's size).
pub fn zelda_art(rom: &Path, (sheet, palette): (u32, u32), header: u64, quest: &crate::case_knows::Progress) -> Option<Art> {
    let crate::case_knows::Progress::Quest { pendants, crystals, dark_world, .. } = quest else { return None };
    let mut f = crate::background_io::open(rom).ok()?;
    let len = f.metadata().ok()?.len();
    let packed = bytes_at(&mut f, header + sheet as u64, 0x1000.min(len.saturating_sub(header + sheet as u64)) as usize)?;
    let tiles = zelda_unpack(&packed, 0, 0x1000).filter(|d| d.len() >= ZELDA_SHEET.len && crc32fast::hash(&d[..ZELDA_SHEET.len]) == ZELDA_SHEET.crc)?;
    let raw = bytes_at(&mut f, header + palette as u64, 128).filter(|p| crc32fast::hash(p) == ZELDA_PALETTE.crc)?;
    let colours = colours_n(&raw, 64);
    let pal = |n: usize| [0, colours[n * 4 + 1], colours[n * 4 + 2], colours[n * 4 + 3]];
    // The pause menu's tiles (`Hud_DrawProgressIcons`): a pendant is tiles 0x2B-0x2E of the sheet in palette 1, 3 or 7,
    // the empty one 0x3B-0x3E in 4; a crystal 0x44-0x45 in 3, the empty one 0x46-0x47 in 4.
    let (got, total) = if *dark_world { (*crystals, 7u8) } else { (*pendants, 3) };
    let mut out = Art { plate: true, ..Art::default() };
    for i in 0..total {
        let won = i < got;
        let mut pic = if *dark_world { Pic { w: 16, h: 8, px: vec![0; 128] } } else { Pic { w: 16, h: 16, px: vec![0; 256] } };
        if *dark_world {
            let (t, p) = if won { (0x44, 3) } else { (0x46, 4) };
            tile_2bpp(&tiles, t, &pal(p), &mut pic, (0, 0))?;
            tile_2bpp(&tiles, t + 1, &pal(p), &mut pic, (8, 0))?;
        } else {
            let (t, p) = if won { (0x2B, [1, 3, 7][i as usize]) } else { (0x3B, 4) };
            for (k, at) in [(0, 0), (8, 0), (0, 8), (8, 8)].into_iter().enumerate() {
                tile_2bpp(&tiles, t + k, &pal(p), &mut pic, at)?;
            }
        }
        out.badges.push((pic, true));
    }
    Some(out)
}

/// `len` bytes of the file at `at`.
fn bytes_at(f: &mut (impl std::io::Read + std::io::Seek), at: u64, len: usize) -> Option<Vec<u8>> {
    let mut b = vec![0; len];
    f.seek(SeekFrom::Start(at)).ok()?;
    f.read_exact(&mut b).ok()?;
    Some(b)
}

/// 16 colours (BGR555) as 0xFFRRGGBB.
fn colours(pal: &[u8]) -> [u32; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&colours_n(pal, 16));
    out
}

/// `n` colours (BGR555) as 0xFFRRGGBB.
fn colours_n(pal: &[u8], n: usize) -> Vec<u32> {
    (0..n)
        .map(|i| {
            let v = u16::from_le_bytes([pal[2 * i], pal[2 * i + 1]]) as u32;
            let ch = |s: u32| ((v >> s) & 31) * 255 / 31;
            0xFF00_0000 | ch(0) << 16 | ch(5) << 8 | ch(10)
        })
        .collect()
}

/// A picture of `tw` x `th` tiles (8 x 8, 4 bits a pixel) from `tiles`, tile (x, y) at `index(x, y)`; colour 0 clear.
fn picture(tiles: &[u8], (tw, th): (u32, u32), index: impl Fn(u32, u32) -> usize, pal: &[u32; 16]) -> Option<Pic> {
    let (w, h) = (tw * 8, th * 8);
    let mut px = vec![0u32; (w * h) as usize];
    for ty in 0..th {
        for tx in 0..tw {
            let tile = tiles.get(index(tx, ty) * 32..index(tx, ty) * 32 + 32)?;
            for y in 0..8 {
                for x in 0..8 {
                    let b = tile[(y * 4 + x / 2) as usize];
                    let c = if x % 2 == 1 { b >> 4 } else { b & 15 };
                    if c != 0 {
                        px[((ty * 8 + y) * w + tx * 8 + x) as usize] = pal[c as usize];
                    }
                }
            }
        }
    }
    Some(Pic { w, h, px })
}

/// The pictures for `team` from the ROM at `rom`, by its `places`: whatever can be read; a row that can't is empty.
pub fn art(rom: &Path, places: &Places, team: &TeamArt) -> Art {
    let Ok(mut f) = crate::background_io::open(rom) else { return Art::default() };
    let len = f.metadata().map_or(0, |m| m.len()) as usize;
    let mut out = Art::default();
    // Each icon: its pointer, its palette's number, its first frame (4 x 4 tiles).
    let icons: Option<Vec<Pic>> = team
        .team
        .iter()
        .map(|&sp| {
            let sp = if sp > EGG { return None } else { sp as u64 };
            let ptr = u32le(&bytes_at(&mut f, places.icons as u64 + 4 * sp, 4)?, 0);
            let pal_no = *bytes_at(&mut f, places.numbers as u64 + sp, 1)?.first()? as usize;
            if !inside(ptr, len, 512) || pal_no > 2 {
                return None;
            }
            let pal = colours(&bytes_at(&mut f, places.palettes[pal_no] as u64, 32)?);
            let tiles = bytes_at(&mut f, (ptr - ROM) as u64, 512)?;
            picture(&tiles, (4, 4), |x, y| (y * 4 + x) as usize, &pal)
        })
        .collect();
    out.team = icons.unwrap_or_default();
    // The badges: the sheet (unpacked when packed) and its palette, cut into eight 2 x 2 tile pictures.
    let sheet = places.badges.and_then(|(at, packed)| {
        if packed {
            let raw = bytes_at(&mut f, at as u64, 2048.min(len.saturating_sub(at as usize)))?;
            lz77(&raw, 0)
        } else {
            bytes_at(&mut f, at as u64, 1024)
        }
    });
    let pal = places.badge_palette.and_then(|at| bytes_at(&mut f, at as u64, 32)).filter(|p| crc32fast::hash(p) == BADGE_PALETTE.crc);
    if let (Some(sheet), Some(pal)) = (sheet.filter(|s| s.len() == 1024), pal) {
        let pal = colours(&pal);
        out.badges = (0..8u32).filter_map(|b| picture(&sheet, (2, 2), |x, y| (y * 16 + 2 * b + x) as usize, &pal).map(|p| (p, team.badges & (1 << b) != 0))).collect();
        if out.badges.len() != 8 {
            out.badges.clear();
        }
    }
    out
}

/// The DS games: Diamond, Pearl, Platinum, HeartGold, SoulSilver, Black, White, Black 2, White 2.
pub fn is_ds(kind: Kind) -> bool {
    matches!(kind, Kind::DiamondPearl | Kind::Platinum | Kind::HeartGoldSoulSilver | Kind::BlackWhite | Kind::BlackWhite2)
}

/// The DS icons' palette numbers (one byte each: species 0 to 493, the egg, Manaphy's egg), the same 496 bytes in
/// Platinum, HeartGold and SoulSilver (pret's pokeplatinum and pokeheartgold have them as a table,
/// `sPokemonIconPaletteIndex`, `sPokemonPalNoBySpeciesAndForm`).
const DS_ICON_PALETTES: Mark = Mark { len: 496, crc: 0x5C7E_2B6D, probe: (0, 0x0101_0100) };
/// The icon archive's first icon: before it the palettes and the sprite tables (`icon_00000_NCGR` is file 7).
const DS_FIRST_ICON: usize = 7;
/// Bigger file tables or code than this are no DS game's (a broken header).
const DS_MAX_PART: usize = 8 << 20;

/// Where a DS Pokémon ROM's icons are: the icon archive (NARC) by its file offset and size, and the icons' palette
/// numbers as read from the game's code (one digit each; the code may be packed, so they are kept, not its place).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NdsPlaces {
    narc: (u32, u32),
    numbers: Vec<u8>,
}

impl NdsPlaces {
    /// One line for the cache: `nds <narc offset> <narc size> <palette numbers as digits>`.
    pub fn line(&self) -> String {
        let digits: String = self.numbers.iter().map(|n| (b'0' + n) as char).collect();
        format!("nds {:x} {:x} {digits}", self.narc.0, self.narc.1)
    }

    pub fn parse(s: &str) -> Option<NdsPlaces> {
        let f: Vec<&str> = s.strip_prefix("nds ")?.split(' ').collect();
        let [at, len, digits] = f[..] else { return None };
        let numbers: Vec<u8> = digits.bytes().map(|b| b.wrapping_sub(b'0')).collect();
        // Checked again as found: a changed cache line never gives wrong colours.
        (crc32fast::hash(&numbers) == DS_ICON_PALETTES.crc).then_some(())?;
        Some(NdsPlaces { narc: (u32::from_str_radix(at, 16).ok()?, u32::from_str_radix(len, 16).ok()?), numbers })
    }
}

/// The DS's backward LZ, as its SDK packs code (the last 8 bytes say how much is packed and how much it grows):
/// unpacked from the end toward the start. None for anything that isn't one whole.
pub fn blz(b: &[u8]) -> Option<Vec<u8>> {
    let n = b.len();
    let (packed, more) = (u32le(b, n.checked_sub(8)?), u32le(b, n - 4) as usize);
    let (whole, head) = ((packed & 0xFF_FFFF) as usize, (packed >> 24) as usize);
    if more == 0 || more > DS_MAX_PART || head < 8 || whole > n || head > whole {
        return None;
    }
    let end = n - whole;
    let mut out = b.to_vec();
    out.resize(n + more, 0);
    let (mut src, mut dst) = (n - head, n + more);
    while src > end && dst > end {
        src -= 1;
        let mut flags = b[src];
        for _ in 0..8 {
            if src <= end || dst <= end {
                break;
            }
            if flags & 0x80 != 0 {
                src = src.checked_sub(2).filter(|s| *s >= end)?;
                let x = u16::from_le_bytes([b[src], b[src + 1]]) as usize;
                let (len, back) = ((x >> 12) + 3, (x & 0xFFF) + 3);
                for _ in 0..len {
                    dst = dst.checked_sub(1).filter(|d| *d >= end)?;
                    out[dst] = *out.get(dst + back)?;
                }
            } else {
                src -= 1;
                dst -= 1;
                out[dst] = b[src];
            }
            flags <<= 1;
        }
    }
    (dst == end).then_some(out)
}

/// A DS ROM's file system (NitroFS): the file id of `path` by the name table, then its (offset, size) by the
/// allocation table.
fn nitro_file(f: &mut (impl std::io::Read + std::io::Seek), header: &[u8], path: &str) -> Option<(u32, u32)> {
    let (fnt_at, fnt_len, fat_at, fat_len) = (u32le(header, 0x40), u32le(header, 0x44) as usize, u32le(header, 0x48), u32le(header, 0x4C) as usize);
    if fnt_len > DS_MAX_PART || fat_len > DS_MAX_PART {
        return None;
    }
    let fnt = bytes_at(f, fnt_at as u64, fnt_len)?;
    let mut dir = 0usize;
    let parts: Vec<&str> = path.split('/').collect();
    let mut id = None;
    for (k, part) in parts.iter().enumerate() {
        let last = k + 1 == parts.len();
        let entry = fnt.get(dir * 8..dir * 8 + 8)?;
        let (mut o, mut file) = (u32le(entry, 0) as usize, u16::from_le_bytes([entry[4], entry[5]]));
        let mut next = None;
        loop {
            let l = *fnt.get(o)? as usize;
            o += 1;
            if l == 0 {
                break;
            }
            let name = fnt.get(o..o + (l & 0x7F))?;
            o += l & 0x7F;
            if l & 0x80 != 0 {
                let sub = u16::from_le_bytes([*fnt.get(o)?, *fnt.get(o + 1)?]);
                o += 2;
                if !last && name == part.as_bytes() {
                    next = Some((sub & 0xFFF) as usize);
                    break;
                }
            } else {
                if last && name == part.as_bytes() {
                    id = Some(file as usize);
                    break;
                }
                file += 1;
            }
        }
        if last {
            break;
        }
        dir = next?;
    }
    let id = id?;
    if 8 * id + 8 > fat_len {
        return None;
    }
    let e = bytes_at(f, fat_at as u64 + 8 * id as u64, 8)?;
    let (start, end) = (u32le(&e, 0), u32le(&e, 4));
    (end > start).then_some((start, end - start))
}

/// The files of a NARC archive (offset, size within it), from its first `head` bytes.
fn narc_files(head: &[u8]) -> Option<Vec<(usize, usize)>> {
    // Every size and count here is the file's own: a damaged archive or a hack's must read as none, never past the end.
    if head.get(..4)? != b"NARC" {
        return None;
    }
    let o = u16::from_le_bytes([*head.get(0xC)?, *head.get(0xD)?]) as usize;
    if head.get(o..o + 4)? != b"BTAF" {
        return None;
    }
    let (size, count) = (u32_at(head, o + 4)? as usize, u16::from_le_bytes([*head.get(o + 8)?, *head.get(o + 9)?]) as usize);
    let btnf = o.checked_add(size)?;
    if head.get(btnf..btnf.checked_add(4)?)? != b"BTNF" {
        return None;
    }
    let gmif = btnf.checked_add(u32_at(head, btnf + 4)? as usize)?;
    if head.get(gmif..gmif.checked_add(4)?)? != b"GMIF" {
        return None;
    }
    (0..count)
        .map(|i| {
            let (s, e) = (u32_at(head, o + 12 + 8 * i)? as usize, u32_at(head, o + 16 + 8 * i)? as usize);
            (e >= s).then_some((gmif + 8 + s, e - s))
        })
        .collect()
}

/// Where `mark` is in `code`: its first word, then its checksum.
fn find_in(code: &[u8], mark: &Mark) -> Option<usize> {
    let (po, pw) = mark.probe;
    (0..code.len().saturating_sub(mark.len)).find(|&o| u32le(code, o + po) == pw && crc32fast::hash(&code[o..o + mark.len]) == mark.crc)
}

/// The icon palette numbers in a DS ROM's code: the main program (unpacked when its module parameters say it is
/// packed), else each overlay (unpacked when the overlay table says so).
fn ds_palette_numbers(f: &mut (impl std::io::Read + std::io::Seek), header: &[u8], mark: &Mark) -> Option<Vec<u8>> {
    let (arm9_at, ram, arm9_len) = (u32le(header, 0x20), u32le(header, 0x28), u32le(header, 0x2C) as usize);
    let look = |code: &[u8]| find_in(code, mark).map(|o| code[o..o + mark.len].to_vec());
    if arm9_len <= DS_MAX_PART {
        let arm9 = bytes_at(f, arm9_at as u64, arm9_len)?;
        if let Some(n) = look(&arm9) {
            return Some(n);
        }
        // The module parameters (`0xDEC00621 0x2106C0DE` at their 0x1C), their 0x14: where the packed code ends.
        let magic = [0x21, 0x06, 0xC0, 0xDE, 0xDE, 0xC0, 0x06, 0x21];
        if let Some(p) = arm9.windows(8).position(|w| w == magic).and_then(|m| m.checked_sub(0x1C)) {
            let end = u32le(&arm9, p + 0x14).wrapping_sub(ram) as usize;
            if let Some(n) = arm9.get(..end).filter(|_| end > 0).and_then(blz).and_then(|c| look(&c)) {
                return Some(n);
            }
        }
    }
    let (table_at, table_len, fat_at) = (u32le(header, 0x50), u32le(header, 0x54) as usize, u32le(header, 0x48));
    if table_len > 64 << 10 {
        return None;
    }
    let table = bytes_at(f, table_at as u64, table_len)?;
    for e in table.chunks_exact(32) {
        let (file, flags) = (u32le(e, 0x18), u32le(e, 0x1C));
        let Some(fe) = bytes_at(f, fat_at as u64 + 8 * file as u64, 8) else { continue };
        let (start, end) = (u32le(&fe, 0), u32le(&fe, 4));
        if end <= start || (end - start) as usize > DS_MAX_PART {
            continue;
        }
        let Some(code) = bytes_at(f, start as u64, (end - start) as usize) else { continue };
        let code = if flags >> 24 & 1 != 0 { code.get(..(flags & 0xFF_FFFF) as usize).and_then(blz) } else { Some(code) };
        if let Some(n) = code.and_then(|c| look(&c)) {
            return Some(n);
        }
    }
    None
}

/// The places of a DS Pokémon ROM's icons (Diamond, Pearl, Platinum, HeartGold, SoulSilver), read from the file at
/// `rom` part by part (its header, file tables, code and the icon archive's table, a few MiB of 64 to 128): None when
/// they aren't there as the games have them (a hack that changed them, Black and White: not read).
pub fn find_nds(rom: &Path, kind: Kind) -> Option<NdsPlaces> {
    find_nds_by(rom, kind, &DS_ICON_PALETTES)
}

/// The icon archive's path in each game's file system.
fn ds_icon_archive(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::DiamondPearl => "poketool/icongra/poke_icon.narc",
        Kind::Platinum => "poketool/icongra/pl_poke_icon.narc",
        Kind::HeartGoldSoulSilver => "a/0/2/0",
        _ => return None,
    })
}

fn find_nds_by(rom: &Path, kind: Kind, mark: &Mark) -> Option<NdsPlaces> {
    let path = ds_icon_archive(kind)?;
    let mut f = crate::background_io::open(rom).ok()?;
    let header = bytes_at(&mut f, 0, 0x200)?;
    let narc = nitro_file(&mut f, &header, path)?;
    let head = bytes_at(&mut f, narc.0 as u64, (narc.1 as usize).min(64 << 10))?;
    let files = narc_files(&head)?;
    if files.len() < DS_FIRST_ICON + mark.len || files.iter().any(|(s, l)| s + l > narc.1 as usize) || head.get(files[0].0..files[0].0 + 4)? != b"RLCN" {
        return None;
    }
    let numbers = ds_palette_numbers(&mut f, &header, mark)?;
    numbers.iter().all(|n| *n <= 9).then_some(NdsPlaces { narc, numbers })
}

/// Snapshots only (`POCKETUI_SNAPSHOT_DS_PALETTES`): the icon archive of `rom` with palette numbers given by the
/// developer, for a ROM whose code doesn't hold the games' table. Never used when reading a player's game.
pub fn snapshot_nds_places(rom: &Path, kind: Kind, numbers: Vec<u8>) -> Option<NdsPlaces> {
    let path = ds_icon_archive(kind)?;
    let mut f = crate::background_io::open(rom).ok()?;
    let header = bytes_at(&mut f, 0, 0x200)?;
    let narc = nitro_file(&mut f, &header, path)?;
    (numbers.len() >= DS_ICON_PALETTES.len).then_some(NdsPlaces { narc, numbers })
}

/// One file of the icon archive.
fn narc_file(f: &mut (impl std::io::Read + std::io::Seek), narc: (u32, u32), files: &[(usize, usize)], i: usize) -> Option<Vec<u8>> {
    let (s, l) = *files.get(i)?;
    (s + l <= narc.1 as usize && l <= 64 << 10).then_some(())?;
    bytes_at(f, narc.0 as u64 + s as u64, l)
}

/// A DS icon's first frame (32 x 32 of its 32 x 64, 4 bits a pixel) from its NCGR, in palette `n` of the NCLR.
fn ds_icon(ncgr: &[u8], nclr: &[u8], n: usize) -> Option<Pic> {
    // NCLR: "RLCN", its "TTLP" part at 0x10 with the colours' place (from the part's 8th byte) at 0x1C.
    if nclr.get(..4)? != b"RLCN" || nclr.get(0x10..0x14)? != b"TTLP" {
        return None;
    }
    let colours_at = 0x18usize.checked_add(u32_at(nclr, 0x24)? as usize)?;
    let pal = colours(nclr.get(colours_at + n * 32..colours_at + n * 32 + 32)?);
    // NCGR: "RGCN", its "RAHC" part at 0x10: 4 bits a pixel (3), the tiles' size at 0x28, their place (from the
    // part's 8th byte) at 0x2C.
    if ncgr.get(..4)? != b"RGCN" || ncgr.get(0x10..0x14)? != b"RAHC" || u32_at(ncgr, 0x1C)? != 3 {
        return None;
    }
    let (size, at) = (u32_at(ncgr, 0x28)? as usize, 0x18usize.checked_add(u32_at(ncgr, 0x2C)? as usize)?);
    let tiles = ncgr.get(at..at + size.min(0x400))?;
    picture(tiles, (4, 4), |x, y| (y * 4 + x) as usize, &pal)
}

/// The DS team's icons from the ROM at `rom`, by its `places`: whatever can be read; none when one can't (a whole row
/// or none). The DS games' badges are not drawn: the line counts them.
pub fn nds_art(rom: &Path, places: &NdsPlaces, team: &TeamArt) -> Art {
    let Ok(mut f) = crate::background_io::open(rom) else { return Art::default() };
    let Some(head) = bytes_at(&mut f, places.narc.0 as u64, (places.narc.1 as usize).min(64 << 10)) else { return Art::default() };
    let Some(files) = narc_files(&head) else { return Art::default() };
    let Some(nclr) = narc_file(&mut f, places.narc, &files, 0) else { return Art::default() };
    let icons: Option<Vec<Pic>> = team
        .team
        .iter()
        .map(|&sp| {
            let n = *places.numbers.get(sp as usize)? as usize;
            let ncgr = narc_file(&mut f, places.narc, &files, DS_FIRST_ICON + sp as usize)?;
            ds_icon(&ncgr, &nclr, n)
        })
        .collect();
    Art { team: icons.unwrap_or_default(), ..Art::default() }
}

/// Paints `pic` at (x, y), each pixel `k` x `k`; `dim` (0 = as is) mixes it toward `toward` (a badge not won yet).
pub fn paint(c: &mut crate::gfx::Canvas, pic: &Pic, (x, y): (i32, i32), k: i32, dim: u8, toward: u32) {
    for py in 0..pic.h as i32 {
        for px in 0..pic.w as i32 {
            let p = pic.px[(py * pic.w as i32 + px) as usize];
            if p >> 24 == 0 {
                continue;
            }
            let rgb = if dim == 0 { p & 0x00FF_FFFF } else { crate::gfx::mix(p & 0x00FF_FFFF, toward, dim as u32) };
            c.rect(x + px * k, y + py * k, k, k, rgb);
        }
    }
}

/// How big the pictures are drawn: their own pixels times a whole number, so they stay crisp, about as tall as one and
/// a half lines of the body text beside them (2 at the normal text size, more with Settings → Text size).
pub fn scale() -> i32 {
    ((crate::type_scale::px(28) as i32 + 7) / 14).clamp(1, 4)
}

/// The pictures and the measure under the line, from (x, y) (their top) at most `w` wide: the team's icons standing on
/// one line with the badges after them (won ones bright, the others faint toward `paper`), wrapping to a second row
/// when there isn't room; or a slim bar, or a row of marks. Returns the height used (0 for nothing).
pub fn paint_strip(c: &mut crate::gfx::Canvas, art: Option<&Art>, meter: Option<crate::case_knows::Meter>, (x, y, w): (i32, i32, i32), k: i32, (ink, soft, paper): (u32, u32, u32)) -> i32 {
    use crate::case_knows::Meter;
    let mut h = 0;
    if let Some(a) = art {
        let gap = 4 * k;
        let boxes: Vec<(i32, i32, i32, i32)> = a.team.iter().map(bounds).collect();
        let row_h = boxes.iter().map(|b| b.3).max().unwrap_or(0) * k;
        let team_w: i32 = boxes.iter().map(|b| b.2 * k + gap).sum();
        let badges_w = if a.badges.is_empty() { 0 } else { 8 * 16 * k + 7 * 2 * k };
        let mut cx = x;
        for (pic, b) in a.team.iter().zip(&boxes) {
            // Each stands on the row's foot, without the room the icon has around it.
            let crop = Pic { w: b.2 as u32, h: b.3 as u32, px: (0..b.3).flat_map(|yy| (0..b.2).map(move |xx| (xx, yy))).map(|(xx, yy)| pic.px[((b.1 + yy) * pic.w as i32 + b.0 + xx) as usize]).collect() };
            paint(c, &crop, (cx, y + row_h - b.3 * k), k, 0, 0);
            cx += b.2 * k + gap;
        }
        let badges_w = if a.plate { a.badges.len() as i32 * 18 * k - 2 * k } else { badges_w };
        let inset = if a.plate { (4 * k, 3 * k) } else { (0, 0) };
        let one_row = team_w + 8 * k + badges_w <= w;
        let (bx, by) = if a.team.is_empty() { (x + inset.0, y + inset.1) } else if one_row { (cx + 4 * k, y + row_h - 16 * k) } else { (x, y + row_h + 6 * k) };
        let badge_h = a.badges.iter().map(|(p, _)| p.h as i32).max().unwrap_or(0) * k;
        if a.plate && !a.badges.is_empty() {
            // The game's menu window: its pictures are drawn for a dark ground (a white chain, grey empty ones).
            c.round_rect(bx - inset.0, by - inset.1, badges_w + 2 * inset.0, badge_h + 2 * inset.1, 3 * k, 0x0010_1018);
        }
        for (i, (pic, won)) in a.badges.iter().enumerate() {
            paint(c, pic, (bx + i as i32 * 18 * k, by), k, if *won || a.plate { 0 } else { 190 }, paper);
        }
        h = if a.badges.is_empty() { row_h } else { (by + badge_h + inset.1 - y).max(row_h) };
    }
    match meter {
        Some(Meter::Share { done, of }) if of > 0 => {
            let bw = w.min(220 * k);
            let bh = 5 * k;
            c.round_rect(x, y + h, bw, bh, bh / 2, crate::gfx::mix(paper, soft, 110));
            let fill = (bw as i64 * done.min(of) as i64 / of as i64) as i32;
            if fill > 0 {
                c.round_rect(x, y + h, fill.max(bh), bh, bh / 2, ink);
            }
            h += bh;
        }
        Some(Meter::Marks { done, of }) => {
            let (m, gap) = (9 * k, 5 * k);
            for i in 0..of as i32 {
                let mx = x + i * (m + gap);
                if i < done as i32 {
                    c.round_rect(mx, y + h, m, m, 2 * k, ink);
                } else {
                    c.round_rect(mx, y + h, m, m, 2 * k, crate::gfx::mix(paper, soft, 110));
                    c.round_rect(mx + k, y + h + k, m - 2 * k, m - 2 * k, k, paper);
                }
            }
            h += m;
        }
        _ => {}
    }
    h
}

/// The smallest box around a picture's drawn pixels: (x, y, w, h); icons have room around the Pokémon.
pub fn bounds(pic: &Pic) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (pic.w as i32, pic.h as i32, -1, -1);
    for y in 0..pic.h as i32 {
        for x in 0..pic.w as i32 {
            if pic.px[(y * pic.w as i32 + x) as usize] >> 24 != 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x1 < 0 { (0, 0, 0, 0) } else { (x0, y0, x1 - x0 + 1, y1 - y0 + 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Synthetic bytes, never game assets: solve only the four-byte CRC suffix
    /// so the actual format guards and file readers are reached by the rule.
    fn crc_suffix(mut bytes: Vec<u8>, crc: u32) -> Vec<u8> {
        let at = bytes.len() - 4; bytes[at..].fill(0);
        let base = crc32fast::hash(&bytes);
        let mut basis = [(0u32, 0u32); 32];
        for bit in 0..32 {
            bytes[at + bit / 8] ^= 1 << (bit % 8);
            let (mut value, mut mask) = (crc32fast::hash(&bytes) ^ base, 1u32 << bit);
            bytes[at + bit / 8] ^= 1 << (bit % 8);
            for pivot in (0..32).rev() {
                if value & (1 << pivot) == 0 { continue; }
                if basis[pivot].0 == 0 { basis[pivot] = (value, mask); break; }
                value ^= basis[pivot].0; mask ^= basis[pivot].1;
            }
        }
        let (mut value, mut mask) = (crc ^ base, 0u32);
        for pivot in (0..32).rev() {
            if value & (1 << pivot) != 0 { value ^= basis[pivot].0; mask ^= basis[pivot].1; }
        }
        assert_eq!(value, 0); bytes[at..].copy_from_slice(&mask.to_le_bytes());
        assert_eq!(crc32fast::hash(&bytes), crc); bytes
    }

    #[test]
    fn file_source_rule_case_rom_art() {
        let d = crate::testutil::TestDir::new("case-art-file-rule");
        let (ds, mark, _) = ds_rom(&d, true);
        let ds_seed = std::fs::read(&ds).unwrap();
        let ds_places = find_nds_by(&ds, Kind::HeartGoldSoulSilver, &mark).unwrap();
        let team = TeamArt { team: vec![4], badges: 3 };
        for (label, bytes) in crate::file_faults::faults(&ds_seed) {
            let _ = std::fs::remove_file(&ds);
            if let Some(bytes) = bytes { std::fs::write(&ds, bytes).unwrap(); }
            let _ = find_nds_by(&ds, Kind::HeartGoldSoulSilver, &mark);
            let art = nds_art(&ds, &ds_places, &team);
            if label == "valid" { assert_eq!(art.team.len(), 1); assert_ne!(art.team[0].px[0], 0); }
        }
        let gb = d.join("synthetic.gb"); let mut seed = vec![0x55; 0x6000];
        for (at, code) in [(0x100, GB_READ_ICON), (0x200, GB_LOAD_ICON)] {
            let code: Vec<u8> = code.iter().map(|b| b.unwrap_or(0)).collect();
            seed[at..at + code.len()].copy_from_slice(&code);
        }
        seed[0x106..0x108].copy_from_slice(&0x400u16.to_le_bytes());
        seed[0x205..0x207].copy_from_slice(&0x600u16.to_le_bytes()); seed[0x20c] = 1;
        seed[0x400..0x400 + 251].copy_from_slice(&crc_suffix(vec![1; 251], GB_ICON_NUMBERS));
        for i in 0..39 { seed[0x600 + i * 2..0x602 + i * 2].copy_from_slice(&(0x4000u16 + i as u16 * 128).to_le_bytes()); }
        seed[0x4000..0x4000 + 39 * 128].copy_from_slice(&crc_suffix(vec![0x55; 39 * 128], GB_ICONS));
        let mut palette = vec![0; 8]; palette[..4].copy_from_slice(&GB_PARTY_PALETTE.probe.1.to_le_bytes());
        seed[0x800..0x808].copy_from_slice(&crc_suffix(palette, GB_PARTY_PALETTE.crc));
        let places = find_gb(&seed).expect("synthetic GB passes real discovery guards");
        for (label, bytes) in crate::file_faults::faults(&seed) {
            let _ = std::fs::remove_file(&gb);
            if let Some(bytes) = bytes { let _ = find_gb(&bytes); std::fs::write(&gb, bytes).unwrap(); }
            let art = gb_art(&gb, &places, &team);
            if label == "valid" { assert_eq!(art.team.len(), 1); assert!(art.team[0].px.iter().any(|p| *p != 0)); }
        }
        let zelda = d.join("synthetic.sfc");
        let mut sheet = vec![0x55; ZELDA_SHEET.len]; sheet[..3].fill(0xff); sheet[3] = 0xc0;
        let sheet = crc_suffix(sheet, ZELDA_SHEET.crc);
        // The actual discovery probe is the compressed stream's first word.
        let mut packed = vec![0x22, 0xff, 0x0c]; packed.extend(&sheet[3..16]);
        for chunk in sheet[16..].chunks(32) { packed.push((chunk.len()-1) as u8); packed.extend(chunk); }
        packed.push(0xff); packed.resize(0x1000, 0);
        let mut palette = vec![0x55; 128]; palette[..4].copy_from_slice(&ZELDA_PALETTE.probe.1.to_le_bytes());
        packed.extend(crc_suffix(palette, ZELDA_PALETTE.crc));
        let places = find_zelda(&packed).expect("synthetic Zelda passes real discovery guards");
        let quest = crate::case_knows::Progress::Quest { pendants: 2, crystals: 5, hearts: 8, dark_world: false };
        for (label, bytes) in crate::file_faults::faults(&packed) {
            let _ = std::fs::remove_file(&zelda);
            if let Some(bytes) = bytes { let _ = find_zelda(&bytes); std::fs::write(&zelda, bytes).unwrap(); }
            let art = zelda_art(&zelda, places, 0, &quest);
            if label == "valid" { let art = art.unwrap(); assert_eq!(art.badges.len(), 3); assert!(art.badges[0].0.px.iter().any(|p| *p != 0)); }
        }
    }

    /// A made-up ROM laid out as the games are (no picture of any game in it): icon pointers, the palette numbers with
    /// the games' checksum replaced by the test's own, palettes, a packed badge sheet.
    #[test]
    fn lz77_unpacks_what_it_should_and_refuses_the_rest() {
        // "ABCABCABC": 3 literals, then a copy of 6 from 3 back.
        let packed = [0x10, 9, 0, 0, 0b0001_0000, b'A', b'B', b'C', 0x30, 0x02];
        assert_eq!(lz77(&packed, 0).as_deref(), Some(&b"ABCABCABC"[..]));
        assert_eq!(lz77(&[0x10, 9, 0, 0, 0b1000_0000, 0x30, 0x05], 0), None, "a copy from before the start");
        assert_eq!(lz77(&[0x10, 9, 0, 0, 0], 0), None, "cut short");
        assert_eq!(lz77(&[0x11, 9, 0, 0], 0), None, "another packing");
    }

    #[test]
    fn zelda_unpacks_its_four_kinds_and_refuses_the_rest() {
        // 3 literals "abc", 3 times "x", "ab" again from offset 0, 4 counting up from 7, the end.
        let packed = [0x02, b'a', b'b', b'c', 0x22, b'x', 0x81, 0x00, 0x00, 0x63, 7, 0xFF];
        let want: Vec<u8> = b"abcxxxab".iter().copied().chain([7, 8, 9, 10]).collect();
        assert_eq!(zelda_unpack(&packed, 0, 64), Some(want));
        assert_eq!(zelda_unpack(&[0x81, 0x05, 0x00, 0xFF], 0, 64), None, "a copy from beyond what is there");
        assert_eq!(zelda_unpack(&[0x1F, 1, 2], 0, 64), None, "cut short");
        assert_eq!(zelda_unpack(&[0x1F; 40], 0, 8), None, "longer than allowed");
    }

    #[test]
    fn places_go_through_the_cache_line_unchanged() {
        let p = Places { icons: 0x57BCA8, numbers: 0x57C388, palettes: [1, 2, 3], badges: Some((0x56F5CC, true)), badge_palette: Some(0x56F50C) };
        assert_eq!(Places::parse(&p.line()), Some(p.clone()));
        let none = Places { badges: None, badge_palette: None, ..p };
        assert_eq!(Places::parse(&none.line()), Some(none));
        assert_eq!(Places::parse("garbage"), None);
    }

    #[test]
    fn a_rom_without_the_tables_has_no_places() {
        assert_eq!(find(&vec![0u8; 1 << 20], Kind::Emerald), None);
        assert_eq!(find(&vec![0u8; 1 << 20], Kind::SuperMetroid), None, "only the GBA Pokémon games");
    }

    #[test]
    fn gold_places_go_through_the_cache_line_and_need_the_games_tables() {
        let p = GbPlaces { numbers: 0x8E975, pointers: 0x8EA70, bank: 0x23, badges: Some(0x2622F), badge_palette: None, party_palette: 0xBAC6 };
        assert_eq!(GbPlaces::parse(&p.line()), Some(p));
        assert_eq!(GbPlaces::parse(&Places { icons: 1, numbers: 2, palettes: [3, 4, 5], badges: None, badge_palette: None }.line()), None, "a GBA line");
        // The code that reads the icon numbers is found, but they aren't the games' own: no places, no pictures.
        let mut rom = vec![0u8; 0x10000];
        let code: Vec<u8> = GB_READ_ICON.iter().map(|b| b.unwrap_or(0x50)).collect();
        rom[0x4100..0x4100 + code.len()].copy_from_slice(&code);
        assert_eq!(code_at(&rom, GB_READ_ICON), Some(0x4100));
        assert_eq!(gb_offset(0x4100, 0x5050), 0x5050);
        assert_eq!(gb_offset(0x8E964, 0x6975), 0x8E975, "bank 0x23's address");
        assert_eq!(find_gb(&rom), None);
        assert_eq!(find_gb(&vec![0u8; 1 << 21]), None);
    }

    #[test]
    fn a_game_boy_picture_is_four_tiles_left_to_right_top_to_bottom() {
        // Tile 1 (top right): its first pixel colour 3; tile 2 (bottom left): its first pixel colour 1.
        let mut tiles = vec![0u8; 64];
        tiles[16] = 0x80;
        tiles[17] = 0x80;
        tiles[32] = 0x80;
        let p = gb_picture(&tiles, &[0, 0xFF00_0001, 0xFF00_0002, 0xFF00_0003]).unwrap();
        assert_eq!((p.px[8], p.px[8 * 16], p.px[0]), (0xFF00_0003, 0xFF00_0001, 0));
    }

    /// The DS's backward LZ: "xyz" stays as it is, then (read from the end) 3 letters and a copy of 18 from 3 on.
    #[test]
    fn blz_unpacks_from_the_end_and_refuses_the_rest() {
        let mut b = b"xyz".to_vec();
        b.extend([0x00, 0xF0, b'A', b'B', b'C', 0b0001_0000]);
        b.extend((14u32 | 8 << 24).to_le_bytes());
        b.extend(7u32.to_le_bytes());
        let want: Vec<u8> = b"xyz".iter().copied().chain(b"ABC".iter().copied().cycle().take(21)).collect();
        assert_eq!(blz(&b), Some(want));
        let mut short = b.clone();
        short[b.len() - 4] = 9;
        assert_eq!(blz(&short), None, "says it grows by more than it does");
        assert_eq!(blz(&[0u8; 8]), None, "nothing packed");
        assert_eq!(blz(&[1, 2, 3]), None, "too short");
    }

    /// A made-up DS ROM laid out as HeartGold's (no picture or table of any game in it): a header, a name and an
    /// allocation table with `a/0/2/0`, an icon archive (palettes, six other files, 496 icons), and code holding a
    /// table of palette numbers (made up, found by its own checksum).
    fn ds_rom(dir: &Path, with_table: bool) -> (std::path::PathBuf, Mark, Vec<u8>) {
        let numbers: Vec<u8> = (0..496).map(|i| if i < 4 { [0, 1, 1, 1][i] } else { (i * 7 % 3) as u8 }).collect();
        let mark = Mark { len: 496, crc: crc32fast::hash(&numbers), probe: (0, 0x0101_0100) };
        // The palettes: palette n's colour 1 is (n + 1) * 8 in red.
        let mut nclr = b"RLCN".to_vec();
        nclr.resize(0x10, 0);
        nclr.extend(b"TTLP");
        nclr.resize(0x24, 0);
        nclr.extend(0x10u32.to_le_bytes());
        nclr.resize(0x28 + 16 * 32, 0);
        for n in 0..16 {
            nclr[0x28 + n * 32 + 2..0x28 + n * 32 + 4].copy_from_slice(&(((n + 1) * 2) as u16).to_le_bytes());
        }
        // One icon for all: its first pixel colour 1.
        let mut ncgr = b"RGCN".to_vec();
        ncgr.resize(0x10, 0);
        ncgr.extend(b"RAHC");
        ncgr.resize(0x1C, 0);
        ncgr.extend(3u32.to_le_bytes());
        ncgr.resize(0x28, 0);
        ncgr.extend(0x400u32.to_le_bytes());
        ncgr.extend(0x18u32.to_le_bytes());
        ncgr.resize(0x30 + 0x400, 0);
        ncgr[0x30] = 0x01;
        let count = 7 + 496;
        let mut data = nclr.clone();
        let icon_at = data.len();
        data.extend(&ncgr);
        let mut entries = Vec::new();
        for i in 0..count {
            let (s, l) = if i == 0 { (0, nclr.len()) } else if i < 7 { (0, 4) } else { (icon_at, ncgr.len()) };
            entries.extend((s as u32).to_le_bytes());
            entries.extend(((s + l) as u32).to_le_bytes());
        }
        let mut narc = b"NARC".to_vec();
        narc.extend([0xFE, 0xFF, 0, 1, 0, 0, 0, 0, 0x10, 0, 3, 0]);
        narc.extend(b"BTAF");
        narc.extend(((12 + entries.len()) as u32).to_le_bytes());
        narc.extend((count as u32).to_le_bytes());
        narc.extend(&entries);
        narc.extend(b"BTNF");
        narc.extend(16u32.to_le_bytes());
        narc.extend([4, 0, 0, 0, 0, 0, 1, 0]);
        narc.extend(b"GMIF");
        narc.extend(((8 + data.len()) as u32).to_le_bytes());
        narc.extend(&data);
        // The names: root > a > 0 > 2 > file 0.
        let mut fnt = Vec::new();
        let subs: [&[u8]; 4] = [&[0x81, b'a', 0x01, 0xF0, 0], &[0x81, b'0', 0x02, 0xF0, 0], &[0x81, b'2', 0x03, 0xF0, 0], &[0x01, b'0', 0]];
        let mut at = 32;
        for (i, s) in subs.iter().enumerate() {
            fnt.extend((at as u32).to_le_bytes());
            fnt.extend(0u16.to_le_bytes());
            fnt.extend((if i == 0 { 4u16 } else { 0xF000 + i as u16 - 1 }).to_le_bytes());
            at += s.len();
        }
        for s in subs {
            fnt.extend(s);
        }
        let mut arm9 = vec![0x55u8; 1024];
        if with_table {
            arm9.extend(&numbers);
        }
        arm9.extend([0x55u8; 64]);
        let (arm9_at, fnt_at) = (0x200, 0x200 + arm9.len());
        let fat_at = fnt_at + fnt.len();
        let narc_at = (fat_at + 8 + 0x1FF) & !0x1FF;
        let mut rom = vec![0u8; 0x200];
        for (o, v) in [(0x20, arm9_at), (0x28, 0x0200_0000), (0x2C, arm9.len()), (0x40, fnt_at), (0x44, fnt.len()), (0x48, fat_at), (0x4C, 8)] {
            rom[o..o + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }
        rom.extend(&arm9);
        rom.extend(&fnt);
        rom.extend((narc_at as u32).to_le_bytes());
        rom.extend(((narc_at + narc.len()) as u32).to_le_bytes());
        rom.resize(narc_at, 0);
        rom.extend(&narc);
        let path = dir.join(if with_table { "hg.nds" } else { "hack.nds" });
        std::fs::write(&path, &rom).unwrap();
        (path, mark, numbers)
    }

    #[test]
    fn a_ds_rom_gives_its_icons_from_its_own_files() {
        let d = crate::testutil::TestDir::new("case-knows-nds-art");
        let (rom, mark, numbers) = ds_rom(&d, true);
        let places = find_nds_by(&rom, Kind::HeartGoldSoulSilver, &mark).expect("places");
        assert_eq!(places.numbers, numbers);
        assert_eq!(find_nds_by(&rom, Kind::Platinum, &mark), None, "Platinum's archive isn't in this ROM");
        assert_eq!(find_nds_by(&rom, Kind::BlackWhite, &mark), None, "Black and White: the line alone");
        // Species 4 has palette number 4 * 7 % 3 = 1, the egg (494) 494 * 7 % 3 = 2: colour 1 of those palettes.
        let art = nds_art(&rom, &places, &TeamArt { team: vec![4, crate::case_knows::DS_EGG], badges: 0 });
        assert_eq!(art.team.len(), 2);
        let red = |n: u32| 0xFF00_0000 | ((n + 1) * 2 * 255 / 31) << 16;
        assert_eq!((art.team[0].w, art.team[0].h, art.team[0].px[0], art.team[1].px[0]), (32, 32, red(1), red(2)));
        assert!(art.badges.is_empty());
        // A species past the icons: no row rather than a wrong picture.
        assert!(nds_art(&rom, &places, &TeamArt { team: vec![4, 600], badges: 0 }).team.is_empty());
        // Through the cache line (the real checksum guards it: a made-up table doesn't pass).
        assert_eq!(NdsPlaces::parse(&places.line()), None);
        assert_eq!(NdsPlaces::parse("nds 1 2 3"), None);
        // A ROM whose code doesn't have the games' table (a hack that changed the icons' colours): no places.
        let (hack, mark, _) = ds_rom(&d, false);
        assert_eq!(find_nds_by(&hack, Kind::HeartGoldSoulSilver, &mark), None);
        assert_eq!(find_nds(&std::path::PathBuf::from("/nonexistent.nds"), Kind::Platinum), None);
    }

    #[test]
    fn a_picture_is_its_tiles_in_its_palette_with_colour_0_clear() {
        let mut pal = [0u32; 16];
        pal[1] = 0xFF11_2233;
        // One tile: the first pixel colour 1, the rest 0.
        let mut tile = vec![0u8; 32];
        tile[0] = 0x01;
        let p = picture(&tile, (1, 1), |_, _| 0, &pal).unwrap();
        assert_eq!((p.w, p.h, p.px[0], p.px[1]), (8, 8, 0xFF11_2233, 0));
        assert_eq!(bounds(&p), (0, 0, 1, 1));
    }

    /// With the player's own ROMs on this computer (`CASE_KNOWS_ROMS=<folder>`, never in the repository): the places are
    /// found and the pictures made. Skipped without them.
    #[test]
    #[ignore = "needs the player's own ROMs: CASE_KNOWS_ROMS=<folder> cargo test -- --ignored case_knows_art"]
    fn finds_the_pictures_in_real_roms() {
        let Ok(dir) = std::env::var("CASE_KNOWS_ROMS") else { return };
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let rom = std::fs::read(e.path()).unwrap();
            let headless = if rom.len() % 1024 == 512 { &rom[512..] } else { &rom[..] };
            let Some(r) = crate::case_knows::release(crc32fast::hash(&rom), Some(crc32fast::hash(headless))).filter(|r| has_pictures(r.kind)) else { continue };
            if r.kind == Kind::LinkToThePast {
                let t = std::time::Instant::now();
                let places = find_zelda(headless).unwrap_or_else(|| panic!("{}: no pause menu pictures", r.name));
                let found = t.elapsed();
                let header = (rom.len() - headless.len()) as u64;
                for (quest, n) in [(crate::case_knows::Progress::Quest { pendants: 2, crystals: 0, hearts: 9, dark_world: false }, 3), (crate::case_knows::Progress::Quest { pendants: 3, crystals: 5, hearts: 15, dark_world: true }, 7)] {
                    let a = zelda_art(&e.path(), places, header, &quest).unwrap_or_else(|| panic!("{}: no pictures", r.name));
                    assert_eq!(a.badges.len(), n);
                }
                println!("{}: zelda {places:x?} in {found:?}", r.name);
                continue;
            }
            if matches!(r.kind, Kind::GoldSilver | Kind::Crystal) {
                let t = std::time::Instant::now();
                let places = find_gb(&rom).unwrap_or_else(|| panic!("{}: no places", r.name));
                let found = t.elapsed();
                let a = gb_art(&e.path(), &places, &TeamArt { team: vec![157, 25, GB_EGG], badges: 0b101 });
                assert_eq!(a.team.len(), 3, "{}", r.name);
                assert_eq!(a.badges.iter().filter(|b| b.1).count(), 2, "{}", r.name);
                println!("{}: {} in {found:?}", r.name, places.line());
                continue;
            }
            let t = std::time::Instant::now();
            let places = find(&rom, r.kind).unwrap_or_else(|| panic!("{}: no places", r.name));
            let found = t.elapsed();
            let a = art(&e.path(), &places, &TeamArt { team: vec![6, 25, 280, EGG], badges: 0b101 });
            assert_eq!(a.team.len(), 4, "{}", r.name);
            assert_eq!(a.badges.iter().filter(|b| b.1).count(), 2, "{}", r.name);
            println!("{}: {} in {found:?}", r.name, places.line());
        }
    }
}
