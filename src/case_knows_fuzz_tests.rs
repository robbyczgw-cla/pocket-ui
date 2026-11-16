//! "The case knows" never takes pocket-ui down (the release builds abort on a panic, and the reader thread is
//! pocket-ui's own): every save reader, every ROM picture reader and the cache files, fed with what a player's card
//! can hold — the valid saves of the tests cut short at every length, with bytes changed, random bytes of every save
//! size, a save of one game read as another's, a damaged cache, a ROM that is something else. They may answer
//! nothing; they must answer.
//!
//! Seeded, so a finding is the same on every run. `CASE_KNOWS_FUZZ_ROUNDS=<n>` multiplies the rounds for a long
//! run (`scripts/crash-sweep.sh` uses 20 with overflow checks on).
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::case_knows::tests::*;
use crate::case_knows::{self, Kind::*, Lang, Memory, Progress, TeamArt, RELEASES};
use crate::case_knows_art;
use crate::case_knows_rpg_tests::*;

/// xorshift64*: the same numbers on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

fn rounds() -> usize {
    std::env::var("CASE_KNOWS_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).max(1)
}

/// Runs `f`, and keeps what panicked (at most 40 lines, one per `what`).
fn guard(found: &mut Vec<String>, what: impl Fn() -> String, f: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(f)).is_err() && found.len() < 40 {
        let w = what();
        if !found.contains(&w) {
            found.push(w);
        }
    }
}

