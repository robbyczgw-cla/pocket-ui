//! One hint bar on every page (docs/DESIGN.md "Hint bar", polish review 12.11. item 10): the shelf's pages, the game card,
//! the console pages, search, Curator, the first start, Settings, the quick settings and the pause menus all show their
//! keys in the same dark pill at the bottom right, the same height, the same distance from the screen's edges (flat since
//! 18.11.2026: `kit::HINT_GAP` over the bottom edge, `kit::HINT_H` high at Normal text), in every
//! look, in the plain and the Retro style, in English, German and Spanish, from 640x480 to 1920x1080.
//!
//! What a player sees is read from the frame's record (`POCKETUI_NAV_PROBE`: the bar's box `‹hints›` and one mark per
//! hint, key and word): where the bar is and how big, that no hint is a key without its word, and that no text runs into it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// The pages that change with the look (the shelf and what hangs off it), and the pages that don't.
const BY_LOOK: &[&str] = &["home", "home:-2", "shelf:gba:shelf", "shelf:psx:split", "cabinet:dd", "big-shelf", "info:split:snes:chrono", "console:gba",
    "search:results:mario", "curator:home:shelf:shelf", "starter-home"];
const SAME: &[&str] = &["setup:first", "setup:first-games", "settings", "settings:0:4", "sys:quick", "menu:pocket-core", "big-pause", "buttons:1",
    "about:device=flip2", "gallery:delete", "console:gb:credits:8"];
/// The panels pocket-ui runs on: the small ones (painted at 1280x960), the Brick's, the Mini V2's and the Pocket 5's.
const SIZES: &[(u32, u32)] = &[(640, 480), (1024, 768), (1240, 1080), (1920, 1080)];

/// The right edges the bar may end 30 px short of, and the bottom edge, on a panel (`scaled_layout::logical_size`): a
/// small panel is painted at 1280x960. On a 16:9 screen the Shelf look and the search are painted in a column in the
/// middle (`app_column::column_width`, 1440 wide on 1920x1080), Settings and the first start's steps in a wider one
/// (`app_column::page_column_width`, 1800), the other looks, the game card, the console
/// page and the pause menu across the whole screen: their bar sits in the corner of their own page, in the same place on
/// every page of a kind and the same distance from the edge everywhere.
fn page_edges((w, h): (u32, u32)) -> (Vec<i32>, i32) {
    let (w, h) = pocket_ui::scaled_layout::logical_size((w, h));
    let (w, h) = (w as i32, h as i32);
    let column = if w >= 1600 { (w * 3 / 4).max(1440).min(w) } else { w };
    let page = if w >= 1600 { (w * 15 / 16).min(2 * h).min(w) } else { w };
    let mut rights = vec![w, (w - column) / 2 + column, (w - page) / 2 + page];
    rights.sort();
    rights.dedup();
    (rights, h)
}

struct Job {
    state: String,
    text: &'static str,
    look: &'static str,
    retro: bool,
    lang: &'static str,
    size: (u32, u32),
}

struct Seen {
    bar: (i32, i32, i32, i32),
    hints: Vec<((i32, i32, i32, i32), String, String)>,
    strings: Vec<((i32, i32, i32, i32), String)>,
    faults: String,
}

fn read_record(nav: &Path, boxes: &Path) -> Seen {
    let mut bar = None;
    let mut hints = Vec::new();
    let mut strings = Vec::new();
    for line in std::fs::read_to_string(nav).unwrap_or_default().lines() {
        let f: Vec<&str> = line.splitn(5, '\t').collect();
        if f.len() < 5 {
            continue;
        }
        let r = (f[0].parse().unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap());
        if f[4] == "‹hints›" {
            bar = Some(r);
        } else if let Some(h) = f[4].strip_prefix("‹nav:hint›") {
            let (k, l) = h.split_once('\t').unwrap_or((h, ""));
            hints.push((r, k.to_string(), l.to_string()));
        } else if !f[4].starts_with("‹") {
            strings.push((r, f[4].to_string()));
        }
    }
    Seen { bar: bar.unwrap_or((0, 0, 0, 0)), hints, strings, faults: std::fs::read_to_string(boxes).unwrap_or_default() }
}

