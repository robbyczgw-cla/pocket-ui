//! "The case knows where you are" for the role-playing games added in 0.5 (Super Mario RPG, Lufia II, …): saves made
//! here byte by byte as each game writes them, laid out like saves the real games wrote in a headless emulator (no one's
//! save is in the repository). With such saves on this computer, `CASE_KNOWS_SAVES=<folder>` reads them too
//! (`real_saves_read_as_the_games_show_them`).
use crate::case_knows::{read, Kind, Kind::*, Lang, Memory, Progress, Release, RELEASES};

fn release_of(kind: Kind) -> &'static Release {
    RELEASES.iter().find(|r| r.kind == kind && r.lang == Lang::En).unwrap()
}

fn line(p: &Option<Progress>) -> String {
    p.as_ref().map_or("(nothing)".into(), |p| p.parts().join(" · "))
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Super Mario RPG's 32 KiB as snes9x keeps it: `slots` (slot, Mario's level) and the slot chosen last; a free slot as
/// the game leaves it (0xFF, then the cartridge's 0x60), the rest of the file other data.
pub(crate) fn mario_rpg_save(slots: &[(usize, u8)], last: u8) -> Vec<u8> {
    let mut s = vec![0x60u8; 0x8000];
    for i in 0..4 {
        s[i * 0x800] = 0xFF;
    }
    for &(i, level) in slots {
        let f = &mut s[i * 0x800..(i + 1) * 0x800];
        f.fill(0);
        f[0] = level;
        f[0xB5..0xBD].copy_from_slice(b"ROB     ");
        f[0x3C2] = 1;
        for (k, b) in f[0x10..0x80].iter_mut().enumerate() {
            *b = (k * 7) as u8;
        }
        let sum = f[..0x7FC].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        put16(f, 0x7FC, !sum);
    }
    s[0x7FE] = last;
    s[0x2000..0x2010].copy_from_slice(&[0, 0x0A, 0xD0, 0x11, 0, 0, 0x20, 0xC0, 0x42, 0x40, 0xF4, 3, 0x87, 0x90, 0x30, 0]);
    s
}

#[test]
fn super_mario_rpg_says_marios_level() {
    let r = release_of(MarioRpg);
    // As the real game's first save: slot 1, level 1, "ROB".
    assert_eq!(line(&read(r, &mario_rpg_save(&[(0, 1)], 0), None).0), "Mario Lv. 1");
    // The slot chosen last of two.
    assert_eq!(line(&read(r, &mario_rpg_save(&[(0, 1), (2, 17)], 2), None).0), "Mario Lv. 17");
    // Only the first 8 KiB, as other emulators keep it.
    assert_eq!(line(&read(r, &mario_rpg_save(&[(1, 9)], 1)[..0x2000], None).0), "Mario Lv. 9");
}

#[test]
fn super_mario_rpg_believes_only_what_the_game_would_load() {
    let r = release_of(MarioRpg);
    assert_eq!(read(r, &mario_rpg_save(&[], 0), None).0, None, "four free slots");
    assert_eq!(read(r, &vec![0u8; 0x8000], None).0, None, "zeros: no sum's complement is 0");
    assert_eq!(read(r, &vec![0xFFu8; 0x8000], None).0, None);
    assert_eq!(read(r, &[], None).0, None);
    let save = mario_rpg_save(&[(0, 12)], 0);
    assert_eq!(read(r, &save[..0x1FFF], None).0, None, "cut short");
    let mut broken = save.clone();
    broken[0x300] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "one bit off");
    // A level the game can't reach (30 is the most).
    assert_eq!(read(r, &mario_rpg_save(&[(0, 31)], 0), None).0, None);
}

/// Lufia II's random numbers as the game draws them (the reader's own are in `case_knows`; this is a second copy so a
/// mistake in one doesn't hide in both).
fn lufia_2_numbers(seed: u8, n: usize) -> Vec<u8> {
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
    let refill = |t: &mut [u8; 55]| {
        for x in 0..24 {
            t[x] ^= t[x + 31];
        }
        for x in 24..55 {
            t[x] ^= t[x - 24];
        }
    };
    for _ in 0..3 {
        refill(&mut t);
    }
    let mut i = 54;
    (0..n)
        .map(|_| {
            i += 1;
            if i >= 55 {
                refill(&mut t);
                i = 0;
            }
            ((0xFF * t[i] as u32) >> 8) as u8
        })
        .collect()
}

/// One Lufia II slot: leader, the hero's name and level, the clock (h, m, s).
pub(crate) type Lufia2Slot<'a> = (usize, u8, &'a str, u8, (u8, u8, u8));

