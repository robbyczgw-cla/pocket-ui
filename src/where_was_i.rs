//! "Where was I?": coming back to a game after a long break. While the moment loads (the picture of where the game was
//! left fades in, `launch_picture`), a quiet card at the bottom says when it was played last, how long it has been
//! played in all, and the player's own note from the game card, if there is one. Nothing is asked. A pocket-core game
//! stands still at its moment under the card for [`SHOWN`] (counted with the time it took to load) and then plays on; any
//! button ends that at once. Other emulators show the card while they load, until the game covers it
//! (docs/user/continue.md "Where was I?").
//!
//! Only when a moment is continued (the launch has a picture of it and keeps it, `frozen::at_launch`) and the last
//! session ended more than [`AWAY`] ago; never in Kids Mode; Settings → Playing → Remind me where I was switches it
//! off. Deciding is a lookup in what pocket-ui holds already (the recent games, the play log, the notes).

use crate::gfx::{mix, Canvas};
use crate::kit;
use crate::palette::pal;
use crate::text::{Face, Text};
use crate::type_scale;

/// How long a game must have rested before its next start shows the card.
pub const AWAY: u64 = 14 * DAY;
/// How long a pocket-core game stands still at its moment under the card, unless a button comes first.
pub const SHOWN: std::time::Duration = std::time::Duration::from_secs(4);
const DAY: u64 = 86_400;

/// What the card says; built at the launch, painted while the moment loads.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// "Last played 3 weeks ago".
    pub ago: String,
    /// "12 h 05 min played"; None when the time is not known (or under a minute), and in Easy.
    pub played: Option<String>,
    /// The game card's note as the player wrote it; None without one.
    pub note: Option<String>,
}

/// What the launch knows about the game and the player's choices.
#[derive(Clone, Debug, Default)]
pub struct Facts<'a> {
    /// Settings → Playing → Remind me where I was.
    pub on: bool,
    /// Kids Mode is on: no card.
    pub kids: bool,
    /// The Easy look: the plain card, without the time played.
    pub easy: bool,
    /// Unix seconds the last session ended; None when the game was never played (as far as pocket-ui knows).
    pub last_played: Option<u64>,
    /// Seconds played in all.
    pub play_secs: u64,
    /// The game card's note ("" without one).
    pub note: &'a str,
}

/// The card for a start at `now` (unix seconds), or None when there is none to show. Whether a moment is continued is
/// known only when the launch picture comes (`App::where_was_i_shown`).
pub fn card(f: &Facts, now: u64) -> Option<Card> {
    if !f.on || f.kids {
        return None;
    }
    let away = now.checked_sub(f.last_played?)?;
    if away <= AWAY {
        return None;
    }
    let played = Some(f.play_secs).filter(|s| *s >= 60 && !f.easy).map(|s| format!("{} played", crate::play_time::duration(s)));
    let note = Some(f.note.trim()).filter(|n| !n.is_empty()).map(str::to_string);
    Some(Card { ago: ago(away), played, note })
}

/// "Last played 3 weeks ago", "Last played 4 months ago", "Last played a year ago": the break in round words.
pub fn ago(secs: u64) -> String {
    let days = secs / DAY;
    match days {
        ..=59 => format!("Last played {} weeks ago", (days / 7).max(2)),
        60..=364 => format!("Last played {} months ago", days / 30),
        365..=729 => "Last played a year ago".to_string(),
        _ => format!("Last played {} years ago", days / 365),
    }
}

/// When a game of `path` was last played and how long in all: the recent games first (what the game card says), else
/// the play log (a game that fell off the recent games' twenty while others were played).
pub fn last_played(recent: Option<(u64, u64)>, log: Option<&crate::play_log::Log>, path: &str) -> Option<(u64, u64)> {
    if let Some(r) = recent {
        return Some(r);
    }
    let log = log?;
    let game = log.find(path)?;
    let mut last = None::<u64>;
    let mut secs = 0;
    for s in log.sessions.iter().filter(|s| s.game == game && s.ended) {
        last = Some(last.map_or(s.end(), |l| l.max(s.end())));
        secs += s.secs;
    }
    for c in log.carried.iter().filter(|c| c.game == game) {
        secs += c.secs;
        last = last.max(c.last_played);
    }
    last.map(|l| (l, secs))
}

const TITLE: u32 = 37;
const BODY: u32 = 28;
/// The widest the card gets on a large screen: a note of a few words reads as one line, not a band across the screen.
const MAX_W: i32 = 900;

