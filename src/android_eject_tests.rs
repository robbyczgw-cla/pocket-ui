//! Android's way back from a game (`android/src/eject.rs`) on a host, with the launcher's own page, start scene and way
//! back: what a player sees from Save & quit to the shelf, frame by frame at 60 Hz, on the Odin 2 Mini's screen and a
//! phone held upright. The wiring into the app (`lib.rs`: the player's note on return, any input ending it) is proven on
//! the device (ANDROID-EJECT.md); here the pictures and the page's state.
#![allow(dead_code)]
fn data_dir() -> std::path::PathBuf {
    thread_local! {static DATA: tempfile::TempDir = tempfile::tempdir().unwrap();}
    DATA.with(|d| d.path().to_path_buf())
}
#[path = "../android/src/play_cards.rs"]
mod play_cards;
#[path = "../android/src/ui.rs"]
mod ui;
#[path = "../android/src/insert.rs"]
mod insert;
#[path = "../android/src/eject.rs"]
mod eject;
fn bundled_core(system: &str) -> Option<&'static str> {
    static CORES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    CORES.get_or_init(|| serde_json::from_str(include_str!("../android/cores.lock.json")).unwrap())["cores"]
        .as_array()?
        .iter()
        .find(|c| c["systems"].as_array().is_some_and(|s| s.iter().any(|v| v.as_str() == Some(system))))?["id"]
        .as_str()
}

use pocket_ui::{
    document_library::Game,
    eject::{LastPicture, Moment, Screen},
    gfx::Canvas,
    look_settings::Theme,
    motion::Motion,
    pad::Button,
    text::Text,
};
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_micros(16_667);
/// The Odin 2 Mini (1920 x 1080 at 2.0 dp per pixel... the launcher's own measure) and a phone held upright.
const SCREENS: [((u32, u32), f32); 2] = [((1920, 1080), 2.0), ((1080, 2340), 2.6)];

fn game(system: &'static str, relative: &str, name: &str) -> Game {
    Game {
        name: name.into(), key: format!("test:{relative}"), uri: format!("content://test/{relative}"), relative: relative.into(),
        system: system, system_id: system, profile: "retroarch:test", color: 0x3060a0, size: 1 << 20, document_size: 1 << 20,
        missing: vec![], covers: vec![], pictures: vec![], cheats: String::new(), parts: vec![],
    }
}

/// The launcher on the Games page, a controller in hand, `played` among other games and selected: as after its start.
fn shelf(played: &Game, theme: Theme, (size, dp): ((u32, u32), f32)) -> ui::Ui {
    let mut u = ui::Ui::new(ui::Preferences { onboarding_done: true, language: Some(pocket_ui::i18n::Lang::En), theme, ..Default::default() });
    u.dp = dp;
    u.touch_mode = false;
    let mut games = vec![game("snes", "snes/Another.sfc", "Another Game"), game("gba", "gba/Third.gba", "Third Game")];
    games.insert(1, played.clone());
    u.install(ui::library(games));
    let at = u.library.as_ref().unwrap().games.iter().position(|g| g.key == played.key).unwrap();
    for _ in 0..8 {
        if u.visible.get(u.selected) == Some(&at) {
            break;
        }
        u.press(Button::Right);
    }
    assert_eq!(u.game().map(|g| g.key.clone()), Some(played.key.clone()), "the played game is selected");
    page(&mut u, size);
    u
}

fn page(u: &mut ui::Ui, (w, h): (u32, u32)) -> Vec<u32> {
    let mut px = vec![0; (w * h) as usize];
    u.paint(&mut Canvas::new(&mut px, w, h), &mut Text::builtin(), None);
    px
}

