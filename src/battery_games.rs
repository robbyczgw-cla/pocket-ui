//! Battery per game: what each game has cost while it ran on battery, kept over restarts, and what that means for
//! the game running now ("About 3 h 20 min left in this game" in the pause menu) and on its card ("About 3 h 20 min
//! left, 18 % an hour", [`card_line`]).
//!
//! * [`Ledger`] is the data: per game (system, file name, emulator) the seconds it ran on battery and the points of
//!   charge it used meanwhile. It is fed by the battery sampler (`battery_history`), one pair of neighbouring
//!   samples at a time, under the same rules as the history's rates (`battery_estimate`): both on battery, no gap
//!   (the device slept) and no jump of the gauge between them, and the same game running at both. The gauge's whole
//!   numbers don't matter: the drops of a run add up to the run's whole drop, a point of noise up is taken back by
//!   the next point down.
//! * Old play counts less: once a game has [`CAP`] of time, its seconds and points are scaled down together, so the
//!   rate keeps its value but new play moves it (a new performance setting, an older battery).
//! * [`Ledger::rate`] answers for a game in this order: the game on this emulator, the game on any emulator, the
//!   other games of its system (this emulator first), every game played. Each needs [`MIN_SECS`] on battery and a
//!   drop of [`MIN_DROP`] points; with none of them there is nothing to say yet ("measuring"), never a guess.
//! * [`left`] turns a rate and the charge now into the line the pause menu shows ([`Left::line`]); [`card_line`] is
//!   the card's, which has no emulator to go by (the game on any emulator first) and gives the rate as well.
//!
//! The file (`battery-games.json`, a few kB) is written through `fsutil` once a minute while a game runs.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::battery::Sample;

pub const FILE: &str = "battery-games.json";
/// A level of [`Ledger::rate`] needs this much time on battery ...
pub const MIN_SECS: f64 = 20.0 * 60.0;
/// ... and a drop of this many points: two would be a rounding of the gauge.
pub const MIN_DROP: f64 = 3.0;
/// The most play one game keeps at full weight; beyond it the older play is scaled down.
pub const CAP: f64 = 4.0 * 3600.0;
/// The most games kept; the ones played longest ago go first.
pub const MAX_GAMES: usize = 300;

/// The game running: its system id ("ps2"), its file name and the emulator that plays it ("pocket-core").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Key {
    pub system: String,
    pub game: String,
    pub emulator: String,
}

impl Key {
    pub fn new(system: &str, rom: &Path, emulator: &str) -> Key {
        Key {
            system: system.to_string(),
            game: rom.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string(),
            emulator: emulator.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub system: String,
    pub game: String,
    pub emulator: String,
    /// Seconds on battery (scaled down past [`CAP`]).
    pub secs: f64,
    /// Points of charge used in them.
    pub drop: f64,
    /// When it last ran on battery (unix seconds).
    pub last: u64,
}

impl Entry {
    fn is(&self, k: &Key) -> bool {
        self.system == k.system && self.game == k.game && self.emulator == k.emulator
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub games: Vec<Entry>,
}

/// What a rate is based on, from the closest to the widest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// This game (on this emulator, or on any).
    Game,
    /// The other games of its system.
    System,
    /// Every game played.
    AllGames,
}

/// Points an hour while playing, what it is based on, and how much play it was measured over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    pub per_hour: f64,
    pub basis: Basis,
    pub secs: f64,
}

/// Sums of seconds and points, and the rate when they are enough.
fn rate_of<'a>(entries: impl Iterator<Item = &'a Entry>, basis: Basis) -> Option<Rate> {
    let (secs, drop) = entries.fold((0.0, 0.0), |(s, d), e| (s + e.secs, d + e.drop));
    (secs >= MIN_SECS && drop >= MIN_DROP).then(|| Rate {
        per_hour: drop * 3600.0 / secs,
        basis,
        secs,
    })
}

impl Ledger {
    pub fn load(data_dir: &Path) -> Ledger {
        crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        crate::fsutil::write_json(&data_dir.join(FILE), self)
    }

