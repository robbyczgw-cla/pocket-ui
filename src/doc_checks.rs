//! Doc-drift checks: the user guide (`docs/user/`, which is also the manual on the device) and the developer documents
//! against the code, so a feature that changes a menu, a row, a default or a module can't leave the documentation
//! behind without a red test. What each check holds is listed in docs/dev/tests.md, "Documentation checks".
//!
//! The checks read the Markdown as it is in the repository (no network, a few milliseconds). They compare names and
//! structure, not prose: a row of a table, a bold name, a link. A failure names the file and what is missing; fix the
//! document (or the code), never the check.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pocket_ui::manual;

/// As cargo says it when the test runs (not `env!`: a binary reused from a shared build folder would read a gone copy).
fn root() -> PathBuf {
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| env!("CARGO_MANIFEST_DIR").into()))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// ---- reading Markdown -------------------------------------------------------------------------------------------

/// `md` with HTML comments blanked out (their line breaks kept, so line numbers stay right).
fn without_comments(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(at) = rest.find("<!--") {
        out.push_str(&rest[..at]);
        let end = rest[at..].find("-->").map_or(rest.len(), |e| at + e + 3);
        out.extend(rest[at..end].chars().filter(|c| *c == '\n'));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The lines of `md` that are prose: not in a fenced code block, not in a comment. (1-based line number, line.)
fn prose(md: &str) -> Vec<(usize, String)> {
    let mut fenced = false;
    let mut out = Vec::new();
    for (i, line) in without_comments(md).lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            out.push((i + 1, line.to_string()));
        }
    }
    out
}

/// A line without its inline code spans (a link written in backticks is an example, not a link).
fn without_code(line: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    for c in line.chars() {
        if c == '`' {
            code = !code;
        } else if !code {
            out.push(c);
        }
    }
    out
}

/// The headings of `md` outside code blocks: (level, text).
fn headings(md: &str) -> Vec<(usize, String)> {
    prose(md)
        .into_iter()
        .filter_map(|(_, line)| {
            let level = line.chars().take_while(|c| *c == '#').count();
            let text = line.get(level..)?;
            ((1..=6).contains(&level) && text.starts_with(' ')).then(|| (level, text.trim().trim_end_matches('#').trim().to_string()))
        })
        .collect()
}

/// The part of `md` under the heading `title` (any level), up to the next heading of the same or a higher level.
fn section(md: &str, title: &str) -> String {
    let lines = prose(md);
    let level_of = |line: &str| {
        let n = line.chars().take_while(|c| *c == '#').count();
        (n >= 1 && line[n..].starts_with(' ')).then_some(n)
    };
    let start = lines
        .iter()
        .position(|(_, l)| level_of(l).is_some() && l.trim_start_matches('#').trim() == title)
        .unwrap_or_else(|| panic!("no heading {title:?}"));
    let level = level_of(&lines[start].1).unwrap_or(1);
    lines[start + 1..]
        .iter()
        .take_while(|(_, l)| level_of(l).is_none_or(|n| n > level))
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body rows of the first table in `text`, as trimmed cells (header and alignment row left out).
fn first_table(text: &str) -> Vec<Vec<String>> {
    let rows: Vec<Vec<String>> = text
        .lines()
        .skip_while(|l| !l.trim_start().starts_with('|'))
        .take_while(|l| l.trim_start().starts_with('|'))
        .map(|l| l.trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect())
        .collect();
    assert!(rows.len() >= 2, "no table in {text:.80?}");
    rows.into_iter().skip(2).collect()
}

/// The text of the first bold span of `cell` (`**Look**` → `Look`), or the cell itself.
fn bold(cell: &str) -> String {
    match cell.split("**").nth(1) {
        Some(b) if cell.contains("**") => b.to_string(),
        _ => cell.to_string(),
    }
}

/// Lower case, no blanks: "HOME + Left / Right" and "HOME + Left/Right" read the same.
fn squeezed(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

/// The anchor GitHub gives a heading: lower case; letters, digits, '-', '_' and blanks kept, blanks become '-'.
/// Inline markup counts by its text (`**b**` and `` `c` `` and `[t](u)`).
fn slug(heading: &str) -> String {
    let mut text = String::new();
    let mut rest = heading;
    // [text](url) → text
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|c| open + c) else { break };
        let Some(end) = rest[close..].find(')').map(|e| close + e) else { break };
        text.push_str(&rest[..open]);
        text.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    text.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Every anchor of `md`: its headings' slugs (repeated headings get "-1", "-2" as on GitHub) and `<a id>`/`<a name>`.
fn anchors(md: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, h) in headings(md) {
        let base = slug(&h);
        let mut s = base.clone();
        let mut n = 0;
        while out.contains(&s) {
            n += 1;
            s = format!("{base}-{n}");
        }
        out.insert(s);
    }
    for (_, line) in prose(md) {
        for key in ["<a id=\"", "<a name=\""] {
            for part in line.split(key).skip(1) {
                if let Some(id) = part.split('"').next() {
                    out.insert(id.to_string());
                }
            }
        }
    }
    out
}

/// The links of `md` outside code: (line, target) for every `[text](target)` and `![alt](target)`.
fn links(md: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in prose(md) {
        let line = without_code(&line);
        let mut rest = line.as_str();
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            // Up to the closing bracket, with one level of brackets inside (Wikipedia URLs have them).
            let mut depth = 0;
            let end = after.char_indices().find(|&(_, c)| match c {
                '(' => {
                    depth += 1;
                    false
                }
                ')' if depth == 0 => true,
                ')' => {
                    depth -= 1;
                    false
                }
                _ => false,
            });
            let Some((end, _)) = end else { break };
            let target = after[..end].split_whitespace().next().unwrap_or("").trim_matches(|c| c == '<' || c == '>');
            out.push((n, target.to_string()));
            rest = &after[end..];
        }
    }
    out
}

/// The Markdown files the link check reads: the root's, and everything under `docs/` and `assets/`.
fn markdown_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for f in std::fs::read_dir(root()).unwrap().flatten() {
        if f.path().extension().is_some_and(|x| x == "md") {
            out.push(f.path());
        }
    }
    walk(&root().join("docs"), &mut out);
    walk(&root().join("assets"), &mut out);
    out.sort();
    out
}

/// The `.rs` files under `src/`, recursively.
fn rust_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src"), &mut out);
    out.sort();
    out
}

/// Whether `text` names `word` on its own: not as the end of a longer name (`motion.rs` is not in `app_motion.rs`).
fn names(text: &str, word: &str) -> bool {
    text.match_indices(word)
        .any(|(at, _)| !text[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_'))
}

// ---- the user guide against the code ------------------------------------------------------------------------------

#[test]
fn the_settings_table_lists_every_settings_row_in_order() {
    // Settings is three pages (the manual's 30 000 px limit): how it opens, Look, Playing and Games; Screen & sound and Controls; then
    // Connections, System and Help & tools.
    let md = format!("{}\n{}\n{}\n{}", read("docs/user/settings.md"), read("docs/user/settings-playing.md"), read("docs/user/settings-sound.md"), read("docs/user/settings-system.md"));
    // The categories, in their order, each naming its entries.
    let open = first_table(&section(&md, "Open Settings"));
    let doc: Vec<String> = open.iter().map(|r| bold(&r[0])).collect();
    // The Look category carries a zero-width mark (so German can translate it apart from its row "Look").
    let code: Vec<&str> = super::SETTINGS_GROUPS.iter().take(super::app_settings_groups::SHOWN).map(|c| c.0.trim_end_matches('\u{200b}')).collect();
    assert_eq!(
        doc, code,
        "docs/user/settings.md, \"Open Settings\": one row per category of main.rs SETTINGS_GROUPS, in its order"
    );
    for (row, (name, entries)) in open.iter().zip(super::SETTINGS_GROUPS.iter()) {
        for (entry, _, _) in entries.iter() {
            assert!(row[1].contains(entry), "docs/user/settings.md, \"Open Settings\": the row of {name} names {entry}");
        }
    }
    // Under each category's own heading, one row per entry, in its order: together every row of SETTINGS.
    let mut all = Vec::new();
    for (name, entries) in super::SETTINGS_GROUPS.iter() {
        let name = name.trim_end_matches('\u{200b}');
        let doc: Vec<String> = first_table(&section(&md, name)).iter().map(|r| bold(&r[0])).collect();
        let code: Vec<&str> = entries.iter().map(|s| s.0).collect();
        assert_eq!(doc, code, "docs/user/settings.md, \"{name}\": one row per entry of the category in main.rs SETTINGS_GROUPS, in its order");
        all.extend(doc);
    }
    let code: Vec<&str> = super::SETTINGS.iter().map(|s| s.0).collect();
    assert_eq!(all, code, "docs/user/settings.md: every row of main.rs SETTINGS, in its order");
}

#[test]
fn every_cozy_extra_is_described_with_its_start() {
    let rows = first_table(&section(&read("docs/user/settings.md"), "Cozy extras"));
    let doc: Vec<String> = rows.iter().map(|r| bold(&r[0])).collect();
    let mut code: Vec<&str> = pocket_ui::cozy_extras::Extra::shown().iter().map(|e| e.label()).collect();
    code.push(super::app_cozy_extras::GBA_MODEL_NAME);
    assert_eq!(doc, code, "docs/user/settings.md, \"Cozy extras\": every switch and the GBA model selector, in page order");
    for (r, e) in rows.iter().zip(pocket_ui::cozy_extras::Extra::shown()) {
        // Playful cases follows Animations until it is switched (`Extras::playful_animations`).
        let start = if e == pocket_ui::cozy_extras::Extra::Playful { "On with Cozy, Off with Quick" } else if e.default_on() { "On" } else { "Off" };
        assert_eq!(r[2], start, "docs/user/settings.md, \"Cozy extras\": {}'s start", e.label());
    }
    assert_eq!(rows.last().unwrap()[2], pocket_ui::system_art::GbaModel::default().label(), "Cozy extras: the GBA selector starts with the classic model");
}

#[test]
fn settings_curator_rows_and_their_start_settings_are_described() {
    let md = read("docs/user/settings-playing.md");
    let rows = first_table(&section(&md, "Curator"));
    let doc: Vec<(String, String)> = rows.iter().map(|r| (bold(&r[0]), r[1].clone())).collect();
    let mut prefs = pocket_ui::curator::prefs::Prefs::default();
    let code: Vec<(String, String)> = super::app_curator::SETTINGS_ROWS
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            (
                name.to_string(),
                if *super::app_curator::switch(&mut prefs, i) { "On" } else { "Off" }.to_string(),
            )
        })
        .collect();
    assert_eq!(
        doc, code,
        "docs/user/settings.md, \"Curator\": the rows of Settings → Curator and their start settings"
    );
}

#[test]
fn sleep_and_power_rows_their_start_values_and_choices_are_described() {
    use pocket_ui::sleep_settings::{Config, Hold, Row, ShortPress, ROWS, SCREEN_OFF_CHOICES, SLEEP_AFTER_CHOICES};
    let start = Config::default();
    for (page, heading) in [
        #[cfg(not(feature = "brick"))]
        ("docs/user/quick-settings.md", "Sleep & power settings"),
        #[cfg(feature = "brick")]
        ("docs/user/brick-quick-settings.md.inc", "Sleep & power settings"),
    ] {
        let md = read(page);
        let rows = first_table(&section(&md, heading));
        let doc: Vec<String> = rows.iter().map(|r| bold(&r[0])).collect();
        let shown: Vec<Row> = ROWS.to_vec();
        let code: Vec<&str> = shown.iter().map(|r| r.label()).collect();
        assert_eq!(doc, code, "{page}, \"{heading}\": the rows of Settings → System → Sleep & power");
        for (row, cells) in shown.iter().zip(&rows) {
            let start = format!("**{}**", start.value(*row));
            assert!(cells[1].contains(&start), "{page}, {}: the start value {start} in bold, in {:?}", row.label(), cells[1]);
            let choices: Vec<String> = match row {
                Row::ScreenOff => SCREEN_OFF_CHOICES.iter().map(|c| pocket_ui::sleep_settings::screen_off_label(*c)).collect(),
                Row::SleepAfter => SLEEP_AFTER_CHOICES.iter().map(|c| pocket_ui::sleep_settings::sleep_after_label(*c)).collect(),
                Row::SwitchOff => pocket_ui::auto_off::CHOICES.iter().map(|c| pocket_ui::auto_off::label(*c)).collect(),
                Row::ShortPress => ShortPress::ALL.iter().map(|p| p.label().to_string()).collect(),
                Row::Hold => Hold::ALL.iter().map(|h| h.label().to_string()).collect(),
            };
            for c in choices {
                assert!(cells[1].contains(&c), "{page}, {}: the choice {c:?}", row.label());
            }
        }
    }
    // Settings and the power button's own page point to the table.
    assert!(section(&read("docs/user/settings-system.md"), "Sleep & power").contains("quick-settings.md#sleep--power-settings"));
    assert!(section(&read("docs/user/controls-settings.md"), "Power and volume").contains("quick-settings.md#sleep--power-settings"));
}

