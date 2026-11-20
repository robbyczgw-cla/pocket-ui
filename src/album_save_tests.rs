//! The album's moments read from saves as each game writes them (`case_knows::tests`' builders: the layouts checked
//! against saves the games wrote, 15.11.2026; no one's real save is in the repository), a save before and after per
//! kind of reading; and the camera's whole way from a save the game writes to a photo in the album.

use super::*;
use crate::case_knows::tests::*;
use crate::case_knows::{self, Kind, Kind::*, Lang, Release, RELEASES};

fn release(kind: Kind) -> &'static Release {
    RELEASES.iter().find(|r| r.kind == kind && r.lang == Lang::En).unwrap()
}

/// The note the step from `before` to `after` (two saves of `kind`) takes, its first part.
fn step(kind: Kind, before: &[u8], after: &[u8]) -> Option<String> {
    let r = release(kind);
    let (a, mem) = case_knows::read(r, before, None);
    let (b, _) = case_knows::read(r, after, Some(&mem));
    let (a, b) = (a.expect("before reads"), b.expect("after reads"));
    moment(&a, &b).map(|n| n[0].clone())
}

const HOUR: u32 = 60 * 60 * 60;

#[test]
fn pokemon_a_badge_and_a_level_of_ten() {
    let b = |badges, lv, h| gen1_save(&[("PIKACHU", lv), ("ONIX", 12)], badges, (h, 0));
    assert_eq!(step(Gen1, &b(0b1, 21, 3), &b(0b11, 22, 4)).as_deref(), Some("Badge 2"));
    assert_eq!(step(Gen1, &b(0b11, 29, 4), &b(0b11, 30, 5)).as_deref(), Some("Pikachu reached Lv. 30"));
    assert_eq!(step(Gen1, &b(0b11, 30, 4), &b(0b11, 31, 9)), None, "a level, the clock: nothing");
    // The GBA games count their badges (the builder takes the count).
    let e = |badges, lv, h, n| gen3_save(Emerald, &[("TORCHIC", lv, false)], badges, (h, 0), n);
    assert_eq!(step(Emerald, &e(3, 25, 5, 3), &e(4, 26, 6, 4)).as_deref(), Some("Badge 4"));
}

#[test]
fn zelda_remains_instruments_essences_elements_medallions() {
    assert_eq!(step(MajorasMask, &majora_save(&[(0, 0b0001, 0x60, false)], false), &majora_save(&[(0, 0b0011, 0x70, false)], false)).as_deref(), Some("2 of 4 remains"));
    assert_eq!(step(LinksAwakening, &ladx_save(&[(1, 0b0011, 5)]), &ladx_save(&[(1, 0b0111, 6)])).as_deref(), Some("3 of 8 instruments"));
    assert_eq!(step(OracleOfAges, &oracle_save(true, &[(1, 0b1, 16)]), &oracle_save(true, &[(1, 0b11, 20)])).as_deref(), Some("2 of 8 essences"));
    assert_eq!(step(MinishCap, &minish_save(&[(0, 0b01, 48)], Some(0), false), &minish_save(&[(0, 0b0101, 56)], Some(0), false)).as_deref(), Some("2 of 4 elements"));
    // Ocarina: the child's stones, then the adult's medallions.
    assert_eq!(step(Ocarina, &ocarina_save(&[(0, 0b01 << 18, 4)]), &ocarina_save(&[(0, 0b11 << 18, 5)])).as_deref(), Some("2 of 3 spiritual stones"));
    // Hearts alone (a piece of heart made whole) are not a step.
    assert_eq!(step(LinksAwakening, &ladx_save(&[(1, 0b0011, 5)]), &ladx_save(&[(1, 0b0011, 6)])), None);
}

#[test]
fn nintendo_64_cups_medals_balloons_spirits() {
    assert_eq!(step(MarioKart64, &mario_kart_64_save([0b11, 0, 0, 0]), &mario_kart_64_save([0b1111, 0, 0, 0])).as_deref(), Some("2 of 16 gold cups"));
    assert_eq!(step(StarFox64, &star_fox_64_save(&[0x07]), &star_fox_64_save(&[0x07, 0x07])).as_deref(), Some("2 medals"));
    assert_eq!(step(DiddyKongRacing, &dkr_save(0, 9), &dkr_save(0, 10)).as_deref(), Some("10 of 47 golden balloons"));
    assert_eq!(step(DiddyKongRacing, &dkr_save(0, 10), &dkr_save(0, 11)), None, "a balloon between the fives");
    let pm = |spirits, lv| paper_mario_save(&[(0, 0, 5, 1000, spirits, lv)], Some(0));
    assert_eq!(step(PaperMario, &pm(2, 8), &pm(3, 9)).as_deref(), Some("3 of 7 Star Spirits"));
    // Super Mario 64: 7 → 8 stars opens a door, 9 → 10 is a star like any other.
    let sm = |stars: &[u8]| mario_64_save(&[(0, 0, stars)]);
    assert_eq!(step(Mario64, &sm(&[0x7F]), &sm(&[0x7F, 0x01])).as_deref(), Some("8 of 120 stars"));
    assert_eq!(step(Mario64, &sm(&[0x7F, 0x03]), &sm(&[0x7F, 0x07])), None);
}

