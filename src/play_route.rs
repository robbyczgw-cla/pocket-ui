//! Which way a game starts on Android, decided in one place for the Mobile view and the handheld view.
//!
//! A game starts in one of four ways: in pocket-ui's own player with a core that is in the APK or already downloaded
//! (`Way::Own`), in the same player once its core has been downloaded (`Way::NeedsCore`), in another emulator app the
//! player chose for the system or for this game (`Way::App`), or, for a system the own player doesn't have (PS2, GameCube
//! and Wii, 3DS, Xbox), in the first installed app that takes it (`Way::App` again) or nowhere (`Way::NoApp`).
//!
//! The Mobile view (`android/src`) asks `chosen_app`; the handheld view asks `Phone::way`, shows the same ways on its
//! "Runs on" page (`runs_on_android`, `app_runs_on_android`) and stores the choices where Mobile does: `plays_with` (system →
//! package) and `plays_with_game` (game key → package, or `OWN`) in launcher.json. The two views never disagree about
//! what starts a game because neither has a second rule.
//!
//! Everything here is plain data, so every path has a test on Linux; what the phone has (its apps, the cores on disk, the
//! download channel) is read by the host and handed in as a `Phone`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};


/// What `plays_with_game` holds for a game that keeps pocket-ui's own player while its system plays in an app.

/// The systems our own player on Android plays without a release qualification yet: the one list behind the Mobile view's
/// "Experimental" (`play_cards::experimental`) and the handheld view's system names (`play_route_cfg::es_systems`). Nintendo
/// DS left it on 12.11.2026 (three games, 180 s each, zero underruns, Continue with sound: ANDROID-BIG-SYSTEMS.md).
pub fn experimental(system: &str) -> bool {
    matches!(system, "n64" | "psp" | "saturn" | "dreamcast")
}

pub const OWN: &str = "own";

/// An emulator app on the phone: its package and the name the phone shows for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneApp {
    pub package: String,
    pub label: String,
}

/// A core the download channel offers: its id, its name and its size in bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreOffer {
    pub core: String,
    pub name: String,
    pub size: u64,
}

/// What the phone has, as the host last read it.
#[derive(Clone, Debug, Default)]
pub struct Phone {
    /// The apps that take a system, by emulator profile (`document_library::system`'s third value), only installed ones.
    pub apps: HashMap<String, Vec<PhoneApp>>,
    /// The cores that are on the phone now (`corePath` is not empty), bundled or downloaded.
    pub cores: HashSet<String>,
    /// What the download channel offers, and whether it is open (no address: "Not available here").
    pub offers: Vec<CoreOffer>,
    pub channel_open: bool,
    /// Settings → Game Boy core: Gambatte for Game Boy and Game Boy Color.
    pub gambatte: bool,
}

/// What a game starts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Way {
    /// pocket-ui's own player with this core, which is on the phone.
    Own { core: String },
    /// pocket-ui's own player, but its core is not on the phone yet (a download core).
    NeedsCore { core: String },
    /// Another app starts it.
    App(PhoneApp),
    /// A system the own player doesn't have, and no installed app takes it.
    NoApp { profile: String },
}

/// pocket-ui's own core for a system (android/cores.lock.json: the APK's bundled cores and the optional downloads).
pub fn bundled_core(system: &str) -> Option<&'static str> {
    static CORES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    CORES
        .get_or_init(|| serde_json::from_str(include_str!("../android/cores.lock.json")).unwrap_or_default())
        .get("cores")?
        .as_array()?
        .iter()
        .find(|c| c["systems"].as_array().is_some_and(|s| s.iter().any(|s| s.as_str() == Some(system))))?
        .get("id")?
        .as_str()
}

/// The core our own player starts a system with: `bundled_core`, or Gambatte for the Game Boys when Settings says so.
pub fn own_core(system: &str, gambatte: bool) -> Option<&'static str> {
    if gambatte && matches!(system, "gb" | "gbc") {
        Some("gambatte")
    } else {
        bundled_core(system)
    }
}

/// Named built-in DS choices share the same preference map as installed apps.
pub fn own_choice(value:&str)->bool {
    value==OWN || matches!(value,"own:dsperate"|"own:melondsds")
}

pub fn selected_own_core(system:&str,gambatte:bool,game:Option<&str>,console:Option<&str>)->Option<&'static str> {
    if system=="nds" {
        let chosen=game.filter(|c|own_choice(c)).or(console);
        match chosen {
            Some("own:dsperate")=>return Some("dsperate"),
            Some("own:melondsds")=>return Some("melondsds"),
            _=>{},
        }
    }
    own_core(system,gambatte)
}

