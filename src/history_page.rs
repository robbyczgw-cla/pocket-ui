//! Curator → History and the Year (or Month) in review: what a press does and how the pages look, painted from what
//! `play_history` worked out. Nothing here reads a file: the App (`app_history.rs`) opens the pages, gives them the
//! summary, system names, covers and the Echoes card, and acts on what a press asks for. The feature card's preview
//! paints the same functions with made-up history (`play_history::demo`).
//!
//! History: the period's title with Week · Month · Year beside it (L1/R1), one line of numbers, a quiet bar chart
//! of time per day (per week for a year), the most played games as spines (A opens a game's card) and, beside
//! them, the facts the period has: systems, what you leaned into, streaks, comebacks, completed games.
//! The review: full-screen cards, Left/Right between them, a dot per card; L1/R1 the review before or after.

use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::pad::Button;
use crate::palette::{pal, MARGIN, STRIPE};
use crate::play_history::{self as ph, Card, GameTime, Period, Review, Span, Summary};
use crate::status_corner::{self, Corner};
use crate::text::{Face, Text};
use crate::touch::Area;

const CONFIRM: Button = Button::East;
const CANCEL: Button = Button::South;
/// Games listed on the History page.
pub const ROWS: usize = 5;

/// A touch target a page painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Row(usize),
    Button(Button),
    /// That button pressed this many times: a tab two steps away goes straight there (Year from Week).
    Presses(Button, u8),
}

/// System names for a system id: (full, short), "Game Boy Advance", "GBA".
pub type Names<'a> = &'a dyn Fn(&str) -> (String, String);

fn stripe(system: &str) -> u32 {
    crate::bezel::system_colour(system)
}

// ---- History ---------------------------------------------------------------------------------------------------

/// What a press on History asks for.
#[derive(Debug, PartialEq)]
pub enum Out {
    None,
    Back,
    /// A: the card of this game (an entry of `summary.games`).
    Card(usize),
    /// Another period (L1/R1 another span, Left/Right the one before or after): work it out and `set` it.
    Period(Period),
    /// Y: this month's or year's review.
    Review,
}

#[derive(Clone, Debug)]
pub struct HistoryPage {
    /// The period asked for: its title shows at once.
    pub period: Period,
    /// The numbers, worked out on a thread (`app_history`): None until the first arrive; while another period is
    /// being counted, the last one's stay on screen (a few milliseconds) and A waits.
    pub summary: Option<Summary>,
    pub sel: usize,
    /// The local day now.
    pub today: i64,
}

impl HistoryPage {
    pub fn new(period: Period, today: i64) -> HistoryPage {
        HistoryPage { period, summary: None, sel: 0, today }
    }

    /// The numbers of a period arrived; false (and nothing changes) when another period was asked for since.
    pub fn set(&mut self, summary: Summary) -> bool {
        if summary.period != self.period {
            return false;
        }
        if self.summary.as_ref().is_none_or(|s| s.period != summary.period) {
            self.sel = 0;
        }
        self.summary = Some(summary);
        true
    }

    /// The numbers on screen are the ones of the period asked for.
    pub fn ready(&self) -> bool {
        self.summary.as_ref().is_some_and(|s| s.period == self.period)
    }

    fn rows(&self) -> usize {
        self.summary.as_ref().map_or(0, |s| s.games.len().min(ROWS))
    }

    /// Left goes back as far as the period the log began in; with nothing recorded, nowhere.
    pub fn can_go_back(&self) -> bool {
        self.summary.as_ref().and_then(|s| s.first_day).is_some_and(|d| self.period.first > d)
    }

    /// Right goes up to the period running now, never into the future.
    pub fn can_go_on(&self) -> bool {
        self.period.next().first <= self.today
    }

    /// The period of another span "around" this one: the running one when this one is running, else the one with
    /// this period's first day.
    pub fn span_period(&self, span: Span) -> Period {
        let p = self.period;
        Period::of(span, if p.contains(self.today) { self.today } else { p.first })
    }

    /// A press. Another period is asked for at once (`self.period`); the App counts it and `set`s it.
    pub fn press(&mut self, b: Button) -> Out {
        let p = self.period;
        let at = Span::ALL.iter().position(|s| *s == p.span).unwrap_or(0);
        let wanted = match b {
            Button::Up => {
                self.sel = self.sel.saturating_sub(1);
                None
            }
            Button::Down => {
                self.sel = (self.sel + 1).min(self.rows().saturating_sub(1));
                None
            }
            Button::L1 if at > 0 => Some(self.span_period(Span::ALL[at - 1])),
            Button::R1 if at + 1 < Span::ALL.len() => Some(self.span_period(Span::ALL[at + 1])),
            Button::Left if self.can_go_back() => Some(p.prev()),
            Button::Right if self.can_go_on() => Some(p.next()),
            b if b == CONFIRM && self.ready() && self.rows() > 0 => return Out::Card(self.sel),
            Button::West if p.span != Span::Week => return Out::Review,
            b if b == CANCEL => return Out::Back,
            _ => None,
        };
        match wanted {
            Some(next) => {
                self.period = next;
                Out::Period(next)
            }
            None => Out::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut h = Vec::new();
        if self.ready() && self.rows() > 0 {
            h.push(("A", "Details"));
        }
        h.push(("L1 R1", "Week, month, year"));
        if self.can_go_back() || self.can_go_on() {
            h.push(("< >", "Earlier, later"));
        }
        if self.period.span != Span::Week {
            h.push(("Y", "Review"));
        }
        h.push(("B", "Back"));
        h
    }
}

/// The line of numbers under the title.
pub fn numbers(s: &Summary, today: i64) -> String {
    if s.is_empty() {
        return match s.first_day {
            Some(d) if s.before_log() => format!("Not recorded: History counts from {}.", ph::date_year(d)),
            _ => format!("Nothing played {}.", s.period.phrase(today)),
        };
    }
    // A session whose end was never recorded has an unknown time, not "under a minute" (`ph::sessions_text`).
    let mut parts = Vec::new();
    if s.secs > 0 || s.untimed == 0 {
        parts.push(format!("{} played", ph::duration(s.secs)));
    }
    parts.push(ph::sessions_text(s.sessions, s.untimed, s.secs));
    parts.push(match s.period.span {
        Span::Week | Span::Month => format!("{} of {} days", s.days_played, s.period.days),
        Span::Year => ph::count(s.days_played as usize, "day", "days"),
    });
    parts.join(" · ")
}

/// The line in the empty chart of a period without play.
pub fn empty_chart(s: &Summary) -> &'static str {
    match s.first_day {
        None => "Games you play show up here, day by day.",
        Some(_) if s.before_log() => "Play from before then isn't in History.",
        Some(_) => "No sessions in these days.",
    }
}