#[test]
fn role_playing_games_the_leaders_level_of_ten_and_a_friend() {
    assert_eq!(step(FinalFantasy6, &ff6_save(0, ("TERRA", 19), (8, 0)), &ff6_save(0, ("TERRA", 20), (9, 0))).as_deref(), Some("Terra reached Lv. 20"));
    assert_eq!(step(FinalFantasy6, &ff6_save(0, ("TERRA", 20), (8, 0)), &ff6_save(0, ("TERRA", 21), (9, 0))), None);
    let gs = |lv, h| golden_sun_save(false, &[(0, 0, 4, "Isaac", lv, h * HOUR)]);
    assert_eq!(step(GoldenSun, &gs(19, 9), &gs(20, 10)).as_deref(), Some("Isaac reached Lv. 20"));
    assert_eq!(step(EarthBound, &earthbound_save(0, &[1, 2, 0, 0, 0, 0], "Ness", 12), &earthbound_save(0, &[1, 2, 3, 0, 0, 0], "Ness", 13)).as_deref(), Some("2 friends"));
}

#[test]
fn a_kirby_shard_and_a_link_to_the_past_pendant() {
    assert_eq!(step(KirbyMirror, &kirby_save(&[(0, 0b0001, 3)]), &kirby_save(&[(0, 0b0011, 4)])).as_deref(), Some("2 of 8 mirror shards"));
    let z = |pendants, crystals, progress| zelda_save([Some((pendants, crystals, 6, progress)), None, None]);
    assert_eq!(step(LinkToThePast, &z(0b1, 0, 1), &z(0b11, 0, 1)).as_deref(), Some("2 of 3 pendants"));
}

/// A game's battery save as the case-knows reading finds it: a ROM the caches know as `kind`'s release, its save
/// beside it. The camera never reads a ROM through while the game runs, so the cache line is what arms it.
fn game(data: &Path, kind: Kind, save: &[u8]) -> PathBuf {
    let roms = data.join("roms/gba");
    std::fs::create_dir_all(&roms).unwrap();
    let rom = roms.join("Game.gba");
    std::fs::write(&rom, b"not a real game").unwrap();
    std::fs::write(rom.with_extension("srm"), save).unwrap();
    let m = std::fs::metadata(&rom).unwrap();
    let mtime = m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let crc = RELEASES.iter().find(|r| r.kind == kind && r.verified()).unwrap().crc;
    std::fs::create_dir_all(data.join("identify")).unwrap();
    std::fs::write(data.join("identify/case-knows.tsv"), format!("{}\t{mtime}\t{crc:08X}\t-\t\t{}\n", m.len(), rom.display())).unwrap();
    rom
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < until {
        if what() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// A made-up game picture (a GBA's 240 x 160): two halves of different colours, so the photo can be checked.
fn picture() -> Picture {
    let mut px = vec![0x00_20_80_20u32; 240 * 160];
    for p in px.iter_mut().skip(240 * 80) {
        *p = 0x00_e0_40_40;
    }
    Picture::Image(std::sync::Arc::new(crate::image::Image { w: 240, h: 160, px }))
}

/// The journey: a game is played, its save shows a new badge, the camera asks for one picture, and the photo lies in the
/// game's album with its note, at most 320 px wide, a JPEG that reads back; the same badge again (an older save loaded
/// and played) takes no second photo, and a clock that ticks takes none.
#[test]
fn a_new_badge_in_the_save_puts_a_photo_in_the_case() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path();
    let save = |badges, h| gen3_save(Emerald, &[("TORCHIC", 25, false)], badges, (h, 12), 7);
    let rom = game(data, Emerald, &save(1, 3));
    let cam = Camera::start(data, "gba", &rom, true).expect("on");
    cam.set_screen(1080);
    assert!(wait_for(|| cam.armed()), "a verified game arms the camera");
    // The clock alone: compared, no picture asked for.
    cam.saved(&save(1, 4));
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!cam.wants_picture());
    // The second badge: one picture, one photo.
    cam.saved(&save(2, 4));
    assert!(wait_for(|| cam.wants_picture()), "a moment asks the game's thread for its picture");
    cam.picture(picture());
    assert!(wait_for(|| cam.taken() == 1));
    // The moment over the game: the photo's stages at the screen's size, once.
    let moment = cam.take_moment().expect("the moment over the game");
    assert_eq!((moment.w, moment.h), (crate::album_eject::scale(1080) * frame_w((240, 160)), crate::album_eject::scale(1080) * FRAME.1));
    assert!(cam.take_moment().is_none());
    let photos = list(&dir_of(data, &rom));
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].note, vec!["Badge 2".to_string(), "4 h 12 min".to_string()]);
    assert_eq!(photos[0].size, (240, 160));
    let img = crate::image::load_jpeg(&photos[0].file).expect("a JPEG that reads back");
    assert_eq!((img.w, img.h), (240, 160));
    let (top, bottom) = (img.px[240 * 20 + 100], img.px[240 * 140 + 100]);
    assert!((top >> 8 & 0xff) > 0x60 && (bottom >> 16 & 0xff) > 0xb0, "the picture as the game showed it: {top:06x} {bottom:06x}");
    // An older save loaded and the badge won again: no second photo.
    cam.saved(&save(1, 5));
    cam.saved(&save(2, 6));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!cam.wants_picture());
    assert_eq!(list(&dir_of(data, &rom)).len(), 1);
}

