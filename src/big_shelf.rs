//! The Easy look's order and jumps (docs/BIG-SHELF.md; the screen is `app_big_shelf`; "Big" in the code). One stable
//! order, so a game stays where it is between sessions: the favourites first, then each system with all its games. A group is
//! what Up and Down jump between. A favourite stands twice, in front and on its system.

/// Where a group starts in the order. `system` None: the favourites.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    pub start: usize,
    pub system: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Order {
    /// (system, game), as `Item::Game`.
    pub games: Vec<(usize, usize)>,
    pub groups: Vec<Group>,
    /// How many of `games` are the favourites in front.
    pub front: usize,
    /// How many games on the systems behind them: "3 of 9" counts there, the favourites in front have no number.
    pub total: usize,
}

/// `counts`: each system shown and how many games it has, in the library's order. `favourites`: games of those systems.
pub fn arrange(counts: &[(usize, usize)], favourites: &[(usize, usize)]) -> Order {
    let mut order = Order::default();
    // Favourites that are all the games have nothing to stand in front of: each would just come twice.
    let games: usize = counts.iter().map(|(_, count)| count).sum();
    if !favourites.is_empty() && favourites.len() < games {
        order.groups.push(Group { start: 0, system: None });
        order.games.extend_from_slice(favourites);
        order.front = favourites.len();
    }
    for &(system, count) in counts.iter().filter(|(_, count)| *count > 0) {
        order.groups.push(Group { start: order.games.len(), system: Some(system) });
        order.games.extend((0..count).map(|g| (system, g)));
    }
    order.total = order.games.len() - order.front;
    order
}

/// The group the place `at` is in.
pub fn group_at(groups: &[Group], at: usize) -> usize {
    groups.iter().rposition(|g| g.start <= at).unwrap_or(0)
}

/// Left or Right. A press goes round at the ends; a held button stops there, so it can't run past the first game unseen.
pub fn step(len: usize, at: usize, forward: bool, held: bool) -> Option<usize> {
    let last = len.checked_sub(1)?;
    match (forward, at >= last, at == 0) {
        (true, true, _) if held => None,
        (true, true, _) => Some(0),
        (true, false, _) => Some(at + 1),
        (false, _, true) if held => None,
        (false, _, true) => Some(last),
        (false, _, false) => Some(at - 1),
    }
}

/// Up or Down. Down: the first game of the next group. Up: the first game of this group, from there the group before,
/// as L2 does with letters (`letters::jump`). A press goes round; a held button stops at the ends.
pub fn group_jump(groups: &[Group], at: usize, forward: bool, held: bool) -> Option<usize> {
    let last = groups.len().checked_sub(1)?;
    let here = group_at(groups, at);
    let to = if forward {
        match here == last {
            true if held => return None,
            true => 0,
            false => here + 1,
        }
    } else if groups[here].start < at {
        here
    } else {
        match here == 0 {
            true if held => return None,
            true => last,
            false => here - 1,
        }
    };
    Some(groups[to].start).filter(|start| *start != at)
}

/// What to do about a game that didn't start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    GameCheck,
    AddGames,
}

/// A failed start in plain words: `says` is for the player, `detail` goes small under the file's name.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub says: String,
    pub detail: Option<String>,
    pub fix: Fix,
}

