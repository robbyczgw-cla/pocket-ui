//! The Options page of Android's player in the handhelds' pause menu (`pause_list`, a game from the Handheld view), laid
//! out as Linux's (`main.rs` `opt_entries`, `paint_options`): Picture ›, Emulator ›, Controls ›, Cheats ›, then
//! what stands under Speed, rumble, fan on Linux and the player has (Earlier states, Over the game), Keep for, Restore
//! defaults. The same names, the same order, the same lines under the rows; Picture has Resolution where Linux has it on
//! the same handheld (`render_scale`, the chip class from Android's system properties). What the player has no counterpart
//! for is left out, each row with its reason in the tests' `NOT_ON_ANDROID`; Screenshot, which Linux has as HOME + A,
//! stays as the page's last row.
//!
//! Until 17.11.2026 the page was a list of its own: "This game: Zoom, Screenshot" first (the focus landed on Zoom), then
//! every option of the emulator by section (the Retroid Pocket Mini V2 hand test: "the options look completely different
//! from Linux, with the useless Zoom at the top").
//!
//! The Picture page uses the Handheld view's own rows (`options::GROUPS`' Picture with `ANDROID_VALUES`: Screen scaling,
//! Look, Screen frame), so a choice reads the same in the view's This game page and in the game; the player keeps its
//! own indices (`picture::Settings`: scaling Fit/Integer/Stretch, frame None/Console/CRT TV, effect Sharp/Smooth/CRT/
//! Scanlines), and [`PictureRow`] translates.
//!
//! Android's player library has no `options`, `look` or `over_game` (Linux's): the words are written here, and the tests
//! (on Linux) hold them to Linux's, so a change there fails here until both read the same.

/// A row of the Options page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Picture,
    /// The emulator's own options (the core's), by section, with "Advanced (this emulator)" last.
    Emulator,
    /// The player's control rows (Hold to toggle, the Disc of a game of several).
    Controls,
    Zoom,
    Cheats,
    /// Off / On: a state every 5 minutes of play for the pause menu's Earlier (Linux: Speed, rumble, fan › Earlier states).
    Earlier,
    /// Off / FPS: the frame rate and the CPU in the top left corner (Linux: Speed, rumble, fan › Over the game).
    OverGame,
    /// Where the Picture and Over the game choices are kept: this game, or every game of the console.
    Keep,
    /// Back to the defaults for the console and the game; asks for a second A (Linux's Restore defaults).
    Restore,
    Screenshot,
}

/// What the game has: an emulator with options, control rows, cheats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Has {
    pub emulator: bool,
    pub controls: bool,
    pub cheats: bool,
}

/// The page's rows in Linux's order (`opt_entries`: the groups, Emulator and Controls after Picture, Cheats after Controls,
/// Speed, rumble, fan's rows the player has (Earlier states, Over the game), Zoom after the groups (0.5.1: no longer second,
/// under Picture), Keep for, Restore defaults), Screenshot last.
pub fn top(has: Has) -> Vec<Entry> {
    let mut v = vec![Entry::Picture];
    if has.emulator {
        v.push(Entry::Emulator);
    }
    if has.controls {
        v.push(Entry::Controls);
    }
    if has.cheats {
        v.push(Entry::Cheats);
    }
    v.extend([Entry::Earlier, Entry::OverGame, Entry::Zoom, Entry::Keep, Entry::Restore, Entry::Screenshot]);
    v
}

