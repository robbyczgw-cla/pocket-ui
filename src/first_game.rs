//! The first game ever started on this card: one line over it says how to get out. HOME is named nowhere else on the
//! way from the first start to the first game, and in a game START and SELECT are the game's (new-player walk 2026-11-02).
use std::path::Path;

pub const NOTE: &str = "HOME opens the menu: save, load, quit.";

/// The first start's last page says it too, in plain words (a first game that runs in RetroArch or another emulator has no line over it).
pub const LAST_PAGE_LINE: &str = "In a game, HOME opens the menu: save, load, quit.";

const MARK: &str = "home-told";

/// Whether the first start's last page names HOME: not in Kids Mode (its own small pause menu, a child who can't read it).
pub fn told_on_last_page(data_dir: &Path) -> bool {
    !crate::kids::Config::load(data_dir).on
}

/// Whether to say it now: not in Kids Mode, which has its own small pause menu and a child who can't read it, and once
/// only. The mark is written here: a game that dies right after still counts as told.
pub fn tell_once(data_dir: &Path) -> bool {
    let mark = data_dir.join(MARK);
    if mark.exists() || crate::kids::Config::load(data_dir).on {
        return false;
    }
    let _ = std::fs::write(mark, b"");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_said_once_and_not_to_a_child() {
        let dir = crate::testutil::TestDir::new("first-game");
        assert!(tell_once(&dir), "the first game");
        assert!(!tell_once(&dir), "the second");

        let kids = crate::testutil::TestDir::new("first-game-kids");
        crate::kids::Config { on: true, ..Default::default() }.save(&kids);
        assert!(!tell_once(&kids), "Kids Mode");
    }

    #[test]
    fn the_last_page_names_home_except_in_kids_mode() {
        assert!(told_on_last_page(&crate::testutil::TestDir::new("first-game-page")));
        let kids = crate::testutil::TestDir::new("first-game-page-kids");
        crate::kids::Config { on: true, ..Default::default() }.save(&kids);
        assert!(!told_on_last_page(&kids));
    }
}