/// One valid save of every layout the readers know, as the tests make them; the builders' own limits (an extreme value
/// they can't write, an overflow in a debug build) skip that one seed, never the run.
fn valid_saves() -> Vec<(case_knows::Kind, Vec<u8>)> {
    fn seed(kind: case_knows::Kind, make: impl Fn() -> Vec<u8>) -> Option<(case_knows::Kind, Vec<u8>)> {
        catch_unwind(AssertUnwindSafe(make)).ok().map(|s| (kind, s))
    }
    const HOUR: u32 = 3600 * 60;
    let ds_team: &[(&str, u8, bool)] = &[("TURTWIG", 14, false)];
    let g5_team: &[(&str, u8, bool)] = &[("Tepig", 9, false)];
    let seeds = vec![
        seed(Gen1, || gen1_save(&[("CHARIZARD", 45), ("PIKACHU", 30)], 0b0111_1111, (31, 12))),
        seed(GoldSilver, || gen2_save(false, &[("EGG", 5, true), ("TYPHLOSION", 52, false)], (0xFF, 0b111), (40, 7))),
        seed(Crystal, || gen2_save(true, &[("TYPHLOSION", 52, false)], (0xFF, 0xFF), (999, 59))),
        seed(Emerald, || gen3_save(Emerald, &[("BLAZIKEN", 61, false)], 6, (24, 40), 50)),
        seed(RubySapphire, || gen3_save(RubySapphire, &[("EGG", 1, true), ("BLAZIKEN", 100, false)], 0xFF, (999, 59), u32::MAX - 1)),
        seed(FireRedLeafGreen, || gen3_save(FireRedLeafGreen, &[("BLAZIKEN", 61, false)], 0xFF, (24, 40), 0)),
        seed(SuperMetroid, || metroid_save(1, (599, 100, 20, 10, 5), 0x0004 | 0x1000, 0x1000 | 0x0002, (4, 31))),
        seed(SuperMetroid, || metroid_save(0, (u16::MAX, u16::MAX, u16::MAX, u16::MAX, u16::MAX), u16::MAX, u16::MAX, (u16::MAX, u16::MAX))),
        seed(ZeroMission, || zero_mission_save(&[(1, (399, 15, 4, 2), 0b1000_0011, 0b0000_0101, 1, (2, 7))], Some(1))),
        seed(ZeroMission, || zero_mission_save(&[(0, (u16::MAX, u16::MAX, 255, 255), 255, 255, 255, (255, 255))], Some(255))),
        seed(Mario64, || mario_64_save(&[(0, 0b11, &[0x7F, 0x80 | 0b111])])),
        seed(Mario64, || mario_64_save(&[(3, 0xFF, &[0xFF; 25])])),
        seed(Ocarina, || ocarina_save(&[(1, 0b11 << 18, 5)])),
        seed(Ocarina, || ocarina_save(&[(0, u32::MAX, 255)])),
        seed(PaperMario, || paper_mario_save(&[(0, 0, 5, 1000, 1, 3), (2, 1, 9, u32::MAX, 255, 255)], Some(1))),
        seed(MarioKart64, || mario_kart_64_save([0xFF, 0b00_01_10_11, 0b11, 0])),
        seed(StarFox64, || star_fox_64_save(&[0x07, 0x05, 0x1F])),
        seed(StarFox64, || star_fox_64_save(&[0xFF; 16])),
        seed(DiddyKongRacing, || dkr_save(2, 34)),
        seed(DiddyKongRacing, || dkr_save(0, 255)),
        seed(FinalFantasy6, || ff6_save(1, ("TERRA", 32), (18, 44))),
        seed(FinalFantasy6, || ff6_save(2, ("TERRA", 255), (255, 255))),
        seed(EarthBound, || earthbound_save(1, &[1, 2, 3, 0, 0, 0], "Ness", 23)),
        seed(EarthBound, || earthbound_save(0, &[4, 3, 2, 1, 0, 0], "Ness", 255)),
        seed(LinksAwakening, || ladx_save(&[(1, 0b0001_1111, 9)])),
        seed(LinksAwakening, || ladx_save(&[(0, 0xFF, 255)])),
        seed(MinishCap, || minish_save(&[(0, 0b0101, 48), (2, 0b0101_0101, 120)], Some(2), false)),
        seed(MinishCap, || minish_save(&[(1, 0xFF, 255)], Some(255), true)),
        seed(MajorasMask, || majora_save(&[(0, 0b0011, 0x90, false)], false)),
        seed(MajorasMask, || majora_save(&[(1, 0xFF, i16::MIN, true)], true)),
        seed(OracleOfAges, || oracle_save(true, &[(1, 0b0001_1111, 36)])),
        seed(OracleOfSeasons, || oracle_save(false, &[(0, 0xFF, 255)])),
        seed(DiamondPearl, || gen4_save(DiamondPearl, [Some(((3, 1), ds_team, (0b11, 0), (4, 5))), None])),
        seed(Platinum, || gen4_save(Platinum, [Some(((3, 1), ds_team, (0xFF, 0xFF), (u16::MAX, 255))), Some(((4, 2), ds_team, (0, 0), (0, 0)))])),
        seed(HeartGoldSoulSilver, || gen4_save(HeartGoldSoulSilver, [None, Some(((u32::MAX, u32::MAX), ds_team, (0xFF, 0xFF), (999, 59)))])),
        seed(BlackWhite, || gen5_save(BlackWhite, g5_team, 1, (2, 30))),
        seed(BlackWhite2, || gen5_save(BlackWhite2, g5_team, 0xFF, (u16::MAX, 255))),
        seed(MarioWorld, || mario_save([Some(6), None, None])),
        seed(MarioWorld, || mario_save([Some(255), Some(255), Some(255)])),
        seed(LinkToThePast, || zelda_save([None, Some((0b101, 0, 6, 2)), None])),
        seed(LinkToThePast, || zelda_save([Some((0xFF, 0xFF, 255, 255)), None, Some((0, 0xFF, 0, 0))])),
        seed(FireEmblem7, || fe_save(false, &[(0, 0x16, 3, 0x40, 14 * HOUR, false), (1, 0x16, 2, 0, HOUR, false)], 0)),
        seed(FireEmblem8, || fe_save(true, &[(2, 0xFF, 0xFF, 0xFF, u32::MAX, true)], 2)),
        seed(DonkeyKong1, || dkc_save(DonkeyKong1, &[(1, 62, 0)])),
        seed(DonkeyKong2, || dkc_save(DonkeyKong2, &[(0, 255, u32::MAX)])),
        seed(DonkeyKong3, || dkc_save(DonkeyKong3, &[(2, 105, 1000)])),
        seed(ChronoTrigger, || chrono_save(&[(0, 0, "Crono", 12, (5, 7))], 0)),
        seed(ChronoTrigger, || chrono_save(&[(2, 6, "Crono", 255, (255, 255))], 255)),
        seed(GoldenSun, || golden_sun_save(false, &[(0, 0, 4, "Isaac", 23, 12 * HOUR)])),
        seed(GoldenSunLostAge, || golden_sun_save(true, &[(2, 255, u16::MAX, "Felix", 255, u32::MAX)])),
        seed(KirbyMirror, || kirby_save(&[(1, 0b0010_1101, 3)])),
        seed(KirbyMirror, || kirby_save(&[(0, 0xFF, 255)])),
        seed(MarioRpg, || mario_rpg_save(&[(0, 1), (2, 17)], 2)),
        seed(MarioRpg, || mario_rpg_save(&[(3, 255)], 255)),
        seed(Lufia2, || lufia_2_save(&[(0, 0, "Rob", 1, (0, 1, 32)), (1, 0, "Rob", 1, (0, 2, 38))], 1)),
        seed(Lufia2, || lufia_2_save(&[(3, 255, "Maxim", 127, (255, 255, 255))], 3)),
        seed(MysteryDungeonRed, || mystery_dungeon_save(295, 5, "Rob", (0, 5, 2), false)),
        seed(MysteryDungeonRed, || mystery_dungeon_save(u16::MAX, 127, "Charizard12", (u16::MAX, 255, 255), true)),
        seed(FinalFantasy6Advance, || ff6a_save(&[(0, "", 4, (0, 13, 35)), (1, "Tina", 42, (31, 5, 9))], 2)),
        seed(FinalFantasy6Advance, || ff6a_save(&[(2, "Terra", 255, (255, 255, 255))], 255)),
        seed(TacticsOgre, || tactics_ogre_save(&[(3, 2, 0, 1, "Rob", 1, (0, 26)), (9, 10, 1, 2, "Alphonse", 12, (5, 40))])),
        seed(TacticsOgre, || tactics_ogre_save(&[(15, 15, 2, u32::MAX, "Rob", 255, (255, 255))])),
        seed(BreathOfFire2, || breath_of_fire_2_save(&[(0, "Rob", 1), (2, "Ryu", 23)])),
        seed(BreathOfFire2, || breath_of_fire_2_save(&[(1, "Ryu", 255)])),
        seed(SuperstarSaga, || superstar_saga_save(&[(0, false, 1, 1, 46_800), (2, true, 11, 22, 450_000)], true)),
        seed(SuperstarSaga, || superstar_saga_save(&[(1, true, 255, 255, u32::MAX)], false)),
        seed(Mother3, || mother_3_save(&[(0, "Lucas", 1, 29_720), (1, "Lucas", 1, 49_522)], 1, &[(0, 0)])),
        seed(Mother3, || mother_3_save(&[(1, "Claus", 255, u32::MAX)], u32::MAX, &[])),
    ];
    let made: Vec<(case_knows::Kind, Vec<u8>)> = seeds.into_iter().flatten().collect();
    assert!(made.len() >= 45, "only {} of the seed saves could be made", made.len());
    made
}