/// Lufia II's 8 KiB: `slots`, the slot saved last; free slots as the game writes them (0xF0 first, scrambled zeros
/// with their sum).
pub(crate) fn lufia_2_save(slots: &[Lufia2Slot], last: u8) -> Vec<u8> {
    let mut s = vec![0u8; 0x2000];
    for i in 0..4 {
        let mut plain = vec![0u8; 0x800];
        let mut first = 0xF0;
        if let Some(&(_, leader, name, level, (h, m, sec))) = slots.iter().find(|x| x.0 == i) {
            first = 0;
            for (k, c) in name.bytes().enumerate() {
                plain[0x3CC - k] = c;
            }
            plain[0x3CE..0x3D2].copy_from_slice(&[0xFF, 0xFF, 0xFF, leader]);
            plain[0x3F6..0x3F9].copy_from_slice(&[sec, m, h]);
            plain[0x5F1] = level << 1;
            plain[0x5F1 + 0x3D] = 32;
        }
        let seed = [0x3E, 0x61, 0x5A, 0][i];
        let numbers = lufia_2_numbers(seed, 0x7FC);
        let f = &mut s[i * 0x800..(i + 1) * 0x800];
        f[0] = first;
        f[1] = seed;
        for k in 4..0x800 {
            f[k] = plain[k].wrapping_add(numbers[k - 4]);
        }
        let sum = (2..0x400).fold(0x6502u16, |a, w| a.wrapping_add(u16::from_le_bytes([f[2 * w], f[2 * w + 1]])));
        put16(f, 2, sum);
    }
    s[0] |= last;
    s
}

#[test]
fn lufia_2_names_the_hero_his_level_and_the_clock() {
    let r = release_of(Lufia2);
    // As the real game's first save: Rob, level 1, 1:32 in (its slot list says "00:01").
    assert_eq!(line(&read(r, &lufia_2_save(&[(0, 0, "Rob", 1, (0, 1, 32))], 0), None).0), "Rob Lv. 1 · game clock 0:01");
    // The slot saved last, as slot 0's first byte says (the real game's second save: slot 2).
    let two = lufia_2_save(&[(0, 0, "Rob", 1, (0, 1, 32)), (1, 0, "Rob", 1, (0, 2, 38))], 1);
    assert_eq!(line(&read(r, &two, None).0), "Rob Lv. 1 · game clock 0:02");
    assert_eq!(line(&read(r, &lufia_2_save(&[(3, 0, "Maxim", 47, (31, 5, 0))], 3), None).0), "Maxim Lv. 47 · game clock 31:05");
    // Someone else leading (only the hero's name is saved): the clock alone.
    assert_eq!(line(&read(r, &lufia_2_save(&[(0, 2, "Maxim", 20, (9, 0, 0))], 0), None).0), "game clock 9:00");
}

#[test]
fn lufia_2_believes_only_what_the_game_would_load() {
    let r = release_of(Lufia2);
    assert_eq!(read(r, &lufia_2_save(&[], 0), None).0, None, "four free slots");
    assert_eq!(read(r, &vec![0u8; 0x2000], None).0, None);
    assert_eq!(read(r, &vec![0xFFu8; 0x2000], None).0, None);
    assert_eq!(read(r, &[], None).0, None);
    let save = lufia_2_save(&[(0, 0, "Rob", 12, (3, 4, 5))], 0);
    assert_eq!(read(r, &save[..0x1800], None).0, None, "cut short");
    let mut broken = save.clone();
    broken[0x500] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "one bit off");
    // A level the game can't have shows no name; minutes past 59 no line.
    assert_eq!(line(&read(r, &lufia_2_save(&[(0, 0, "Rob", 120, (3, 4, 5))], 0), None).0), "game clock 3:04");
    assert_eq!(read(r, &lufia_2_save(&[(0, 0, "Rob", 12, (3, 64, 5))], 0), None).0, None);
}

/// Sets `n` bits at bit `at` of a Pokémon Mystery Dungeon stream, the value's lowest bit first.
fn put_bits(stream: &mut [u8], at: usize, n: usize, v: u32) {
    for j in 0..n {
        let (byte, bit) = ((at + j) >> 3, (at + j) & 7);
        stream[byte] = stream[byte] & !(1 << bit) | (((v >> j) & 1) as u8) << bit;
    }
}

