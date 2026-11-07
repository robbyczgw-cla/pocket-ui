//! Part of the pocket-ui binary: the App's half of the navigation explorer's record (`pocket_ui::nav_probe`,
//! tests/nav_explorer.rs). After the test driver's `frame` or `nav` has painted a frame and `text::boxes::finish` has
//! written its strings and marks, this adds where the App is (`@where`), the list the frame registered for touch (`@list`
//! selected row and length, `@hit` each row's area), a fingerprint of the picture (`@hash`) and `@end`, which the
//! explorer waits for. Only with `--test-driver` and `POCKETUI_NAV_PROBE`; nothing here runs otherwise.
//!
//! `@where` names the page as a player would point at it, without the selection: "settings:List Look", "setup:look
//! (settings)", "pause:Options", with what covers it after a " / " ("quick", "power", "leave", "notice"...). Two frames
//! with the same `@where` are the same page; the explorer tells a page's own views apart by their rows.

use pocket_ui::touch::Target;

use super::{app_settings_groups, App, SettingsPage, SETTINGS_GROUPS};

/// A `Debug` name without the numbers that only say which row is selected: `Groups(3)` is `Groups`, `Options { sel: 2 }`
/// is `Options`, `Group { group: 1, sel: 0 }` is `Group { group: 1 }` (which group is a page of its own).
fn without_selection(debug: &str) -> String {
    let mut s = debug.to_string();
    // ", sel: 3" and "sel: 3, " and "sel: 3" alone.
    while let Some(at) = s.find("sel: ") {
        let digits = s[at + 5..].chars().take_while(|c| c.is_ascii_digit()).count();
        let mut end = at + 5 + digits;
        let mut start = at;
        if s[end..].starts_with(", ") {
            end += 2;
        } else if s[..start].ends_with(", ") {
            start -= 2;
        }
        s.replace_range(start..end, "");
    }
    let s = s.replace(" {  }", "").replace(" { }", "");
    // A lone number in brackets: the selected row of a page that has nothing else.
    match s.split_once('(') {
        Some((name, rest)) if rest.trim_end_matches(')').chars().all(|c| c.is_ascii_digit()) => name.to_string(),
        _ => s,
    }
}

impl App {
    /// The page the App shows and what covers it (see the module's head).
    pub(crate) fn nav_where(&self) -> String {
        let mut at = if let Some(m) = &self.menu {
            format!("pause:{}", without_selection(&format!("{:?}", m.page)))
        } else if let Some(setup) = &self.setup {
            let from_settings = if self.settings_page.is_some() { " (settings)" } else { "" };
            let cover = if setup.network_menu_open() { " / network menu" } else { "" };
            format!("setup:{}{from_settings}{cover}", setup.at())
        } else if let Some(page) = self.settings_page {
            match page {
                // A category's list is one page per category, whichever of its rows is selected.
                SettingsPage::List(row) => format!("settings:List {}", SETTINGS_GROUPS[app_settings_groups::group_of(row)].0.trim_end_matches('\u{200b}')),
                other => format!("settings:{}", without_selection(&format!("{other:?}"))),
            }
        } else if self.this_game.is_some() {
            "this game".into()
        } else if self.controls_card.is_some() {
            "controls card".into()
        } else if self.port_files.is_some() {
            "port files".into()
        } else if self.card.is_some() {
            "card".into()
        } else if self.console.is_some() {
            "console".into()
        } else if self.running.is_some() {
            // A game in front, pocket-ui behind it.
            "game".into()
        } else {
            format!("shelf:{:?}", self.screen)
        };
        // A notice is painted over the shelf's pages only.
        let notice = self.notice.is_some() && self.menu.is_none() && self.setup.is_none() && self.settings_page.is_none();
        for (covers, name) in [
            (self.where_was_i.is_some(), "where was I"),
            (self.switcher.is_some(), "recent games"),
            (self.sys.quick_open(), "quick"),
            (self.sys.power_menu_open(), "power"),
            (self.leave.is_some(), "leave"),
            (notice, "notice"),
        ] {
            if covers {
                at += " / ";
                at += name;
            }
        }
        at
    }

    /// The App's lines of the record, after the strings and marks `text::boxes::finish` wrote for `buf`.
    pub(crate) fn nav_probe_done(&self, buf: &[u32]) {
        if !pocket_ui::nav_probe::on() {
            return;
        }
        let Some(path) = std::env::var_os(pocket_ui::nav_probe::ENV) else { return };
        let mut out = std::fs::read_to_string(&path).unwrap_or_default();
        out += &format!("@where\t{}\n", self.nav_where());
        // The quick settings and the power menu keep their own touch map: the App's is the covered page's.
        let covered = self.sys.quick_open() || self.sys.power_menu_open();
        if let Some((sel, len)) = self.hits.list_state().filter(|_| !covered) {
            out += &format!("@list\t{sel}\t{len}\n");
        }
        for (a, t) in self.hits.targets().iter().filter(|_| !covered) {
            if let Target::Row(i) = t {
                out += &format!("@hit\t{}\t{}\t{}\t{}\t{i}\n", a.x, a.y, a.x + a.w, a.y + a.h);
            }
        }
        // FNV-1a over the pixels: whether two frames look the same.
        let hash = buf.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, p| (h ^ u64::from(*p)).wrapping_mul(0x0100_0000_01b3));
        out += &format!("@hash\t{hash:016x}\n@end\n");
        // Whole or not at all: the explorer reads the file as soon as it ends in "@end".
        let tmp = std::path::PathBuf::from(format!("{}.part", std::path::Path::new(&path).display()));
        if std::fs::write(&tmp, out).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::without_selection;

    #[test]
    fn a_page_is_named_without_its_selected_row() {
        assert_eq!(without_selection("Groups(3)"), "Groups");
        assert_eq!(without_selection("Options { sel: 2 }"), "Options");
        assert_eq!(without_selection("Group { group: 1, sel: 0 }"), "Group { group: 1 }");
        assert_eq!(without_selection("Emulator { cat: Some(2), sel: 4 }"), "Emulator { cat: Some(2) }");
        assert_eq!(without_selection("Manual"), "Manual");
        assert_eq!(without_selection("SecondScreen(Some((\"a\", \"b\")))"), "SecondScreen(Some((\"a\", \"b\")))");
    }
}
