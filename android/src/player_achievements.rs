//! RetroAchievements in the APK's own player: the same session, offline copy and card as pocket-core on Linux
//! (`pocket_ui::achievements`), started only when an account is signed in. Without one nothing runs and nothing
//! touches the network. Signing in happens in the launcher (Settings → RetroAchievements); only the token is kept.
//!
//! The game thread only calls `frame` after each emulated frame (rcheevos reads the game's memory; well under a
//! millisecond), `idle` while paused and `reset` after a state was loaded. Hashing the game, signing in, loading its
//! set and every request run on the session's own threads, so the game starts as fast as without an account.
use pocket_ui::{
    achievements::{
        account::Account,
        cards::{Card, Cards},
        consoles, http,
        session::{Config, Session},
        snapshot::{Snapshot, State},
    },
    gfx::Canvas,
    libretro::Core,
    text::Text,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the session may take to say goodbye: the game's own exit must not wait. An unlock still on its way is kept in
/// `pending.json` before it is sent, so it goes with the next session if this one is cut short.
const CLOSE_WAIT: Duration = Duration::from_millis(100);

pub struct Achievements {
    session: Option<Session>,
    cards: Cards,
    snapshot: PathBuf,
    rom: PathBuf,
    /// The card last painted, with whether its badge was there: a different one needs a repaint.
    painted: Option<(u32, bool)>,
}

/// Where the pause menu's list is written (`<data>/achievements/running.json`).
fn snapshot_path(data: &Path) -> PathBuf {
    pocket_ui::achievements::dir(data).join("running.json")
}

impl Achievements {
    /// Starts achievements for this game when an account is signed in and its system has them. `rom` is the path the
    /// core was given: for a game opened without a copy that is the alias of the document's descriptor
    /// (`android_content::DirectGame`), which hashes like the file itself.
    pub fn start(data: &Path, system: &str, rom: &Path, key: &str, core: &Core) -> Option<Self> {
        let snapshot = snapshot_path(data);
        // The last game's list goes, so the pause menu never shows one that isn't this session's.
        let _ = std::fs::remove_file(&snapshot);
        let Some(account) = Account::load(data) else {
            log::info!("retroachievements: not signed in, nothing runs");
            return None;
        };
        let console = consoles::console_id(system)?;
        let config = Config {
            data_dir: data.to_path_buf(),
            account,
            system: system.to_string(),
            rom: rom.to_path_buf(),
            console,
            core_name: core.name.clone(),
            snapshot: snapshot.clone(),
            verbose: false,
            mastery_key: Some(key.to_string()),
        };
        let agent = http::user_agent(Some((&core.name, &core.version)));
        #[cfg(not(feature = "ra-replay"))]
        let transport = Box::new(http::AndroidHttp { user_agent: agent });
        // A QA build answers from the device (`ra_replay`); a release APK has no such code.
        #[cfg(feature = "ra-replay")]
        let transport = Box::new({ let _ = agent; super::ra_replay::Replay::new(console) });
        let session = Session::start(config, core.achievement_memory(), transport)?;
        log::info!("retroachievements: session started for {system} (console {console}, core {})", core.name);
        Some(Achievements { session: Some(session), cards: Cards::default(), snapshot, rom: rom.to_path_buf(), painted: None })
    }

    /// After each emulated frame.
    pub fn frame(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.frame();
        }
    }

    /// While no frame runs (the pause menu): rc_client's timers (retries, the ping) go on.
    pub fn idle(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.idle();
        }
    }

    /// A state was loaded: achievements start again from waiting, so the loaded moment can't unlock anything.
    pub fn reset(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.reset();
        }
    }

    /// The pause menu opened: the list is written now.
    pub fn menu_opened(&self) {
        if let Some(s) = &self.session {
            s.menu_opened();
        }
    }

    /// What the session had to say, as notes for the line over the game, and whether the card on the screen changed
    /// (the overlay is painted again then).
    pub fn poll(&mut self, now: Instant) -> (Vec<String>, bool) {
        if let Some(s) = &self.session {
            self.cards.feed(s.take_notices(), now);
        }
        let notes = self.cards.take_notes();
        for note in &notes {
            log::info!("retroachievements: note {note:?}");
        }
        let now_painted = self.cards.current(now).map(|c| (c.id, c.badge.is_some()));
        let changed = now_painted != self.painted;
        if changed {
            if let Some(card) = self.cards.current(now) {
                log::info!("retroachievements: card {} {:?} {} points, badge {}", card.id, card.title, card.points, card.badge.is_some());
            }
        }
        self.painted = now_painted;
        (notes, changed)
    }

    /// The unlock card on the screen now, if any.
    pub fn card(&mut self, now: Instant) -> Option<&Card> {
        self.cards.current(now)
    }

    /// Paints the card at the top centre of the overlay; returns its area.
    pub fn paint_card(c: &mut Canvas, text: &mut Text, card: &Card, w: i32, top: i32) -> pocket_ui::touch::Area {
        let max = pocket_ui::unlock_card::text_max(w);
        let (cw, ch) = pocket_ui::unlock_card::size(text, &card.title, card.points, max);
        let x = ((w - cw) / 2).max(0);
        pocket_ui::unlock_card::paint(c, text, (x, top), &card.title, card.points, card.badge.as_ref(), max);
        pocket_ui::touch::Area::new(x, top, cw, ch)
    }

    /// The running game's list for the pause menu (written by the session when the menu opened).
    pub fn list(&self) -> Snapshot {
        Snapshot::read(&self.snapshot, &self.rom).unwrap_or(Snapshot { state: State::Loading, ..Default::default() })
    }

    /// Ends the session before the core unloads (it reads the core's memory).
    pub fn close(&mut self) {
        if let Some(s) = self.session.take() {
            s.close(CLOSE_WAIT);
        }
    }
}

impl Drop for Achievements {
    fn drop(&mut self) {
        self.close();
    }
}
