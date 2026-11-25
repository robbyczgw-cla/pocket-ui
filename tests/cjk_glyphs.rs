//! No "?" and no empty box for letters that none of our fonts can draw (`src/latinise.rs`): every console page
//! ("Also known as Nintendo GameCube, ????????????", snapshot 2026-11-12) and the cards of games with Japanese and
//! Korean metadata (name, description, developer, publisher, genre) are rendered with `pocket-ui --snapshot` and the
//! strings the painter really painted (`POCKETUI_VISIBLE_TEXT`) are read back: none may hold a letter that neither the
//! embedded faces nor the fallback font have a glyph for, since that letter would be a "?" or nothing on the screen.
//! Without a CJK font on the system (`POCKETUI_CJK_FONT=off`, what a desktop, the Brick and the web demo have), and
//! with Noto Sans CJK (`POCKETUI_TEST_CJK_FONT`, else the unpacked ROCKNIX image in /tmp/rocknix-root-20261001; without
//! it that half says SKIPPED).
//!
//! On Linux without a compositor it starts scripts/wayland-stub.py (python3), as tests/cjk_snapshots.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn font() -> Option<PathBuf> {
    let path = std::env::var_os("POCKETUI_TEST_CJK_FONT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp/rocknix-root-20261001/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf"));
    path.is_file().then_some(path)
}

/// (file name, name, description, developer, publisher, genre, players) of the made-up games: Japanese and Korean
/// metadata as a scraper in the player's language writes it, and one game with an English description only.
const GAMES: &[(&str, &str, &str, &str, &str, &str, &str)] = &[
    ("ファイナルファンタジーVI (Japan)", "ファイナルファンタジーVI", "魔導の力を持つ少女ティナを中心に、帝国と戦う仲間たちの物語。美しい音楽とドラマチックな展開で知られる名作ロールプレイングゲーム。", "スクウェア", "スクウェア", "ロールプレイング", "1"),
    ("スーパーマリオワールド (Japan)", "スーパーマリオワールド", "マリオとルイージがヨッシーと一緒にダイナソーランドを冒険するアクションゲーム。", "任天堂", "Nintendo / 任天堂", "アクション", "1-2"),
    ("젤다의 전설 (Korea)", "젤다의 전설", "링크가 하이랄 왕국을 구하기 위해 모험을 떠나는 액션 어드벤처 게임입니다.", "닌텐도", "현대전자", "액션 어드벤처", "1"),
    ("聖剣伝説2 (Japan)", "聖剣伝説2", "", "スクウェア", "スクウェア", "", ""),
    ("Super Mario World (USA)", "Super Mario World", "Mario and Luigi explore Dinosaur Land with Yoshi.", "Nintendo EAD", "Nintendo", "Platform", "1-2"),
    ("Chrono Trigger (USA)", "Chrono Trigger", "A boy and his friends travel through time (クロノ・トリガー) to stop a catastrophe.", "スクウェア / Square", "Square", "RPG", "1"),
];

/// The library: es_systems.cfg, empty ROM files and a gamelist.xml for the games above on the Super Nintendo; every
/// console of the sheets has a system with one empty ROM, so that its page can be opened.
fn library(dir: &Path) {
    let mut cfg = String::from("<?xml version=\"1.0\"?>\n<systemList>\n");
    let snes = dir.join("roms").join("snes");
    std::fs::create_dir_all(&snes).unwrap();
    let mut xml = String::from("<?xml version=\"1.0\"?>\n<gameList>\n");
    for (file, name, desc, dev, publisher, genre, players) in GAMES {
        std::fs::write(snes.join(format!("{file}.sfc")), b"").unwrap();
        xml += &format!("<game><path>./{file}.sfc</path><name>{name}</name><desc>{desc}</desc><developer>{dev}</developer><publisher>{publisher}</publisher><genre>{genre}</genre><players>{players}</players></game>\n");
    }
    xml += "</gameList>\n";
    std::fs::write(snes.join("gamelist.xml"), xml).unwrap();
    cfg += &format!("  <system><name>snes</name><fullname>Super Nintendo</fullname><path>{}</path><extension>.sfc</extension><command>true</command></system>\n", snes.display());
    for c in pocket_ui::consoles::all().iter().filter(|c| c.id != "snes") {
        let folder = dir.join("roms").join(&c.id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Game.zip"), b"").unwrap();
        cfg += &format!("  <system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>.zip</extension><command>true</command></system>\n", folder.display(), id = c.id, name = c.name);
    }
    cfg += "</systemList>\n";
    std::fs::write(dir.join("es_systems.cfg"), cfg).unwrap();
}

/// The strings `state` paints (last frame), with the system's fallback font `font` (None: none) and with pocket-ui's own
/// Chinese font files (`own`; false: a card without them, `POCKETUI_FONTS_DIR=off`).
fn visible(tmp: &Path, stub: Option<&Stub>, state: &str, font: Option<&Path>, own: bool) -> Vec<String> {
    let name = format!("{}-{}-{}", state.replace([':', '/'], "-"), if font.is_some() { "font" } else { "none" }, if own { "own" } else { "bare" });
    let data = tmp.join(format!("data-{name}"));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"onboarded": true}"#).unwrap();
    let (png, text) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.txt")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_VISIBLE_TEXT", &text)
        .env("POCKETUI_CJK_FONT", font.map_or("off".into(), |f| f.as_os_str().to_owned()))
        .env("POCKETUI_FONTS_DIR", if own { pocket_ui_fonts() } else { "off".into() })
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} didn't render: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(&text).unwrap_or_default().lines().map(str::to_string).collect()
}

