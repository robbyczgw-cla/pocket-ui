//! The App's side of ScreenScraper (`pocket_ui::screenscraper`, docs/SCREENSCRAPER.md): Settings → Games → Box art &
//! downloads → ScreenScraper (On/Off, Sign in with X), and the quiet background run that follows the box art.
//!
//! * **After the box art, on its rules.** The run starts where the libretro covers' run ends or finds nothing to do
//!   (`auto_covers`): online, no game running, not while another picture job holds `covers::claim`, never with
//!   Pictures Off. A game that starts stops it (`background_io::paused`, inside `screenscraper::run_until`), and so does
//!   an update that looks on the card or installs (the shelf's daily look for a new version only reads the list and
//!   leaves it running); it goes on from the index after the game or the update (`screenscraper_resume`), and the log
//!   names what stopped it.
//! * **Spines first.** Five passes over the library: the spines (`box-2D-side`, what the shelf shows), then the series'
//!   logos for the shelf by series (`wheel`, one game per series, `series_logo_games`), then the box
//!   backs and the labels (`box-2D-back`, `support-texture`, `support-2D`, for the game box and the cartridge or
//!   disc), then missing box art, then missing title screens and screenshots
//!   (`sstitle`, `ss`) for the box's back. The description, genre and dates come with the game's `jeuInfos`
//!   (`screenscraper_facts`), and a card reads them on a thread (`screenscraper_card_facts`). Every pass asks each game only once
//!   (`screenscraper::Index`); a pass with nothing to fetch sends nothing.
//! * **What the row says** ([`RowState`]): the account, the run's progress, what it found or why it ended, the day's
//!   count. Kept while the program runs; the Setup page gets it when it opens and as it changes.
//! * **Sign in** checks the name and password once (`ssuserInfos`) on a thread before they are kept; a refused login
//!   is not kept.
//!
//! Every request is made on a thread of its own; the UI thread only reads the login file when the page opens.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::screenscraper::{self, Ended, Face, Refusal, RowState};

use crate::{App, UserEvent};

/// The passes of a run, in order: what the shelf shows first. Last the title screen and a screenshot for the box's back
/// and the card. The libretro run comes first (`auto_covers`).
/// The series' logos (`wheel`) right after the spines: one game per series of the shelf by series
/// (`screenscraper::series_logo_games`), only while Home is by series.
const PASSES: [&[Face]; 5] = [&[Face::BoxSide], &[Face::Wheel], &[Face::BoxBack, Face::SupportTexture, Face::Support], &[Face::Cover], &[Face::Title, Face::Snap]];