#[test]
fn performance_and_fan_rows_and_their_start_values_are_described() {
    use pocket_ui::perf_fan_page::{fan_label, on_off, performance_label, PerfFanPage, ROWS};
    use pocket_ui::power::{Fan, Performance};
    let md = read("docs/user/performance.md");
    let rows = first_table(&section(&md, "Performance and fan"));
    let page = PerfFanPage::new(pocket_ui::perf_fan::Prefs::default());
    let doc: Vec<String> = rows.iter().map(|r| bold(&r[0])).collect();
    let code: Vec<&str> = ROWS.iter().map(|r| page.label(*r)).collect();
    assert_eq!(doc, code, "docs/user/performance.md, \"Performance and fan\": the page's rows");
    for (row, cells) in ROWS.iter().zip(&rows) {
        let start = format!("**{}**", page.value(*row));
        assert!(
            cells[1].contains(&start),
            "performance.md, {}: the start value {start} in bold, in {:?}",
            page.label(*row),
            cells[1]
        );
        let labels: Vec<&str> = match row {
            pocket_ui::perf_fan_page::Row::GamesPerformance => [Performance::Battery, Performance::Auto, Performance::Performance]
                .map(performance_label)
                .to_vec(),
            pocket_ui::perf_fan_page::Row::SmoothFan => [true, false].map(on_off).to_vec(),
            _ => [Fan::Quiet, Fan::Auto, Fan::Cool].map(fan_label).to_vec(),
        };
        for l in labels {
            assert!(cells[1].contains(l), "performance.md, {}: the choice {l:?}", page.label(*row));
        }
    }
}

#[test]
fn the_quick_settings_table_lists_every_row_in_order() {
    let md = read("docs/user/quick-settings.md");
    let doc: Vec<String> = first_table(&section(&md, "Quick settings")).iter().map(|r| bold(&r[0])).collect();
    let code: Vec<&str> = pocket_ui::quick::Row::ALL.iter().map(|r| r.label()).collect();
    assert_eq!(doc, code, "docs/user/quick-settings.md, \"Quick settings\": one row per panel row, in order");
    // The two choice rows name their choices, in the words the panel uses.
    use pocket_ui::perf_fan_page::{fan_label, performance_label};
    let text = section(&md, "Quick settings");
    for l in pocket_ui::quick_profiles::PERFORMANCE.map(performance_label).iter().chain(pocket_ui::quick_profiles::FAN.map(fan_label).iter()) {
        assert!(text.contains(l), "quick-settings.md, Quick settings: the choice {l:?}");
    }
}

/// The Look row's line says the picker's order: left and right go through them as they are named.
#[test]
fn the_look_row_names_the_looks_in_the_pickers_order() {
    use pocket_ui::settings::Theme;
    let line = super::SETTINGS.iter().find(|s| s.0 == "Layout").expect("the Settings row").2;
    let at: Vec<usize> = Theme::ALL.iter().map(|t| line.find(t.name()).unwrap_or_else(|| panic!("{} in {line}", t.name()))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{line}");
}

#[test]
fn look_appearance_motion_on_start_and_screen_frames_choices_are_described() {
    use pocket_ui::settings::{Appearance, Motion, Theme};
    let playing = read("docs/user/playing.md");
    let looks: Vec<String> = first_table(&section(&playing, "Looks")).iter().map(|r| bold(&r[0])).collect();
    let themes: Vec<&str> = Theme::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(
        looks, themes,
        "docs/user/playing.md, \"Looks\": one row per look, in the order of Settings → Look"
    );
    let settings = format!("{}\n{}\n{}", read("docs/user/settings.md"), read("docs/user/settings-playing.md"), read("docs/user/settings-sound.md"));
    for (title, names) in [
        ("Appearance", Appearance::ALL.iter().map(|a| a.name()).collect::<Vec<_>>()),
        ("Motion", Motion::ALL.iter().map(|m| m.name()).collect()),
        (
            "On start",
            [pocket_ui::on_start::OnStart::Home, pocket_ui::on_start::OnStart::Continue]
                .iter()
                .map(|o| o.label())
                .collect(),
        ),
        ("Screen frames", pocket_ui::settings::Frames::ALL.iter().map(|f| f.label()).collect()),
        ("Over the game", pocket_ui::over_game::Level::ALL.iter().map(|l| l.label()).collect()),
        ("Button layout", pocket_ui::button_layout::ButtonLayout::ALL.iter().map(|l| l.label()).collect()),
    ] {
        let text = section(&settings, title);
        for name in names {
            assert!(
                text.contains(&format!("**{name}**")),
                "docs/user/settings.md, \"{title}\": the choice **{name}**"
            );
        }
    }
    // "the four looks" stayed in two pages after the Cabinet came: a count in words must be the real one.
    let words = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
    let want = words[Theme::ALL.len() - 1];
    for file in user_pages().iter().map(|p| format!("docs/user/{p}.md")).chain(["README.md".to_string()]) {
        for (n, line) in prose(&read(&file)) {
            let lower = line.to_lowercase();
            for w in words {
                assert!(
                    w == want || !lower.contains(&format!("{w} looks")),
                    "{file}:{n}: \"{w} looks\", there are {want}"
                );
            }
        }
    }
}

#[test]
fn the_pause_menu_table_matches_the_menu_of_each_emulator() {
    use super::MainItem::{self, *};
    use pocket_ui::quit::Saver;
    // Every row of the menu. A new one has to be added here (the match stops compiling) and to the guide.
    let all = [Continue, Save, Load, Earlier, Disc, Options, Achievements, Zoom, PixelPath, Players, Quit];
    for item in all {
        match item {
            Continue | Save | Load | Earlier | Disc | Options | Achievements | Zoom | PixelPath | Players | Quit => {}
        }
    }
    let md = read("docs/user/pause-menu.md");
    for item in all {
        assert!(
            md.contains(&format!("**{}**", item.label())),
            "docs/user/pause-menu.md describes **{}**",
            item.label()
        );
    }
    // The emulators' states known (a PS2 game before ARMSX2 named it has no Save or Load either: described in prose).
    // Each row as the menu shows it with Settings' defaults (Continue where I quit on): Quit reads "Save & quit" where
    // it keeps the moment (`quit_saves`). The table said "Quit" for all of them while the menu said "Save & quit".
    let page = |saver: Saver, achievements: bool| {
        let saves = pocket_ui::pause_saves::Saves::of(saver, true, None);
        (super::main_items(matches!(saver, Saver::PocketCore { .. }), saves, achievements), saves)
    };
    let mut seen = BTreeSet::new();
    for row in first_table(&md) {
        let kind = row[0].as_str();
        // RetroAchievements' Hardcore has its row once it is offered (`achievements::hardcore::OFFERED`,
        // docs/dev/hardcore-guide.md); until then the guide has none.
        let (key, (want, saves)) = if kind.contains("Hardcore") {
            assert!(pocket_ui::achievements::hardcore::OFFERED, "docs/user/pause-menu.md has a Hardcore row, but Hardcore is not offered");
            let saves = super::app_achievements::hardcore_saves(pocket_ui::pause_saves::Saves::of(Saver::PocketCore { states: true }, true, None), true);
            ("pocket-core hardcore", (super::main_items(true, saves, true), saves))
        } else if kind.contains("signed in") {
            ("pocket-core signed in", page(Saver::PocketCore { states: true }, true))
        } else if kind.contains("no save states") {
            ("pocket-core without states", page(Saver::PocketCore { states: false }, false))
        } else if kind.starts_with("pocket-ui's own host") {
            ("pocket-core", page(Saver::PocketCore { states: true }, false))
        } else if kind.contains("PlayStation 2") {
            ("standalone", page(Saver::Standalone, false))
        } else if kind.starts_with("Xbox") {
            // hakuX while its core has no save states: the line in place of Save and Load.
            let saves = pocket_ui::pause_saves::Saves::of(Saver::Standalone, false, Some(pocket_ui::hakux::NO_STATES));
            ("hakuX", (super::main_items(false, saves, false), saves))
        } else if kind.starts_with("RetroArch") {
            ("retroarch", page(Saver::RetroArch, false))
        } else if kind.starts_with("PortMaster") {
            ("port", page(Saver::Port, false))
        } else if kind.starts_with("Other emulators") {
            ("other", page(Saver::Other, false))
        } else {
            panic!("docs/user/pause-menu.md: a row for an unknown kind of emulator: {kind:?}")
        };
        let doc: Vec<&str> = row[1].split(", ").collect();
        let quit_saves = super::quit_saves(false, saves);
        let code: Vec<&str> = want.iter().map(|i| i.shown(quit_saves)).collect();
        assert_eq!(doc, code, "docs/user/pause-menu.md, the row {kind:?}");
        seen.insert(key);
    }
    let kinds = if pocket_ui::achievements::hardcore::OFFERED { 9 } else { 8 };
    assert_eq!(seen.len(), kinds, "docs/user/pause-menu.md: a row for each kind of emulator, {seen:?}");
    // What Quit does is a setting, not a row: the guide names it beside Quit.
    let name = super::SETTINGS.iter().find(|s| matches!(s.1, super::Opens::RememberQuit)).expect("the Settings row").0;
    assert!(md.contains(&format!("**{name}**")), "docs/user/pause-menu.md names Settings → **{name}** under Quit");
    assert!(!md.contains("Quit without saving**"), "docs/user/pause-menu.md: \"Quit without saving\" is no row any more");
}

#[test]
fn every_option_group_setting_and_choice_is_described() {
    // The Screen frame and Screen orientation sections have a page of their own (options-screen.md).
    let md = format!("{}\n{}", read("docs/user/options.md"), read("docs/user/options-screen.md"));
    let flat = squeezed(&md);
    let titles: Vec<String> = headings(&md).into_iter().map(|(_, h)| h).collect();
    // A setting is named in bold or is the heading of its own part.
    let described = |name: &str| md.contains(&format!("**{name}**")) || titles.iter().any(|t| t == name);
    for g in pocket_ui::options::GROUPS {
        assert!(described(g.name), "docs/user/options.md: the group **{}**", g.name);
        for s in g.settings {
            assert!(described(s.name), "docs/user/options.md: the setting **{}** ({})", s.name, g.name);
            for c in s.choices {
                assert!(flat.contains(&squeezed(c.label)), "docs/user/options.md: {} → {:?}", s.name, c.label);
            }
        }
    }
    // The Extras table marks each start value in bold.
    let extras = pocket_ui::options::GROUPS.iter().find(|g| g.name == "Speed, rumble, fan").expect("the Speed, rumble, fan group");
    let rows = first_table(&section(&md, "Speed, rumble, fan"));
    for s in extras.settings {
        let row = rows
            .iter()
            .find(|r| bold(&r[0]) == s.name)
            .unwrap_or_else(|| panic!("options.md, Speed, rumble, fan: a row for {}", s.name));
        let start = format!("**{}**", s.choices[s.default].label);
        assert!(row[1].contains(&start), "options.md, Speed, rumble, fan, {}: the start value {start}", s.name);
    }
    for name in ["Emulator", "Controls", "Cheats", "Keep for", "Restore defaults"] {
        assert!(md.contains(&format!("**{name}**")), "docs/user/options.md: the entry **{name}**");
    }
    for choice in ["This game", "This console"] {
        assert!(md.contains(&format!("**{choice}**")), "docs/user/options.md: Keep for → **{choice}**");
    }
}

#[test]
fn every_lens_filter_is_described() {
    let lens = section(&read("docs/user/curator.md"), "Lens");
    // Markdown wraps lines anywhere: values are compared without blanks and case.
    let flat = squeezed(&lens);
    for f in pocket_ui::curator::Facet::ALL {
        assert!(names(&lens, f.label()), "docs/user/curator.md, \"Lens\": the filter {}", f.label());
        // The values every library has the same of (Length's steps, Story, Grinding, Reading) by their names.
        for value in f.fixed_labels() {
            assert!(flat.contains(&squeezed(&value)), "docs/user/curator.md, \"Lens\": {} → {value:?}", f.label());
        }
    }
}

#[test]
fn every_feature_card_has_a_user_guide_section() {
    // Also the TODO entries: whoever wires one finds its section already named.
    for f in pocket_ui::features::FEATURES {
        let (page, anchor) = f.guide.split_once('#').unwrap_or_else(|| panic!("feature `{}`: guide {:?} is not page.md#anchor", f.id, f.guide));
        let rel = format!("docs/user/{page}");
        assert!(root().join(&rel).exists(), "feature `{}`: {rel} does not exist", f.id);
        assert!(anchors(&read(&rel)).contains(anchor), "feature `{}`: {rel} has no heading for #{anchor}", f.id);
    }
}

#[test]
fn every_tour_basic_has_a_user_guide_section() {
    for b in pocket_ui::features::BASICS {
        let (page, anchor) = b.guide.split_once('#').unwrap_or_else(|| panic!("tour card `{}`: guide {:?} is not page.md#anchor", b.id, b.guide));
        let rel = format!("docs/user/{page}");
        assert!(root().join(&rel).exists(), "tour card `{}`: {rel} does not exist", b.id);
        assert!(anchors(&read(&rel)).contains(anchor), "tour card `{}`: {rel} has no heading for #{anchor}", b.id);
    }
    let tour = read("docs/user/tour.md");
    for b in pocket_ui::features::BASICS {
        assert!(tour.contains(&format!("**{}**", b.title)), "docs/user/tour.md names the Tour card {:?} in bold", b.title);
    }
}

#[test]
fn the_tour_table_lists_every_switch_card_and_what_the_profiles_do() {
    use pocket_ui::features::{available, Pick};
    let tour = read("docs/user/tour.md");
    let word = |p: Pick| match p {
        Pick::On => "On",
        Pick::Off => "Off",
        Pick::Keep => "As it is",
    };
    let rows: Vec<Vec<String>> = first_table(&tour);
    let cards: Vec<_> = available().map(|(_, f)| f).filter(|f| f.switch.is_some()).collect();
    let got: Vec<(String, String, String)> = rows.iter().map(|r| (bold(&r[0]), r.get(1).cloned().unwrap_or_default(), r.get(2).cloned().unwrap_or_default())).collect();
    let want: Vec<(String, String, String)> = cards.iter().map(|f| (f.title.to_string(), word(f.simple).to_string(), word(f.enthusiast).to_string())).collect();
    assert_eq!(got, want, "docs/user/tour.md: one row per switch card of features::FEATURES, in order: | **Title** | The shelf | Enthusiast | … |");
    for f in available().map(|(_, f)| f).filter(|f| f.switch.is_none()) {
        assert!(tour.contains(&format!("**{}**", f.title)), "docs/user/tour.md: the card **{}** (no switch)", f.title);
    }
}

// ---- the manual on the device -------------------------------------------------------------------------------------

/// The pages of `docs/user/`, by file name without `.md`, sorted.
fn user_pages() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root().join("docs/user"))
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".md").map(str::to_string))
        .collect();
    out.sort();
    out
}