/// The player's note at Save & quit: a game picture (blue with a bright square) standing in the middle of the window.
fn note(played: &Game, (w, h): (u32, u32), picture: bool) -> LastPicture {
    let path = data_dir().join(format!("{}-last.png", played.system_id));
    if picture {
        let (pw, ph) = (256u32, 224u32);
        let mut e = png::Encoder::new(std::fs::File::create(&path).unwrap(), pw, ph);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..pw * ph).flat_map(|i| if (i % pw).abs_diff(pw / 2) < 20 && (i / pw).abs_diff(ph / 2) < 20 { [250, 250, 250] } else { [40, 90, 200] }).collect();
        e.write_header().unwrap().write_image_data(&px).unwrap();
    }
    let gh = h as i32 * 9 / 10;
    let gw = gh * 8 / 7;
    LastPicture {
        key: ui::game_key(played, false), picture: picture.then_some(path), rect: Some(((w as i32 - gw) / 2, (h as i32 - gh) / 2, gw, gh)),
        window: (w, h), saved: true, at_ms: 0,
    }
}

/// What the launcher does when it is back (`App::begin_eject`): the page under the way back, then the way back.
fn begin(u: &mut ui::Ui, played: &Game, size: (u32, u32), note: &LastPicture, at: Instant) -> Option<eject::Eject> {
    let on_shelf = u.shows_case();
    if eject::why_not(u.prefs.cozy.unwrap_or(true), u.prefs.motion, on_shelf, u.game().is_some_and(|g| g.key == played.key)).is_some() {
        return None;
    }
    u.back_from(&played.key, note.saved);
    u.launching = true;
    u.returning = true;
    let pixels = page(u, size);
    u.launching = false;
    u.returning = false;
    let Some(case) = u.front_at.take() else {
        u.ajar = None;
        return None;
    };
    let page = eject::Page { pixels, size, window: size, offset: (0, 0), scale: (1.0, 1.0) };
    Some(eject::begin(played, case, page, note, None, false, &mut Text::builtin(), at))
}

/// The page `since` its case came home (`Ui::case_home`): its own clock, set as far back.
fn page_since(u: &mut ui::Ui, key: &str, size: (u32, u32), since: Duration) -> Vec<u32> {
    u.case_home(key, Instant::now().checked_sub(since).unwrap());
    page(u, size)
}

/// Mean difference of two pictures of the same size, 0-255 per colour channel (as tests/eject.rs).
fn diff(a: &[u32], b: &[u32]) -> f64 {
    let ch = |p: u32, s: u32| (p >> s & 0xff) as i64;
    a.iter().zip(b).map(|(x, y)| (0..3).map(|i| (ch(*x, i * 8) - ch(*y, i * 8)).unsigned_abs()).sum::<u64>()).sum::<u64>() as f64 / (a.len() * 3) as f64
}

/// The systems of the handhelds' test, the consoles with their own scene since 11.11., and one without any art.
const SYSTEMS: [(&str, &str, &str); 15] = [
    ("snes", "snes/Chrono Trigger (USA).sfc", "Chrono Trigger"), ("n64", "n64/Super Mario 64 (USA).z64", "Super Mario 64"),
    ("gb", "gb/Tetris (World).gb", "Tetris"), ("gba", "gba/Golden Sun (USA, Europe).gba", "Golden Sun"),
    ("psx", "psx/Final Fantasy VII (USA).chd", "Final Fantasy VII"), ("saturn", "saturn/Fixture Disc.chd", "Fixture Disc"),
    ("dreamcast", "dreamcast/Fixture Disc.chd", "Fixture Disc"), ("nes", "nes/Fixture (USA).nes", "Fixture"),
    ("gbc", "gbc/Fixture (USA).gbc", "Fixture"), ("psp", "psp/Fixture (USA).iso", "Fixture"),
    ("mastersystem", "mastersystem/Fixture (USA).sms", "Fixture"), ("gamegear", "gamegear/Fixture (World).gg", "Fixture"),
    ("pcengine", "pcengine/Fixture (Japan).pce", "Fixture"), ("3ds", "3ds/Fixture (USA).3ds", "Fixture"),
    ("unknown-console", "unknown-console/Missing Art.bin", "Missing Art"),
];