#[derive(Debug)]
pub(crate) enum ScreenScraperEvent {
    /// `pass`: which of [`PASSES`] (0: the spines).
    /// `pictures`: saved so far in this pass.
    Progress { pass: usize, done: usize, total: usize, pictures: usize, generation: u64 },
    /// `generation`: the sign-in state the run started with (`ScreenScraperUi::generation`); a run of an account signed out since
    /// says nothing of the account.
    /// `held`: games a guest's brake left for tomorrow.
    /// `why`: what stopped it, when it ended `Stopped` (for the log).
    Done { pictures: usize, held: usize, ended: Ended, today: (u32, u32), user: String, generation: u64, why: Option<&'static str> },
    /// The sign-in's answer: the account's numbers, or why not (None: no answer).
    /// The login is kept only when this is still the newest sign-in or out (`generation`), on the UI's side.
    /// `answer`: the account's numbers and the day's count as the limits use it (`Client::day_count`).
    SignedIn { answer: Result<(screenscraper::Quota, (u32, u32)), Option<Refusal>>, login: screenscraper::Login, generation: u64 },
    /// A card's facts, read on a thread (`screenscraper_card_facts`): for the card of `path`, if it is still open.
    Facts { path: PathBuf, facts: Box<pocket_ui::screenscraper_facts::Facts> },
}

/// ScreenScraper while the program runs.
#[derive(Default)]
pub(crate) struct ScreenScraperUi {
    pub(crate) row: RowState,
    running: bool,
    cancel: Arc<AtomicBool>,
    /// The spines' pass of the running run: (done, total), for the shelf's small line.
    spines: Option<(usize, usize)>,
    /// When and at how many saved pictures the shelf last looked for new spines during this run.
    spine_refresh: Option<(Instant, usize)>,
    /// The last run ended for the day (the daily limit, the guest's brake): the shelf says the rest comes tomorrow.
    resting: bool,
    /// Counts sign-ins and sign-outs: a run's end from before one is not the account's.
    generation: u64,
    /// Home became by series while a run went on (its logo pass had no series): another run when it ends.
    series_logos_due: bool,
    /// The last run was stopped (a game, an update on the card) and goes on from its index when the card is free
    /// again ([`App::screenscraper_resume`]).
    stopped: bool,
}

impl ScreenScraperUi {
    pub(crate) fn new(on: bool, show: screenscraper::Show) -> ScreenScraperUi {
        let mut ui = ScreenScraperUi { row: RowState { on, show, ..RowState::default() }, ..ScreenScraperUi::default() };
        // Snapshots and tests (`POCKETUI_MOCK`, nothing is sent): the shelf's line as a run shows it,
        // `POCKETUI_SCREENSCRAPER_NOTE=spines:<done>:<total>` or `tomorrow`.
        if std::env::var_os("POCKETUI_MOCK").is_some() {
            let note = std::env::var("POCKETUI_SCREENSCRAPER_NOTE").unwrap_or_default();
            let mut parts = note.split(':');
            match (parts.next(), parts.next().and_then(|n| n.parse().ok()), parts.next().and_then(|n| n.parse().ok())) {
                (Some("spines"), Some(done), Some(total)) => {
                    ui.running = true;
                    ui.spines = Some((done, total));
                }
                (Some("tomorrow"), _, _) => ui.resting = true,
                _ => {}
            }
        }
        ui
    }

    /// Publish the first spine immediately, then group new pictures while the pass is running.
    fn spine_refresh_due(&mut self, pictures: usize, pass_done: bool, now: Instant) -> bool {
        if pictures == 0 || self.spine_refresh.is_some_and(|(when, saved)| {
            pictures <= saved || (!pass_done && now.duration_since(when) < Duration::from_secs(10))
        }) {
            return false;
        }
        self.spine_refresh = Some((now, pictures));
        true
    }
}

/// The shelf's line (`App::screenscraper_shelf_note`): `on` (ScreenScraper and Pictures), a run going, its spines' count once one
/// came in, the day ended.
fn shelf_line(on: bool, running: bool, spines: Option<(usize, usize)>, resting: bool) -> Option<String> {
    if !on {
        return None;
    }
    match spines {
        Some((done, total)) if running && total > 0 => Some(format!("Getting spines {done} of {total}")),
        _ if resting && !running => Some("Spines: more tomorrow".to_string()),
        _ => None,
    }
}

/// Whether a run that ended goes on later from its index: stopped by a game or an update, not by Off, a sign-in or
/// out (those change `generation`, so the end is not `current`).
fn goes_on_later(ended: Ended, current: bool) -> bool {
    current && ended == Ended::Stopped
}

/// The row's line for a run that ended.
fn ended_line(pictures: usize, ended: Ended) -> Option<String> {
    match ended {
        Ended::Refused(why) => Some(why.message().to_string()),
        // A game started: the run goes on after it, nothing to say.
        Ended::Stopped => None,
        Ended::Done if pictures == 1 => Some("1 new picture.".to_string()),
        Ended::Done if pictures > 0 => Some(format!("{pictures} new pictures.")),
        Ended::Done => Some("Nothing new to download.".to_string()),
    }
}

impl App {
    /// The row as the page shows it: the run's state, and the account kept on the card.
    pub(crate) fn screenscraper_row_state(&self) -> RowState {
        let mut row = self.screenscraper.row.clone();
        if row.user.is_none() && screenscraper::shown() {
            row.user = screenscraper::login(&self.data_dir).map(|l| l.user);
        }
        row
    }

    /// The page, while it is open, shows the row as it is now.
    fn screenscraper_show(&mut self) {
        let row = self.screenscraper.row.clone();
        if let Some(setup) = self.setup.as_mut() {
            setup.set_screenscraper(row);
        }
        self.request_redraw();
    }