#[test]
fn every_user_page_is_in_the_manual_or_marked_desktop_only() {
    let in_manual: BTreeSet<&str> = manual::PAGES.iter().map(|(n, _)| *n).collect();
    for p in user_pages() {
        let skipped = cfg!(feature = "brick") && manual::BRICK_SKIPS.contains(&p.as_str());
        let (m, d) = (in_manual.contains(p.as_str()), manual::DESKTOP_ONLY.contains(&p.as_str()) || skipped);
        assert!(
            m != d,
            "docs/user/{p}.md: in manual::PAGES or in manual::DESKTOP_ONLY, exactly one (manual {m}, desktop {d})"
        );
    }
    for (n, md) in manual::PAGES {
        #[cfg(not(feature = "brick"))]
        assert_eq!(*md, read(&format!("docs/user/{n}.md")), "manual::PAGES {n:?} bundles docs/user/{n}.md");
        #[cfg(feature = "brick")]
        {
            let source = if matches!(*n, "index" | "controls" | "copy-games" | "performance" | "quick-settings") {
                format!("docs/user/brick-{n}.md.inc")
            } else {
                format!("docs/user/{n}.md")
            };
            assert_eq!(*md, read(&source), "manual::PAGES {n:?} bundles {source}");
        }
    }
}

#[test]
fn the_first_page_lists_the_manual_pages_in_their_order() {
    // The Brick's manual starts with its own page, with the same list but for `manual::BRICK_SKIPS`.
    let first = if cfg!(feature = "brick") { "docs/user/brick-index.md.inc" } else { "docs/user/index.md" };
    let pages = section(&read(first), "Pages");
    // Pages of the guide are linked by their file name; the developer documents are one folder up.
    let listed: Vec<String> = links(&pages)
        .into_iter()
        .filter(|(_, t)| !t.contains('/'))
        .filter_map(|(_, t)| t.strip_suffix(".md").map(str::to_string))
        .collect();
    let manual: Vec<&str> = manual::PAGES.iter().map(|(n, _)| *n).filter(|n| *n != "index").collect();
    assert_eq!(
        listed, manual,
        "docs/user/index.md, \"Pages\": every page after this one, in manual::PAGES order"
    );
}

// ---- the developer documents ----------------------------------------------------------------------------------------

#[test]
#[ignore = "release check: bookkeeping (scripts/bookkeeping.sh)"]
fn the_documentation_index_lists_every_document() {
    let index = read("docs/README.md");
    let linked: BTreeSet<String> = links(&index).into_iter().map(|(_, t)| t).collect();
    let mut missing = Vec::new();
    for dir in ["docs", "docs/user", "docs/dev"] {
        for e in std::fs::read_dir(root().join(dir)).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") || (dir == "docs" && name == "README.md") {
                continue;
            }
            let rel = if dir == "docs" { name } else { format!("{}/{name}", &dir[5..]) };
            if !linked.contains(&rel) {
                missing.push(rel);
            }
        }
    }
    missing.sort();
    assert!(missing.is_empty(), "docs/README.md links every document; missing: {missing:?}");
}

// Bookkeeping, not a guard (Fable 5.2 friction audit, 14.11.): a warning in the nightly run and the release check.
#[test]
#[ignore = "release check: bookkeeping (scripts/bookkeeping.sh)"]
fn developing_md_names_every_source_module() {
    let layout = section(&read("docs/dev/layout.md"), "Repository layout");
    let binaries = section(&read("docs/dev/building.md"), "Building");
    let mut missing = Vec::new();
    for e in std::fs::read_dir(root().join("src")).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let wanted = if e.path().is_dir() { format!("{name}/") } else { name.clone() };
        if e.path().is_dir() && name == "bin" {
            for b in std::fs::read_dir(e.path()).unwrap().flatten() {
                let b = b.file_name().to_string_lossy().to_string();
                if !names(&binaries, &b) {
                    missing.push(format!("bin/{b} (the binaries table under Building)"));
                }
            }
        } else if (name.ends_with(".rs") || name.ends_with(".c") || e.path().is_dir()) && !names(&layout, &wanted) {
            missing.push(wanted);
        }
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "docs/dev/layout.md, \"Repository layout\" names every module of src/; missing: {missing:?}"
    );
}

