//! QA only (cargo feature `ra-replay`, never in a release APK): a RetroAchievements server that answers from the
//! device, so the whole path (sign in, identify the game, load its set, unlock, send, keep for later) can be proved on
//! hardware without an account. Real code runs everywhere else (rcheevos, the session, the offline copy); only the
//! network is replaced. `QA Replay Game` has three achievements that unlock by themselves after 5, 20 and 1800 frames (the last about half a minute into a game) (hit counts: no game memory
//! is needed; the session test `a_set_of_hit_counts_unlocks_by_itself_after_a_few_frames_and_masters_the_game` proves the set).
//!
//! Controlled from the outside with files in the app's external files folder (adb can write there):
//! - `ra-replay.mode` holding `offline`: every request fails as without a network (never switch the radio off on a
//!   device reached over Wi-Fi). Anything else, or no file: online.
//! - `ra-replay.log` (written here): one line per request, `<unix seconds> <request name> [achievement id | game hash]`;
//!   never a password, token or user name.
//! - `ra-replay.unlocked` (written here): the server's memory of what was unlocked, so a later session starts with it.
//!
//! A password of `wrong` is refused, as the site refuses one.
use pocket_ui::achievements::http::{Request, Response, Transport};
use pocket_ui::achievements::offline::param;
use std::io::Write;
use std::path::PathBuf;

const BADGE: [u8; 82] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220, 0, 0, 0, 25, 73, 68, 65, 84, 120, 156, 99,
    184, 60, 75, 71, 75, 75, 15, 147, 100, 192, 42, 10, 36, 25, 6, 165, 14, 0, 104, 46, 67, 97, 200, 172, 7, 128, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub struct Replay {
    pub dir: PathBuf,
    pub console: u32,
}

impl Replay {
    /// The replay for the player's console; its files live in the external files folder.
    pub fn new(console: u32) -> Replay {
        Replay { dir: PathBuf::from(pocket_ui::platform::android::string("diagnosticsPath")), console }
    }

    fn offline(&self) -> bool {
        std::fs::read_to_string(self.dir.join("ra-replay.mode")).is_ok_and(|m| m.trim() == "offline")
    }

    fn note(&self, line: &str) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("ra-replay.log")) {
            let _ = writeln!(f, "{now} {line}");
        }
    }

    fn unlocked(&self) -> Vec<(u32, u64)> {
        std::fs::read_to_string(self.dir.join("ra-replay.unlocked"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (id, when) = l.split_once(' ')?;
                Some((id.parse().ok()?, when.parse().ok()?))
            })
            .collect()
    }

    fn sets(&self) -> String {
        let one = |id: u32, title: &str, points: u32, addr: &str, badge: &str| {
            format!(r#"{{"ID":{id},"Title":"{title}","Description":"{title}: QA replay","Flags":3,"Points":{points},"MemAddr":"{addr}","Author":"QA","BadgeName":"{badge}","Created":1367266583,"Modified":1376929305}}"#)
        };
        let list = [one(99101, "First frames", 5, "1=1.5.", "90001"), one(99102, "Still running", 10, "1=1.20.", "90002"), one(99103, "Last of three", 25, "1=1.1800.", "90003")].join(",");
        format!(
            r#"{{"Success":true,"GameId":99001,"Title":"QA Replay Game","ConsoleId":{c},"ImageIconUrl":"https://qa.invalid/Images/90000.png","RichPresenceGameId":99001,"RichPresencePatch":"","Sets":[{{"AchievementSetId":99011,"GameId":99001,"Title":null,"Type":"core","ImageIconUrl":"https://qa.invalid/Images/90000.png","Achievements":[{list}],"Leaderboards":[]}}]}}"#,
            c = self.console
        )
    }
}

impl Transport for Replay {
    fn call(&self, req: &Request) -> Response {
        let post = req.post_data.as_deref().unwrap_or("");
        let name = param(post, "r").unwrap_or("?");
        if self.offline() {
            self.note(&format!("{name} offline"));
            return Response::unreachable();
        }
        match name {
            "login2" => {
                self.note("login2");
                if param(post, "p") == Some("wrong") {
                    return Response { status: 200, body: br#"{"Success":false,"Error":"Invalid user/password combination. Please try again.","Code":"invalid_credentials"}"#.to_vec() };
                }
                Response::ok(r#"{"Success":true,"User":"QA Replay","Token":"ReplayToken","Score":1,"SoftcoreScore":1,"Messages":0}"#)
            }
            "achievementsets" => {
                // The game's hash is no secret: it shows on hardware that a game opened without a copy, or from a zip,
                // is recognised by the same hash as the file itself.
                self.note(&format!("achievementsets hash={}", param(post, "m").unwrap_or("?")));
                Response::ok(self.sets())
            }
            "startsession" => {
                self.note("startsession");
                let unlocked: Vec<String> = self.unlocked().iter().map(|(id, when)| format!(r#"{{"ID":{id},"When":{when}}}"#)).collect();
                Response::ok(format!(r#"{{"Success":true,"Unlocks":[{}],"HardcoreUnlocks":[]}}"#, unlocked.join(",")))
            }
            "awardachievement" => {
                let id: u32 = param(post, "a").and_then(|a| a.parse().ok()).unwrap_or(0);
                self.note(&format!("awardachievement {id}"));
                let mut known = self.unlocked();
                if id != 0 && !known.iter().any(|(k, _)| *k == id) {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                    known.push((id, now));
                    let text: String = known.iter().map(|(i, w)| format!("{i} {w}\n")).collect();
                    let _ = std::fs::write(self.dir.join("ra-replay.unlocked"), text);
                }
                Response::ok(format!(r#"{{"Success":true,"Score":100,"SoftcoreScore":100,"AchievementID":{id},"AchievementsRemaining":{}}}"#, 3usize.saturating_sub(known.len())))
            }
            "ping" => Response::ok(r#"{"Success":true}"#),
            other => {
                self.note(&format!("{other} unanswered"));
                Response::unreachable()
            }
        }
    }

    fn download(&self, _url: &str, dest: &std::path::Path) -> bool {
        !self.offline() && std::fs::write(dest, BADGE).is_ok()
    }
}
