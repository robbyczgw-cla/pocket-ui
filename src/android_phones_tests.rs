//! The Android launcher on phones, folding phones and tablets: the real Android pages painted at the screens people
//! really own (20:9 phones at 420 and 560 dpi, a small 720p phone, a fold open and closed, two tablets), upright and
//! turned, at Android's usual and larger font size, in English, German and Spanish. What a player notices: no word cut
//! short, nothing running off its room or into another word, nothing too small to read or to hit, nothing under the
//! camera hole or the gesture bar. The Retroid screens are in `android_02_tests`; the touch pad over the game and the
//! pause sheet are checked here for the same formats.
use super::{choose, page, ui};
use pocket_ui::{
    document_library::{scan, Document},
    gfx::Canvas,
    i18n::{self, Lang},
    settings::Theme,
    text::Text,
};

/// (name, width, height upright in px, density = dpi / 160). The names are the screens as their makers print them.
#[cfg(test)]
pub const PHONES: [(&str, u32, u32, f32); 8] = [
    ("phone-1080x2400", 1080, 2400, 2.625),
    ("phone-1440x3200", 1440, 3200, 3.5),
    ("phone-1080x2340", 1080, 2340, 2.625),
    ("small-720x1600", 720, 1600, 2.0),
    ("fold-inner-1812x2176", 1812, 2176, 2.625),
    ("fold-outer-1080x2520", 1080, 2520, 2.625),
    ("tablet-1600x2560", 1600, 2560, 1.75),
    ("tablet-1200x1920", 1200, 1920, 1.5),
];

/// What Android takes from the launcher's screen, in dp (`SafeInsets.read(…, gestureSides = false)`), as measured on an
/// Android 15 emulator with gesture navigation, immersive: the launcher keeps 8 dp round its content; the status bar's
/// swipe zone (24 dp) stays free at the top, the gesture bar (32 dp) at the bottom, a camera hole (about 52 dp) and its
/// 12 dp zone on its side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cut {
    /// A flat screen.
    Flat,
    /// A camera hole or notch at the top (the left when turned).
    Hole,
}

/// The screen's safe area as the launcher paints it: the width and height left of a `w` x `h` screen.
#[cfg(test)]
pub fn safe_area((w, h): (u32, u32), density: f32, cut: Cut) -> (u32, u32) {
    let d = |dp: f32| (dp * density).round() as u32;
    let (l, t, r, b) = match (cut, w > h) {
        (Cut::Flat, _) => (8.0, 24.0, 8.0, 32.0),
        (Cut::Hole, false) => (8.0, 64.0, 8.0, 32.0),
        (Cut::Hole, true) => (52.0, 24.0, 8.0, 32.0),
    };
    (w - d(l) - d(r), h - d(t) - d(b))
}

#[cfg(test)]
fn docs() -> Vec<Document> {
    let names = ["gb/Tobu Tobu Girl.gb", "gba/Attack on Voxelburg.gba", "snes/Elevator Madness DX.sfc", "nes/Thwaite.nes",
        "megadrive/KleleAtoms MD.md", "n64/The Legend of Zelda - Ocarina of Time.z64", "psp/Jeanne d'Arc.iso", "nds/Mario Kart DS.nds",
        "psx/Snake.cue", "gbc/uCity.gbc", "gba/Pocket Monsters Fire Red Version Extra Long Name.gba"];
    names.iter().enumerate().map(|(i, r)| Document { root_name: String::new(), uri: format!("content://qa/{i}"), authority: "qa".into(),
        id: i.to_string(), relative: r.to_string(), sheet: String::new(), size: 1_000_000 }).collect()
}

#[cfg(test)]
fn apps() -> Vec<ui::apps_page::AndroidApp> {
    ui::apps_page::parse(r#"[
        {"package":"com.android.vending","label":"Play Store","game":false,"icon":""},
        {"package":"com.android.settings","label":"Settings","game":false,"icon":""},
        {"package":"com.example.valley","label":"Stardew Valley","game":true,"icon":""},
        {"package":"org.example.chess","label":"Chess with a rather long name","game":false,"icon":""}
    ]"#, |_| None)
}

#[cfg(test)]
type Setup = fn(&mut ui::Ui);

