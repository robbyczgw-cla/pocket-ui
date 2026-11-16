//! "Where was I?": coming back to a game after a long break. While the moment loads (the picture of where the game was
//! left fades in, `launch_picture`), a quiet card at the bottom says when it was played last, how long it has been
//! played in all, and the player's own note from the game card, if there is one. Nothing is asked. A pocket-core game
//! stands still at its moment under the card until any button is pressed. Other emulators show the card while they load, until the game covers it
//! (docs/user/continue.md "Where was I?").
//!
//! Only when a moment is continued (the launch has a picture of it and keeps it, `frozen::at_launch`) and the last
//! session ended more than [`AWAY`] ago; never in Kids Mode; Settings → Playing → Remind me where I was is Off until it is
//! switched on (0.5). Deciding is a lookup in what pocket-ui holds already (the recent games, the play log, the notes).

use crate::gfx::{mix, Canvas};
use crate::kit;
use crate::palette::pal;
use crate::text::{Face, Text};
use crate::type_scale;

/// How long a game must have rested before its next start shows the card.
pub const AWAY: u64 = 14 * DAY;
const DAY: u64 = 86_400;

/// What the card says; built at the launch, painted while the moment loads.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// "Last played 3 weeks ago".
    pub ago: String,
    /// "12 h 05 min played"; None when the time is not known (or under a minute), and in Easy.
    pub played: Option<String>,
    /// Where the player is, read from the game's own save ("8 badges · Charizard Lv. 45 · 31:12 h", `case_knows`), in
    /// the interface's language; set by the app. It takes the place of `played`: one clock on the card, not two.
    pub progress: Option<String>,
    /// The team's pictures from the player's own ROM under it (`case_knows_art`), set with `progress`.
    pub art: Option<std::sync::Arc<crate::case_knows_art::Art>>,
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
    Some(Card { ago: ago(away), played, progress: None, art: None, note })
}

/// The height the team's pictures add under the line (`k` their scale), 0 without.
fn team_h(card: &Card, k: i32) -> i32 {
    let Some(a) = card.art.as_ref().filter(|a| !a.team.is_empty() && card.progress.is_some()) else { return 0 };
    a.team.iter().map(|p| crate::case_knows_art::bounds(p).3).max().unwrap_or(0) * k + 20
}

