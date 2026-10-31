//! Play history: what the play log (`play_log`) says about a week, a month or a year. Pure functions over a
//! [`Log`], no files, no drawing: Curator → History and the Year in review paint what they return
//! (`history_page`), and other parts of pocket-ui (the web companion) can serve the same numbers; every result is
//! `serde::Serialize`. docs/HISTORY.md has the API in one table.
//!
//! **Honest numbers.** Only what the log recorded is counted:
//! - a session's time is the time the game was open (its pause menu included; light sleep and suspends not), split
//!   at local midnight between the days it ran on, each stretch between two sleeps on the days it was played; a day
//!   and a period are the player's local ones, in the time zone the session started in;
//! - a session is a start that ran for at least a minute (`library_meta::MIN_SESSION_SECS`), or one that never ended
//!   (pocket-ui or the battery went down; it counts as a session without time); shorter starts add time only;
//! - sessions from before the device's clock was set, or dated after today (a clock that was ahead and was put
//!   right), count in no period, only in [`all_time`];
//! - the totals from before the log began ([`play_log::Carried`]) count in [`all_time`] and as the last play before a
//!   comeback, never in a week, month or year;
//! - "Completed" is a game marked Completed in that period (the card's status), not a guess.
//!
//! The genres come from the caller ([`Describe`]): pocket-ui passes Curator's tags (curatordb), and the same call
//! says which files are one game (Merge versions). Without it ([`Plain`]) there are no genres and every file is a
//! game of its own.

use std::collections::HashMap;

use serde::Serialize;

use crate::library_meta::{Status, MIN_SESSION_SECS};
use crate::play_log::{Game, Log, CLOCK_SET};

pub const DAY: i64 = 86_400;
/// A game played again after at least this many days "came back".
pub const BREAK_DAYS: i64 = 60;
/// Games, systems and genres a summary lists at most.
pub const TOP: usize = 10;

// ---- days and periods -------------------------------------------------------------------------------------------

pub use crate::civil_date::{civil, day_of, local_day};

/// 0 for Monday … 6 for Sunday.
pub fn weekday(day: i64) -> u32 {
    (day + 3).rem_euclid(7) as u32
}

pub const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// How long a period is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Span {
    Week,
    Month,
    Year,
}

impl Span {
    pub const ALL: [Span; 3] = [Span::Week, Span::Month, Span::Year];

    pub fn label(self) -> &'static str {
        match self {
            Span::Week => "Week",
            Span::Month => "Month",
            Span::Year => "Year",
        }
    }
}

/// A week (Monday to Sunday), a calendar month or a calendar year, as local day numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Period {
    pub span: Span,
    /// The first day.
    pub first: i64,
    /// How many days.
    pub days: i64,
}

impl Period {
    /// The period of this span that has `day` in it.
    pub fn of(span: Span, day: i64) -> Period {
        let (y, m, _) = civil(day);
        match span {
            Span::Week => Period { span, first: day - weekday(day) as i64, days: 7 },
            Span::Month => {
                let first = day_of(y, m, 1);
                let next = if m == 12 { day_of(y + 1, 1, 1) } else { day_of(y, m + 1, 1) };
                Period { span, first, days: next - first }
            }
            Span::Year => {
                let first = day_of(y, 1, 1);
                Period { span, first, days: day_of(y + 1, 1, 1) - first }
            }
        }
    }

    /// The one running now.
    pub fn current(span: Span, now: u64, tz: i32) -> Period {
        Period::of(span, local_day(now, tz))
    }

    pub fn last(&self) -> i64 {
        self.first + self.days - 1
    }

    pub fn contains(&self, day: i64) -> bool {
        (self.first..self.first + self.days).contains(&day)
    }

    pub fn prev(&self) -> Period {
        Period::of(self.span, self.first - 1)
    }

    pub fn next(&self) -> Period {
        Period::of(self.span, self.first + self.days)
    }

    /// The year it is in (its first day's).
    pub fn year(&self) -> i32 {
        civil(self.first).0
    }

    /// "This week", "Last week", "Week of 12 October", "October 2026", "2026".
    pub fn label(&self, today: i64) -> String {
        let (y, m, d) = civil(self.first);
        match self.span {
            Span::Week if self.contains(today) => "This week".into(),
            Span::Week if self.contains(today - 7) => "Last week".into(),
            Span::Week if civil(today).0 == y => format!("Week of {d} {}", MONTHS[m as usize - 1]),
            Span::Week => format!("Week of {d} {} {y}", MONTHS[m as usize - 1]),
            Span::Month => format!("{} {y}", MONTHS[m as usize - 1]),
            Span::Year => y.to_string(),
        }
    }

    /// The same span, a different "this" for the next period: "this week", "in October", "in 2026".
    pub fn phrase(&self, today: i64) -> String {
        let (y, m, _) = civil(self.first);
        match self.span {
            Span::Week if self.contains(today) => "this week".into(),
            Span::Week if self.contains(today - 7) => "last week".into(),
            Span::Week => format!("in the week of {} {}", civil(self.first).2, MONTHS[m as usize - 1]),
            Span::Month if self.contains(today) => "this month".into(),
            Span::Month if civil(today).0 == y => format!("in {}", MONTHS[m as usize - 1]),
            Span::Month => format!("in {} {y}", MONTHS[m as usize - 1]),
            Span::Year if self.contains(today) => "this year".into(),
            Span::Year => format!("in {y}"),
        }
    }

    /// The bars of its chart: one per day for a week or month, one per week (Monday to Sunday, cut to the year)
    /// for a year. (first day, days).
    pub fn buckets(&self) -> Vec<(i64, i64)> {
        match self.span {
            Span::Week | Span::Month => (0..self.days).map(|k| (self.first + k, 1)).collect(),
            Span::Year => {
                let mut out = Vec::new();
                let mut d = self.first;
                while d <= self.last() {
                    let end = (d - weekday(d) as i64 + 7).min(self.first + self.days);
                    out.push((d, end - d));
                    d = end;
                }
                out
            }
        }
    }
}

// ---- formatting ----------------------------------------------------------------------------------------------------

/// "under a minute", "12 min", "3 h 05 min", "41 h".
pub fn duration(secs: u64) -> String {
    let m = secs / 60;
    match m {
        0 => "under a minute".into(),
        1..=59 => format!("{m} min"),
        60..=599 => format!("{} h {:02} min", m / 60, m % 60),
        _ => format!("{} h", (m + 30) / 60),
    }
}

/// "14 March".
pub fn date(day: i64) -> String {
    let (_, m, d) = civil(day);
    format!("{d} {}", MONTHS[m as usize - 1])
}

/// "14 March 2025".
pub fn date_year(day: i64) -> String {
    let (y, _, _) = civil(day);
    format!("{} {y}", date(day))
}

/// How long a break was: "9 weeks", "7 months", "2 years".
pub fn gap(days: i64) -> String {
    match days {
        0..=13 => format!("{days} days"),
        14..=90 => format!("{} weeks", days / 7),
        91..=729 => format!("{} months", (days as f64 / 30.44).round() as i64),
        _ => format!("{} years", days / 365),
    }
}

/// Time played, honest about sessions whose end was never recorded (pocket-ui or the Pocket went down while they
/// ran; their time is unknown, not zero): "2 h 05 min"; "time not recorded" when they are all there is; else the
/// known time and how many lack it, "2 h 05 min, 1 without time" (`long`: "…, 1 session without time").
pub fn time_text(secs: u64, untimed: u32, long: bool) -> String {
    match (secs, untimed) {
        (_, 0) => duration(secs),
        (0, _) => "time not recorded".into(),
        (_, n) if long => format!("{}, {} without time", duration(secs), count(n as usize, "session", "sessions")),
        (_, n) => format!("{}, {n} without time", duration(secs)),
    }
}

/// "3 sessions"; with sessions whose end was never recorded "3 sessions, 1 without time", or "1 session, time not
/// recorded" when no time at all is known (`secs` 0).
pub fn sessions_text(sessions: u32, untimed: u32, secs: u64) -> String {
    let n = count(sessions as usize, "session", "sessions");
    match untimed {
        0 => n,
        u if u >= sessions && secs == 0 => format!("{n}, time not recorded"),
        u => format!("{n}, {u} without time"),
    }
}

/// "1 game", "3 games".
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

// ---- what the caller knows about games -------------------------------------------------------------------------

/// What kind of genre label it is, from the most general.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenreKind {
    /// "racing", "role-playing".
    Family,
    /// "arcade racing", "metroidvania".
    SubGenre,
    /// "crash takedowns", "drifting".
    Mechanic,
}

/// A genre-like tag of a game: its short label as reasons write it, its kind, how strongly the game has it (1-3).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Genre {
    pub label: String,
    pub kind: GenreKind,
    pub weight: f32,
}

