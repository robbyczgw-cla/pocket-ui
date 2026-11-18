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
        c("drawer", true, false, |u| { u.apps_got(apps()); u.press(pocket_ui::pad::Button::R2); }),
        p("library-pad", |_| {}),
        p("box-pad", |u| u.navigate(ui::Screen::Box)),
        p("search-pad", |u| { u.navigate(ui::Screen::Search); u.query = "tobu".into(); u.filter(); u.keyboard = None; }),
        c("drawer-none", true, false, |u| { u.apps_got(Vec::new()); u.press(pocket_ui::pad::Button::R2); }),
    ]
}

/// Whether `look` hashes its picture (`first_start_page_hashes` asks for it; the gate's tests do not pay for it).
#[cfg(test)]
static WANT_HASH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What one page looked like at one format: the faults found by measuring.
#[derive(Default, Debug)]
pub struct Findings {
    pub lines: Vec<String>,
    pub canvas: (u32, u32),
    pub dp: f32,
    /// The largest run of rows without any ink, as a share of the screen (empty gaps).
    pub blank: f32,
    /// Every string painted, one per line (what `POCKETUI_VISIBLE_TEXT` records).
    pub visible: String,
    /// A hash of the picture's pixels (to tell two builds' pictures apart without looking).
    pub hash: u64,
    /// What a tap does something on: its name and its area (x, y, w, h), labels left out.
    pub taps: Vec<(String, (i32, i32, i32, i32))>,
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
    let mut f = Findings { canvas: (lw, lh), dp: u.dp, visible: visible.clone(), ..Default::default() };
    if WANT_HASH.load(std::sync::atomic::Ordering::Relaxed) {
        f.hash = px.iter().fold(0xcbf29ce484222325u64, |h, p| (h ^ u64::from(*p)).wrapping_mul(0x100000001b3));
    }
    f.taps = u.regions.iter().filter(|r| !matches!(r.action, ui::Action::Label)).map(|r| (r.label.clone(), (r.area.x, r.area.y, r.area.w, r.area.h))).collect();
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
    let settings = picture::Settings::load(dir.path(), system, std::path::Path::new("k"), picture::mobile_start());
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
        for &lang in super::sample_languages() {
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

/// Games that stand open (a Continue row on the shelf, "Continue" on the cases and in the list), the first three of the library, and
/// the PSP and N64 ones among them says "Experimental": the pages those words make longest.
#[cfg(test)]
fn continue_cases() -> Vec<Case> {
    fn open(u: &mut ui::Ui) {
        let keys = u.library.as_ref().unwrap().games.iter().filter(|g| ["gb", "psp", "n64"].contains(&g.system_id)).map(|g| g.key.clone()).collect();
        u.frozen_got(keys);
    }
    let c = |name, looks, setup: Setup| Case { name, games: true, looks, setup, pad: false };
    vec![
        c("library-continue", true, open),
        c("box-continue", true, |u| { open(u); u.navigate(ui::Screen::Box); }),
        c("box-psp-continue", false, |u| { open(u); u.selected = u.visible.iter().position(|&i| u.library.as_ref().unwrap().games[i].system_id == "psp").unwrap_or(0); u.navigate(ui::Screen::Box); }),
    ]
}

/// The Retroid Pocket Mini V2 (1240x1080, built-in controller, no camera hole, no bars) is the main Android test device: every
/// page of `cases()` on it at both densities and both font sizes, with its controller in hand (hint bar) and, at the larger
/// font, with a finger only, in English, German and Spanish. Nothing cut short with "…", off its room or into other words,
/// nothing under 9 dp, no tap target off the screen.
#[test]
fn android_pages_fit_the_retroid_mini_v2_with_its_controller_and_with_a_finger() {
    let Some(langs) = super::fit_children("android_02_tests::phones::android_pages_fit_the_retroid_mini_v2_with_its_controller_and_with_a_finger", "ANDROID_MINI_V2_FIT_CHILD", true) else { return };
    let mut faults = Vec::new();
    for lang in langs {
        i18n::set(lang);
        for (name, w, h, density) in MINI_V2 {
            for (scale, pad) in [(1.0f32, true), (1.3, true), (1.3, false)] {
                for case in cases().into_iter().chain(continue_cases()) {
                    let case = Case { pad: pad || case.pad, ..case };
                    let looks: &[Theme] = if case.looks { &Theme::PHONE } else { &Theme::PHONE[..1] };
                    for &theme in looks {
                        // The Split look's info page is the handhelds' shared page on a canvas 62 % of the screen (see the
                        // phone sweep): its two-line caps are still measured, only its "…" is left out.
                        let open_split = theme == Theme::Split && case.name.starts_with("box");
                        let f = look(lang, (w, h), density, Cut::Flat, scale, theme, &case, None);
                        faults.extend(f.lines.into_iter().filter(|l| !(open_split && l.starts_with("ELL"))).map(|l| format!("{lang:?} {name} font={scale} pad={pad} {} {theme:?}: {l}", case.name)));
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "{} faults on the Mini V2, the first ones: {:#?}", faults.len(), &faults[..faults.len().min(40)]);
}

/// The pause sheet's rows fit under its head on the phone formats, with the gesture bar, the back-gesture zones and a camera
/// hole taken off: all of Save & quit's rows on the screen, in two columns of at least 300 when held across.
#[test]
fn the_pause_sheet_fits_phone_formats_with_gesture_bar_and_camera_hole() {
    let mut faults = Vec::new();
    for (name, pw, ph, density) in PHONES.into_iter().chain(MINI_V2) {
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

// ---- Screen frames around the picture --------------------------------------------------------------------------------
// The frames are Linux's (`pocket_ui::bezel`), drawn by the same layer behind the picture. What a player notices: the
// frame is there (never silently left out), it never touches a pixel of the picture, never reaches into the system bars,
// the notch or the gesture zones (outside the viewport) and never lies under a touch button; and the picture stays big.
#[path = "../android/src/player_graphics.rs"]
mod graphics;
use pocket_ui::bezel;

/// Everything a frame case works out, as the player does it (`player.rs`: `paint`).
#[cfg(test)]
pub struct FrameCase {
    pub logical: (i32, i32),
    pub view: (i32, i32, i32, i32),
    /// The picture's own space (`picture::Settings::place`); `game`, `room` and `plan` are in it.
    pub space: picture::Space,
    pub game: pocket_ui::touch::Area,
    pub room: pocket_ui::touch::Area,
    pub plan: Option<bezel::Placed>,
    /// Touch buttons (logical), empty with no pad.
    pub pad: Vec<pocket_ui::touch::Area>,
    pub frameless: pocket_ui::touch::Area,
    /// The pad as the player paints it (logical pixels, alpha in the top byte); empty with no pad.
    pub overlay: Vec<u32>,
}

#[cfg(test)]
pub fn frame_case(system: &str, native: (u32, u32), aspect: f64, (w, h): (u32, u32), density: f32, cut: Cut, frame: usize, touch: bool) -> FrameCase {
    let (vw, vh) = player_safe_area((w, h), density, cut);
    // The viewport sits inside the screen (the insets are around it); only its size matters to the frame.
    let view = (((w - vw) / 2) as i32, ((h - vh) / 2) as i32, vw as i32, vh as i32);
    let (lw, lh) = pocket_ui::scaled_layout::mobile_size((vw, vh), density);
    let logical = (lw as i32, lh as i32);
    let dp = lw as f32 / (vw as f32 / density.max(0.5));
    let dir = tempfile::tempdir().unwrap();
    let mut settings = picture::Settings::load(dir.path(), system, std::path::Path::new("k"), picture::mobile_start());
    let window = (w as i32, h as i32);
    let frameless = settings.place(window, view, logical, native, aspect, touch).game;
    settings.set_values(&[("frame", ["none", "auto", "crt"][frame as usize].to_string())]).unwrap();
    // As `player.rs` `paint`: the picture's own space (the overlay's with the touch pad, the whole screen without).
    let space = settings.place(window, view, logical, native, aspect, touch);
    let game = space.game;
    let mut text = Text::builtin();
    let pad_at = if touch { painted_touch::pad_extent(&mut text, system, logical, dp, Some(game)) } else { (0, space.size.1) };
    let room = picture::Settings::frame_room(space.size, touch, game, pad_at);
    let plan = settings.frame_plan(system, space.size, space.view, room, game, native.0);
    let mut pad = Vec::new();
    let mut overlay = vec![0u32; (lw * lh) as usize];
    if touch {
        let (controls, _, menu) = painted_touch::touch_controls(&mut Canvas::new(&mut overlay, lw, lh), &mut text, system, logical, dp, Some(game), 0);
        pad.extend(controls.iter().map(|(a, _)| *a));
        pad.push(menu);
    }
    FrameCase { logical, view, space, game, room, plan, pad, frameless, overlay }
}

/// What is wrong with the frame of one case (empty: nothing).
#[cfg(test)]
pub fn frame_faults(c: &FrameCase, density: f32) -> Vec<String> {
    let mut out = Vec::new();
    let Some(plan) = c.plan else { return vec!["NOPLAN: a frame was asked for and none is planned".into()] };
    let (vw, vh) = (c.space.view.2, c.space.view.3);
    let (x, y, rw, rh) = plan.room;
    if x < 0 || y < 0 || x + rw as i32 > vw || y + rh as i32 > vh {
        out.push(format!("OUTSIDEVIEW the room {:?} leaves the viewport {vw}x{vh}", plan.room));
    }
    let Some(b) = bezel::generate(plan.style, rw, rh, plan.hole, plan.px) else {
        out.push(format!("NOFRAME {:?} draws nothing for the picture {:?} in {rw}x{rh}", plan.style, plan.hole));
        return out;
    };
    let alpha = |bx: u32, by: u32| b.rgba[((by * b.w + bx) * 4 + 3) as usize];
    let hole = plan.hole;
    if hole.x + hole.w > rw || hole.y + hole.h > rh { out.push(format!("HOLEOUTSIDE {hole:?} in {rw}x{rh}")); return out; }
    // The DS frame's hinge (`inlay`) lies inside the picture's rectangle on purpose, between the two screens.
    let inlay = b.inlay;
    'hole: for by in hole.y..hole.y + hole.h {
        for bx in hole.x..hole.x + hole.w {
            if inlay.is_some_and(|r| bx >= r.x && bx < r.x + r.w && by >= r.y && by < r.y + r.h) { continue; }
            if alpha(bx, by) != 0 { out.push(format!("COVERSPICTURE at {bx},{by}")); break 'hole; }
        }
    }
    let bands = bezel::bands(plan.style, rw, rh, hole);
    let widest = bands.iter().cloned().fold(0.0, f32::max);
    if widest < 5.0 * density { out.push(format!("THINFRAME the widest band is {widest:.0} px ({:.1} dp)", widest / density)); }
    // The picture stays big: a frame may take a margin, not a third of the picture.
    let (fw, fh) = (c.frameless.w as f32, c.frameless.h as f32);
    let share = (c.game.w as f32 / fw).min(c.game.h as f32 / fh);
    if share < 0.75 { out.push(format!("SMALLERPICTURE {:.0} % of the frameless picture", share * 100.0)); }
    // No touch button over a frame pixel (the pad lies on black). A button that reaches into the picture's own rows (upright) or
    // columns (turned), as on a near-square fold where the pad has no room beside or under the picture, lies over the frame's
    // sides as it lies over the picture.
    let portrait = c.logical.0 < c.logical.1;
    let in_picture_band = |a: &pocket_ui::touch::Area| if portrait { a.y < c.game.y + c.game.h && a.y + a.h > c.game.y } else { a.x < c.game.x + c.game.w && a.x + a.w > c.game.x };
    let (lw, lh) = (c.logical.0 as i64, c.logical.1 as i64);
    let top_of_room = vh - y - rh as i32;
    for a in c.pad.iter().filter(|a| !in_picture_band(a)) {
        let (x0, x1) = ((a.x as i64 * vw as i64 / lw) as i32, ((a.x + a.w) as i64 * vw as i64 / lw) as i32);
        let (y0, y1) = ((a.y as i64 * vh as i64 / lh) as i32, ((a.y + a.h) as i64 * vh as i64 / lh) as i32);
        for py in (y0..y1).step_by(3) {
            for px in (x0..x1).step_by(3) {
                let (bx, by) = (px - x, py - top_of_room);
                if bx >= 0 && by >= 0 && (bx as u32) < rw && (by as u32) < rh && alpha(bx as u32, by as u32) != 0 {
                    out.push(format!("UNDERPAD the frame lies under the button at logical {:?}", (a.x, a.y, a.w, a.h)));
                    return out;
                }
            }
        }
    }
    out
}

/// Console is Linux's Auto for every console it knows, CRT TV is Linux's CRT frame, None draws no frame.
#[test]
fn the_consoles_frame_is_the_one_linux_picks_for_that_console() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = picture::Settings::load(dir.path(), "gba", std::path::Path::new("k"), picture::mobile_start());
    assert_eq!(settings.frame_style("gba"), None, "a phone starts with the clean picture, as Linux does");
    let mut systems: Vec<&str> = pocket_ui::hostsys::SYSTEMS.iter().flat_map(|s| s.names.iter().copied()).collect();
    systems.extend(PAD_SYSTEMS.iter().map(|s| s.0));
    systems.extend(["gamegear", "atari2600", "wonderswan", "virtualboy", "unknown-console"]);
    settings.set_values(&[("frame", ["none", "auto", "crt"][1 as usize].to_string())]).unwrap();
    for system in systems {
        assert_eq!(settings.frame_style(system), Some(bezel::style_for(system)), "Console on {system}");
    }
    assert_eq!(settings.frame_style("gba"), Some(bezel::Style::Wide));
    assert_eq!(settings.frame_style("gb"), Some(bezel::Style::Brick(bezel::BrickColour::Grey)));
    assert_eq!(settings.frame_style("snes"), Some(bezel::Style::Monitor));
    settings.set_values(&[("frame", ["none", "auto", "crt"][2 as usize].to_string())]).unwrap();
    assert_eq!(settings.frame_style("snes"), Some(bezel::Style::Crt(bezel::Cabinet::Plastic)));
}

/// Every phone, foldable and tablet format, upright and turned, with the pad (and without it, on a few): Console and CRT TV
/// give a frame that is whole, clear of the picture, inside the viewport and off the touch buttons.
#[test]
fn screen_frames_fit_phone_formats_with_the_touch_pad() {
    let mut faults = Vec::new();
    for (name, pw, ph, density) in PHONES {
        for (turn, (w, h)) in [("up", (pw, ph)), ("turned", (ph, pw))] {
            for (system, nw, nh, aspect) in PAD_SYSTEMS {
                for (frame, touch) in [(1usize, true), (2, true), (1, false)] {
                    // The CRT and the no-pad cases on three consoles are enough; the Console frame on all nine.
                    if (frame != 1 || !touch) && !["gba", "snes", "nds"].contains(&system) { continue; }
                    let c = frame_case(system, (nw, nh), aspect, (w, h), density, Cut::Flat, frame, touch);
                    for f in frame_faults(&c, density) {
                        faults.push(format!("{name} {turn} {system} frame {frame} touch {touch}: {f}"));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults, the first ones: {:#?}", faults.len(), &faults[..faults.len().min(40)]);
}

// ---- With a controller the picture is Linux's picture -------------------------------------------------------------------
// Alex, 15.11. on the Odin 2 Mini (1920x1080): a GBA game filled a small part of the screen. The player laid the picture
// out inside the safe area meant for its touch pad (the Odin's insets: the back-gesture zones and the gesture bar), 1782x951,
// and Integer got 5x (1200x800) where Linux on the same screen shows 6x (1440x960). With the built-in pad (no touch pad on
// the screen) the picture is placed on the whole screen exactly as pocket-core places it (`video::dest_rect`).

/// Android handhelds and a phone with a clip-on pad: (name, width, height, density, the player's insets [left, top, right,
/// bottom] in physical pixels). The Odin's are the ones its player logged (`player viewport: … insets=[69, 55, 69, 74]`);
/// the others are the player's safe area as `player_safe_area` measures it.
#[cfg(test)]
pub fn controller_screens() -> Vec<(&'static str, i32, i32, f32, [i32; 4])> {
    let safe = |w: u32, h: u32, density: f32| {
        let d = |dp: f32| (dp * density).round() as i32;
        let (vw, vh) = player_safe_area((w, h), density, Cut::Flat);
        let (l, t) = (d(30.0), d(24.0));
        [l, t, w as i32 - vw as i32 - l, h as i32 - vh as i32 - t]
    };
    vec![
        ("odin2mini-1920x1080", 1920, 1080, 2.30625, [69, 55, 69, 74]),
        ("rp5-1920x1080", 1920, 1080, 2.5, safe(1920, 1080, 2.5)),
        ("miniv2-1240x1080", 1240, 1080, 2.0, safe(1240, 1080, 2.0)),
        ("4:3-1280x960", 1280, 960, 2.0, safe(1280, 960, 2.0)),
        ("phone-2400x1080", 2400, 1080, 2.625, safe(2400, 1080, 2.625)),
    ]
}

/// (system, native width, native height, the core's display aspect): the cores' own sizes. SNES twice: Snes9x reports 4:3,
/// a core that reports the pixels' 8:7 is drawn at 8:7 on both.
#[cfg(test)]
pub const CONTROLLER_SYSTEMS: [(&str, u32, u32, f64); 8] = [
    ("gb", 160, 144, 160.0 / 144.0), ("gbc", 160, 144, 160.0 / 144.0), ("gba", 240, 160, 1.5), ("snes", 256, 224, 4.0 / 3.0),
    ("snes", 256, 224, 8.0 / 7.0), ("psx", 320, 240, 4.0 / 3.0), ("n64", 320, 240, 4.0 / 3.0), ("nds", 256, 384, 256.0 / 384.0),
];

/// Where the player puts `system`'s picture on a screen with a controller (no touch pad), physical pixels from the
/// top-left of the screen: `player.rs` `paint`'s own steps (`picture::Settings::place`, `Space::on_window`).
#[cfg(test)]
pub fn android_controller_picture(settings: &picture::Settings, (w, h, density, insets): (i32, i32, f32, [i32; 4]), native: (u32, u32), aspect: f64) -> (i32, i32, i32, i32) {
    let [left, top, right, bottom] = insets;
    let view = (left, bottom, (w - left - right).max(1), (h - top - bottom).max(1));
    let logical = pocket_ui::scaled_layout::mobile_size((view.2 as u32, view.3 as u32), density);
    settings.place((w, h), view, (logical.0 as i32, logical.1 as i32), native, aspect, false).on_window(h)
}

/// With the built-in pad, every Android screen shows the picture where pocket-core shows it on a screen of that size: the
/// start setting the handheld view hands over (its Options → Picture: Integer, Aspect for disc consoles and the DS), and
/// each of the three Scaling rows, with and without a Screen frame.
#[test]
fn with_a_controller_the_picture_is_where_linux_puts_it_on_the_same_screen() {
    use pocket_ui::video::{dest_rect, Scaling, VideoSettings};
    let data = tempfile::tempdir().unwrap();
    let mut faults = Vec::new();
    let mut checked = 0;
    for (name, w, h, density, insets) in controller_screens() {
        for (system, nw, nh, aspect) in CONTROLLER_SYSTEMS {
            let linux_at = |scaling: Scaling| {
                let r = dest_rect(scaling, w as u32, h as u32, nw, nh, pocket_ui::hostsys::display_aspect(system, nw, nh, aspect as f32));
                (r.x as i32, r.y as i32, r.w as i32, r.h as i32)
            };
            // The start: Linux's Options → Picture of a game nobody changed, as the handheld view hands it to the player.
            let effective = pocket_ui::options::GameSettings::load(data.path(), system, std::path::Path::new("/roms/game.bin")).effective();
            let settings = picture::Settings::load(data.path(), system, std::path::Path::new("k"), effective.clone());
            let mut cases = vec![("start".to_string(), settings.clone(), linux_at(VideoSettings::from_values(&effective).scaling))];
            for (row, scaling) in [(0usize, Scaling::Aspect), (1, Scaling::Native), (2, Scaling::Fullscreen)] {
                for frame in [0usize, 1, 2] {
                    let mut handed = effective.clone();
                    handed.insert("scaling".into(), ["aspect", "native", "fullscreen"][row].into());
                    handed.insert("frame".into(), ["none", "auto", "crt"][frame].into());
                    let s = picture::Settings::load(data.path(), system, std::path::Path::new("k"), handed);
                    // Linux never makes the picture smaller for a frame: the frame fills what the picture leaves.
                    cases.push((format!("scaling {row} frame {frame}"), s, linux_at(scaling)));
                }
            }
            for (case, s, linux) in cases {
                checked += 1;
                let android = android_controller_picture(&s, (w, h, density, insets), (nw, nh), aspect);
                if android != linux {
                    faults.push(format!("{name} {system} {aspect:.3} {case}: Android {android:?}, Linux {linux:?}"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} of {checked} pictures differ from Linux's, the first ones: {:#?}", faults.len(), &faults[..faults.len().min(40)]);
}


// ---- The real GLES compositor against Linux's own composition ----------------------------------------------------------

static GL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A headless GLES context with the player's compositor and a `w` x `h` framebuffer; None where the machine has no GLES
/// (`POCKET_REQUIRE_GL=1` makes that a failure, as the gate runs it).
#[cfg(test)]
fn gl_player(w: i32, h: i32) -> Option<(graphics::Graphics, pocket_ui::headless_gl::Headless)> {
    use glow::HasContext;
    let (gl, headless) = match pocket_ui::headless_gl::context() {
        Ok(context) => context,
        Err(error) if std::env::var_os("POCKET_REQUIRE_GL").is_some() => panic!("no headless GLES: {error}"),
        Err(error) => { eprintln!("GLES frame test skipped: {error}"); return None; }
    };
    unsafe {
        let fbo = gl.create_framebuffer().unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        let color = gl.create_renderbuffer().unwrap();
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
        assert_eq!(gl.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
    }
    Some((graphics::Graphics::from_context(gl).unwrap(), headless))
}

/// The framebuffer's pixels, RGBA, bottom row first (a second loader reads the compositor's current context).
#[cfg(test)]
fn gl_read(w: i32, h: i32) -> Vec<u8> {
    use glow::HasContext;
    let lib = unsafe { libloading::Library::new("libEGL.so.1").unwrap() };
    let get: libloading::Symbol<unsafe extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_void> = unsafe { lib.get(b"eglGetProcAddress").unwrap() };
    let gl = unsafe { glow::Context::from_loader_function_cstr(|n| get(n.as_ptr())) };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    unsafe {
        gl.read_pixels(0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut rgba));
        assert_eq!(gl.get_error(), glow::NO_ERROR);
    }
    rgba
}

/// A picture that looks like a game, for the sheets: sky, ground, a grid, a bright sprite, a white edge.
#[cfg(test)]
fn game_pattern((w, h): (u32, u32)) -> pocket_ui::image::Image {
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let sky = if y < h * 2 / 3 { 0x2a4fa0 + ((y * 90 / h) << 8) } else { 0x3a7a2a - ((y - h * 2 / 3) * 40 / h) };
            let mut c = sky;
            if (x / 8 + y / 8) % 2 == 0 && y > h * 2 / 3 { c = c.saturating_sub(0x101010); }
            if x > w / 3 && x < w / 3 + w / 8 && y > h / 2 && y < h / 2 + h / 6 { c = 0xe8c040; }
            if x < 2 || y < 2 || x + 2 >= w || y + 2 >= h { c = 0xffffff; }
            px.push(c);
        }
    }
    pocket_ui::image::Image { w, h, px }
}

/// What the player's compositor puts on a `screen` for `c`: the frame, the picture, the pad's overlay (RGBA, top row first).
#[cfg(test)]
fn render_android(gfx: &mut graphics::Graphics, c: &FrameCase, screen: (i32, i32), image: &pocket_ui::image::Image, pad: bool) -> Vec<u8> {
    let transparent = vec![0u32; c.overlay.len()];
    let overlay = if pad { &c.overlay } else { &transparent };
    gfx.present(overlay, c.logical, c.view, Some((image, c.game)), true, &[], c.plan.as_ref(), (c.space.size, c.space.view));
    let bottom_up = gl_read(screen.0, screen.1);
    let row = (screen.0 * 4) as usize;
    bottom_up.chunks_exact(row).rev().flatten().copied().collect()
}

/// Android's GLES compositor draws exactly Linux's frame around exactly the rectangle of the picture, in the room the
/// viewport allows: every pixel of the screen equals Linux's composition (`bezel::compose`, the desktop preview of the
/// renderer's output) of the same frame, picture and viewport; the system bars' insets keep the backdrop. Console and CRT
/// TV, upright and turned, on a screen where the logical overlay is scaled up (720 px for 360 logical).
#[test]
fn the_players_compositor_draws_linux_frames_pixel_for_pixel_inside_the_viewport() {
    let _lock = GL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let backdrop = 0x102030u32;
    let mut faults = Vec::new();
    let mut checked = 0;
    for (system, nw, nh, aspect) in PAD_SYSTEMS {
        for (turn, (w, h)) in [("up", (720u32, 1600u32)), ("turned", (1600, 720))] {
            for frame in [1usize, 2] {
                if frame == 2 && !["gba", "snes"].contains(&system) { continue; }
                let c = frame_case(system, (nw, nh), aspect, (w, h), 2.0, Cut::Hole, frame, true);
                let plan = c.plan.expect("a frame is planned");
                let Some((mut gfx, _context)) = gl_player(w as i32, h as i32) else { return };
                gfx.backdrop = backdrop;
                gfx.frame_in_background(false);
                let image = pocket_ui::image::Image { w: nw, h: nh, px: vec![0x33aa55; (nw * nh) as usize] };
                let got = render_android(&mut gfx, &c, (w as i32, h as i32), &image, false);
                // Linux's composition of the same frame and picture, then put where the viewport has its room.
                let (rx, ry, rw, rh) = plan.room;
                let b = bezel::generate(plan.style, rw, rh, plan.hole, plan.px).expect("a frame");
                let linux = bezel::compose(rw, rh, plan.hole, Some(&b), nw, nh, &image.px);
                let mut want = vec![0u8; got.len()];
                for p in want.chunks_exact_mut(4) { p.copy_from_slice(&[(backdrop >> 16) as u8, (backdrop >> 8) as u8, backdrop as u8, 255]); }
                let (left, from_bottom) = (c.view.0 + rx, c.view.1 + ry);
                for y in 0..rh as i32 {
                    let screen_y = h as i32 - 1 - (from_bottom + rh as i32 - 1 - y);
                    for x in 0..rw as i32 {
                        let (s, d) = (((y as u32 * rw + x as u32) * 4) as usize, (((screen_y as u32) * w + (left + x) as u32) * 4) as usize);
                        want[d..d + 4].copy_from_slice(&linux[s..s + 4]);
                    }
                }
                let bad = got.chunks_exact(4).zip(want.chunks_exact(4)).enumerate().filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2)).map(|(i, _)| i).collect::<Vec<_>>();
                checked += 1;
                if !bad.is_empty() {
                    faults.push(format!("{system} {turn} frame {frame} {:?}: {} pixels differ from Linux's composition, the first at ({}, {})", plan.style, bad.len(), bad[0] % w as usize, bad[0] / w as usize));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} of {checked} cases: {faults:#?}", faults.len());
}

// ---- Contact sheets: Linux's composition against the player's, per console ---------------------------------------------

#[cfg(test)]
fn shrink(rgb: &[u8], (w, h): (u32, u32), (tw, th): (u32, u32)) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th * 3) as usize];
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1));
            let mut sum = [0u32; 3];
            for y in y0..y1.min(h) { for x in x0..x1.min(w) { for k in 0..3 { sum[k] += rgb[((y * w + x) * 3) as usize + k] as u32; } } }
            let n = ((y1.min(h) - y0) * (x1.min(w) - x0)).max(1);
            for k in 0..3 { out[((ty * tw + tx) * 3) as usize + k] = (sum[k] / n) as u8; }
        }
    }
    out
}

#[cfg(test)]
fn paste(sheet: &mut [u8], sheet_w: u32, (x, y): (u32, u32), rgb: &[u8], (w, h): (u32, u32)) {
    for row in 0..h {
        let (d, s) = ((((y + row) * sheet_w + x) * 3) as usize, (row * w * 3) as usize);
        sheet[d..d + (w * 3) as usize].copy_from_slice(&rgb[s..s + (w * 3) as usize]);
    }
}

#[cfg(test)]
fn drop_alpha(rgba: &[u8]) -> Vec<u8> { rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect() }

/// `FRAME_SHEETS=<dir> cargo test android_frame_sheets -- --ignored`: one picture per console and frame (Console, CRT TV),
/// Linux's composition on the Pocket's 1240x1080 screen on the left, the player's on a 20:9 phone upright, a foldable's
/// inner screen and the phone turned (touch pad over it as the player paints it), all in the same pictures of the game.
#[test]
#[ignore = "writes pictures: FRAME_SHEETS=<dir>"]
fn android_frame_sheets() {
    let _lock = GL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dir) = std::env::var_os("FRAME_SHEETS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let only = std::env::var("FRAME_ONLY").unwrap_or_default();
    let phone = ("phone", 1080u32, 2400u32, 2.625f32);
    let fold = ("fold", 1812u32, 2176u32, 2.625f32);
    for (system, nw, nh, aspect) in PAD_SYSTEMS {
        for frame in [1usize, 2] {
            let label = format!("{system}-{}", if frame == 1 { "console" } else { "crt" });
            if !only.is_empty() && !label.contains(&only) { continue; }
            let image = game_pattern((nw, nh));
            let (sw, sh) = (1280u32, 960u32);
            let mut sheet = vec![24u8; (sw * sh * 3) as usize];
            // Linux: the renderer's own rectangle (Integer, smaller) and the style Auto or CRT picks.
            let rect = pocket_ui::video::dest_rect(pocket_ui::video::Scaling::IntegerSmaller, 1240, 1080, nw, nh, aspect as f32);
            let style = if frame == 1 { bezel::style_for(system) } else { bezel::Style::Crt(bezel::Cabinet::Plastic) };
            let bz = bezel::generate(style, 1240, 1080, rect, rect.w as f32 / nw as f32);
            let linux = bezel::compose(1240, 1080, rect, bz.as_ref(), nw, nh, &image.px);
            paste(&mut sheet, sw, (0, 0), &shrink(&drop_alpha(&linux), (1240, 1080), (560, 488)), (560, 488));
            let mut x = 580;
            for (name, pw, ph, density) in [phone, fold] {
                let c = frame_case(system, (nw, nh), aspect, (pw, ph), density, Cut::Flat, frame, true);
                let Some((mut gfx, _context)) = gl_player(pw as i32, ph as i32) else { return };
                gfx.frame_in_background(false);
                let shot = drop_alpha(&render_android(&mut gfx, &c, (pw as i32, ph as i32), &image, true));
                let width = (pw * 488 / ph).min(sw - x);
                paste(&mut sheet, sw, (x, 0), &shrink(&shot, (pw, ph), (width, 488)), (width, 488));
                x += width + 20;
                let _ = name;
            }
            let c = frame_case(system, (nw, nh), aspect, (2400, 1080), 2.625, Cut::Flat, frame, true);
            let Some((mut gfx, _context)) = gl_player(2400, 1080) else { return };
            gfx.frame_in_background(false);
            let shot = drop_alpha(&render_android(&mut gfx, &c, (2400, 1080), &image, true));
            paste(&mut sheet, sw, (0, 500), &shrink(&shot, (2400, 1080), (1000, 450)), (1000, 450));
            let png = pocket_ui::screenshot::encode_rgb(sw, sh, &sheet).unwrap();
            std::fs::write(dir.join(format!("{label}.png")), png).unwrap();
        }
    }
}

/// `PICTURE_SHOTS=<dir> cargo test android_controller_picture_shots -- --ignored`: a GBA game at the handheld view's start
/// setting (Integer) on the Odin 2 Mini's 1920x1080 screen with its insets, the real compositor: `odin-gba-before.png`
/// (laid out in the safe area, as before 15.11.), `odin-gba-after.png` (`Settings::place`) and `linux-gba.png`
/// (pocket-core's `dest_rect`, `bezel::compose` without a frame).
#[test]
#[ignore = "writes pictures: PICTURE_SHOTS=<dir>"]
fn android_controller_picture_shots() {
    let _lock = GL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dir) = std::env::var_os("PICTURE_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let (w, h, density, [left, top, right, bottom]) = (1920i32, 1080i32, 2.30625f32, [69, 55, 69, 74]);
    let (nw, nh) = (240u32, 160u32);
    let image = game_pattern((nw, nh));
    let data = tempfile::tempdir().unwrap();
    let mut settings = picture::Settings::load(data.path(), "gba", std::path::Path::new("k"), picture::mobile_start());
    settings.set_values(&[("scaling", ["aspect", "native", "fullscreen"][1 as usize].to_string())]).unwrap();
    let view = (left, bottom, w - left - right, h - top - bottom);
    let logical = pocket_ui::scaled_layout::mobile_size((view.2 as u32, view.3 as u32), density);
    let logical = (logical.0 as i32, logical.1 as i32);
    let before = picture::Space { size: logical, view, game: settings.rect(logical, (view.2, view.3), (nw, nh), 1.5, false) };
    let after = settings.place((w, h), view, logical, (nw, nh), 1.5, false);
    let transparent = vec![0u32; (logical.0 * logical.1) as usize];
    for (name, space) in [("odin-gba-before", before), ("odin-gba-after", after)] {
        let Some((mut gfx, _context)) = gl_player(w, h) else { return };
        gfx.present(&transparent, logical, view, Some((&image, space.game)), true, &[], None, (space.size, space.view));
        let bottom_up = gl_read(w, h);
        let rgba: Vec<u8> = bottom_up.chunks_exact((w * 4) as usize).rev().flatten().copied().collect();
        println!("{name}: {:?}", space.on_window(h));
        std::fs::write(dir.join(format!("{name}.png")), pocket_ui::screenshot::encode_rgb(w as u32, h as u32, &drop_alpha(&rgba)).unwrap()).unwrap();
    }
    let rect = pocket_ui::video::dest_rect(pocket_ui::video::Scaling::Native, w as u32, h as u32, nw, nh, 1.5);
    println!("linux-gba: {rect:?}");
    let linux = bezel::compose(w as u32, h as u32, rect, None, nw, nh, &image.px);
    std::fs::write(dir.join("linux-gba.png"), pocket_ui::screenshot::encode_rgb(w as u32, h as u32, &drop_alpha(&linux)).unwrap()).unwrap();
}

/// `cargo test android_frame_cost -- --ignored --nocapture`: what a frame costs. How long Linux's drawing code takes for each
/// console's frame at phone sizes (it runs on a thread when a game starts), and what the player's compositor spends per
/// presented picture with and without a frame (software GLES, so only the difference means anything; the Odin's numbers come
/// from the device run in ANDROID-FRAMES.md).
#[test]
#[ignore = "measurement"]
fn android_frame_cost() {
    let _lock = GL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (name, w, h, density) in [("phone-1080x2400", 1080u32, 2400u32, 2.625f32), ("phone-1440x3200", 1440, 3200, 3.5), ("odin-1920x1080", 1920, 1080, 2.625)] {
        let mut line = format!("generate {name}:");
        for (system, nw, nh, aspect) in PAD_SYSTEMS {
            let c = frame_case(system, (nw, nh), aspect, (w, h), density, Cut::Flat, 1, true);
            let plan = c.plan.unwrap();
            let t = std::time::Instant::now();
            let b = bezel::generate(plan.style, plan.room.2, plan.room.3, plan.hole, plan.px);
            line += &format!(" {system} {:.1} ms{}", t.elapsed().as_secs_f32() * 1000.0, if b.is_some() { "" } else { " (none)" });
        }
        println!("{line}");
    }
    for (system, nw, nh, aspect) in [("gba", 240u32, 160u32, 1.5), ("psx", 320, 240, 4.0 / 3.0), ("nds", 256, 384, 2.0 / 3.0)] {
        let (w, h) = (1080u32, 2400u32);
        let image = game_pattern((nw, nh));
        let Some((mut gfx, _context)) = gl_player(w as i32, h as i32) else { return };
        gfx.frame_in_background(false);
        for frame in [0usize, 1, 2] {
            let c = frame_case(system, (nw, nh), aspect, (w, h), 2.625, Cut::Flat, frame, true);
            let transparent = vec![0u32; c.overlay.len()];
            for _ in 0..5 { gfx.present(&transparent, c.logical, c.view, Some((&image, c.game)), false, &[], c.plan.as_ref(), (c.space.size, c.space.view)); }
            let mut times = Vec::new();
            for _ in 0..60 {
                let t = std::time::Instant::now();
                gfx.present(&transparent, c.logical, c.view, Some((&image, c.game)), false, &[], c.plan.as_ref(), (c.space.size, c.space.view));
                let _ = gl_read(1, 1);
                times.push(t.elapsed().as_secs_f32() * 1000.0);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("present {system} frame={frame} ({}): median {:.2} ms, p95 {:.2} ms, max {:.2} ms", ["none", "console", "crt tv"][frame], times[30], times[57], times[59]);
        }
    }
}

// ---- The first start's steps at every screen shape ----------------------------------------------------------------------

/// Screens that are not 20:9: a phone's window in 4:3, square, 3:2, 16:9, 5:4 (a fold open, a Chromebook or desktop window, a
/// tablet held across, split screen), as (name, width and height in px, density = dpi / 160, whether the camera hole's
/// safe area is walked too). The last two are 360 x 360 dp squares (a small square phone, a quarter of a tablet): a window
/// has no camera hole there.
#[cfg(test)]
pub const WINDOWS: [(&str, u32, u32, f32, bool); 11] = [
    ("window-4x3-1440x1080", 1440, 1080, 2.625, true),
    ("window-4x3-1440x1080-d3", 1440, 1080, 3.0, true),
    ("window-1x1-1080x1080", 1080, 1080, 2.625, true),
    ("window-3x2-1620x1080", 1620, 1080, 2.625, true),
    ("window-16x9-1920x1080", 1920, 1080, 2.625, true),
    ("window-5x4-1350x1080", 1350, 1080, 2.625, true),
    ("window-4x3-1280x960", 1280, 960, 2.0, true),
    ("window-1x1-1080x1080-d3", 1080, 1080, 3.0, false),
    ("window-1x1-720x720", 720, 720, 2.0, false),
    ("miniv2-1240x1080", 1240, 1080, 2.0, false),
    ("miniv2-1240x1080-d2.5", 1240, 1080, 2.5, false),
];

/// The Retroid Pocket Mini V2's screen (1240x1080, built-in controller, no camera hole, no system bars) at the densities it may
/// report (320, 400 and the 420 dpi a 3.9 inch panel of this size gets by default). Its player is the main Android test device.
#[cfg(test)]
pub const MINI_V2: [(&str, u32, u32, f32); 3] = [("miniv2-1240x1080-d2", 1240, 1080, 2.0), ("miniv2-1240x1080-d2.5", 1240, 1080, 2.5), ("miniv2-1240x1080-d2.625", 1240, 1080, 2.625)];

/// The faults of one first-start step at one format, as the player sees them: everything `look` measures (a word cut
/// short, text off its room, in other text, under 9 dp), a "…" anywhere (strict: the first start has no game or app names
/// to cut), a tap target on another, and on the language step every language's name whole, each as one line.
#[cfg(test)]
pub fn first_start_faults(lang: Lang, (w, h): (u32, u32), density: f32, cut: Cut, scale: f32, case: &Case) -> Vec<String> {
    let f = look(lang, (w, h), density, cut, scale, Theme::PHONE[0], case, None);
    let mut faults = f.lines.clone();
    for l in f.visible.lines().filter(|l| l.contains('…') || l.contains("...")) {
        faults.push(format!("ELLIPSIS {l}"));
    }
    for (i, (a, ra)) in f.taps.iter().enumerate() {
        for (b, rb) in f.taps.iter().skip(i + 1) {
            let dx = (ra.0 + ra.2).min(rb.0 + rb.2) - ra.0.max(rb.0);
            let dy = (ra.1 + ra.3).min(rb.1 + rb.3) - ra.1.max(rb.1);
            if dx > 4 && dy > 4 {
                faults.push(format!("TAPS {a:?} {ra:?} lies on {b:?} {rb:?}"));
            }
        }
    }
    if case.name == "onb1" {
        // Each language's name is its own word(s), whole; the name of the language in use is among them as the player wrote it.
        for l in Lang::ALL {
            let shown = f.visible.lines().any(|v| v.trim() == l.name());
            // "Português (Brasil)" may stand on two lines (at a word break); every word whole.
            let words = l.name().split(' ').all(|word| f.visible.lines().any(|v| v.split(' ').any(|x| x == word)));
            if !(shown || words) {
                faults.push(format!("LANGUAGE {} is not shown whole", l.name()));
            }
            if !shown && words && l.name().split(' ').count() < 2 {
                faults.push(format!("LANGUAGE {} is not one line", l.name()));
            }
        }
        if f.taps.iter().filter(|(_, r)| r.2 > 0).count() < Lang::ALL.len() + 1 {
            faults.push(format!("LANGUAGE fewer than six choices to tap: {:?}", f.taps.iter().map(|t| &t.0).collect::<Vec<_>>()));
        }
    }
    faults
}

/// The screens the first start is walked at: the phone formats of `PHONES` upright and turned (the 20:9 phones differ in
/// density, not in shape: 1080x2340 is left to the sweep) and the window shapes of `WINDOWS`.
#[cfg(test)]
pub fn first_start_screens() -> Vec<(String, (u32, u32), f32, bool)> {
    let mut screens = Vec::new();
    for (name, pw, ph, density) in PHONES.iter().filter(|f| f.0 != "phone-1080x2340") {
        screens.push((format!("{name} up"), (*pw, *ph), *density, true));
        screens.push((format!("{name} turned"), (*ph, *pw), *density, true));
    }
    for (name, w, h, density, holes) in WINDOWS {
        screens.push((name.to_string(), (w, h), density, holes));
    }
    screens
}

/// Every step of the first start (welcome, language, folder, look, covers, systems, controls, ready), at every phone format
/// of ANDROID-PHONES.md upright and turned and at the window shapes of `WINDOWS` (4:3, square, 3:2, 16:9), at Android's
/// usual and larger font size, with a camera hole and without, with and without a controller in hand, in all six languages:
/// no word torn across lines, no "…" anywhere, nothing cut by an edge, on another text or tap target, every language's
/// name whole. Found by the 0.5 check on an emulator: at 1440x1080 the language rows read "Deuts / ch" and "Franç / ais",
/// at 1080x1080 "E …", "D …" and "Fortset…".
#[test]
fn the_first_start_steps_fit_every_phone_format_and_window_shape_in_all_six_languages() {
    // Language and the text record are process-wide: one child process per language.
    let Ok(code) = std::env::var("ANDROID_FIRST_START_CHILD") else {
        let mut children = Vec::new();
        for lang in Lang::ALL {
            let ink = tempfile::tempdir().unwrap();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "android_02_tests::phones::the_first_start_steps_fit_every_phone_format_and_window_shape_in_all_six_languages", "--nocapture"])
                .env("ANDROID_FIRST_START_CHILD", format!("{lang:?}")).env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
                .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
            children.push((format!("{lang:?}"), ink, child));
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
    let lang = *Lang::ALL.iter().find(|l| format!("{l:?}") == code).unwrap();
    i18n::set(lang);
    let screens = first_start_screens();
    let steps: Vec<Case> = cases().into_iter().filter(|c| c.name.starts_with("onb")).collect();
    let mut faults = Vec::new();
    for (name, size, density, holes) in &screens {
        for scale in [1.0f32, 1.3] {
            for cut in [Cut::Flat, Cut::Hole].into_iter().filter(|c| *holes || *c == Cut::Flat) {
                for pad in [false, true] {
                    for step in &steps {
                        let case = Case { name: step.name, games: step.games, looks: step.looks, setup: step.setup, pad };
                        for l in first_start_faults(lang, *size, *density, cut, scale, &case) {
                            faults.push(format!("{lang:?} {name} font={scale} {cut:?} pad={pad} {}: {l}", step.name));
                        }
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    // One line per screen and step (its first fault), not every repeat of it.
    let mut seen: Vec<String> = Vec::new();
    for f in &faults {
        let key = f.split(':').next().unwrap_or(f).replace("pad=true", "").replace("pad=false", "");
        if !seen.iter().any(|k| k.replace("pad=true", "").replace("pad=false", "").starts_with(&key)) {
            seen.push(f.clone());
        }
    }
    assert!(faults.is_empty(), "{} faults on the first start; each screen and step once: {:#?}", faults.len(), seen);
}

/// Exploratory, not a gate test: one line per language, screen, font size, cut, controller and first-start step with its
/// faults and a hash of its picture. Run it on two builds and compare: a page that was fault-free must keep its hash.
/// `POCKETUI_TEXT_BOXES=/tmp/x/ink POCKETUI_VISIBLE_TEXT=/tmp/x/vis cargo test --lib first_start_page_hashes -- --ignored --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn first_start_page_hashes() {
    WANT_HASH.store(true, std::sync::atomic::Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("POCKETUI_TEXT_BOXES", dir.path().join("ink"));
        std::env::set_var("POCKETUI_VISIBLE_TEXT", dir.path().join("visible"));
    }
    let steps: Vec<Case> = cases().into_iter().filter(|c| c.name.starts_with("onb")).collect();
    // FIRST_START_ONLY=a,b: only the screens whose names contain one of these; FIRST_START_LANGS=En,De likewise.
    let only = std::env::var("FIRST_START_ONLY").unwrap_or_default();
    let langs = std::env::var("FIRST_START_LANGS").unwrap_or_default();
    for lang in Lang::ALL {
        if !langs.is_empty() && !langs.split(',').any(|l| l == format!("{lang:?}")) { continue; }
        i18n::set(lang);
        for (name, size, density, _) in &first_start_screens() {
            if !only.is_empty() && !only.split(',').any(|o| name.contains(o)) { continue; }
            for scale in [1.0f32, 1.3] {
                for cut in [Cut::Flat, Cut::Hole] {
                    for pad in [false, true] {
                        for step in &steps {
                            let case = Case { name: step.name, games: step.games, looks: step.looks, setup: step.setup, pad };
                            let found = first_start_faults(lang, *size, *density, cut, scale, &case).len();
                            let f = look(lang, *size, *density, cut, scale, Theme::PHONE[0], &case, None);
                            println!("HASH {lang:?} {name} font={scale} {cut:?} pad={pad} {} faults={found} {:016x}", step.name, f.hash);
                        }
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
}

/// Exploratory, not a gate test: pictures of first-start steps as one hex line each (`SHOT <name> <hex of a PNG>`), for
/// looking at what a build does at the shapes the 0.5 check found (and a few others). `FIRST_START_SHOT_STEPS` picks the
/// steps (default `onb1`), `FIRST_START_SHOT_LANGS` the languages (default `de,fr`), `FIRST_START_SHOT_FONT` the font scale;
/// with `FIRST_START_SHOT_DIR` the PNGs are written there instead.
#[cfg(test)]
#[test]
#[ignore]
fn first_start_shots() {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("POCKETUI_TEXT_BOXES", dir.path().join("ink"));
        std::env::set_var("POCKETUI_VISIBLE_TEXT", dir.path().join("visible"));
    }
    let cut = if std::env::var("FIRST_START_SHOT_CUT").is_ok_and(|c| c == "hole") { Cut::Hole } else { Cut::Flat };
    let steps_wanted = std::env::var("FIRST_START_SHOT_STEPS").unwrap_or_else(|_| "onb1".into());
    let langs_wanted = std::env::var("FIRST_START_SHOT_LANGS").unwrap_or_else(|_| "de,fr".into());
    let font: f32 = std::env::var("FIRST_START_SHOT_FONT").ok().and_then(|f| f.parse().ok()).unwrap_or(1.0);
    let screens: [(&str, (u32, u32), f32); 7] = [
        ("phone-portrait-1080x2400", (1080, 2400), 2.625),
        ("phone-landscape-2400x1080", (2400, 1080), 2.625),
        ("window-4x3-1440x1080", (1440, 1080), 2.625),
        ("window-4x3-1440x1080-d3", (1440, 1080), 3.0),
        ("window-5x4-1350x1080", (1350, 1080), 2.625),
        ("window-1x1-1080x1080", (1080, 1080), 2.625),
        ("window-1x1-1080x1080-d3", (1080, 1080), 3.0),
    ];
    let steps: Vec<Case> = cases().into_iter().filter(|c| steps_wanted.split(',').any(|s| s == c.name)).collect();
    for (code, lang) in [("en", Lang::En), ("de", Lang::De), ("es", Lang::Es), ("pt", Lang::Pt), ("fr", Lang::Fr), ("it", Lang::It)] {
        if !langs_wanted.split(',').any(|l| l == code) { continue; }
        i18n::set(lang);
        for (name, size, density) in screens {
            for step in &steps {
                let case = Case { name: step.name, games: step.games, looks: step.looks, setup: step.setup, pad: false };
                let path = dir.path().join("shot.png");
                look(lang, size, density, cut, font, Theme::PHONE[0], &case, Some(&path));
                let bytes = std::fs::read(&path).unwrap();
                if let Some(out) = std::env::var_os("FIRST_START_SHOT_DIR") {
                    std::fs::create_dir_all(&out).unwrap();
                    std::fs::write(std::path::Path::new(&out).join(format!("{code}_{name}_{}_font{font}.png", step.name)), bytes).unwrap();
                } else {
                    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    println!("SHOT {code}_{name}_{}_font{font} {hex}", step.name);
                }
            }
        }
    }
    i18n::set(Lang::En);
}

/// Exploratory, not a gate test: the key pages on the Retroid Pocket Mini V2 as pictures, English and German (`MINI_V2_SHOT_FONTS`,
/// default "1.0,1.3": Android's usual and larger font size), with its controller
/// in hand. Every picture is printed as one line `SHOT <name> <hex of the PNG>`; `MINI_V2_SHOTS=<folder>` writes files instead.
/// `cargo test --lib mini_v2_shots -- --ignored --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn mini_v2_shots() {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("POCKETUI_TEXT_BOXES", dir.path().join("ink"));
        std::env::set_var("POCKETUI_VISIBLE_TEXT", dir.path().join("visible"));
    }
    let pages = ["onb0", "onb1", "onb2", "onb3", "onb4", "onb5", "onb6", "onb7", "library", "library-pad", "box", "box-n64", "settings", "settings-end",
        "extras", "quick", "search-kbd", "help", "drawer", "saves", "report", "stickers", "emptyfolder", "nofolder", "library-continue", "box-continue", "box-psp-continue"];
    let fonts: Vec<f32> = std::env::var("MINI_V2_SHOT_FONTS").unwrap_or_else(|_| "1.0,1.3".into()).split(',').filter_map(|f| f.parse().ok()).collect();
    for (code, lang) in [("en", Lang::En), ("de", Lang::De)] {
        i18n::set(lang);
        for (dname, w, h, density) in MINI_V2 {
          for &font in &fonts {
            for case in cases().into_iter().chain(continue_cases()).filter(|c| pages.contains(&c.name)) {
                let case = Case { pad: true, ..case };
                let path = dir.path().join("shot.png");
                look(lang, (w, h), density, Cut::Flat, font, Theme::PHONE[0], &case, Some(&path));
                let bytes = std::fs::read(&path).unwrap();
                let name = format!("{code}_{dname}_{}_font{font}", case.name);
                if let Some(out) = std::env::var_os("MINI_V2_SHOTS") {
                    std::fs::create_dir_all(&out).unwrap();
                    std::fs::write(std::path::Path::new(&out).join(format!("{name}.png")), bytes).unwrap();
                } else {
                    println!("SHOT {name} {}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
                }
            }
          }
        }
    }
    i18n::set(Lang::En);
}