/// pocket-ui's own font files (`assets/fonts`, as `text_fallback::shipped_dir` finds them on the build computer).
fn pocket_ui_fonts() -> std::ffi::OsString {
    Path::new(&Root).join("assets/fonts").into_os_string()
}

/// The letters of `strings` that would be painted as "?" or nothing: a letter or digit that no embedded face, not
/// pocket-ui's own Chinese fonts (when `own`) and not the fallback font (`font`) has.
fn unpaintable(strings: &[String], font: Option<&Path>, own: bool) -> Vec<String> {
    let fallback = font.and_then(pocket_ui::text_fallback::open);
    let covered = |c: char| (pocket_ui::latinise::base_covers(c) && (own || !pocket_ui::text_fallback::is_cjk(c))) || fallback.as_ref().is_some_and(|f| f.has(c));
    strings
        .iter()
        .filter(|s| s.chars().any(|c| c.is_alphanumeric() && !covered(c)))
        .map(|s| format!("{s:?} has {:?}", s.chars().filter(|c| c.is_alphanumeric() && !covered(*c)).collect::<String>()))
        .collect()
}

/// Every console page, with and without its sources; with the font only the two consoles that have another name in
/// Japanese.
#[test]
fn no_console_page_paints_a_letter_no_font_has() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    let mut problems = Vec::new();
    let japanese = ["gamecube", "wonderswan"];
    for c in pocket_ui::consoles::all() {
        for state in [format!("console:{}", c.id), format!("console:{}:sources", c.id)] {
            let strings = visible(&tmp, stub.as_ref(), &state, None, false);
            assert!(!strings.is_empty(), "{state}: nothing painted");
            problems.extend(unpaintable(&strings, None, false).into_iter().map(|p| format!("{state} (no font): {p}")));
        }
    }
    for id in japanese {
        let strings = visible(&tmp, stub.as_ref(), &format!("console:{id}"), None, true);
        problems.extend(unpaintable(&strings, None, true).into_iter().map(|p| format!("console:{id} (own fonts): {p}")));
    }
    if let Some(font) = font() {
        for id in japanese {
            let strings = visible(&tmp, stub.as_ref(), &format!("console:{id}"), Some(&font), true);
            problems.extend(unpaintable(&strings, Some(&font), true).into_iter().map(|p| format!("console:{id} (font): {p}")));
        }
    } else {
        eprintln!("SKIPPED the half with Noto Sans CJK (set POCKETUI_TEST_CJK_FONT to ROCKNIX's NotoSansCJKsc-Regular.otf)");
    }
    drop(stub);
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

/// The GameCube's page says nothing about a Japanese name without any CJK font, and "Nintendo GameCube" still stands
/// there; with pocket-ui's own fonts (every handheld since Chinese, the Brick too) the Japanese name is written as it is.
#[test]
fn the_gamecube_page_has_no_question_marks_where_its_japanese_name_was() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    let strings = visible(&tmp, stub.as_ref(), "console:gamecube", None, false);
    let line = strings.iter().find(|s| s.starts_with("Also known as")).unwrap_or_else(|| panic!("no 'Also known as' line in {strings:?}"));
    assert!(line.contains("Nintendo GameCube") && !line.contains('?') && !line.contains("ニンテンドー"), "{line:?}");
    let wonder = visible(&tmp, stub.as_ref(), "console:wonderswan", None, false);
    assert!(wonder.iter().all(|s| !s.contains("ワンダー") && !s.contains('?')), "{wonder:?}");
    let own = visible(&tmp, stub.as_ref(), "console:gamecube", None, true);
    assert!(own.iter().any(|s| s.contains("ニンテンドー")), "the own fonts write it: {own:?}");
}