impl Entry {
    /// The row's name, Linux's words.
    pub fn label(self) -> &'static str {
        match self {
            Entry::Picture => "Picture",
            Entry::Emulator => "Emulator",
            Entry::Controls => "Controls",
            Entry::Zoom => "Zoom",
            Entry::Cheats => "Cheats",
            Entry::Earlier => "Earlier states",
            Entry::OverGame => "Over the game",
            Entry::Keep => "Keep for",
            Entry::Restore => "Restore defaults",
            Entry::Screenshot => "Screenshot",
        }
    }

    /// The game setting the row is on Linux's page (`options::GROUPS`), None for a page of its own or a row Linux has
    /// outside the groups (Keep for, Restore defaults).
    pub fn setting_key(self) -> Option<&'static str> {
        match self {
            Entry::Earlier => Some(crate::timeline_policy::KEY),
            Entry::OverGame => Some("over_game"),
            _ => None,
        }
    }

    /// Whether A opens a page (">" at the right), as Linux's groups.
    pub fn opens(self) -> bool {
        matches!(self, Entry::Picture | Entry::Emulator | Entry::Controls | Entry::Zoom | Entry::Cheats)
    }

    /// The line at the bottom left while the row is selected: Linux's (`paint_options`) where Linux has the row.
    pub fn desc(self, keep_game: bool) -> &'static str {
        match self {
            Entry::Picture => PICTURE_DESC,
            Entry::Emulator => "Settings of the emulator itself, such as colours and palettes.",
            Entry::Controls => "Which button does what, and the left stick.",
            Entry::Zoom => "The picture larger, to read small text. Move with the D-pad.",
            Entry::Cheats => "Infinite lives and the like, from the libretro cheat database.",
            Entry::Earlier => EARLIER_DESC,
            Entry::OverGame => OVER_GAME_DESC,
            Entry::Keep if keep_game => "Changes show at once and stay for this game. Left or right: every game on this console.",
            Entry::Keep => "Changes show at once and stay for every game on this console. Left or right: this game only.",
            Entry::Restore => RESTORE_DESC,
            Entry::Screenshot => "A picture of the game as it is now.",
        }
    }

    /// The heading of the page the row opens: "Options › Picture", as Linux's.
    pub fn heading(self) -> String {
        format!("Options \u{203a} {}", self.label())
    }
}

/// Over the game on Android: Off and FPS (the sensors and Pixel Path are Linux's).
pub const OVER_GAME_VALUES: [&str; 2] = ["off", "fps"];
/// The row's line: Linux's first line, what the player draws.
pub const OVER_GAME_DESC: &str = "FPS shows frames per second and CPU load in the top left corner.";

/// Earlier states' line, Linux's (`options`' row).
pub const EARLIER_DESC: &str = "Keeps a state every 5 minutes of play, half an hour back,\nand where each session began: Earlier in this menu.\nOff by default for N64, PSP, DS, Dreamcast and Saturn (big states).";

/// "Off" / "On": Earlier states' value, Linux's.
pub fn earlier_label(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// Restore defaults' line, and what it says after the first A and after the second: Linux's page uses these too.
pub const RESTORE_DESC: &str = "Removes the saved settings for this console and this game.";
pub const RESTORE_ASK: &str = "Press A again to restore the defaults. B keeps your settings.";
pub const RESTORE_DONE: &str = "Back to defaults.";

/// Linux's Picture group line names the orientation, which the player hasn't: the page says what it has.
pub const PICTURE_DESC: &str = "Size, look and frame.";

/// "Off" / "FPS": the value's label, Linux's.
pub fn over_game_label(on: bool) -> &'static str {
    if on {
        "FPS"
    } else {
        "Off"
    }
}

/// "This game" / "This console": Keep for's value, Linux's.
pub fn keep_label(game: bool) -> &'static str {
    if game {
        "This game"
    } else {
        "This console"
    }
}

/// One of the Picture page's rows: Linux's key, and the player's list it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureRow {
    Scaling,
    Look,
    Frame,
    /// Options → Picture → Resolution (`render_scale`): N64, PSP, Dreamcast and PlayStation on a handheld measured for it.
    /// Its index is the factor less one (Original 0, 2x 1 …); it steps with [`resolution_step`], up to the handheld's highest.
    Resolution,
}

/// The Picture page's rows every game has, in Linux's order (Screen scaling, Look, Screen frame).
pub const PICTURE_ROWS: [PictureRow; 3] = [PictureRow::Scaling, PictureRow::Look, PictureRow::Frame];

/// The Picture page of a game: [`PICTURE_ROWS`], and Resolution last where this handheld has it for the console
/// (`render_scale::here`, as Linux's page has it there).
pub fn picture_rows(resolution: Option<crate::render_scale::Range>) -> Vec<PictureRow> {
    let mut rows = PICTURE_ROWS.to_vec();
    if resolution.is_some() {
        rows.push(PictureRow::Resolution);
    }
    rows
}