    /// The row was switched (Settings): kept; On starts a run, Off stops one after the picture it is on.
    pub(crate) fn screenscraper_switched(&mut self, on: bool) {
        self.screenscraper.row.on = on;
        self.save_settings();
        if on {
            self.auto_screenscraper("switched on");
        } else {
            self.screenscraper_stop();
        }
    }

    /// The rows under ScreenScraper changed (Real spines, labels, box backs): at once, without a restart, the shelf and the
    /// open box show what they say (`covers_changed` makes the spines, labels and the box's back again); what is
    /// switched off is not fetched by the next run, and a row switched on brings its pictures with the next run.
    pub(crate) fn screenscraper_pictures_switched(&mut self, show: screenscraper::Show) {
        let before = self.screenscraper.row.show;
        self.screenscraper.row.show = show;
        self.save_settings();
        // Only Real manuals changed: kept, nothing else to do. The Details row follows when a card opens next; no
        // picture is made again and no run is started (Sol 26.11.2026).
        if !pictures_differ(before, show) {
            return;
        }
        self.covers.clear();
        self.pictures_changed();
        self.cabinet.covers_changed();
        self.look_previews = None;
        self.auto_screenscraper("pictures shown changed");
    }

    /// Whether ScreenScraper's `face` is shown now: its row On (Real spines, labels, box backs). Off shows exactly what
    /// pocket-ui draws without it, also for pictures already on the card (owner 26.11.2026). ScreenScraper itself Off
    /// only sends nothing: what it brought stays shown.
    pub(crate) fn screenscraper_shows(&self, face: Face) -> bool {
        self.screenscraper.row.show.fetches(face)
    }

    /// Off (ScreenScraper or Pictures), a sign-in or a sign-out: the running run gives up the request it is on, and the
    /// shelf goes back to exactly what it shows without ScreenScraper.
    pub(crate) fn screenscraper_stop(&mut self) {
        self.screenscraper.cancel.store(true, Ordering::Relaxed);
        // Whatever was on its way (a run's progress and end, a sign-in's answer) speaks for nobody now: a sign-in that
        // succeeds after Off is not kept.
        self.screenscraper.generation += 1;
        self.screenscraper.spines = None;
        self.screenscraper.resting = false;
        self.screenscraper.stopped = false;
    }

    /// The small line on the shelf while ScreenScraper works, where the quiet dot is otherwise (Alex 26.11.2026):
    /// the spines' count while they come, "Spines: more tomorrow" when the day's limit paused it. None while it is off or idle:
    /// the shelf is then exactly as without it.
    pub(crate) fn screenscraper_shelf_note(&self) -> Option<String> {
        shelf_line(self.screenscraper.row.on && !self.art_off, self.screenscraper.running, self.screenscraper.spines, self.screenscraper.resting)
    }

    /// Home just became by series: the series' logos are fetched now, or after the running run (whose logo pass knew no
    /// series).
    pub(crate) fn screenscraper_series_appeared(&mut self) {
        if !pocket_ui::series_look::SERIES_LOGOS {
            return;
        }
        if self.screenscraper.running {
            self.screenscraper.series_logos_due = true;
        } else {
            self.auto_screenscraper("series");
        }
    }

    /// A run that a game or an update stopped goes on (`auto_screenscraper` checks the rest: no game, the card free,
    /// online). Called when an update on the card is over (`resume_shelf_after_update`) and when the run's end comes in.
    pub(crate) fn screenscraper_resume(&mut self, why: &str) {
        // Not in the moment before the restart after an install either, nor ahead of the checksums: Identify's end starts
        // the covers' run, and that one's end ScreenScraper's (a run started now would hold `covers::claim` against it).
        if self.screenscraper.stopped && !self.screenscraper.running && self.updates.restart_at.is_none() && !self.identify.busy() {
            self.auto_screenscraper(why);
        }
    }