/// From the shelf's notice (`fallback`, or pocket-core's own reason). What it would send a player to do in an emulator's menus or
/// a log stays out: the plain sentence and the one thing to press.
pub fn failure(name: &str, system: &str, notice: &str) -> Failure {
    let plain = if system == "ports" { "It may need files that aren't on the card." } else { "The game file may be damaged, or a file it needs isn't on the card." }.to_string();
    if notice.contains("BIOS files come from") {
        return Failure { says: notice.to_string(), detail: None, fix: Fix::AddGames };
    }
    if notice.starts_with(&format!("{name} didn't start")) {
        return Failure { says: plain, detail: None, fix: Fix::GameCheck };
    }
    let fix = if notice.contains("BIOS") { Fix::AddGames } else { Fix::GameCheck };
    let detail = Some(notice.to_string()).filter(|n| !n.contains("RetroArch") && !n.contains("Flatpak"));
    Failure { says: plain, detail, fix }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_says_it_plainly_and_names_no_emulator() {
        let f = failure("Metroid", "psx", "Metroid didn't start. The file may be damaged, or the system needs a BIOS file (Settings › Games › Add games).");
        assert_eq!((f.fix, f.detail), (Fix::GameCheck, None));
        assert!(!f.says.contains("Settings"), "{}", f.says);
        let f = failure("Metroid", "psx", "Metroid didn't start. scph.bin is missing. BIOS files come from your own console.");
        assert_eq!(f.fix, Fix::AddGames);
        assert!(f.says.contains("scph.bin"));
        let f = failure("Doom", "ports", "Doom didn't start. It may need files or libraries that aren't installed: reinstall it in Ports, or check log.txt in its folder.");
        assert!(!f.says.contains("log.txt") && f.says.contains("files"), "{}", f.says);
        let f = failure("Metroid", "gba", "Metroid didn't start. The system's core may not be installed: add it in RetroArch (Flatpak) > Online Updater > Core Downloader.");
        assert!(!f.says.contains("RetroArch") && f.detail.is_none(), "{f:?}");
        let f = failure("Metroid", "gba", "load /x_libretro.so: RetroArch said no");
        assert_eq!(f.detail, None, "an emulator's name stays off the page");
        let f = failure("Metroid", "psx", "This ROM set isn't one the core takes");
        assert_eq!(f.detail.as_deref(), Some("This ROM set isn't one the core takes"), "pocket-core's own reason, small");
        assert_eq!(failure("Metroid", "psx", "BIOS scph.bin missing").fix, Fix::AddGames);
    }

    #[test]
    fn the_order_is_favourites_then_every_system_whole() {
        let order = arrange(&[(0, 2), (1, 1), (2, 0)], &[(1, 0)]);
        assert_eq!(order.games, vec![(1, 0), (0, 0), (0, 1), (1, 0)]);
        assert_eq!(order.groups, vec![Group { start: 0, system: None }, Group { start: 1, system: Some(0) }, Group { start: 3, system: Some(1) }]);
        assert_eq!(order.front, 1);
    }

    #[test]
    fn the_count_is_of_the_systems_with_the_favourites_outside_it() {
        assert_eq!(arrange(&[(0, 2), (1, 1)], &[(1, 0)]).total, 3);
        assert_eq!(arrange(&[(0, 3)], &[]).total, 3);
        assert_eq!(arrange(&[], &[]), Order::default());
    }

    #[test]
    fn when_every_game_is_a_favourite_nothing_stands_twice() {
        let one = arrange(&[(0, 1)], &[(0, 0)]);
        assert_eq!((one.games.len(), one.front, one.total), (1, 0, 1));
        let all = arrange(&[(0, 1), (1, 1)], &[(0, 0), (1, 0)]);
        assert_eq!((all.games.len(), all.front), (2, 0));
    }

    #[test]
    fn playing_changes_nothing_in_the_order() {
        // What was played last isn't an input at all: the same library, the same places.
        assert_eq!(arrange(&[(0, 5), (1, 5)], &[]), arrange(&[(0, 5), (1, 5)], &[]));
    }

    #[test]
    fn left_and_right_go_round_on_a_press_and_stop_when_held() {
        assert_eq!(step(3, 2, true, false), Some(0));
        assert_eq!(step(3, 0, false, false), Some(2));
        assert_eq!(step(3, 2, true, true), None);
        assert_eq!(step(3, 0, false, true), None);
        assert_eq!(step(3, 1, true, true), Some(2));
        assert_eq!(step(0, 0, true, false), None);
        assert_eq!(step(1, 0, true, false), Some(0), "one game: a press lands on it again");
    }

    #[test]
    fn up_and_down_land_on_the_first_game_of_a_group() {
        let groups = [Group { start: 0, system: None }, Group { start: 20, system: Some(0) }, Group { start: 200, system: Some(1) }];
        assert_eq!(group_jump(&groups, 3, true, false), Some(20));
        assert_eq!(group_jump(&groups, 25, false, false), Some(20), "first this group's first game");
        assert_eq!(group_jump(&groups, 20, false, false), Some(0), "then the group before");
        assert_eq!(group_jump(&groups, 250, true, false), Some(0), "a press goes round");
        assert_eq!(group_jump(&groups, 0, false, false), Some(200));
        assert_eq!(group_jump(&groups, 250, true, true), None, "held: stops at the end");
        assert_eq!(group_jump(&groups, 0, false, true), None);
        assert_eq!(group_jump(&[], 0, true, false), None);
        assert_eq!(group_jump(&groups[..1], 0, true, false), None, "a single group: nowhere to go");
    }
}