/// What the history can't know from the log: a game's genres (curatordb) and which files are one game.
pub trait Describe {
    /// The game's genre tags, strongest first; none by default.
    fn genres(&self, _game: &Game) -> Vec<Genre> {
        Vec::new()
    }
    /// The name the library gives the file today: it tells two versions of a game apart ("Tobu Tobu Girl (Europe)",
    /// `library::tell_apart`), which the log, written before or without the library, may not; None: the log's name.
    fn name(&self, _game: &Game) -> Option<String> {
        None
    }
    /// The work a file is a version of (Merge versions: the same work on two systems is one game); None: its own.
    fn work(&self, _game: &Game) -> Option<String> {
        None
    }
}

/// No genres, every file a game of its own.
pub struct Plain;
impl Describe for Plain {}

// ---- results ---------------------------------------------------------------------------------------------------------

/// A game in a period. With merged versions, the version played most stands for the game.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct GameTime {
    /// Index into the log's games (the version shown).
    pub game: usize,
    pub path: String,
    pub name: String,
    pub system: String,
    pub secs: u64,
    pub sessions: u32,
    /// Of `sessions`, those without a known end (pocket-ui or the Pocket went down while it ran): their time is
    /// unknown, not zero, and is in no `secs`.
    pub untimed: u32,
    /// Days with a session of it.
    pub days: u32,
    pub last_day: i64,
    /// Played for the first time ever in this period (no earlier session, nothing from before the log).
    pub new: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SystemTime {
    pub system: String,
    pub secs: u64,
    pub sessions: u32,
    /// Of `sessions`, those without a known end.
    pub untimed: u32,
    pub games: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GenreTime {
    pub label: String,
    pub kind: GenreKind,
    /// Time in games with this tag.
    pub secs: u64,
    pub games: u32,
}

/// One bar of the chart: its first day, its days, the time in them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Bar {
    pub first: i64,
    pub days: i64,
    pub secs: u64,
    /// Sessions in it without a known end (a day played, its time unknown).
    pub untimed: u32,
}

/// Days in a row with a session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Streak {
    pub days: u32,
    pub from: i64,
    pub to: i64,
}

/// A game played again after a long break.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Comeback {
    pub game: GameTime,
    pub gap_days: i64,
    pub day: i64,
}

/// A session worth naming: the first of the period, the longest.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Moment {
    pub game: GameTime,
    pub start: u64,
    pub tz: i32,
    pub secs: u64,
    pub day: i64,
}

/// Everything Curator → History shows for one period.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Summary {
    pub period: Period,
    pub label: String,
    pub secs: u64,
    pub sessions: u32,
    /// Of `sessions`, those without a known end: counted as sessions and days, their time unknown (in no `secs`).
    pub untimed: u32,
    pub days_played: u32,
    /// Most played first (time, then sessions).
    pub games: Vec<GameTime>,
    pub systems: Vec<SystemTime>,
    /// Most leaned into first: particular tags (sub-genres, mechanics) before families.
    pub genres: Vec<GenreTime>,
    /// The two labels that say it best: "arcade racing · crash takedowns"; empty without genres.
    pub leaned_into: String,
    pub bars: Vec<Bar>,
    /// The longest run of days in a row within the period.
    pub longest_streak: Option<Streak>,
    /// Days in a row up to today (or yesterday), when the period is the one running now; 0 otherwise.
    pub current_streak: u32,
    /// Longest break first.
    pub comebacks: Vec<Comeback>,
    /// Games marked Completed in the period, with the day; in order.
    pub completed: Vec<(GameTime, i64)>,
    pub first: Option<Moment>,
    pub longest: Option<Moment>,
    pub new_games: u32,
    /// The day the log began, when that is inside the period: what came before is not in these numbers.
    pub counting_since: Option<i64>,
    /// The local day of the log's first dated session (None: nothing dated yet). A period that ends before it was
    /// not recorded, which the page says instead of "Nothing played".
    pub first_day: Option<i64>,
    /// Part of the log could not be read (damaged frames were skipped).
    pub damaged: bool,
    /// An older log was no log at all and was kept aside (`play_log::Log::set_aside`): the log began again on
    /// `first_day`.
    pub set_aside: bool,
}

impl Summary {
    pub fn is_empty(&self) -> bool {
        self.sessions == 0 && self.secs == 0
    }

    /// The period ended before the log began (or nothing dated was recorded yet): empty because it was not counted.
    pub fn before_log(&self) -> bool {
        self.first_day.is_none_or(|d| self.period.last() < d)
    }
}

/// The time of one stretch of play falling on each local day it ran on: (day, seconds).
fn split(start: u64, tz: i32, secs: u64, mut each: impl FnMut(i64, u64)) {
    let mut t = start as i64 + tz as i64;
    let mut left = secs as i64;
    while left > 0 {
        let day = t.div_euclid(DAY);
        let part = left.min((day + 1) * DAY - t);
        each(day, part as u64);
        t += part;
        left -= part;
    }
}

fn counts_as_session(secs: u64, ended: bool) -> bool {
    !ended || secs >= MIN_SESSION_SECS
}

