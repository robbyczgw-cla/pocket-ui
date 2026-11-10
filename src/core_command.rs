//! The existing line FIFO grammar, decoded once before dispatch. Unknown input remains a no-op.
//! Keep its permissive numeric defaults and literal-space payloads: old and new processes overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    /// Battery save and auto-resume state (also window close, SIGTERM and SELECT + START).
    Save,
    /// Battery save only: the existing `quit nosave` wire command.
    SkipResume,
    /// Save, then announce the handoff result: the existing `quit handoff` command.
    ForSwitch,
}

impl Quit {
    pub fn from_command(words: &[&str]) -> Option<Self> {
        match words {
            ["quit"] => Some(Self::Save),
            ["quit", "nosave"] => Some(Self::SkipResume),
            ["quit", "handoff"] => Some(Self::ForSwitch),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Load<'a> { Slot(Option<usize>), Named(&'a str), Earlier(&'a str, Option<u64>) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    Unknown, AudioStats, Pause, Resume, Sleep, ScreenOff, Wake, Autosave,
    PixelPath(u32), Shot(u32), Preview(u32, u32, u32), Save(Option<usize>), Load(Load<'a>),
    Restart, Slot(usize), Reload, Players, Cheats, Look(i8, i8, u16), Note(&'a str), Osd(&'a str),
    Disc(&'a str), Press(u16, u64), AchievementDemo, Peek(usize), Axis(crate::pad::Axis, f32),
    Option(&'a str, &'a str), Control(&'a str, &'a str), Set(&'a str, &'a str), Quit(Quit),
}

impl<'a> Command<'a> {
    pub fn parse(line: &'a str, test: bool) -> Self {
        let cmd = line.trim();
        let words: Vec<&str> = cmd.split_whitespace().collect();
        let slot = || words.get(1).and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1)).filter(|n| *n < crate::slots::SLOTS);
        let rest = |prefix: &str| cmd.strip_prefix(prefix).and_then(|r| r.split_once(' '))
            .map(|(k, v)| (k, v.trim()));
        match words.as_slice() {
            ["audio", "stats"] => Self::AudioStats,
            ["pause"] => Self::Pause, ["resume"] => Self::Resume, ["sleep"] => Self::Sleep,
            ["screen", "off"] => Self::ScreenOff, ["wake"] => Self::Wake, ["autosave"] => Self::Autosave,
            ["pixelpath", seq] => Self::PixelPath(seq.parse().unwrap_or(0)),
            ["shot", seq] => Self::Shot(seq.parse().unwrap_or(0)),
            ["preview", ..] => {
                let n: Vec<u32> = words[1..].iter().filter_map(|v| v.parse().ok()).collect();
                match n.as_slice() { [w, h, more @ ..] => Self::Preview(*w, *h, more.first().copied().unwrap_or(0)), _ => Self::Unknown }
            }
            ["save", ..] => Self::Save(slot()),
            ["load", name @ ("auto" | "undo")] => Self::Load(Load::Named(name)),
            ["restart"] => Self::Restart,
            ["load", name, listed @ ..] if crate::timeline::Name::parse(name).is_some() =>
                Self::Load(Load::Earlier(name, listed.first().and_then(|t| t.parse().ok()))),
            ["load", ..] => Self::Load(Load::Slot(slot())),
            ["slot", ..] => slot().map(Self::Slot).unwrap_or(Self::Unknown),
            ["reload"] => Self::Reload, [crate::players::COMMAND] => Self::Players, ["cheats"] => Self::Cheats,
            ["look", w, c, rest @ ..] => Self::Look(w.parse().unwrap_or(0), c.parse().unwrap_or(0), rest.first().and_then(|n| n.parse().ok()).unwrap_or(0)),
            ["note", ..] => cmd.strip_prefix("note ").map(|s| Self::Note(s.trim())).unwrap_or(Self::Unknown),
            ["osd", ..] => cmd.strip_prefix("osd ").map(|s| Self::Osd(s.trim())).unwrap_or(Self::Unknown),
            ["disc", which] => Self::Disc(which),
            ["press", name, ..] if test => {
                let names = ["B", "Y", "SELECT", "START", "UP", "DOWN", "LEFT", "RIGHT", "A", "X", "L", "R", "L2", "R2", "L3", "R3"];
                names.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|bit|
                    Self::Press(1 << bit, words.get(2).and_then(|m| m.parse().ok()).unwrap_or(100))).unwrap_or(Self::Unknown)
            }
            ["achievement-demo"] if test => Self::AchievementDemo,
            ["peek", at] if test => Self::Peek(usize::from_str_radix(at.trim_start_matches("0x"), 16).unwrap_or(0)),
            ["axis", name, value] if test => {
                use crate::pad::Axis;
                let axis = match *name { "lx" => Some(Axis::LeftX), "ly" => Some(Axis::LeftY), "rx" => Some(Axis::RightX), "ry" => Some(Axis::RightY), _ => None };
                match (axis, value.parse::<f32>()) { (Some(a), Ok(v)) => Self::Axis(a, v), _ => Self::Unknown }
            }
            ["opt", ..] => rest("opt ").map(|(k,v)| Self::Option(k,v)).unwrap_or(Self::Unknown),
            ["ctl", ..] => rest("ctl ").map(|(k,v)| Self::Control(k,v)).unwrap_or(Self::Unknown),
            ["set", key, value] => Self::Set(key,value),
            ["quit", ..] => Quit::from_command(&words).map(Self::Quit).unwrap_or(Self::Unknown),
            _ => Self::Unknown,
        }
    }

    /// The FIFO reader interrupts a long frame only for these existing urgent requests.
    pub fn urgent(self) -> bool { matches!(self, Self::Pause | Self::Quit(_)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn core_command_legacy_numeric_defaults_and_arity() {
        for (line, want) in [("save", Command::Save(None)), ("save 0", Command::Save(None)),
            ("save 8 extra", Command::Save(Some(7))), ("load auto", Command::Load(Load::Named("auto"))),
            ("load auto extra", Command::Load(Load::Slot(None))), ("slot 2 extra", Command::Slot(1)),
            ("preview bad 640 480 ignored 7 99", Command::Preview(640,480,7)),
            ("shot invalid", Command::Shot(0)), ("shot 4294967296", Command::Shot(0)), ("shot 1 extra", Command::Unknown),
            ("look 999 -2 bad extra", Command::Look(0,-2,0)), ("set frame x extra", Command::Unknown)] {
            assert_eq!(Command::parse(line, false), want, "{line}");
        }
    }
    #[test]
    fn core_command_existing_control_words_and_timeline_names() {
        for (line, want) in [("pause", Command::Pause), ("resume",Command::Resume), ("sleep",Command::Sleep),
            ("screen off",Command::ScreenOff), ("wake",Command::Wake), ("autosave",Command::Autosave),
            ("reload",Command::Reload), ("restart",Command::Restart), (crate::players::COMMAND,Command::Players),
            ("cheats",Command::Cheats), ("audio\tstats",Command::AudioStats), ("disc next",Command::Disc("next")),
            ("disc bad",Command::Disc("bad")), ("pixelpath 7",Command::PixelPath(7)),
            ("load earlier3 42 extra",Command::Load(Load::Earlier("earlier3",Some(42)))),
            ("load started1 invalid",Command::Load(Load::Earlier("started1",None))),
            ("load earlier0 42",Command::Load(Load::Slot(None)))] {
            assert_eq!(Command::parse(line,false),want,"{line}");
            assert_eq!(Command::parse(line,false).urgent(),matches!(want,Command::Pause));
        }
        for line in ["resume extra", "sleep extra", "wake extra", "screen off extra", "autosave extra",
            "reload extra", "restart extra", "players extra", "cheats extra", "audio stats extra", "disc next extra"] {
            assert_eq!(Command::parse(line,false),Command::Unknown,"{line}");
        }
    }
    #[test]
    fn core_command_literal_space_payloads_are_unchanged() {
        for (line, want) in [("note  a  b  ", Command::Note("a  b")), ("note\ta", Command::Unknown),
            ("opt key value with spaces", Command::Option("key","value with spaces")),
            ("opt key\tvalue", Command::Unknown), ("opt  key value", Command::Option("","key value")),
            ("ctl A none", Command::Control("A","none")), ("osd a|b|1|0", Command::Osd("a|b|1|0"))] {
            assert_eq!(Command::parse(line,false), want, "{line}");
        }
    }
    #[test]
    fn core_command_quit_and_test_inputs_are_strict() {
        for (line, quit) in [("quit", Quit::Save),("quit\tnosave",Quit::SkipResume),("quit handoff",Quit::ForSwitch)] {
            assert_eq!(Command::parse(line,false), Command::Quit(quit));
            assert!(Command::parse(line,false).urgent());
        }
        for line in ["quit extra", "quit nosave extra", "pause extra", "press A 25", "peek 0x10", "achievement-demo", "axis lx 1"] {
            assert_eq!(Command::parse(line,false),Command::Unknown, "{line}");
        }
        assert_eq!(Command::parse("press a bad",true),Command::Press(256,100));
        assert_eq!(Command::parse("peek 0X10",true),Command::Peek(0));
        assert_eq!(Command::parse("peek 0x10",true),Command::Peek(16));
        assert!(matches!(Command::parse("axis lx NaN",true),Command::Axis(_,v) if v.is_nan()));
    }
}