    /// The run of pictures from ScreenScraper, when it may run (module docs).
    pub(crate) fn auto_screenscraper(&mut self, why: &str) {
        if !self.screenscraper.row.on || self.screenscraper.running || self.art_off || !screenscraper::available() {
            return;
        }
        if self.running.is_some() || pocket_ui::background_io::paused() || pocket_ui::wifi::ip().is_none() {
            return;
        }
        let Some(dev) = screenscraper::dev() else { return };
        if self.covers_wait_for_gamelists() {
            return;
        }
        let mut games = self.all_games();
        if games.is_empty() {
            return;
        }
        let Some(claim) = pocket_ui::covers::claim() else { return };
        println!("screenscraper ({why}): {} games", games.len());
        self.screenscraper.running = true;
        self.screenscraper.stopped = false;
        self.screenscraper.spine_refresh = None;
        self.screenscraper.cancel = Arc::default();
        let (proxy, data_dir, cancel, generation, show) = (self.proxy.clone(), self.data_dir.clone(), self.screenscraper.cancel.clone(), self.screenscraper.generation, self.screenscraper.row.show);
        let gamelist = self.gamelist.index.clone();
        let names = self.identify.names.clone();
        // The shelf by series' logos: each series' games in the order its spine takes them (none while Home is by console).
        let series = self.series_logo_candidates();
        std::thread::spawn(move || {
            pocket_ui::web::lower_priority();
            screenscraper::prioritize_recognized(&mut games, &names);
            let net = pocket_ui::romm_net::system();
            let clock = screenscraper::RealClock::new();
            let login = screenscraper::login(&data_dir);
            let user = login.as_ref().map(|l| l.user.clone()).unwrap_or_default();
            let Some(mut client) = screenscraper::Client::new(net.as_ref(), &clock, screenscraper::API, dev, login) else { return };
            // The first stop's reason, read when it is seen (later a game may hold the card for another reason).
            let why = std::sync::OnceLock::new();
            let stop = || {
                let reason = if cancel.load(Ordering::Relaxed) {
                    Some("switched off or signed in again")
                } else {
                    pocket_ui::background_io::cancelled().then(pocket_ui::background_io::why_stopped)
                };
                if let Some(reason) = reason {
                    let _ = why.set(reason);
                }
                reason.is_some()
            };
            let (mut pictures, mut held, mut ended) = (0, 0, Ended::Done);
            for (pass, want) in PASSES.into_iter().enumerate() {
                // What the player switched off is not downloaded (the rows under ScreenScraper).
                let want: Vec<Face> = want.iter().copied().filter(|f| show.fetches(*f)).collect();
                if want.is_empty() {
                    continue;
                }
                let want = &want[..];
                let progress = |p: screenscraper::Progress| {
                    let _ = proxy.send_event(UserEvent::ScreenScraper(ScreenScraperEvent::Progress { pass, done: p.done, total: p.total, pictures: p.pictures, generation }));
                };
                let mut run = None;
                let completed = pocket_ui::background_io::job(|| {
                    let games = if want == [Face::Wheel] {
                        std::borrow::Cow::Owned(screenscraper::series_logo_games(&data_dir, &series))
                    } else {
                        screenscraper::games_for_pass(&games, want, &gamelist)
                    };
                    run = Some(screenscraper::run_until(&mut client, &data_dir, &games, want, &progress, &stop));
                }).is_some();
                // A game can interrupt the job after it saved pictures. Publish those at the end too.
                if let Some(run) = run {
                    pictures += run.pictures;
                    held = held.max(run.held);
                    ended = run.ended;
                }
                if !completed {
                    ended = Ended::Stopped;
                }
                if ended != Ended::Done {
                    break;
                }
            }
            // A job that never started (`background_io::job`) did not ask `stop`: what holds the card now.
            let why = (ended == Ended::Stopped).then(|| why.get().copied().unwrap_or_else(pocket_ui::background_io::why_stopped));
            // The day's count as the limits use it (the server's own `requeststoday` lags behind the pictures).
            let today = client.day_count();
            // Free before the end is told: the end may start the run again at once (`screenscraper_resume`).
            drop(claim);
            let _ = proxy.send_event(UserEvent::ScreenScraper(ScreenScraperEvent::Done { pictures, held, ended, today, user, generation, why }));
        });
    }