/// The cards of the games with Japanese and Korean metadata: the box and the info page (Split), the shelf's names
/// and the search; every one of them with and without the font.
#[test]
fn cards_of_japanese_and_korean_games_paint_no_letter_no_font_has() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    let font = font();
    let mut states = vec!["letters:snes:0".to_string(), "letters:snes:3:split".to_string(), "search:results:マリオ".to_string(), "search:results:젤다".to_string()];
    for game in ["ファイナル", "スーパーマリオ", "젤다", "聖剣", "Chrono", "Super Mario World"] {
        states.push(format!("curator:card:snes:{game}"));
        states.push(format!("info:split:snes:{game}"));
        states.push(format!("box:snes:{game}"));
    }
    let mut problems = Vec::new();
    let mut shown = std::collections::BTreeMap::new();
    let mut own = std::collections::BTreeMap::new();
    for state in &states {
        let strings = visible(&tmp, stub.as_ref(), state, None, false);
        problems.extend(unpaintable(&strings, None, false).into_iter().map(|p| format!("{state} (no font): {p}")));
        shown.insert(state.clone(), strings);
        let strings = visible(&tmp, stub.as_ref(), state, None, true);
        problems.extend(unpaintable(&strings, None, true).into_iter().map(|p| format!("{state} (own fonts): {p}")));
        own.insert(state.clone(), strings);
        if let Some(font) = &font {
            let strings = visible(&tmp, stub.as_ref(), state, Some(font), true);
            problems.extend(unpaintable(&strings, Some(font), true).into_iter().map(|p| format!("{state} (font): {p}")));
        }
    }
    drop(stub);
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
    // Something stands where the Japanese name was: its Latin letters without any CJK font, not a gap; with
    // pocket-ui's own fonts the name as written.
    for state in ["curator:card:snes:ファイナル", "info:split:snes:ファイナル"] {
        let card = &shown[state];
        assert!(card.iter().any(|s| s.contains("Fainarufantajii")), "{state}: the name in Latin letters: {card:?}");
        let card = &own[state];
        assert!(card.iter().any(|s| s.contains("ファイナル")) && card.iter().all(|s| !s.contains("Fainarufantajii")), "{state}: the name as written: {card:?}");
    }
    // The Korean card is Korean even without a system font: pocket-ui ships hangul (`text_fallback::korean`), so its
    // name and its description are painted as written, never in Latin letters (until 21.11.2026: "Jeldaui Jeonseol").
    let info = &shown["info:split:snes:젤다"];
    assert!(info.iter().any(|s| s.contains("젤다의")), "{info:?}");
    assert!(info.iter().any(|s| s.contains("링크가")), "the description too: {info:?}");
    assert!(info.iter().all(|s| !s.contains('?') && !s.contains("Jeldaui") && !s.contains("Ringkeu")), "{info:?}");
}

/// Lead 22.11.2026: a language still being translated (Korean, `Lang::complete` false) is not among the welcome's
/// language pills (the Settings row walks the same `Lang::row`); a developer sees it with `POCKETUI_LANG_PREVIEW=1`.
/// Before, "한국어" stood there as the seventh choice and its pages were English.
#[test]
fn the_welcome_offers_korean_only_when_it_is_complete_or_previewed() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    let shown = |preview: bool| {
        let data = tmp.join(format!("data-welcome-{preview}"));
        std::fs::create_dir_all(&data).unwrap();
        let (png, text) = (tmp.join(format!("welcome-{preview}.png")), tmp.join(format!("welcome-{preview}.txt")));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        if preview {
            cmd.env("POCKETUI_LANG_PREVIEW", "1");
        }
        let out = cmd
            .args(["--snapshot", "setup:first", png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_VISIBLE_TEXT", &text)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(&text).unwrap_or_default()
    };
    let (plain, preview) = (shown(false), shown(true));
    drop(stub);
    assert!(plain.contains("Italiano") && plain.contains("Deutsch"), "the languages are there: {plain}");
    // Korean is complete (switched on 2026-11): offered to everyone; before, only to a developer's preview.
    assert_eq!(plain.contains("한국어"), pocket_ui::i18n::Lang::Ko.complete(), "Korean is offered exactly when its translation is complete: {plain}");
    assert!(preview.contains("한국어"), "a developer's preview shows it: {preview}");
    // Chinese is being translated (24.11.2026): not offered yet, a developer's preview shows its name in its own letters.
    assert_eq!(plain.contains("简体中文"), pocket_ui::i18n::Lang::Zh.complete(), "{plain}");
    assert!(preview.contains("简体中文"), "a developer's preview shows it: {preview}");
}
