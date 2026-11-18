//! Fast-forward and rewind, as both players run them: pocket-core on Linux (`extras`, its frame loop) and Android's player
//! (`android/src/player.rs`). The same choices (Speed, rumble, fan › Fast-forward speed, Rewind), the same shortcuts
//! (HOME + R2 turns fast-forward on and off, HOME + L2 held plays backwards, `hotkeys`), the same history (`rewind`: one
//! state back per screen refresh, a minute at most) and the same lines when rewind can't run.
//!
//! Until 18.11.2026 this lived in `extras` alone and Android's player had neither (ROADMAP „Android-Player wie Karte").

/// How fast fast-forward runs (Speed, rumble, fan › Fast-forward speed, key `ff_speed`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FastForward {
    Times(u32),
    /// As many frames as fit into the time of one screen refresh.
    Max,
}

impl FastForward {
    pub fn parse(s: &str) -> Self {
        match s {
            "max" => Self::Max,
            s => s.parse::<u32>().ok().filter(|n| (2..=16).contains(n)).map_or(Self::Times(4), Self::Times),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Times(n) => format!("{n}x"),
            Self::Max => "max".into(),
        }
    }

    /// The setting's value (`ff_speed`).
    pub fn value(self) -> String {
        match self {
            Self::Times(n) => n.to_string(),
            Self::Max => "max".into(),
        }
    }
}

impl Default for FastForward {
    fn default() -> Self {
        Self::Times(4)
    }
}

/// The setting keys.
pub const FF_KEY: &str = "ff_speed";
pub const REWIND_KEY: &str = "rewind";

/// Rewind keeps at most this much memory and this many frames (a minute at 60 fps).
pub const REWIND_BYTES: usize = 48 << 20;
pub const REWIND_FRAMES: usize = 3600;

/// Fast-forward "max" stops starting emulated frames after this long per screen refresh.
pub const MAX_FF_BUDGET_MS: f32 = 12.0;
pub const MAX_FF_RUNS: u32 = 24;

/// What HOME + L2 says when rewind is switched off.
pub const REWIND_OFF: &str = "Rewind is off. Switch it on under Speed, rumble, fan.";

/// The badge in the top right corner while it runs: "REWIND", "FF 4x"; empty otherwise.
pub fn badge(back: bool, ff: Option<FastForward>) -> String {
    match (back, ff) {
        (true, _) => "REWIND".into(),
        (_, Some(speed)) => format!("FF {}", speed.label()),
        _ => String::new(),
    }
}

/// The badge's distance from the screen's corner (pocket-core's `overlay::MARGIN`).
pub const BADGE_MARGIN: i32 = 24;

/// The emulated frames of one screen refresh: `due` at normal speed, more while fast-forward is on.
pub fn runs(ff: Option<FastForward>, due: u32) -> u32 {
    match ff {
        None => due,
        Some(FastForward::Times(n)) => n,
        Some(FastForward::Max) => MAX_FF_RUNS,
    }
}

/// Whether frame `i` (0-based) of `runs` is the refresh's last: "As fast as possible" stops at its time budget (`spent_ms`
/// since the refresh began), and the frame expected to be the last one is shown.
pub fn last_run(ff: Option<FastForward>, i: u32, runs: u32, spent_ms: f32) -> bool {
    i + 1 >= runs || (ff == Some(FastForward::Max) && i >= 1 && spent_ms + spent_ms / i as f32 > MAX_FF_BUDGET_MS)
}

/// The rewind shortcut over one screen refresh (`turn`).
#[derive(Default)]
pub struct RewindHold {
    /// The shortcut was down on the previous refresh.
    pub was_held: bool,
    /// Frames stepped back during the current hold.
    pub rewound: u32,
}

/// What one refresh does with the rewind shortcut.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Turn {
    /// Step one state back this refresh.
    pub back: bool,
    /// Said when the shortcut goes down but rewind can't run (Hardcore, or Rewind off).
    pub notice: Option<&'static str>,
    /// The hold ended after this many frames back: the moment rewound to is the game's now (achievements start over).
    pub ended: Option<u32>,
}

impl RewindHold {
    /// `held`: HOME + L2 is down; `on`: Rewind is switched on; `hardcore`: RetroAchievements' Hardcore runs (no rewind).
    pub fn turn(&mut self, held: bool, on: bool, hardcore: bool) -> Turn {
        let mut t = Turn::default();
        if held && !self.was_held {
            if hardcore {
                t.notice = Some(crate::achievements::hardcore::NO_REWIND);
            } else if !on {
                t.notice = Some(REWIND_OFF);
            }
        }
        if self.was_held && !held && self.rewound > 0 {
            t.ended = Some(std::mem::take(&mut self.rewound));
        }
        self.was_held = held;
        t.back = held && on && !hardcore;
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_parse_as_the_setting_writes_them() {
        for v in ["2", "3", "4", "8", "max"] {
            assert_eq!(FastForward::parse(v).value(), v);
        }
        assert_eq!(FastForward::parse("nonsense"), FastForward::Times(4));
        assert_eq!(runs(None, 1), 1);
        assert_eq!(runs(Some(FastForward::Times(8)), 1), 8);
        assert_eq!(runs(Some(FastForward::Max), 1), MAX_FF_RUNS);
    }

    #[test]
    fn the_badge_reads_as_pocket_core_s() {
        assert_eq!(badge(true, Some(FastForward::Max)), "REWIND");
        assert_eq!(badge(false, Some(FastForward::Times(4))), "FF 4x");
        assert_eq!(badge(false, Some(FastForward::Max)), "FF max");
        assert_eq!(badge(false, None), "");
        #[cfg(target_os = "linux")]
        assert_eq!(BADGE_MARGIN, crate::overlay::MARGIN);
    }

    #[test]
    fn max_stops_at_its_budget() {
        let max = Some(FastForward::Max);
        assert!(!last_run(max, 1, MAX_FF_RUNS, 2.0));
        assert!(last_run(max, 2, MAX_FF_RUNS, 11.0), "11 ms for two frames: a third would pass 12 ms");
        assert!(last_run(Some(FastForward::Times(4)), 3, 4, 0.1));
        assert!(!last_run(Some(FastForward::Times(4)), 2, 4, 50.0), "a fixed speed runs all its frames");
    }

    #[test]
    fn rewind_hold_says_why_once_and_reports_its_end() {
        let mut h = RewindHold::default();
        let off = h.turn(true, false, false);
        assert_eq!((off.back, off.notice), (false, Some(REWIND_OFF)));
        assert_eq!(h.turn(true, false, false).notice, None, "said once per press");
        h.turn(false, true, false);
        assert_eq!(h.turn(true, false, true).notice, Some(crate::achievements::hardcore::NO_REWIND));
        h.turn(false, true, false);
        let on = h.turn(true, true, false);
        assert!(on.back && on.notice.is_none());
        h.rewound = 30;
        assert_eq!(h.turn(false, true, false).ended, Some(30));
        assert_eq!(h.rewound, 0);
    }
}