/// Pokémon Mystery Dungeon: Red Rescue Team's 128 KiB: the game's save in both copies with the leader (record number,
/// level, name) and the clock (h, m, s); `first_damaged`: the first copy's sum off by one.
pub(crate) fn mystery_dungeon_save(leader: u16, level: u8, name: &str, (h, m, sec): (u16, u8, u8), first_damaged: bool) -> Vec<u8> {
    let mut c = vec![0u8; 0x57D4];
    c[0x404..0x414].copy_from_slice(b"POKE_DUNGEON__05");
    c[0x414..0x418].copy_from_slice(&0x0507_1412u32.to_le_bytes());
    c[0x424] = 7;
    let stream = &mut c[0x448..];
    // Two Pokémon recruited: the partner (record 0) and the leader.
    put_bits(stream, 0, 7, 5);
    put_bits(stream, 7, 9, 4);
    for (k, b) in b"Charmander".iter().enumerate() {
        put_bits(stream, 243 + 8 * k, 8, *b as u32);
    }
    let at = 323 * leader as usize;
    put_bits(stream, at, 7, level as u32);
    put_bits(stream, at + 7, 9, 66);
    for (k, b) in name.bytes().enumerate() {
        put_bits(stream, at + 243 + 8 * k, 8, b as u32);
    }
    put_bits(stream, 134_791, 16, leader as u32);
    c[0x4EC8..0x4ECF].copy_from_slice(b"Pok\xE9mon"[..7].as_ref());
    let w = (sec as u32) << 20 | (m as u32) << 26 | 17 << 14;
    c[0x4EE0..0x4EE4].copy_from_slice(&w.to_le_bytes());
    put16(&mut c, 0x4EE4, h & 0x3FFF);
    let sum = (1..0x57D4 / 4).fold(0u32, |a, w| a.wrapping_add(u32::from_le_bytes(c[4 * w..4 * w + 4].try_into().unwrap())));
    c[..4].copy_from_slice(&sum.to_le_bytes());
    let mut s = vec![0u8; 0x20000];
    s[..0x57D4].copy_from_slice(&c);
    s[0x6000..0x6000 + 0x57D4].copy_from_slice(&c);
    if first_damaged {
        s[0] ^= 1;
    }
    s
}

#[test]
fn mystery_dungeon_red_names_the_leader_and_the_clock() {
    let r = release_of(MysteryDungeonRed);
    // As the real game's first save: Rob (a Machop) at level 5, 0:05:02 on the Continue screen.
    assert_eq!(line(&read(r, &mystery_dungeon_save(295, 5, "Rob", (0, 5, 2), false), None).0), "Rob Lv. 5 · game clock 0:05");
    assert_eq!(line(&read(r, &mystery_dungeon_save(412, 100, "Charizard", (123, 59, 59), false), None).0), "Charizard Lv. 100 · game clock 123:59");
    // The first copy damaged: the second, as the game does.
    assert_eq!(line(&read(r, &mystery_dungeon_save(3, 31, "Pika", (20, 7, 0), true), None).0), "Pika Lv. 31 · game clock 20:07");
}

#[test]
fn mystery_dungeon_red_believes_only_what_the_game_would_load() {
    let r = release_of(MysteryDungeonRed);
    assert_eq!(read(r, &vec![0xFFu8; 0x20000], None).0, None, "erased flash");
    assert_eq!(read(r, &vec![0u8; 0x20000], None).0, None, "zeros add up, but the magic is missing");
    assert_eq!(read(r, &[], None).0, None);
    let save = mystery_dungeon_save(295, 5, "Rob", (0, 5, 2), false);
    assert_eq!(read(r, &save[..0x57D0], None).0, None, "cut short");
    let mut both = save.clone();
    both[0x1000] ^= 1;
    both[0x7000] ^= 1;
    assert_eq!(read(r, &both, None).0, None, "both copies damaged");
    let mut magic = save.clone();
    for at in [0x414, 0x6414] {
        magic[at] ^= 1;
        magic[at + 1] ^= 1;
    }
    // (The sums fixed: only the magic is wrong.)
    for base in [0, 0x6000] {
        let sum = (1..0x57D4 / 4).fold(0u32, |a, w| a.wrapping_add(u32::from_le_bytes(magic[base + 4 * w..base + 4 * w + 4].try_into().unwrap())));
        magic[base..base + 4].copy_from_slice(&sum.to_le_bytes());
    }
    assert_eq!(read(r, &magic, None).0, None, "no magic");
    // A level the game can't have keeps the name out; a record number past the last too.
    assert_eq!(line(&read(r, &mystery_dungeon_save(295, 101, "Rob", (0, 5, 2), false), None).0), "game clock 0:05");
}

/// One Final Fantasy VI Advance slot: the leader's name in plain letters ("" not named yet), level, clock (h, m, s).
pub(crate) type Ff6aSlot<'a> = (usize, &'a str, u8, (u8, u8, u8));

