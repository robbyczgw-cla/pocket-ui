//! Part of the pocket-ui binary: how a game starts on Android's handheld view, and the "Runs on" page that shows it.
//!
//! The decision is `pocket_ui::play_route`'s (`Phone::way`), the same the Mobile view asks: our own player with a core that is
//! there, our own player with a core to download first, or another emulator app (chosen for the system or the game, or the
//! first one that takes a system we have no player for). This file feeds it (the phone's apps and cores, the choices in
//! launcher.json) and acts on its answer:
//!
//! * `resolve_runner` answers from it where there is a phone (`app_emu_choice`), so the launch, the shelf and the page agree;
//! * a game whose core isn't on the phone doesn't start: its "Runs on" page opens on the Download row (`phone_prelaunch`);
//! * a game no installed app can start says which app it needs, as Mobile does (`document_check::app_missing`);
//! * a save on the page goes into launcher.json's `plays_with` / `plays_with_game`, where the Mobile view reads it.
//!
//! The phone is read by the host (`app_android`, once at start and again when the view is back in front); a test or a
//! snapshot describes one with `POCKETUI_PHONE=<json file>` (`play_route::Phone::from_json`), which is read only under the
//! test driver and `--snapshot`.

use std::path::{Path, PathBuf};

use pocket_ui::emu_choice::{self, Cfg, EsEmulator, Resolved, Runner, POCKET_CORE};
use pocket_ui::emu_choice_page::{Download, OwnCore, PhoneSubject};
use pocket_ui::library::System;
use pocket_ui::play_route::{self, Phone, Way};
use pocket_ui::play_route_cfg::{self, GameRef};

use super::app_emu_choice::{Inputs, Opener};
use super::{App, Notice};


/// A test or a snapshot's phone, from `POCKETUI_PHONE` (a file) and the choices in `data_dir`'s launcher.json. Nothing
/// without the variable, and never in a normal run: only the test driver and `--snapshot` call it.
pub(crate) fn init_for_tests(data_dir: &Path) {
    // A test's or a snapshot's device storage for the first start's look (`android_first_start::look`): a folder laid out
    // like /storage (emulated/0/..., XXXX-XXXX/...).
    if let Some(storage) = std::env::var_os("POCKETUI_FIRST_START_STORAGE") {
        pocket_ui::android_first_start::look(Path::new(&storage), true);
    }
    let Some(file) = std::env::var_os("POCKETUI_PHONE") else { return };
    let json = std::fs::read_to_string(&file).unwrap_or_default();
    play_route::set_phone(Phone::from_json(&json));
    play_route::set_prefs(play_route::prefs_from_json(&std::fs::read_to_string(data_dir.join("launcher.json")).unwrap_or_default()));
}

fn game_ref(system: &str, path: &Path) -> Option<GameRef> {
    Some(GameRef { system: system.to_string(), file: path.file_name()?.to_str()?.to_string(), key: play_route::game_key(path)? })
}

impl App {
    /// Where the choices are: the Mobile view's launcher.json (the app's files folder on Android).
    pub(crate) fn phone_prefs_path(&self) -> PathBuf {
        #[cfg(all(target_os = "android", feature = "handheld"))]
        return pocket_ui::platform::android::data_dir().join("launcher.json");
        #[cfg(not(all(target_os = "android", feature = "handheld")))]
        return self.data_dir.join("launcher.json");
    }