/// Works out a period. `today` is the local day now (for "this week" and the streak running now).
pub fn summarise(log: &Log, period: Period, today: i64, d: &dyn Describe) -> Summary {
    // Which games are one (merged versions): a key per game of the log.
    let mut keys: HashMap<String, usize> = HashMap::new();
    let key_of: Vec<usize> = log
        .games
        .iter()
        .map(|g| {
            let k = d.work(g).map_or_else(|| format!("file:{}", g.path), |w| format!("work:{w}"));
            let n = keys.len();
            *keys.entry(k).or_insert(n)
        })
        .collect();
    let groups = keys.len();

    // Per group: time, sessions, days; and per version, time (the one played most stands for the game).
    struct Acc {
        secs: u64,
        sessions: u32,
        untimed: u32,
        days: Vec<i64>,
        last_day: i64,
        versions: HashMap<usize, u64>,
    }
    let mut acc: Vec<Option<Acc>> = (0..groups).map(|_| None).collect();
    // The day each group was last played before the session being looked at (carried totals first).
    let mut last_seen: Vec<Option<i64>> = vec![None; groups];
    let mut ever_before: Vec<bool> = vec![false; groups];
    for c in &log.carried {
        let k = key_of[c.game];
        ever_before[k] = true;
        if let Some(t) = c.last_played.filter(|t| *t >= CLOCK_SET) {
            let day = local_day(t, 0);
            last_seen[k] = Some(last_seen[k].map_or(day, |l| l.max(day)));
        }
    }
    let buckets = period.buckets();
    let mut bars: Vec<Bar> = buckets.iter().map(|&(first, days)| Bar { first, days, secs: 0, untimed: 0 }).collect();
    let bar_of = |day: i64| -> Option<usize> {
        if !period.contains(day) {
            return None;
        }
        match period.span {
            Span::Week | Span::Month => Some((day - period.first) as usize),
            Span::Year => Some(buckets.partition_point(|&(f, _)| f <= day) - 1),
        }
    };
    let mut played_days: Vec<i64> = Vec::new();
    let mut period_days: Vec<i64> = Vec::new();
    let (mut secs, mut sessions, mut untimed) = (0u64, 0u32, 0u32);
    let mut comebacks: HashMap<usize, (i64, i64)> = HashMap::new();
    let mut first: Option<(usize, &crate::play_log::Session)> = None;
    let mut longest: Option<(usize, &crate::play_log::Session)> = None;
    let mut new_in_period: Vec<bool> = vec![false; groups];
    // Per console, from the sessions themselves: merged versions are one game, but each console keeps the time and
    // sessions played on it (review SOL-2026-10-24 #9). The games of each: the merged games played on it.
    let mut by_system: HashMap<&str, (SystemTime, Vec<usize>)> = HashMap::new();
    for s in &log.sessions {
        let day = local_day(s.start, s.tz);
        // Before the clock was set, or after today (the clock was wrong ahead, then put right): no day to put it on.
        if s.start < CLOCK_SET || day > today {
            continue;
        }
        let k = key_of[s.game];
        let counted = counts_as_session(s.secs, s.ended);
        if counted {
            played_days.push(day);
        }
        let in_period = period.contains(day);
        // Time inside the period, split at midnight (a session from New Year's Eve counts in both years).
        let mut inside = 0u64;
        // Stretch by stretch when sleep came in between (a suspend over midnight: each part on its own day).
        if s.secs > 0 && day <= period.last() && local_day(s.end(), s.tz) >= period.first {
            for (at, secs) in s.stretches() {
                split(at, s.tz, secs, |d, t| {
                    if let Some(b) = bar_of(d) {
                        bars[b].secs += t;
                        inside += t;
                    }
                });
            }
        }
        if in_period || inside > 0 {
            let a = acc[k].get_or_insert_with(|| Acc { secs: 0, sessions: 0, untimed: 0, days: Vec::new(), last_day: day, versions: HashMap::new() });
            a.secs += inside;
            *a.versions.entry(s.game).or_default() += inside.max(1);
            a.last_day = a.last_day.max(day);
            secs += inside;
            let system = log.games[s.game].system.as_str();
            let (st, sys_games) = by_system.entry(system).or_insert_with(|| (SystemTime { system: system.to_string(), ..Default::default() }, Vec::new()));
            st.secs += inside;
            if in_period && counted {
                st.sessions += 1;
                st.untimed += u32::from(!s.ended);
            }
            if (inside > 0 || (in_period && counted)) && !sys_games.contains(&k) {
                sys_games.push(k);
            }
            if in_period && counted {
                a.sessions += 1;
                a.days.push(day);
                sessions += 1;
                if !s.ended {
                    a.untimed += 1;
                    untimed += 1;
                    if let Some(b) = bar_of(day) {
                        bars[b].untimed += 1;
                    }
                }
                period_days.push(day);
                if first.is_none() {
                    first = Some((k, s));
                }
                if longest.is_none_or(|(_, l)| s.secs > l.secs) && s.ended {
                    longest = Some((k, s));
                }
                if !ever_before[k] {
                    new_in_period[k] = true;
                }
                if let Some(prev) = last_seen[k] {
                    let gap = day - prev;
                    if gap >= BREAK_DAYS && comebacks.get(&k).is_none_or(|(g, _)| gap > *g) {
                        comebacks.insert(k, (gap, day));
                    }
                }
            }
        }
        if counted {
            ever_before[k] = true;
            last_seen[k] = Some(last_seen[k].map_or(day, |l| l.max(day)));
        }
    }

    // The games of the period.
    let mut game_times: Vec<Option<GameTime>> = vec![None; groups];
    for (k, a) in acc.iter_mut().enumerate() {
        let Some(a) = a else { continue };
        a.days.sort_unstable();
        a.days.dedup();
        let shown = a.versions.iter().max_by_key(|(g, t)| (**t, std::cmp::Reverse(**g))).map(|(g, _)| *g).unwrap_or(0);
        let g = &log.games[shown];
        game_times[k] = Some(GameTime {
            game: shown,
            path: g.path.clone(),
            name: d.name(g).unwrap_or_else(|| g.name.clone()),
            system: g.system.clone(),
            secs: a.secs,
            sessions: a.sessions,
            untimed: a.untimed,
            days: a.days.len() as u32,
            last_day: a.last_day,
            new: new_in_period[k],
        });
    }
    let gt = |k: usize| game_times[k].clone().unwrap_or_default();
    let mut games: Vec<GameTime> = game_times.iter().flatten().filter(|g| g.secs > 0 || g.sessions > 0).cloned().collect();
    games.sort_by(|a, b| (b.secs, b.sessions, &a.name).cmp(&(a.secs, a.sessions, &b.name)));

    // Systems, as the sessions were played on them.
    let mut systems: Vec<SystemTime> = by_system
        .into_values()
        .map(|(mut st, games)| {
            st.games = games.len() as u32;
            st
        })
        .filter(|st| st.secs > 0 || st.sessions > 0)
        .collect();
    systems.sort_by(|a, b| (b.secs, b.sessions, &a.system).cmp(&(a.secs, a.sessions, &b.system)));
    systems.truncate(TOP);

    // Genres, weighted by time.
    let mut genre_acc: HashMap<(String, GenreKind), (f64, u64, u32)> = HashMap::new();
    for g in &games {
        let game = &log.games[g.game];
        for t in d.genres(game) {
            let e = genre_acc.entry((t.label.clone(), t.kind)).or_default();
            e.0 += g.secs as f64 * t.weight as f64;
            e.1 += g.secs;
            e.2 += 1;
        }
    }
    // (label, kind) -> (time weighted by the tag's strength, time, games).
    type Ranked = ((String, GenreKind), (f64, u64, u32));
    let mut ranked: Vec<Ranked> = genre_acc.into_iter().filter(|(_, v)| v.1 > 0).collect();
    // Particular tags say more than a family ("arcade racing" over "racing").
    ranked.sort_by(|a, b| {
        let fam = |k: GenreKind| k == GenreKind::Family;
        (fam(a.0 .1), b.1 .0, &a.0 .0).partial_cmp(&(fam(b.0 .1), a.1 .0, &b.0 .0)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let leaned_into = {
        let pick = |kind: GenreKind| ranked.iter().find(|(k, _)| k.1 == kind).map(|(k, _)| k.0.clone());
        let mut parts: Vec<String> = [pick(GenreKind::SubGenre), pick(GenreKind::Mechanic)].into_iter().flatten().collect();
        for (k, _) in &ranked {
            if parts.len() >= 2 {
                break;
            }
            if !parts.contains(&k.0) {
                parts.push(k.0.clone());
            }
        }
        parts.join(" · ")
    };
    let genres: Vec<GenreTime> = ranked.into_iter().take(TOP).map(|((label, kind), (_, secs, n))| GenreTime { label, kind, secs, games: n }).collect();

    // Streaks.
    played_days.sort_unstable();
    played_days.dedup();
    period_days.sort_unstable();
    period_days.dedup();
    let mut longest_streak: Option<Streak> = None;
    let mut run_start = 0usize;
    for i in 0..period_days.len() {
        if i > 0 && period_days[i] != period_days[i - 1] + 1 {
            run_start = i;
        }
        let days = (i - run_start + 1) as u32;
        if longest_streak.is_none_or(|s| days > s.days) {
            longest_streak = Some(Streak { days, from: period_days[run_start], to: period_days[i] });
        }
    }
    let current_streak = if period.contains(today) {
        match played_days.last() {
            Some(&last) if last == today || last == today - 1 => {
                let mut n = 1u32;
                let mut i = played_days.len() - 1;
                while i > 0 && played_days[i - 1] == played_days[i] - 1 {
                    n += 1;
                    i -= 1;
                }
                n
            }
            _ => 0,
        }
    } else {
        0
    };

    // Comebacks and completions.
    let mut comebacks: Vec<Comeback> = comebacks.into_iter().map(|(k, (gap_days, day))| Comeback { game: gt(k), gap_days, day }).collect();
    comebacks.sort_by(|a, b| (b.gap_days, a.day).cmp(&(a.gap_days, b.day)));
    let mut done: HashMap<usize, i64> = HashMap::new();
    for st in log.statuses.iter().filter(|s| s.status == Status::Completed && s.at >= CLOCK_SET) {
        let day = local_day(st.at, st.tz);
        if period.contains(day) && day <= today {
            done.insert(key_of[st.game], day);
        }
    }
    let mut completed: Vec<(GameTime, i64)> = done
        .into_iter()
        .map(|(k, day)| {
            let mut g = gt(k);
            if g.path.is_empty() {
                // Marked completed without being played in the period: named from the log's own record.
                let shown = key_of.iter().position(|x| *x == k).unwrap_or(0);
                let game = &log.games[shown];
                g = GameTime { game: shown, path: game.path.clone(), name: d.name(game).unwrap_or_else(|| game.name.clone()), system: game.system.clone(), ..Default::default() };
            }
            (g, day)
        })
        .collect();
    completed.sort_by(|a, b| (a.1, &a.0.name).cmp(&(b.1, &b.0.name)));

    let moment = |m: Option<(usize, &crate::play_log::Session)>| m.map(|(k, s)| Moment { game: gt(k), start: s.start, tz: s.tz, secs: s.secs, day: local_day(s.start, s.tz) });
    let since = log.counting_since().map(|t| log.sessions.iter().find(|s| s.start == t).map_or(local_day(t, 0), |s| local_day(t, s.tz))).filter(|d| *d <= today);
    Summary {
        period,
        label: period.label(today),
        secs,
        sessions,
        untimed,
        days_played: period_days.len() as u32,
        new_games: new_in_period.iter().zip(&game_times).filter(|(n, g)| **n && g.is_some()).count() as u32,
        games,
        systems,
        genres,
        leaned_into,
        bars,
        longest_streak,
        current_streak,
        comebacks,
        completed,
        first: moment(first),
        longest: moment(longest),
        counting_since: since.filter(|d| *d > period.first && *d <= period.last()),
        first_day: since,
        damaged: log.damaged > 0,
        set_aside: log.set_aside,
    }
}

/// Seconds played from `since` to `now` (unix), from the newest session back: only the sessions of that stretch
/// are looked at, cheap enough for a row's side text. Sessions dated after `now` (a clock that was ahead) don't count.
pub fn secs_since(log: &Log, since: u64, now: u64) -> u64 {
    played_since(log, since, now).0
}

/// The History row's side text from `played_since` this week: "40 min this week"; "Nothing this week" rather
/// than nothing, so the row and Split's panel are never blank (only a sign that the log is not there gives "").
pub fn week_side(played: Option<(u64, u32)>) -> String {
    match played {
        Some((secs, 0)) if secs >= 60 => format!("{} this week", duration(secs)),
        Some((secs, n)) if n > 0 && secs >= 60 => format!("{} this week, {n} without time", duration(secs)),
        Some((_, n)) if n > 0 => format!("{} this week, time not recorded", count(n as usize, "session", "sessions")),
        Some(_) => "Nothing this week".into(),
        None => String::new(),
    }
}

/// `secs_since`, and how many sessions of that stretch have no known end (their time unknown).
pub fn played_since(log: &Log, since: u64, now: u64) -> (u64, u32) {
    log.sessions.iter().rev().skip_while(|s| s.start > now).take_while(|s| s.end() >= since).fold((0, 0), |(t, n), s| {
        let played: u64 = s.stretches().map(|(at, secs)| secs.saturating_sub(since.saturating_sub(at))).sum();
        (t + played, n + (!s.ended && s.start >= since) as u32)
    })
}

/// Totals over everything, the time from before the log included.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AllTime {
    pub secs: u64,
    pub sessions: u32,
    pub games: u32,
    /// Of `secs`, what was carried over from before the log began.
    pub carried_secs: u64,
    /// Of `sessions`, those without a known end (their time is in no `secs`).
    pub untimed: u32,
    /// Unix seconds of the log's first dated session.
    pub counting_since: Option<u64>,
}

pub fn all_time(log: &Log) -> AllTime {
    let mut played = vec![false; log.games.len()];
    let mut a = AllTime { counting_since: log.counting_since(), ..Default::default() };
    for s in &log.sessions {
        a.secs += s.secs;
        a.sessions += counts_as_session(s.secs, s.ended) as u32;
        a.untimed += !s.ended as u32;
        played[s.game] = true;
    }
    for c in &log.carried {
        a.secs += c.secs;
        a.carried_secs += c.secs;
        a.sessions += c.sessions;
        played[c.game] |= c.secs > 0 || c.sessions > 0;
    }
    a.games = played.iter().filter(|p| **p).count() as u32;
    a
}

// ---- the review --------------------------------------------------------------------------------------------------

/// One full-screen card of a Year (or Month) in review.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "card", rename_all = "snake_case")]
pub enum Card {
    /// The period in three numbers.
    Overview { secs: u64, sessions: u32, untimed: u32, days: u32, games: u32, counting_since: Option<i64> },
    TopGame { game: GameTime, percent: u32 },
    TopSystem { system: String, secs: u64, untimed: u32, games: u32, percent: u32, only: bool },
    Genres { line: String, genres: Vec<GenreTime> },
    First { moment: Moment },
    Longest { moment: Moment },
    CameBack { comeback: Comeback },
    Completed { games: Vec<(GameTime, i64)> },
    Streak { streak: Streak },
    /// "You might like next", from Echoes (filled in by the caller, which has the index).
    Next { name: String, system: String, path: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Review {
    pub period: Period,
    /// "2026", "October 2026".
    pub title: String,
    pub summary: Summary,
    pub cards: Vec<Card>,
}

fn percent(part: u64, whole: u64) -> u32 {
    if whole == 0 {
        0
    } else {
        ((part as f64 * 100.0 / whole as f64).round() as u32).min(100)
    }
}

/// The cards of a period's review, in order. Only cards there is something true to say on: a quiet month has two.
pub fn review(log: &Log, period: Period, today: i64, d: &dyn Describe) -> Review {
    let s = summarise(log, period, today, d);
    let mut cards = vec![Card::Overview { secs: s.secs, sessions: s.sessions, untimed: s.untimed, days: s.days_played, games: s.games.len() as u32, counting_since: s.counting_since }];
    if let Some(top) = s.games.first().filter(|g| g.secs > 0) {
        cards.push(Card::TopGame { game: top.clone(), percent: percent(top.secs, s.secs) });
    }
    if let Some(sys) = s.systems.first().filter(|x| x.secs > 0) {
        cards.push(Card::TopSystem { system: sys.system.clone(), secs: sys.secs, untimed: sys.untimed, games: sys.games, percent: percent(sys.secs, s.secs), only: s.systems.len() == 1 });
    }
    if !s.leaned_into.is_empty() {
        cards.push(Card::Genres { line: s.leaned_into.clone(), genres: s.genres.iter().take(3).cloned().collect() });
    }
    if let Some(m) = s.first.clone() {
        cards.push(Card::First { moment: m });
    }
    if let Some(m) = s.longest.clone().filter(|m| m.secs >= MIN_SESSION_SECS) {
        cards.push(Card::Longest { moment: m });
    }
    if let Some(c) = s.comebacks.first().cloned() {
        cards.push(Card::CameBack { comeback: c });
    }
    if !s.completed.is_empty() {
        cards.push(Card::Completed { games: s.completed.clone() });
    }
    if let Some(st) = s.longest_streak.filter(|st| st.days >= 2) {
        cards.push(Card::Streak { streak: st });
    }
    let title = period.label(today);
    Review { period, title, summary: s, cards }
}

// ---- the read API for other parts of pocket-ui (the web companion) ---------------------------------------------

/// "2026-10-23" for a local day number.
pub fn iso(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Sessions `stats_json` lists under "recent", newest first.
pub const RECENT: usize = 20;

/// What a `stats_json` result was worked out from: the data directory (None: a log handed to `stats_value`), the
/// log's contents (`fingerprint`), the local day and the UTC offset. Another log, another day or zone: counted anew.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StatsKey {
    dir: Option<std::path::PathBuf>,
    log: u64,
    today: i64,
    tz: i32,
}

/// What `stats_json` last worked out, and for which log and day: a page that asks again gets it without counting.
static STATS_CACHE: std::sync::Mutex<Option<(StatsKey, serde_json::Value)>> = std::sync::Mutex::new(None);

/// A hash of everything in a log that the numbers come from (games, sessions with their parts, statuses, carried
/// totals, damage): two logs with the same counts are still told apart (review SOL-2026-10-24 #14). A few
/// hundred microseconds for years of play, against the milliseconds counting takes.
fn fingerprint(log: &Log) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for g in &log.games {
        (&g.path, &g.name, &g.system).hash(&mut h);
    }
    for s in &log.sessions {
        (s.game, s.start, s.tz, s.secs, s.ended, &s.emulator).hash(&mut h);
        for p in &s.parts {
            (p.at, p.secs).hash(&mut h);
        }
    }
    for st in &log.statuses {
        (st.game, st.at, st.tz, std::mem::discriminant(&st.status)).hash(&mut h);
    }
    for c in &log.carried {
        (c.game, c.secs, c.sessions, c.last_played).hash(&mut h);
    }
    (log.games.len(), log.sessions.len(), log.statuses.len(), log.carried.len(), log.migrated, log.damaged, log.set_aside).hash(&mut h);
    h.finish()
}

/// The play history of a data directory as JSON, for the web companion's `GET /api/stats` (its `HistorySource`:
/// `web::set_history_source(pocket_ui::play_history::stats_json)`). Its shape is in docs/HISTORY.md ("Read API").
///
/// - Cheap: the writer's copy of the log when it is this process's (no file read), the file otherwise; the result is
///   kept until the log or the day changes. Counting a large log takes some milliseconds, on the caller's thread
///   (the web server's, never the UI's).
/// - None when there is no log yet, when Play history is switched off, or when the log can't be counted.
/// - Never panics: every index into the log is checked before anything is counted.
/// - Genres and merged versions need Curator's index, which this has no access to: none here (every file is a game).
pub fn stats_json(data_dir: &std::path::Path) -> Option<serde_json::Value> {
    let log = crate::play_log::log_for(data_dir)?;
    let now = crate::recents::now_secs();
    let tz = crate::appearance::utc_offset_now(now as i64);
    stats_of(Some(data_dir), &log, now, tz)
}

/// `stats_json` for a log in hand, at `now` (unix) `tz` seconds east of UTC.
pub fn stats_value(log: &Log, now: u64, tz: i32) -> Option<serde_json::Value> {
    stats_of(None, log, now, tz)
}

fn stats_of(dir: Option<&std::path::Path>, log: &Log, now: u64, tz: i32) -> Option<serde_json::Value> {
    use serde_json::json;
    let n = log.games.len();
    let sound = log.sessions.iter().all(|s| s.game < n) && log.statuses.iter().all(|s| s.game < n) && log.carried.iter().all(|c| c.game < n);
    if !sound || (log.sessions.is_empty() && log.carried.is_empty() && log.statuses.is_empty()) {
        return None;
    }
    let today = local_day(now, tz);
    let key = StatsKey { dir: dir.map(std::path::Path::to_path_buf), log: fingerprint(log), today, tz };
    let mut cache = STATS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, v)) = cache.as_ref() {
        if *k == key {
            return Some(v.clone());
        }
    }
    let game = |g: &GameTime| json!({ "name": g.name, "system": g.system, "path": g.path });
    let period = |span: Span| {
        let s = summarise(log, Period::of(span, today), today, &Plain);
        json!({
            "label": s.label,
            "first": iso(s.period.first),
            "last": iso(s.period.last()),
            "secs": s.secs,
            "sessions": s.sessions,
            "untimed_sessions": s.untimed,
            "days_played": s.days_played,
            "games": s.games.iter().take(TOP).map(|g| json!({ "name": g.name, "system": g.system, "path": g.path, "secs": g.secs, "sessions": g.sessions, "untimed_sessions": g.untimed, "days": g.days, "new": g.new })).collect::<Vec<_>>(),
            "systems": s.systems.iter().map(|x| json!({ "system": x.system, "secs": x.secs, "sessions": x.sessions, "untimed_sessions": x.untimed, "games": x.games })).collect::<Vec<_>>(),
            "bars": s.bars.iter().map(|b| json!({ "first": iso(b.first), "days": b.days, "secs": b.secs, "untimed_sessions": b.untimed })).collect::<Vec<_>>(),
            "longest_streak": s.longest_streak.map(|st| json!({ "days": st.days, "from": iso(st.from), "to": iso(st.to) })),
            "current_streak": s.current_streak,
            "came_back": s.comebacks.iter().map(|c| { let mut v = game(&c.game); v["gap_days"] = json!(c.gap_days); v["date"] = json!(iso(c.day)); v }).collect::<Vec<_>>(),
            "completed": s.completed.iter().map(|(g, d)| { let mut v = game(g); v["date"] = json!(iso(*d)); v }).collect::<Vec<_>>(),
            "new_games": s.new_games,
            "not_recorded": s.is_empty() && s.before_log(),
        })
    };
    let a = all_time(log);
    let first_day = log.counting_since().map(|t| log.sessions.iter().find(|s| s.start == t).map_or(local_day(t, 0), |s| local_day(t, s.tz)));
    let recent: Vec<serde_json::Value> = log
        .sessions
        .iter()
        .rev()
        .take(RECENT)
        .map(|s| {
            let g = &log.games[s.game];
            json!({ "name": g.name, "system": g.system, "path": g.path, "start": s.start, "date": (s.start >= CLOCK_SET).then(|| iso(local_day(s.start, s.tz))), "secs": s.secs, "ended": s.ended, "emulator": s.emulator })
        })
        .collect();
    let v = json!({
        "version": 1,
        "today": iso(today),
        "counting_since": first_day.map(iso),
        "damaged": log.damaged > 0 || log.set_aside,
        "all_time": { "secs": a.secs, "sessions": a.sessions, "untimed_sessions": a.untimed, "games": a.games, "carried_secs": a.carried_secs },
        "week": period(Span::Week),
        "month": period(Span::Month),
        "year": period(Span::Year),
        "recent": recent,
    });
    *cache = Some((key, v.clone()));
    Some(v)
}

// ---- made-up history ---------------------------------------------------------------------------------------------

/// Games for made-up history where no library is at hand (the feature card's preview, tests): "Sample game" and
/// friends on a few systems.
pub fn demo_games() -> Vec<Game> {
    let systems = ["gba", "snes", "ps2", "gb", "psx", "md", "n64", "nds", "ports", "nes", "gbc", "psp"];
    (0..24)
        .map(|i| {
            let name = if i == 0 { "Sample game".to_string() } else { format!("Sample game {}", i + 1) };
            let system = systems[i % systems.len()];
            Game { path: format!("/storage/roms/{system}/{name}.rom"), name, system: system.into() }
        })
        .collect()
}

/// Made-up history ending on `today` (a local day, `tz` seconds east of UTC): about two and a half years of evening
/// sessions, heavier on a few favourites, with breaks, a game that comes back after months, two games marked completed
/// this year, a streak up to today and some time from before the log. Always the same for the same input: snapshots
/// and the feature card use it.
pub fn demo(games: &[Game], today: i64, tz: i32) -> Log {
    use crate::play_log::{build, Carried, Session, StatusChange};
    let n = games.len().max(1);
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move |m: u64| -> u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % m.max(1)
    };
    let at = |day: i64, minute: i64| (day * DAY + minute * 60 - tz as i64) as u64;
    let first = today - 900;
    let mut sessions = Vec::new();
    for day in first..=today {
        let (_, m, _) = civil(day);
        // A quieter summer, busier winters, a few weeks of nothing now and then; always the last four days.
        let quiet = matches!(m, 6..=8) as u64 * 25 + 30;
        let streak = day > today - 4;
        if !streak && (rnd(100) < quiet || (day - first) % 97 < 9) {
            continue;
        }
        for k in 0..(1 + rnd(3)) {
            // Favourites first: the smaller of two picks.
            let mut g = rnd(n as u64).min(rnd(n as u64)) as usize;
            // The fifth game: played early on, then not for months, then again in this year's spring.
            if g == 4 && !(day < first + 60 || (today - 200..today - 150).contains(&day)) {
                g = 1;
            }
            let start = 18 * 60 + k as i64 * 110 + rnd(50) as i64;
            let secs = 600 + rnd(5400);
            sessions.push(Session { game: g.min(n - 1), start: at(day, start), tz, secs, ended: true, emulator: "pocket-core".into(), ..Default::default() });
        }
    }
    let year_first = day_of(civil(today).0, 1, 1);
    let statuses = vec![
        StatusChange { game: 0, at: at(year_first + 40, 21 * 60), tz, status: crate::library_meta::Status::Completed },
        StatusChange { game: 2.min(n - 1), at: at(today - 20, 21 * 60), tz, status: crate::library_meta::Status::Completed },
    ];
    let carried = vec![Carried { game: 1.min(n - 1), secs: 36_000, sessions: 14, last_played: Some(at(first - 30, 20 * 60)) }];
    build(games.to_vec(), sessions, statuses, carried)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn the_history_row_always_says_something_about_this_week() {
        assert_eq!(week_side(Some((40 * 60, 0))), "40 min this week");
        assert_eq!(week_side(Some((0, 0))), "Nothing this week", "a quiet week is said, not left blank");
        assert_eq!(week_side(Some((30, 0))), "Nothing this week", "under a minute is not a week's play");
        assert_eq!(week_side(Some((0, 2))), "2 sessions this week, time not recorded");
        assert_eq!(week_side(None), "");
    }
    use crate::play_log::{build, Carried, Session, StatusChange};

    const TZ: i32 = 3600; // Vienna in winter

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> u64 {
        (day_of(y, m, d) * DAY + h as i64 * 3600 + min as i64 * 60 - TZ as i64) as u64
    }

    fn game(name: &str, system: &str) -> Game {
        Game { path: format!("/storage/roms/{system}/{name}.rom"), name: name.into(), system: system.into() }
    }

    fn session(game: usize, start: u64, secs: u64) -> Session {
        Session { game, start, tz: TZ, secs, ended: true, emulator: "pocket-core".into(), ..Default::default() }
    }

    #[test]
    fn calendar_days_and_periods() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(weekday(0), 3, "1 January 1970 was a Thursday");
        for (y, m, d) in [(2026, 10, 23), (2024, 2, 29), (2000, 3, 1), (1999, 12, 31)] {
            assert_eq!(civil(day_of(y, m, d)), (y, m, d));
        }
        let today = day_of(2026, 10, 23); // a Friday
        assert_eq!(weekday(today), 4);
        let w = Period::of(Span::Week, today);
        assert_eq!((civil(w.first), w.days), ((2026, 10, 19), 7), "weeks start on Monday");
        assert_eq!(w.label(today), "This week");
        assert_eq!(w.prev().label(today), "Last week");
        assert_eq!(w.prev().prev().label(today), "Week of 5 October");
        let m = Period::of(Span::Month, today);
        assert_eq!((civil(m.first), m.days, m.label(today)), ((2026, 10, 1), 31, "October 2026".to_string()));
        assert_eq!(Period::of(Span::Month, day_of(2024, 2, 10)).days, 29, "a leap February");
        assert_eq!(m.next().label(today), "November 2026");
        let y = Period::of(Span::Year, today);
        assert_eq!((y.days, y.label(today), y.prev().label(today)), (365, "2026".to_string(), "2025".to_string()));
        // A year's bars are weeks, Monday to Sunday, the first and last cut to the year.
        let b = y.buckets();
        assert_eq!(b.first(), Some(&(day_of(2026, 1, 1), 4)), "Thursday 1 January to Sunday the 4th");
        assert_eq!(b.iter().map(|x| x.1).sum::<i64>(), 365);
        assert!(b.iter().skip(1).all(|(d, _)| weekday(*d) == 0));
        assert_eq!(m.buckets().len(), 31);
        // A local day is the player's: 23:30 in Vienna is still that day.
        assert_eq!(local_day(at(2026, 1, 5, 23, 30), TZ), day_of(2026, 1, 5));
        assert_eq!(local_day(at(2026, 1, 5, 23, 30), 0), day_of(2026, 1, 5));
        assert_eq!(local_day(at(2026, 1, 6, 0, 30), 0), day_of(2026, 1, 5), "in UTC it was the evening before");
    }

    #[test]
    fn the_librarys_name_for_a_file_replaces_the_one_the_log_has() {
        struct Told;
        impl Describe for Told {
            fn name(&self, g: &Game) -> Option<String> {
                g.path.ends_with("Tetris.rom").then(|| "Tetris (World)".to_string())
            }
        }
        let log = build(vec![game("Burnout 3", "ps2"), game("Tetris", "gb")], vec![session(0, at(2026, 10, 19, 20, 0), 60), session(1, at(2026, 10, 19, 21, 0), 600)], Vec::new(), Vec::new());
        let today = day_of(2026, 10, 23);
        let names = |d: &dyn Describe| summarise(&log, Period::of(Span::Week, today), today, d).games.into_iter().map(|g| g.name).collect::<Vec<_>>();
        assert_eq!(names(&Told), ["Tetris (World)", "Burnout 3"], "the library knows one of them");
        assert_eq!(names(&Plain), ["Tetris", "Burnout 3"], "without the library the log's names stay");
    }

    #[test]
    fn a_period_adds_up_time_sessions_games_and_systems() {
        let games = vec![game("Burnout 3", "ps2"), game("Tetris", "gb"), game("Zelda", "gba")];
        let sessions = vec![
            session(0, at(2026, 10, 19, 20, 0), 3600),
            session(0, at(2026, 10, 21, 21, 0), 1800),
            session(1, at(2026, 10, 21, 8, 0), 600),
            session(1, at(2026, 10, 22, 8, 0), 30), // a start, not a session: time only
            session(2, at(2026, 10, 12, 10, 0), 7200), // last week
        ];
        let log = build(games, sessions, Vec::new(), Vec::new());
        let today = day_of(2026, 10, 23);
        let s = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((s.secs, s.sessions, s.days_played), (3600 + 1800 + 600 + 30, 3, 2));
        assert_eq!(s.games.iter().map(|g| (g.name.as_str(), g.secs, g.sessions)).collect::<Vec<_>>(), [("Burnout 3", 5400, 2), ("Tetris", 630, 1)]);
        assert_eq!(s.systems.iter().map(|x| x.system.as_str()).collect::<Vec<_>>(), ["ps2", "gb"]);
        assert_eq!(s.bars.iter().map(|b| b.secs).collect::<Vec<_>>(), [3600, 0, 2400, 30, 0, 0, 0]);
        assert_eq!(s.first.as_ref().unwrap().game.name, "Burnout 3");
        assert_eq!(s.longest.as_ref().unwrap().secs, 3600);
        let month = summarise(&log, Period::of(Span::Month, today), today, &Plain);
        assert_eq!(month.secs, 13_230);
        assert_eq!(month.games[0].name, "Zelda");
        assert!(summarise(&log, Period::of(Span::Year, today).prev(), today, &Plain).is_empty());
    }

    #[test]
    fn a_session_over_midnight_is_split_between_its_days_and_years() {
        let log = build(vec![game("Doom", "ports")], vec![session(0, at(2025, 12, 31, 23, 0), 7200)], Vec::new(), Vec::new());
        let today = day_of(2026, 3, 1);
        let y26 = summarise(&log, Period::of(Span::Year, today), today, &Plain);
        let y25 = summarise(&log, Period::of(Span::Year, today).prev(), today, &Plain);
        assert_eq!((y25.secs, y26.secs), (3600, 3600), "an hour in each year");
        assert_eq!((y25.sessions, y26.sessions), (1, 0), "the session counts where it started");
        assert_eq!(y26.games.len(), 1, "but the game was played in 2026 too");
    }

    #[test]
    fn streaks_count_days_in_a_row() {
        let today = day_of(2026, 10, 23);
        let mut sessions = Vec::new();
        // 3-7 October: five days; then 20, 21, 22 and 23 October: four days up to today.
        for d in [3, 4, 5, 6, 7, 20, 21, 22, 23] {
            sessions.push(session(0, at(2026, 10, d, 18, 0), 900));
        }
        sessions.push(session(0, at(2026, 10, 10, 18, 0), 20)); // too short for a day
        let log = build(vec![game("Tetris", "gb")], sessions, Vec::new(), Vec::new());
        let m = summarise(&log, Period::of(Span::Month, today), today, &Plain);
        assert_eq!(m.longest_streak, Some(Streak { days: 5, from: day_of(2026, 10, 3), to: day_of(2026, 10, 7) }));
        assert_eq!(m.current_streak, 4);
        // A week that started mid-run: only its own days.
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!(w.longest_streak.map(|s| s.days), Some(4));
        // Not played today yet: yesterday's run still counts; two days ago it doesn't.
        assert_eq!(summarise(&log, Period::of(Span::Month, today), today + 1, &Plain).current_streak, 4);
        assert_eq!(summarise(&log, Period::of(Span::Month, today), today + 2, &Plain).current_streak, 0);
        assert_eq!(summarise(&log, Period::of(Span::Month, today).prev(), today, &Plain).current_streak, 0, "only for the period running now");
    }

    #[test]
    fn a_game_back_after_a_long_break_and_games_completed() {
        let today = day_of(2026, 10, 23);
        let games = vec![game("Zelda", "gba"), game("Tetris", "gb"), game("Chrono Trigger", "snes")];
        let sessions = vec![
            session(0, at(2026, 2, 1, 18, 0), 3600),
            session(0, at(2026, 9, 14, 18, 0), 3600), // 225 days later
            session(1, at(2026, 9, 1, 18, 0), 3600),
            session(1, at(2026, 10, 1, 18, 0), 3600), // 30 days: no break
            session(2, at(2026, 10, 2, 18, 0), 3600), // last played before the log, 18 months ago
        ];
        let carried = vec![Carried { game: 2, secs: 90_000, sessions: 40, last_played: Some(at(2025, 4, 2, 20, 0)) }];
        let statuses = vec![
            StatusChange { game: 1, at: at(2026, 10, 5, 12, 0), tz: TZ, status: Status::Completed },
            StatusChange { game: 0, at: at(2025, 12, 1, 12, 0), tz: TZ, status: Status::Completed }, // the year before
        ];
        let log = build(games, sessions, statuses, carried);
        let y = summarise(&log, Period::of(Span::Year, today), today, &Plain);
        assert_eq!(y.comebacks.iter().map(|c| (c.game.name.as_str(), c.gap_days)).collect::<Vec<_>>(), [("Chrono Trigger", 548), ("Zelda", 225)]);
        assert_eq!(gap(225), "7 months");
        assert_eq!(y.completed.iter().map(|(g, d)| (g.name.as_str(), *d)).collect::<Vec<_>>(), [("Tetris", day_of(2026, 10, 5))]);
        assert_eq!(y.new_games, 2, "Chrono Trigger was played before the log");
        // The carried time is no part of a year, but of all time.
        assert_eq!(y.secs, 5 * 3600);
        let a = all_time(&log);
        assert_eq!((a.secs, a.sessions, a.games, a.carried_secs), (5 * 3600 + 90_000, 45, 3, 90_000));
        // In October only the comeback in October.
        let m = summarise(&log, Period::of(Span::Month, today), today, &Plain);
        assert_eq!(m.comebacks.len(), 1);
    }

    struct Tags;
    impl Describe for Tags {
        fn genres(&self, g: &Game) -> Vec<Genre> {
            let t = |label: &str, kind| Genre { label: label.into(), kind, weight: 2.0 };
            match g.name.as_str() {
                "Burnout 3" | "Burnout Revenge" => vec![t("racing", GenreKind::Family), t("arcade racing", GenreKind::SubGenre), t("crash takedowns", GenreKind::Mechanic)],
                "Gran Turismo 4" => vec![t("racing", GenreKind::Family), t("sim racing", GenreKind::SubGenre)],
                "Tetris" => vec![t("puzzle", GenreKind::Family)],
                _ => Vec::new(),
            }
        }
        fn work(&self, g: &Game) -> Option<String> {
            g.name.starts_with("Chrono").then(|| "Q761815".to_string())
        }
    }

    #[test]
    fn genres_follow_the_time_and_versions_merge() {
        let today = day_of(2026, 10, 23);
        let games = vec![game("Burnout 3", "ps2"), game("Burnout Revenge", "ps2"), game("Gran Turismo 4", "ps2"), game("Tetris", "gb"), game("Chrono Trigger", "snes"), game("Chrono Trigger DS", "nds")];
        let sessions = vec![
            session(0, at(2026, 10, 2, 18, 0), 7200),
            session(1, at(2026, 10, 3, 18, 0), 3600),
            session(2, at(2026, 10, 4, 18, 0), 3600),
            session(3, at(2026, 10, 5, 18, 0), 9000),
            session(4, at(2026, 10, 6, 18, 0), 1800),
            session(5, at(2026, 10, 7, 18, 0), 3000),
        ];
        let log = build(games, sessions, Vec::new(), Vec::new());
        let m = summarise(&log, Period::of(Span::Month, today), today, &Tags);
        assert_eq!(m.leaned_into, "arcade racing · crash takedowns");
        assert_eq!(m.genres[0].label, "arcade racing");
        assert_eq!((m.genres[0].secs, m.genres[0].games), (10_800, 2));
        assert!(m.genres.iter().position(|g| g.label == "racing") > m.genres.iter().position(|g| g.label == "sim racing"), "families after particular tags");
        // The two Chrono Triggers are one game, shown as the version played more.
        let chrono: Vec<_> = m.games.iter().filter(|g| g.name.starts_with("Chrono")).collect();
        assert_eq!(chrono.len(), 1);
        assert_eq!((chrono[0].name.as_str(), chrono[0].secs, chrono[0].sessions), ("Chrono Trigger DS", 4800, 2));
        let plain = summarise(&log, Period::of(Span::Month, today), today, &Plain);
        assert_eq!(plain.games.len(), 6);
        assert!(plain.leaned_into.is_empty() && plain.genres.is_empty());
    }

    /// Review SOL-2026-10-24 #9: with Merge versions the game is one, but each console keeps the time played on it.
    #[test]
    fn merged_versions_leave_each_console_its_own_time() {
        let today = day_of(2026, 10, 23);
        let games = vec![game("Chrono Trigger", "snes"), game("Chrono Trigger DS", "nds"), game("Tetris", "gb")];
        let mut cut = session(1, at(2026, 10, 21, 18, 0), 0);
        cut.ended = false;
        let sessions = vec![session(0, at(2026, 10, 19, 18, 0), 1800), session(1, at(2026, 10, 20, 18, 0), 3000), cut, session(2, at(2026, 10, 22, 18, 0), 600)];
        let log = build(games, sessions, Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Tags);
        let chrono: Vec<_> = w.games.iter().filter(|g| g.name.starts_with("Chrono")).map(|g| (g.name.as_str(), g.secs, g.sessions)).collect();
        assert_eq!(chrono, [("Chrono Trigger DS", 4800, 3)], "one game, shown as the version played most");
        let systems: Vec<_> = w.systems.iter().map(|s| (s.system.as_str(), s.secs, s.sessions, s.untimed, s.games)).collect();
        assert_eq!(systems, [("nds", 3000, 2, 1, 1), ("snes", 1800, 1, 0, 1), ("gb", 600, 1, 0, 1)], "each console its own sessions");
        assert_eq!(w.systems.iter().map(|s| s.secs).sum::<u64>(), w.secs);
        // The review's console card: the share of the console, not of the merged game.
        let r = review(&log, Period::of(Span::Week, today), today, &Tags);
        let top = r.cards.iter().find_map(|c| if let Card::TopSystem { system, percent, only, .. } = c { Some((system.clone(), *percent, *only)) } else { None });
        assert_eq!(top, Some(("nds".to_string(), 56, false)));
    }

    /// Review SOL-2026-10-24 #14: two different logs with the same counts and the same last session are two
    /// histories, not one from the cache.
    #[test]
    fn the_read_api_cache_tells_logs_apart() {
        let now = at(2026, 10, 23, 12, 0);
        let last = session(1, at(2026, 10, 22, 8, 0), 600);
        let a = build(vec![game("Burnout 3", "ps2"), game("Tetris", "gb")], vec![session(0, at(2026, 10, 19, 20, 0), 3600), last.clone()], Vec::new(), Vec::new());
        let b = build(vec![game("Zelda", "gba"), game("Tetris", "gb")], vec![session(0, at(2026, 10, 20, 20, 0), 1200), last], Vec::new(), Vec::new());
        let (va, vb) = (stats_value(&a, now, TZ).unwrap(), stats_value(&b, now, TZ).unwrap());
        assert_eq!((va["week"]["secs"].as_u64(), vb["week"]["secs"].as_u64()), (Some(4200), Some(1800)));
        assert_eq!(vb["week"]["games"][0]["name"], "Zelda");
        assert_eq!(stats_value(&a, now, TZ).unwrap(), va, "and each one again");
        // The same log in another time zone is another day's count.
        let east = stats_value(&a, at(2026, 10, 25, 23, 30), TZ + 3600).unwrap();
        assert_eq!(east["today"], "2026-10-26");
    }

    #[test]
    fn unended_sessions_and_an_unset_clock() {
        let today = day_of(2026, 10, 23);
        let mut crashed = session(0, at(2026, 10, 20, 18, 0), 0);
        crashed.ended = false;
        let unset = session(0, 86_400 * 10, 3600); // 1970: the clock was never set
        let log = build(vec![game("Tetris", "gb")], vec![crashed, unset], Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((w.sessions, w.secs, w.days_played), (1, 0, 1), "a session without time, on its day");
        assert!(w.longest.is_none(), "no length to name");
        assert_eq!(all_time(&log).secs, 3600, "the undated hour counts in all time only");
        assert_eq!(log.counting_since(), Some(at(2026, 10, 20, 18, 0)));
    }

    /// VDEV-ROUND-2026-10-25: a start that failed at once is closed with no time (`play_log::never_started`); it is
    /// no session, no session "without time", no day played and no game of the week. Six refused zips and patches
    /// read "6 without time" in History before.
    #[test]
    fn a_start_that_never_was_on_screen_counts_for_nothing() {
        let today = day_of(2026, 10, 23);
        let refused = session(1, at(2026, 10, 22, 19, 0), 0);
        let log = build(vec![game("Tetris", "gb"), game("Pipes", "snes")], vec![session(0, at(2026, 10, 21, 18, 0), 600), refused], Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((w.sessions, w.untimed, w.secs, w.days_played), (1, 0, 600, 1));
        assert_eq!(w.games.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Tetris"]);
        assert_eq!(w.systems.len(), 1, "nor a console played");
    }

    #[test]
    fn a_review_says_only_what_is_there() {
        let today = day_of(2026, 12, 10);
        let games = vec![game("Burnout 3", "ps2"), game("Tetris", "gb")];
        let mut sessions = vec![session(0, at(2026, 3, 1, 10, 0), 7200), session(1, at(2026, 3, 2, 10, 0), 600), session(0, at(2026, 3, 3, 10, 0), 9000)];
        sessions.push(session(1, at(2026, 11, 20, 10, 0), 600));
        let log = build(games, sessions, Vec::new(), Vec::new());
        let r = review(&log, Period::of(Span::Year, today), today, &Tags);
        assert_eq!(r.title, "2026");
        let kinds: Vec<&str> = r
            .cards
            .iter()
            .map(|c| match c {
                Card::Overview { .. } => "overview",
                Card::TopGame { .. } => "top game",
                Card::TopSystem { .. } => "top system",
                Card::Genres { .. } => "genres",
                Card::First { .. } => "first",
                Card::Longest { .. } => "longest",
                Card::CameBack { .. } => "came back",
                Card::Completed { .. } => "completed",
                Card::Streak { .. } => "streak",
                Card::Next { .. } => "next",
            })
            .collect();
        assert_eq!(kinds, ["overview", "top game", "top system", "genres", "first", "longest", "came back", "streak"], "Tetris came back in November after 263 days");
        match &r.cards[1] {
            Card::TopGame { game, percent } => assert_eq!((game.name.as_str(), *percent), ("Burnout 3", 93)),
            c => panic!("{c:?}"),
        }
        let empty = review(&log, Period::of(Span::Month, today), today, &Tags);
        assert_eq!(empty.title, "December 2026");
        assert_eq!(empty.cards.len(), 1, "a month with nothing: the overview only");
        // Serialises for the web companion.
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"card\":\"top_game\"") && json.contains("\"span\":\"year\""), "{json}");
    }

    #[test]
    fn durations_read_calmly() {
        assert_eq!(duration(30), "under a minute");
        assert_eq!(duration(12 * 60 + 5), "12 min");
        assert_eq!(duration(3 * 3600 + 5 * 60), "3 h 05 min");
        assert_eq!(duration(41 * 3600 + 20 * 60), "41 h");
        assert_eq!(date(day_of(2026, 3, 14)), "14 March");
        assert_eq!(gap(63), "9 weeks");
        assert_eq!(gap(800), "2 years");
    }

    /// Five years of eight sessions a day over 400 games (about 14,600 sessions): every view stays well under the
    /// 50 ms budget on the Mini V2 (this machine is 2-3 times faster). The pages work them out on a thread anyway.
    #[test]
    fn five_years_of_fake_history_summarise_quickly() {
        let log = fake_years(5, 8, 400);
        let today = local_day(log.sessions.last().unwrap().start, TZ);
        let t = std::time::Instant::now();
        for span in Span::ALL {
            let s = summarise(&log, Period::of(span, today), today, &Tags);
            assert!(!s.is_empty());
        }
        let r = review(&log, Period::of(Span::Year, today), today, &Tags);
        assert!(r.cards.len() >= 5);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("5 years ({} sessions): three periods and a review in {ms:.1} ms", log.sessions.len());
        if !cfg!(debug_assertions) {
            assert!(ms < 40.0, "{ms:.1} ms");
        }
    }

    #[test]
    fn a_clock_that_was_ahead_puts_nothing_in_the_future() {
        let today = day_of(2026, 10, 23);
        let ahead = session(0, at(2027, 3, 1, 18, 0), 3600); // the clock said 2027, then the network put it right
        let log = build(vec![game("Tetris", "gb")], vec![session(0, at(2026, 10, 22, 18, 0), 600), ahead], Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((w.secs, w.current_streak), (600, 1), "yesterday's streak still counts");
        assert!(summarise(&log, Period::of(Span::Year, day_of(2027, 3, 1)), today, &Plain).is_empty());
        assert_eq!(all_time(&log).secs, 4200, "its time is in all time");
        let now = at(2026, 10, 23, 12, 0);
        assert_eq!(secs_since(&log, at(2026, 10, 19, 0, 0), now), 600);
    }

    #[test]
    fn a_session_without_an_end_has_an_unknown_time_not_a_short_one() {
        let today = day_of(2026, 10, 23);
        let mut cut = session(0, at(2026, 10, 23, 10, 0), 0);
        cut.ended = false; // pocket-ui was ended while the game ran
        let log = build(vec![game("uCity", "gbc")], vec![cut.clone()], Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((w.sessions, w.untimed, w.secs, w.days_played), (1, 1, 0, 1));
        assert_eq!((w.games[0].untimed, w.systems[0].untimed), (1, 1));
        assert_eq!(w.bars[4].untimed, 1, "Friday: played, time unknown");
        assert_eq!(time_text(w.games[0].secs, w.games[0].untimed, false), "time not recorded");
        assert_eq!(sessions_text(w.sessions, w.untimed, w.secs), "1 session, time not recorded");
        // Known and unknown together: the known total, and how many have no time.
        let log = build(vec![game("uCity", "gbc"), game("Tetris", "gb")], vec![session(1, at(2026, 10, 22, 10, 0), 3900), session(0, at(2026, 10, 21, 10, 0), 600), cut], Vec::new(), Vec::new());
        let w = summarise(&log, Period::of(Span::Week, today), today, &Plain);
        assert_eq!((w.sessions, w.untimed, w.secs), (3, 1, 4500));
        assert_eq!(time_text(w.secs, w.untimed, true), "1 h 15 min, 1 session without time");
        assert_eq!(time_text(w.secs, w.untimed, false), "1 h 15 min, 1 without time");
        assert_eq!(sessions_text(w.sessions, w.untimed, w.secs), "3 sessions, 1 without time");
        let ucity = w.games.iter().find(|g| g.name == "uCity").unwrap();
        assert_eq!(time_text(ucity.secs, ucity.untimed, false), "10 min, 1 without time");
        assert_eq!(time_text(600, 0, false), "10 min");
        assert_eq!(played_since(&log, at(2026, 10, 19, 0, 0), at(2026, 10, 23, 12, 0)), (4500, 1));
        assert_eq!(all_time(&log).untimed, 1);
        // The web companion gets the count beside the time.
        let v = stats_value(&log, at(2026, 10, 23, 12, 0), TZ).unwrap();
        assert_eq!((v["week"]["secs"].as_u64(), v["week"]["untimed_sessions"].as_u64()), (Some(4500), Some(1)));
        let g = v["week"]["games"].as_array().unwrap().iter().find(|g| g["name"] == "uCity").unwrap().clone();
        assert_eq!((g["secs"].as_u64(), g["untimed_sessions"].as_u64()), (Some(600), Some(1)));
        assert_eq!(v["week"]["bars"][4]["untimed_sessions"], 1);
        assert_eq!(v["all_time"]["untimed_sessions"], 1);
        assert_eq!(v["recent"][0]["ended"], false);
    }

    #[test]
    fn the_read_api_for_the_web_companion() {
        // Exactly the web companion's `HistorySource`.
        let _source: fn(&std::path::Path) -> Option<serde_json::Value> = stats_json;
        let now = at(2026, 10, 23, 12, 0);
        assert_eq!(stats_value(&Log::default(), now, TZ), None, "no log, no history");
        let games = vec![game("Burnout 3", "ps2"), game("Tetris", "gb")];
        let sessions = vec![session(0, at(2026, 10, 19, 20, 0), 3600), session(1, at(2026, 10, 22, 8, 0), 600), session(0, at(2026, 3, 2, 10, 0), 9000)];
        let statuses = vec![StatusChange { game: 1, at: at(2026, 10, 22, 9, 0), tz: TZ, status: Status::Completed }];
        let carried = vec![Carried { game: 0, secs: 7200, sessions: 3, last_played: None }];
        let log = build(games.clone(), sessions.clone(), statuses, carried);
        let v = stats_value(&log, now, TZ).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["today"], "2026-10-23");
        assert_eq!(v["counting_since"], "2026-03-02");
        assert_eq!(v["damaged"], false);
        assert_eq!(v["all_time"]["secs"], 3600 + 600 + 9000 + 7200);
        assert_eq!(v["all_time"]["carried_secs"], 7200);
        let week = &v["week"];
        assert_eq!((week["label"].as_str(), week["first"].as_str(), week["last"].as_str()), (Some("This week"), Some("2026-10-19"), Some("2026-10-25")));
        assert_eq!((week["secs"].as_u64(), week["sessions"].as_u64(), week["days_played"].as_u64()), (Some(4200), Some(2), Some(2)));
        assert_eq!(week["games"][0]["name"], "Burnout 3");
        assert_eq!(week["games"][0]["path"], "/storage/roms/ps2/Burnout 3.rom");
        assert_eq!(week["bars"].as_array().unwrap().len(), 7);
        assert_eq!(week["bars"][0], serde_json::json!({ "first": "2026-10-19", "days": 1, "secs": 3600, "untimed_sessions": 0 }));
        assert_eq!(week["completed"][0]["name"], "Tetris");
        assert_eq!(week["completed"][0]["date"], "2026-10-22");
        assert_eq!(week["not_recorded"], false);
        assert_eq!(v["month"]["sessions"], 2);
        assert_eq!(v["year"]["sessions"], 3);
        assert_eq!(v["year"]["bars"].as_array().unwrap().len(), 53);
        assert_eq!(v["recent"].as_array().unwrap().len(), 3);
        assert_eq!(v["recent"][0]["name"], "Tetris", "newest first");
        assert_eq!(v["recent"][0]["date"], "2026-10-22");
        // Asked again: the same, without counting (the cache).
        assert_eq!(stats_value(&log, now, TZ).unwrap(), v);
        // A log whose records point past its games (it can't come from the file, but never a panic).
        let mut broken = log.clone();
        broken.sessions[0].game = 99;
        assert_eq!(stats_value(&broken, now, TZ), None);
        // The file side: no file, no history; a written file, its numbers.
        let d = TestDir::new("history-api");
        assert_eq!(stats_json(&d), None, "no data directory at all");
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(stats_json(&d), None, "no log yet");
        std::fs::write(d.join(crate::play_log::FILE), b"\0\0\0 not a play log").unwrap();
        assert_eq!(stats_json(&d), None, "a damaged file is no history, and no panic");
        std::fs::write(d.join(crate::play_log::FILE), crate::play_log::encode(&build(games, sessions, Vec::new(), Vec::new()))).unwrap();
        let v = stats_json(&d).unwrap();
        assert_eq!(v["all_time"]["sessions"], 3);
        assert!(serde_json::to_string(&v).unwrap().len() < 20_000, "small enough to send every time");
    }

    /// Made-up history: `per_day` sessions a day for `years` years, over `games` games on a few systems.
    pub(crate) fn fake_years(years: u32, per_day: u32, games: usize) -> Log {
        let systems = ["gba", "snes", "psx", "ps2", "gb", "nes", "md", "n64"];
        let g: Vec<Game> = (0..games).map(|i| game(&format!("Game {i:03}"), systems[i % systems.len()])).collect();
        let start = at(2026 - years as i32, 1, 1, 9, 0);
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        let mut sessions = Vec::new();
        for day in 0..(years as u64 * 365) {
            for k in 0..per_day as u64 {
                let game = (next() % games).min(next() % games);
                sessions.push(session(game, start + day * 86_400 + k * 5400, 300 + (next() % 5000) as u64));
            }
        }
        build(g, sessions, Vec::new(), Vec::new())
    }
}
