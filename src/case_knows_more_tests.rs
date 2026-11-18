//! "The case knows where you are" for the games added after 0.5 (Banjo-Kazooie, …): saves made here byte by byte as
//! each game writes them, and saves the real games wrote in a headless emulator, given here by their non-zero bytes
//! (made for these tests, no one's own save). With more such saves on this computer, `CASE_KNOWS_SAVES=<folder>` reads
//! them too (`real_saves_read_as_the_games_show_them`).
use crate::case_knows::{rare_sums, read, Kind, Kind::*, Memory, Progress, Release, RELEASES};

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn release_of(kind: Kind) -> &'static Release {
    RELEASES.iter().find(|r| r.kind == kind && r.name.contains("USA")).unwrap()
}

fn line(p: &Option<Progress>) -> String {
    p.as_ref().map_or("(nothing)".into(), |p| p.parts().join(" · "))
}

/// `len` bytes, zero but for `spans` (offset, hex bytes).
fn bytes_of(len: usize, spans: &[(usize, &str)]) -> Vec<u8> {
    let mut b = vec![0u8; len];
    for &(at, hex) in spans {
        for (i, pair) in hex.as_bytes().chunks(2).enumerate() {
            b[at + i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
    }
    b
}

#[test]
fn rares_sum_matches_the_games() {
    // bryc/rare-n64-chksm's vectors: Banjo-Kazooie's full file, Banjo-Tooie's.
    let bk = bytes_of(116, &[(0, "1102ffffffffffffffffffffffff1efffffffffffffffffff7fdfffb6fd9ffff000064c993264c99326404c71053084e0ace0fd732620ec22bde234012fa031dfbffffff3fffffffff7ffebf3da6f2675effdfffe7ffffebffbfd6f2fd61f917000000000863c8641402000fffff00001fff0000")]);
    let (a, b) = rare_sums(&bk);
    assert_eq!(a ^ b, 0x5970_ECA5);
    // 28 zero bytes, Banjo-Kazooie's empty global block (the game writes this sum at the EEPROM's end).
    let (a, b) = rare_sums(&[0u8; 28]);
    assert_eq!(a ^ b, 0x32C9_A1E6);
}

/// Banjo-Kazooie's EEPROM (512 bytes): `files` as (file 0 to 3, game 1 to 3, the Jiggies, the best notes of the nine
/// note worlds, the seconds of the eleven places); the other files empty as the game leaves them (no 0x11, its sum).
pub(crate) fn banjo_save(files: &[(usize, u8, &[usize], [u8; 9], [u16; 11])]) -> Vec<u8> {
    let mut s = vec![0u8; 512];
    let seal = |f: &mut [u8]| {
        let (a, b) = rare_sums(&f[..0x74]);
        f[0x74..0x78].copy_from_slice(&(a ^ b).to_be_bytes());
    };
    for i in 0..4 {
        let f = &mut s[i * 0x78..(i + 1) * 0x78];
        f[1] = i as u8 + 1;
        seal(f);
    }
    for &(i, game, jiggies, notes, seconds) in files {
        let f = &mut s[i * 0x78..(i + 1) * 0x78];
        f.fill(0);
        f[0] = 0x11;
        f[1] = game;
        for &n in jiggies {
            f[2 + (n - 1) / 8] |= 1 << (n & 7);
        }
        let field = notes.iter().fold(0u64, |v, n| v << 7 | *n as u64);
        f[0x22..0x2A].copy_from_slice(&field.to_be_bytes());
        for (p, t) in seconds.iter().enumerate() {
            f[0x2A + 2 * p..0x2C + 2 * p].copy_from_slice(&t.to_be_bytes());
        }
        f[0x40] = 0x80;
        seal(f);
    }
    let (a, b) = rare_sums(&[0u8; 0x1C]);
    s[0x1FC..].copy_from_slice(&(a ^ b).to_be_bytes());
    s
}

/// Saves the game itself wrote (mupen64plus, USA Rev 1; "SAVE AND QUIT" from the pause menu), as their non-zero bytes.
/// The game select then said "GAME 1: TIME 1:02:15, 5 JIGSAWS, 123 NOTES." and "GAME 1: TIME 5:17:57, 100 JIGSAWS,
/// 900 NOTES."
fn banjo_written() -> [Vec<u8>; 2] {
    [
        bytes_of(
            512,
            &[(1, "02"), (116, "344603d20003"), (236, "9e192567"), (274, "3292"), (282, "0e8b"), (303, "0c"), (356, "5feb8f2911010e18"), (394, "3292"), (402, "0e8b"), (423, "0c"), (476, "5feb8f29"), (508, "32c9a1e6")],
        ),
        bytes_of(
            512,
            &[
                (1, "03"),
                (116, "9e1925670002"),
                (236, "344603d2"),
                (248, "ffffffffffff1e"),
                (274, "64c993264c993264465703e8"),
                (303, "46"),
                (356, "7793a6d21101ffffffffffffffffffffffff1e"),
                (394, "64c993264c993264465703e8"),
                (423, "46"),
                (476, "7793a6d2"),
                (508, "32c9a1e6"),
            ],
        ),
    ]
}

#[test]
fn banjo_kazooie_reads_the_saves_the_game_wrote() {
    let r = release_of(BanjoKazooie);
    let [first, second] = banjo_written();
    // The game moved game 1 to file 3 and wiped file 2's first 8 bytes (its 0x11): only file 3 counts.
    assert_eq!(line(&read(r, &first, None).0), "5 Jiggies · 123 notes · game clock 1:02");
    assert_eq!(line(&read(r, &second, None).0), "100 Jiggies · 900 notes · game clock 5:17");
    // In mupen64plus's .srm the EEPROM comes first.
    let mut srm = first.clone();
    srm.resize(0x48800, 0);
    assert_eq!(line(&read(r, &srm, None).0), "5 Jiggies · 123 notes · game clock 1:02");
    assert_eq!(read(r, &first, None).0.unwrap().meter(), Some(crate::case_knows::Meter::Share { done: 5, of: 100 }));
}

#[test]
fn banjo_kazooie_counts_as_its_game_select_does() {
    let r = release_of(BanjoKazooie);
    // Jiggy 8 lives in byte 0 bit 0, Jiggy 9 in byte 1 bit 1 (the game's own numbering); 101 and on aren't counted.
    let notes = [100, 7, 0, 0, 0, 0, 0, 0, 1];
    let s = banjo_save(&[(2, 3, &[8, 9, 100], notes, [3600, 0, 0, 0, 0, 0, 0, 0, 0, 0, 61])]);
    assert_eq!(line(&read(r, &s, None).0), "3 Jiggies · 108 notes · game clock 1:01");
    let s = banjo_save(&[(0, 1, &[1], [1, 0, 0, 0, 0, 0, 0, 0, 0], [59; 11])]);
    assert_eq!(line(&read(r, &s, None).0), "1 Jiggy · 1 note · game clock 0:10");
}

#[test]
fn banjo_kazooie_follows_the_game_that_changed_not_the_file_it_moved_to() {
    let r = release_of(BanjoKazooie);
    let zero = [0u16; 11];
    let one = banjo_save(&[(0, 1, &[1, 2], [0; 9], zero), (1, 2, &[1, 2, 3, 4], [0; 9], zero)]);
    // Two games and nothing remembered: no guess.
    let (p, m) = read(r, &one, None);
    assert_eq!(p, None);
    // Game 2 saved: it moved from file 1 to the spare file 3.
    let two = banjo_save(&[(0, 1, &[1, 2], [0; 9], zero), (3, 2, &[1, 2, 3, 4, 5], [0; 9], zero)]);
    let (p, m) = read(r, &two, Some(&m));
    assert_eq!(line(&p), "5 Jiggies · 0 notes · game clock 0:00");
    // Game 1 only moved to file 1 (its bytes the same): still game 2, the one played.
    let three = banjo_save(&[(1, 1, &[1, 2], [0; 9], zero), (3, 2, &[1, 2, 3, 4, 5], [0; 9], zero)]);
    assert_eq!(line(&read(r, &three, Some(&m)).0), "5 Jiggies · 0 notes · game clock 0:00");
    // The same game in two files (a copy the game didn't finish wiping): the first counts, as in the game select.
    let twice = banjo_save(&[(0, 1, &[1], [0; 9], zero), (2, 1, &[1, 2, 3], [0; 9], zero)]);
    assert_eq!(line(&read(r, &twice, None).0), "1 Jiggy · 0 notes · game clock 0:00");
}

#[test]
fn banjo_kazooie_believes_only_what_the_game_would_load() {
    let r = release_of(BanjoKazooie);
    let full = [100u8; 9];
    let good = banjo_save(&[(1, 1, &[1, 2, 3], full, [0; 11])]);
    assert_eq!(line(&read(r, &good, None).0), "3 Jiggies · 900 notes · game clock 0:00");
    assert_eq!(read(r, &banjo_save(&[]), None).0, None, "three empty games");
    assert_eq!(read(r, &vec![0u8; 512], None).0, None);
    assert_eq!(read(r, &vec![0xFFu8; 512], None).0, None);
    assert_eq!(read(r, &good[..511], None).0, None, "cut short");
    let mut broken = good.clone();
    broken[0x78 + 0x30] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "a changed byte fails the sum");
    for game in [0u8, 4, 0xFF] {
        assert_eq!(read(r, &banjo_save(&[(1, game, &[1], full, [0; 11])]), None).0, None, "game {game}");
    }
    // A world has 100 notes: 101 is no file the game wrote.
    let mut too_many = full;
    too_many[4] = 101;
    assert_eq!(read(r, &banjo_save(&[(1, 1, &[1], too_many, [0; 11])]), None).0, None);
    // Another game's EEPROM (Super Mario 64's 512 bytes) reads as nothing.
    assert_eq!(read(r, &crate::case_knows::tests::mario_64_save(&[(0, 0b11, &[0x7F])]), None).0, None);
}

/// Metroid Fusion's 32 KiB: `files` as (file 0 to 2, max energy, max missiles, max power bombs, clock h:m); erased
/// flash (0xFF) elsewhere; the file chosen last in the 0x7000 record, as the file select writes it.
pub(crate) fn fusion_save(files: &[(usize, u16, u16, u8, (u8, u8))], last: Option<u8>) -> Vec<u8> {
    let mut s = vec![0xFFu8; 0x8000];
    for &(i, energy, missiles, power_bombs, (h, m)) in files {
        let at = 0x200 + 2 * i * 0x1200;
        let f = &mut s[at..at + 0x1200];
        f.fill(0);
        f[..16].copy_from_slice(b"MetroidEpisode4A");
        f[0x18] = 1;
        put16(f, 0xC4, energy);
        put16(f, 0xC6, energy);
        put16(f, 0xC8, missiles);
        put16(f, 0xCA, missiles);
        f[0xCC] = power_bombs;
        f[0xCD] = power_bombs;
        f[0xE4] = h;
        f[0xE5] = m;
        f[0xF4..0xFC].copy_from_slice(b"SAVE_END");
        fusion_seal(f);
    }
    if let Some(last) = last {
        let r = &mut s[0x7000..0x7040];
        r[..39].copy_from_slice(&bytes_of(39, &[(0, "1e000000000053454c454354534156454e554d42455200454e44444154415f53454c5f53415645")]));
        r[22] = last;
        // As for a file: the 16-bit words, the sum and its complement among them, add up to the sum.
        let rest = (0..32).fold(0u16, |a, w| a.wrapping_add(u16::from_le_bytes([r[w * 2], r[w * 2 + 1]])));
        put16(r, 2, rest.wrapping_sub(1));
        put16(r, 4, !rest.wrapping_sub(1));
    }
    s
}

/// A Fusion file's sum at 16 and its complement at 20: all its 32-bit words, those two among them, add up to the sum,
/// so the sum is the other words' total less one.
fn fusion_seal(f: &mut [u8]) {
    put32(f, 16, 0);
    put32(f, 20, 0);
    let rest = (0..f.len() / 4).fold(0u32, |a, w| a.wrapping_add(u32::from_le_bytes(f[w * 4..w * 4 + 4].try_into().unwrap())));
    let sum = rest.wrapping_sub(1);
    put32(f, 16, sum);
    put32(f, 20, !sum);
}

/// File A as Metroid Fusion wrote it (mGBA, USA; its own save routine called from the pause screen's debug page with
/// 399 energy, 25 missiles, 14 power bombs and the clock at 2:07): its non-zero bytes, then the file select's record.
/// The file select then showed "A 99 ▮▮▮ 02:07 MAIN DECK".
fn fusion_written() -> Vec<u8> {
    let mut s = vec![0xFFu8; 0x8000];
    let mut f = bytes_of(
        0x1200,
        &[
            (0, "4d6574726f6964457069736f64653441ff64c198009b3e67000100000100060800000100100d0200010000ff80"),
            (49, "08"),
            (64, "80000008800000088000000880008003"),
            (103, "01"),
            (114, "20"),
            (118, "5e01bf09"),
            (129, "070200e2ff82ff1e"),
            (176, "b5000630"),
            (192, "b50006308f018f01190019000e0e0101"),
            (228, "02071e1f2a001e0001020000534156455f454e44"),
            (822, "1f"),
        ],
    );
    f[1664..].fill(0xFF);
    s[0x200..0x1400].copy_from_slice(&f);
    s[0x7000..0x7027].copy_from_slice(&bytes_of(39, &[(0, "1e00eba3145c53454c454354534156454e554d42455200454e44444154415f53454c5f53415645")]));
    s
}

#[test]
fn metroid_fusion_reads_the_save_the_game_wrote() {
    let r = release_of(MetroidFusion);
    let s = fusion_written();
    // Tanks: 3 of energy, (25 - 10) / 5 = 3 of missiles, (14 - 10) / 2 = 2 of power bombs.
    assert_eq!(line(&read(r, &s, None).0), "8% items · game clock 2:07");
    // The copy is used when the file fails its sum, as the game does.
    let mut broken = s.clone();
    broken[0x200 + 0x300] ^= 1;
    assert_eq!(read(r, &broken, None).0, None);
    broken[0x1400..0x2600].copy_from_slice(&s[0x200..0x1400]);
    assert_eq!(line(&read(r, &broken, None).0), "8% items · game clock 2:07");
}

#[test]
fn metroid_fusion_counts_tanks_as_its_ending_does_in_the_file_chosen_last() {
    let r = release_of(MetroidFusion);
    let s = fusion_save(&[(0, 99, 10, 10, (0, 0)), (1, 1299, 100, 20, (12, 34))], Some(1));
    // 12 + 18 + 5 tanks.
    assert_eq!(line(&read(r, &s, None).0), "35% items · game clock 12:34");
    let s = fusion_save(&[(0, 99, 10, 10, (0, 0)), (1, 1299, 100, 20, (12, 34))], Some(0));
    assert_eq!(line(&read(r, &s, None).0), "0% items · game clock 0:00");
    // A full file: the game stops at 100.
    let s = fusion_save(&[(2, 2099, 255, 255, (99, 59))], Some(2));
    assert_eq!(line(&read(r, &s, None).0), "100% items · game clock 99:59");
}

#[test]
fn metroid_fusion_believes_only_what_the_game_would_load() {
    let r = release_of(MetroidFusion);
    // A file the game made at a new start and never saved: no equipment, no line.
    assert_eq!(read(r, &fusion_save(&[(0, 0, 0, 0, (0, 0))], Some(0)), None).0, None);
    assert_eq!(read(r, &fusion_save(&[], None), None).0, None);
    assert_eq!(read(r, &vec![0u8; 0x8000], None).0, None);
    assert_eq!(read(r, &fusion_written()[..0x7000], None).0, None, "cut short");
    let mut odd = fusion_save(&[(0, 399, 25, 14, (2, 60))], Some(0));
    assert_eq!(read(r, &odd, None).0, None, "minute 60");
    odd[0x200] = b'Z';
    assert_eq!(read(r, &odd, None).0, None);
    // Zero Mission's save is no Fusion save.
    assert_eq!(read(r, &crate::case_knows::tests::zero_mission_save(&[(0, (399, 15, 4, 2), 0, 0, 1, (2, 7))], Some(0)), None).0, None);
}

/// GoldenEye 007's EEPROM (512 bytes): `saves` as (save 0 to 4, folder, turn, best times as ((difficulty, mission),
/// seconds), 007 open); the other saves free (bit 7), as the game leaves them.
pub(crate) fn goldeneye_save(saves: &[(usize, u8, u8, &[((usize, usize), u16)], bool)]) -> Vec<u8> {
    let mut s = vec![0u8; 512];
    let seal = |f: &mut [u8]| {
        let (a, b) = rare_sums(&f[8..]);
        f[..4].copy_from_slice(&a.to_be_bytes());
        f[4..8].copy_from_slice(&b.to_be_bytes());
    };
    for i in 0..5 {
        let f = &mut s[0x20 + i * 0x60..0x20 + (i + 1) * 0x60];
        f[8] = 0x80;
        seal(f);
    }
    for &(i, folder, turn, times, open_007) in saves {
        let f = &mut s[0x20 + i * 0x60..0x20 + (i + 1) * 0x60];
        f.fill(0);
        f[8] = folder | turn << 3;
        f[9] = open_007 as u8;
        f[10] = 0x80;
        f[11] = 0x80;
        for &((difficulty, mission), seconds) in times {
            let at = (difficulty * 20 + mission) * 10;
            for k in 0..10 {
                if seconds >> (9 - k) & 1 != 0 {
                    f[0x12 + (at + k) / 8] |= 0x80 >> ((at + k) % 8);
                }
            }
        }
        seal(f);
    }
    s
}

/// GoldenEye 007's EEPROM after the game copied our folder 1 (Agent: Dam, Facility, Runway; Secret Agent: Surface) to
/// folder 2 in its own folder select (mupen64plus, USA): our save 0, the game's copy in save 4 (folder 1, turn 1, its
/// own sums), the two empty folders the game made itself. Its folder select then showed "Secret Agent / Mission 2.i"
/// on both folders.
fn goldeneye_written() -> Vec<u8> {
    bytes_of(
        512,
        &[
            (0, "892438338752784842"),
            (32, "770017d286ebe55600008080"),
            (50, "1f4c8168"),
            (78, "012c"),
            (128, "dba8ae518b55d2508000ffff003a"),
            (224, "19d16cf856aead8b4200ffff003a"),
            (320, "fb85b6866c5eaab86300ffff003a"),
            (416, "33651f3d3ecea44809008080"),
            (434, "1f4c8168"),
            (462, "012c"),
        ],
    )
}

#[test]
fn goldeneye_reads_the_folder_the_game_wrote() {
    let r = release_of(GoldenEye);
    let written = goldeneye_written();
    // Two folders with the same progress and nothing remembered: no guess.
    assert_eq!(read(r, &written, None).0, None);
    // Before the copy only folder 1 had progress; then the game wrote folder 2: that is the one played.
    let mut before = written.clone();
    before[0x20 + 4 * 0x60..0x20 + 5 * 0x60].fill(0);
    let (p, m) = read(r, &before, None);
    assert_eq!(line(&p), "Mission 2.i: Surface · Secret Agent");
    let (p, m) = read(r, &written, Some(&m));
    assert_eq!(line(&p), "Mission 2.i: Surface · Secret Agent");
    assert_eq!(m.current, Some(1));
}

#[test]
fn goldeneye_names_the_furthest_mission_at_the_hardest_difficulty() {
    let r = release_of(GoldenEye);
    let agent: &[((usize, usize), u16)] = &[((0, 0), 125), ((0, 13), 300)];
    assert_eq!(line(&read(r, &goldeneye_save(&[(0, 2, 0, agent, false)]), None).0), "Mission 6.v: Train · Agent");
    // A harder difficulty wins over a further mission.
    let mixed: &[((usize, usize), u16)] = &[((0, 19), 900), ((2, 0), 1023)];
    assert_eq!(line(&read(r, &goldeneye_save(&[(1, 0, 0, mixed, false)]), None).0), "Mission 1.i: Dam · 00 Agent");
    // 007 open by its bit, or by every mission at 00 Agent: the folder says "007" alone.
    assert_eq!(line(&read(r, &goldeneye_save(&[(0, 1, 0, agent, true)]), None).0), "007");
    let all: Vec<((usize, usize), u16)> = (0..20).map(|m| ((2, m), 60)).collect();
    assert_eq!(line(&read(r, &goldeneye_save(&[(3, 3, 0, &all, false)]), None).0), "007");
    // A folder's two saves: the one whose turn follows the other's (3 then 0).
    let older: &[((usize, usize), u16)] = &[((0, 0), 100)];
    let newer: &[((usize, usize), u16)] = &[((0, 0), 100), ((0, 1), 100)];
    let s = goldeneye_save(&[(0, 0, 3, older, false), (2, 0, 0, newer, false)]);
    assert_eq!(line(&read(r, &s, None).0), "Mission 1.ii: Facility · Agent");
    let s = goldeneye_save(&[(0, 0, 0, older, false), (2, 0, 3, newer, false)]);
    assert_eq!(line(&read(r, &s, None).0), "Mission 1.i: Dam · Agent");
}

#[test]
fn goldeneye_believes_only_what_the_game_would_load() {
    let r = release_of(GoldenEye);
    let some: &[((usize, usize), u16)] = &[((1, 4), 200)];
    let good = goldeneye_save(&[(2, 1, 0, some, false)]);
    assert_eq!(line(&read(r, &good, None).0), "Mission 2.ii: Bunker · Secret Agent");
    // Folders without a mission done (as a new game's) say nothing.
    assert_eq!(read(r, &goldeneye_save(&[(0, 0, 0, &[], false)]), None).0, None);
    assert_eq!(read(r, &goldeneye_save(&[]), None).0, None);
    assert_eq!(read(r, &vec![0u8; 512], None).0, None);
    assert_eq!(read(r, &vec![0xFFu8; 512], None).0, None);
    assert_eq!(read(r, &good[..0x1FF], None).0, None, "cut short");
    let mut broken = good.clone();
    broken[0x20 + 2 * 0x60 + 0x30] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "a changed byte fails the sums");
    // Folder 5 to 7 don't exist.
    assert_eq!(read(r, &goldeneye_save(&[(2, 5, 0, some, false)]), None).0, None);
    // Banjo-Kazooie's EEPROM is no GoldenEye save.
    assert_eq!(read(r, &banjo_save(&[(0, 1, &[1], [0; 9], [0; 11])]), None).0, None);
}