/// The lengths a save is cut to: every one up to 64, then 64 steps over the rest, and the ones beside the end.
fn cuts(len: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..=64.min(len)).collect();
    v.extend((1..64).map(|i| len * i / 64));
    v.extend([len.saturating_sub(1), len + 1, len * 2]);
    v.sort_unstable();
    v.dedup();
    v
}

/// What a reader keeps between reads, as a damaged cache line can give it.
fn memory(rng: &mut Rng) -> Option<Memory> {
    match rng.below(4) {
        0 => None,
        1 => Some(Memory::default()),
        _ => Some(Memory {
            prints: (0..rng.below(9)).map(|_| if rng.below(3) == 0 { 0 } else { rng.next() as u32 }).collect(),
            current: [None, Some(rng.below(4)), Some(rng.below(1000)), Some(usize::MAX)][rng.below(4)],
        }),
    }
}

/// Everything the line, the meter and the pictures do with an answer (they run on the UI thread).
fn use_answer(p: &Option<Progress>) {
    if let Some(p) = p {
        let _ = case_knows::Progress::parts(p);
        let _ = p.meter();
    }
}

#[test]
fn save_readers_never_panic_on_damaged_cut_or_foreign_saves() {
    let mut found = Vec::new();
    let saves = valid_saves();
    let mut rng = Rng(0x5EED_CA5E_0005);
    // One release of each kind and language is enough for the layouts; the rest differ only in the CRC.
    let mut releases: Vec<&case_knows::Release> = Vec::new();
    for r in RELEASES {
        if !releases.iter().any(|o| o.kind == r.kind && o.lang == r.lang) {
            releases.push(r);
        }
    }
    for round in 0..rounds() {
        for (kind, save) in &saves {
            let own: Vec<&&case_knows::Release> = releases.iter().filter(|r| r.kind == *kind).collect();
            for r in &own {
                // Cut short at every length, with whatever a damaged cache remembers.
                for cut in cuts(save.len()) {
                    let mut s = save.clone();
                    s.resize(cut, 0xFF);
                    let before = memory(&mut rng);
                    guard(&mut found, || format!("{:?} ({:?}): a valid save cut to {cut} of {} bytes, before {before:?}", r.kind, r.lang, save.len()), || {
                        let (p, m) = case_knows::read(r, &s, before.as_ref());
                        use_answer(&p);
                        let _ = case_knows::read(r, &s, Some(&m));
                    });
                }
                // Bytes changed: single ones, and pairs that keep a byte sum (+d here, -d nearby), so a checksum of
                // added bytes still passes and the values behind it are believed.
                for i in 0..200 {
                    let mut s = save.clone();
                    let n = 1 + rng.below(8);
                    for _ in 0..n {
                        let at = rng.below(s.len());
                        let d = rng.next() as u8;
                        if rng.below(2) == 0 {
                            s[at] = d;
                        } else {
                            let other = (at + 2 * (1 + rng.below(32))).min(s.len() - 1);
                            s[at] = s[at].wrapping_add(d);
                            s[other] = s[other].wrapping_sub(d);
                        }
                    }
                    let before = memory(&mut rng);
                    guard(&mut found, || format!("{:?} ({:?}): a valid save with {n} bytes changed (round {round}, try {i}), before {before:?}", r.kind, r.lang), || {
                        let (p, m) = case_knows::read(r, &s, before.as_ref());
                        use_answer(&p);
                        let _ = case_knows::read(r, &s, Some(&m));
                    });
                }
            }
            // Every other game's reader on this save (a save renamed, a ROM that is another release).
            for r in &releases {
                let before = memory(&mut rng);
                guard(&mut found, || format!("{:?} ({:?}) reading a {kind:?} save, before {before:?}", r.kind, r.lang), || {
                    use_answer(&case_knows::read(r, save, before.as_ref()).0);
                });
            }
        }
        // Random bytes, blank flash and zeroes at every size a battery save has.
        for size in [0usize, 1, 2, 3, 7, 8, 64, 512, 513, 2048, 0x2000, 0x8000, 0x8001, 0x10000, 0x20000, 0x40000, 0x80000] {
            for fill in 0..3 {
                let s = match fill {
                    0 => rng.bytes(size),
                    1 => vec![0xFF; size],
                    _ => vec![0; size],
                };
                for r in &releases {
                    let before = memory(&mut rng);
                    guard(&mut found, || format!("{:?} ({:?}): {size} bytes of fill {fill}, before {before:?}", r.kind, r.lang), || {
                        use_answer(&case_knows::read(r, &s, before.as_ref()).0);
                    });
                }
            }
        }
    }
    assert!(found.is_empty(), "case-knows save readers panicked:\n{}", found.join("\n"));
}