/// The way back ends on the still shelf's own picture and nothing jumps there or after it, while what is inside comes
/// out of the case (the handhelds' jump of 11.11.: the shelf's own animation started anew at the end). Every 60 Hz frame
/// from the way back's last moments to 0.7 s after it, on both screens, both looks with cases, every kind of medium.
/// The way back is played backwards, so it ends moving (the page's dimming lifts last, fastest at the end): the step
/// onto the shelf goes on as the steps before it (a quarter more at most; before the fix it was two and a half times
/// the step before it on a phone held upright), and the scene's end is the shelf's picture itself (also the start's first frame: at
/// A on a phone held upright the console stood visible at the bottom, under the screen only on the handhelds).
#[test]
fn the_way_back_hands_over_to_the_still_shelf_without_a_jump() {
    // Mean per colour channel: the same picture within rounding, and the case's game sliding out of it after.
    const SAME: f64 = 0.2;
    const STILL: f64 = 0.5;
    const AFTER: f64 = 1.5;
    let mut failures = Vec::new();
    for (system, relative, name) in SYSTEMS {
        let played = game(system, relative, name);
        for theme in [Theme::Shelf, Theme::Cabinet] {
            for screen in SCREENS {
                let size = screen.0;
                let mut u = shelf(&played, theme, screen);
                let t0 = Instant::now();
                let mut ej = begin(&mut u, &played, size, &note(&played, size, true), t0).unwrap_or_else(|| panic!("{system} {theme:?}: no way back"));
                let len = ej.way.len();
                let n = (len.as_secs_f64() / FRAME.as_secs_f64()).ceil() as u32;
                let mut buf = vec![0; (size.0 * size.1) as usize];
                // Late in the way back the scene stands over the page: something other than the still page.
                ej.paint(&mut buf, t0 + len.mul_f32(0.75));
                let mid = buf.clone();
                let mut frames = Vec::new();
                for k in n.saturating_sub(4)..n {
                    assert!(!ej.done(t0 + FRAME * k), "{system}: over before its length");
                    ej.paint(&mut buf, t0 + FRAME * k);
                    frames.push(buf.clone());
                }
                assert!(ej.done(t0 + FRAME * n));
                ej.paint(&mut buf, t0 + FRAME * n);
                let end = buf.clone();
                // Home at the frame it ended on, then the still shelf's own frames.
                for k in 0..42u32 {
                    frames.push(page_since(&mut u, &played.key, size, FRAME * k));
                }
                let still = frames.last().unwrap();
                assert!(diff(&mid, still) > 1.0, "{system} {theme:?} {size:?}: the way back shows nothing over the page");
                let rest = diff(&end, &frames[4]);
                let steps: Vec<f64> = frames.windows(2).map(|p| diff(&p[0], &p[1])).collect();
                let hand = steps[3];
                let after = steps[4..].iter().cloned().fold(0.0, f64::max);
                if rest > SAME || hand > (steps[2] * 1.25).max(STILL) || after > AFTER {
                    failures.push(format!("{system} {theme:?} {size:?}: scene's end against the shelf {rest:.2}, last steps {:.2?}, hand-over {hand:.2}, worst after {after:.2}", &steps[..3]));
                }
            }
        }
    }
    assert!(failures.is_empty(), "the case jumps when the way back ends:\n{}", failures.join("\n"));
}