/// The card's height for the width `w` it is painted at.
pub fn height(text: &mut Text, card: &Card, w: i32) -> i32 {
    let lines = note_lines(text, card, w).len() as i32;
    let mut h = kit::PANEL_PAD * 2 + type_scale::px(TITLE) as i32;
    if card.played.is_some() {
        h += type_scale::line(BODY, 12);
    }
    if lines > 0 {
        h += 18 + lines * type_scale::line(BODY, 10);
    }
    h
}

fn note_lines(text: &mut Text, card: &Card, w: i32) -> Vec<String> {
    let Some(note) = &card.note else { return Vec::new() };
    // The player's words, never taken for a line of the interface.
    let was = text.translating();
    text.set_translate(false);
    let lines = text.wrap_prose_at_most(Face::Body, BODY, &format!("“{note}”"), w - kit::PANEL_PAD * 2, 3);
    text.set_translate(was);
    lines
}

/// The widest the card may be on a `screen_w` wide screen.
fn widest(screen_w: i32) -> i32 {
    (screen_w - kit::PAD * 2).min(type_scale::wide(MAX_W, BODY)).max(1)
}

/// The card's width on a `screen_w` wide screen: as wide as its longest line, so that a short card stays small.
pub fn width(text: &mut Text, card: &Card, screen_w: i32) -> i32 {
    let most = widest(screen_w);
    let mut longest = text.width(Face::Title, TITLE, &card.ago);
    if let Some(p) = &card.played {
        longest = longest.max(text.width(Face::Body, BODY, p));
    }
    let was = text.translating();
    text.set_translate(false);
    for line in note_lines(text, card, most) {
        longest = longest.max(text.width(Face::Body, BODY, &line));
    }
    text.set_translate(was);
    (longest + kit::PANEL_PAD * 2).min(most)
}