/// The emulator profile of a system, as `installedApps` asks for it ("psp", "dolphin", "retroarch:mupen64plus_next").
/// `system` is the library's id (the Mobile view's) or ROCKNIX's name ("wii" is GameCube / Wii here).
pub fn profile_of(system: &str) -> Option<&'static str> {
    crate::document_library::system(system).map(|s| s.2)
}

/// The Mobile view's id of a system, from ROCKNIX's name: the key `plays_with` is stored under.
pub fn system_key(system: &str) -> &'static str {
    crate::document_library::system(system).map_or("", |s| s.0)
}

/// The profiles of the given systems that have a way besides our player, comma separated: the question for
/// `installedApps`.
pub fn profiles_asked<'a>(systems: impl IntoIterator<Item = &'a str>) -> String {
    let mut profiles: Vec<&str> = systems.into_iter().filter_map(profile_of).collect();
    profiles.sort_unstable();
    profiles.dedup();
    profiles.join(",")
}

/// The app a game starts in when one is chosen and still on the phone, else None (the own player, or, without an own
/// player, the caller's first app). The game's own choice first, then its system's. `own`: the system has a player of
/// ours, so `OWN` means something.
///
/// An app that is gone from the phone is no choice: the system's way applies, and the choice holds again when the app
/// comes back (it is never removed from the file).
pub fn chosen_app<'a>(own: bool, game: Option<&str>, system: Option<&str>, apps: &'a [PhoneApp]) -> Option<&'a PhoneApp> {
    if let Some(chosen) = game {
        if own_choice(chosen) {
            if own {
                return None;
            }
        } else if let Some(app) = apps.iter().find(|a| a.package == chosen) {
            return Some(app);
        }
    }
    let chosen = system?;
    apps.iter().find(|a| a.package == chosen)
}

impl Phone {
    /// The apps on the phone that take this system.
    pub fn apps_for(&self, system: &str) -> &[PhoneApp] {
        profile_of(system).and_then(|p| self.apps.get(p)).map_or(&[], Vec::as_slice)
    }

    pub fn offer(&self, core: &str) -> Option<&CoreOffer> {
        self.offers.iter().find(|o| o.core == core)
    }

    /// What a game of `system` starts in, given its own choice and its system's (`plays_with_game`, `plays_with`).
    pub fn way(&self, system: &str, game: Option<&str>, system_choice: Option<&str>) -> Way {
        let apps = self.apps_for(system);
        let own = selected_own_core(system, self.gambatte, game, system_choice);
        if let Some(app) = chosen_app(own.is_some(), game, system_choice, apps) {
            return Way::App(app.clone());
        }
        match own {
            Some(core) if self.cores.contains(core) => Way::Own { core: core.to_string() },
            Some(core) => Way::NeedsCore { core: core.to_string() },
            None => match apps.first() {
                Some(app) => Way::App(app.clone()),
                None => Way::NoApp { profile: profile_of(system).unwrap_or_default().to_string() },
            },
        }
    }