/// A TV game pulls together to a bright line, a handheld's display goes dark, then black lifts off the page with the
/// console up; without a picture (or a TV without the old-TV switch-on) it starts from black.
#[test]
fn a_tv_picture_pulls_together_to_a_line_and_a_handheld_display_goes_dark() {
    let size = (1920, 1080);
    for (system, relative, name, screen) in [("snes", "snes/A.sfc", "A", Screen::Tv), ("gba", "gba/B.gba", "B", Screen::Lcd)] {
        let played = game(system, relative, name);
        let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
        let t0 = Instant::now();
        let mut ej = begin(&mut u, &played, size, &note(&played, size, true), t0).unwrap();
        assert_eq!((ej.way.screen, ej.way.off), (screen, true), "{system}");
        let mut buf = vec![0; (size.0 * size.1) as usize];
        let lit = |buf: &[u32]| buf.iter().filter(|p| (**p & 0xff) > 0x60).count();
        ej.paint(&mut buf, t0);
        let first = lit(&buf);
        assert!(first > 100_000, "{system}: the game's last picture first ({first})");
        assert_eq!(ej.margin(t0), 0, "black round it");
        let middle = t0 + Duration::from_millis(match screen { Screen::Tv => 190, Screen::Lcd => 200 });
        ej.paint(&mut buf, middle);
        let row = |y: u32| (0..size.0).filter(|x| buf[(y * size.0 + x) as usize] & 0xff > 0xe0).count();
        match screen {
            Screen::Tv => assert!(row(540) > 600 && row(400) == 0, "{system}: a bright line across the middle ({} {})", row(540), row(400)),
            Screen::Lcd => assert!(lit(&buf) < first / 4, "{system}: the display is going dark ({})", lit(&buf)),
        }
        let lifting = match screen { Screen::Tv => pocket_ui::eject::TV_OFF, Screen::Lcd => pocket_ui::eject::LCD_OFF } + pocket_ui::eject::FADE / 2;
        let off = match ej.moment(t0 + lifting) {
            Moment::Shelf { black, .. } => black,
            m => panic!("{system}: {m:?}"),
        };
        assert!(off > 0.0 && off < 1.0, "{system}: black lifting off the page ({off})");
    }
    // No picture came (the player had no frame yet): from black, no old TV over nothing.
    let played = game("snes", "snes/C.sfc", "C");
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    let ej = begin(&mut u, &played, size, &note(&played, size, false), Instant::now()).unwrap();
    assert!(!ej.way.off && matches!(ej.moment(ej.at), Moment::Shelf { black, .. } if black == 1.0));
}

/// Quick, Motion Reduced or Off, the list look, a page without the game's case: straight back to the page, the case in
/// its place and open at once, as before. The settings are the ones there are; no new row.
#[test]
fn quick_motion_below_full_and_pages_without_a_case_go_straight_back() {
    let size = (1920, 1080);
    let played = game("snes", "snes/A.sfc", "A");
    let note = note(&played, size, true);
    let still = |u: &mut ui::Ui| {
        assert!(u.ajar.is_none(), "nothing held in its case");
        assert_eq!(u.ajar_out(&played.key, Instant::now()), 1.0);
    };
    for (cozy, motion) in [(Some(false), Motion::Full), (Some(true), Motion::Reduced), (Some(true), Motion::Off)] {
        let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
        u.prefs.cozy = cozy;
        u.prefs.motion = motion;
        assert!(begin(&mut u, &played, size, &note, Instant::now()).is_none(), "{cozy:?} {motion:?}");
        still(&mut u);
    }
    // The list look shows no case to come home to, as the start (no insert there either).
    let mut u = shelf(&played, Theme::Split, (size, 2.0));
    assert!(begin(&mut u, &played, size, &note, Instant::now()).is_none(), "list look");
    still(&mut u);
    // Back on another game's page, or the game card in front: not this game's way back.
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    u.selected = (u.selected + 1) % u.visible.len();
    page(&mut u, size);
    assert!(begin(&mut u, &played, size, &note, Instant::now()).is_none(), "another game selected");
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    u.press(Button::North);
    assert!(u.screen == ui::Screen::Box);
    assert!(begin(&mut u, &played, size, &note, Instant::now()).is_none(), "the game card");
    // Motion Reduced in the middle of the slide out of the case: it stands out at once.
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    u.back_from(&played.key, true);
    u.case_home(&played.key, Instant::now());
    u.prefs.motion = Motion::Reduced;
    page(&mut u, size);
    assert_eq!(u.ajar_out(&played.key, Instant::now()), 1.0);
    assert!(!u.ajar_moving(Instant::now()));
}