    /// `secs` on battery with `key` running, and the `drop` in points over them (negative when the gauge went up);
    /// `now` in unix seconds.
    pub fn add(&mut self, key: &Key, secs: u64, drop: i32, now: u64) {
        if key.game.is_empty() || secs == 0 {
            return;
        }
        let i = match self.games.iter().position(|e| e.is(key)) {
            Some(i) => i,
            None => {
                if self.games.len() >= MAX_GAMES {
                    // The game played longest ago makes room.
                    if let Some(old) = self.games.iter().enumerate().min_by_key(|(_, e)| e.last).map(|(i, _)| i) {
                        self.games.swap_remove(old);
                    }
                }
                self.games.push(Entry {
                    system: key.system.clone(),
                    game: key.game.clone(),
                    emulator: key.emulator.clone(),
                    secs: 0.0,
                    drop: 0.0,
                    last: now,
                });
                self.games.len() - 1
            }
        };
        let e = &mut self.games[i];
        e.secs += secs as f64;
        e.drop += drop as f64;
        e.last = now;
        if e.secs > CAP {
            let k = CAP / e.secs;
            e.secs *= k;
            e.drop *= k;
        }
    }

    /// The rate for `key`, from the closest measurement that is enough: this game on this emulator, this game, the
    /// other games of its system on this emulator, of its system, every game. None: nothing is enough yet.
    pub fn rate(&self, key: &Key) -> Option<Rate> {
        let game = |e: &&Entry| e.system == key.system && e.game == key.game;
        let system = |e: &&Entry| e.system == key.system;
        rate_of(self.games.iter().filter(|e| e.is(key)), Basis::Game)
            .or_else(|| rate_of(self.games.iter().filter(game), Basis::Game))
            .or_else(|| rate_of(self.games.iter().filter(system).filter(|e| e.emulator == key.emulator), Basis::System))
            .or_else(|| rate_of(self.games.iter().filter(system), Basis::System))
            .or_else(|| rate_of(self.games.iter(), Basis::AllGames))
    }
}

/// What the pause menu can say about the battery in this game.
#[derive(Clone, Debug, PartialEq)]
pub enum Left {
    /// On the charger (charging or full).
    Charging,
    /// Not enough play on battery anywhere yet.
    Measuring,
    /// Seconds until empty at the rate of `basis`.
    Time { secs: u64, basis: Basis },
}

/// How old the newest sample may be: the history samples once a minute; older means pocket-ui wasn't sampling.
const FRESH: u64 = crate::battery_history::GAP;

/// What there is to say for a game with `rate`, the charge being `now` (the newest sample) at `t`. None without a
/// battery reading of the last minutes: then the line says nothing rather than something old.
pub fn left(rate: Option<Rate>, now: Option<&Sample>, t: u64) -> Option<Left> {
    let s = now.filter(|s| t.saturating_sub(s.t) <= FRESH)?;
    if s.plugged() {
        return Some(Left::Charging);
    }
    Some(match rate {
        Some(r) if r.per_hour > 0.0 => Left::Time {
            secs: (s.capacity as f64 / r.per_hour * 3600.0) as u64,
            basis: r.basis,
        },
        _ => Left::Measuring,
    })
}

/// "About 3 h 20 min left", "More than a day left".
fn time_left(secs: u64) -> String {
    let about = crate::battery::about(secs);
    match about.strip_prefix("about ") {
        Some(rest) => format!("About {rest} left"),
        None => format!("{} left", capitalise(&about)),
    }
}

/// Whose measurement it is, when not this game's own: " (PS2 games)", " (all games)".
fn whose(basis: Basis, system: &str) -> String {
    match basis {
        Basis::Game => String::new(),
        Basis::System => format!(" ({system} games)"),
        Basis::AllGames => " (all games)".to_string(),
    }
}

/// "12 % an hour" ("under 1 % an hour" for very light games).
fn per_hour(rate: &Rate) -> String {
    if rate.per_hour < 1.0 {
        "under 1% an hour".to_string()
    } else {
        format!("{:.0}% an hour", rate.per_hour)
    }
}