/// A round top for the chart's scale, in seconds: the smallest step at or above the highest bar.
fn scale_top(max: u64) -> u64 {
    const MINUTES: [u64; 18] = [15, 30, 60, 120, 180, 240, 360, 480, 720, 960, 1440, 2160, 2880, 4320, 5760, 7200, 8640, 10_080];
    MINUTES.iter().map(|m| m * 60).find(|s| *s >= max).unwrap_or(max.max(1))
}

fn scale_label(secs: u64) -> String {
    if secs < 3600 {
        format!("{} min", secs / 60)
    } else {
        format!("{} h", secs / 3600)
    }
}

/// The bar chart: time per bar, the scale's top and zero at the left, the days (or months) under it.
pub fn paint_chart(c: &mut Canvas, text: &mut Text, s: &Summary, today: i64, (x, y, w, h): (i32, i32, i32, i32)) {
    let p = pal();
    let grid = p.pick(0x26262a, 0xdcd6ca);
    let n = s.bars.len().max(1) as i32;
    let max = s.bars.iter().map(|b| b.secs).max().unwrap_or(0);
    let top = scale_top(max);
    c.rect(x, y + h, w, 2, grid);
    c.rect(x, y, w, 1, grid);
    if max > 0 {
        // No scale for a chart without a known time (only sessions without an end): nothing to measure.
        text.draw_right(c, Face::Body, 21, x - 14, y + 8, p.dust, &scale_label(top));
    }
    text.draw_right(c, Face::Body, 21, x - 14, y + h + 7, p.dust, "0");
    let slot = w as f32 / n as f32;
    let bar_w = ((slot * 0.62).round() as i32).max(3);
    for (i, b) in s.bars.iter().enumerate() {
        let bx = x + (slot * i as f32 + (slot - bar_w as f32) / 2.0).round() as i32;
        let now = (b.first..b.first + b.days).contains(&today);
        if b.first > today || s.first_day.is_none_or(|d| b.first + b.days <= d) {
            // Days still to come, or from before the log began (not recorded, not "no play"): nothing to mark.
            continue;
        }
        if b.secs == 0 && b.untimed > 0 {
            // Played, but no end was recorded (pocket-ui or the Pocket stopped): a ring over the line, no height to
            // pretend a time.
            let r = (bar_w / 4).clamp(4, 9);
            let (cx, cy) = (bx + bar_w / 2, y + h - r - 8);
            c.disc(cx, cy, r, if now { p.chalk } else { mix(p.dust, p.chalk, 70) });
            c.disc(cx, cy, (r - 3).max(1), p.void);
            continue;
        }
        if b.secs == 0 {
            // A day without play: a small mark on the line, so the days stay countable.
            c.rect(bx, y + h - 4, bar_w, 4, mix(p.spine, p.dust, 60));
            continue;
        }
        let bh = ((b.secs as f64 / top as f64) * h as f64).round().max(3.0) as i32;
        let colour = if now { p.chalk } else { mix(p.dust, p.chalk, 70) };
        c.rect(bx, y + h - bh, bar_w, bh, colour);
    }
    // Under the bars: the weekdays, every seventh day of a month, the months of a year.
    let label_y = y + h + 36;
    let label_at = |i: usize, label: &str, text: &mut Text, c: &mut Canvas| {
        let cx = x + (slot * (i as f32 + 0.5)).round() as i32;
        let lw = text.width(Face::Body, 21, label);
        text.draw(c, Face::Body, 21, (cx - lw / 2).clamp(x - 8, x + w - lw), label_y, p.dust, label);
    };
    match s.period.span {
        Span::Week => {
            for (i, d) in ph::WEEKDAYS.iter().enumerate() {
                label_at(i, d, text, c);
            }
        }
        Span::Month => {
            for i in (0..s.bars.len()).step_by(7) {
                label_at(i, &(i + 1).to_string(), text, c);
            }
        }
        Span::Year => {
            // Each month under the week its first day falls in.
            for (i, b) in s.bars.iter().enumerate() {
                if let Some(first) = (b.first..b.first + b.days).find(|d| ph::civil(*d).2 == 1) {
                    label_at(i, &ph::MONTHS[ph::civil(first).1 as usize - 1][..3], text, c);
                }
            }
        }
    }
}

/// The streak fact: the period's longest run of days and the one running now. When the longest is the one running
/// (it began before the period, so the period sees only its end), one figure: the run as it is.
pub fn streak_line(s: &Summary, today: i64) -> Option<String> {
    let longest = s.longest_streak.filter(|st| st.days >= 2);
    let running = (s.current_streak >= 2).then_some(s.current_streak);
    let up_to = |st: &ph::Streak| if st.to == today { "up to today" } else { "up to yesterday" };
    match (longest, running) {
        (Some(st), Some(n)) if st.to >= today - 1 => Some(format!("{n} days in a row, {}", up_to(&st))),
        (Some(st), Some(n)) => Some(format!("Longest {} days; now {n} days in a row", st.days)),
        (Some(st), None) => Some(format!("Longest {} days in a row", st.days)),
        (None, Some(n)) => Some(format!("{n} days in a row")),
        (None, None) => None,
    }
}