#[test]
fn rom_readers_never_panic_on_any_rom_or_header() {
    let mut found = Vec::new();
    let mut rng = Rng(0xD5_0000_0005);
    for round in 0..rounds() {
        for i in 0..300 {
            let len = [0usize, 1, 3, 4, 5, 16, 0x15F, 0x160, 0x200, 4096, 65536][rng.below(11)];
            let mut b = rng.bytes(len);
            // The magic words the readers look for, at random places, so they go past their first check.
            for (magic, at) in [(&b"\x10"[..], 0usize), (b"NARC", 0), (b"RLCN", 0), (b"RGCN", 0), (b"TTLP", 0x10), (b"RAHC", 0x10), (b"BTAF", 0x10), (b"BTNF", 0x1C)] {
                if rng.below(3) == 0 && b.len() >= at + magic.len() {
                    b[at..at + magic.len()].copy_from_slice(magic);
                }
            }
            let at = rng.below(len + 2);
            guard(&mut found, || format!("ds_key/lz77/blz/zelda_unpack on {len} random bytes at {at} (round {round}, try {i})"), || {
                let _ = case_knows::ds_key(&b);
                let _ = case_knows_art::lz77(&b, at);
                let _ = case_knows_art::blz(&b);
                let _ = case_knows_art::zelda_unpack(&b, at, 0x1000);
            });
            guard(&mut found, || format!("find/find_gb/find_zelda on {len} random bytes (round {round}, try {i})"), || {
                for kind in [RubySapphire, Emerald, FireRedLeafGreen, Gen1] {
                    let _ = case_knows_art::find(&b, kind);
                }
                let _ = case_knows_art::find_gb(&b);
                let _ = case_knows_art::find_zelda(&b);
            });
        }
    }
    assert!(found.is_empty(), "case-knows ROM readers panicked:\n{}", found.join("\n"));
}