/// Final Fantasy VI Advance's 32 KiB as the game writes it: `slots` (Terra leading party 1, Wedge and Biggs behind her,
/// as at the Narshe mines), unused slots all zeros, the slot saved last (1-3).
pub(crate) fn ff6a_save(slots: &[Ff6aSlot], last: u8) -> Vec<u8> {
    let letters = |t: &str| -> Vec<u8> { t.bytes().map(|c| if c.is_ascii_uppercase() { 0x20 + c - b'A' } else { 0x3A + c - b'a' }).collect() };
    let mut s = vec![0u8; 0x8000];
    for &(i, name, level, (h, m, sec)) in slots {
        let f = &mut s[i * 0xA00..(i + 1) * 0xA00];
        let mut put = |c: usize, name: &[u8], level: u8, bits: u8| {
            let ch = &mut f[c * 0x25..(c + 1) * 0x25];
            ch[0] = c as u8;
            ch[2..8].fill(0);
            ch[2..2 + name.len()].copy_from_slice(name);
            ch[8] = level;
            ch[9..11].copy_from_slice(&45u16.to_le_bytes());
            f[0x250 + c] = bits;
        };
        put(0, &if name.is_empty() { vec![0x5F; 6] } else { letters(name) }, level, 0xC1);
        put(14, &letters("Wedge"), 3, 0x49);
        put(15, &letters("Biggs"), 3, 0x51);
        f[0x260..0x263].copy_from_slice(&[0x46, 0x0E, 0]);
        f[0x263..0x266].copy_from_slice(&[h, m, sec]);
        f[0x266] = 137;
        f[0x6F5] = 1;
        let sum = f[..0x9FE].iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
        put16(f, 0x9FE, sum);
    }
    s[0x1F00..0x1F20].copy_from_slice(b"FINAL FANTASY VI     ADVANCE    ");
    s[0x1F20] = 1;
    s[0x1FF0] = last;
    s[0x1FF8..0x2000].copy_from_slice(&[0x1B, 0xE4, 0x1B, 0xE4, 0x1B, 0xE4, 0x1B, 0xE4]);
    s[0x23FC..0x2400].copy_from_slice(&[0x47, 0x4D, 0x60, 0x1F]);
    s
}

#[test]
fn final_fantasy_6_advance_names_the_party_leader_and_the_clock() {
    let r = release_of(FinalFantasy6Advance);
    assert_eq!(line(&read(r, &ff6a_save(&[(1, "Tina", 42, (31, 5, 9))], 2), None).0), "Tina Lv. 42 · game clock 31:05");
    // As the real game's two saves at the Narshe mines: Terra not named yet ("??????"), the second slot saved last.
    let two = ff6a_save(&[(0, "", 4, (0, 13, 35)), (1, "", 4, (0, 14, 56))], 2);
    assert_eq!(line(&read(r, &two, None).0), "game clock 0:14");
    assert_eq!(line(&read(r, &ff6a_save(&[(2, "TERRA", 30, (20, 0, 0))], 3), None).0), "Terra Lv. 30 · game clock 20:00");
}

#[test]
fn final_fantasy_6_advance_believes_only_what_the_game_would_load() {
    let r = release_of(FinalFantasy6Advance);
    assert_eq!(read(r, &ff6a_save(&[], 0), None).0, None, "three unused slots: zeros add up to 0, but are no file");
    assert_eq!(read(r, &vec![0u8; 0x8000], None).0, None);
    assert_eq!(read(r, &vec![0xFFu8; 0x8000], None).0, None);
    let save = ff6a_save(&[(0, "Tina", 5, (1, 2, 3))], 1);
    assert_eq!(read(r, &save[..0x1F10], None).0, None, "cut short");
    let mut broken = save.clone();
    broken[0x100] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "one bit off");
    // The SNES game's save is laid out alike but has no "ADVANCE" mark: not this game's.
    let mut snes = save.clone();
    snes[0x1F00..0x1F20].fill(0);
    assert_eq!(read(r, &snes, None).0, None);
}

/// One Tactics Ogre save: the sector of its first part, of its second, its file, count, the hero, level, clock (h, m).
pub(crate) type TacticsSave<'a> = (usize, usize, u8, u32, &'a str, u8, (u8, u8));

/// Tactics Ogre: The Knight of Lodis's 64 KiB of flash: erased sectors, and `saves` as the game writes them (the
/// header, the summary, both parts, the CRC over them).
pub(crate) fn tactics_ogre_save(saves: &[TacticsSave]) -> Vec<u8> {
    let mut s = vec![0xFFu8; 0x10000];
    for &(first, second, file, count, name, level, (h, m)) in saves {
        let mut head = vec![0u8; 0x1000];
        head[..8].copy_from_slice(b"TOGQST02");
        head[8..0xC].copy_from_slice(&count.to_le_bytes());
        head[0x11] = file;
        head[0x14..0x1C].fill(0xFF);
        for (k, c) in name.bytes().enumerate() {
            head[0x14 + k] = if c.is_ascii_uppercase() { c - b'A' } else { 26 + c - b'a' };
        }
        head[0x24] = level;
        head[0x26..0x28].copy_from_slice(&433u16.to_le_bytes());
        head[0x28..0x2A].copy_from_slice(&[5, 20]);
        head[0x2A] = h;
        head[0x2B] = m;
        for (k, b) in head[0x30..0x200].iter_mut().enumerate() {
            *b = (k * 13) as u8;
        }
        let mut tail = vec![0u8; 0x1000];
        for (k, b) in tail[..0x420].iter_mut().enumerate() {
            *b = (k * 5) as u8;
        }
        let mut both = head.clone();
        both.extend_from_slice(&tail[..0x420]);
        let crc = crc32fast::hash(&both);
        head[0xC..0x10].copy_from_slice(&crc.to_le_bytes());
        head[0x12] = second as u8;
        s[first * 0x1000..(first + 1) * 0x1000].copy_from_slice(&head);
        s[second * 0x1000..(second + 1) * 0x1000].copy_from_slice(&tail);
    }
    s
}