/// The facts beside the games, as (heading, line) pairs, the most telling first.
pub fn facts(s: &Summary, today: i64, names: Names) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // First what the numbers leave out: it qualifies all of them, so it is never the line that doesn't fit.
    if let Some(d) = s.counting_since {
        let line = if s.set_aside {
            format!("{}: an older history file could not be read", ph::date(d))
        } else {
            format!("{}: play before that is not in these numbers", ph::date(d))
        };
        out.push(("Counted since".to_string(), line));
    }
    if s.damaged {
        out.push(("Not counted".to_string(), "Part of the history file could not be read".to_string()));
    }
    if !s.systems.is_empty() {
        let line = s.systems.iter().take(3).map(|x| format!("{} {}", names(&x.system).1, ph::time_text(x.secs, x.untimed, false))).collect::<Vec<_>>().join(" · ");
        out.push(("Systems".to_string(), line));
    }
    if !s.leaned_into.is_empty() {
        out.push(("Leaned into".to_string(), crate::curator::features::capitalise(&s.leaned_into)));
    }
    out.extend(streak_line(s, today).map(|l| ("Streak".to_string(), l)));
    if let Some(cb) = s.comebacks.first() {
        out.push(("Back after a long break".to_string(), format!("{}, after {}", cb.game.name, ph::gap(cb.gap_days))));
    }
    if !s.completed.is_empty() {
        let names: Vec<&str> = s.completed.iter().map(|(g, _)| g.name.as_str()).collect();
        out.push(("Completed".to_string(), names.join(" · ")));
    }
    if s.new_games > 0 {
        out.push(("New to you".to_string(), format!("{} played for the first time", ph::count(s.new_games as usize, "game", "games"))));
    }
    out
}

/// A spine or, selected, the paper label; returns the (text, secondary) colours.
fn row_back(c: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), stripe_colour: u32, selected: bool) -> (u32, u32) {
    let p = pal();
    c.rect(x, y, w, h, if selected { p.label } else { p.spine });
    if selected {
        crate::game_card::lift_shadow(c, x, y + h, w, p.t);
    }
    c.rect(x, y, STRIPE, h, if selected { stripe_colour } else { mix(stripe_colour, p.spine, 150) });
    if selected {
        (p.ink, p.label_dust)
    } else {
        (p.chalk, p.dust)
    }
}

/// The top line: the page's place, left (cut short before the corner); the status corner, right.
fn top_line(c: &mut Canvas, text: &mut Text, place: &str, corner: Option<&Corner>) {
    let right = c.w - MARGIN;
    let left = corner.map_or(right, |k| k.paint(c, text, right, 72, pal().dust));
    let place = text.fit(Face::Body, 28, place, status_corner::room_left_of(left, right) - MARGIN);
    text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, &place);
}

/// Paints History; returns its touch targets. `corner`: the status corner for the top right.
pub fn paint_history(c: &mut Canvas, text: &mut Text, page: &HistoryPage, names: Names, corner: Option<&Corner>) -> Vec<(Area, Hit)> {
    let p = pal();
    let period = page.period;
    let mut hits = Vec::new();
    c.clear(p.void);
    top_line(c, text, "Curator · History", corner);
    // Week · Month · Year, right; the selected one in chalk with a rule under it.
    let mut tx = c.w - MARGIN;
    let at = Span::ALL.iter().position(|x| *x == period.span).unwrap_or(0);
    for (i, span) in Span::ALL.iter().enumerate().rev() {
        let lw = text.width(Face::Body, 28, span.label());
        tx -= lw;
        let on = *span == period.span;
        text.draw(c, Face::Body, 28, tx, 156, if on { p.chalk } else { p.dust }, span.label());
        if on {
            c.rect(tx, 168, lw, 3, p.chalk);
        } else {
            // As many steps as the tab is away: a tap on Year from Week opens Year, not Month.
            hits.push((Area::new(tx - 12, 112, lw + 24, 72), Hit::Presses(if i < at { Button::L1 } else { Button::R1 }, at.abs_diff(i) as u8)));
        }
        tx -= 36;
    }
    // The title is the period asked for at once; the numbers follow from the thread a moment later.
    let title = text.fit(Face::Title, 67, &period.label(page.today), tx - MARGIN - 24);
    text.draw(c, Face::Title, 67, MARGIN, 160, p.chalk, &title);
    // On a screen with fewer than 1080 lines (the Mini V1's 960) the chart gives up most of the difference, so the
    // five most played games still end above the hints (docs/DEVICES.md).
    let short = (1080 - c.h).max(0) * 2 / 3;
    let (cx, cy, cw, ch) = (MARGIN + 72, 256, c.w - 2 * MARGIN - 72, 196 - short);
    let Some(s) = page.summary.as_ref() else {
        text.draw(c, Face::Body, 28, MARGIN, 208, p.dust, "Counting…");
        c.rect(cx, cy + ch, cw, 2, p.pick(0x26262a, 0xdcd6ca));
        return hits;
    };
    let line = text.fit(Face::Body, 28, &numbers(s, page.today), c.w - 2 * MARGIN);
    text.draw(c, Face::Body, 28, MARGIN, 208, p.dust, &line);

    if s.is_empty() {
        c.rect(cx, cy + ch, cw, 2, p.pick(0x26262a, 0xdcd6ca));
        let msg = empty_chart(s);
        let lw = text.width(Face::Body, 28, msg);
        // An open blank notebook above the words, inside the chart's box (`ui_art`).
        let base = cy + ch / 2 + crate::type_scale::base(10, 28);
        let mut band = c.rows(0, cy + ch);
        let dy = crate::ui_art::above_words(&mut band, crate::ui_art::Art::History, cx + cw / 2, true, cy + 4, base, 28);
        text.draw(c, Face::Body, 28, cx + (cw - lw) / 2, (base + dy).min(cy + ch - 12), p.dust, msg);
    } else {
        paint_chart(c, text, s, page.today, (cx, cy, cw, ch));
    }
    for (b, x0, x1) in [(Button::Left, 0, c.w / 2), (Button::Right, c.w / 2, c.w)] {
        hits.push((Area::new(x0, cy, x1 - x0, ch + 50), Hit::Button(b)));
    }

    // The most played games, left.
    let (lx, lw) = (MARGIN, 620);
    let top = 548 - short;
    if !s.games.is_empty() {
        text.draw(c, Face::Body, 21, lx, top, p.dust, "Most played");
        let (row, gap) = (crate::type_scale::row(64, 37), 8);
        for (i, g) in s.games.iter().take(ROWS).enumerate() {
            let y = top + 20 + i as i32 * (row + gap);
            let selected = i == page.sel;
            let (fg, dim) = row_back(c, (lx, y, lw, row), stripe(&g.system), selected);
            let side = format!("{} · {}", names(&g.system).1, ph::time_text(g.secs, g.untimed, false));
            let sw = text.width(Face::Body, 24, &side);
            // A game without a name in the log (an old file's record): its file name.
            let fallback;
            let name = if g.name.is_empty() {
                fallback = std::path::Path::new(&g.path).file_stem().map_or_else(|| "Unknown game".to_string(), |n| n.to_string_lossy().into_owned());
                fallback.as_str()
            } else {
                g.name.as_str()
            };
            let name = text.fit(Face::Title, 37, name, lw - STRIPE - 24 - sw - 40);
            text.draw(c, Face::Title, 37, lx + STRIPE + 20, y + row / 2 + crate::type_scale::base(13, 37), fg, &name);
            text.draw_right(c, Face::Body, 24, lx + lw - 20, y + row / 2 + crate::type_scale::base(9, 24), dim, &side);
            hits.push((Area::new(lx, y, lw, row), Hit::Row(i)));
        }
    }
    // The facts, right (or across, when nothing was played).
    let fx = if s.games.is_empty() { MARGIN } else { lx + lw + 44 };
    let fw = c.w - MARGIN - fx;
    let mut y = top;
    for (head, line) in facts(s, page.today, names) {
        let lines = text.wrap(Face::Body, 28, &line, fw);
        let step = crate::type_scale::line(28, 10);
        let need = 30 + lines.len().min(2) as i32 * step;
        if y + need > c.h - 120 {
            break;
        }
        text.draw(c, Face::Body, 21, fx, y, p.dust, &head);
        for (k, l) in lines.iter().take(2).enumerate() {
            text.draw(c, Face::Body, 28, fx, y + step + k as i32 * step, p.chalk, l);
        }
        y += need + 26;
    }
    hits
}