fn jobs() -> Vec<Job> {
    let mut jobs = Vec::new();
    for &size in SIZES {
        for look in ["shelf", "split", "cabinet", "big"] {
            for &state in BY_LOOK {
                jobs.push(Job { state: state.to_string(), text: "normal", look, retro: false, lang: "en", size });
            }
        }
        for &state in SAME {
            jobs.push(Job { state: state.to_string(), text: "normal", look: "shelf", retro: false, lang: "en", size });
        }
        // Retro (Departure Mono) and the longer languages: the lengths that squeeze a bar most.
        for look in ["shelf", "cabinet"] {
            for &state in BY_LOOK.iter().chain(SAME) {
                jobs.push(Job { state: state.to_string(), text: "normal", look, retro: true, lang: "en", size });
                for lang in ["de", "es", "pt", "fr", "it"] {
                    jobs.push(Job { state: state.to_string(), text: "normal", look, retro: false, lang, size });
                }
            }
        }
    }
    jobs
}

struct Stub {
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

#[test]
fn every_page_has_the_same_hint_bar_in_every_look_style_language_and_size() {
    run(jobs(), true);
}

/// The pages the layout matrices are made for (`tests/layout-risk-states.txt`: narrow columns, long metadata, keyboards,
/// bottom rows) in the longest languages with Large text, on the three panels where the bar leaves least room: no text
/// runs into the bar, and the bar is the one form.
#[test]
fn the_pages_with_most_at_the_bottom_keep_clear_of_the_bar_in_large_german_and_spanish() {
    let states: Vec<String> = std::fs::read_to_string(Path::new(&ROOT).join("tests/layout-risk-states.txt")).unwrap()
        .lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string)
        // And the other pages of the Settings family found running into the bar there (13.11.2026).
        .chain(["sync:on", "sync:open", "battery:3:samples", "battery:3:reset:samples", "battery:reset-ask", "report:es"].map(str::to_string)).collect();
    let mut jobs = Vec::new();
    for &size in &[(640, 480), (1024, 768), (1240, 1080)] {
        for state in &states {
            for lang in ["de", "es", "pt", "fr", "it"] {
                jobs.push(Job { state: state.clone(), text: "large", look: "shelf", retro: false, lang, size });
            }
        }
    }
    run(jobs, false);
}