#[test]
fn tactics_ogre_names_the_hero_his_level_and_the_clock() {
    let r = release_of(TacticsOgre);
    // As the real game's first save: Rob, level 1, 0:26, its parts in sectors 3 and 2.
    assert_eq!(line(&read(r, &tactics_ogre_save(&[(3, 2, 0, 1, "Rob", 1, (0, 26))]), None).0), "Rob Lv. 1 · game clock 0:26");
    // The file saved last (the highest count), whatever its place in the flash.
    let two = tactics_ogre_save(&[(3, 2, 0, 1, "Rob", 1, (0, 26)), (9, 10, 1, 2, "Alphonse", 12, (5, 40))]);
    assert_eq!(line(&read(r, &two, None).0), "Alphonse Lv. 12 · game clock 5:40");
}

#[test]
fn tactics_ogre_believes_only_what_the_game_would_load() {
    let r = release_of(TacticsOgre);
    assert_eq!(read(r, &vec![0xFFu8; 0x10000], None).0, None, "erased flash");
    assert_eq!(read(r, &vec![0u8; 0x10000], None).0, None);
    let save = tactics_ogre_save(&[(3, 2, 0, 1, "Rob", 1, (0, 26))]);
    assert_eq!(read(r, &save[..0xFFFF], None).0, None, "cut short");
    for at in [0x3100, 0x2010, 0x3014] {
        let mut broken = save.clone();
        broken[at] ^= 1;
        assert_eq!(read(r, &broken, None).0, None, "one bit off at {at:#x}");
    }
    // A newer save that is damaged gives way to the older one.
    let mut newer = tactics_ogre_save(&[(3, 2, 0, 1, "Rob", 1, (0, 26)), (5, 6, 0, 2, "Rob", 2, (1, 0))]);
    newer[0x6100] ^= 1;
    assert_eq!(line(&read(r, &newer, None).0), "Rob Lv. 1 · game clock 0:26");
    // A suspended battle (file 3) is no file to show.
    assert_eq!(read(r, &tactics_ogre_save(&[(3, 2, 3, 1, "Rob", 1, (0, 26))]), None).0, None);
}

/// Breath of Fire II's 8 KiB: `slots` (slot, the hero's name, level), the rest as the game writes a new file's (the
/// other characters' records after the hero's, the clock at 0x6A8); free slots without the mark.
pub(crate) fn breath_of_fire_2_save(slots: &[(usize, &str, u8)]) -> Vec<u8> {
    let mut s = vec![0u8; 0x2000];
    for &(i, name, level) in slots {
        let f = &mut s[i * 0xAA0..(i + 1) * 0xAA0];
        f[2..10].copy_from_slice(b"SAN!BOF2");
        let others: [(&[u8], u8, u8); 8] =
            [(b"Bow", 10, 1), (b"Rand", 2, 6), (b"Katt", 3, 6), (b"Nina", 4, 8), (b"Sten", 6, 8), (b"Jean", 5, 10), (b"Spar", 7, 12), (b"Bleu", 8, 35)];
        let mut put = |k: usize, n: &[u8], id: u8, lv: u8| {
            let c = &mut f[0x1E0 + 0x40 * k..0x1E0 + 0x40 * (k + 1)];
            c[..n.len()].copy_from_slice(n);
            c[4] = 0x80;
            c[6] = id;
            c[7] = lv;
        };
        put(0, name.as_bytes(), 9, level);
        for (k, (n, id, lv)) in others.iter().enumerate() {
            put(k + 1, n, *id, *lv);
        }
        let sum = (1..0xAA0 / 2).fold(0u16, |a, w| a.wrapping_add(u16::from_le_bytes([f[2 * w], f[2 * w + 1]])));
        put16(f, 0, sum);
    }
    s
}

#[test]
fn breath_of_fire_2_names_the_hero_and_his_level() {
    let r = release_of(BreathOfFire2);
    // As the real game's new file: "No.1 Rob 01".
    assert_eq!(line(&read(r, &breath_of_fire_2_save(&[(0, "Rob", 1)]), None).0), "Rob Lv. 1");
    assert_eq!(line(&read(r, &breath_of_fire_2_save(&[(2, "Ryu", 23)]), None).0), "Ryu Lv. 23");
    // Two files and nothing to go by: nothing rather than a guess; then the one saved again.
    let before = breath_of_fire_2_save(&[(0, "Rob", 1), (1, "Ryu", 5)]);
    let (p, memory) = read(r, &before, None);
    assert_eq!(p, None);
    let after = breath_of_fire_2_save(&[(0, "Rob", 1), (1, "Ryu", 6)]);
    assert_eq!(line(&read(r, &after, Some(&memory)).0), "Ryu Lv. 6");
}