/// Resolution's line, Linux's.
pub const RESOLUTION_DESC: &str = "The resolution the 3D picture is drawn at. Original is the console's own; 2x and up are sharper and cost the GPU more. Takes effect the next time the game starts.";

/// Resolution's index (factor less one) after one step Left or Right, round through the factors this handheld offers
/// (`render_scale::offers`: Original up to the highest that held full speed).
pub fn resolution_step(range: Option<crate::render_scale::Range>, index: usize, delta: i32) -> usize {
    let n = range.map_or(1, |r| i32::from(r.max.clamp(1, 4)));
    (index.min(3) as i32 + delta).rem_euclid(n) as usize
}

/// Resolution's index for a saved choice (`render_scale::VALUES`, "1" to "4"), the handheld's default where none is
/// saved or it is more than offered: what pocket-core gives the core at the start (`render_scale::for_start`).
pub fn resolution_index(range: Option<crate::render_scale::Range>, chosen: Option<u8>) -> usize {
    let Some(r) = range else { return 0 };
    let f = chosen.filter(|f| (1..=r.max).contains(f)).unwrap_or(r.default);
    usize::from(f.clamp(1, 4)) - 1
}

impl PictureRow {
    pub fn key(self) -> &'static str {
        match self {
            PictureRow::Scaling => "scaling",
            PictureRow::Look => "look",
            PictureRow::Frame => "frame",
            PictureRow::Resolution => crate::render_scale::KEY,
        }
    }

    /// Linux's value for each of the player's indices (`picture::Settings`: scaling Fit/Integer/Stretch, effect
    /// Sharp/Smooth/CRT/Scanlines, frame None/Console/CRT TV), as `android_handheld::picture_choice` reads them the other way.
    fn values(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["aspect", "native", "fullscreen"],
            PictureRow::Look => &["clean", "smooth", "oldtv", "scanlines"],
            PictureRow::Frame => &["none", "auto", "crt"],
            PictureRow::Resolution => &crate::render_scale::VALUES,
        }
    }

    /// The row's name: "Screen scaling", "Look", "Screen frame".
    pub fn name(self) -> &'static str {
        match self {
            PictureRow::Scaling => "Screen scaling",
            PictureRow::Look => "Look",
            PictureRow::Frame => "Screen frame",
            PictureRow::Resolution => "Resolution",
        }
    }

    /// The labels of `values`, as the Handheld view shows them ("Integer", "Old TV", "Console": `options::label_in`).
    fn labels(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["Aspect", "Integer", "Fullscreen"],
            PictureRow::Look => &["Clean", "Smooth", "Old TV", "Scanlines"],
            PictureRow::Frame => &["None", "Console", "CRT"],
            PictureRow::Resolution => &["Original", "2x", "3x", "4x"],
        }
    }

    /// The values in the order the Handheld view lists them (`options::ANDROID_VALUES`).
    fn view_order(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["native", "aspect", "fullscreen"],
            PictureRow::Look => &["clean", "smooth", "scanlines", "oldtv"],
            PictureRow::Frame => &["none", "auto", "crt"],
            PictureRow::Resolution => &crate::render_scale::VALUES,
        }
    }

    /// The value's label for the player's index.
    pub fn label(self, index: usize) -> &'static str {
        self.labels()[index.min(self.labels().len() - 1)]
    }

    /// The row's line, as the Handheld view's: Android's words for Screen scaling and Screen frame
    /// (`options::android_desc`), the Look's own line for each look (`look::explain`).
    pub fn desc(self, index: usize) -> &'static str {
        match self {
            PictureRow::Scaling => "Integer scales by whole numbers (integer scaling): every pixel the same size, the sharpest.\nAspect is as large as fits, in the console's shape.\nFullscreen fills the screen and stretches pixels.",
            PictureRow::Resolution => RESOLUTION_DESC,
            PictureRow::Frame => "Fills the space around the picture. Console draws the console's own frame.\nCRT TV puts an old tube TV around it.\nNone leaves just the picture.",
            PictureRow::Look => match self.values()[index.min(3)] {
                "smooth" => "Pixels stay sharp, but stair-steps on slanted edges get rounded off.",
                "oldtv" => "Scanlines plus the glow of a tube TV. The TV around it is a Screen frame.",
                "scanlines" => "Dark lines between the rows, like a TV. Fades out in hi-res menus.",
                _ => "Plain pixels, nothing added, all the same size at any scaling.",
            },
        }
    }

    /// The player's index after one step Left (-1) or Right (+1), in the order the Handheld view lists the values
    /// (Integer, Aspect, Fullscreen; Clean, Smooth, Scanlines, Old TV; None, Console, CRT), round. Resolution steps with
    /// [`resolution_step`], which knows the handheld's highest factor.
    pub fn step(self, index: usize, delta: i32) -> usize {
        let order = self.view_order();
        let value = self.values()[index.min(self.values().len() - 1)];
        let at = order.iter().position(|v| *v == value).unwrap_or(0) as i32;
        let next = order[(at + delta).rem_euclid(order.len() as i32) as usize];
        self.values().iter().position(|v| *v == next).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options;

    /// The page's rows stand in Linux's order and carry Linux's names; the player's extras (Screenshot) come after them.
    #[test]
    fn the_page_is_linux_s_order_with_linux_s_names() {
        let all = top(Has { emulator: true, controls: true, cheats: true });
        let names: Vec<&str> = all.iter().map(|e| e.label()).collect();
        assert_eq!(names, ["Picture", "Emulator", "Controls", "Cheats", "Earlier states", "Over the game", "Zoom", "Keep for", "Restore defaults", "Screenshot"]);
        assert_eq!(Entry::Picture.label(), options::GROUPS[0].name);
        assert_eq!(Entry::OverGame.label(), options::setting(crate::over_game::KEY).unwrap().name);
        // Zoom is never first and never the row the page opens on.
        for has in [Has::default(), Has { emulator: true, ..Has::default() }, Has { emulator: true, controls: true, cheats: false }] {
            let rows = top(has);
            assert_eq!(rows[0], Entry::Picture);
            assert!(rows.iter().position(|e| *e == Entry::Zoom).unwrap() > 0);
        }
        assert_eq!(Entry::Picture.heading(), "Options \u{203a} Picture");
        assert!(Entry::Picture.opens() && !Entry::OverGame.opens() && !Entry::Keep.opens() && !Entry::Screenshot.opens());
    }

    /// The Picture page reads as the Handheld view's own Options page on Android: Linux's row names, the values it offers in
    /// its order and with its labels, its lines.
    #[test]
    fn the_picture_rows_read_and_step_as_the_handheld_view_s() {
        for row in PICTURE_ROWS {
            let setting = options::setting(row.key()).unwrap();
            assert_eq!(row.name(), setting.name);
            let offered = options::ANDROID_VALUES.iter().find(|(k, _)| *k == row.key()).unwrap().1;
            assert_eq!(row.view_order(), offered, "{row:?}");
            let n = row.values().len();
            assert_eq!(n, offered.len());
            for i in 0..n {
                assert_eq!(row.label(i), options::label_in(setting, row.values()[i], true), "{row:?} {i}");
                let want = match row {
                    PictureRow::Look => crate::look::explain(crate::look::Look::from_value(row.values()[i]).unwrap(), false).to_string(),
                    _ => options::describe_in(setting, None, true),
                };
                assert_eq!(row.desc(i), want, "{row:?} {i}");
                assert_eq!(row.step(row.step(i, 1), -1), i, "{row:?} {i}");
            }
            // Right from the view's first value goes through the view's list in its order and comes back round.
            let mut at = row.values().iter().position(|v| *v == offered[0]).unwrap();
            for want in offered.iter().skip(1).chain(std::iter::once(&offered[0])) {
                at = row.step(at, 1);
                assert_eq!(row.values()[at], *want, "{row:?}");
            }
        }
    }

    /// The Handheld view hands the player its picture choice by the same table the other way (`picture_choice`): every value
    /// the view offers lands on the index this page shows for it.
    #[test]
    fn the_handheld_view_s_choice_shows_as_the_same_value_here() {
        for row in PICTURE_ROWS {
            let offered = options::ANDROID_VALUES.iter().find(|(k, _)| *k == row.key()).unwrap().1;
            for value in offered {
                let mut values = std::collections::BTreeMap::new();
                if row == PictureRow::Look {
                    let effect = match *value { "smooth" => "smooth", "scanlines" => "scanlines", "oldtv" => "crt", _ => "none" };
                    values.insert("sharpness".to_string(), "sharp".to_string());
                    values.insert("effect".to_string(), effect.to_string());
                } else {
                    values.insert(row.key().to_string(), value.to_string());
                }
                let choice = crate::android_handheld::picture_choice(&values);
                let field = match row { PictureRow::Scaling => "scaling", PictureRow::Look => "effect", PictureRow::Frame => "frame", PictureRow::Resolution => unreachable!("not one of PICTURE_ROWS") };
                let index = choice[field].as_u64().unwrap() as usize;
                assert_eq!(row.values()[index], *value, "{row:?} {value}");
            }
        }
    }

    #[test]
    fn over_the_game_and_keep_for_read_as_linux_s() {
        let over = options::setting(crate::over_game::KEY).unwrap();
        for (on, value) in [(false, "off"), (true, "fps")] {
            assert_eq!(over_game_label(on), over.choices.iter().find(|c| c.value == value).unwrap().label);
        }
        assert_eq!(OVER_GAME_VALUES, ["off", "fps"]);
        assert_eq!(OVER_GAME_DESC, over.desc.lines().next().unwrap());
        assert_eq!((keep_label(true), keep_label(false)), ("This game", "This console"));
        assert_eq!(Entry::OverGame.setting_key(), Some(crate::over_game::KEY));
    }

    #[test]
    fn earlier_states_and_restore_defaults_read_as_linux_s() {
        let earlier = options::setting(crate::timeline::KEY).unwrap();
        assert_eq!(Entry::Earlier.label(), earlier.name);
        assert_eq!(Entry::Earlier.desc(false), earlier.desc);
        for (on, value) in [(false, "off"), (true, "on")] {
            assert_eq!(earlier_label(on), earlier.choices.iter().find(|c| c.value == value).unwrap().label);
        }
        // The starting value is Linux's (On, Off for the consoles with big states), from the one place both read.
        for sys in ["gba", "snes", "psx", "n64", "psp", "nds", "dreamcast"] {
            let linux = options::console_default(sys, crate::timeline::KEY).map_or(earlier.choices[usize::from(earlier.default)].value, |v| v);
            assert_eq!(crate::timeline_policy::default_on(sys), linux == "on", "{sys}");
        }
        assert!(!Entry::Restore.opens() && !Entry::Earlier.opens());
        assert_eq!(Entry::Restore.desc(false), RESTORE_DESC);
    }

    /// Resolution on the Picture page: Linux's row (name, values, labels, line), only where this handheld has it for the
    /// console, stepping through what the handheld offers, starting on its default.
    #[test]
    fn resolution_reads_and_steps_as_linux_s_row() {
        use crate::render_scale::{range, Class, Range};
        let linux = options::setting(crate::render_scale::KEY).unwrap();
        let row = PictureRow::Resolution;
        assert_eq!((row.key(), row.name(), row.desc(0)), (linux.key, linux.name, linux.desc));
        for (i, c) in linux.choices.iter().enumerate() {
            assert_eq!((row.values()[i], row.label(i)), (c.value, c.label), "{i}");
        }
        assert_eq!(picture_rows(None), PICTURE_ROWS.to_vec(), "no row where the handheld wasn't measured");
        assert_eq!(picture_rows(range(Class::Sm8250, "n64")).last(), Some(&PictureRow::Resolution), "last, as on Linux");
        let r = Some(Range { default: 2, max: 3 });
        assert_eq!(resolution_index(r, None), 1, "nothing chosen: the default, 2x");
        assert_eq!(resolution_index(r, Some(1)), 0);
        assert_eq!(resolution_index(r, Some(4)), 1, "more than offered here: the default, as the start does");
        // Right from 2x: 3x, then round to Original; Left from Original: 3x. Never 4x where 3x is the highest.
        assert_eq!(resolution_step(r, 1, 1), 2);
        assert_eq!(resolution_step(r, 2, 1), 0);
        assert_eq!(resolution_step(r, 0, -1), 2);
        for i in 0..3 {
            for d in [-1, 1] {
                let next = resolution_step(r, i, d);
                assert!(crate::render_scale::offers(r, crate::render_scale::VALUES[next]), "{i} {d}");
            }
        }
        // Dreamcast (2x at most): Original and 2x only.
        let dc = range(Class::Sm8250, "dreamcast");
        assert_eq!((resolution_step(dc, 0, 1), resolution_step(dc, 1, 1)), (1, 0));
    }

    /// Linux's rows the player has no counterpart for, each with why. Every other row of Linux's Options page on the same
    /// handheld is on Android's page too (`android_s_page_has_linux_s_rows_on_the_same_handheld`).
    const NOT_ON_ANDROID: &[(&str, &str)] = &[
        // Picture: the player draws Fit/Integer/Stretch, four looks and the Console/CRT frames in their first variant.
        ("sharpness", "feature missing on Android: the player has no Custom look"),
        ("effect", "feature missing on Android: the player has no Custom look"),
        ("lcd_colour", "feature missing on Android: no LCD grid look in the player"),
        ("frame_colour", "feature missing on Android: frames only in their first variant"),
        ("frame_tv", "feature missing on Android: frames only in their first variant"),
        ("frame_brick", "feature missing on Android: frames only in their first variant"),
        ("frame_pocket", "feature missing on Android: frames only in their first variant"),
        ("frame_clamshell", "feature missing on Android: frames only in their first variant"),
        ("orientation", "feature missing on Android: the player doesn't turn the picture and the D-pad"),
        // Shortcuts: HOME + a button. Android owns Home; the player's pause menu opens on Back / Mode.
        ("hk_save", "Linux only: no HOME layer, Android owns Home"),
        ("hk_load", "Linux only: no HOME layer, Android owns Home"),
        ("hk_slot", "Linux only: no HOME layer, Android owns Home"),
        ("hk_ff", "Linux only: no HOME layer, Android owns Home"),
        ("hk_rewind", "Linux only: no HOME layer, Android owns Home"),
        ("hk_shot", "Linux only: no HOME layer, Android owns Home"),
        ("hk_over", "Linux only: no HOME layer, Android owns Home"),
        ("hk_zoom", "Linux only: no HOME layer, Android owns Home"),
        // Speed, rumble, fan.
        ("ff_speed", "feature missing on Android: the player has no fast-forward"),
        ("rewind", "feature missing on Android: the player has no rewind"),
        ("rumble", "feature missing on Android: the player sends no rumble"),
        ("run_ahead", "feature missing on Android: the player's loop has no run-ahead"),
        ("performance", "Linux only: Android owns the CPU's governor"),
        ("fan", "Linux only: Android owns the fan"),
        // Linux's page outside the groups.
        ("Runs on", "Linux only: on Android the emulator is chosen before the start (Plays with)"),
    ];

    /// Linux's Options page on a handheld of `class` for `system`: the groups' rows it lists there (`options::applies_in`,
    /// Resolution by the class's numbers, the fan on these Snapdragon handhelds), then the rows outside the groups
    /// (`main.rs` `opt_entries`: Emulator, Controls, Cheats, Zoom, Runs on, Keep for, Restore defaults).
    fn linux_rows(class: Option<crate::render_scale::Class>, system: &str) -> std::collections::BTreeSet<&'static str> {
        let mut rows = std::collections::BTreeSet::new();
        for setting in options::GROUPS.iter().flat_map(|g| g.settings.iter()) {
            let shown = match setting.key {
                crate::render_scale::KEY => class.and_then(|c| crate::render_scale::range(c, system)).is_some(),
                "fan" => class.is_some(),
                key => options::applies_in(key, system, false),
            };
            if shown {
                rows.insert(setting.key);
            }
        }
        rows.extend(["Emulator", "Controls", "Zoom", "Cheats", "Runs on", "Keep for", "Restore defaults"]);
        rows
    }

    /// Android's page for the same game, built as the player builds it (`top`, `picture_rows`), as setting keys where
    /// Linux has the row as a setting.
    fn android_rows(class: Option<crate::render_scale::Class>, system: &str) -> std::collections::BTreeSet<&'static str> {
        let mut rows = std::collections::BTreeSet::new();
        for entry in top(Has { emulator: true, controls: true, cheats: true }) {
            match entry {
                Entry::Picture => rows.extend(picture_rows(class.and_then(|c| crate::render_scale::range(c, system))).iter().map(|r| r.key())),
                Entry::Screenshot => (), // the player's own (Linux: HOME + A)
                e => {
                    rows.insert(e.setting_key().unwrap_or(e.label()));
                }
            }
        }
        rows
    }

    /// The class of rows, not one row: on an Android handheld of a chip class Linux has numbers for, the player's Options
    /// page has every row Linux's has on that chip, Resolution included, except `NOT_ON_ANDROID`; and nothing Linux lacks
    /// but Screenshot. Until 17.11.2026 Resolution was missing because Android never found the chip (no device tree in
    /// the app's sandbox), and Earlier states and Restore defaults for no reason.
    #[test]
    fn android_s_page_has_linux_s_rows_on_the_same_handheld() {
        use crate::render_scale::Class;
        let out: std::collections::BTreeSet<&str> = NOT_ON_ANDROID.iter().map(|(k, _)| *k).collect();
        for class in [Some(Class::Sm8250), Some(Class::Sm8550), None] {
            for sys in ["n64", "psp", "dreamcast", "psx", "gba", "gbc", "snes", "nes", "megadrive", "nds", "saturn"] {
                let linux = linux_rows(class, sys);
                let android = android_rows(class, sys);
                let missing: Vec<_> = linux.difference(&android).filter(|k| !out.contains(*k)).collect();
                assert!(missing.is_empty(), "{class:?} {sys}: Linux has {missing:?}, Android's page doesn't");
                let extra: Vec<_> = android.difference(&linux).collect();
                assert!(extra.is_empty(), "{class:?} {sys}: Android has {extra:?}, Linux's page doesn't");
                for key in &out {
                    assert!(!android.contains(key), "{key}: listed as not on Android, but the page has it");
                }
            }
        }
        // Resolution for the four 3D systems on both Snapdragon classes.
        for sys in ["n64", "psp", "dreamcast", "psx"] {
            assert!(android_rows(Some(Class::Sm8250), sys).contains(crate::render_scale::KEY), "{sys}");
        }
        // Every exception is a real Linux row of this handheld, with a reason.
        let all_linux: std::collections::BTreeSet<&str> = ["n64", "gba", "snes"].iter().flat_map(|s| linux_rows(Some(Class::Sm8250), s)).collect();
        for (key, why) in NOT_ON_ANDROID {
            assert!(all_linux.contains(key) || options::setting(key).is_some(), "{key}: not a Linux row");
            assert!(why.starts_with("feature missing on Android") || why.starts_with("Linux only"), "{key}: {why}");
        }
    }

    /// Every line under a row of the Options page reads whole on Android, never cut with "…" (the Mini V2 showed
    /// "Die Auflösung, in der das 3D-Bild gezeichnet wird. Original ist …", 17.11.): each row's line (both Keep for
    /// lines, every value's line of the Picture rows, Resolution, Restore's two answers and a long emulator option), in
    /// every language, on the Mini V2 at the densities it may report and on a small phone both ways, with each text size,
    /// is painted with all its words, below the selected row (`pause_list::paint_list`).
    #[test]
    fn every_row_s_line_reads_whole_on_every_screen_language_and_text_size() {
        use crate::pause_list::{paint_list, List, ListRow};
        use crate::text_size::TextSize;
        let mut descs: Vec<(String, bool, bool)> = Vec::new(); // (line, opens, has a value)
        for e in top(Has { emulator: true, controls: true, cheats: true }) {
            for keep in [true, false] {
                descs.push((e.desc(keep).to_string(), e.opens(), matches!(e, Entry::Earlier | Entry::OverGame | Entry::Keep)));
            }
        }
        for row in PICTURE_ROWS.into_iter().chain([PictureRow::Resolution]) {
            for i in 0..row.values().len() {
                descs.push((row.desc(i).to_string(), false, true));
            }
        }
        descs.push((RESTORE_ASK.to_string(), false, false));
        descs.push((RESTORE_DONE.to_string(), false, false));
        // An emulator option's own line, as long as the cores' longest ones the player shows.
        descs.push(("Renders the 3D picture at a multiple of the console's own resolution. Higher values are sharper but need a faster graphics chip; some games show seams or lines at the edges of their 2D pictures, and a few effects that read the picture back (blur, heat haze, the map in some games) only look right at the original resolution.".to_string(), false, true));
        // The overlay's size on these screens (`scaled_layout::mobile_size`, as the player lays it out).
        let screens = [(1240, 1080, 2.0), (1240, 1080, 2.5), (1240, 1080, 2.625), (1600, 720, 2.0), (720, 1600, 2.0), (2400, 1080, 2.625)];
        let rows: Vec<ListRow> = ["Picture", "Emulator", "Controls", "Earlier states", "Over the game", "Zoom", "Keep for", "Restore defaults", "Screenshot"]
            .iter().map(|l| ListRow::Row { label: (*l).into(), value: String::new(), opens: false, enabled: true }).collect();
        let mut text = crate::text::Text::builtin();
        let mut faults = Vec::new();
        for lang in crate::i18n::Lang::ALL {
            crate::i18n::set(lang);
            for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
                crate::type_scale::set(size);
                for (sw, sh, density) in screens {
                    let (w, h) = crate::scaled_layout::mobile_size((sw, sh), density);
                    let mut px = vec![0u32; (w * h) as usize];
                    let mut c = crate::gfx::Canvas::new(&mut px, w, h);
                    for (desc, opens, value) in &descs {
                        let hints: &[(&str, &str)] = if *opens { &[("B", "Back"), ("A", "Open")] } else if *value { &[("B", "Back"), ("< >", "Change")] } else { &[("B", "Back"), ("A", "OK")] };
                        for sel in [0, rows.len() - 1] {
                            let out = paint_list(&mut c, &mut text, &List { title: "Options", rows: &rows, sel, desc, hints });
                            let at = format!("{lang:?} {size:?} {sw}x{sh}@{density} ({w}x{h}) sel {sel}: {:?}", desc.chars().take(40).collect::<String>());
                            let painted: Vec<&str> = out.desc.iter().flat_map(|l| l.split_whitespace()).collect();
                            // Each paragraph (a line of the text) is translated on its own, as `kit::footer_lines` does.
                            let whole: Vec<String> = desc.lines().filter(|p| !p.trim().is_empty()).map(|p| text.translated(p).to_string()).collect();
                            let want: Vec<&str> = whole.iter().flat_map(|p| p.split_whitespace()).collect();
                            if painted != want || out.desc.iter().any(|l| l.contains('\u{2026}')) {
                                faults.push(format!("{at}: cut: {:?}", out.desc));
                            }
                            match out.rows[sel] {
                                Some(a) if a.y + a.h <= out.desc_top => {}
                                r => faults.push(format!("{at}: the selected row {r:?} isn't above the line (top {})", out.desc_top)),
                            }
                            if out.desc_top < crate::kit::PAD + crate::kit::PILL {
                                faults.push(format!("{at}: the line reaches the title"));
                            }
                        }
                    }
                }
            }
        }
        crate::type_scale::set(TextSize::Normal);
        crate::i18n::set(crate::i18n::Lang::En);
        assert!(faults.is_empty(), "{} of the Options page's lines don't read whole:\n{}", faults.len(), faults.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
    }
}