    /// The phone as a test or a snapshot describes it: `{"apps": {"psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}]},
    /// "cores": ["mgba"], "offers": [{"core": "ppsspp", "name": "PSP", "size": 18600000}], "channel_open": true}`.
    pub fn from_json(json: &str) -> Phone {
        let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        let mut phone = Phone { channel_open: v["channel_open"].as_bool().unwrap_or(false), gambatte: v["gambatte"].as_bool().unwrap_or(false), ..Phone::default() };
        phone.set_installed_apps(&v["apps"].to_string());
        phone.cores = v["cores"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(str::to_string)).collect();
        phone.offers = v["offers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| Some(CoreOffer { core: o["core"].as_str()?.to_string(), name: o["name"].as_str().unwrap_or_default().to_string(), size: o["size"].as_u64().unwrap_or(0) }))
            .collect();
        phone
    }

    /// `installedApps`'s answer: `{"psp": [{"p": package, "l": name}]}` (a profile with no app is left out).
    pub fn set_installed_apps(&mut self, json: &str) {
        let read: HashMap<String, Vec<serde_json::Value>> = serde_json::from_str(json).unwrap_or_default();
        self.apps = read
            .into_iter()
            .map(|(profile, apps)| {
                let apps = apps
                    .iter()
                    .filter_map(|a| Some(PhoneApp { package: a["p"].as_str()?.to_string(), label: short(a["l"].as_str().unwrap_or_default(), a["p"].as_str()?) }))
                    .collect();
                (profile, apps)
            })
            .collect();
    }
}

/// The name of an app in a row: the phone's label without a "(...)" tail, in words that fit beside a system's name on a
/// narrow screen (the package when the phone gave none).
pub fn short(label: &str, package: &str) -> String {
    let label = label.split('(').next().unwrap_or_default().trim();
    let mut out = String::new();
    for word in label.split_whitespace() {
        if !out.is_empty() && out.chars().count() + 1 + word.chars().count() > 16 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() { package.to_string() } else { out }
}

/// The emulator profiles of every system the library knows: the question for `installedApps`.
pub fn all_profiles() -> String {
    profiles_asked(crate::document_library::SYSTEM_IDS)
}

/// A game's key as the Mobile view has it (`Game::key`, the storage provider's `authority:id`), made from the file's real
/// path, so a choice made for a game in one view holds in the other. A path outside `/storage` (a test's folder) is used
/// as it is.
pub fn game_key(path: &std::path::Path) -> Option<String> {
    let path = path.to_str()?;
    let id = if let Some(rest) = path.strip_prefix("/storage/emulated/0/") {
        format!("primary:{rest}")
    } else if let Some((volume, inner)) = path.strip_prefix("/storage/").and_then(|r| r.split_once('/')) {
        format!("{volume}:{inner}")
    } else {
        path.to_string()
    };
    Some(format!("com.android.externalstorage.documents:{id}"))
}

// ---- what the host knows now ----------------------------------------------------------------------------------------

static PHONE: RwLock<Option<Arc<Phone>>> = RwLock::new(None);

/// The host read the phone (at start, after a scan, back in front, after a download).
pub fn set_phone(phone: Phone) {
    *PHONE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(phone));
}

/// What the host last read; None where there is no phone (every Linux handheld).
pub fn phone() -> Option<Arc<Phone>> {
    PHONE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

static PREFS: RwLock<Option<Arc<Prefs>>> = RwLock::new(None);

/// The choices as launcher.json held them when the host last read it (at start, after a save).
pub fn prefs() -> Arc<Prefs> {
    PREFS.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

pub fn set_prefs(prefs: Prefs) {
    *PREFS.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(prefs));
}

// ---- the choices in launcher.json -------------------------------------------------------------------------------------

/// `plays_with` and `plays_with_game` of launcher.json, the two keys both views read and write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub plays_with: BTreeMap<String, String>,
    pub plays_with_game: BTreeMap<String, String>,
}

fn string_map(v: &serde_json::Value) -> BTreeMap<String, String> {
    v.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()
}

/// The choices of a launcher.json text; a missing, broken or oddly shaped value means no choice (the rest of the file is
/// the Mobile view's and stays as it is).
pub fn prefs_from_json(json: &str) -> Prefs {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    Prefs { plays_with: string_map(&v["plays_with"]), plays_with_game: string_map(&v["plays_with_game"]) }
}

/// `settings.gameboy_gambatte` of the same file.
pub fn gambatte_from_json(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| v["gameboy_gambatte"].as_bool()).unwrap_or(false)
}