#[test]
fn breath_of_fire_2_believes_only_what_the_game_would_load() {
    let r = release_of(BreathOfFire2);
    assert_eq!(read(r, &breath_of_fire_2_save(&[]), None).0, None, "three free slots");
    assert_eq!(read(r, &vec![0u8; 0x2000], None).0, None);
    assert_eq!(read(r, &vec![0xFFu8; 0x2000], None).0, None);
    let save = breath_of_fire_2_save(&[(0, "Rob", 12)]);
    assert_eq!(read(r, &save[..0x1000], None).0, None, "cut short");
    let mut broken = save.clone();
    broken[0x300] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "one bit off");
    assert_eq!(read(r, &breath_of_fire_2_save(&[(0, "Rob", 120)]), None).0, None, "a level the game can't have");
}

/// One Superstar Saga file: its number, whether Luigi leads, Mario's and Luigi's levels, the play time in frames.
pub(crate) type SuperstarFile = (usize, bool, u8, u8, u32);

/// Mario & Luigi: Superstar Saga's 8 KiB of EEPROM with `files` (100 coins, a room, as the game's first Save Album
/// writes them); `as_mgba`: each 8-byte block back to front, as mGBA keeps the file.
pub(crate) fn superstar_saga_save(files: &[SuperstarFile], as_mgba: bool) -> Vec<u8> {
    let mut e = vec![0u8; 0x2000];
    e[..6].copy_from_slice(b"MALRPG");
    for &(i, luigi, mario_level, luigi_level, frames) in files {
        e[6] |= 1 << i;
        let f = &mut e[0x10 + 0x6F8 * i..0x10 + 0x6F8 * (i + 1)];
        f[0] = i as u8;
        f[2..4].copy_from_slice(b"AD");
        let d = &mut f[4..0x6F4];
        d[0] = 0x20 | if luigi { 8 } else { 0 };
        d[4..6].copy_from_slice(&0x0123u16.to_le_bytes());
        d[0x4C] = mario_level;
        d[0x88] = luigi_level;
        d[0x8A] = 1;
        d[0xA4..0xA6].copy_from_slice(&100u16.to_le_bytes());
        d[0x6EC..0x6F0].copy_from_slice(&frames.to_le_bytes());
        f[1] = f[4..0x6F4].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    }
    if as_mgba {
        e = e.chunks(8).flat_map(|b| b.iter().rev().copied()).collect();
    }
    e
}

#[test]
fn superstar_saga_names_the_leading_brother_his_level_and_the_clock() {
    let r = release_of(SuperstarSaga);
    const MINUTE: u32 = 3600;
    // As the real game's first Save Album: file 1, both at level 1, 00:13 on the LOAD screen.
    assert_eq!(line(&read(r, &superstar_saga_save(&[(0, false, 1, 1, 13 * MINUTE + 20)], true), None).0), "Mario Lv. 1 · game clock 0:13");
    // The other block order; Luigi leading.
    assert_eq!(line(&read(r, &superstar_saga_save(&[(2, true, 11, 22, 125 * MINUTE)], false), None).0), "Luigi Lv. 22 · game clock 2:05");
    // Two files and nothing to go by: nothing; then the one saved again.
    let (p, memory) = read(r, &superstar_saga_save(&[(0, false, 3, 3, MINUTE), (1, false, 9, 8, 60 * MINUTE)], true), None);
    assert_eq!(p, None);
    let after = superstar_saga_save(&[(0, false, 4, 4, 2 * MINUTE), (1, false, 9, 8, 60 * MINUTE)], true);
    assert_eq!(line(&read(r, &after, Some(&memory)).0), "Mario Lv. 4 · game clock 0:02");
}

#[test]
fn superstar_saga_believes_only_what_the_game_would_load() {
    let r = release_of(SuperstarSaga);
    assert_eq!(read(r, &superstar_saga_save(&[], true), None).0, None, "no file in use");
    assert_eq!(read(r, &vec![0xFFu8; 0x2000], None).0, None);
    assert_eq!(read(r, &vec![0u8; 0x2000], None).0, None);
    let save = superstar_saga_save(&[(0, false, 5, 5, 3600)], true);
    assert_eq!(read(r, &save[..0x1000], None).0, None, "cut short");
    let mut broken = save.clone();
    broken[0x300] ^= 1;
    assert_eq!(read(r, &broken, None).0, None, "one bit off");
    // A file the header doesn't count (the game clears its bit when its sum fails) is not read.
    let mut unused = superstar_saga_save(&[(0, false, 5, 5, 3600)], false);
    unused[6] = 0;
    assert_eq!(read(r, &unused, None).0, None);
    // A level the game can't have keeps the brother out.
    assert_eq!(line(&read(r, &superstar_saga_save(&[(0, false, 0, 1, 3600)], true), None).0), "game clock 0:01");
}