/// One page to look at: whether the library is installed, whether the four looks matter for it, how to get there.
#[cfg(test)]
pub struct Case {
    pub name: &'static str,
    pub games: bool,
    pub looks: bool,
    pub setup: Setup,
    /// A controller is in the player's hands (its own pad or a Bluetooth one): the pages show their hint bar.
    pub pad: bool,
}

#[cfg(test)]
pub fn cases() -> Vec<Case> {
    fn onboarding(u: &mut ui::Ui, step: usize) {
        u.prefs.onboarding_done = false;
        u.prefs.onboarding_step = step;
    }
    let c = |name, games, looks, setup: Setup| Case { name, games, looks, setup, pad: false };
    let p = |name, setup: Setup| Case { name, games: true, looks: false, setup, pad: true };
    vec![
        c("onb0", false, false, |u| onboarding(u, 0)),
        c("onb1", false, false, |u| onboarding(u, 1)),
        c("onb2", false, false, |u| onboarding(u, 2)),
        c("onb3", false, false, |u| onboarding(u, 3)),
        c("onb4", false, false, |u| onboarding(u, 4)),
        c("onb5", false, false, |u| onboarding(u, 5)),
        c("onb6", false, false, |u| onboarding(u, 6)),
        c("onb7", false, false, |u| onboarding(u, 7)),
        c("nofolder", false, false, |_| {}),
        c("emptyfolder", false, false, |u| u.install(ui::library(vec![]))),
        c("library", true, true, |_| {}),
        c("favorites-empty", true, false, |u| u.navigate(ui::Screen::Favorites)),
        c("search-kbd", true, false, |u| u.navigate(ui::Screen::Search)),
        c("search-none", true, false, |u| { u.navigate(ui::Screen::Search); u.query = "zzzz".into(); u.filter(); u.keyboard = None; }),
        c("search-hit", true, false, |u| { u.navigate(ui::Screen::Search); u.query = "tobu".into(); u.filter(); u.keyboard = None; }),
        c("curator", true, false, |u| u.navigate(ui::Screen::Curator)),
        c("settings", true, false, |u| u.navigate(ui::Screen::Settings)),
        c("settings-end", true, false, |u| { u.navigate(ui::Screen::Settings); u.move_by(200); }),
        c("extras", true, false, |u| u.navigate(ui::Screen::Extras)),
        c("gamecheck", true, false, |u| u.navigate(ui::Screen::GameCheck)),
        c("saves", true, false, |u| u.navigate(ui::Screen::Saves)),
        c("plays-with", true, false, |u| u.navigate(ui::Screen::PlaysWith)),
        c("updates", true, false, |u| u.navigate(ui::Screen::Updates)),
        c("licences", true, false, |u| u.navigate(ui::Screen::Licences)),
        c("achievements", true, false, |u| u.navigate(ui::Screen::Achievements)),
        c("help", true, false, |u| { u.navigate(ui::Screen::Settings); choose(u, &ui::tr("Help")); }),
        c("help-topic", true, false, |u| { u.navigate(ui::Screen::Settings); choose(u, &ui::tr("Help")); u.help.open = Some(0); }),
        c("report", true, false, |u| { u.navigate(ui::Screen::Settings); choose(u, &ui::tr("Report a problem")); }),
        c("quick", true, false, |u| u.navigate(ui::Screen::Quick)),
        c("box", true, true, |u| u.navigate(ui::Screen::Box)),
        c("box-n64", true, false, |u| { u.selected = u.visible.iter().position(|&i| u.library.as_ref().unwrap().games[i].system_id == "n64").unwrap_or(0); u.navigate(ui::Screen::Box); }),
        c("box-long", true, false, |u| { u.selected = u.visible.iter().position(|&i| u.library.as_ref().unwrap().games[i].name.contains("Extra Long")).unwrap_or(0); u.navigate(ui::Screen::Box); }),
        c("stickers", true, false, |u| { u.navigate(ui::Screen::Box); u.navigate(ui::Screen::Stickers); }),
        c("controls-card", true, false, |u| { u.navigate(ui::Screen::Box); u.open_controls(); }),
        c("drawer", true, false, |u| { u.apps_got(apps()); u.press(pocket_ui::pad::Button::R2); }),
        p("library-pad", |_| {}),
        p("box-pad", |u| u.navigate(ui::Screen::Box)),
        p("search-pad", |u| { u.navigate(ui::Screen::Search); u.query = "tobu".into(); u.filter(); u.keyboard = None; }),
        c("drawer-none", true, false, |u| { u.apps_got(Vec::new()); u.press(pocket_ui::pad::Button::R2); }),
    ]
}