// ---- the review --------------------------------------------------------------------------------------------------

/// What a press on the review asks for.
#[derive(Debug, PartialEq)]
pub enum ReviewOut {
    None,
    Back,
    /// A on a card about one game: its card.
    Card(GameTime),
    /// L1/R1: the review of the period before or after.
    Period(Period),
}

#[derive(Clone, Debug)]
pub struct ReviewPage {
    /// The year or month asked for.
    pub period: Period,
    /// Its cards, worked out on a thread (`app_history`); the last period's while another is counted.
    pub review: Option<Review>,
    pub card: usize,
    pub today: i64,
}

/// The game a card is about, if it is about one.
pub fn card_game(card: &Card) -> Option<GameTime> {
    match card {
        Card::TopGame { game, .. } => Some(game.clone()),
        Card::First { moment } | Card::Longest { moment } => Some(moment.game.clone()),
        Card::CameBack { comeback } => Some(comeback.game.clone()),
        Card::Completed { games } if games.len() == 1 => Some(games[0].0.clone()),
        Card::Next { name, system, path, .. } => Some(GameTime { name: name.clone(), system: system.clone(), path: path.clone(), ..Default::default() }),
        _ => None,
    }
}

impl ReviewPage {
    pub fn new(period: Period, today: i64) -> ReviewPage {
        ReviewPage { period, review: None, card: 0, today }
    }

    /// The cards of a period arrived; false when another period was asked for since.
    pub fn set(&mut self, review: Review) -> bool {
        if review.period != self.period {
            return false;
        }
        self.review = Some(review);
        self.card = 0;
        true
    }

    pub fn ready(&self) -> bool {
        self.review.as_ref().is_some_and(|r| r.period == self.period)
    }

    /// The card on screen, once its period's cards are there.
    pub fn current(&self) -> Option<&Card> {
        self.review.as_ref().filter(|_| self.ready()).and_then(|r| r.cards.get(self.card))
    }

    /// L1 goes back as far as the year (or month) the log began in.
    pub fn can_go_back(&self) -> bool {
        self.review.as_ref().and_then(|r| r.summary.first_day).is_some_and(|d| self.period.first > d)
    }

    pub fn can_go_on(&self) -> bool {
        self.period.next().first <= self.today
    }