    /// The name and password typed under Sign in: checked once, kept when ScreenScraper takes them.
    pub(crate) fn screenscraper_sign_in(&mut self, user: String, password: String) {
        // Off means no request at all, and without Wi-Fi there is no one to ask (nothing is kept then).
        if !self.screenscraper.row.on || pocket_ui::wifi::ip().is_none() {
            self.screenscraper.row.line = Some("Needs Wi-Fi".to_string()).filter(|_| self.screenscraper.row.on);
            return;
        }
        // One request at a time: the running run stops first (`screenscraper::LANE` keeps them apart anyway).
        self.screenscraper_stop();
        self.screenscraper.generation += 1;
        self.screenscraper.cancel = Arc::default();
        let (proxy, data_dir, cancel, generation) = (self.proxy.clone(), self.data_dir.clone(), self.screenscraper.cancel.clone(), self.screenscraper.generation);
        self.screenscraper.row.line = Some("Signing in…".to_string());
        std::thread::spawn(move || {
            let login = screenscraper::Login { user, password };
            let stop = || cancel.load(Ordering::Relaxed);
            let answer = match screenscraper::dev() {
                None => Err(None),
                Some(dev) => {
                    let net = pocket_ui::romm_net::system();
                    let clock = screenscraper::RealClock::new();
                    match screenscraper::Client::new(net.as_ref(), &clock, screenscraper::API, dev, Some(login.clone())) {
                        None => Err(None),
                        Some(mut client) => {
                            // Booked in the account's day like every request, and not sent when its day is over.
                            client.keep_day(&data_dir);
                            match client.user_info(&stop) {
                                Ok(Some(q)) => Ok((q, client.day_count())),
                                Ok(None) => Err(None),
                                // A day's end is in the day's file already (`Client::answered`).
                                Err(why) => Err(Some(why)),
                            }
                        }
                    }
                }
            };
            let _ = proxy.send_event(UserEvent::ScreenScraper(ScreenScraperEvent::SignedIn { answer, login, generation }));
        });
    }

    /// X on the row of a signed-in account: the login goes, the pictures stay.
    pub(crate) fn screenscraper_sign_out(&mut self) {
        // A run with the account stops now; its end no longer speaks for it.
        self.screenscraper_stop();
        self.screenscraper.generation += 1;
        screenscraper::forget_login(&self.data_dir);
        self.screenscraper.row.user = None;
        self.screenscraper.row.line = None;
        self.screenscraper.row.today = (0, 0);
    }

    pub(crate) fn screenscraper_event(&mut self, e: ScreenScraperEvent) {
        match e {
            ScreenScraperEvent::Progress { generation, .. } if generation != self.screenscraper.generation => {}
            ScreenScraperEvent::SignedIn { generation, .. } if generation != self.screenscraper.generation => {}
            ScreenScraperEvent::Progress { pass, done, total, pictures, .. } => {
                self.screenscraper.row.line = Some(format!("Looking up {done} of {total} games…"));
                // Only once a spine came in: offline (Android's Wi-Fi check can't tell) the run ends without any, and
                // the shelf never showed a line for it.
                self.screenscraper.spines = (pass == 0 && (pictures > 0 || self.screenscraper.spines.is_some())).then_some((done, total));
                if pass == 0 && self.screenscraper.spine_refresh_due(pictures, done == total, Instant::now()) {
                    self.cabinet.spines_changed();
                }
                // The series' logos show as soon as their pass is through.
                if pass == 1 && pictures > 0 && done == total {
                    self.cabinet.spines_changed();
                }
            }
            ScreenScraperEvent::Done { pictures, held, ended, today, user, generation, why } => {
                match why {
                    Some(why) => println!("screenscraper: {pictures} pictures, {ended:?} ({why})"),
                    None => println!("screenscraper: {pictures} pictures, {ended:?}"),
                }
                // The index may list new manuals: read again when a card opens (`app_manual`).
                self.game_manuals_index_changed();
                self.screenscraper.running = false;
                self.screenscraper.spines = None;
                let current = generation == self.screenscraper.generation;
                self.screenscraper.resting = current && (held > 0 || matches!(ended, Ended::Refused(r) if r.for_today()));
                if current {
                    self.screenscraper.row.line = ended_line(pictures, ended);
                    if today.1 > 0 {
                        self.screenscraper.row.today = today;
                    }
                    if !user.is_empty() {
                        self.screenscraper.row.user = Some(user);
                    }
                }
                // Home became by series during the run: its logos now.
                if std::mem::take(&mut self.screenscraper.series_logos_due) && current && ended == Ended::Done {
                    self.auto_screenscraper("series");
                }
                // Stopped for a game or an update: on again from its index once the card is free. When this end comes
                // after the update already let go (`resume_shelf_after_update` found the run still going), now.
                self.screenscraper.stopped = goes_on_later(ended, current);
                self.screenscraper_resume("after a stop");
                if pictures > 0 {
                    // The new spines, backs and box art show at once.
                    self.covers.clear();
                    self.pictures_changed();
                    self.cabinet.covers_changed();
                    self.look_previews = None;
                }
            }
            ScreenScraperEvent::SignedIn { answer: Ok((q, today)), login, .. } => {
                // Kept now, on the newest sign-in only: a refused or unanswered one is never kept.
                if screenscraper::save_login(&self.data_dir, &login).is_err() {
                    self.screenscraper.row.user = None;
                    self.screenscraper.row.line = Some(Refusal::Unreachable.message().to_string());
                } else {
                    self.screenscraper.row.user = Some(q.user.clone()).filter(|u| !u.is_empty()).or(Some(login.user.clone()));
                    self.screenscraper.row.line = None;
                    if today.1 > 0 {
                        self.screenscraper.row.today = today;
                    }
                    self.auto_screenscraper("signed in");
                }
            }
            ScreenScraperEvent::Facts { path, facts } => {
                self.screenscraper_fill_card(&path, &facts);
                return;
            }
            ScreenScraperEvent::SignedIn { answer: Err(why), .. } => {
                self.screenscraper.row.user = None;
                self.screenscraper.row.line = Some(why.unwrap_or(Refusal::Unreachable).message().to_string());
            }
        }
        self.screenscraper_show();
    }