/// A DS ROM header whose file tables point at a NitroFS with the icon archive's path, and a NARC there with
/// random parts: what a hack with the same game code can hold.
fn nds_rom(rng: &mut Rng, path: &str) -> Vec<u8> {
    let mut rom = vec![0u8; 0x4000];
    let fnt_at = 0x200usize;
    // The name table: one directory per path part, each with one entry.
    let parts: Vec<&str> = path.split('/').collect();
    let mut fnt = vec![0u8; 8 * parts.len()];
    for (k, part) in parts.iter().enumerate() {
        let o = fnt.len();
        fnt[k * 8..k * 8 + 4].copy_from_slice(&(o as u32).to_le_bytes());
        let last = k + 1 == parts.len();
        fnt.push(part.len() as u8 | if last { 0 } else { 0x80 });
        fnt.extend_from_slice(part.as_bytes());
        if !last {
            fnt.extend_from_slice(&(0xF000u16 + k as u16 + 1).to_le_bytes());
        }
        fnt.push(0);
    }
    rom[fnt_at..fnt_at + fnt.len()].copy_from_slice(&fnt);
    let fat_at = 0x400usize;
    let narc_at = 0x800usize;
    let narc_len = 16 + rng.below(0x3000);
    rom[0x40..0x44].copy_from_slice(&(fnt_at as u32).to_le_bytes());
    rom[0x44..0x48].copy_from_slice(&(fnt.len() as u32).to_le_bytes());
    rom[0x48..0x4C].copy_from_slice(&(fat_at as u32).to_le_bytes());
    rom[0x4C..0x50].copy_from_slice(&8u32.to_le_bytes());
    rom[fat_at..fat_at + 4].copy_from_slice(&(narc_at as u32).to_le_bytes());
    rom[fat_at + 4..fat_at + 8].copy_from_slice(&((narc_at + narc_len) as u32).to_le_bytes());
    // The code and overlay tables: random places and sizes, small or past the end.
    for at in [0x20usize, 0x28, 0x2C, 0x50, 0x54] {
        let v = [rng.next() as u32, rng.below(0x4000) as u32, 0][rng.below(3)];
        rom[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    // The NARC: its magic and part names where the reader looks, with random sizes and counts.
    let narc = &mut rom[narc_at..narc_at + narc_len.min(0x3800)];
    for b in narc.iter_mut() {
        *b = rng.next() as u8;
    }
    narc[..4].copy_from_slice(b"NARC");
    if narc.len() >= 0x14 {
        narc[0xC..0xE].copy_from_slice(&0x10u16.to_le_bytes());
        narc[0x10..0x14].copy_from_slice(b"BTAF");
        if rng.below(2) == 0 && narc.len() >= 0x1A {
            let size = 12 + 8 * rng.below(4);
            narc[0x14..0x18].copy_from_slice(&(size as u32).to_le_bytes());
            narc[0x18..0x1A].copy_from_slice(&[rng.next() as u8, 0]);
            if narc.len() >= 0x10 + size + 8 {
                narc[0x10 + size..0x14 + size].copy_from_slice(b"BTNF");
            }
        }
    }
    rom.truncate([rom.len(), narc_at + 4, narc_at + 0xE, narc_at + 0x13, narc_at + 0x18, narc_at + rng.below(0x1000)][rng.below(6)]);
    rom
}

#[test]
fn rom_pictures_never_panic_on_a_strange_rom_or_a_damaged_cache_line() {
    let mut found = Vec::new();
    let mut rng = Rng(0xA27_0005);
    let dir = tempfile::tempdir().unwrap();
    let rom = dir.path().join("game.bin");
    let team = TeamArt { team: vec![0, 1, 25, 251, 252, 253, 412, 413, 493, 649, u16::MAX], badges: 0xFF };
    let quests = [
        Progress::Quest { pendants: 3, crystals: 0, hearts: 3, dark_world: false },
        Progress::Quest { pendants: 255, crystals: 255, hearts: 255, dark_world: true },
    ];
    for round in 0..rounds() {
        for i in 0..150 {
            let kind = [DiamondPearl, Platinum, HeartGoldSoulSilver][rng.below(3)];
            let path = match kind {
                DiamondPearl => "poketool/icongra/poke_icon.narc",
                Platinum => "poketool/icongra/pl_poke_icon.narc",
                _ => "a/0/2/0",
            };
            let bytes = if rng.below(4) == 0 {
                let n = rng.below(0x400);
                rng.bytes(n)
            } else {
                nds_rom(&mut rng, path)
            };
            std::fs::write(&rom, &bytes).unwrap();
            guard(&mut found, || format!("find_nds({kind:?}) on a {} byte DS ROM with a strange icon archive (round {round}, try {i})", bytes.len()), || {
                if let Some(p) = case_knows_art::find_nds(&rom, kind) {
                    let _ = case_knows_art::nds_art(&rom, &p, &team);
                }
                if let Some(p) = case_knows_art::snapshot_nds_places(&rom, kind, vec![0; 600]) {
                    let _ = case_knows_art::nds_art(&rom, &p, &team);
                }
            });
            // Cache lines as a damaged file can have them, with a random ROM behind.
            let hex = |rng: &mut Rng| format!("{:x}", [rng.next() as u32, rng.below(bytes.len() + 64) as u32, 0, u32::MAX][rng.below(4)]);
            let line = (0..8).map(|_| hex(&mut rng)).collect::<Vec<_>>().join(" ");
            let gb = format!("gb {} {} {:x} {} {} {}", hex(&mut rng), hex(&mut rng), rng.below(256), hex(&mut rng), hex(&mut rng), hex(&mut rng));
            let zelda = (rng.next() as u32 % (bytes.len() as u32 + 64), rng.next() as u32 % (bytes.len() as u32 + 64));
            guard(&mut found, || format!("art/gb_art/zelda_art with cache lines {line:?} {gb:?} {zelda:?} on {} random bytes", bytes.len()), || {
                if let Some(p) = case_knows_art::Places::parse(&line) {
                    let _ = case_knows_art::art(&rom, &p, &team);
                }
                if let Some(p) = case_knows_art::GbPlaces::parse(&gb) {
                    let _ = case_knows_art::gb_art(&rom, &p, &team);
                }
                for q in &quests {
                    let _ = case_knows_art::zelda_art(&rom, zelda, [0, 512, u32::MAX as u64][rng.below(3)], q);
                }
                let _ = case_knows_art::NdsPlaces::parse(&format!("nds {} {} 0123456789", hex(&mut rng), hex(&mut rng)));
            });
        }
    }
    assert!(found.is_empty(), "case-knows picture readers panicked:\n{}", found.join("\n"));
}

/// The two cache files in the data folder, damaged every way a power cut or a hand edit can: the reader starts,
/// answers for real ROMs and saves of every known size, and writes them back.
#[test]
fn a_damaged_case_knows_cache_never_panics() {
    let mut found = Vec::new();
    let mut rng = Rng(0xCAC4E_0005);
    let saves = valid_saves();
    for i in 0..60 * rounds() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("identify")).unwrap();
        // A ROM per system whose save layout is read, of a release's size, and its save.
        let mut roms = Vec::new();
        for (sys, ext, size) in [("gb", "gb", 1usize << 20), ("gba", "gba", 8 << 20), ("snes", "sfc", (1 << 20) + 512), ("n64", "z64", 8 << 20), ("nds", "nds", 0x4000)] {
            let rom = dir.path().join(format!("Game {i}.{ext}"));
            let len = if rng.below(3) == 0 { rng.below(64) } else { size };
            let f = std::fs::File::create(&rom).unwrap();
            f.set_len(len as u64).unwrap();
            drop(f);
            let (_, save) = &saves[rng.below(saves.len())];
            let mut save = save.clone();
            save.truncate(rng.below(save.len() + 1).max(save.len() * (rng.below(2))));
            std::fs::write(rom.with_extension("srm"), &save).unwrap();
            std::fs::write(rom.with_extension("sav"), &save).unwrap();
            roms.push((sys, rom));
        }
        // The cache lines: the right ROMs with random releases, slots and prints, or bytes that are no line at all.
        let mut known = String::new();
        let mut art = String::new();
        for (_, rom) in &roms {
            let len = std::fs::metadata(rom).unwrap().len();
            let mtime = std::fs::metadata(rom).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let crc = if rng.below(3) == 0 { format!("{:08X}", rng.next() as u32) } else { format!("{:08X}", RELEASES[rng.below(RELEASES.len())].crc) };
            let current = ["-".to_string(), rng.below(4).to_string(), rng.below(100_000).to_string(), usize::MAX.to_string(), "-1".to_string()][rng.below(5)].clone();
            let prints: Vec<String> = (0..rng.below(9)).map(|_| format!("{:x}", rng.next() as u32)).collect();
            known += &format!("{len}\t{mtime}\t{crc}\t{current}\t{}\t{}\n", prints.join(","), rom.display());
            let places = match rng.below(5) {
                0 => "-".to_string(),
                1 => format!("zelda {:x} {:x}", rng.next() as u32, rng.below(len as usize + 8)),
                2 => format!("gb {:x} {:x} {:x} - - {:x}", rng.below(len as usize + 8), rng.next() as u32, rng.below(256), rng.next() as u32),
                3 => format!("nds {:x} {:x} 0123", rng.next() as u32, rng.next() as u32),
                _ => (0..8).map(|_| format!("{:x}", rng.below(len as usize + 8))).collect::<Vec<_>>().join(" "),
            };
            art += &format!("{len}\t{mtime}\t{places}\t{}\n", rom.display());
        }
        if rng.below(4) == 0 {
            known = String::from_utf8_lossy(&rng.bytes(300)).into_owned();
        }
        // Where `case_knows_read` keeps them.
        std::fs::write(data.join("identify").join("case-knows.tsv"), &known).unwrap();
        std::fs::write(data.join("identify").join("case-knows-art.tsv"), &art).unwrap();
        guard(&mut found, || format!("case_knows_read with a damaged cache (try {i}):\n{known}\n{art}"), || {
            let mut reading = crate::case_knows_read::Reading::new(&data);
            for (sys, rom) in &roms {
                let _ = reading.cheap(rom);
                if let Some(p) = reading.progress(sys, rom) {
                    let _ = crate::case_knows_read::line(&p);
                    let _ = reading.art(rom, &p);
                }
                let _ = reading.art(rom, &Progress::Quest { pendants: 1, crystals: 0, hearts: 3, dark_world: false });
                let _ = reading.art(rom, &Progress::Pokemon { badges: 1, lead: None, hours: 0, minutes: 0, art: TeamArt { team: vec![1, 412], badges: 1 } });
            }
            reading.keep();
        });
    }
    let _ = Lang::En;
    assert!(found.is_empty(), "case-knows reading panicked:\n{}", found.join("\n"));
}