fn dir_of(data: &Path, rom: &Path) -> PathBuf {
    dir(data, "gba", rom)
}

/// The game ends right after the save that holds the step: `finish` compares it and keeps the photo before it returns,
/// and with nothing new it returns at once.
#[test]
fn the_save_at_the_games_end_is_compared_before_it_quits() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path();
    let save = |badges| gen3_save(Emerald, &[("TORCHIC", 25, false)], badges, (3, 0), 7);
    let rom = game(data, Emerald, &save(1));
    let cam = Camera::start(data, "gba", &rom, true).unwrap();
    assert!(wait_for(|| cam.armed()));
    let t = std::time::Instant::now();
    cam.finish(&save(1), || Some(picture()), std::time::Duration::from_secs(5));
    assert!(t.elapsed() < std::time::Duration::from_secs(1), "nothing new: no wait ({:?})", t.elapsed());
    cam.finish(&save(3), || Some(picture()), std::time::Duration::from_secs(5));
    assert_eq!(list(&dir_of(data, &rom)).len(), 1, "kept before finish returned");
}

/// A game whose save isn't read (not a verified release, or unknown to the caches) never arms the camera: nothing is
/// copied for it, and Off makes no camera at all.
#[test]
fn an_unread_game_or_the_switch_off_takes_no_photo() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Camera::start(dir.path(), "gba", Path::new("/nowhere.gba"), false).is_none());
    let cam = Camera::start(dir.path(), "gba", Path::new("/nowhere.gba"), true).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!cam.armed());
    cam.saved(b"anything");
    assert!(!cam.wants_picture());
}

/// The session's limit: five steps in one session make five photos, the sixth none.
#[test]
fn a_session_adds_five_photos_at_most() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path();
    let save = |badges| gen1_save(&[("PIKACHU", 20)], badges, (3, 0));
    let rom = game(data, Gen1, &save(0));
    let cam = Camera::start(data, "gba", &rom, true).unwrap();
    assert!(wait_for(|| cam.armed()));
    let mut bits = 0u8;
    for i in 0..7 {
        bits |= 1 << i;
        cam.saved(&save(bits));
        if wait_for(|| cam.wants_picture() || cam.taken() == PER_SESSION) && cam.wants_picture() {
            cam.picture(picture());
            let n = i + 1;
            assert!(wait_for(|| cam.taken() == n));
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(cam.taken(), PER_SESSION);
    assert_eq!(list(&dir_of(data, &rom)).len(), PER_SESSION);
}

/// A big picture (a PlayStation's 640 x 480) is halved until it is at most 320 wide, and stays small on the card.
#[test]
fn big_pictures_are_halved_and_small() {
    let (w, h) = (640u32, 480u32);
    let rgb: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i / 640 % 256) as u8, 90]).collect();
    let (jpeg, size) = encode(w, h, &rgb).unwrap();
    assert_eq!(size, (320, 240));
    assert!(jpeg.len() < 60_000, "{}", jpeg.len());
}
