//! Doc-drift checks: the user guide (`docs/user/`, which is also the manual on the device) and the developer documents
//! against the code, so a feature that changes a menu, a row, a default or a module can't leave the documentation
//! behind without a red test. What each check holds is listed in docs/DEVELOPING.md, "Documentation checks".
//!
//! The checks read the Markdown as it is in the repository (no network, a few milliseconds). They compare names and
//! structure, not prose: a row of a table, a bold name, a link. A failure names the file and what is missing; fix the
//! document (or the code), never the check.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pocket_ui::manual;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
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
    // Settings is two pages (the manual's 30 000 px limit): Look & feel, Playing and Games, then Connections and System.
    let md = format!("{}\n{}", read("docs/user/settings.md"), read("docs/user/settings-system.md"));
    // The categories, in their order, each naming its entries.
    let open = first_table(&section(&md, "Open Settings"));
    let doc: Vec<String> = open.iter().map(|r| bold(&r[0])).collect();
    let code: Vec<&str> = super::SETTINGS_GROUPS.iter().map(|c| c.0).collect();
    assert_eq!(
        doc, code,
        "docs/user/settings.md, \"Open Settings\": one row per category of main.rs SETTINGS_GROUPS, in its order"
    );
    for (row, (name, _, entries)) in open.iter().zip(super::SETTINGS_GROUPS.iter()) {
        for (entry, _, _) in entries.iter() {
            assert!(row[1].contains(entry), "docs/user/settings.md, \"Open Settings\": the row of {name} names {entry}");
        }
    }
    // Under each category's own heading, one row per entry, in its order: together every row of SETTINGS.
    let mut all = Vec::new();
    for (name, _, entries) in super::SETTINGS_GROUPS.iter() {
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
    let code: Vec<&str> = pocket_ui::cozy_extras::Extra::shown().iter().map(|e| e.label()).collect();
    assert_eq!(doc, code, "docs/user/settings.md, \"Cozy extras\": one row per extra on the page, in its order");
    for (r, e) in rows.iter().zip(pocket_ui::cozy_extras::Extra::shown()) {
        // Playful animations starts as Animations is (`Extra::default_on`).
        let start = match e {
            pocket_ui::cozy_extras::Extra::Playful => "On with Cozy, Off with Quick",
            e if e.default_on() => "On",
            _ => "Off",
        };
        assert_eq!(r[2], start, "docs/user/settings.md, \"Cozy extras\": {}'s start", e.label());
    }
}