/// A Ruby, Sapphire, Emerald, FireRed or LeafGreen save of 64 KiB (the half-size file some emulators write for a
/// 64 KiB flash, with one copy): read, not a crash. Found by the sweep (14.11.2026): `gen3` let every file of one copy
/// and more through and then read the second copy's sectors past the end; the release build aborts on that panic, so
/// pocket-ui ended whenever the shelf asked about such a game.
#[test]
fn a_gba_pokemon_save_of_one_copy_is_read_not_a_crash() {
    for kind in [Emerald, RubySapphire, FireRedLeafGreen] {
        let r = RELEASES.iter().find(|r| r.kind == kind && r.lang == Lang::En).unwrap();
        let full = gen3_save(kind, &[("BLAZIKEN", 61, false)], 6, (24, 40), 50);
        for len in [14 * 4096, 0x10000, 0x10000 + 1, 2 * 14 * 4096 - 1] {
            let half = &full[..len];
            let p = catch_unwind(AssertUnwindSafe(|| case_knows::read(r, half, None).0));
            let p = p.unwrap_or_else(|_| panic!("{kind:?}: a save of {len} bytes panicked"));
            let line = p.as_ref().map(case_knows::Progress::parts).unwrap_or_default().join(" · ");
            assert!(line.contains("Lv. 61"), "{kind:?}, {len} bytes: the first copy's team is read: {line:?}");
        }
    }
}

