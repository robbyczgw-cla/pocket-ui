//! The Options page of Android's player in the handhelds' pause menu (`pause_list`, a game from the Handheld view), laid
//! out as Linux's (`main.rs` `opt_entries`, `paint_options`): Picture ›, Emulator ›, Controls ›, Zoom ›, Cheats ›, then
//! what stands under Speed, rumble, fan on Linux and the player has (Over the game), Keep for. The same names, the same
//! order, the same lines under the rows. What the player has no counterpart for (Shortcuts, Runs on, Restore defaults,
//! rumble and fan) is left out; Screenshot, which Linux has as HOME + A, stays as the page's last row.
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
    /// Off / FPS: the frame rate and the CPU in the top left corner (Linux: Speed, rumble, fan › Over the game).
    OverGame,
    /// Where the Picture and Over the game choices are kept: this game, or every game of the console.
    Keep,
    Screenshot,
}

/// What the game has: an emulator with options, control rows, cheats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Has {
    pub emulator: bool,
    pub controls: bool,
    pub cheats: bool,
}

/// The page's rows in Linux's order (`opt_entries`: the groups, Emulator and Controls after Picture, Zoom after Controls,
/// Cheats after Zoom, Speed, rumble, fan's row, Keep for), Screenshot last.
pub fn top(has: Has) -> Vec<Entry> {
    let mut v = vec![Entry::Picture];
    if has.emulator {
        v.push(Entry::Emulator);
    }
    if has.controls {
        v.push(Entry::Controls);
    }
    v.push(Entry::Zoom);
    if has.cheats {
        v.push(Entry::Cheats);
    }
    v.extend([Entry::OverGame, Entry::Keep, Entry::Screenshot]);
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
            Entry::OverGame => "Over the game",
            Entry::Keep => "Keep for",
            Entry::Screenshot => "Screenshot",
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
            Entry::OverGame => OVER_GAME_DESC,
            Entry::Keep if keep_game => "Changes show at once and stay for this game. Left or right: every game on this console.",
            Entry::Keep => "Changes show at once and stay for every game on this console. Left or right: this game only.",
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
}

/// The Picture page's rows in Linux's order (Screen scaling, Look, Screen frame).
pub const PICTURE_ROWS: [PictureRow; 3] = [PictureRow::Scaling, PictureRow::Look, PictureRow::Frame];

impl PictureRow {
    pub fn key(self) -> &'static str {
        match self {
            PictureRow::Scaling => "scaling",
            PictureRow::Look => "look",
            PictureRow::Frame => "frame",
        }
    }

    /// Linux's value for each of the player's indices (`picture::Settings`: scaling Fit/Integer/Stretch, effect
    /// Sharp/Smooth/CRT/Scanlines, frame None/Console/CRT TV), as `android_handheld::picture_choice` reads them the other way.
    fn values(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["aspect", "native", "fullscreen"],
            PictureRow::Look => &["clean", "smooth", "oldtv", "scanlines"],
            PictureRow::Frame => &["none", "auto", "crt"],
        }
    }

    /// The row's name: "Screen scaling", "Look", "Screen frame".
    pub fn name(self) -> &'static str {
        match self {
            PictureRow::Scaling => "Screen scaling",
            PictureRow::Look => "Look",
            PictureRow::Frame => "Screen frame",
        }
    }

    /// The labels of `values`, as the Handheld view shows them ("Integer", "Old TV", "Console": `options::label_in`).
    fn labels(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["Aspect", "Integer", "Fullscreen"],
            PictureRow::Look => &["Clean", "Smooth", "Old TV", "Scanlines"],
            PictureRow::Frame => &["None", "Console", "CRT"],
        }
    }

    /// The values in the order the Handheld view lists them (`options::ANDROID_VALUES`).
    fn view_order(self) -> &'static [&'static str] {
        match self {
            PictureRow::Scaling => &["native", "aspect", "fullscreen"],
            PictureRow::Look => &["clean", "smooth", "scanlines", "oldtv"],
            PictureRow::Frame => &["none", "auto", "crt"],
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
    /// (Integer, Aspect, Fullscreen; Clean, Smooth, Scanlines, Old TV; None, Console, CRT), round.
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
        assert_eq!(names, ["Picture", "Emulator", "Controls", "Zoom", "Cheats", "Over the game", "Keep for", "Screenshot"]);
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
                let field = match row { PictureRow::Scaling => "scaling", PictureRow::Look => "effect", PictureRow::Frame => "frame" };
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
    }
}