/// Paints the card centred, its bottom edge at `bottom`, mixed in at `alpha` (0 nothing, 255 all of it) like the
/// picture behind it.
pub fn paint(c: &mut Canvas, text: &mut Text, card: &Card, bottom: i32, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let w = width(text, card, c.w);
    let h = height(text, card, w);
    let (x, y) = ((c.w - w) / 2, (bottom - h).max(0));
    // It covers what the faded page has under it (the hint bar), for the snapshot checks of text running into text.
    crate::text::boxes::covered_since(crate::text::boxes::mark(), c, (x, y, w, h));
    // What is under it, to mix the card in at `alpha` (only its own rectangle).
    let under = (alpha < 255).then(|| (y..(y + h).min(c.h)).flat_map(|r| c.buf[(r * c.w + x.max(0)) as usize..(r * c.w + (x + w).min(c.w)) as usize].to_vec()).collect::<Vec<u32>>());
    kit::panel(c, x, y, w, h);
    let tx = x + kit::PANEL_PAD;
    let mut base = y + kit::PANEL_PAD + type_scale::px(TITLE) as i32 * 4 / 5;
    let room = w - kit::PANEL_PAD * 2;
    let ago = text.fit(Face::Title, TITLE, &card.ago, room);
    text.draw(c, Face::Title, TITLE, tx, base, pal().text, &ago);
    if let Some(played) = &card.played {
        base += type_scale::line(BODY, 12);
        let played = text.fit(Face::Body, BODY, played, room);
        text.draw(c, Face::Body, BODY, tx, base, pal().quiet, &played);
    }
    let lines = note_lines(text, card, w);
    if !lines.is_empty() {
        base += 18;
        let was = text.translating();
        text.set_translate(false);
        for line in &lines {
            base += type_scale::line(BODY, 10);
            text.draw(c, Face::Body, BODY, tx, base, pal().text, line);
        }
        text.set_translate(was);
    }
    if let Some(under) = under {
        let (x0, x1) = (x.max(0), (x + w).min(c.w));
        let mut i = 0;
        for r in y..(y + h).min(c.h) {
            for col in x0..x1 {
                let p = (r * c.w + col) as usize;
                c.buf[p] = mix(under[i], c.buf[p], alpha as u32);
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn facts(days_ago: u64, note: &str) -> Facts<'_> {
        Facts { on: true, kids: false, easy: false, last_played: Some(NOW - days_ago * DAY), play_secs: 12 * 3600 + 5 * 60, note }
    }

    #[test]
    fn no_card_within_fourteen_days() {
        assert_eq!(card(&facts(0, "x"), NOW), None, "today");
        assert_eq!(card(&facts(13, "x"), NOW), None);
        assert_eq!(card(&Facts { last_played: Some(NOW - AWAY), ..facts(0, "x") }, NOW), None, "exactly 14 days: not yet");
        assert!(card(&Facts { last_played: Some(NOW - AWAY - 1), ..facts(0, "x") }, NOW).is_some(), "past 14 days");
        assert!(card(&facts(23, ""), NOW).is_some());
        // A clock set back (the last session "in the future") and a game never played: no card, no wrong words.
        assert_eq!(card(&Facts { last_played: Some(NOW + DAY), ..facts(0, "") }, NOW), None);
        assert_eq!(card(&Facts { last_played: None, ..facts(0, "") }, NOW), None);
    }

    #[test]
    fn switched_off_or_in_kids_mode_there_is_none() {
        assert_eq!(card(&Facts { on: false, ..facts(30, "x") }, NOW), None);
        assert_eq!(card(&Facts { kids: true, ..facts(30, "x") }, NOW), None);
    }

    #[test]
    fn without_a_note_there_is_no_note_field() {
        let c = card(&facts(23, ""), NOW).unwrap();
        assert_eq!(c, Card { ago: "Last played 3 weeks ago".into(), played: Some("12 h 05 min played".into()), note: None });
        assert_eq!(card(&facts(23, "   \n"), NOW).unwrap().note, None, "blank is no note");
        let c = card(&facts(23, " Fire temple, key still missing. "), NOW).unwrap();
        assert_eq!(c.note.as_deref(), Some("Fire temple, key still missing."));
    }

    #[test]
    fn easy_is_plain_and_an_unknown_time_is_left_out() {
        assert_eq!(card(&Facts { easy: true, ..facts(23, "") }, NOW).unwrap().played, None);
        assert_eq!(card(&Facts { play_secs: 0, ..facts(23, "") }, NOW).unwrap().played, None);
        assert_eq!(card(&Facts { play_secs: 59, ..facts(23, "") }, NOW).unwrap().played, None);
    }

    #[test]
    fn the_break_in_round_words() {
        let words = |d: u64| ago(d * DAY + 3600);
        assert_eq!(words(14), "Last played 2 weeks ago");
        assert_eq!(words(23), "Last played 3 weeks ago");
        assert_eq!(words(59), "Last played 8 weeks ago");
        assert_eq!(words(60), "Last played 2 months ago");
        assert_eq!(words(364), "Last played 12 months ago");
        assert_eq!(words(365), "Last played a year ago");
        assert_eq!(words(800), "Last played 2 years ago");
    }

    #[test]
    fn the_recent_games_first_then_the_play_log() {
        assert_eq!(last_played(Some((5, 6)), None, "/x"), Some((5, 6)));
        assert_eq!(last_played(None, None, "/x"), None);
        let game = crate::play_log::Game { path: "/roms/gba/a.gba".into(), name: "A".into(), system: "gba".into() };
        let s = |start: u64, secs: u64, ended: bool| crate::play_log::Session { game: 0, start, secs, ended, ..Default::default() };
        let log = crate::play_log::build(vec![game], vec![s(100, 50, true), s(1000, 30, true), s(5000, 0, false)], Vec::new(), vec![crate::play_log::Carried { game: 0, secs: 20, sessions: 1, last_played: Some(10) }]);
        assert_eq!(last_played(None, Some(&log), "/roms/gba/a.gba"), Some((1030, 100)), "the last ended session; all the time");
        assert_eq!(last_played(None, Some(&log), "/roms/gba/b.gba"), None);
    }

    #[test]
    fn deciding_is_quick_even_with_a_long_play_log() {
        // It runs at every start, after the game's process is on its way: a lookup, never a wait. Years of play (20 000
        // sessions over 200 games), each game looked up in the log as if it had fallen off the recent games.
        let games: Vec<_> = (0..200).map(|i| crate::play_log::Game { path: format!("/roms/snes/{i}.sfc"), name: format!("{i}"), system: "snes".into() }).collect();
        let sessions = (0..20_000u64).map(|i| crate::play_log::Session { game: (i % 200) as usize, start: 1_700_000_000 + i * 3600, secs: 1800, ended: true, ..Default::default() }).collect();
        let log = crate::play_log::build(games, sessions, Vec::new(), Vec::new());
        let t = std::time::Instant::now();
        for i in 0..200 {
            let (last, secs) = last_played(None, Some(&log), &format!("/roms/snes/{i}.sfc")).unwrap();
            let f = Facts { on: true, last_played: Some(last), play_secs: secs, note: "a note", ..Default::default() };
            assert!(card(&f, last + 30 * DAY).is_some());
        }
        // 200 starts' worth; one start takes a hundredth of this.
        assert!(t.elapsed() < std::time::Duration::from_millis(200), "{:?} for 200 starts", t.elapsed());
    }
}