    /// The card of `rom` opened: its ScreenScraper facts are read on a thread and fill in what the card lacks
    /// (`screenscraper_facts::fill_card`). What was kept shows whether ScreenScraper is On or Off, as the spines do
    /// (decided 26.11.2026: Off sends nothing, what came stays); a game never looked up has no file and its card is as
    /// without ScreenScraper. Snapshots (`cabinet.inline`) read them at once.
    pub(crate) fn screenscraper_card_facts(&mut self, system: &str, rom: &std::path::Path) {
        let (data_dir, system, rom) = (self.data_dir.clone(), system.to_string(), rom.to_path_buf());
        if self.cabinet.inline {
            if let Some(facts) = pocket_ui::screenscraper_facts::load(&data_dir, &system, &rom) {
                self.screenscraper_fill_card(&rom, &facts);
            }
            return;
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            if let Some(facts) = pocket_ui::screenscraper_facts::load(&data_dir, &system, &rom) {
                let _ = proxy.send_event(UserEvent::ScreenScraper(ScreenScraperEvent::Facts { path: rom, facts: Box::new(facts) }));
            }
        });
    }

    fn screenscraper_fill_card(&mut self, rom: &std::path::Path, facts: &pocket_ui::screenscraper_facts::Facts) {
        let Some(open) = self.card.as_mut().filter(|o| o.path == rom) else { return };
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if pocket_ui::screenscraper_facts::fill_card(&mut open.card.data, facts, pocket_ui::i18n::current().code(), stem, &open.source_facts) {
            open.card.data.leave_out_unsayable();
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_ends_with_a_line_the_player_reads() {
        assert_eq!(ended_line(0, Ended::Done).as_deref(), Some("Nothing new to download."));
        assert_eq!(ended_line(1, Ended::Done).as_deref(), Some("1 new picture."));
        assert_eq!(ended_line(42, Ended::Done).as_deref(), Some("42 new pictures."));
        assert_eq!(ended_line(3, Ended::Refused(Refusal::QuotaToday)).as_deref(), Some(Refusal::QuotaToday.message()));
        assert_eq!(ended_line(3, Ended::Stopped), None, "a game stopped it: it goes on later, nothing to say");
    }

    /// Sol 27.11.2026: a run stopped by an update look came back only once the library was read again. A run that a
    /// game or an update stopped goes on; one ended by Off or a sign-in (no longer `current`), done or refused does not.
    #[test]
    fn a_run_stopped_for_a_game_or_an_update_goes_on_later() {
        assert!(goes_on_later(Ended::Stopped, true));
        assert!(!goes_on_later(Ended::Stopped, false), "Off, signed in or out meanwhile");
        assert!(!goes_on_later(Ended::Done, true));
        assert!(!goes_on_later(Ended::Refused(Refusal::QuotaToday), true));
    }

    /// Alex 26.11.2026: off or offline the shelf is today's, no line; a line only once spines really come, and
    /// "Spines: more tomorrow" after the day's share. Android's Wi-Fi check says online even offline (review 26.11.): a run there
    /// that gets nothing never shows the count (`Progress` keeps it back until a picture is saved).
    #[test]
    fn the_shelf_line_shows_only_while_spines_come_or_the_day_is_over() {
        assert_eq!(shelf_line(true, true, Some((34, 120)), false).as_deref(), Some("Getting spines 34 of 120"));
        assert_eq!(shelf_line(true, false, None, true).as_deref(), Some("Spines: more tomorrow"));
        assert_eq!(shelf_line(false, true, Some((34, 120)), false), None, "off: nothing");
        assert_eq!(shelf_line(false, false, None, true), None, "off: nothing, even after the day ended");
        assert_eq!(shelf_line(true, true, None, false), None, "running, nothing in yet (offline): nothing");
        assert_eq!(shelf_line(true, false, None, false), None, "idle: nothing");
        assert_eq!(shelf_line(true, true, Some((0, 0)), false), None, "no games: nothing");
    }

    #[test]
    fn the_first_spine_refreshes_the_shelf_before_the_run_finishes() {
        let mut ui = ScreenScraperUi { running: true, ..ScreenScraperUi::default() };
        let start = Instant::now();
        assert!(!ui.spine_refresh_due(0, false, start));
        assert!(ui.spine_refresh_due(1, false, start), "the first picture is visible during the run");
        assert!(!ui.spine_refresh_due(2, false, start + Duration::from_secs(1)));
        assert!(ui.spine_refresh_due(3, false, start + Duration::from_secs(10)), "refresh the group of new pictures");
        assert!(!ui.spine_refresh_due(3, false, start + Duration::from_secs(11)), "no new picture to show");
        assert!(ui.spine_refresh_due(4, true, start + Duration::from_secs(12)), "publish the remaining pictures at pass end");
    }

    #[test]
    fn the_spines_come_first() {
        assert_eq!(PASSES[0], &[Face::BoxSide], "the shelf shows the spines: they are fetched before anything else");
        assert_eq!(PASSES[1], &[Face::Wheel], "the series' logos right after the spines: they are spines too");
        assert!(PASSES.iter().flat_map(|p| p.iter()).filter(|f| **f == Face::Cover).count() == 1 && PASSES[3] == [Face::Cover], "box art after the box's faces: libretro's comes first");
        assert_eq!(PASSES[4], &[Face::Title, Face::Snap], "the back's pictures last, after everything the shelf and the box show");
    }
}

/// The rows that change pictures differ (spines, labels, box backs); Real manuals alone changes none.
fn pictures_differ(a: screenscraper::Show, b: screenscraper::Show) -> bool {
    (a.spines, a.labels, a.backs) != (b.spines, b.labels, b.backs)
}

#[cfg(test)]
mod manuals_switch_tests {
    use super::*;

    /// Sol 26.11.2026 (P2): switching Real manuals ran the pictures' handler: covers and previews thrown away, a run
    /// with requests started. Only the three picture rows change pictures.
    #[test]
    fn real_manuals_alone_changes_no_picture() {
        let on = screenscraper::Show::default();
        let manuals_off = screenscraper::Show { manuals: false, ..on };
        assert!(!pictures_differ(on, manuals_off), "Real manuals: only kept");
        for other in [screenscraper::Show { spines: false, ..on }, screenscraper::Show { labels: false, ..on }, screenscraper::Show { backs: !on.backs, ..on }] {
            assert!(pictures_differ(on, other), "{other:?}: pictures change");
        }
    }
}