/// Saves the real games wrote (a headless emulator on a developer's computer; never in the repository), read as each
/// game's own screen showed them. `CASE_KNOWS_SAVES=<folder>` with files named as below.
#[test]
#[ignore = "needs saves written by the games: CASE_KNOWS_SAVES=<folder> cargo test -- --ignored real_saves"]
fn real_saves_read_as_the_games_show_them() {
    let Ok(dir) = std::env::var("CASE_KNOWS_SAVES") else { return };
    let dir = std::path::Path::new(&dir);
    let cases: &[(Kind, &str, &str)] = &[
        // Game select: "GAME 1: TIME 1:02:15, 5 JIGSAWS, 123 NOTES." and "TIME 5:17:57, 100 JIGSAWS, 900 NOTES.".
        (BanjoKazooie, "bk-1.eep", "5 Jiggies · 123 notes · game clock 1:02"),
        (BanjoKazooie, "bk-2.eep", "100 Jiggies · 900 notes · game clock 5:17"),
        // Samus Data: "A 99 ▮▮▮ 02:07" (399 energy); the ending's count of its tanks: 8 %.
        (MetroidFusion, "mf-1.srm", "8% items · game clock 2:07"),
        // Our folder 1 as loaded: "Secret Agent / Mission 2.i" (the copy the game wrote makes two such folders).
        (GoldenEye, "ge-0-ours.eep", "Mission 2.i: Surface · Secret Agent"),
    ];
    for &(kind, file, want) in cases {
        let Ok(save) = std::fs::read(dir.join(file)) else { continue };
        assert_eq!(line(&read(release_of(kind), &save, None::<&Memory>).0), want, "{file}");
    }
}