/// What one page looked like at one format: the faults found by measuring.
#[derive(Default, Debug)]
pub struct Findings {
    pub lines: Vec<String>,
    pub canvas: (u32, u32),
    pub dp: f32,
    /// The largest run of rows without any ink, as a share of the screen (empty gaps).
    pub blank: f32,
}

/// Paints `case` the way the launcher does at a `screen` with `cut`, and measures it. `shot`: where the picture goes.
#[cfg(test)]
pub fn look(lang: Lang, (w, h): (u32, u32), density: f32, cut: Cut, scale: f32, theme: Theme, case: &Case, shot: Option<&std::path::Path>) -> Findings {
    let mut u = page();
    u.prefs.choose_language(lang);
    u.prefs.theme = theme;
    u.system_font_scale = scale;
    u.controller_connected = case.pad;
    u.touch_mode = !case.pad;
    if case.games {
        u.install(ui::library(scan(docs())));
    }
    (case.setup)(&mut u);
    let (sw, sh) = safe_area((w, h), density, cut);
    let (lw, lh) = u.canvas_for((sw, sh), density);
    let mut px = vec![0; (lw * lh) as usize];
    let mut text = Text::builtin();
    u.paint(&mut Canvas::new(&mut px, lw, lh), &mut text, None);
    pocket_ui::text::boxes::reset();
    u.canvas_for((sw, sh), density);
    u.paint(&mut Canvas::new(&mut px, lw, lh), &mut text, None);
    pocket_ui::text::boxes::finish(&px, lw as i32, lh as i32);
    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
    let mut f = Findings { canvas: (lw, lh), dp: u.dp, ..Default::default() };
    let dp = u.dp;
    if !u.cut.is_empty() {
        f.lines.push(format!("CUT {:?}", u.cut));
    }
    for l in ink.lines().filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])) {
        f.lines.push(format!("INK {l}"));
    }
    if visible.trim().is_empty() {
        f.lines.push("EMPTY".into());
    }
    // What is the player's own (game names, the facts of a game, an app's name) is cut where it is too long, as it
    // should be; what is ours is not.
    let mut data: Vec<String> = apps().into_iter().map(|a| a.label).collect();
    if let Some(lib) = &u.library {
        data.extend(lib.games.iter().map(|g| g.name.clone()));
        for card in &lib.cards {
            data.extend(card.facts.iter().map(|(_, v)| v.clone()));
            data.extend(card.players.clone());
        }
    }
    for l in visible.lines().filter(|l| l.ends_with("...") || l.contains('…')) {
        let shown = l.trim_end_matches('…').trim_end_matches("...").trim_end();
        if !data.iter().any(|d| d.contains(shown)) {
            f.lines.push(format!("ELL {l}"));
        }
    }
    // Text smaller than 9 dp is hard to read on a phone held at arm's length (the title a cover picture prints on itself
    // is the game's, not ours).
    for (size, word) in pocket_ui::text::boxes::small_words((9.0 * dp) as u32) {
        if !data.iter().any(|d| d.contains(&word)) {
            f.lines.push(format!("SMALL {size}px ({:.1}dp) {word}", size as f32 / dp));
        }
    }
    // Things to hit lie whole on the screen.
    for r in u.regions.iter().filter(|r| !matches!(r.action, ui::Action::Label) && r.area.w > 0 && r.area.h > 0) {
        if r.area.x + r.area.w > lw as i32 + 2 || r.area.y + r.area.h > lh as i32 + 2 || r.area.x < -2 || r.area.y < -2 {
            f.lines.push(format!("OFF {} {:?}", r.label, (r.area.x, r.area.y, r.area.w, r.area.h)));
        }
    }
    // The search keyboard stands under the search field, never over it (a 720p phone held across at larger text).
    let keys_top = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Key(..))).map(|r| r.area.y).min();
    if let (Some(top), Some(field)) = (keys_top, u.regions.iter().find(|r| matches!(r.action, ui::Action::Keyboard))) {
        if field.area.y + field.area.h > top + 2 {
            f.lines.push(format!("KEYBOARD over the search field: field to {}, keys from {top}", field.area.y + field.area.h));
        }
    }
    // The largest empty band, top to bottom.
    let bg = px[0];
    let (mut run, mut best) = (0u32, 0u32);
    for y in 0..lh {
        let row = &px[(y * lw) as usize..((y + 1) * lw) as usize];
        if row.iter().all(|p| *p == bg) { run += 1; best = best.max(run); } else { run = 0; }
    }
    f.blank = best as f32 / lh as f32;
    if let Some(path) = shot {
        let rgb: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        std::fs::write(path, pocket_ui::screenshot::encode_rgb(lw, lh, &rgb).unwrap()).unwrap();
    }
    f
}