/// Any button or touch ends the way back at once (`App::skip_eject`: the case is home that moment) and the page under it
/// is the live shelf: it has its places to tap and the press moves on from the game just played, from the first frame
/// and at any moment of the way back.
#[test]
fn the_shelf_under_the_way_back_takes_a_press_at_any_moment() {
    for screen in SCREENS {
        let size = screen.0;
        let played = game("psx", "psx/Final Fantasy VII (USA).chd", "Final Fantasy VII");
        for ms in [0u64, 100, 400, 900, 1500] {
            let mut u = shelf(&played, Theme::Shelf, screen);
            let t0 = Instant::now();
            let ej = begin(&mut u, &played, size, &note(&played, size, true), t0).unwrap();
            assert!(!ej.done(t0 + Duration::from_millis(ms)), "{ms} ms: still on its way");
            // The page painted under it has what a tap needs: the cases and the game's Continue.
            assert!(u.regions.iter().any(|r| r.label == "Final Fantasy VII"), "{ms} ms: the case can be tapped: {:?}", u.regions.iter().map(|r| r.label.clone()).collect::<Vec<_>>());
            assert!(u.regions.iter().any(|r| r.label == "Play"), "{ms} ms: its Play button");
            // A press now: the case is home, the press does what it does on the shelf.
            u.case_home(&played.key, t0 + Duration::from_millis(ms));
            let before = u.game().unwrap().key.clone();
            u.press(Button::Right);
            assert_ne!(u.game().unwrap().key, before, "{ms} ms: Right moves on from the game");
            u.press(Button::Left);
            assert_eq!(u.game().unwrap().key, before);
            // And the case stands there again, open: what is inside comes out at once.
            let px = page(&mut u, size);
            let ((x, y, w, h), _) = {
                u.launching = true;
                page(&mut u, size);
                u.launching = false;
                u.front_at.take().unwrap()
            };
            let mut empty = 0;
            for yy in y..y + h {
                for xx in x..x + w {
                    empty += (px[(yy as u32 * size.0 + xx as u32) as usize] == pocket_ui::palette::pal().void) as u32;
                }
            }
            assert!(empty < (w * h) as u32 / 2, "{ms} ms: the case is drawn in its place ({empty} of {} empty)", w * h);
        }
    }
}

/// The game just saved shows Continue and its open case from the first frame back, before the read of the files says
/// so; what is inside stays in while the case comes home, then comes out on its own clock and stops.
#[test]
fn the_case_comes_home_shut_then_its_game_comes_a_little_out() {
    let size = (1920, 1080);
    let played = game("gba", "gba/Golden Sun (USA, Europe).gba", "Golden Sun");
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    assert!(!u.is_frozen(&played.key));
    let t0 = Instant::now();
    let _ej = begin(&mut u, &played, size, &note(&played, size, true), t0).unwrap();
    assert!(u.is_frozen(&played.key), "Save & quit: it stands open at once");
    assert_eq!(u.ajar_out(&played.key, t0 + Duration::from_secs(5)), 0.0, "inside while it is on its way");
    assert!(!u.ajar_moving(t0 + Duration::from_secs(5)));
    u.case_home(&played.key, t0);
    assert!(u.ajar_moving(t0 + Duration::from_millis(100)) && u.ajar_out(&played.key, t0 + Duration::from_millis(100)) > 0.0);
    assert_eq!(u.ajar_out(&played.key, t0 + Duration::from_millis(400)), 1.0);
    assert!(!u.ajar_moving(t0 + Duration::from_millis(400)), "and stops: no frames for nothing");
    // The same files read a moment later change nothing; a read that differs makes the shelf again.
    assert!(!u.frozen_got(vec![played.key.clone()]));
    assert!(u.frozen_got(vec![]));
    // "Return to library" after a failed load saved nothing: not open because of it.
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    let mut n = note(&played, size, false);
    n.saved = false;
    begin(&mut u, &played, size, &n, Instant::now()).unwrap();
    assert!(!u.is_frozen(&played.key));
}