/// `bar_everywhere`: a page without a bar fails (some of the layout matrix's pages, a message card, have none).
fn run(jobs: Vec<Job>, bar_everywhere: bool) {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let socket = tmp.join("wayland-stub");
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().expect("python3");
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
        Stub { child }
    });
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let work: Arc<Mutex<Vec<(usize, Job)>>> = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect()));
    let failed = Arc::new(Mutex::new(Vec::<String>::new()));
    // The bar's heights seen on each text size and panel: one, whatever the page, look, style or language.
    let heights = Arc::new(Mutex::new(std::collections::BTreeMap::<String, std::collections::BTreeSet<i32>>::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, failed, tmp, bin, socket, heights) = (work.clone(), failed.clone(), tmp.clone(), bin.clone(), socket.clone(), heights.clone());
            std::thread::spawn(move || loop {
                let Some((i, job)) = work.lock().unwrap().pop() else { break };
                let name = format!("{} / {} / {} / {} / {} / {}x{}", job.state, job.text, job.look, if job.retro { "retro" } else { "plain" }, job.lang, job.size.0, job.size.1);
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let mut settings = serde_json::json!({"theme": job.look, "onboarded": true, "motion": "off"});
                if job.retro {
                    settings["font"] = "retro".into();
                    settings["retro_letters"] = "departure".into();
                }
                std::fs::write(data.join("settings.json"), settings.to_string()).unwrap();
                let (png, nav, boxes) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.nav")), tmp.join(format!("{i}.boxes")));
                let mut cmd = Command::new(&bin);
                if socket.exists() {
                    cmd.env("WAYLAND_DISPLAY", &socket);
                }
                let out = cmd.args(["--snapshot", &job.state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", job.lang)
                    .env("POCKETUI_TEXT_SIZE", job.text).env("POCKETUI_SNAPSHOT_SIZE", format!("{}x{}", job.size.0, job.size.1))
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
                    .env("POCKETUI_NAV_PROBE", &nav).env("POCKETUI_TEXT_BOXES", &boxes)
                    .output().expect("run pocket-ui");
                let _ = std::fs::remove_dir_all(&data);
                if !out.status.success() {
                    failed.lock().unwrap().push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                    continue;
                }
                let seen = read_record(&nav, &boxes);
                let (rights, lh) = page_edges(job.size);
                // Flat since 18.11.2026 (the owner: "flachere Tipps"): `HINT_GAP` over the bottom edge, at least `HINT_H` high
                // and flatter than the old bar (`PILL`); the one height of a panel and text size is checked after the run.
                // Under 960 lines (1024x768) gap and height keep 960's share of the page (`kit::chrome`, 0.6.1).
                let (pad, gap, least, old) = (pocket_ui::kit::PAD, pocket_ui::kit::chrome(pocket_ui::kit::HINT_GAP, lh), pocket_ui::kit::chrome(pocket_ui::kit::HINT_H, lh), pocket_ui::kit::PILL);
                let (x0, y0, x1, y1) = seen.bar;
                let mut bad = Vec::new();
                if seen.hints.is_empty() {
                    if bar_everywhere {
                        bad.push("no hint bar".to_string());
                    }
                } else if seen.bar == (0, 0, 0, 0) {
                    // A snapshot taken in the middle of the hints' cross-fade (`cabinet:ddrra`) has painted-over marks: not a page.
                } else {
                    if !rights.iter().any(|r| x1 == r - pad) || y1 != lh - gap || y1 - y0 < least || y1 - y0 >= old {
                        bad.push(format!("the bar is at {x0},{y0}..{x1},{y1}; the one form ends {pad} from the right edge of its page ({rights:?}), {gap} over the bottom edge ({lh}), and is {least} high or a little more with larger letters, under {old}"));
                    }
                    heights.lock().unwrap().entry(format!("{} {}x{}", job.text, job.size.0, job.size.1)).or_default().insert(y1 - y0);
                    // (A chip elsewhere on the page, like the first start's "L2 Aa" at the top, is not a key of the bar.)
                    for ((hx0, hy0, hx1, hy1), key, label) in seen.hints.iter().filter(|((_, hy0, _, hy1), ..)| *hy1 > y0 && *hy0 < y1) {
                        if label.trim().is_empty() || key.trim().is_empty() {
                            bad.push(format!("a hint without its key or its word: '{key}' '{label}'"));
                        }
                        if *hx0 < x0 || *hx1 > x1 || *hy0 < y0 || *hy1 > y1 {
                            bad.push(format!("hint {key} {label} sticks out of the bar"));
                        }
                    }
                    // Every key is a chip of the one size: its box is as high as the bar.
                    // And what is written next to the bar stays out of it.
                    for ((sx0, sy0, sx1, sy1), s) in &seen.strings {
                        let inside_hint = seen.hints.iter().any(|((hx0, hy0, hx1, hy1), ..)| sx0 >= hx0 && sx1 <= hx1 && sy0 >= hy0 && sy1 <= hy1);
                        let touches = *sx0 < x1 && *sx1 > x0 && *sy0 < y1 && *sy1 > y0;
                        if touches && !inside_hint {
                            bad.push(format!("'{s}' runs into the bar"));
                        }
                    }
                }
                if seen.faults.lines().any(|l| l.contains("‹hints›")) {
                    bad.push(format!("text runs into the hints:\n{}", seen.faults));
                }
                if !bad.is_empty() {
                    failed.lock().unwrap().push(format!("{name}: {}", bad.join("; ")));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let mut failed = failed.lock().unwrap().clone();
    for (key, seen) in heights.lock().unwrap().iter().filter(|(_, h)| h.len() > 1) {
        failed.push(format!("{key}: the bar has more than one height: {seen:?}"));
    }
    failed.sort();
    assert!(failed.is_empty(), "{} page(s) with a different or a broken hint bar:\n{}", failed.len(), failed.join("\n"));
}

/// The shelf's toast stands over the hint bar for its moment (`App::paint`'s toast at the bottom): its pill cut keys in
/// half ("Open means your moment is waiting" over HOME on Android's Shelf and Curator pages, the crate's "… is on the
/// shelf now", 19.11.2026). Nothing caught it: the pill marks what is under it as covered (`boxes::covered_since`), so the
/// record lost the cut keys and the text check let them pass. Here the page is taken twice, without and with the toast:
/// a key of the bar the pill comes over must not still stand beside it, unchanged. Every look, the crate, English and
/// German, a small panel, the Mini V2's and a 20:9 phone's.
#[test]
fn the_shelf_s_toast_never_cuts_a_hint_key_in_half() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let socket = tmp.join("wayland-stub");
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().expect("python3");
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
        Stub { child }
    });
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let mut i = 0;
    let mut shot = |state: &str, look: &str, lang: &str, size: (u32, u32)| -> Result<(Seen, pocket_ui::image::Image), String> {
        i += 1;
        let data = tmp.join(format!("data-{i}"));
        copy_dir(&tmp.join("data"), &data);
        std::fs::write(data.join("settings.json"), serde_json::json!({"theme": look, "onboarded": true, "motion": "off"}).to_string()).unwrap();
        let (png, nav, boxes) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.nav")), tmp.join(format!("{i}.boxes")));
        let mut cmd = Command::new(&bin);
        if socket.exists() {
            cmd.env("WAYLAND_DISPLAY", &socket);
        }
        let out = cmd.args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
            .env("POCKETUI_TEXT_SIZE", "normal").env("POCKETUI_SNAPSHOT_SIZE", format!("{}x{}", size.0, size.1))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
            .env("POCKETUI_NAV_PROBE", &nav).env("POCKETUI_TEXT_BOXES", &boxes)
            .output().expect("run pocket-ui");
        let _ = std::fs::remove_dir_all(&data);
        if !out.status.success() {
            return Err(format!("did not render: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let picture = pocket_ui::image::load_image(&png).ok_or("no picture")?;
        Ok((read_record(&nav, &boxes), picture))
    };
    let mut failed = Vec::new();
    let mut cut_before = 0;
    for size in [(640u32, 480u32), (1240, 1080), (2400, 1080)] {
        for (look, state) in [("shelf", "home"), ("split", "home"), ("cabinet", "home"), ("big", "home"), ("shelf", "shelf:gba:shelf"), ("shelf", "crate:0")] {
            for (lang, said) in [("en", "Open means your moment is waiting"), ("de", "Offen heißt: Dein Moment wartet")] {
                let name = format!("{state} / {look} / {lang} / {}x{}", size.0, size.1);
                let (plain, toasted) = match (shot(state, look, lang, size), shot(&format!("{state}:toasted"), look, lang, size)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => {
                        failed.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                // The pill: the line's box and 24 px of pill on each side, 52 high (`App::paint`), and a few pixels round it.
                let Some(((tx0, ty0, tx1, ty1), _)) = toasted.0.strings.iter().find(|(_, s)| s.contains(said)) else {
                    failed.push(format!("{name}: no toast on the page"));
                    continue;
                };
                let mid = (ty0 + ty1) / 2;
                let pill = (tx0 - 28, mid - 30, tx1 + 28, mid + 30);
                let (a, b) = (&plain.1, &toasted.1);
                for ((hx0, hy0, hx1, hy1), key, label) in &plain.0.hints {
                    if !(*hx0 < pill.2 && *hx1 > pill.0 && *hy0 < pill.3 && *hy1 > pill.1) {
                        continue;
                    }
                    cut_before += 1;
                    // The part of the key beside the pill: still the same pixels as without the toast = half a key stands there.
                    let (mut beside, mut same) = (0, 0);
                    for y in (*hy0).max(0)..(*hy1).min(a.h as i32) {
                        for x in (*hx0).max(0)..(*hx1).min(a.w as i32) {
                            if x >= pill.0 && x < pill.2 && y >= pill.1 && y < pill.3 {
                                continue;
                            }
                            beside += 1;
                            let at = (y as u32 * a.w + x as u32) as usize;
                            same += usize::from(a.px.get(at) == b.px.get(at));
                        }
                    }
                    if beside >= 40 && same * 10 >= beside * 6 {
                        failed.push(format!("{name}: the hint {key} {label} ({hx0},{hy0}..{hx1},{hy1}) stands half under the toast's pill {pill:?}: {same} of {beside} pixels beside it unchanged"));
                    }
                }
            }
        }
    }
    assert!(cut_before > 0, "the toast's pill came over no key of the bar on any page: the check would see nothing");
    assert!(failed.is_empty(), "{} page(s) where the toast cuts the hint bar:\n{}", failed.len(), failed.join("\n"));
}