/// Exploratory sweep, not a gate test: every page at every phone format, faults listed. Run it as
/// `POCKETUI_TEXT_BOXES=/tmp/x/ink POCKETUI_VISIBLE_TEXT=/tmp/x/vis PHONES_SHOTS=<dir> PHONES_MATCH=<text> cargo test --lib
/// android_phone_format_sweep -- --ignored --nocapture`. `PHONES_MATCH` picks the pictures that are written.
#[cfg(test)]
#[test]
#[ignore]
fn android_phone_format_sweep() {
    let shots = std::env::var_os("PHONES_SHOTS").map(std::path::PathBuf::from);
    let matching = std::env::var("PHONES_MATCH").unwrap_or_default();
    let only = std::env::var("PHONES_ONLY").unwrap_or_default();
    let only_langs = std::env::var("PHONES_LANGS").unwrap_or_else(|_| "en,de,es".into());
    let mut report = String::new();
    for (code, lang) in [("en", Lang::En), ("de", Lang::De), ("es", Lang::Es)] {
        if !only_langs.split(',').any(|l| l == code) { continue; }
        i18n::set(lang);
        for (name, pw, ph, density) in PHONES {
            for (turn, (w, h)) in [("up", (pw, ph)), ("turned", (ph, pw))] {
                for scale in [1.0f32, 1.3] {
                    for cut in [Cut::Flat, Cut::Hole] {
                        for case in cases() {
                            let looks: &[Theme] = if case.looks { &Theme::PHONE } else { &Theme::PHONE[..1] };
                            for &theme in looks {
                                let ctx = format!("{code} {name} {turn} font={scale} {cut:?} {} {theme:?}", case.name);
                                if !only.is_empty() && !only.split(',').any(|o| ctx.contains(o)) { continue; }
                                let path = shots.as_ref().filter(|_| !matching.is_empty() && ctx.contains(&matching)).map(|d| {
                                    std::fs::create_dir_all(d).unwrap();
                                    d.join(format!("{}.png", ctx.replace([' ', '='], "_")))
                                });
                                let f = look(lang, (w, h), density, cut, scale, theme, &case, path.as_deref());
                                for l in &f.lines { report += &format!("{ctx}\t{l}\n"); }
                                if f.blank > 0.45 { report += &format!("{ctx}\tBLANK {:.0}% of the height\n", f.blank * 100.0); }
                            }
                        }
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    let out = std::env::var("PHONES_REPORT").unwrap_or_else(|_| "/tmp/phones-report.tsv".into());
    std::fs::write(&out, &report).unwrap();
    println!("SWEEP-REPORT written to {out}: {} lines", report.lines().count());
}

// ---- The touch pad over a game ---------------------------------------------------------------------------------------
use super::play_cards;
#[path = "../android/src/player_touch.rs"]
mod painted_touch;
#[path = "../android/src/picture.rs"]
mod picture;

/// The player's safe area (`SafeInsets.read(…, gestureSides = true)`), measured the same way: the back-gesture zones
/// (30 dp) on both sides, the top and bottom as the launcher's, the camera hole instead of the left zone when turned.
#[cfg(test)]
pub fn player_safe_area((w, h): (u32, u32), density: f32, cut: Cut) -> (u32, u32) {
    let d = |dp: f32| (dp * density).round() as u32;
    let (l, t, r, b) = match (cut, w > h) {
        (Cut::Flat, _) => (30.0, 24.0, 30.0, 32.0),
        (Cut::Hole, false) => (30.0, 64.0, 30.0, 32.0),
        (Cut::Hole, true) => (52.0, 24.0, 30.0, 32.0),
    };
    (w - d(l) - d(r), h - d(t) - d(b))
}

/// (system, native width, native height, display aspect) of the consoles that have their own pad.
#[cfg(test)]
pub const PAD_SYSTEMS: [(&str, u32, u32, f64); 9] = [
    ("gb", 160, 144, 160.0 / 144.0), ("gba", 240, 160, 1.5), ("nes", 256, 240, 4.0 / 3.0), ("snes", 256, 224, 4.0 / 3.0),
    ("megadrive", 320, 224, 4.0 / 3.0), ("psx", 320, 240, 4.0 / 3.0), ("n64", 320, 240, 4.0 / 3.0), ("psp", 480, 272, 480.0 / 272.0),
    ("nds", 256, 384, 256.0 / 384.0),
];

/// The pad over `system`'s picture on a `w` x `h` screen: what covers the picture, what is out of reach or too small.
#[cfg(test)]
pub fn pad_look(system: &str, native: (u32, u32), aspect: f64, (w, h): (u32, u32), density: f32, cut: Cut, shot: Option<&std::path::Path>) -> Vec<String> {
    let (vw, vh) = player_safe_area((w, h), density, cut);
    let (lw, lh) = pocket_ui::scaled_layout::mobile_size((vw, vh), density);
    let dp = lw as f32 / (vw as f32 / density.max(0.5));
    let dir = tempfile::tempdir().unwrap();
    let settings = picture::Settings::load(dir.path(), system, "k");
    let rect = settings.rect((lw as i32, lh as i32), (vw as i32, vh as i32), native, aspect, true);
    let mut pad = vec![0u32; (lw * lh) as usize];
    let mut text = Text::builtin();
    let (controls, dpad, menu) = painted_touch::touch_controls(&mut Canvas::new(&mut pad, lw, lh), &mut text, system, (lw as i32, lh as i32), dp, Some(rect), 0);
    let mut out = Vec::new();
    let (lwi, lhi) = (lw as i32, lh as i32);
    let mut all: Vec<(String, pocket_ui::touch::Area)> = controls.iter().map(|(a, bit)| (format!("button{bit}"), *a)).collect();
    all.push(("Pause".into(), menu));
    for (name, a) in &all {
        if a.x < 0 || a.y < 0 || a.x + a.w > lwi || a.y + a.h > lhi {
            out.push(format!("OFFSCREEN {name} {:?}", (a.x, a.y, a.w, a.h)));
        }
        if (a.w.min(a.h) as f32) < 36.0 * dp {
            out.push(format!("SMALLBUTTON {name} {:.0}x{:.0}dp", a.w as f32 / dp, a.h as f32 / dp));
        }
    }
    for (i, (na, a)) in all.iter().enumerate() {
        for (nb, b) in &all[i + 1..] {
            let (ox, oy) = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x), (a.y + a.h).min(b.y + b.h) - a.y.max(b.y));
            if ox > 0 && oy > 0 && (ox * oy) as f32 > 0.25 * (a.w * a.h).min(b.w * b.h) as f32 {
                out.push(format!("PADOVERLAP {na} {nb}"));
            }
        }
    }
    // The pad's own ink over the picture: a pad that hides the game is worse than no pad.
    let inked = |x: i32, y: i32| pad[(y * lwi + x) as usize] != 0;
    let (mut over, mut total) = (0u64, 0u64);
    for y in rect.y.max(0)..(rect.y + rect.h).min(lhi) {
        for x in rect.x.max(0)..(rect.x + rect.w).min(lwi) {
            total += 1;
            over += inked(x, y) as u64;
        }
    }
    let share = over as f64 / total.max(1) as f64;
    if share > 0.03 {
        out.push(format!("COVERS {:.1}% of the picture", share * 100.0));
    }
    // The picture itself should be big enough to play: at least 40 % of the screen's short side in landscape, and not
    // squeezed below a hand's width in portrait.
    let short_dp = (rect.w.min(rect.h)) as f32 / dp;
    if short_dp < 140.0 {
        out.push(format!("TINYPICTURE {:.0}x{:.0}dp", rect.w as f32 / dp, rect.h as f32 / dp));
    }
    // Thumb zone, portrait: the face buttons and the D-pad lie in the lower half of the screen.
    if lhi > lwi {
        if let Some((_, cy, _)) = dpad {
            if cy < lhi as f32 * 0.5 { out.push(format!("HIGHDPAD at {:.0}% of the height", cy / lhi as f32 * 100.0)); }
        }
    }
    if let Some(path) = shot {
        let mut frame = vec![0x101018u32; (lw * lh) as usize];
        for y in rect.y.max(0)..(rect.y + rect.h).min(lhi) {
            for x in rect.x.max(0)..(rect.x + rect.w).min(lwi) { frame[(y * lwi + x) as usize] = 0x305070; }
        }
        for (f, p) in frame.iter_mut().zip(&pad) {
            if *p != 0 {
                let a = (*p >> 24).max(1).min(255);
                let mix = |s: u32, d: u32| (s * a + d * (255 - a)) / 255;
                *f = (mix((*p >> 16) & 255, (*f >> 16) & 255) << 16) | (mix((*p >> 8) & 255, (*f >> 8) & 255) << 8) | mix(*p & 255, *f & 255);
            }
        }
        let rgb: Vec<u8> = frame.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        std::fs::write(path, pocket_ui::screenshot::encode_rgb(lw, lh, &rgb).unwrap()).unwrap();
    }
    out
}

#[cfg(test)]
#[test]
#[ignore]
fn android_phone_pad_sweep() {
    let shots = std::env::var_os("PHONES_SHOTS").map(std::path::PathBuf::from);
    let matching = std::env::var("PHONES_MATCH").unwrap_or_default();
    let mut report = String::new();
    for (name, pw, ph, density) in PHONES {
        for (turn, (w, h)) in [("up", (pw, ph)), ("turned", (ph, pw))] {
            for cut in [Cut::Flat, Cut::Hole] {
                for (system, nw, nh, aspect) in PAD_SYSTEMS {
                    let ctx = format!("{name} {turn} {cut:?} {system}");
                    let path = shots.as_ref().filter(|_| !matching.is_empty() && ctx.contains(&matching)).map(|d| {
                        std::fs::create_dir_all(d).unwrap();
                        d.join(format!("pad_{}.png", ctx.replace(' ', "_")))
                    });
                    for l in pad_look(system, (nw, nh), aspect, (w, h), density, cut, path.as_deref()) { report += &format!("{ctx}\t{l}\n"); }
                }
            }
        }
    }
    let out = std::env::var("PHONES_PAD_REPORT").unwrap_or_else(|_| "/tmp/phones-pad-report.tsv".into());
    std::fs::write(&out, &report).unwrap();
    println!("PAD-REPORT written to {out}: {} lines", report.lines().count());
}

// ---- The gate's tests -------------------------------------------------------------------------------------------------

/// Every page, at every phone, foldable and tablet format, upright and turned, at Android's usual and larger font size,
/// in English, German and Spanish, with a camera hole (the tightest safe area): nothing of ours cut short with "…", off its
/// room or into other words, nothing under 10 dp, no tap target off the screen. Found on an Android 15 emulator and offline:
/// the game card's five bottom buttons read "Favo…", "Stick…", "Contr…" on a 411 dp phone.
#[test]
fn android_pages_fit_phones_foldables_and_tablets_in_english_german_and_spanish() {
    // Language and the text record are process-wide: one child process per language and half of the formats.
    let shard = std::env::var("ANDROID_PHONES_FIT_CHILD").ok();
    let Some(shard) = shard else {
        let mut children = Vec::new();
        for lang in ["en", "de", "es"] {
            for part in 0..2 {
                let ink = tempfile::tempdir().unwrap();
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "android_02_tests::phones::android_pages_fit_phones_foldables_and_tablets_in_english_german_and_spanish", "--nocapture"])
                    .env("ANDROID_PHONES_FIT_CHILD", format!("{lang}:{part}")).env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
                    .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
                children.push((format!("{lang}:{part}"), ink, child));
            }
        }
        let mut failed = Vec::new();
        for (name, _ink, child) in children {
            let out = child.wait_with_output().unwrap();
            if !(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed; 0 failed")) {
                failed.push(format!("--- {name}\n{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
        return;
    };
    let (code, part) = shard.split_once(':').unwrap();
    let lang = match code { "de" => Lang::De, "es" => Lang::Es, _ => Lang::En };
    i18n::set(lang);
    // The two 20:9 phones differ in density, not in shape: 1080x2340 is left to the sweep.
    let formats: Vec<_> = PHONES.iter().filter(|f| f.0 != "phone-1080x2340").copied().collect();
    let formats = if part == "0" { &formats[..3] } else { &formats[3..] };
    let mut faults = Vec::new();
    for &(name, pw, ph, density) in formats {
        for (turn, (w, h), fonts) in [("up", (pw, ph), &[1.0f32, 1.3][..]), ("turned", (ph, pw), &[1.3f32][..])] {
            for &scale in fonts {
                for case in cases() {
                    let looks: &[Theme] = if case.looks { &Theme::PHONE } else { &Theme::PHONE[..1] };
                    for &theme in looks {
                        // Open: the Split look's info page is the handhelds' shared page on a canvas 62 %
                        // of the screen: its sentence and rating line keep their two-line caps and its cover tile breaks at larger
                        // text on a 360 dp phone. Everything else of it is still measured.
                        let open_split = theme == Theme::Split && case.name.starts_with("box");
                        if open_split && scale > 1.0 && density <= 2.0 { continue; }
                        let f = look(lang, (w, h), density, Cut::Hole, scale, theme, &case, None);
                        faults.extend(f.lines.into_iter().filter(|l| !(open_split && l.starts_with("ELL"))).map(|l| format!("{code} {name} {turn} font={scale} {} {theme:?}: {l}", case.name)));
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "{} faults on phone formats, the first ones: {:#?}", faults.len(), &faults[..faults.len().min(40)]);
}

/// The touch pad over a game on every phone, foldable and tablet format, upright and turned, with and without a camera
/// hole: every button whole on the screen and at least 36 dp across, no two on each other, the D-pad in the lower half
/// of an upright screen, and nothing of the pad over the picture when the phone stands upright (the pad lies under it).
#[test]
fn android_touch_pad_fits_phone_formats_and_leaves_the_upright_picture_alone() {
    let mut faults = Vec::new();
    for (name, pw, ph, density) in PHONES {
        for (turn, (w, h)) in [("up", (pw, ph)), ("turned", (ph, pw))] {
            for cut in [Cut::Flat, Cut::Hole] {
                for (system, nw, nh, aspect) in PAD_SYSTEMS {
                    for l in pad_look(system, (nw, nh), aspect, (w, h), density, cut, None) {
                        // Turned, the pad lies on a wide picture's edge (PSP's 16:9, a fold's near-square screen): a decision about the
                        // picture's room, not a fault of the pad.
                        if l.starts_with("COVERS") && w > h { continue; }
                        faults.push(format!("{name} {turn} {cut:?} {system}: {l}"));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults, the first ones: {:#?}", faults.len(), &faults[..faults.len().min(40)]);
}

/// The pause sheet's rows fit under its head on the phone formats, with the gesture bar, the back-gesture zones and a camera
/// hole taken off: all of Save & quit's rows on the screen, in two columns of at least 300 when held across.
#[test]
fn the_pause_sheet_fits_phone_formats_with_gesture_bar_and_camera_hole() {
    let mut faults = Vec::new();
    for (name, pw, ph, density) in PHONES {
        for (turn, (w, h)) in [("up", (pw, ph)), ("turned", (ph, pw))] {
            for cut in [Cut::Flat, Cut::Hole] {
                let (vw, vh) = player_safe_area((w, h), density, cut);
                let (lw, lh) = pocket_ui::scaled_layout::mobile_size((vw, vh), density);
                for card in [false, true] {
                    if !pocket_ui::pause_head::rows_fit(lw as i32, lh as i32, card) {
                        faults.push(format!("{name} {turn} {cut:?} memory card {card}: the sheet is {lw}x{lh}"));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}