impl Left {
    /// "About 3 h 20 min left in this game", "About 4 h left (PS2 games)", "About 5 h left (all games)", "Charging",
    /// "Battery: measuring this game…". `system` is what the player calls the system ("PS2").
    pub fn line(&self, system: &str) -> String {
        match self {
            Left::Charging => "Charging".to_string(),
            Left::Measuring => "Battery: measuring this game\u{2026}".to_string(),
            Left::Time { secs, basis: Basis::Game } => format!("{} in this game", time_left(*secs)),
            // Not this game's own measurement: whose it is, in brackets.
            Left::Time { secs, basis } => format!("{}{}", time_left(*secs), whose(*basis, system)),
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The card's Battery line for a game on the shelf, from the same measurements as the pause menu (`rate` is
/// [`Ledger::rate`] for the game, `now` the newest sample at `t`): "About 3 h 20 min left, 18 % an hour" (this
/// game's own, at the charge now), "About 4 h left (PS2 games)", "About 5 h left (all games)", "Measuring this game…"
/// (nothing measured enough yet); on the charger the rate alone ("About 18 % an hour", "About 15 % an hour (PS2
/// games)"). None without a battery reading and without a rate (a device without a battery): nothing to say there.
pub fn card_line(rate: Option<Rate>, now: Option<&Sample>, t: u64, system: &str) -> Option<String> {
    let measuring = || "Measuring this game\u{2026}".to_string();
    Some(match (left(rate, now, t), rate) {
        (Some(Left::Time { secs, basis: Basis::Game }), Some(r)) => format!("{}, {}", time_left(secs), per_hour(&r)),
        (Some(Left::Time { secs, basis }), _) => format!("{}{}", time_left(secs), whose(basis, system)),
        (Some(Left::Measuring), _) | (Some(Left::Charging), None) => measuring(),
        (Some(Left::Charging) | None, Some(r)) => {
            let rate = if r.per_hour < 1.0 { per_hour(&r) } else { format!("about {}", per_hour(&r)) };
            format!("{}{}", capitalise(&rate), whose(r.basis, system))
        }
        (None, None) => return None,
    })
}

/// One pair of samples for the ledger: Some((seconds, drop)) when `a` → `b` is play on battery the ledger counts
/// (both on battery, no gap or jump between them: `battery_estimate`'s rule).
pub fn pair(a: &Sample, b: &Sample) -> Option<(u64, i32)> {
    let ok = b.t > a.t && !a.plugged() && !b.plugged() && crate::battery_estimate::continuous(a, b);
    ok.then(|| (b.t - a.t, a.capacity as i32 - b.capacity as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const T0: u64 = 1_790_000_000 / 60 * 60;

    fn key(system: &str, game: &str, emulator: &str) -> Key {
        Key {
            system: system.into(),
            game: game.into(),
            emulator: emulator.into(),
        }
    }

    fn s(t: u64, capacity: u8, charging: bool) -> Sample {
        Sample {
            t,
            capacity,
            charging,
            full: false,
            mv: 3900,
            ma: 500,
            temp: None,
            tag: 0,
        }
    }

    /// Feeds the ledger as the sampler does: a sample a minute for `minutes` from `from`, the charge along `cap(m)`.
    fn play(l: &mut Ledger, k: &Key, from: u64, minutes: u64, cap: impl Fn(u64) -> f64, charging: impl Fn(u64) -> bool) -> u64 {
        let mut prev: Option<Sample> = None;
        for m in 0..=minutes {
            let x = s(from + m * 60, cap(m).round().clamp(0.0, 100.0) as u8, charging(m));
            if let Some((secs, drop)) = prev.as_ref().and_then(|p| pair(p, &x)) {
                l.add(k, secs, drop, x.t);
            }
            prev = Some(x);
        }
        from + minutes * 60
    }

    fn steady(per_hour: f64, start: f64) -> impl Fn(u64) -> f64 {
        move |m| start - per_hour * m as f64 / 60.0
    }

    #[test]
    fn a_steady_game_gets_its_own_rate_and_time_left() {
        let mut l = Ledger::default();
        let k = key("ps2", "Okage.chd", "armsx2");
        let end = play(&mut l, &k, T0, 60, steady(18.0, 90.0), |_| false);
        let r = l.rate(&k).unwrap();
        assert_eq!(r.basis, Basis::Game);
        assert!((r.per_hour - 18.0).abs() < 1.5, "{r:?}");
        // At 60 % that is 3 h 20 min.
        let left = left(Some(r), Some(&s(end, 60, false)), end).unwrap();
        let Left::Time { secs, basis: Basis::Game } = left else { panic!("{left:?}") };
        assert!((secs as f64 / 3600.0 - 60.0 / 18.0).abs() < 0.3, "{secs}");
        assert!(left.line("PS2").ends_with("left in this game"), "{}", left.line("PS2"));
    }

    #[test]
    fn the_gauges_noise_cancels_out() {
        // 12 points an hour, but every other minute the gauge reads a point high.
        let mut l = Ledger::default();
        let k = key("gba", "Minish.gba", "pocket-core");
        play(
            &mut l,
            &k,
            T0,
            90,
            |m| 90.0 - 12.0 * m as f64 / 60.0 + if m % 2 == 1 { 1.0 } else { 0.0 },
            |_| false,
        );
        let r = l.rate(&k).unwrap();
        assert!((r.per_hour - 12.0).abs() < 1.5, "{r:?}");
        // A flicker up and down without a real drain is no drain at all.
        let mut l = Ledger::default();
        play(&mut l, &k, T0, 60, |m| if m % 3 == 1 { 71.0 } else { 70.0 }, |_| false);
        assert!(l.rate(&k).is_none(), "{:?}", l.games);
    }

    #[test]
    fn a_short_session_is_not_enough_and_falls_back_to_the_system_then_to_all_games() {
        let mut l = Ledger::default();
        // Ten minutes of a new N64 game: too short for its own rate.
        let new = key("n64", "Mario Kart 64.z64", "pocket-core");
        play(&mut l, &new, T0, 10, steady(30.0, 80.0), |_| false);
        assert_eq!(l.rate(&new), None, "nothing is enough anywhere: measuring");
        let now = s(T0 + 600, 75, false);
        assert_eq!(left(l.rate(&new), Some(&now), now.t), Some(Left::Measuring));
        // Another N64 game was played for an hour: its rate stands in, said as such.
        play(&mut l, &key("n64", "Zelda.z64", "pocket-core"), T0 + 3600, 60, steady(24.0, 95.0), |_| false);
        let r = l.rate(&new).unwrap();
        assert_eq!(r.basis, Basis::System);
        assert!((r.per_hour - 24.0).abs() < 2.5, "{r:?}");
        let line = left(Some(r), Some(&now), now.t).unwrap().line("N64");
        assert!(line.ends_with(" left (N64 games)"), "{line}");
        // A GB game never played: every game played stands in.
        let gb = key("gb", "Tetris.gb", "pocket-core");
        let r = l.rate(&gb).unwrap();
        assert_eq!(r.basis, Basis::AllGames);
        assert!(left(Some(r), Some(&now), now.t).unwrap().line("Game Boy").ends_with(" left (all games)"));
        // Once the new game has its own 20 minutes and three points, it is its own.
        play(&mut l, &new, T0 + 3 * 3600, 30, steady(30.0, 80.0), |_| false);
        assert_eq!(l.rate(&new).unwrap().basis, Basis::Game);
    }

    #[test]
    fn the_same_game_on_another_emulator_counts_after_this_emulator() {
        let mut l = Ledger::default();
        let ra = key("psx", "Crash.chd", "retroarch");
        let own = key("psx", "Crash.chd", "pocket-core");
        play(&mut l, &ra, T0, 60, steady(20.0, 90.0), |_| false);
        assert_eq!(l.rate(&own).unwrap().basis, Basis::Game, "the game itself, though on RetroArch");
        play(&mut l, &own, T0 + 7200, 60, steady(10.0, 90.0), |_| false);
        assert!((l.rate(&own).unwrap().per_hour - 10.0).abs() < 1.5, "this emulator's own first");
        assert!((l.rate(&ra).unwrap().per_hour - 20.0).abs() < 1.5);
        let both = l.rate(&key("psx", "Crash.chd", "")).unwrap();
        assert!(
            (both.per_hour - 15.0).abs() < 1.5 && both.basis == Basis::Game,
            "the card: the game on any emulator, {both:?}"
        );
    }

    #[test]
    fn charging_and_sleeping_count_for_nothing() {
        let mut l = Ledger::default();
        let k = key("snes", "Zelda.sfc", "pocket-core");
        // Half an hour on battery at 12 an hour, then half an hour on the charger (the charge rises).
        play(
            &mut l,
            &k,
            T0,
            60,
            |m| if m <= 30 { 80.0 - 0.2 * m as f64 } else { 74.0 + (m - 30) as f64 },
            |m| m > 30,
        );
        let r = l.rate(&k).unwrap();
        assert!((r.secs - 1800.0).abs() < 1.0, "only the minutes on battery: {r:?}");
        assert!((r.per_hour - 12.0).abs() < 2.5, "{r:?}");
        // Asleep for five hours in the middle of the game, 20 points lost: not a rate.
        let mut l = Ledger::default();
        play(&mut l, &k, T0, 30, steady(12.0, 80.0), |_| false);
        play(&mut l, &k, T0 + 30 * 60 + 5 * 3600, 30, steady(12.0, 54.0), |_| false);
        let mut prev = s(T0 + 30 * 60, 74, false);
        let woke = s(prev.t + 5 * 3600, 54, false);
        assert_eq!(pair(&prev, &woke), None, "across the sleep");
        prev.t += 120;
        assert_eq!(pair(&prev, &s(prev.t + 60, 60, false)), None, "a jump of the gauge without a gap");
        let r = l.rate(&k).unwrap();
        assert!((r.secs - 3600.0).abs() < 1.0 && (r.per_hour - 12.0).abs() < 2.5, "{r:?}");
        // On the charger now: the menu says so, whatever the rate.
        let plugged = s(T0, 50, true);
        assert_eq!(left(Some(r), Some(&plugged), T0), Some(Left::Charging));
        assert_eq!(Left::Charging.line("SNES"), "Charging");
        let full = Sample {
            charging: false,
            full: true,
            ..plugged
        };
        assert_eq!(left(None, Some(&full), T0), Some(Left::Charging));
    }

    #[test]
    fn without_a_recent_reading_nothing_is_said() {
        let r = Rate {
            per_hour: 10.0,
            basis: Basis::Game,
            secs: 3600.0,
        };
        assert_eq!(left(Some(r), None, T0), None, "no battery");
        assert_eq!(left(Some(r), Some(&s(T0 - 3600, 50, false)), T0), None, "the newest sample is an hour old");
        assert!(left(Some(r), Some(&s(T0 - 60, 50, false)), T0).is_some());
    }

    #[test]
    fn old_play_counts_less_so_a_new_setting_shows() {
        let mut l = Ledger::default();
        let k = key("ports", "Deltarune.sh", "portmaster");
        // Ten hours at 20 an hour, then two at 10 (Battery chosen): the rate leans to the new one.
        let mut t = T0;
        for _ in 0..10 {
            t = play(&mut l, &k, t, 60, steady(20.0, 90.0), |_| false) + 3600;
        }
        assert!(l.games[0].secs <= CAP + 1.0);
        for _ in 0..2 {
            t = play(&mut l, &k, t, 60, steady(10.0, 90.0), |_| false) + 3600;
        }
        // All twelve hours alike would be 18.3; with the older play scaled down the two new hours weigh about 40 %.
        let r = l.rate(&k).unwrap();
        assert!(r.per_hour < 17.0 && r.per_hour > 13.0, "{r:?}");
    }

    #[test]
    fn the_ledger_keeps_its_size_and_survives_a_restart_and_a_damaged_file() {
        let d = TestDir::new("batgames");
        let mut l = Ledger::default();
        for i in 0..(MAX_GAMES + 20) {
            l.add(&key("gb", &format!("game{i}.gb"), "pocket-core"), 60, 1, T0 + i as u64);
        }
        assert_eq!(l.games.len(), MAX_GAMES);
        assert!(!l.games.iter().any(|e| e.game == "game0.gb"), "the one played longest ago went");
        assert!(l.games.iter().any(|e| e.game == format!("game{}.gb", MAX_GAMES + 19)));
        l.save(&d).unwrap();
        assert_eq!(Ledger::load(&d), l);
        let whole = std::fs::read(d.join(FILE)).unwrap();
        std::fs::write(d.join(FILE), &whole[..whole.len() / 2]).unwrap();
        assert_eq!(Ledger::load(&d), l, "cut off: the backup");
        assert_eq!(Ledger::load(&d.join("nothing")), Ledger::default());
        // Nameless games aren't kept.
        let mut e = Ledger::default();
        e.add(&Key::default(), 60, 1, T0);
        assert!(e.games.is_empty());
    }

    #[test]
    fn lines_read_as_sentences() {
        let t = |secs, basis| Left::Time { secs, basis };
        assert_eq!(t(3 * 3600 + 17 * 60, Basis::Game).line("PS2"), "About 3 h 20 min left in this game");
        assert_eq!(t(4 * 3600, Basis::System).line("PS2"), "About 4 h left (PS2 games)");
        assert_eq!(t(30 * 3600, Basis::AllGames).line("GBA"), "More than a day left (all games)");
        assert_eq!(t(25 * 60, Basis::Game).line("GBA"), "About 25 min left in this game");
        assert_eq!(Left::Measuring.line("GBA"), "Battery: measuring this game\u{2026}");
        let r = Rate {
            per_hour: 12.2,
            basis: Basis::Game,
            secs: 2.0 * 3600.0,
        };
        let on_battery = s(T0, 60, false);
        let charging = s(T0, 60, true);
        let line = |r: Option<Rate>, now: Option<&Sample>| card_line(r, now, T0, "GBA");
        // 60 % at 12.2 an hour: 4 h 55 min, said to the ten minutes.
        assert_eq!(line(Some(r), Some(&on_battery)).as_deref(), Some("About 5 h left, 12% an hour"));
        assert_eq!(
            line(
                Some(Rate {
                    per_hour: 15.0,
                    basis: Basis::System,
                    ..r
                }),
                Some(&on_battery)
            )
            .as_deref(),
            Some("About 4 h left (GBA games)")
        );
        assert_eq!(
            line(
                Some(Rate {
                    per_hour: 15.0,
                    basis: Basis::AllGames,
                    ..r
                }),
                Some(&on_battery)
            )
            .as_deref(),
            Some("About 4 h left (all games)")
        );
        // On the charger the time left means nothing: the rate alone.
        assert_eq!(line(Some(r), Some(&charging)).as_deref(), Some("About 12% an hour"));
        assert_eq!(line(Some(Rate { per_hour: 0.6, ..r }), Some(&charging)).as_deref(), Some("Under 1% an hour"));
        assert_eq!(
            line(Some(Rate { basis: Basis::System, ..r }), Some(&charging)).as_deref(),
            Some("About 12% an hour (GBA games)")
        );
    }

    #[test]
    fn a_card_without_measurements_says_so_and_a_pocket_without_a_battery_says_nothing() {
        // A first start: no game played on battery yet. The card says it is measuring, never a guess or a gap.
        let l = Ledger::default();
        let minish = Key::new("gba", Path::new("/storage/roms/gba/Minish.gba"), "");
        assert_eq!(
            card_line(l.rate(&minish), Some(&s(T0, 80, false)), T0, "GBA").as_deref(),
            Some("Measuring this game\u{2026}")
        );
        assert_eq!(
            card_line(l.rate(&minish), Some(&s(T0, 80, true)), T0, "GBA").as_deref(),
            Some("Measuring this game\u{2026}"),
            "on the charger too"
        );
        // A desktop or a Pocket whose battery can't be read: no line at all, rather than "measuring" forever.
        assert_eq!(card_line(l.rate(&minish), None, T0, "GBA"), None);
        // The card has no emulator to go by: the game on any emulator is its own measurement.
        let mut l = Ledger::default();
        play(&mut l, &key("gba", "Minish.gba", "retroarch"), T0, 60, steady(12.0, 90.0), |_| false);
        assert_eq!(l.rate(&minish).unwrap().basis, Basis::Game);
        assert_eq!(Key::new("gb", Path::new("/storage/roms/gb/Tetris.gb"), "pocket-core").game, "Tetris.gb");
    }
}