#[test]
fn settings_curator_rows_and_their_start_settings_are_described() {
    let md = read("docs/user/settings.md");
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
    for (page, heading) in [("docs/user/quick-settings.md", "Sleep & power settings")] {
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
    let settings = read("docs/user/settings.md");
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
    let all = [Continue, Save, Load, Earlier, Disc, Options, Achievements, QuickSettings, PixelPath, Players, Quit];
    for item in all {
        match item {
            Continue | Save | Load | Earlier | Disc | Options | Achievements | QuickSettings | PixelPath | Players | Quit => {}
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
    let page = |saver: Saver, _options: bool, achievements: bool| -> Vec<MainItem> {
        let hosted = matches!(saver, Saver::PocketCore { .. });
        super::main_items(hosted, pocket_ui::pause_saves::Saves::of(saver, true, None), achievements)
    };
    let mut seen = BTreeSet::new();
    for row in first_table(&md) {
        let kind = row[0].as_str();
        let (key, want) = if kind.contains("signed in") {
            ("pocket-core signed in", page(Saver::PocketCore { states: true }, false, true))
        } else if kind.contains("no save states") {
            ("pocket-core without states", page(Saver::PocketCore { states: false }, false, false))
        } else if kind.starts_with("pocket-ui's own host") {
            ("pocket-core", page(Saver::PocketCore { states: true }, false, false))
        } else if kind.contains("PlayStation 2") {
            ("standalone", page(Saver::Standalone, true, false))
        } else if kind.starts_with("Xbox") {
            // hakuX while its core has no save states: the line in place of Save and Load.
            let saves = pocket_ui::pause_saves::Saves::of(Saver::Standalone, false, Some(pocket_ui::hakux::NO_STATES));
            ("hakuX", super::main_items(false, saves, false))
        } else if kind.starts_with("RetroArch") {
            ("retroarch", page(Saver::RetroArch, false, false))
        } else if kind.starts_with("PortMaster") {
            ("port", page(Saver::Port, false, false))
        } else if kind.starts_with("Other emulators") {
            ("other", page(Saver::Other, false, false))
        } else {
            panic!("docs/user/pause-menu.md: a row for an unknown kind of emulator: {kind:?}")
        };
        let doc: Vec<&str> = row[1].split(", ").collect();
        let code: Vec<&str> = want.iter().map(|i| i.label()).collect();
        assert_eq!(doc, code, "docs/user/pause-menu.md, the row {kind:?}");
        seen.insert(key);
    }
    assert_eq!(seen.len(), 8, "docs/user/pause-menu.md: a row for each kind of emulator, {seen:?}");
    // What Quit does is a setting, not a row: the guide names it beside Quit.
    let name = super::SETTINGS.iter().find(|s| matches!(s.1, super::Opens::RememberQuit)).expect("the Settings row").0;
    assert!(md.contains(&format!("**{name}**")), "docs/user/pause-menu.md names Settings → **{name}** under Quit");
    assert!(!md.contains("Quit without saving**"), "docs/user/pause-menu.md: \"Quit without saving\" is no row any more");
}

#[test]
fn every_option_group_setting_and_choice_is_described() {
    let md = read("docs/user/options.md");
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
    let extras = pocket_ui::options::GROUPS.iter().find(|g| g.name == "Extras").expect("the Extras group");
    let rows = first_table(&section(&md, "Extras"));
    for s in extras.settings {
        let row = rows
            .iter()
            .find(|r| bold(&r[0]) == s.name)
            .unwrap_or_else(|| panic!("options.md, Extras: a row for {}", s.name));
        let start = format!("**{}**", s.choices[s.default].label);
        assert!(row[1].contains(&start), "options.md, Extras, {}: the start value {start}", s.name);
    }
    for name in ["Emulator", "Controls", "Cheats", "Save changes"] {
        assert!(md.contains(&format!("**{name}**")), "docs/user/options.md: the entry **{name}**");
    }
    for choice in super::SAVE_CHANGES {
        assert!(md.contains(&format!("**{choice}**")), "docs/user/options.md: Save changes → **{choice}**");
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
    assert_eq!(got, want, "docs/user/tour.md: one row per switch card of features::FEATURES, in order: | **Title** | Simple | Enthusiast | … |");
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
        let (m, d) = (in_manual.contains(p.as_str()), manual::DESKTOP_ONLY.contains(&p.as_str()));
        assert!(
            m != d,
            "docs/user/{p}.md: in manual::PAGES or in manual::DESKTOP_ONLY, exactly one (manual {m}, desktop {d})"
        );
    }
    for (n, md) in manual::PAGES {
        assert_eq!(*md, read(&format!("docs/user/{n}.md")), "manual::PAGES {n:?} bundles docs/user/{n}.md");
    }
}

#[test]
fn the_first_page_lists_the_manual_pages_in_their_order() {
    let pages = section(&read("docs/user/index.md"), "Pages");
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
fn the_documentation_index_lists_every_document() {
    let index = read("docs/README.md");
    let linked: BTreeSet<String> = links(&index).into_iter().map(|(_, t)| t).collect();
    let mut missing = Vec::new();
    for dir in ["docs", "docs/user"] {
        for e in std::fs::read_dir(root().join(dir)).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") || (dir == "docs" && name == "README.md") {
                continue;
            }
            let rel = if dir == "docs" { name } else { format!("user/{name}") };
            if !linked.contains(&rel) {
                missing.push(rel);
            }
        }
    }
    missing.sort();
    assert!(missing.is_empty(), "docs/README.md links every document; missing: {missing:?}");
}

#[test]
fn developing_md_names_every_source_module() {
    let layout = section(&read("docs/DEVELOPING.md"), "Repository layout");
    let binaries = section(&read("docs/DEVELOPING.md"), "Building");
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
        "docs/DEVELOPING.md, \"Repository layout\" names every module of src/; missing: {missing:?}"
    );
}

#[test]
fn every_environment_variable_is_documented() {
    let env = section(&read("docs/DEVELOPING.md"), "Environment variables");
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
        "docs/DEVELOPING.md, \"Environment variables\" has every variable the code reads; missing: {missing:?}"
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
fn settings_paths_on_the_screen_use_the_glossarys_arrow() {
    // Put together, so this test's own lines don't count.
    let (en, de) = (["Settings", " > "].concat(), ["Einstellungen", " > "].concat());
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
    for (n, line) in read("assets/i18n/de.tsv").lines().enumerate() {
        if !line.starts_with('#') && (line.contains(&en) || line.contains(&de)) {
            bad.push(format!("assets/i18n/de.tsv:{}: {line}", n + 1));
        }
    }
    assert!(bad.is_empty(), "write a Settings path with \"›\":\n{}", bad.join("\n"));
}