/// One Mother 3 file: its number, the leader's name and level, the play time in sixtieths of a second.
pub(crate) type Mother3File<'a> = (usize, &'a str, u8, u32);

/// A Mother 3 bank as the game writes it: 2, 0x334D, its sum's complement at `check`, the data from 8.
fn mother_3_bank(data: &[u8], check: usize) -> Vec<u8> {
    let mut b = vec![0u8; 0x1000];
    b[..4].copy_from_slice(&[2, 0, 0x4D, 0x33]);
    b[8..8 + data.len()].copy_from_slice(data);
    let sum = (0..0x800).fold(0u16, |a, w| a.wrapping_add(u16::from_le_bytes([b[2 * w], b[2 * w + 1]])));
    put16(&mut b, check, 0xFFFF - sum);
    b
}

/// Mother 3's 64 KiB of flash: the game's own bank (and its mirror) with the files' summaries, marks and the file saved
/// last, and each file's three banks in both copies; `damaged`: (file, copy) whose middle bank is off by one bit.
pub(crate) fn mother_3_save(files: &[Mother3File], last: u32, damaged: &[(usize, usize)]) -> Vec<u8> {
    let mut s = vec![0xFFu8; 0x10000];
    let mut system = vec![0u8; 0xE8];
    system[..4].copy_from_slice(&0x7418u32.to_le_bytes());
    for &(i, name, level, time) in files {
        let at = 0x64 * i;
        let sum = &mut system[at..at + 0x60];
        sum[..4].copy_from_slice(&time.to_le_bytes());
        sum[0x0A] = 0x15;
        sum[0x34..0x36].copy_from_slice(&[2, 3]);
        for k in 0..8 {
            let l = name.as_bytes().get(k).map_or(0xFFFF, |c| *c as u16 - 0x20);
            put16(sum, 0x36 + 2 * k, l);
        }
        sum[0x46] = level;
        sum[0x48..0x4A].copy_from_slice(&38u16.to_le_bytes());
        system[0x60 + at] = 1;
        for copy in 0..2 {
            for k in 0..3 {
                let data: Vec<u8> = (0..0x200).map(|n| (n * 7 + k * 3 + i) as u8).collect();
                let mut b = mother_3_bank(&data, 4);
                if k == 1 && damaged.contains(&(i, copy)) {
                    b[0x400] ^= 1;
                }
                let n = 6 * i + 4 + 3 * copy + k;
                s[n * 0x1000..(n + 1) * 0x1000].copy_from_slice(&b);
            }
        }
    }
    let mut own = vec![0u8; 0xE8];
    own[..4].copy_from_slice(&last.to_le_bytes());
    own[4..].copy_from_slice(&system[..0xE4]);
    // The game's own bank keeps its sum at 0xF0 (its first word after the marks is the file saved last).
    let mut b = vec![0u8; 0x1000];
    b[..4].copy_from_slice(&[2, 0, 0x4D, 0x33]);
    b[4..4 + 0xE8].copy_from_slice(&own);
    let sum = (0..0x800).fold(0u16, |a, w| a.wrapping_add(u16::from_le_bytes([b[2 * w], b[2 * w + 1]])));
    put16(&mut b, 0xF0, 0xFFFF - sum);
    s[..0x1000].copy_from_slice(&b);
    s[0x1000..0x2000].copy_from_slice(&b);
    s[0x2000..0x4000].fill(0);
    s
}

#[test]
fn mother_3_names_the_leader_his_level_and_the_clock_as_the_load_screen_does() {
    let r = release_of(Mother3);
    const MINUTE: u32 = 3600;
    // As the real game's two saves at the first frog: Lucas, level 1, 00:08 and 00:13, the second file saved last.
    let one = mother_3_save(&[(0, "Lucas", 1, 8 * MINUTE + 15)], 0, &[]);
    assert_eq!(line(&read(r, &one, None).0), "Lucas Lv. 1 · game clock 0:08");
    let two = mother_3_save(&[(0, "Lucas", 1, 8 * MINUTE + 15), (1, "Lucas", 1, 13 * MINUTE + 45)], 1, &[]);
    assert_eq!(line(&read(r, &two, None).0), "Lucas Lv. 1 · game clock 0:13");
    assert_eq!(line(&read(r, &mother_3_save(&[(1, "Maxi", 23, 12 * 216_000 + 34 * MINUTE)], 1, &[]), None).0), "Maxi Lv. 23 · game clock 12:34");
    // One copy damaged: the other carries the file, as in the game.
    let half = mother_3_save(&[(0, "Flint", 30, 40 * 216_000)], 0, &[(0, 0)]);
    assert_eq!(line(&read(r, &half, None).0), "Flint Lv. 30 · game clock 40:00");
}