impl Card {
    /// The card's second line, if any: where the player is, else the time played.
    fn second(&self) -> Option<&String> {
        self.progress.as_ref().or(self.played.as_ref())
    }
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
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
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
pub fn height(text: &mut Text, card: &Card, w: i32, k: i32) -> i32 {
    let lines = note_lines(text, card, w).len() as i32;
    let mut h = kit::PANEL_PAD * 2 + type_scale::px(TITLE) as i32;
    if card.second().is_some() {
        h += type_scale::line(BODY, 12);
    }
    h += team_h(card, k);
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
    if let Some(p) = &card.played.as_ref().filter(|_| card.progress.is_none()) {
        longest = longest.max(text.width(Face::Body, BODY, p));
    }
    if let Some(p) = &card.progress {
        longest = longest.max(text.width_plain(Face::Body, BODY, p));
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
    let h = height(text, card, w, crate::case_knows_art::scale());
    let (x, y) = ((c.w - w) / 2, (bottom - h).max(0));
    // It covers what the faded page has under it (the hint bar), for the snapshot checks of text running into text.
    crate::text::boxes::covered_since(crate::text::boxes::mark(), c, (x, y, w, h));
    // What is under it, to mix the card in at `alpha` (only its own rectangle).
    // The screen pixels it covers (design pixels at `n` = 1; painted natively the screen's own, so the mix costs a
    // quarter and the edges round as the panel's `rect` does).
    let rows = (c.pixels().len() / c.stride().max(1)) as i32;
    let (sx0, sx1) = (c.screen_x(x.max(0)).clamp(0, c.stride() as i32), c.screen_x((x + w).min(c.w)).clamp(0, c.stride() as i32));
    let (sy0, sy1) = (c.screen_y(y.max(0)).clamp(0, rows), c.screen_y((y + h).min(c.h)).clamp(0, rows));
    let stride = c.stride() as i32;
    let under = (alpha < 255).then(|| (sy0..sy1).flat_map(|r| c.pixels()[(r * stride + sx0) as usize..(r * stride + sx1.max(sx0)) as usize].to_vec()).collect::<Vec<u32>>());
    kit::panel(c, x, y, w, h);
    let tx = x + kit::PANEL_PAD;
    let mut base = y + kit::PANEL_PAD + type_scale::px(TITLE) as i32 * 4 / 5;
    let room = w - kit::PANEL_PAD * 2;
    let ago = text.fit(Face::Title, TITLE, &card.ago, room);
    text.draw(c, Face::Title, TITLE, tx, base, pal().text, &ago);
    if let Some(progress) = &card.progress {
        // The game's own words and names, in the interface's language already: painted as they are.
        base += type_scale::line(BODY, 12);
        let progress = crate::case_knows_read::fit(text, Face::Body, BODY, progress, room);
        text.draw_name(c, Face::Body, BODY, tx, base, pal().text, &progress);
        // The team, small, under the line; no badges here: the card stays a few words over the game.
        if let Some(team) = card.art.as_ref().filter(|a| !a.team.is_empty()).map(|a| crate::case_knows_art::Art { team: a.team.clone(), ..Default::default() }) {
            let k = crate::case_knows_art::scale();
            base += crate::case_knows_art::paint_strip(c, Some(&team), None, (tx, base + 10, room), k, (pal().text, pal().quiet, pal().pill)) + 10;
        }
    } else if let Some(played) = &card.played {
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
        let mut i = 0;
        for r in sy0..sy1 {
            for col in sx0..sx1 {
                let p = (r * stride + col) as usize;
                c.pixels_mut()[p] = mix(under[i], c.pixels()[p], alpha as u32);
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    /// Painted natively (`n` = 2) against the old way (the design canvas and the 2:1 filter), over a page with a hard
    /// edge under the card: Departure Mono and the panel as before bit for bit when it is all there; mixed in half way,
    /// the screen pixels are mixed instead of the design pixels, a step of rounding apart.
    #[test]
    fn painted_natively_the_card_is_the_old_picture() {
        use crate::font_style::{self, FontStyle, RetroLetters};
        font_style::set_retro_letters(RetroLetters::Departure);
        let mut t = Text::builtin().pinned_to(FontStyle::Retro);
        let card = card(&facts(23, "Get the sword first"), NOW).unwrap();
        let (w, h) = (640u32, 480u32);
        crate::scaled_layout::set_step((w * 2, h * 2), (w, h));
        for (alpha, most) in [(255u8, 0u32), (128, 1)] {
            let page = |c: &mut Canvas| {
                c.rect(0, 0, c.w, c.h, 0x1b1712);
                c.rect(0, 600, c.w, 360, 0x6a4f2a);
            };
            let mut design = vec![0u32; (w * h * 4) as usize];
            {
                let mut c = Canvas::new(&mut design, w * 2, h * 2);
                page(&mut c);
                paint(&mut c, &mut t, &card, 900, alpha);
            }
            let mut old = vec![0u32; (w * h) as usize];
            crate::scaled_layout::ScaledFrame::default().downscale(&design, (w * 2, h * 2), &mut old, (w, h));
            let mut new = vec![0u32; (w * h) as usize];
            {
                let mut c = Canvas::screen(&mut new, (w, h), 2);
                page(&mut c);
                paint(&mut c, &mut t, &card, 900, alpha);
            }
            // The card's own edges round to the screen as a rectangle's do (an odd edge takes or leaves the whole screen
            // pixel instead of half of it); everywhere else the old picture within `most`.
            let cw = width(&mut t, &card, (w * 2) as i32);
            let ch = height(&mut t, &card, cw, crate::case_knows_art::scale());
            let (x0, y0) = (((w * 2) as i32 - cw) / 2, 900 - ch);
            let edge = |v: i32, a: i32, b: i32| (v - a / 2).abs() <= 1 || (v - (a + b) / 2).abs() <= 1;
            for (i, (a, b)) in old.iter().zip(&new).enumerate() {
                let (sx, sy) = ((i as u32 % w) as i32, (i as u32 / w) as i32);
                let d = [0, 8, 16].iter().map(|s| ((a >> s) & 0xff).abs_diff((b >> s) & 0xff)).max().unwrap();
                let on_edge = edge(sx, x0, cw) || edge(sy, y0, ch);
                assert!(d <= most || (alpha < 255 && on_edge), "alpha {alpha}: {d} steps apart at {sx},{sy} (card from {x0},{y0}, {cw}x{ch})");
            }
            assert!(new.iter().zip(&old).filter(|(a, _)| **a != 0x1b1712 && **a != 0x6a4f2a).count() > 1000, "alpha {alpha}: the card is there");
        }
        crate::scaled_layout::set_step((1240, 1080), (1240, 1080));
    }

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
        assert_eq!(c, Card { ago: "Last played 3 weeks ago".into(), played: Some("12 h 05 min played".into()), progress: None, art: None, note: None });
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
        let t = crate::testutil::clock::CpuInstant::now();
        for i in 0..200 {
            let (last, secs) = last_played(None, Some(&log), &format!("/roms/snes/{i}.sfc")).unwrap();
            let f = Facts { on: true, last_played: Some(last), play_secs: secs, note: "a note", ..Default::default() };
            assert!(card(&f, last + 30 * DAY).is_some());
        }
        // 200 starts' worth; one start takes a hundredth of this.
        assert!(t.elapsed() < std::time::Duration::from_millis(200), "{:?} for 200 starts", t.elapsed());
    }
}