// Bookkeeping, not a guard (Fable 5.2 friction audit, 14.11.): a warning in the nightly run and the release check.
#[test]
#[ignore = "release check: bookkeeping (scripts/bookkeeping.sh)"]
fn every_environment_variable_is_documented() {
    let env = section(&read("docs/dev/environment.md"), "Environment variables");
    let prefix = ["\"", "POCKET"].concat();
    let mut missing = BTreeSet::new();
    for file in rust_sources() {
        if file.ends_with("doc_checks.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (at, _) in text.match_indices(&prefix) {
            let name: String = text[at + 1..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            let closed = text[at + 1 + name.len()..].starts_with('"');
            if closed && name.contains('_') && !env.contains(&format!("`{name}")) {
                missing.insert(format!("{name} ({})", file.strip_prefix(root()).unwrap().display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "docs/dev/environment.md, \"Environment variables\" has every variable the code reads; missing: {missing:?}"
    );
}

#[test]
fn every_relative_link_and_anchor_resolves() {
    let mut broken = Vec::new();
    for file in markdown_files() {
        let md = std::fs::read_to_string(&file).unwrap();
        let rel = file.strip_prefix(root()).unwrap().display().to_string();
        for (line, target) in links(&md) {
            if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            // A picture the manual draws itself (`manual::Block::Picture`): it has to be one it knows.
            if let Some(name) = target.strip_prefix("pocket-ui:") {
                if !manual::PICTURES.contains(&name) {
                    broken.push(format!("{rel}:{line}: {target} (no such picture in the manual)"));
                }
                continue;
            }
            let (path, anchor) = match target.split_once('#') {
                Some((p, a)) => (p, Some(a)),
                None => (target.as_str(), None),
            };
            // The README links GitHub's own pages of the repository (../../releases/..., ../../issues), relative to
            // blob/main/README.md, so that the tree names no owner. They are not files of the tree.
            if rel == "README.md" && (path.starts_with("../../releases") || path.starts_with("../../issues")) {
                continue;
            }
            let dest = if path.is_empty() {
                file.clone()
            } else {
                file.parent().unwrap().join(path.replace("%20", " "))
            };
            if !dest.exists() {
                broken.push(format!("{rel}:{line}: {target} (no such file)"));
                continue;
            }
            if let Some(a) = anchor.filter(|_| dest.extension().is_some_and(|x| x == "md")) {
                let there = std::fs::read_to_string(&dest).unwrap();
                if !anchors(&there).contains(a) {
                    broken.push(format!("{rel}:{line}: {target} (no such heading)"));
                }
            }
        }
    }
    assert!(broken.is_empty(), "{} broken links:\n{}", broken.len(), broken.join("\n"));
}

#[test]
fn the_slugs_are_githubs() {
    assert_eq!(slug("The pause menu (HOME)"), "the-pause-menu-home");
    assert_eq!(slug("2. Start pocket-ui"), "2-start-pocket-ui");
    assert_eq!(slug("Tailscale (Settings → Tailscale)"), "tailscale-settings--tailscale");
    assert_eq!(
        slug("Options for PlayStation 2, GameCube, Wii and 3DS"),
        "options-for-playstation-2-gamecube-wii-and-3ds"
    );
    assert_eq!(slug("RetroAchievements (`src/achievements/`)"), "retroachievements-srcachievements");
    assert_eq!(slug("[Pixel Path](x.md) as built"), "pixel-path-as-built");
    let a = anchors("# A\n## Same\n```\n## Not a heading\n```\n## Same\n<!-- ## Gone -->\n");
    assert_eq!(a.into_iter().collect::<Vec<_>>(), ["a", "same", "same-1"]);
    assert!(names("`src/app_motion.rs`, `motion.rs`", "motion.rs"));
    assert!(!names("`src/app_motion.rs`", "motion.rs"));
}

/// The README presents the product, not a release (18.11.2026): both editions stay on it for good, what is new lives in
/// the release notes and CHANGELOG.md. A "### New in X" section went stale twice: 0.5.0 still said "New in 0.4", and
/// 0.5.1's replaced 0.5's, which held the only Android line, so the Android app vanished from GitHub.
#[test]
fn the_readme_shows_both_editions_and_points_at_the_changelog() {
    let md = without_comments(&read("README.md"));
    let mut missing = Vec::new();
    for want in [
        "pocket-ui OS",
        "pocket-ui for Android",
        "https://pocketui.xyz/install\"",
        "https://pocketui.xyz/install#android",
        "(CHANGELOG.md)",
        "../../releases",
    ] {
        if !md.contains(want) {
            missing.push(want);
        }
    }
    assert!(missing.is_empty(), "README.md must name both editions with their download links and link the changelog; missing: {missing:?}");
    let stale: Vec<_> = headings(&md).into_iter().filter(|(_, h)| h.starts_with("New in")).collect();
    assert!(
        stale.is_empty(),
        "README.md has no \"New in\" section (what is new goes to CHANGELOG.md and the release notes): {stale:?}"
    );
    assert!(!md.to_lowercase().contains("home screen"), "README.md: pocket-ui is a frontend, never a \"home screen\"");
}

// ---- the status ledger ---------------------------------------------------------------------------------------------

/// The five statuses of docs/STATUS.md, strongest first.
const LEDGER_STATUSES: [&str; 5] =
    ["device-measured", "device smoke-checked", "virtual device only", "host tests only", "not built"];

#[test]
fn the_status_ledger_rows_have_a_known_status_and_no_private_data() {
    let md = read("docs/STATUS.md");
    let mut rows = 0;
    for (n, line) in md.lines().enumerate() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
        // A feature row: Feature | Status | Evidence | Open. (The legend has two columns, the header says "Status".)
        if !line.trim_start().starts_with('|') || cells.len() != 4 || cells[1] == "Status" || cells[1].starts_with("---") {
            continue;
        }
        rows += 1;
        assert!(
            LEDGER_STATUSES.contains(&cells[1]),
            "docs/STATUS.md:{}: the status of \"{}\" is {:?}, one of {LEDGER_STATUSES:?}",
            n + 1,
            cells[0],
            cells[1]
        );
        assert!(!cells[2].is_empty() && !cells[3].is_empty(), "docs/STATUS.md:{}: Evidence and Open are filled in", n + 1);
    }
    assert!(rows >= 40, "docs/STATUS.md: the ledger has {rows} feature rows");
    // No IP addresses or MAC addresses in a public document.
    for (n, line) in md.lines().enumerate() {
        for w in line.split_whitespace() {
            let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            let ip = w.split('.').count() == 4 && w.split('.').all(|p| p.parse::<u8>().is_ok());
            let mac = w.split(':').count() == 6 && w.split(':').all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(!ip && !mac, "docs/STATUS.md:{}: an IP or MAC address ({w})", n + 1);
        }
    }
    for word in [".ts.net", "@proton", "@gmail", "serial"] {
        assert!(!md.contains(word), "docs/STATUS.md must not contain {word:?} (private data)");
    }
}

// ---- the style of a path in Settings ---------------------------------------------------------------------------

/// A path in Settings on the screen is "Settings › Playing › Battery" (docs/i18n-glossary.md); the first-day run
/// (2026-10-26) still found "Settings > …" in places. Every line of `src/` with a string in it (comments left out)
/// and both sides of `de.tsv` are checked.
#[test]
#[ignore = "release check: sh scripts/release-check.sh"]
fn settings_paths_on_the_screen_use_the_glossarys_arrow() {
    // Put together, so this test's own lines don't count.
    let (en, de, es, pt, fr, it) = (["Settings", " > "].concat(), ["Einstellungen", " > "].concat(), ["Ajustes", " > "].concat(), ["Configurações", " > "].concat(), ["Paramètres", " > "].concat(), ["Impostazioni", " > "].concat());
    let mut bad = Vec::new();
    for f in rust_sources() {
        let src = std::fs::read_to_string(&f).unwrap();
        for (n, line) in src.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || !code.contains('"') {
                continue;
            }
            if line.contains(&en) {
                bad.push(format!("{}:{}: {}", f.strip_prefix(root()).unwrap_or(&f).display(), n + 1, code));
            }
        }
    }
    for file in ["de.tsv", "es.tsv", "pt.tsv", "fr.tsv", "it.tsv"] {
        for (n, line) in read(&format!("assets/i18n/{file}")).lines().enumerate() {
            if !line.starts_with('#') && (line.contains(&en) || line.contains(&de) || line.contains(&es) || line.contains(&pt) || line.contains(&fr) || line.contains(&it)) {
                bad.push(format!("assets/i18n/{file}:{}: {line}", n + 1));
            }
        }
    }
    assert!(bad.is_empty(), "write a Settings path with \"›\":\n{}", bad.join("\n"));
}

// ---- the Android app's own Settings ---------------------------------------------------------------------------------

/// The rows of the Android app's Settings list: the labels `menu_rows` in android/src/ui.rs gives each `Setting`
/// (`Setting::PlaysWith => row("Plays with", "")`). Android has one flat list, no categories ("Settings › Plays with"), and a row
/// can lead to a page of its own ("Settings › Cozy extras › Download now": the part after the row is not checked, as on Linux).
fn android_settings_rows() -> Vec<String> {
    let ui = read("android/src/ui.rs");
    let rows: Vec<String> = ui
        .lines()
        .filter_map(|l| {
            let rest = &l[l.find("=> row(\"")? + 8..];
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect();
    assert!(rows.len() >= 20 && rows.iter().any(|r| r == "Plays with"), "android/src/ui.rs no longer lists the Settings rows as `=> row(\"…\"`: {rows:?}");
    rows
}

/// Everything the Android app's sources spell out, with the string escapes undone: a text of `assets/i18n/*.tsv` whose English
/// key is in it is an Android text and is checked against [`android_settings_rows`] instead of the Linux categories.
fn android_source_text() -> String {
    let mut all = String::new();
    for e in std::fs::read_dir(root().join("android/src")).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "rs") {
            all.push_str(&std::fs::read_to_string(e.path()).unwrap());
        }
    }
    // A line built with concat!(…, pocket_ui::contact_address!(), …) reads as one text on the device (a430e78ab): join it so
    // the path check finds it as Android's and not as a Linux line.
    let joined = all.replace("\", pocket_ui::contact_address!(), \"", pocket_ui::CONTACT);
    joined.replace("\\\"", "\"").replace("\\u{203a}", "\u{203a}")
}

/// The problems in the Settings paths of one Android line: "Settings › R" (or with an arrow) where R must be a row of Android's
/// Settings. `rows` are the names in the line's language, `word` is "Settings" or its translation.
fn android_path_problems(line: &str, word: &str, rows: &[String]) -> Vec<String> {
    let line = line.replace('\u{2192}', "\u{203a}");
    let start = format!("{word} \u{203a} ");
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = line[at..].find(&start) {
        let rest = &line[at + i + start.len()..];
        let ok = rows.iter().any(|r| rest.starts_with(r.as_str()) && !rest[r.len()..].chars().next().is_some_and(char::is_alphanumeric));
        if !ok {
            out.push(format!("{word} \u{203a} {}: not a row of the Android app's Settings (android/src/ui.rs)", rest.chars().take(30).collect::<String>()));
        }
        at += i + start.len();
    }
    out
}

/// The check of the Android texts catches a path to a row the Android app does not have, accepts a page below a row, and reads the
/// rows from the app's own list (so a renamed row turns the Help text red).
#[test]
fn an_android_settings_path_names_a_row_of_the_android_list() {
    let rows = android_settings_rows();
    assert!(android_path_problems("Settings \u{203a} Plays with: for each system", "Settings", &rows).is_empty());
    assert!(android_path_problems("PSP is optional. Settings \u{2192} Cozy extras \u{203a} Download now loads it.", "Settings", &rows).is_empty());
    assert!(android_path_problems("Ajustes \u{203a} Copia de partidas.", "Ajustes", &["Copia de partidas".to_string()]).is_empty());
    let bad = android_path_problems("Settings \u{203a} Playing \u{203a} Battery, and Settings \u{203a} Save backup", "Settings", &rows);
    assert_eq!(bad.len(), 1, "Playing is a Linux category, not a row of Android's list: {bad:?}");
    assert!(android_path_problems("Settings \u{203a} Plays withx", "Settings", &rows).len() == 1, "a row name is matched as whole words");
}

// ---- every path in Settings names a real category and row ------------------------------------------------------

/// The problems in the Settings paths of one line: "Settings › A › B" (also written with an arrow, with `\u{203a}`
/// in a Rust string, or with Markdown bold and links around the names). A must be a category, B a row of A; deeper
/// parts (the rows of a page) are not checked. `next` is the following line, for a path that breaks after its arrow.
fn settings_path_problems(line: &str, next: &str, word: &str, cats: &[(String, Vec<String>)]) -> Vec<String> {
    fn norm(s: &str) -> String {
        s.replace("\\u{203a}", "\u{203a}").replace('\u{2192}', "\u{203a}")
    }
    fn plain(s: &str) -> &str {
        s.trim_start_matches(|c: char| c.is_whitespace() || "*[`_".contains(c))
    }
    /// The longest of `names` that `s` starts with (as whole words), and what follows it.
    fn take<'a>(s: &'a str, names: impl Iterator<Item = &'a String>) -> Option<(&'a String, &'a str)> {
        let mut best: Option<(&String, &str)> = None;
        for n in names {
            if let Some(rest) = s.strip_prefix(n.as_str()) {
                let whole = rest.chars().next().is_none_or(|c| !c.is_alphanumeric());
                if whole && best.is_none_or(|(b, _)| n.len() > b.len()) {
                    best = Some((n, rest));
                }
            }
        }
        best
    }
    /// Past the Markdown that closes a name: `**`, `]`, a link target `(...)`, a backtick.
    fn past_markup(mut s: &str) -> &str {
        loop {
            let t = s.trim_start_matches(|c: char| "*]`_".contains(c));
            if let Some(r) = t.strip_prefix('(') {
                if let Some(i) = r.find(')') {
                    s = &r[i + 1..];
                    continue;
                }
            }
            if t.len() == s.len() {
                return t;
            }
            s = t;
        }
    }
    let (line, next) = (norm(line), norm(next));
    let mut out = Vec::new();
    let head = format!("{word} \u{203a}");
    let mut from = 0;
    while let Some(i) = line[from..].find(&head) {
        let at = from + i;
        from = at + head.len();
        if line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let mut rest = line[from..].trim_start().to_string();
        if rest.is_empty() {
            rest = next.trim_start().to_string();
        }
        let rest = plain(&rest);
        let shown: String = rest.chars().take(28).collect();
        let Some((cat, after)) = take(rest, cats.iter().map(|c| &c.0)) else {
            out.push(format!("{word} \u{203a} {shown}: not a category"));
            continue;
        };
        let after = past_markup(after).trim_start();
        let after = if after.is_empty() { next.trim_start() } else { after };
        let Some(row) = after.strip_prefix('\u{203a}') else {
            // "the Tour row in Settings › System": the row named before must be one of that category's.
            let before = line[..at].trim_end().trim_end_matches(" in").trim_end();
            if let Some(named) = before.strip_suffix(" row") {
                let named = named.trim_end_matches(|c: char| "*`\"\u{201d}_]".contains(c));
                let all = cats.iter().flat_map(|c| &c.1);
                if all.clone().any(|r| named.ends_with(r.as_str())) && !cats.iter().find(|c| &c.0 == cat).unwrap().1.iter().any(|r| named.ends_with(r.as_str())) {
                    out.push(format!("\"{}\" row in {word} \u{203a} {cat}: not a row of {cat}", named.rsplit(' ').next().unwrap_or(named)));
                }
            }
            continue;
        };
        let row = if row.trim().is_empty() { plain(&next) } else { plain(row) };
        let rows = &cats.iter().find(|c| &c.0 == cat).unwrap().1;
        if take(row, rows.iter()).is_none() {
            let shown: String = row.chars().take(28).collect();
            out.push(format!("{word} \u{203a} {cat} \u{203a} {shown}: not a row of {cat}"));
        }
    }
    out
}

#[test]
fn a_row_named_before_its_category_must_be_in_it() {
    let en = settings_names(None);
    let bad = settings_path_problems("the Tour row in Settings \u{2192} System then says", "", "Settings", &en);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(settings_path_problems("the Tour row in Settings \u{2192} Help & tools then says", "", "Settings", &en).is_empty());
}

/// The names of the categories and their rows, in English or through a language's table.
fn settings_names(table: Option<&std::collections::HashMap<String, String>>) -> Vec<(String, Vec<String>)> {
    let tr = |s: &str| -> String {
        let key = table.and_then(|m| m.get(s)).map(|g| g.as_str()).unwrap_or(s);
        key.trim_end_matches('\u{200b}').to_string()
    };
    super::SETTINGS_GROUPS
        .iter()
        .map(|(c, rows)| {
            let mut names: Vec<String> = rows.iter().map(|r| tr(r.0)).collect();
            // The Brick calls the lights row as its page (`app_settings_groups::row_name`): a path may name it either way.
            if rows.iter().any(|r| matches!(r.1, super::Opens::Leds)) {
                names.push(tr("Lights & buttons"));
            }
            // And the performance row "Performance" (no fan on the Brick, `perf_fan_page::title`).
            if rows.iter().any(|r| matches!(r.1, super::Opens::PerfFan)) {
                names.push(tr("Performance"));
            }
            (tr(c), names)
        })
        .collect()
}

/// A path in Settings in the user documents (docs/user, README.md), on the web page, in a string of `src/` and on
/// both sides of each table has to name a category of main.rs SETTINGS_GROUPS, and a row of that category.
/// Left out: dated and developer documents, comments in `src/`, this file (it spells bad paths on purpose).
#[test]
#[ignore = "release check: sh scripts/release-check.sh"]
fn every_path_in_settings_names_a_category_and_one_of_its_rows() {
    let en = settings_names(None);
    let mut bad = Vec::new();
    let mut scan = |name: String, text: &str, word: &str, cats: &[(String, Vec<String>)], skip_comments: bool| {
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if skip_comments && (t.starts_with("//") || !t.contains('"')) || t.starts_with('#') && name.ends_with(".tsv") {
                continue;
            }
            for p in settings_path_problems(line, lines.get(n + 1).copied().unwrap_or(""), word, cats) {
                bad.push(format!("{name}:{}: {p}", n + 1));
            }
        }
    };
    let rel = |p: &Path| p.strip_prefix(root()).unwrap_or(p).display().to_string();
    // The Android app's page describes that app's own Settings ("Settings → First start"), not these categories.
    let mut docs: Vec<PathBuf> = markdown_files()
        .into_iter()
        .filter(|p| p.starts_with(root().join("docs/user")) && !p.ends_with("android-mvp.md"))
        .collect();
    docs.push(root().join("README.md"));
    for f in docs {
        scan(rel(&f), &std::fs::read_to_string(&f).unwrap(), "Settings", &en, false);
    }
    for e in std::fs::read_dir(root().join("src/web/page")).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "js" || x == "html") {
            scan(rel(&p), &std::fs::read_to_string(&p).unwrap(), "Settings", &en, false);
        }
    }
    for f in rust_sources() {
        if !f.ends_with("doc_checks.rs") {
            scan(rel(&f), &std::fs::read_to_string(&f).unwrap(), "Settings", &en, true);
        }
    }
    // Both sides of each table: the English key as the code paints it, the translation with the translated names.
    for (file, word) in [("de.tsv", "Einstellungen"), ("es.tsv", "Ajustes"), ("pt.tsv", "Configurações"), ("fr.tsv", "Paramètres"), ("it.tsv", "Impostazioni")] {
        let tsv = read(&format!("assets/i18n/{file}"));
        let map: std::collections::HashMap<String, String> = tsv
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('\t'))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let names = settings_names(Some(&map));
        let (android_rows, android_text) = (android_settings_rows(), android_source_text());
        let android_names: Vec<String> = android_rows.iter().map(|r| map.get(r).cloned().unwrap_or_else(|| r.clone())).collect();
        for (n, line) in tsv.lines().enumerate().filter(|(_, l)| !l.starts_with('#')) {
            let (k, v) = line.split_once('\t').unwrap_or((line, ""));
            // A text of the Android app (its Help page, its download notes): Android has a Settings list of its own.
            if android_text.contains(k) {
                for p in android_path_problems(k, "Settings", &android_rows) {
                    bad.push(format!("assets/i18n/{file}:{} (English, Android): {p}", n + 1));
                }
                for p in android_path_problems(v, word, &android_names) {
                    bad.push(format!("assets/i18n/{file}:{} (translated, Android): {p}", n + 1));
                }
                continue;
            }
            for p in settings_path_problems(k, "", "Settings", &en) {
                bad.push(format!("assets/i18n/{file}:{} (English): {p}", n + 1));
            }
            for p in settings_path_problems(v, "", word, &names) {
                bad.push(format!("assets/i18n/{file}:{} (translated): {p}", n + 1));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "a path \"Settings › A › B\" needs A = a category and B = a row of A (main.rs SETTINGS_GROUPS):\n{}",
        bad.join("\n")
    );
}