/// 12.11. (Alex on the Odin 2 Mini: "Cart kommt von unten rein", "clippt durch Konsole"): the start's console stood
/// in the screen's middle over the page's case, half as large again as on the handhelds and cut off at the bottom, and
/// the medium left the case behind it and went up into its slot. For every system with a scene, both looks with a case,
/// the Odin (with and without its bars) and a phone held upright and across, with the launcher's own page: the console
/// stands whole on the screen and low, and the medium hangs over its slot at most a little higher than where it leaves the
/// case (lifted over the slot by a fifth of the screen at most, as a hand would; 6-8 % on the Odin, 15 % for the tall
/// Game Boy with the Odin's bars shown), never up out of the console from the screen's middle (39 % on the Odin before). Missed before: the start was only checked for staying on
/// the screen.
#[test]
fn the_medium_comes_down_into_a_console_standing_whole_on_the_screen() {
    use pocket_ui::{insert_rig as ir, insert_scene, media};
    const GROUND: u32 = 0xff12_3456;
    let screens = [((1920, 1080), 2.0), ((1782, 951), 2.30625), ((1080, 2340), 2.6), ((2340, 1080), 2.6)];
    for (system, relative, name) in SYSTEMS {
        let played = game(system, relative, name);
        for theme in [Theme::Shelf, Theme::Cabinet] {
            for (size, dp) in screens {
                let (w, h) = (size.0 as i32, size.1 as i32);
                let mut u = shelf(&played, theme, (size, dp));
                u.launching = true;
                page(&mut u, size);
                u.launching = false;
                let (from, front) = u.front_at.take().unwrap();
                let mut launch = insert::Launch::new(&played, front, from, vec![GROUND; (w * h) as usize], size, size, None, false, true, &mut Text::builtin());
                let mut buf = vec![0; (w * h) as usize];
                // The medium is in, the case gone: the console as the fade begins.
                launch.paint_scene(&mut buf, Duration::from_millis(1500));
                let ground = buf[0];
                let (mut l, mut t, mut r, mut b) = (w, h, -1, -1);
                for (i, p) in buf.iter().enumerate() {
                    if *p != ground {
                        let (x, y) = (i as i32 % w, i as i32 / w);
                        (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
                    }
                }
                let at = format!("{system} {theme:?} on {w}x{h}");
                assert!(r >= 0, "{at}: no console");
                assert!(l > 0 && t > 0 && r < w - 1 && b < h - 8, "{at}: the console whole on the screen, {l},{t}..{r},{b}");
                assert!(b >= h * 3 / 4, "{at}: the console low on the screen, under the page's case ({t}..{b})");
                let kind = media::Game::of(insert::rig_system(&played), &played.relative);
                if let Some(f) = ir::files(insert::rig_system(&played), kind) {
                    let base = pocket_ui::system_art::decode(&f.base).map(|p| (p.w as i32, p.h as i32)).unwrap();
                    let (blocks, stage) = insert_scene::phone_stage(Some(&f.rig), base, (w, h));
                    let hover = insert_scene::hover_at(&f.rig, base, blocks, stage, w);
                    let leaves = from.1 as f32 + from.3 as f32 / 2.0;
                    assert!(hover.1 >= leaves - 0.2 * h as f32, "{at}: the medium goes up into its console ({leaves} -> {})", hover.1);
                }
            }
        }
    }
}

/// 12.11. (Alex: "slow"): the player was told to come when the start's fade had run, and the Odin 2 Mini then stayed
/// black for its window, core and first picture (214-580 ms), where Linux shows the game as the fade ends. It is told a
/// player's start earlier now; never before the medium is in, so even the quickest player's first picture comes after a
/// lid or tray has shut. The quick way still lets it come at once. Missed before: the hold was only checked against the
/// fade, not against the player's own start.
#[test]
fn the_player_is_told_to_come_a_player_s_start_before_the_fade_ends() {
    use pocket_ui::{cabinet, insert_rig as ir, insert_scene};
    let played = game("snes", "snes/Chrono Trigger (USA).sfc", "Chrono Trigger");
    let size = (1920, 1080);
    let release_at = |cozy: bool| {
        let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
        u.launching = true;
        let pixels = page(&mut u, size);
        u.launching = false;
        let (from, front) = u.front_at.take().unwrap();
        let mut launch = insert::Launch::new(&played, front, from, pixels, size, size, None, false, cozy, &mut Text::builtin());
        (0..200u64).map(|ms| Duration::from_millis(ms * 10)).find(|d| launch.take_release(launch.at + *d)).unwrap()
    };
    let hold = insert_scene::hold(true).unwrap();
    let at = release_at(true);
    assert_eq!(at, hold - insert::PLAYER_START, "a player's start before the fade has run");
    assert!(at >= cabinet::IN_AT_COZY, "never before the medium is in");
    // The quickest start measured on the Odin 2 Mini (GBA, 214 ms): its first picture after a lid or tray has shut.
    assert!(at + Duration::from_millis(214) >= cabinet::IN_AT_COZY + ir::SHUT_AFTER);
    assert_eq!(release_at(false), Duration::ZERO, "the quick way: at once");
}

/// 12.11.: Android pauses the launcher as the player starts, and winit asks it for no frame then; it paints the start's
/// fade itself to its end (`App::insert_tick`). Its black end is painted once, and once more only to say "Starting":
/// black painted over and over at 60 Hz took time from the player starting (PS1 440 -> 581 ms to its first picture).
#[test]
fn the_start_s_black_end_is_painted_once_and_again_only_to_say_starting() {
    let played = game("snes", "snes/Chrono Trigger (USA).sfc", "Chrono Trigger");
    let size = (1920, 1080);
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    u.launching = true;
    let pixels = page(&mut u, size);
    u.launching = false;
    let (from, front) = u.front_at.take().unwrap();
    let mut launch = insert::Launch::new(&played, front, from, pixels, size, size, None, false, true, &mut Text::builtin());
    let (t0, ms) = (launch.at, |n: u64| Duration::from_millis(n));
    let mut buf = vec![0; (size.0 * size.1) as usize];
    let mut text = Text::builtin();
    // While it moves, every frame.
    for n in [0, 700, 1500] {
        assert!(launch.needs_frame(t0 + ms(n)), "{n} ms: still moving");
        launch.paint(&mut buf, &mut text, t0 + ms(n));
        assert!(launch.needs_frame(t0 + ms(n)), "{n} ms: still moving");
    }
    assert!(launch.take_release(t0 + ms(1360)));
    assert!(launch.needs_frame(t0 + ms(1700)), "the black end, not yet on screen");
    launch.paint(&mut buf, &mut text, t0 + ms(1700));
    assert!(buf.iter().all(|p| *p & 0xff_ffff == 0), "black");
    for n in [1717, 1900, 2400] {
        assert!(!launch.needs_frame(t0 + ms(n)), "{n} ms: black is on screen, nothing to paint");
    }
    // The player is slow to come: the screen says why, once.
    let starting = t0 + ms(1360) + Duration::from_millis(1300);
    assert!(launch.needs_frame(starting));
    launch.paint(&mut buf, &mut text, starting);
    assert!(buf.iter().any(|p| *p & 0xff_ffff != 0), "Starting");
    assert!(!launch.needs_frame(starting + ms(100)));
}

/// Not a check: `POCKETUI_ANDROID_RENDERS=<folder> cargo test -- --ignored android_insert_renders` writes the start and
/// the way back of the consoles with their own scene at moments along them, as the launcher paints them (raw RGB,
/// `<system>-<what>-<ms>.rgb`), for a contact sheet. `POCKETUI_ANDROID_SYSTEMS=snes,gba` picks systems (all by default),
/// `POCKETUI_ANDROID_SCREEN=1920x1080x2.0` the screen and its dp, `POCKETUI_ANDROID_STEP=17` the step in ms (every 60 Hz
/// frame), `POCKETUI_ANDROID_THEME=cabinet` the look (Shelf by default).
#[test]
#[ignore]
fn android_insert_renders() {
    let Some(out) = std::env::var_os("POCKETUI_ANDROID_RENDERS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&out).unwrap();
    let screen = std::env::var("POCKETUI_ANDROID_SCREEN").unwrap_or_else(|_| "1920x1080x2.0".into());
    let parts: Vec<&str> = screen.split('x').collect();
    let size: (u32, u32) = (parts[0].parse().unwrap(), parts[1].parse().unwrap());
    let dp: f32 = parts.get(2).map_or(2.0, |d| d.parse().unwrap());
    let step: u64 = std::env::var("POCKETUI_ANDROID_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(150);
    let theme = if std::env::var("POCKETUI_ANDROID_THEME").is_ok_and(|t| t == "cabinet") { Theme::Cabinet } else { Theme::Shelf };
    let wanted = std::env::var("POCKETUI_ANDROID_SYSTEMS").ok();
    let save = |name: String, px: &[u32]| std::fs::write(out.join(name), image_rgb(px)).unwrap();
    for (system, relative, name) in SYSTEMS.iter().copied().filter(|s| wanted.as_deref().is_none_or(|w| w.split(',').any(|x| x == s.0))) {
        let played = game(system, relative, name);
        // The start: the page with the case taken off it, as `App::begin_insert`.
        let mut u = shelf(&played, theme, (size, dp));
        u.launching = true;
        let pixels = page(&mut u, size);
        u.launching = false;
        let (from, front) = u.front_at.take().unwrap();
        let mut launch = insert::Launch::new(&played, front, from, pixels, size, size, None, false, true, &mut Text::builtin());
        let mut buf = vec![0; (size.0 * size.1) as usize];
        for ms in (0..=1600u64).step_by(step as usize) {
            launch.paint_scene(&mut buf, Duration::from_millis(ms));
            save(format!("{system}-in-{ms:04}.rgb"), &buf);
        }
        // The way back.
        let mut u = shelf(&played, theme, (size, dp));
        let t0 = Instant::now();
        let mut ej = begin(&mut u, &played, size, &note(&played, size, true), t0).unwrap();
        let len = ej.way.len().as_millis() as u64;
        for ms in (0..=len).step_by(step as usize) {
            ej.paint(&mut buf, t0 + Duration::from_millis(ms));
            save(format!("{system}-out-{ms:04}.rgb"), &buf);
        }
    }
}

fn image_rgb(px: &[u32]) -> Vec<u8> {
    px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect()
}

/// Back from the game, the first frame is the last picture beginning to switch off: made from the note alone, before
/// the page and the scene (`App::eject_scene` makes them after it), so the way back is on screen as soon as the page
/// would be. Without its scene yet (or a picture), what would be the page is black, never a half-made page.
#[test]
fn the_first_frames_need_no_page() {
    let size = (1920, 1080);
    let played = game("snes", "snes/A.sfc", "A");
    let n = note(&played, size, true);
    let frame = n.picture.as_deref().and_then(pocket_ui::image::load_png);
    let t0 = Instant::now();
    let mut ej = eject::Eject::new(&played, frame, n.area_in(size, (0, 0)), size, size, t0);
    assert!(ej.needs_scene() && ej.way.off);
    let mut buf = vec![0x123456; (size.0 * size.1) as usize];
    ej.paint(&mut buf, t0);
    assert!(buf.iter().filter(|p| (**p & 0xff) > 0x60).count() > 100_000, "the game's picture, from the note alone");
    assert_eq!(ej.margin(t0), 0);
    // Past the switching off with no scene: black, not a stale or half-made page.
    ej.paint(&mut buf, t0 + pocket_ui::eject::TV_OFF + Duration::from_millis(50));
    assert!(buf.iter().all(|p| *p == 0));
    // With it, the page with the console up under black lifting.
    let mut u = shelf(&played, Theme::Shelf, (size, 2.0));
    let with = begin(&mut u, &played, size, &n, t0).unwrap();
    assert!(!with.needs_scene());
}