/// The file being played, as a damaged cache line remembers it, past the files there are: no crash, no answer from it.
#[test]
fn a_remembered_file_past_the_last_is_forgotten_not_a_crash() {
    let r = RELEASES.iter().find(|r| r.kind == MarioWorld).unwrap();
    let save = mario_save([Some(6), Some(3), None]);
    let (_, mut m) = case_knows::read(r, &save, None);
    for current in [3, 9, usize::MAX] {
        m.current = Some(current);
        let read = catch_unwind(AssertUnwindSafe(|| case_knows::read(r, &save, Some(&m))));
        let (p, after) = read.unwrap_or_else(|_| panic!("current {current}: panicked"));
        assert_eq!((p, after.current), (None, None), "current {current}: two files, neither known to be played");
    }
}

/// A player's own `.cht` (GitHub issue #4) is whatever is on their card: cheat lists cut short and with bytes changed,
/// random bytes, a text of something else, one over the size limit. The page may explain; it must be there and must not panic.
#[test]
fn a_players_own_cheat_file_never_panics_whatever_it_holds() {
    use crate::cheats::{CheatMenu, OWN_MAX};
    let valid = b"cheats = 3\ncheat0_desc = \"Infinite &eacute;ner\"\ncheat0_code = \"7E1F8A03+7E039203\"\ncheat0_enable = false\n\
cheat1_desc = \"Coins\"\ncheat1_address = \"100\"\ncheat1_value = \"9\"\ncheat1_cheat_type = \"1\"\ncheat2_code = \"\"\n".to_vec();
    let mut found = Vec::new();
    let mut rng = Rng(0xC47_0005);
    let dir = crate::testutil::TestDir::new("fuzz-own-cht");
    let rom = dir.join("roms/gba/Game (USA).gba");
    std::fs::create_dir_all(dir.join("roms/gba")).unwrap();
    let file = dir.join("roms/gba/Game (USA).cht");
    let data = dir.join("data");
    let mut run = |body: &[u8], what: String| {
        std::fs::write(&file, body).unwrap();
        guard(&mut found, || what.clone(), || {
            if let Some(mut m) = CheatMenu::load_in(&data, "gba", &rom, &[], false) {
                let rows = m.rows();
                assert_eq!(rows.len(), m.count());
                for i in 0..m.count() + 2 {
                    m.press(i);
                }
                let _ = (m.rows(), m.enabled(), m.unsupported());
            }
            let _ = crate::cheats::active_codes_in(&data, "gba", &rom, &[]);
        });
    };
    for round in 0..rounds() {
        for cut in 0..=valid.len() {
            run(&valid[..cut], format!("a cheat file cut at {cut}"));
        }
        for _ in 0..200 {
            let mut body = valid.clone();
            for _ in 0..1 + rng.below(8) {
                let at = rng.below(body.len());
                body[at] = rng.next() as u8;
            }
            run(&body, format!("a cheat file with changed bytes (round {round})"));
        }
        for n in [0, 1, 7, 300, 5000, OWN_MAX as usize, OWN_MAX as usize + 1, OWN_MAX as usize + 4096] {
            let body = rng.bytes(n);
            run(&body, format!("{n} random bytes as a cheat file"));
        }
    }
    assert!(found.is_empty(), "panics: {found:?}");
}