    /// What `sys` runs with where there is a phone: pocket-ui's own core (when the player has one), nothing from ES's list.
    pub(crate) fn phone_inputs(&self, sys: &System, phone: &Phone) -> Inputs<'static> {
        Inputs { pocket_core: play_route::own_core(&sys.name, phone.gambatte), pocket_cores: if sys.name=="nds" {pocket_ui::ds::CORES} else {&[]}, file_ok: true, fell_back: false, default_override: None }
    }

    /// The apps that take the system, as the page's emulator list: no cores, the first one is the default where our own
    /// player has no core.
    pub(crate) fn phone_emulators(phone: &Phone, system: &str) -> Vec<EsEmulator> {
        phone.apps_for(system).iter().map(|a| EsEmulator { name: a.package.clone(), cores: Vec::new(), default_core: None }).collect()
    }

    /// What the page says about the phone for this system.
    pub(crate) fn phone_subject(phone: &Phone, system: &str) -> PhoneSubject {
        let own = play_route::own_core(system, phone.gambatte).map(|core| {
            let offer = phone.offer(core);
            OwnCore {
                core: core.to_string(),
                name: offer.map(|o| o.name.clone()).unwrap_or_default(),
                size: offer.map_or(0, |o| o.size),
                present: phone.cores.contains(core),
                channel_open: phone.channel_open && offer.is_some(),
            }
        });
        PhoneSubject { labels: phone.apps_for(system).iter().map(|a| (a.package.clone(), a.label.clone())).collect(), own }
    }

    /// The choices of the page for this system and game in system.cfg's words (`play_route_cfg`).
    pub(crate) fn phone_cfg(&self, sys: &System, only: Option<&Path>) -> Cfg {
        let refs: Vec<GameRef> = match only {
            Some(path) => game_ref(&sys.name, path).into_iter().collect(),
            None => sys.games.iter().filter_map(|g| game_ref(&sys.name, &g.path)).collect(),
        };
        play_route_cfg::cfg_from_prefs(&play_route::prefs(), &[sys.name.as_str()], &refs)
    }

    /// What a game of `sys` starts in, and which level said so (`play_route::Phone::way`); None where there is no phone.
    fn phone_way(&self, sys: &System, path: Option<&Path>) -> Option<(Way, emu_choice::From)> {
        let phone = play_route::phone()?;
        let prefs = play_route::prefs();
        let game = path.and_then(play_route::game_key).and_then(|k| prefs.plays_with_game.get(&k).cloned());
        let system = prefs.plays_with.get(play_route::system_key(&sys.name)).cloned();
        let from = if game.is_some() {
            emu_choice::From::Game
        } else if system.is_some() {
            emu_choice::From::Console
        } else {
            emu_choice::From::Default
        };
        Some((phone.way(&sys.name, game.as_deref(), system.as_deref()), from))
    }

    /// `resolve_runner` where there is a phone: our player is `pocket-core` with its core, an app is its package.
    pub(crate) fn phone_resolve(&self, sys: &System, path: Option<&Path>) -> Option<Resolved> {
        let (way, from) = self.phone_way(sys, path)?;
        let runner = match way {
            Way::Own { core } | Way::NeedsCore { core } => Runner::new(POCKET_CORE, &core),
            Way::App(app) => Runner::new(&app.package, ""),
            Way::NoApp { .. } => Runner::default(),
        };
        Some(Resolved { runner, from })
    }


    /// Before a launch is queued: a game that can't start here says why instead of starting. True: it was dealt with.
    /// * our player's core isn't on the phone: the game's "Runs on" page opens on its Download row;
    /// * no app takes the system: which app it needs and where it comes from.
    pub(crate) fn phone_prelaunch(&mut self, system: &str, name: &str, path: &Path) -> bool {
        // An Android game is its app, which is there while it is on the shelf (`app_android_games`).
        if system == pocket_ui::android_games::SYSTEM_ID {
            return false;
        }
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return false };
        let Some((way, _)) = self.phone_way(sys, Some(path)) else { return false };
        match way {
            Way::NeedsCore { .. } => {
                self.phone_refresh();
                if self.open_emu_choice(system, Some((name, path)), Opener::Shelf) {
                    if let Some((page, _)) = self.emu.page.as_mut() {
                        if let Some(at) = page.rows().iter().position(|r| *r == pocket_ui::emu_choice_page::Row::Download) {
                            page.sel = at;
                        }
                    }
                }
                pocket_ui::ui_sound::refuse();
                self.request_redraw();
                true
            }
            Way::NoApp { profile } => {
                let said = pocket_ui::document_check::app_missing(&profile).unwrap_or("No app on this device plays this system.");
                self.notice = Some(Notice::new(said.to_string()));
                pocket_ui::ui_sound::refuse();
                self.request_redraw();
                true
            }
            // The Dolphin app takes no start options for its pad or its touch buttons: before the first GameCube or Wii
            // game, a card says how to set it up there (19.11.2026, the Mini V2), once; the game's card keeps it.
            Way::App(app) if pocket_ui::rescue::is_dolphin(&app.package) && pocket_ui::first_game::tell_dolphin_once(&self.data_dir) => {
                self.open_dolphin_card(Some((system.to_string(), name.to_string(), path.to_path_buf())));
                true
            }
            Way::Own { .. } | Way::App(_) => false,
        }
    }

    /// Dolphin's card (`rescue::DialogKind::Dolphin`): before `launch` (Start the game is selected), or from the game's card.
    pub(crate) fn open_dolphin_card(&mut self, launch: Option<(String, String, PathBuf)>) {
        let start = launch.is_some();
        self.dolphin_launch = launch;
        self.leave = Some(pocket_ui::rescue::Dialog { sel: usize::from(start), ..pocket_ui::rescue::Dialog::new(pocket_ui::rescue::DialogKind::Dolphin { start }) });
        self.request_redraw();
    }

    /// The game card's "Setting up Dolphin" row: a game that plays in the Dolphin app here.
    pub(crate) fn card_dolphin_row(&self, system: &str, path: &Path) -> Option<pocket_ui::game_card_model::CuratorRow> {
        let sys = self.systems.iter().find(|s| s.name == system)?;
        let (way, _) = self.phone_way(sys, Some(path))?;
        matches!(way, Way::App(app) if pocket_ui::rescue::is_dolphin(&app.package)).then(|| pocket_ui::game_card_model::CuratorRow {
            kind: pocket_ui::game_card_model::RowKind::Dolphin,
            title: pocket_ui::rescue::DOLPHIN_TITLE.into(),
            line: pocket_ui::rescue::DOLPHIN_ROW_LINE.into(),
        })
    }

    /// What the page saved goes where Mobile keeps it (launcher.json), and into what the next launch asks.
    pub(crate) fn phone_save(&mut self, edits: &[(String, Option<String>)]) {
        let Some((page, _)) = self.emu.page.as_ref() else { return };
        let system = page.subject.system.clone();
        let refs: Vec<GameRef> = self.systems.iter().find(|s| s.name == system).into_iter().flat_map(|s| s.games.iter().filter_map(|g| game_ref(&s.name, &g.path))).collect();
        let mut prefs = (*play_route::prefs()).clone();
        play_route_cfg::apply_edits(&mut prefs, edits, &refs);
        play_route::set_prefs(prefs.clone());
        let path = self.phone_prefs_path();
        let _ = std::thread::Builder::new().name("plays-with".into()).spawn(move || {
            // The file is the Mobile view's: everything else in it stays as it is.
            let old = std::fs::read_to_string(&path).unwrap_or_default();
            if let Err(e) = pocket_ui::fsutil::write_atomic(&path, play_route::json_with_prefs(&old, &prefs).as_bytes()) {
                eprintln!("plays with: {}: {e}", path.display());
            }
        });
    }

    /// The first start's emulator apps (`android_first_start::take_offers`): kept as Plays with's choices, in launcher.json.
    pub(crate) fn phone_take_offers(&mut self, offers: &[pocket_ui::android_first_start::AppOffer]) {
        let mut prefs = (*play_route::prefs()).clone();
        if !pocket_ui::android_first_start::take_offers(&mut prefs.plays_with, offers) {
            return;
        }
        play_route::set_prefs(prefs.clone());
        let path = self.phone_prefs_path();
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        match pocket_ui::fsutil::write_atomic(&path, play_route::json_with_prefs(&old, &prefs).as_bytes()) {
            Ok(()) => println!("first start: plays with {:?}", offers.iter().map(|o| (o.system, &o.app.package)).collect::<Vec<_>>()),
            Err(e) => eprintln!("plays with: {}: {e}", path.display()),
        }
    }

    /// An emulator app's official page (`android_first_start::APP_DOWNLOADS`), in the browser: nothing is downloaded here.
    pub(crate) fn phone_open_page(&mut self, url: &str) {
        if !pocket_ui::android_first_start::official_page(url) {
            return;
        }
        #[cfg(all(target_os = "android", feature = "handheld"))]
        if let Err(e) = pocket_ui::platform::android::strings("openPage", &[url]) {
            eprintln!("open page: {e}");
        }
        println!("first start: official page {url}");
    }

    /// A games folder another app keeps, taken on the first start's "Your games": read from now on (Android: the host
    /// writes its system list again), the shelf and the page's counts follow.
    pub(crate) fn android_folder_added(&mut self, folder: &std::path::Path) {
        #[cfg(all(target_os = "android", feature = "handheld"))]
        super::app_android::add_games_folder(folder);
        #[cfg(not(all(target_os = "android", feature = "handheld")))]
        pocket_ui::android_first_start::keep_folder(&self.data_dir, folder);
        let catalog = pocket_ui::library::all_systems(&self.es_systems).unwrap_or_default();
        if let Some(s) = self.setup.as_mut() {
            s.set_catalog(catalog);
        }
        self.library_rescan("games folder added");
    }

    /// The phone's apps and cores again, from the host (an app was installed, a core came): Android only.
    pub(crate) fn phone_refresh(&self) {
        #[cfg(all(target_os = "android", feature = "handheld"))]
        super::app_android::refresh_phone();
    }

    /// A on the Download row: the host fetches the core (Android; elsewhere the channel is simply not there).
    pub(crate) fn phone_download(&mut self, core: &str) {
        let failed = |app: &mut App, why: &str| {
            if let Some((page, _)) = app.emu.page.as_mut() {
                page.download_state(Download::Failed(pocket_ui::i18n::tr(why)), None);
            }
        };
        #[cfg(all(target_os = "android", feature = "handheld"))]
        {
            match super::app_android::start_core_download(core, self.proxy.clone()) {
                Ok(()) => {}
                Err(why) => failed(self, &why),
            }
        }
        #[cfg(not(all(target_os = "android", feature = "handheld")))]
        {
            let _ = core;
            failed(self, "The system download channel is not available yet.");
        }
    }

    /// The download's progress and end, on the page that started it (called with every frame and every poll event).
    pub(crate) fn phone_poll(&mut self) {
        #[cfg(all(target_os = "android", feature = "handheld"))]
        {
            use super::app_android::CoreDownload;
            let Some(state) = super::app_android::core_download_poll() else { return };
            let Some((page, _)) = self.emu.page.as_mut() else { return };
            match state {
                CoreDownload::Running(line) => page.download_state(Download::Busy(line), None),
                CoreDownload::Done => page.download_state(Download::Idle, Some(true)),
                CoreDownload::Failed(why) => page.download_state(Download::Failed(pocket_ui::i18n::tr(&why)), None),
            }
            self.request_redraw();
        }
    }
}