// ---- code shape -----------------------------------------------------------------------------------------------------

/// The `src/` files that are test code as a whole: declared under `#[cfg(test)]` as `mod name;`, with or without
/// `#[path = "..."]`.
fn test_only_files(sources: &[PathBuf]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for f in sources {
        let text = std::fs::read_to_string(f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy();
        // `mod x;` in main.rs, lib.rs or mod.rs sits next to the file; anywhere else in a folder named after it.
        let dir = if matches!(&*name, "main.rs" | "lib.rs" | "mod.rs") || f.parent().unwrap().ends_with("src/bin") {
            f.parent().unwrap().to_path_buf()
        } else {
            f.with_extension("")
        };
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (i, l) in lines.iter().enumerate() {
            if *l != "#[cfg(test)]" {
                continue;
            }
            let mut path = None;
            for next in &lines[i + 1..] {
                if let Some(p) = next.strip_prefix("#[path = \"").and_then(|p| p.strip_suffix("\"]")) {
                    path = Some(f.parent().unwrap().join(p));
                } else if next.starts_with("#[") {
                } else {
                    let decl = next.strip_prefix("pub(crate) ").or_else(|| next.strip_prefix("pub ")).unwrap_or(next);
                    if let Some(m) = decl.strip_prefix("mod ").and_then(|m| m.strip_suffix(';')) {
                        out.insert(path.take().unwrap_or_else(|| {
                            let flat = dir.join(format!("{m}.rs"));
                            if flat.exists() {
                                flat
                            } else {
                                dir.join(m).join("mod.rs")
                            }
                        }));
                    }
                    break;
                }
            }
        }
    }
    out
}

/// `text` without its `#[cfg(test)]` items and without `//` comment lines. An item ends at the first line at its own
/// indentation that ends with `}`, `;` or `,` (a field), or before a line indented less (the end of its parent).
fn without_tests(text: &str) -> String {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = String::with_capacity(text.len());
    let mut skip: Option<usize> = None;
    for l in text.lines() {
        let t = l.trim();
        if let Some(i) = skip {
            if t.is_empty() {
                continue;
            }
            if indent(l) < i {
                skip = None;
            } else {
                if indent(l) == i && !t.starts_with("#[") && (t.ends_with('}') || t.ends_with(';') || t.ends_with(',')) {
                    skip = None;
                }
                continue;
            }
        }
        if t == "#[cfg(test)]" {
            skip = Some(indent(l));
            continue;
        }
        if !t.starts_with("//") {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// `Result<…, String>`: a `Result` whose error type (after the last top-level comma) is `String`.
fn string_results(text: &str) -> usize {
    text.match_indices("Result<")
        .filter(|(at, _)| !text[..*at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_'))
        .filter(|(at, _)| {
            let inner = &text[at + "Result<".len()..];
            let (mut depth, mut comma) = (0usize, None);
            for (i, c) in inner.char_indices() {
                match c {
                    '<' | '(' | '[' => depth += 1,
                    ')' | ']' => depth = depth.saturating_sub(1),
                    '>' if depth == 0 => return comma.is_some_and(|c: usize| inner[c + 1..i].trim() == "String"),
                    '>' => depth -= 1,
                    ',' if depth == 0 => comma = Some(i),
                    ';' | '{' => return false,
                    _ => {}
                }
            }
            false
        })
        .count()
}

/// The code-shape counts of the product code in `src/` (tests left out): the capped ones in the order of
/// `tests/code-shape.txt`, and `pub fn`, which is only shown (a new module may bring up to 5 `pub` items, rule 9).
fn code_shape_counts() -> (Vec<(&'static str, usize)>, usize) {
    let sources = rust_sources();
    let tests = test_only_files(&sources);
    let code: String = sources
        .iter()
        .filter(|f| !tests.contains(*f))
        .map(|f| without_tests(&std::fs::read_to_string(f).unwrap()))
        .collect();
    let count = |s: &str| code.matches(s).count();
    let capped = vec![
        ("== \"pocket-core\"", count("== \"pocket-core\"")),
        ("== \"ports\"", count("== \"ports\"")),
        ("let _ = fs::", count("let _ = fs::") + count("let _ = std::fs::")),
        ("Result<_, String>", string_results(&code)),
    ];
    (capped, count("pub fn "))
}

/// A canvas's pixels by hand (docs/dev/native-paint.md "Interfaces for packages 2 to 4"): `.buf[` only in the files `tests/code-shape.txt`
/// lists as `only .buf[`, the design guard (`.design()`, `.design_area(`) only in those it lists as `only guard`. Painted
/// natively, code that indexes the pixels by design coordinates paints the wrong place or slices past the buffer (a crash
/// with `panic = "abort"`); a guard paints right but costs two passes. The lists only shrink: a file that no longer needs
/// its line is printed, a file that isn't listed fails.
#[test]
fn canvas_pixels_by_hand_stay_in_the_listed_files() {
    let list = read("tests/code-shape.txt");
    let only = |what: &str| -> BTreeSet<String> {
        list.lines().filter_map(|l| l.strip_prefix("only ")).filter_map(|l| l.split_once('\t')).filter(|(k, _)| *k == what).map(|(_, f)| f.trim().to_string()).collect()
    };
    let (bufs, guards) = (only(".buf["), only("guard"));
    assert!(bufs.contains("src/gfx.rs"), "tests/code-shape.txt: `only .buf[\tsrc/gfx.rs` is the canvas itself");
    let sources = rust_sources();
    let tests = test_only_files(&sources);
    let (mut wrong, mut stale_buf, mut stale_guard) = (Vec::new(), bufs.clone(), guards.clone());
    for f in sources.iter().filter(|f| !tests.contains(*f)) {
        let path = f.to_string_lossy();
        let name = path.find("src/").map_or(path.to_string(), |i| path[i..].to_string());
        let code = without_tests(&std::fs::read_to_string(f).unwrap());
        if code.contains(".buf[") {
            stale_buf.remove(&name);
            if !bufs.contains(&name) {
                wrong.push(format!("{name}: `.buf[` (paint through the Canvas calls, or `pixels_mut` with `stride`)"));
            }
        }
        // A soft edge built from 1 px shaded rows: painted natively two rows land on one screen row and mix it twice
        // (package 5 found the scrolled pages' edges twice as dark). `shade_rows` paints such an edge as one.
        if name != "src/gfx.rs" {
            for call in [".shade_rect(", ".darken_rect("] {
                for (at, _) in code.match_indices(call) {
                    let args = call_args(&code[at + call.len()..]);
                    if args.get(3).map(|h| h.trim()) == Some("1") {
                        let line = code[..at].lines().count();
                        wrong.push(format!("{name}:{line}: `{}…, 1, …)`: a 1 px shaded row; paint the rows as one with `shade_rows`", call.trim_start_matches('.')));
                    }
                }
            }
        }
        // The canvas itself makes the guard.
        if name != "src/gfx.rs" && (code.contains(".design()") || code.contains(".design_area(")) {
            stale_guard.remove(&name);
            if !guards.contains(&name) {
                wrong.push(format!("{name}: a design guard (`design()`/`design_area()`): paint it natively instead"));
            }
        }
    }
    assert!(wrong.is_empty(), "canvas pixels by hand outside tests/code-shape.txt's lists (docs/dev/native-paint.md):\n{}", wrong.join("\n"));
    for f in stale_buf.iter().map(|f| format!("only .buf[\t{f}")).chain(stale_guard.iter().map(|f| format!("only guard\t{f}"))) {
        eprintln!("code shape: no longer needed, remove from tests/code-shape.txt: {f}");
    }
}

/// The arguments of a call, `rest` starting just after its `(`: split at the commas outside brackets, up to its `)`.
fn call_args(rest: &str) -> Vec<&str> {
    let (mut depth, mut start, mut args) = (0i32, 0, Vec::new());
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                args.push(&rest[start..i]);
                return args;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&rest[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    args
}

#[test]
fn a_call_splits_into_its_arguments_at_the_outer_commas() {
    assert_eq!(call_args("x, y + k, f(a, b), 1, pal().void, 3) more"), ["x", " y + k", " f(a, b)", " 1", " pal().void", " 3"]);
}

/// Four counts of "Code shape" (DEVELOPING.md) may only go down: each is capped by its line in
/// `tests/code-shape.txt`. Whoever lowers one lowers its cap in the same commit. `pub fn` is printed, not capped.
#[test]
#[ignore = "release check: bookkeeping (scripts/bookkeeping.sh)"]
fn the_code_shape_counts_never_go_up() {
    let caps = read("tests/code-shape.txt");
    let caps: Vec<(&str, usize)> = caps
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("only ") && !l.trim().is_empty())
        .map(|l| {
            let (k, v) = l.rsplit_once('\t').unwrap_or_else(|| panic!("tests/code-shape.txt: no tab in {l:?}"));
            (k, v.trim().parse().unwrap_or_else(|_| panic!("tests/code-shape.txt: no number in {l:?}")))
        })
        .collect();
    let (counts, pub_fns) = code_shape_counts();
    eprintln!("code shape: pub fn {pub_fns} in src/ (product code; shown, not capped)");
    assert_eq!(
        caps.iter().map(|c| c.0).collect::<Vec<_>>(),
        counts.iter().map(|c| c.0).collect::<Vec<_>>(),
        "tests/code-shape.txt has to list these counts, in this order"
    );
    let up: Vec<String> = caps
        .iter()
        .zip(&counts)
        .filter(|((_, cap), (_, n))| n > cap)
        .map(|((k, cap), (_, n))| format!("{k}: {n} in src/ (product code), the cap is {cap}"))
        .collect();
    assert!(
        up.is_empty(),
        "a code-shape count went up (docs/DEVELOPING.md, \"Code shape\"); use a type, return the error or keep the item \
         private instead:\n{}",
        up.join("\n")
    );
    let down: Vec<String> =
        caps.iter().zip(&counts).filter(|((_, cap), (_, n))| n < cap).map(|((k, _), (_, n))| format!("{k}\t{n}")).collect();
    if !down.is_empty() {
        eprintln!("code shape: lower these caps in tests/code-shape.txt:\n{}", down.join("\n"));
    }
}

#[test]
fn code_shape_leaves_test_items_and_comments_out() {
    let text = "fn a() {\n    let _ = fs::remove_file(p);\n}\n// let _ = fs::write(p, b);\n#[cfg(test)]\nmod tests {\n    \
                fn b() {\n        let _ = fs::write(p, b);\n    }\n}\nstruct S {\n    #[cfg(test)]\n    x: u8,\n    \
                y: u8,\n}\n#[cfg(test)]\nuse foo;\nfn c() {}\n";
    let kept = without_tests(text);
    assert_eq!(kept.matches("let _ = fs::").count(), 1, "{kept}");
    assert!(kept.contains("y: u8") && kept.contains("fn c()") && !kept.contains("x: u8") && !kept.contains("use foo"));
    assert_eq!(
        string_results("fn a() -> Result<(), String> {}\nfn b() -> io::Result<Vec<u8>> {}\nfn c() -> Result<HashMap<String, u8>, String>;\nfn d() -> Result<String, Error>;"),
        2
    );
}

// ---- the looks' names in every language -----------------------------------------------------------------------------

/// Every look has one name per language, in its row and in every text that names it (the hand test 16.11.2026: the
/// German row said "Vitrine", the descriptions "Schrank", and "Easy" stood beside "Einfach"). A text that names a look
/// (its English with the look's capitalised name, or two looks' names, or "the cabinet" of the Cabinet look) has the
/// look's own translation in it; and German has no "Vitrine" and no "Easy" left.
#[test]
fn i18n_look_names() {
    // The looks' names in the UI (`look_settings::Theme::name`, `features::Profile::name`), Minimal and Cards still in old texts.
    const LOOKS: [&str; 6] = ["Shelf", "Easy", "Split", "Cabinet", "Minimal", "Cards"];
    let word = |text: &str, w: &str| {
        text.match_indices(w).any(|(i, _)| {
            let before = text[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
            let after = text[i + w.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
            before && after
        })
    };
    let mut bad = Vec::new();
    for file in ["de.tsv", "es.tsv", "pt.tsv", "fr.tsv", "it.tsv"] {
        let tsv = read(&format!("assets/i18n/{file}"));
        let rows: Vec<(usize, &str, &str)> = tsv
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.starts_with('#'))
            .filter_map(|(n, l)| l.split_once('\t').map(|(en, tr)| (n + 1, en, tr)))
            .collect();
        let name = |look: &str| rows.iter().find(|(_, en, _)| *en == look).map(|(_, _, tr)| tr.to_string());
        for (n, en, tr) in &rows {
            // A game's difficulty, not the look.
            let en = en.replace("Easy mode", "");
            let named: Vec<&str> = LOOKS.iter().copied().filter(|l| word(&en, l)).collect();
            let mut wants: Vec<&str> = if named.len() >= 2 { named.clone() } else { named.iter().copied().filter(|l| ["Easy", "Cabinet"].contains(l)).collect() };
            // "the cabinet" is the look's (its TV, its top); "the cabinet of the CRT frame" is a TV's own.
            if en.to_lowercase().contains("the cabinet") && !en.contains("CRT frame") && !wants.contains(&"Cabinet") {
                wants.push("Cabinet");
            }
            for look in wants {
                let Some(own) = name(look) else { continue };
                if *look == *en.as_str() {
                    continue;
                }
                if !tr.to_lowercase().contains(&own.to_lowercase()) {
                    bad.push(format!("assets/i18n/{file}:{n}: names {look} but not \"{own}\": {tr}"));
                }
            }
        }
        if file == "de.tsv" {
            for (n, _, tr) in &rows {
                if word(tr, "Vitrine") || word(tr, "Easy") {
                    bad.push(format!("assets/i18n/{file}:{n}: German says Schrank and Einfach: {tr}"));
                }
            }
        }
    }
    // The tests' German expectations too (16.11.: two first-start tests still expected "Easy" on the German pages): a line
    // that names German ("de") expects no old name as a string.
    let quoted = |w: &str| format!("\"{w}\"");
    let mut tests: Vec<PathBuf> = std::fs::read_dir(root().join("tests")).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
    tests.sort();
    for f in tests.into_iter().filter(|p| p.extension().is_some_and(|x| x == "rs")) {
        let src = std::fs::read_to_string(&f).unwrap();
        for (n, line) in src.lines().enumerate() {
            // From each German tuple to the next one: `("en", [.., "Easy", ..]), ("de", [..])` is fine.
            let german = line.match_indices("(\"de\"").map(|(i, _)| {
                let rest = &line[i + 1..];
                &rest[..rest.find("(\"").unwrap_or(rest.len())]
            });
            if german.into_iter().any(|g| g.contains(&quoted("Easy")) || g.contains(&quoted("Vitrine"))) {
                bad.push(format!("{}:{}: a German expectation with an old look name: {}", f.strip_prefix(root()).unwrap_or(&f).display(), n + 1, line.trim()));
            }
        }
    }
    for f in ["fastlane/metadata/android/de-DE/full_description.txt"] {
        if word(&read(f), "Vitrine") {
            bad.push(format!("{f}: the Cabinet look is \"Schrank\" in German"));
        }
    }
    assert!(bad.is_empty(), "one name per look and language:\n{}", bad.join("\n"));
}

// ---- the release checks and the nightly run -------------------------------------------------------------------------

/// Every `fn` of `text` with the reason of the `#[ignore = "…"]` above it (None: not ignored, or ignored without a reason).
fn fns_with_ignore_reason(text: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut reason: Option<String> = None;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(r) = t.strip_prefix("#[ignore = \"") {
            reason = r.rfind('"').map(|end| r[..end].to_string());
        } else if let Some(rest) = ["pub(crate) ", "pub ", "async "].iter().fold(t, |t, p| t.strip_prefix(p).unwrap_or(t)).strip_prefix("fn ") {
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                out.push((name, reason.take()));
            }
        }
    }
    out
}

/// The value of `NAME="…"` on a line of a shell script.
fn shell_value(script: &str, name: &str) -> Option<String> {
    script.lines().find_map(|l| l.strip_prefix(&format!("{name}=\"")).and_then(|r| r.split('"').next()).map(str::to_string))
}

/// The release-only tests (`#[ignore = "release check: …"]`) run in three places that must agree: scripts/pre-release.sh at the
/// cut, scripts/release-check.sh, and scripts/release-extras.sh in the nightly full suite (tools/nightly-gate.sh). 0.5.1 (18.11.):
/// two wrong Settings paths sat on main for a day because no night ran these tests; and two suites (where_states,
/// no_wifi_walk) were run by nobody at all. So: (1) a name in scripts/release-text-tests.txt is a real test (a dead
/// name matches nothing in libtest and stays green); (2) each release-only test of `src/` is in that list, or says it is a
/// bookkeeping check or needs a fixture; (3) each `tests/` file with one is in the text-fit suites or is the nav explorer;
/// (4) pre-release.sh and release-extras.sh name the same suites and the same commands; (5) the nightly runs the extras.
#[test]
fn every_release_only_test_runs_in_pre_release_and_in_the_nightly() {
    let mut bad = Vec::new();
    let listed: Vec<String> = read("scripts/release-text-tests.txt").lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
    let is_release = |r: &Option<String>| r.as_deref().is_some_and(|r| r.starts_with("release check:"));
    // The tests of the list's own binaries: the library and the binaries (`src/`) and tests/i18n_snapshots.rs.
    let mut known: Vec<(String, Option<String>, String)> = Vec::new();
    for f in rust_sources() {
        let rel = f.strip_prefix(root()).unwrap_or(&f).display().to_string();
        for (n, r) in fns_with_ignore_reason(&std::fs::read_to_string(&f).unwrap()) {
            known.push((n, r, rel.clone()));
        }
    }
    for (n, r) in fns_with_ignore_reason(&read("tests/i18n_snapshots.rs")) {
        known.push((n, r, "tests/i18n_snapshots.rs".into()));
    }
    // (1) no dead name (an ordinary test in the list is fine: it just runs with the others).
    for name in &listed {
        if !known.iter().any(|k| &k.0 == name) {
            bad.push(format!("scripts/release-text-tests.txt: {name} is no test any more (a dead name matches nothing and passes)"));
        }
    }
    // (2) the release-only tests of src/ (tests/i18n_snapshots.rs's are run by its suite below).
    for (name, reason, file) in known.iter().filter(|k| k.2.starts_with("src/")) {
        let r = reason.as_deref().unwrap_or("");
        let elsewhere = r.contains("bookkeeping") || r.contains("needs the real database");
        if is_release(reason) && !elsewhere && !listed.contains(name) {
            bad.push(format!("{file}: {name} is release-only but not in scripts/release-text-tests.txt (no night and no cut runs it)"));
        }
    }
    // (3) each tests/ file with release-only cases is a suite pre-release.sh runs with --ignored, or the explorer.
    let pre = std::fs::read_to_string(root().join("scripts/pre-release.sh")).ok();
    let extras = std::fs::read_to_string(root().join("scripts/release-extras.sh")).ok();
    if let (Some(pre), Some(extras)) = (&pre, &extras) {
        let suites_of = |s: &str| shell_value(s, "TEXT_SUITES").unwrap_or_default();
        if suites_of(pre) != suites_of(extras) || suites_of(pre).is_empty() {
            bad.push(format!("scripts/pre-release.sh and scripts/release-extras.sh name different TEXT_SUITES:\n  {}\n  {}", suites_of(pre), suites_of(extras)));
        }
        let suites: Vec<String> = suites_of(extras).split_whitespace().map(str::to_string).collect();
        for s in &suites {
            if !root().join(format!("tests/{s}.rs")).exists() {
                bad.push(format!("TEXT_SUITES names tests/{s}.rs, which does not exist"));
            }
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("tests")).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for f in files.into_iter().filter(|p| p.extension().is_some_and(|x| x == "rs")) {
            let stem = f.file_stem().unwrap().to_string_lossy().to_string();
            for (name, reason) in fns_with_ignore_reason(&std::fs::read_to_string(&f).unwrap()) {
                let r = reason.clone().unwrap_or_default();
                // Fixtures nobody has on a build host, and numbers that are printed, not asserted.
                let by_hand = r.contains("needs POCKETUI_BIG_FIXTURE") || r.contains("prints the numbers");
                if is_release(&reason) && !by_hand && stem != "nav_explorer" && !suites.contains(&stem) {
                    bad.push(format!("tests/{stem}.rs: {name} is release-only, but {stem} is not in TEXT_SUITES (no night and no cut runs it)"));
                }
            }
        }
        // (4) the same commands in both, and the Brick filter finds a test.
        for needle in ["scripts/release-text-tests.txt", "--test i18n_snapshots", "--test nav_explorer", "textfit_extras", "--features brick --test text_size"] {
            for (file, text) in [("scripts/pre-release.sh", pre), ("scripts/release-extras.sh", extras)] {
                if !text.contains(needle) {
                    bad.push(format!("{file} does not run `{needle}` (the other script does)"));
                }
            }
        }
        if !fns_with_ignore_reason(&read("tests/text_size.rs")).iter().any(|(n, _)| n.starts_with("textfit_extras")) {
            bad.push("scripts/pre-release.sh and release-extras.sh filter text_size by `textfit_extras`: no such test".into());
        }
    }
    // (5) the nightly (private tooling, not in the export) runs the extras after the full suite.
    if let Ok(nightly) = std::fs::read_to_string(root().join("maintainer/release-prevention/nightly-gate.sh")) {
        if !nightly.contains("scripts/release-extras.sh") {
            bad.push("maintainer/release-prevention/nightly-gate.sh does not run scripts/release-extras.sh".into());
        }
    }
    assert!(bad.is_empty(), "the release-only checks and the nightly run disagree:\n{}", bad.join("\n"));
}

/// The check itself: it reads a name's ignore reason and skips nothing by accident.
#[test]
fn the_release_only_scan_reads_ignore_reasons() {
    let src = "#[test]\n#[ignore = \"release check: a thing\"]\nfn one() {}\n\n#[test]\nfn two() {}\n    #[ignore = \"timing tier\"]\n    pub fn three() {}\n";
    let got = fns_with_ignore_reason(src);
    assert_eq!(got, vec![("one".into(), Some("release check: a thing".into())), ("two".into(), None), ("three".into(), Some("timing tier".into()))]);
    assert_eq!(shell_value("A=\"x\"\nTEXT_SUITES=\"a b\"\n", "TEXT_SUITES").as_deref(), Some("a b"));
}

// ---- one name per value, and du everywhere -------------------------------------------------------------------------

/// Two values of one row, or two entries of one list, never read the same in any language (review 19.11. #3: Italian
/// had Picture › Look's Smooth and Soft both "Morbido", and the pause menu's Earlier beside Load's "Before load", both
/// "Prima"). The class: every setting's values and every group's rows of the Options (`options::GROUPS`), each Settings
/// category's rows (`SETTINGS_GROUPS`), the layouts, and what the pause menu shows at once (its rows, Load's chips).
#[test]
fn i18n_no_two_entries_of_a_list_share_a_name() {
    use pocket_ui::i18n::{tr_in, Lang};
    use pocket_ui::pause_list::Item;
    use pocket_ui::states::{chip, Target};
    let mut lists: Vec<(String, Vec<String>)> = Vec::new();
    for g in pocket_ui::options::GROUPS.iter() {
        lists.push((format!("Options › {}", g.name), g.settings.iter().map(|s| s.name.to_string()).collect()));
        for s in g.settings {
            lists.push((format!("{} › {}", g.name, s.name), s.choices.iter().map(|c| c.label.to_string()).collect()));
        }
    }
    for (category, rows) in super::SETTINGS_GROUPS.iter() {
        lists.push((format!("Settings › {category}"), rows.iter().map(|r| r.0.to_string()).collect()));
    }
    lists.push(("Settings › Look › Layout".into(), ["Shelf", "Easy", "Split", "Cabinet"].map(String::from).to_vec()));
    let mut pause: Vec<String> = [Item::Continue, Item::Save, Item::Load, Item::Earlier, Item::Disc, Item::Options, Item::Achievements, Item::Quit]
        .iter().map(|i| i.label().to_string()).collect();
    pause.extend(["Save & quit".to_string(), chip(Target::Auto), chip(Target::Undo)]);
    lists.push(("the pause menu with Load's chips".into(), pause));
    let mut bad = Vec::new();
    for lang in Lang::ALL {
        for (list, entries) in &lists {
            let mut seen: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
            for en in entries {
                let said = tr_in(lang, en).to_lowercase();
                if let Some(other) = seen.insert(said.clone(), en) {
                    if other != en {
                        bad.push(format!("{lang:?}: {list}: {other:?} and {en:?} are both {said:?}"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} entries share a name:\n{}", bad.len(), bad.join("\n"));
}

/// The interface says du/tu/tú/tu/você in every language (review 19.11. #3: one French line said "Choisissez"): no formal
/// address in any table, the Brick's included.
#[test]
fn i18n_no_formal_address() {
    // (language, the words of formal address). German's are checked apart: only where a sentence doesn't start, as at
    // its start "Sie"/"Ihre" can be "they"/"their" ("Ihre Spiele bleiben so oder so").
    let formal: [(&str, &[&str]); 5] = [
        ("de", &[]),
        ("fr", &["vous", "votre", "vos", "Choisissez", "Appuyez", "Sélectionnez", "Utilisez", "Activez", "Ouvrez", "Copiez", "Mettez", "Réessayez", "Vérifiez", "Branchez", "Insérez", "Patientez", "Allumez", "Gardez"]),
        ("es", &["usted", "ustedes", "Pulse", "Elija", "Seleccione", "Presione", "Inténtelo", "Asegúrese", "Utilice", "Introduzca", "Vuelva"]),
        ("pt", &["senhor", "senhora", "Vossa"]),
        ("it", &["Prema", "Scelga", "Selezioni", "Utilizzi", "Riprovi", "Attenda", "Lei"]),
    ];
    let mut bad = Vec::new();
    for (lang, words) in formal {
        for file in [format!("assets/i18n/{lang}.tsv"), format!("assets/i18n/brick-{lang}.tsv")] {
            for (n, line) in read(&file).lines().enumerate().filter(|(_, l)| !l.starts_with('#')) {
                let Some((_, tr)) = line.split_once('\t') else { continue };
                let tokens: Vec<&str> = tr.split(|c: char| !c.is_alphanumeric()).collect();
                let mut hits: Vec<&str> = words.iter().copied().filter(|w| tokens.contains(w)).collect();
                if lang == "de" {
                    // "Sie", "Ihnen", "Ihre" … after a word that isn't a sentence's end: the formal you.
                    for word in ["Sie", "Ihnen", "Ihr", "Ihre", "Ihren", "Ihrem", "Ihrer", "Ihres"] {
                        for (i, _) in tr.match_indices(&format!(" {word}")) {
                            let ends = tr[i + 1 + word.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                            if ends && !tr[..i].trim_end().ends_with(['.', ':', '!', '?', '–', '—']) { hits.push(word); }
                        }
                    }
                }
                if !hits.is_empty() { bad.push(format!("{file}:{}: {hits:?}: {tr}", n + 1)); }
            }
        }
    }
    assert!(bad.is_empty(), "formal address:\n{}", bad.join("\n"));
}

/// A renamed row is not named by its old name in the manual (review 19.11. #15: five pages still said **Animations: Cozy**
/// after the row became Game start animation). One line per rename: (old, new).
#[test]
fn the_manual_names_no_renamed_row_by_its_old_name() {
    const RENAMED: [(&str, &str); 1] = [("Animations", "Game start animation")];
    let mut bad = Vec::new();
    for file in markdown_files().into_iter().filter(|f| f.starts_with(root().join("docs/user"))) {
        let md = std::fs::read_to_string(&file).unwrap();
        for (n, line) in md.lines().enumerate() {
            for (old, new) in RENAMED {
                if line.contains(&format!("**{old}**")) || line.contains(&format!("**{old}:")) {
                    bad.push(format!("{}:{}: **{old}** is **{new}** now", file.display(), n + 1));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Public language rules in `docs/user/*`: no vetoed terms ("home screen", "lived-in ... default"), no foreign frontend
/// comparisons/names (NextUI, MinUI, Daijishō, ES-DE, KNULLI, Batocera, standalone ArkOS), and no prose mentions of
/// EmulationStation outside the literal in-app menu row labels (and none at all in Brick-only manual pages).
#[test]
fn the_user_guide_has_no_vetoed_terms_or_other_frontends() {
    let user_dir = root().join("docs/user");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&user_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md" || e == "inc"))
        .collect();
    files.sort();
    let mut bad = Vec::new();
    const OTHER_FRONTENDS: [&str; 6] = ["nextui", "minui", "daijish", "es-de", "knulli", "batocera"];
    const UI_ES_ROWS: [&str; 3] = ["from emulationstation", "back to emulationstation", "open emulationstation"];
    for file in files {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_brick = name.starts_with("brick-");
        let text = std::fs::read_to_string(&file).unwrap();
        for (idx, line) in text.lines().enumerate() {
            let low = line.to_lowercase();
            let ln = idx + 1;
            if low.contains("home screen") {
                bad.push(format!("docs/user/{name}:{ln}: vetoed \"home screen\": {line}"));
            }
            if low.contains("lived-in") && (low.contains("default") || low.contains("from the start")) {
                bad.push(format!("docs/user/{name}:{ln}: vetoed lived-in+default wording: {line}"));
            }
            for fw in OTHER_FRONTENDS {
                if low.contains(fw) {
                    bad.push(format!("docs/user/{name}:{ln}: foreign frontend {fw:?}: {line}"));
                }
            }
            // Standalone "ArkOS" (not "dArkOS", which is pocket-ui's supported port).
            for (pos, _) in low.match_indices("arkos") {
                let prev = pos.checked_sub(1).and_then(|i| low.as_bytes().get(i)).copied();
                if prev != Some(b'd') && prev.is_none_or(|b| !b.is_ascii_alphanumeric()) {
                    bad.push(format!("docs/user/{name}:{ln}: foreign OS \"ArkOS\": {line}"));
                    break;
                }
            }
            if low.contains("emulationstation") {
                if is_brick {
                    bad.push(format!("docs/user/{name}:{ln}: EmulationStation in Brick manual: {line}"));
                } else {
                    // Strip the literal in-app menu row labels; any remaining EmulationStation is prose to neutralize.
                    let mut stripped = low.clone();
                    for row in UI_ES_ROWS {
                        stripped = stripped.replace(row, "");
                    }
                    if stripped.contains("emulationstation") {
                        bad.push(format!("docs/user/{name}:{ln}: prose EmulationStation: {line}"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} user-guide language issues:\n{}", bad.len(), bad.join("\n"));
}

/// `docs/user/*` stays current with DSperate (default DS core on H700, Brick, RK3566, dArkOS) and native `.7z` support in
/// pocket-core.
#[test]
fn the_user_guide_documents_dsperate_and_native_7z() {
    let systems = read("docs/user/systems.md");
    let brick_copy = read("docs/user/brick-copy-games.md.inc");
    let troubleshooting = read("docs/user/troubleshooting.md");
    let coming_from = read("docs/user/coming-from.md");
    assert!(systems.contains("DSperate"), "docs/user/systems.md should document DSperate");
    assert!(!systems.contains("DraStic for DS"), "docs/user/systems.md still claims DraStic for DS on H700");
    assert!(!systems.contains("`.7z` start with RetroArch") && !systems.contains("So do `.7z` games"),
        "docs/user/systems.md still sends .7z games to RetroArch");
    assert!(brick_copy.contains("DSperate"), "docs/user/brick-copy-games.md.inc should document DSperate");
    assert!(!troubleshooting.contains("`.7z` games\nalways go to RetroArch") && !troubleshooting.contains("`.7z` games always go to RetroArch"),
        "docs/user/troubleshooting.md still says .7z games always go to RetroArch");
    assert!(!coming_from.contains("Every other system, `.7z` games"),
        "docs/user/coming-from.md still sends .7z games to RetroArch");
}

// ---- no dashes as punctuation in public texts -----------------------------------------------------------------------

/// Returns `(1-based line, line)` for every line in `text` that contains an em-dash (`—`) or a non-numeric en-dash
/// (`–` not immediately between two ASCII digits, such as `2–5 min` or `48–54 fps`).
fn lines_with_disallowed_dashes(text: &str) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let mut chars = line.char_indices().peekable();
        let mut prev: Option<char> = None;
        let mut hit = false;
        while let Some((_, c)) = chars.next() {
            let next = chars.peek().map(|&(_, nc)| nc);
            if c == '\u{2014}' {
                hit = true;
                break;
            }
            if c == '\u{2013}' {
                let numeric_range = (prev.is_some_and(|p| p.is_ascii_digit()) && next.is_some_and(|n| n.is_ascii_digit()))
                    || (prev == Some('}') && next == Some('{'));
                if !numeric_range {
                    hit = true;
                    break;
                }
            }
            prev = Some(c);
        }
        if hit {
            bad.push((idx + 1, line.to_string()));
        }
    }
    bad
}

/// Paths excluded from the public export (`scripts/public-exclude.txt`), when that file is present (private repo).
fn public_exclude_rules() -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root().join("scripts/public-exclude.txt")) else {
        return Vec::new();
    };
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn is_public_excluded(rel: &str, rules: &[String]) -> bool {
    rules.iter().any(|rule| {
        if rule.ends_with('/') {
            rel.starts_with(rule)
        } else if let Some((pre, suf)) = rule.split_once('*') {
            rel.starts_with(pre) && rel.ends_with(suf)
        } else {
            rel == rule
        }
    })
}

/// Public texts (README, `docs/user/*`, public `docs/**`, `CHANGELOG.md` `[Unreleased]`, `NOTICE.md`, `assets/**/*.md`)
/// must never use em-dashes (`—`) or en-dashes (`–`) as punctuation. En-dashes are only allowed in numeric ranges
/// (`2–5 min`, `48–54 fps`). Published sections of `CHANGELOG.md` stay untouched so they remain byte-equal to their tags.
#[test]
fn public_texts_have_no_dashes() {
    let excludes = public_exclude_rules();
    let mut files = markdown_files();
    // Include Brick/Android `.md.inc` user guide fragments bundled into the manual.
    if let Ok(entries) = std::fs::read_dir(root().join("docs/user")) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "inc") {
                files.push(p);
            }
        }
    }
    files.sort();
    files.dedup();

    let mut bad = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
        if is_public_excluded(&rel, &excludes) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        if rel == "CHANGELOG.md" {
            // Check only [Unreleased]; published sections must stay byte-identical to their release tags.
            let mut in_unreleased = false;
            let mut buf = String::new();
            let mut offset = 0usize;
            for (idx, line) in text.lines().enumerate() {
                if line.starts_with("## [") {
                    if line.starts_with("## [Unreleased]") {
                        in_unreleased = true;
                        offset = idx;
                    } else if in_unreleased {
                        break;
                    }
                }
                if in_unreleased {
                    buf.push_str(line);
                    buf.push('\n');
                }
            }
            for (n, line) in lines_with_disallowed_dashes(&buf) {
                bad.push(format!("{rel}:{}: {line}", offset + n));
            }
            continue;
        }
        for (n, line) in lines_with_disallowed_dashes(&text) {
            bad.push(format!("{rel}:{n}: {line}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} line(s) in public docs contain em-dashes or non-numeric en-dashes (only digit–digit ranges are allowed):\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn disallowed_dash_detector_allows_numeric_ranges_only() {
    assert!(lines_with_disallowed_dashes("Takes 2–5 min at 48–54 fps (1–3 players).").is_empty());
    assert_eq!(lines_with_disallowed_dashes("A sentence — with an em-dash.").len(), 1);
    assert_eq!(lines_with_disallowed_dashes("A sentence – with an en-dash.").len(), 1);
    assert_eq!(lines_with_disallowed_dashes("Wrong range 2—5 min.").len(), 1);
    assert_eq!(lines_with_disallowed_dashes("| Cell | — |").len(), 1);
    assert_eq!(lines_with_disallowed_dashes("| Cell | – |").len(), 1);
    assert!(lines_with_disallowed_dashes("| Cell | - |").is_empty());
}

/// Translation tables (`assets/i18n/*.tsv`) must also be free of em-dashes and non-numeric en-dashes.
#[test]
fn i18n_tables_have_no_dashes() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("assets/i18n"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "tsv"))
        .collect();
    files.sort();
    let mut bad = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file).unwrap();
        for (idx, line) in text.lines().enumerate() {
            if line.starts_with('#') {
                continue;
            }
            if !lines_with_disallowed_dashes(line).is_empty() {
                bad.push(format!("{rel}:{}: {line}", idx + 1));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} line(s) in assets/i18n/*.tsv contain em-dashes or non-numeric en-dashes:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// ---- OpenABL instructions ---------------------------------------------------------------------------------------

/// The blocks of a text: paragraphs, and each list item or table row on its own. (1-based first line, text.)
fn text_blocks(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut open = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        let item = t.starts_with("- ")
            || t.starts_with("* ")
            || t.starts_with('|')
            || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if t.is_empty() {
            open = false;
            continue;
        }
        if !open || item {
            out.push((i + 1, String::new()));
            open = true;
        }
        let last = out.last_mut().unwrap();
        last.1.push_str(t);
        last.1.push(' ');
    }
    out
}

/// `phrase` in `lower` as whole words (`_` counts as a letter: `safe_backup_abl.sh` is not `backup_abl.sh`).
fn has_words(lower: &str, phrase: &str) -> bool {
    lower.match_indices(phrase).any(|(at, _)| {
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let before = lower[..at].chars().next_back().is_none_or(|c| !word(c));
        let after = lower[at + phrase.len()..].chars().next().is_none_or(|c| !word(c));
        before && after
    })
}

/// Problems of one text with OpenABL instructions. 2026-11-22: the install texts said "run backup_abl.sh, then
/// flash_abl.sh" without saying where the scripts come from (our images leave ROCKNIX's `openabl` folder off, and
/// OpenABL's own release has no scripts), said "no PC needed" (the cards and the backup need a computer) and "Android
/// stays untouched" (OpenABL overwrites `abl_a` and `abl_b`). A Pocket 5 owner found no script to run.
fn openabl_problems(text: &str) -> Vec<String> {
    const WRONG: [&str; 5] = ["untouched", "no pc", "no computer", "without a computer", "without a pc"];
    let mut out = Vec::new();
    let blocks = text_blocks(text);
    for (line, block) in &blocks {
        let lower = block.to_lowercase();
        if !lower.contains("openabl") && !lower.contains("_abl.sh") {
            continue;
        }
        for w in WRONG.iter().filter(|w| has_words(&lower, w)) {
            out.push(format!("{line}: says \"{w}\" next to OpenABL (it needs a computer and overwrites the bootloader)"));
        }
    }
    // The source goes with the steps: in the block that names a script or in one of the six before it.
    let source = |b: &str| {
        let l = b.to_lowercase();
        l.contains("rocknix")
            && ["openabl` folder", "openabl folder", "folder `openabl`", "folder openabl", "rocknix_abl"].iter().any(|k| l.contains(k))
    };
    for (i, (line, block)) in blocks.iter().enumerate() {
        if (has_words(block, "backup_abl.sh") || has_words(block, "flash_abl.sh"))
            && !blocks[i.saturating_sub(6)..=i].iter().any(|(_, b)| source(b))
        {
            out.push(format!("{line}: names backup_abl.sh/flash_abl.sh but not where they come from (the ROCKNIX image's openabl folder)"));
        }
    }
    out
}

/// Every public text that tells a player to set up OpenABL says where the scripts come from and never calls it
/// computer-free or Android-untouched: the README and docs (public ones), the notes beside the images
/// (`deploy/image/*.txt.in`) and the Pocket 5 / Flip 2 notes `scripts/make-image.sh` writes.
#[test]
fn openabl_instructions_name_the_scripts_source_and_the_risk() {
    let excludes = public_exclude_rules();
    let mut files: Vec<String> = markdown_files()
        .iter()
        .map(|f| f.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/"))
        .filter(|rel| !is_public_excluded(rel, &excludes) && rel != "CHANGELOG.md")
        .collect();
    for e in std::fs::read_dir(root().join("deploy/image")).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.ends_with(".txt.in") {
            files.push(format!("deploy/image/{name}"));
        }
    }
    files.push("scripts/make-image.sh".into());
    files.sort();
    let mut bad = Vec::new();
    for rel in &files {
        let text = read(rel);
        for p in openabl_problems(&text) {
            bad.push(format!("{rel}:{p}"));
        }
        if matches!(rel.as_str(), "README.md" | "docs/user/index.md" | "docs/INSTALL-DUAL-BOOT.md" | "deploy/image/README-sm8550.txt.in") {
            for claim in [
                "Linux cannot see Android's internal storage",
                "the card never touches Android",
                "Ready-made SD card images write nothing",
                "Started through \"UEFI\" or OpenABL, the card hides",
            ] {
                if text.contains(claim) {
                    bad.push(format!("{rel}: storage isolation depends on the boot path: {claim}"));
                }
            }
        }
        let player_heading = match rel.as_str() {
            "README.md" => Some("Quick answers"),
            "docs/user/index.md" => Some("What it does not touch"),
            _ => None,
        };
        if let Some(heading) = player_heading {
            let answer = section(&text, heading);
            if !answer.contains("INSTALL-DUAL-BOOT.md") || ["GRUB", "/KERNEL", "UFS"].iter().any(|detail| answer.contains(detail)) {
                bad.push(format!("{rel}: {heading} should link to the boot details in the install guide"));
            }
            if rel == "README.md" && !answer.trim_start().starts_with("- **Do I lose my stock system?** No.") {
                bad.push(format!("{rel}: Quick answers should start with how to return to the stock system"));
            }
        }
        if matches!(rel.as_str(), "docs/INSTALL-DUAL-BOOT.md" | "deploy/image/README-sm8550.txt.in")
            && !["GRUB", "/KERNEL", "UFS"].iter().all(|detail| text.contains(detail))
        {
            bad.push(format!("{rel}: explain when the boot path leaves internal UFS enabled"));
        }
        if matches!(rel.as_str(), "docs/INSTALL-DUAL-BOOT.md" | "scripts/make-image.sh" | "deploy/image/README-sm8550.txt.in") {
            let backup = text.split_once("backup_abl.sh").and_then(|(_, rest)| rest.split_once("flash_abl.sh")).map_or("", |(before, _)| before);
            for detail in ["exactly once before flashing", "overwrites the original backup", "computer"] {
                if !backup.contains(detail) {
                    bad.push(format!("{rel}: backup instructions before flashing are missing: {detail}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "OpenABL instructions in public texts:\n{}", bad.join("\n"));
}

#[test]
fn openabl_check_catches_the_old_wording() {
    let old = "Set up OpenABL once. No computer needed: Run Script as Root runs `backup_abl.sh` first, then `flash_abl.sh`.";
    assert_eq!(openabl_problems(old).len(), 2);
    assert_eq!(openabl_problems("If it shows UEFI, install OpenABL once (back up first, Android stays untouched).").len(), 1);
    assert!(openabl_problems("Boot mode: Loader. Nothing is flashed, Android stays untouched.\n\nOpenABL: see below.").is_empty());
    let good = "1. Write the ROCKNIX SM8250 image, copy its `openabl` folder to internal storage.\n2. Run `backup_abl.sh`.";
    assert!(openabl_problems(good).is_empty());
    assert!(!has_words("no pcm output", "no pc"));
    assert!(openabl_problems("Back up abl_a/b with `scripts/android/safe_backup_abl.sh`.").is_empty());
    let far = "Write the ROCKNIX SM8250 image; copy its `openabl` folder.\n\na\n\nb\n\nc\n\nd\n\ne\n\nf\n\nRun `flash_abl.sh`.";
    assert_eq!(openabl_problems(far).len(), 1);
}