#[test]
fn mother_3_believes_only_what_the_game_would_load() {
    let r = release_of(Mother3);
    assert_eq!(read(r, &vec![0xFFu8; 0x10000], None).0, None, "erased flash");
    assert_eq!(read(r, &vec![0u8; 0x10000], None).0, None);
    assert_eq!(read(r, &mother_3_save(&[], 0, &[]), None).0, None, "no file yet");
    let save = mother_3_save(&[(0, "Lucas", 1, 30_000)], 0, &[]);
    assert_eq!(read(r, &save[..0xFFFF], None).0, None, "cut short");
    let mut own = save.clone();
    own[0x20] ^= 1;
    own[0x1020] ^= 1;
    assert_eq!(read(r, &own, None).0, None, "the game's own bank and its mirror damaged");
    // Both copies of the file damaged: the game calls it damaged, so nothing.
    assert_eq!(read(r, &mother_3_save(&[(0, "Lucas", 1, 30_000)], 0, &[(0, 0), (0, 1)]), None).0, None);
    // A name in letters the translation doesn't write (the Japanese game's) keeps the name out.
    let mut japanese = save.clone();
    for b in [0, 0x1000] {
        japanese[b + 4 + 4 + 0x36] = 0x9C;
        let sum = (0..0x800).fold(0u16, |a, w| if w == 0x78 { a } else { a.wrapping_add(u16::from_le_bytes([japanese[b + 2 * w], japanese[b + 2 * w + 1]])) });
        put16(&mut japanese[b..], 0xF0, 0xFFFF - sum);
    }
    assert_eq!(line(&read(r, &japanese, None).0), "game clock 0:08");
}

/// Saves the real games wrote (a headless emulator on a developer's computer; never in the repository), read as each
/// game's own file screen showed them. `CASE_KNOWS_SAVES=<folder>` with files named as below.
#[test]
#[ignore = "needs saves written by the games: CASE_KNOWS_SAVES=<folder> cargo test -- --ignored real_saves"]
fn real_saves_read_as_the_games_show_them() {
    let Ok(dir) = std::env::var("CASE_KNOWS_SAVES") else { return };
    let dir = std::path::Path::new(&dir);
    let cases: &[(Kind, &str, &str)] = &[
        // File screen: "MARIO1 ROB / LV 1 Mario's Pad".
        (MarioRpg, "smrpg-1.sav", "Mario Lv. 1"),
        // Slot list: "Rob TIME 00:01 1"; then slot 2 "Rob 00:02".
        (Lufia2, "lufia2-1.sav", "Rob Lv. 1 · game clock 0:01"),
        (Lufia2, "lufia2-2.sav", "Rob Lv. 1 · game clock 0:02"),
        // Continue: "Name: Rob · Play time: 0:05:02"; the dungeon menu said Lv 5.
        (MysteryDungeonRed, "pmd-1.sav", "Rob Lv. 5 · game clock 0:05"),
        // Load Game: "?????? Magitek Elite LV 4 … Time 0:13", then file 2 "Time 0:14" (Terra not named yet).
        (FinalFantasy6Advance, "ff6a-1.sav", "game clock 0:13"),
        (FinalFantasy6Advance, "ff6a-2.sav", "game clock 0:14"),
        // Continue from save: "Lv. 1 / Rob / Solea / Flama 20 / 0:26".
        (TacticsOgre, "tokol-1.sav", "Rob Lv. 1 · game clock 0:26"),
        // Game Start: "No.1 Rob 01".
        (BreathOfFire2, "bof2-1.sav", "Rob Lv. 1"),
        // LOAD: "100 coins, 00:11, Luigi LV 1, Mario LV 1", then 00:13.
        (SuperstarSaga, "mlss-1.sav", "Mario Lv. 1 · game clock 0:11"),
        (SuperstarSaga, "mlss-2.sav", "Mario Lv. 1 · game clock 0:13"),
        // Load: "LV 1 Lucas Chapter 1 … TIME 00:08", then slot 2 "TIME 00:13" (the cursor on it).
        (Mother3, "m3-1.sav", "Lucas Lv. 1 · game clock 0:08"),
        (Mother3, "m3-2.sav", "Lucas Lv. 1 · game clock 0:13"),
    ];
    let mut checked = 0;
    for (kind, file, want) in cases {
        let Ok(save) = std::fs::read(dir.join(file)) else { continue };
        let (p, _) = read(release_of(*kind), &save, Some(&Memory::default()));
        assert_eq!(line(&p), *want, "{file}");
        checked += 1;
    }
    assert!(checked > 0, "no saves in {}", dir.display());
}