/// The launcher.json text with these choices and everything else as it was. An empty map leaves no key behind.
pub fn json_with_prefs(old: &str, prefs: &Prefs) -> String {
    let mut v: serde_json::Value = serde_json::from_str(old).ok().filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    for (key, map) in [("plays_with", &prefs.plays_with), ("plays_with_game", &prefs.plays_with_game)] {
        if map.is_empty() {
            v.as_object_mut().map(|o| o.remove(key));
        } else {
            v[key] = serde_json::json!(map);
        }
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ds_choices_are_shared_by_both_android_views_and_keep_melonds_as_default() {
        use super::*;
        let phone=Phone {cores:["dsperate".into(),"melondsds".into()].into(),..Default::default()};
        assert_eq!(phone.way("nds",None,None),Way::Own {core:"melondsds".into()});
        assert_eq!(phone.way("nds",None,Some("own:dsperate")),Way::Own {core:"dsperate".into()});
        assert_eq!(phone.way("nds",Some("own:melondsds"),Some("own:dsperate")),Way::Own {core:"melondsds".into()});
        assert_eq!(phone.way("nds",Some(OWN),Some("own:dsperate")),Way::Own {core:"melondsds".into()});
        let apps=vec![PhoneApp {package:"test.ds.app".into(),label:"Installed DS app".into()}];
        assert!(chosen_app(true,Some("own:dsperate"),Some("test.ds.app"),&apps).is_none());
    }
    use super::*;

    fn app(package: &str, label: &str) -> PhoneApp {
        PhoneApp { package: package.into(), label: label.into() }
    }

    /// A phone like Alex's Odin: the APK's cores, PSP not downloaded, PPSSPP and Dolphin installed.
    fn odin() -> Phone {
        Phone::from_json(
            r#"{"apps": {"psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}], "dolphin": [{"p": "org.dolphinemu.dolphinemu", "l": "Dolphin Emulator"}],
                "ps2": [{"p": "com.armsx2", "l": "ARMSX2"}, {"p": "xyz.aethersx2.android", "l": "AetherSX2"}]},
                "cores": ["mgba", "bsnes", "nestopia", "pcsx_rearmed"], "offers": [{"core": "ppsspp", "name": "PSP", "size": 18600000}],
                "channel_open": true}"#,
        )
    }

    #[test]
    fn a_bundled_core_starts_in_our_player_whatever_apps_the_phone_has() {
        let phone = odin();
        assert_eq!(phone.way("gba", None, None), Way::Own { core: "mgba".into() });
        assert_eq!(phone.way("psx", None, None), Way::Own { core: "pcsx_rearmed".into() });
    }

    #[test]
    fn a_download_core_that_is_not_on_the_phone_is_a_download_not_a_dead_end() {
        let phone = odin();
        assert_eq!(phone.way("n64", None, None), Way::NeedsCore { core: "mupen64plus_next".into() });
        assert_eq!(phone.way("psp", None, None), Way::NeedsCore { core: "ppsspp".into() });
        // The same system once the core is downloaded.
        let mut phone = phone;
        phone.cores.insert("ppsspp".into());
        assert_eq!(phone.way("psp", None, None), Way::Own { core: "ppsspp".into() });
    }

    #[test]
    fn a_chosen_app_that_is_installed_wins_over_our_core_even_before_it_is_downloaded() {
        let phone = odin();
        assert_eq!(phone.way("psp", None, Some("org.ppsspp.ppsspp")), Way::App(app("org.ppsspp.ppsspp", "PPSSPP")));
        // The choice of a system without an own core: the first app is the way, a chosen other one beats it.
        assert_eq!(phone.way("ps2", None, None), Way::App(app("com.armsx2", "ARMSX2")));
        assert_eq!(phone.way("ps2", None, Some("xyz.aethersx2.android")), Way::App(app("xyz.aethersx2.android", "AetherSX2")));
    }

    #[test]
    fn a_choice_of_an_app_that_is_gone_means_the_system_goes_back_to_its_own_way() {
        let phone = odin();
        assert_eq!(phone.way("psp", None, Some("org.ppsspp.ppssppgold")), Way::NeedsCore { core: "ppsspp".into() });
        assert_eq!(phone.way("gba", None, Some("com.retroarch.aarch64")), Way::Own { core: "mgba".into() });
        assert_eq!(phone.way("ps2", None, Some("gone.app")), Way::App(app("com.armsx2", "ARMSX2")));
    }

    #[test]
    fn a_game_choice_comes_before_its_system_and_own_is_a_choice_where_we_have_a_player() {
        let phone = odin();
        let sys = Some("org.ppsspp.ppsspp");
        assert_eq!(phone.way("psp", Some(OWN), sys), Way::NeedsCore { core: "ppsspp".into() });
        assert_eq!(phone.way("psp", Some("org.ppsspp.ppsspp"), None), Way::App(app("org.ppsspp.ppsspp", "PPSSPP")));
        // OWN for a system without a player of ours falls through to the system's way.
        assert_eq!(phone.way("ps2", Some(OWN), Some("xyz.aethersx2.android")), Way::App(app("xyz.aethersx2.android", "AetherSX2")));
        // A game's app that is gone: its system's choice applies.
        assert_eq!(phone.way("psp", Some("gone.app"), sys), Way::App(app("org.ppsspp.ppsspp", "PPSSPP")));
    }

    #[test]
    fn a_system_nobody_plays_says_so() {
        let phone = Phone::default();
        assert_eq!(phone.way("ps2", None, None), Way::NoApp { profile: "ps2".into() });
        assert_eq!(phone.way("wii", None, None), Way::NoApp { profile: "dolphin".into() });
    }

    #[test]
    fn game_boy_core_follows_the_setting() {
        let mut phone = Phone::from_json(r#"{"cores": ["mgba", "gambatte"]}"#);
        assert_eq!(phone.way("gb", None, None), Way::Own { core: "mgba".into() });
        phone.gambatte = true;
        assert_eq!(phone.way("gb", None, None), Way::Own { core: "gambatte".into() });
        assert_eq!(phone.way("gba", None, None), Way::Own { core: "mgba".into() });
    }

    #[test]
    fn mobile_and_handheld_ask_the_same_function() {
        let apps = vec![app("a", "A"), app("b", "B")];
        assert_eq!(chosen_app(true, None, Some("b"), &apps), Some(&apps[1]));
        assert_eq!(chosen_app(true, Some("a"), Some("b"), &apps), Some(&apps[0]));
        assert_eq!(chosen_app(true, Some(OWN), Some("b"), &apps), None);
        assert_eq!(chosen_app(false, Some(OWN), Some("b"), &apps), Some(&apps[1]));
        assert_eq!(chosen_app(true, None, None, &apps), None);
        assert_eq!(chosen_app(true, None, Some("c"), &apps), None);
        assert_eq!(chosen_app(true, None, Some("a"), &[]), None);
    }

    #[test]
    fn the_bundled_cores_are_the_apks_lock_file() {
        assert_eq!(bundled_core("gba"), Some("mgba"));
        assert_eq!(bundled_core("psp"), Some("ppsspp"));
        assert_eq!(bundled_core("nds"), Some("melondsds"));
        for none in ["ps2", "gamecube", "wii", "3ds", "xbox", "switch", ""] {
            assert_eq!(bundled_core(none), None, "{none}");
        }
    }

    #[test]
    fn systems_have_the_phones_names_in_either_spelling() {
        assert_eq!(profile_of("wii"), Some("dolphin"));
        assert_eq!(system_key("wii"), "gamecube");
        assert_eq!(system_key("gba"), "gba");
        assert_eq!(profiles_asked(["psp", "gba", "wii", "psp", "nonsense"]), "dolphin,psp,retroarch:mgba");
    }

    #[test]
    fn the_choices_live_in_launcher_json_next_to_everything_mobile_keeps_there() {
        let old = r#"{"theme": "Big", "plays_with": {"psp": "org.ppsspp.ppsspp"}, "plays_with_game": {"00000000000000aa": "own"}, "light": true}"#;
        let mut prefs = prefs_from_json(old);
        assert_eq!(prefs.plays_with.get("psp").map(String::as_str), Some("org.ppsspp.ppsspp"));
        assert_eq!(prefs.plays_with_game.get("00000000000000aa").map(String::as_str), Some(OWN));
        prefs.plays_with.insert("gamecube".into(), "org.dolphinemu.dolphinemu".into());
        let back: serde_json::Value = serde_json::from_str(&json_with_prefs(old, &prefs)).unwrap();
        assert_eq!(back["theme"], "Big");
        assert_eq!(back["light"], true);
        assert_eq!(back["plays_with"]["gamecube"], "org.dolphinemu.dolphinemu");
        // Nothing chosen leaves no key, as a Mobile file that never chose.
        let empty: serde_json::Value = serde_json::from_str(&json_with_prefs(old, &Prefs::default())).unwrap();
        assert!(empty.get("plays_with").is_none() && empty.get("plays_with_game").is_none() && empty["theme"] == "Big");
        // A missing or broken file, an odd value: no choice, no crash, and a file the other view can read.
        assert_eq!(prefs_from_json(""), Prefs::default());
        assert_eq!(prefs_from_json("{not json"), Prefs::default());
        assert_eq!(prefs_from_json(r#"{"plays_with": 5, "plays_with_game": ["x"]}"#), Prefs::default());
        let fresh: serde_json::Value = serde_json::from_str(&json_with_prefs("{broken", &prefs)).unwrap();
        assert_eq!(fresh["plays_with"]["gamecube"], "org.dolphinemu.dolphinemu");
    }

    #[test]
    fn a_game_has_the_key_mobile_gives_the_same_file() {
        let key = |p: &str| game_key(std::path::Path::new(p));
        assert_eq!(key("/storage/emulated/0/ROMs/psp/Alpha.iso").as_deref(), Some("com.android.externalstorage.documents:primary:ROMs/psp/Alpha.iso"));
        assert_eq!(key("/storage/ABCD-1234/ROMs/psp/Alpha.iso").as_deref(), Some("com.android.externalstorage.documents:ABCD-1234:ROMs/psp/Alpha.iso"));
        assert!(key("/tmp/card/roms/psp/Alpha.iso").is_some());
    }

    #[test]
    fn the_host_hands_in_what_it_read() {
        // One process-wide reading, replaced whole (the tests run side by side: only this one touches it).
        set_phone(odin());
        let phone = phone().expect("read");
        assert!(phone.channel_open);
        assert_eq!(phone.offer("ppsspp").map(|o| o.size), Some(18_600_000));
        assert_eq!(short("Dolphin Emulator (Android)", "x"), "Dolphin Emulator");
        assert_eq!(short("", "org.x"), "org.x");
        assert_eq!(short("A very long emulator application name", "x"), "A very long");
    }
}