    pub fn press(&mut self, b: Button) -> ReviewOut {
        let n = self.review.as_ref().filter(|_| self.ready()).map_or(0, |r| r.cards.len());
        let p = self.period;
        let wanted = match b {
            Button::Left => {
                self.card = self.card.saturating_sub(1);
                None
            }
            Button::Right => {
                self.card = (self.card + 1).min(n.saturating_sub(1));
                None
            }
            Button::L1 if self.can_go_back() => Some(p.prev()),
            Button::R1 if self.can_go_on() => Some(p.next()),
            b if b == CONFIRM => return self.current().and_then(card_game).map_or(ReviewOut::None, ReviewOut::Card),
            b if b == CANCEL => return ReviewOut::Back,
            _ => None,
        };
        match wanted {
            Some(next) => {
                self.period = next;
                ReviewOut::Period(next)
            }
            None => ReviewOut::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut h = Vec::new();
        if self.review.as_ref().is_some_and(|r| r.cards.len() > 1) {
            h.push(("< >", "Cards"));
        }
        if self.current().and_then(card_game).is_some() {
            h.push(("A", "Details"));
        }
        if self.can_go_back() || self.can_go_on() {
            h.push(("L1 R1", if self.period.span == Span::Year { "Year" } else { "Month" }));
        }
        h.push(("B", "Back"));
        h
    }
}

/// The words of a card: (kicker, big line, detail, quiet line, the system its colour comes from). `today`: the
/// local day now.
pub fn card_words(r: &Review, card: &Card, today: i64, names: Names) -> (String, String, String, String, Option<String>) {
    let year_or_month = match r.period.span {
        Span::Year => r.period.year().to_string(),
        _ => ph::MONTHS[ph::civil(r.period.first).1 as usize - 1].to_string(),
    };
    let when = format!("in {year_or_month}");
    let s = &r.summary;
    match card {
        Card::Overview { secs, sessions, untimed, days, games, counting_since } => {
            if s.is_empty() {
                return match s.first_day {
                    Some(d) if s.before_log() => (format!("Your {year_or_month}"), "Not recorded".into(), format!("History counts from {}.", ph::date_year(d)), "Play from before then isn't in History.".into(), None),
                    _ if r.period.contains(today) => (format!("Your {year_or_month}"), "Nothing yet".into(), format!("Nothing played {when} so far."), "Games you play show up here.".into(), None),
                    _ => (format!("Your {year_or_month}"), "Nothing played".into(), format!("No sessions {when}."), "Games you play show up here.".into(), None),
                };
            }
            // No time known at all (every session ended without a recorded end): the sessions are the big figure.
            let (big, first) = if *secs == 0 && *untimed > 0 {
                (ph::count(*sessions as usize, "session", "sessions"), "time not recorded".to_string())
            } else {
                (ph::duration(*secs), ph::sessions_text(*sessions, *untimed, *secs))
            };
            let detail = [first, ph::count(*days as usize, "day", "days"), ph::count(*games as usize, "game", "games")].join(" · ");
            let mut quiet = Vec::new();
            if let Some(d) = counting_since {
                quiet.push(format!("Counted since {}: play before that is not in these numbers.", ph::date(*d)));
            }
            if *untimed > 0 {
                quiet.push("Without time: pocket-ui or the Pocket stopped while the game ran.".to_string());
            }
            (format!("Your {year_or_month}"), big, detail, quiet.join(" "), None)
        }
        Card::TopGame { game, percent } => (
            "Most played".into(),
            game.name.clone(),
            format!("{} · {}", ph::duration(game.secs), ph::sessions_text(game.sessions, game.untimed, game.secs)),
            format!("{percent}% of your time · {}", names(&game.system).0),
            Some(game.system.clone()),
        ),
        Card::TopSystem { system, secs, untimed, games, percent, only } => (
            "Most played console".into(),
            names(system).0,
            format!("{} · {}", ph::time_text(*secs, *untimed, true), ph::count(*games as usize, "game", "games")),
            if *only { "All of your time, on this one.".to_string() } else { format!("{percent}% of your time") },
            Some(system.clone()),
        ),
        Card::Genres { line, genres } => {
            let detail = genres.iter().map(|g| format!("{} {}", g.label, ph::duration(g.secs))).collect::<Vec<_>>().join(" · ");
            ("What you leaned into".into(), crate::curator::features::capitalise(line), detail, "From the tags of the games you played, by time".into(), None)
        }
        Card::First { moment } => {
            let local = moment.start as i64 + moment.tz as i64;
            let (h, m) = (local.rem_euclid(ph::DAY) / 3600, local.rem_euclid(3600) / 60);
            let kicker = match r.period.span {
                Span::Year => format!("First game of {year_or_month}"),
                _ => format!("First game in {year_or_month}"),
            };
            (kicker, moment.game.name.clone(), format!("{}, {h:02}:{m:02}", ph::date(moment.day)), names(&moment.game.system).0, Some(moment.game.system.clone()))
        }
        Card::Longest { moment } => (
            "Longest session".into(),
            moment.game.name.clone(),
            ph::duration(moment.secs),
            format!("On {} · {}", ph::date(moment.day), names(&moment.game.system).0),
            Some(moment.game.system.clone()),
        ),
        Card::CameBack { comeback } => (
            "The game you came back to".into(),
            comeback.game.name.clone(),
            format!("After {} away", ph::gap(comeback.gap_days)),
            format!("Back on {} · {}", ph::date(comeback.day), names(&comeback.game.system).0),
            Some(comeback.game.system.clone()),
        ),
        Card::Completed { games } => {
            let big = if games.len() == 1 { games[0].0.name.clone() } else { ph::count(games.len(), "game", "games") };
            let detail = if games.len() == 1 { format!("On {}", ph::date(games[0].1)) } else { games.iter().map(|(g, _)| g.name.as_str()).collect::<Vec<_>>().join(" · ") };
            (format!("Completed {when}"), big, detail, "As marked on the game card".into(), games.first().filter(|_| games.len() == 1).map(|g| g.0.system.clone()))
        }
        Card::Streak { streak } => (
            "Longest streak".into(),
            format!("{} days in a row", streak.days),
            format!("{} to {}", ph::date(streak.from), ph::date(streak.to)),
            String::new(),
            None,
        ),
        Card::Next { name, system, reason, .. } => ("You might like next".into(), name.clone(), crate::curator::features::capitalise(reason), format!("From Echoes · {}", names(system).0), Some(system.clone())),
    }
}

/// The picture a review card shows (`ui_art`): a sticker for a milestone (a game completed, 100 hours, ten games on
/// one console, a week in a row, a game come back to), the blank notebook for a period with nothing played.
pub fn card_art(r: &Review, card: &Card) -> Option<crate::ui_art::Art> {
    use crate::ui_art::{Art, Sticker};
    match card {
        Card::Overview { .. } if r.summary.is_empty() => Some(Art::History),
        Card::Overview { secs, .. } if *secs >= 100 * 3600 => Some(Art::Sticker(Sticker::HundredHours)),
        Card::Completed { games } if !games.is_empty() => Some(Art::Sticker(Sticker::Completed)),
        Card::TopSystem { games, .. } if *games >= 10 => Some(Art::Sticker(Sticker::TenGames)),
        Card::Streak { streak } if streak.days >= 7 => Some(Art::Sticker(Sticker::Streak)),
        Card::CameBack { .. } => Some(Art::Sticker(Sticker::CameBack)),
        _ => None,
    }
}

/// Paints the review's current card; `cover`: the box art of its game, if any.
pub fn paint_review(c: &mut Canvas, text: &mut Text, page: &ReviewPage, names: Names, corner: Option<&Corner>, cover: Option<&Image>) -> Vec<(Area, Hit)> {
    let p = pal();
    c.clear(p.void);
    // The title is the period asked for at once; its cards follow from the thread a moment later.
    let title = page.period.label(page.today);
    let place = match page.period.span {
        Span::Year => format!("Year in review · {title}"),
        _ => format!("Month in review · {title}"),
    };
    let header = |c: &mut Canvas, text: &mut Text| top_line(c, text, &place, corner);
    let Some((r, card)) = page.review.as_ref().and_then(|r| r.cards.get(page.card).map(|c| (r, c))) else {
        c.rect(0, 0, 24, c.h, mix(p.dust, p.void, 90));
        header(c, text);
        text.draw(c, Face::Body, 28, MARGIN, 330, p.dust, "Counting…");
        return Vec::new();
    };
    let (kicker, big, detail, quiet, system) = card_words(r, card, page.today, names);
    // The card's colour runs down the left edge: its game's system, else the quiet colour.
    let edge = system.as_deref().map_or(mix(p.dust, p.void, 90), stripe);
    c.rect(0, 0, 24, c.h, edge);
    header(c, text);
    let mut right = c.w - MARGIN;
    let art = card_art(r, card);
    let k = crate::ui_art::empty_scale(c.h);
    if let Some(img) = cover {
        let (w, h) = img.fit(340, 460);
        let (x, y) = (c.w - MARGIN - w, 300 + (460 - h) / 2);
        c.blit_scaled(img, x, y, w, h);
        right = x - 48;
        // A milestone's sticker on the box's top left corner, as if stuck on it.
        if let Some(a) = art {
            let (aw, ah) = crate::ui_art::size(a, k);
            crate::ui_art::paint(c, a, x - aw / 3, y - ah / 3, k);
            right = x - aw / 3 - 32;
        }
    } else if let Some(a) = art {
        // No box art: the sticker (or the empty notebook) stands where it would be.
        let (aw, ah) = crate::ui_art::size(a, k);
        crate::ui_art::paint(c, a, c.w - MARGIN - 40 - aw, 300 + (460 - ah) / 2, k);
        right = c.w - MARGIN - 40 - aw - 48;
    }
    let width = right - MARGIN;
    text.draw(c, Face::Body, 28, MARGIN, 330, p.dust, &kicker);
    let mut y = 330 + 110;
    let big_px = if text.width(Face::Title, 96, &big) <= width { 96 } else { 67 };
    for line in text.wrap(Face::Title, big_px, &big, width).iter().take(2) {
        let line = text.fit(Face::Title, big_px, line, width);
        text.draw(c, Face::Title, big_px, MARGIN, y, p.chalk, &line);
        y += big_px as i32 + 14;
    }
    y += 24;
    for line in text.wrap(Face::Title, 37, &detail, width).iter().take(3) {
        text.draw(c, Face::Title, 37, MARGIN, y, p.chalk, line);
        y += crate::type_scale::line(37, 13);
    }
    y += 10;
    for line in text.wrap(Face::Body, 28, &quiet, width).iter().take(2) {
        text.draw(c, Face::Body, 28, MARGIN, y, p.dust, line);
        y += crate::type_scale::line(28, 12);
    }
    // A dot per card.
    let n = r.cards.len() as i32;
    let (dot, gap) = (12, 14);
    let total = n * dot + (n - 1).max(0) * gap;
    let mut dx = (c.w - total) / 2;
    for i in 0..n {
        let on = i as usize == page.card;
        c.disc(dx + dot / 2, c.h - 150, dot / 2, if on { p.chalk } else { mix(p.dust, p.void, 120) });
        dx += dot + gap;
    }
    vec![(Area::new(0, 100, c.w / 3, c.h - 220), Hit::Button(Button::Left)), (Area::new(c.w * 2 / 3, 100, c.w / 3, c.h - 220), Hit::Button(Button::Right)), (Area::new(c.w / 3, 260, c.w / 3, 500), Hit::Button(CONFIRM))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_history::{day_of, review, summarise, Plain};
    use crate::play_log::{build, Game, Log, Session};

    fn demo_log(today: i64) -> Log {
        crate::play_history::demo(&crate::play_history::demo_games(), today, 3600)
    }

    /// A page as the App keeps it: asked for, then the thread's numbers set.
    fn page(log: &Log, span: Span, today: i64) -> HistoryPage {
        let mut p = HistoryPage::new(Period::of(span, today), today);
        assert!(p.set(summarise(log, p.period, today, &Plain)));
        p
    }

    #[test]
    fn history_moves_through_games_spans_and_periods() {
        let today = day_of(2026, 10, 23);
        let log = demo_log(today);
        let mut p = page(&log, Span::Month, today);
        assert!(p.summary.as_ref().unwrap().games.len() >= ROWS);
        for _ in 0..10 {
            p.press(Button::Down);
        }
        assert_eq!(p.sel, ROWS - 1, "the list stops at its last row");
        assert_eq!(p.press(CONFIRM), Out::Card(ROWS - 1));
        assert_eq!(p.press(Button::R1), Out::Period(Period::of(Span::Year, today)));
        assert_eq!(p.period, Period::of(Span::Year, today), "the period asked for is the page's at once");
        assert!(!p.ready());
        assert_eq!(p.press(CONFIRM), Out::None, "A waits for the numbers");
        assert!(!p.set(summarise(&log, Period::of(Span::Month, today), today, &Plain)), "numbers of a period no longer asked for are dropped");
        assert_eq!(p.press(Button::L1), Out::Period(Period::of(Span::Month, today)));
        assert_eq!(p.press(Button::L1), Out::Period(Period::of(Span::Week, today)));
        assert_eq!(p.press(Button::Right), Out::None, "no week after this one yet");
        p.set(summarise(&log, p.period, today, &Plain));
        assert_eq!(p.press(Button::R1), Out::Period(Period::of(Span::Month, today)));
        p.set(summarise(&log, p.period, today, &Plain));
        assert_eq!(p.press(Button::Left), Out::Period(Period::of(Span::Month, today).prev()));
        p.set(summarise(&log, p.period, today, &Plain));
        assert_eq!(p.sel, 0);
        assert_eq!(p.press(Button::West), Out::Review);
        assert_eq!(p.press(CANCEL), Out::Back);
        // A past month: L1 takes the week it began with, Right the month after.
        assert_eq!(p.press(Button::Right), Out::Period(Period::of(Span::Month, today)));
        p.set(summarise(&log, p.period, today, &Plain));
        p.press(Button::Left);
        p.set(summarise(&log, p.period, today, &Plain));
        assert_eq!(p.press(Button::L1), Out::Period(Period::of(Span::Week, day_of(2026, 9, 1))));
        // A week has no review.
        let mut w = page(&log, Span::Week, today);
        assert_eq!(w.press(Button::West), Out::None);
        assert!(!w.hints().iter().any(|h| h.0 == "Y"));
    }

    #[test]
    fn left_stops_at_the_period_the_log_began_in() {
        let today = day_of(2026, 10, 23);
        let games = vec![Game { path: "/storage/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into() }];
        let start = (day_of(2026, 9, 20) * ph::DAY + 18 * 3600) as u64;
        let log = build(games, vec![Session { game: 0, start, tz: 0, secs: 1800, ended: true, emulator: String::new(), ..Default::default() }], Vec::new(), Vec::new());
        let mut p = page(&log, Span::Month, today);
        assert_eq!(p.press(Button::Left), Out::Period(Period::of(Span::Month, day_of(2026, 9, 1))));
        p.set(summarise(&log, p.period, today, &Plain));
        assert_eq!(p.press(Button::Left), Out::None, "September is where it began");
        assert!(p.hints().iter().any(|h| h.0 == "< >"), "Right still goes on");
        // Nothing recorded at all: neither way, and the hint says so by not being there.
        let empty = Log::default();
        let mut e = page(&empty, Span::Week, today);
        assert_eq!((e.press(Button::Left), e.press(Button::Right)), (Out::None, Out::None));
        assert!(!e.hints().iter().any(|h| h.0 == "< >" || h.0 == "A"));
    }

    #[test]
    fn missing_data_says_what_it_is() {
        let today = day_of(2026, 10, 23);
        let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
        // An empty log: the first start, or Play history just switched on.
        let empty = summarise(&Log::default(), Period::of(Span::Week, today), today, &Plain);
        assert_eq!(numbers(&empty, today), "Nothing played this week.");
        assert_eq!(empty_chart(&empty), "Games you play show up here, day by day.");
        // A month before the log began (the update with History came in September).
        let games = vec![Game { path: "/storage/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into() }];
        let start = (day_of(2026, 9, 20) * ph::DAY + 18 * 3600) as u64;
        let mut log = build(games, vec![Session { game: 0, start, tz: 0, secs: 1800, ended: true, emulator: String::new(), ..Default::default() }], Vec::new(), Vec::new());
        let before = summarise(&log, Period::of(Span::Month, day_of(2026, 3, 1)), today, &Plain);
        assert_eq!(numbers(&before, today), "Not recorded: History counts from 20 September 2026.");
        assert_eq!(empty_chart(&before), "Play from before then isn't in History.");
        // A quiet week after it began.
        let quiet = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!(numbers(&quiet, today), "Nothing played this week.");
        assert_eq!(empty_chart(&quiet), "No sessions in these days.");
        // A damaged file: said, not hidden.
        log.damaged = 2;
        let month = summarise(&log, Period::of(Span::Month, day_of(2026, 9, 1)), today, &Plain);
        assert!(facts(&month, today, &names).iter().any(|(h, l)| h == "Not counted" && l.contains("could not be read")));
        log.damaged = 0;
        log.set_aside = true;
        let month = summarise(&log, Period::of(Span::Month, day_of(2026, 9, 1)), today, &Plain);
        assert!(facts(&month, today, &names).iter().any(|(h, l)| h == "Counted since" && l == "20 September: an older history file could not be read"));
        // The review says the same.
        let r = review(&log, Period::of(Span::Year, day_of(2025, 6, 1)), today, &Plain);
        let (_, big, detail, ..) = card_words(&r, &r.cards[0], today, &names);
        assert_eq!((big.as_str(), detail.as_str()), ("Not recorded", "History counts from 20 September 2026."));
        let r = review(&Log::default(), Period::of(Span::Year, today), today, &Plain);
        let (_, big, detail, ..) = card_words(&r, &r.cards[0], today, &names);
        assert_eq!((big.as_str(), detail.as_str()), ("Nothing yet", "Nothing played in 2026 so far."));
    }

    #[test]
    fn every_state_paints_something() {
        let today = day_of(2026, 10, 23);
        let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
        let mut text = Text::new();
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        // Before the numbers came: the title and "Counting…", no rows to touch.
        let waiting = HistoryPage::new(Period::of(Span::Week, today), today);
        let hits = paint_history(&mut c, &mut text, &waiting, &names, None);
        assert!(!hits.iter().any(|(_, h)| matches!(h, Hit::Row(_))));
        let review_waiting = ReviewPage::new(Period::of(Span::Year, today), today);
        assert!(paint_review(&mut c, &mut text, &review_waiting, &names, None, None).is_empty());
        assert_eq!(review_waiting.hints(), [("B", "Back")]);
        // A game whose name the log lost: its file name.
        let games = vec![Game { path: "/storage/roms/gb/Lost Game (Europe).gb".into(), name: String::new(), system: "gb".into() }];
        let start = (today * ph::DAY + 10 * 3600) as u64;
        let log = build(games, vec![Session { game: 0, start, tz: 0, secs: 1800, ended: true, emulator: String::new(), ..Default::default() }], Vec::new(), Vec::new());
        let p = page(&log, Span::Week, today);
        let hits = paint_history(&mut c, &mut text, &p, &names, Some(&crate::status_corner::Corner::battery_only("50%")));
        assert!(hits.iter().any(|(_, h)| *h == Hit::Row(0)));
    }

    /// Review SOL-2026-10-24 #15: a tap on a tab opens that span, also the one two steps away.
    #[test]
    fn a_tap_on_a_tab_opens_that_span() {
        let today = day_of(2026, 10, 23);
        let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
        let mut text = Text::new();
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        let log = demo_log(today);
        for from in Span::ALL {
            let p = page(&log, from, today);
            let mut reached = Vec::new();
            for (_, hit) in paint_history(&mut c, &mut text, &p, &names, None) {
                let (b, n) = match hit {
                    Hit::Presses(b @ (Button::L1 | Button::R1), n) => (b, n),
                    Hit::Button(b @ (Button::L1 | Button::R1)) => (b, 1),
                    _ => continue,
                };
                let mut tapped = p.clone();
                for _ in 0..n {
                    tapped.press(b);
                }
                reached.push(tapped.period.span);
            }
            reached.sort_by_key(|s| s.label());
            let mut others: Vec<Span> = Span::ALL.into_iter().filter(|s| *s != from).collect();
            others.sort_by_key(|s| s.label());
            assert_eq!(reached, others, "from {from:?} each other tab opens its own span");
        }
    }

    #[test]
    fn the_review_goes_card_by_card_and_names_its_games() {
        let today = day_of(2026, 12, 20);
        let log = demo_log(today);
        let mut p = ReviewPage::new(Period::of(Span::Year, today), today);
        assert!(p.set(review(&log, p.period, today, &Plain)));
        let n = p.review.as_ref().unwrap().cards.len();
        assert!(n >= 6, "{n} cards");
        assert_eq!(p.press(CONFIRM), ReviewOut::None, "the overview is about no one game");
        p.press(Button::Right);
        assert!(matches!(p.press(CONFIRM), ReviewOut::Card(g) if !g.path.is_empty()), "the top game opens its card");
        for _ in 0..20 {
            p.press(Button::Right);
        }
        assert_eq!(p.card, n - 1);
        assert_eq!(p.press(Button::R1), ReviewOut::None, "no review of next year yet");
        assert_eq!(p.press(Button::L1), ReviewOut::Period(Period::of(Span::Year, today).prev()));
        assert_eq!(p.press(CONFIRM), ReviewOut::None, "A waits for the year's cards");
        let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
        let r = p.review.clone().unwrap();
        for card in &r.cards {
            let (kicker, big, ..) = card_words(&r, card, today, &names);
            assert!(!kicker.is_empty() && !big.is_empty(), "{card:?}");
        }
        assert!(p.set(review(&log, p.period, today, &Plain)));
        assert_eq!(p.card, 0);
    }

    #[test]
    fn a_streak_reads_as_one_figure_when_it_is_the_one_running() {
        let today = day_of(2026, 10, 23);
        let log = demo_log(today);
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert!(w.current_streak >= 4);
        assert_eq!(streak_line(&w, today), Some(format!("{} days in a row, up to today", w.current_streak)));
        let earlier = summarise(&log, Period::of(Span::Month, today).prev(), today, &Plain);
        assert!(streak_line(&earlier, today).is_some_and(|l| l.starts_with("Longest ") && !l.contains("now")));
    }

    #[test]
    fn a_session_cut_off_reads_as_time_not_recorded() {
        let today = day_of(2026, 10, 23);
        let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
        let games = vec![Game { path: "/storage/roms/gbc/uCity.gbc".into(), name: "uCity".into(), system: "gbc".into() }, Game { path: "/storage/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into() }];
        let cut = Session { game: 0, start: (today * ph::DAY + 10 * 3600) as u64, tz: 0, secs: 0, ended: false, emulator: String::new(), ..Default::default() };
        let log = build(games.clone(), vec![cut.clone()], Vec::new(), Vec::new());
        let p = page(&log, Span::Week, today);
        let s = p.summary.as_ref().unwrap();
        assert_eq!(numbers(s, today), "1 session, time not recorded · 1 of 7 days");
        assert!(facts(s, today, &names).iter().any(|(h, l)| h == "Systems" && l == "GBC time not recorded"));
        let r = review(&log, Period::of(Span::Year, today), today, &Plain);
        let (_, big, detail, quiet, _) = card_words(&r, &r.cards[0], today, &names);
        assert_eq!((big.as_str(), detail.as_str()), ("1 session", "time not recorded · 1 day · 1 game"));
        assert!(quiet.contains("stopped while the game ran"), "{quiet}");
        assert!(!r.cards.iter().any(|c| matches!(c, Card::TopGame { .. } | Card::TopSystem { .. } | Card::Longest { .. })), "no time to rank by");
        // Mixed with timed play: the known time, and how many have none.
        let timed = Session { game: 1, start: ((today - 1) * ph::DAY + 10 * 3600) as u64, tz: 0, secs: 3900, ended: true, emulator: String::new(), ..Default::default() };
        let log = build(games, vec![timed, cut], Vec::new(), Vec::new());
        let p = page(&log, Span::Week, today);
        let s = p.summary.as_ref().unwrap();
        assert_eq!(numbers(s, today), "1 h 05 min played · 2 sessions, 1 without time · 2 of 7 days");
        let r = review(&log, Period::of(Span::Year, today), today, &Plain);
        let (_, big, detail, ..) = card_words(&r, &r.cards[0], today, &names);
        assert_eq!((big.as_str(), detail.as_str()), ("1 h 05 min", "2 sessions, 1 without time · 2 days · 2 games"));
        let mut buf = vec![0u32; 1240 * 1080];
        let mut text = Text::new();
        paint_history(&mut Canvas::new(&mut buf, 1240, 1080), &mut text, &p, &names, None);
    }

    #[test]
    fn the_scale_is_round() {
        assert_eq!(scale_top(50 * 60), 3600);
        assert_eq!(scale_top(61 * 60), 7200);
        assert_eq!(scale_top(0), 900);
        assert_eq!(scale_label(7200), "2 h");
        assert_eq!(scale_label(1800), "30 min");
    }
}
