//! Achievements without network. A handheld is often offline when a game starts, and rc_client on its own would give
//! up then (no login, no game data). So the answers of the last online session are kept per game and stand in for the
//! server when it can't be reached:
//!
//! - `<data>/achievements/offline/<hash>.json`: the game's achievement set (`r=achievementsets`) and the player's unlocks
//!   (`r=startsession`), refreshed whenever the server answers, and an unlock confirmed by the server is added to it.
//! - `<data>/achievements/pending.json`: unlocks the server hasn't confirmed yet. Written as soon as an unlock is sent,
//!   removed when the server answers; the next session that gets online sends what is left (`api::award`, with the time
//!   it happened). Until then they count as unlocked, so the same achievement doesn't pop up twice.
//!
//! A token login that can't reach the server is answered from the account itself, so rc_client goes on to the game.
//! Nothing here invents an unlock or a server answer for anything else.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::account::Account;
use super::http::{Request, Response, Transport};

/// `key=value` of a request's POST body (URL-decoded enough for the keys used here: letters, digits, hashes).
pub fn param<'a>(post: &'a str, key: &str) -> Option<&'a str> {
    post.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
}

/// An unlock waiting for the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Unlock {
    pub id: u32,
    /// The game's hash, which the server wants with it.
    pub hash: String,
    /// Unix seconds when it happened.
    pub when: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Pending {
    pub username: String,
    pub unlocks: Vec<Unlock>,
}

impl Pending {
    pub fn path(data_dir: &Path) -> PathBuf {
        super::dir(data_dir).join("pending.json")
    }

    /// The waiting unlocks of `username` (another account's can't be sent with this token and are dropped).
    pub fn load(data_dir: &Path, username: &str) -> Pending {
        let p: Pending = std::fs::read(Self::path(data_dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if p.username == username {
            p
        } else {
            if !p.unlocks.is_empty() {
                eprintln!("achievements: {} unlocks of another account dropped", p.unlocks.len());
            }
            Pending {
                username: username.to_string(),
                unlocks: Vec::new(),
            }
        }
    }

    pub fn save(&self, data_dir: &Path) {
        let path = Self::path(data_dir);
        if self.unlocks.is_empty() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            if let Err(e) = crate::fsutil::write_atomic(&path, &bytes) {
                eprintln!("achievements: can't keep the unlocks to send: {e}");
            }
        }
    }

    pub fn add(&mut self, id: u32, hash: &str, when: u64) {
        if !self.unlocks.iter().any(|u| u.id == id && u.hash == hash) {
            self.unlocks.push(Unlock {
                id,
                hash: hash.to_string(),
                when,
            });
        }
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.unlocks.len();
        self.unlocks.retain(|u| u.id != id);
        before != self.unlocks.len()
    }

    pub fn ids_for(&self, hash: &str) -> Vec<u32> {
        self.unlocks.iter().filter(|u| u.hash.eq_ignore_ascii_case(hash)).map(|u| u.id).collect()
    }
}

/// What was kept of a game for playing it offline.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Kept {
    #[serde(default)]
    sets: Option<Value>,
    #[serde(default)]
    session: Option<Value>,
}

fn kept_path(data_dir: &Path, hash: &str) -> PathBuf {
    let safe: String = hash.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    super::dir(data_dir).join("offline").join(format!("{}.json", safe.to_ascii_lowercase()))
}

fn load_kept(data_dir: &Path, hash: &str) -> Kept {
    std::fs::read(kept_path(data_dir, hash))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_kept(data_dir: &Path, hash: &str, kept: &Kept) {
    if let Ok(bytes) = serde_json::to_vec(kept) {
        if let Err(e) = crate::fsutil::write_atomic(&kept_path(data_dir, hash), &bytes) {
            eprintln!("achievements: can't keep the game for offline play: {e}");
        }
    }
}

/// The server's JSON when it says Success.
fn success(resp: &Response) -> Option<Value> {
    if resp.status != 200 {
        return None;
    }
    let v: Value = serde_json::from_slice(&resp.body).ok()?;
    (v.get("Success") == Some(&Value::Bool(true))).then_some(v)
}

/// Adds `ids` to a startsession answer's softcore unlocks.
fn with_unlocks(mut session: Value, ids: &[u32], now: u64) -> Value {
    if let Some(obj) = session.as_object_mut() {
        let list = obj.entry("Unlocks").or_insert_with(|| json!([]));
        if let Some(arr) = list.as_array_mut() {
            for id in ids {
                if !arr.iter().any(|u| u.get("ID").and_then(Value::as_u64) == Some(*id as u64)) {
                    arr.push(json!({"ID": id, "When": now}));
                }
            }
        }
    }
    session
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// One request of rc_client: asks the server, keeps what is worth keeping, and stands in for it when it can't be
/// reached. `pending` are the unlocks of this account that wait for the server.
pub fn exchange(transport: &dyn Transport, data_dir: &Path, account: &Account, pending: &Pending, req: &Request) -> Response {
    let post = req.post_data.as_deref().unwrap_or_default();
    let resp = transport.call(req);
    let hash = param(post, "m").unwrap_or_default();
    match param(post, "r").unwrap_or_default() {
        // A token login without an answer: the account as it was last confirmed.
        "login2" if resp.failed() && param(post, "t").is_some() => {
            let name = if account.display_name.is_empty() {
                &account.username
            } else {
                &account.display_name
            };
            let body =
                json!({"Success": true, "User": account.username, "DisplayName": name, "Token": account.token, "Score": 0, "SoftcoreScore": 0, "Messages": 0});
            eprintln!("achievements: offline, signed in from the saved account");
            Response::ok(body.to_string())
        }
        "achievementsets" if !hash.is_empty() => {
            if let Some(v) = success(&resp) {
                let mut kept = load_kept(data_dir, hash);
                kept.sets = Some(v);
                save_kept(data_dir, hash, &kept);
                resp
            } else if resp.failed() {
                match load_kept(data_dir, hash).sets {
                    Some(v) => {
                        eprintln!("achievements: offline, using the achievements kept from the last time");
                        Response::ok(v.to_string())
                    }
                    None => resp,
                }
            } else {
                resp
            }
        }
        "startsession" if !hash.is_empty() => {
            let waiting = pending.ids_for(hash);
            if let Some(v) = success(&resp) {
                let mut kept = load_kept(data_dir, hash);
                kept.session = Some(v.clone());
                save_kept(data_dir, hash, &kept);
                // Unlocks still on their way count as unlocked already.
                Response::ok(with_unlocks(v, &waiting, now()).to_string())
            } else if resp.failed() {
                match load_kept(data_dir, hash).session {
                    Some(v) => Response::ok(with_unlocks(v, &waiting, now()).to_string()),
                    None => resp,
                }
            } else {
                resp
            }
        }
        // Confirmed: the kept copy learns it, so an offline start next time knows.
        "awardachievement" if !hash.is_empty() => {
            if let (Some(_), Some(id)) = (success(&resp), param(post, "a").and_then(|a| a.parse::<u32>().ok())) {
                let mut kept = load_kept(data_dir, hash);
                if let Some(s) = kept.session.take() {
                    kept.session = Some(with_unlocks(s, &[id], now()));
                    save_kept(data_dir, hash, &kept);
                }
            }
            resp
        }
        _ => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::api::tests::Mock;
    use crate::testutil::TestDir;

    fn scratch(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("ra-offline-{name}"))
    }

    fn account() -> Account {
        Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: String::new(),
        }
    }

    fn req(post: &str) -> Request {
        Request {
            url: "https://retroachievements.org/dorequest.php".into(),
            post_data: Some(post.into()),
            content_type: None,
        }
    }

    const SETS: &str = r#"{"Success":true,"GameId":1234,"Title":"Sample Game","Sets":[]}"#;
    const SESSION: &str = r#"{"Success":true,"Unlocks":[{"ID":5502,"When":1234567899}],"HardcoreUnlocks":[]}"#;

    fn ids(resp: &Response) -> Vec<u64> {
        let v: Value = serde_json::from_slice(&resp.body).unwrap();
        v["Unlocks"].as_array().unwrap().iter().map(|u| u["ID"].as_u64().unwrap()).collect()
    }

    #[test]
    fn what_the_server_said_online_answers_offline() {
        let dir = scratch("keep");
        let online = Mock::new(vec![
            ("achievementsets", Response::ok(SETS)),
            ("startsession", Response::ok(SESSION)),
            ("awardachievement", Response::ok(r#"{"Success":true}"#)),
        ]);
        let offline = Mock::new(vec![]);
        let none = Pending::default();
        let sets = req("r=achievementsets&u=Alex&t=Tk&m=0123abcd");
        let session = req("r=startsession&u=Alex&t=Tk&g=1234&h=0&m=0123abcd&l=12.5");
        // Never online for this game: offline stays offline.
        assert!(exchange(&offline, &dir, &account(), &none, &sets).failed());
        assert_eq!(exchange(&online, &dir, &account(), &none, &sets), Response::ok(SETS));
        assert_eq!(ids(&exchange(&online, &dir, &account(), &none, &session)), vec![5502]);
        let from_cache = exchange(&offline, &dir, &account(), &none, &sets);
        assert_eq!(from_cache.status, 200);
        assert_eq!(serde_json::from_slice::<Value>(&from_cache.body).unwrap()["Title"], "Sample Game");
        // An unlock the server confirmed is known offline next time.
        exchange(
            &online,
            &dir,
            &account(),
            &none,
            &req("r=awardachievement&u=Alex&t=Tk&a=5501&h=0&m=0123abcd&v=x"),
        );
        assert_eq!(ids(&exchange(&offline, &dir, &account(), &none, &session)), vec![5502, 5501]);
        // One still waiting for the server counts too, online and offline, and only for its game.
        let mut waiting = Pending {
            username: "Alex".into(),
            unlocks: Vec::new(),
        };
        waiting.add(5503, "0123ABCD", 1);
        waiting.add(9999, "ffff", 1);
        assert_eq!(ids(&exchange(&offline, &dir, &account(), &waiting, &session)), vec![5502, 5501, 5503]);
        assert_eq!(ids(&exchange(&online, &dir, &account(), &waiting, &session)), vec![5502, 5503]);
    }

    #[test]
    fn a_token_login_without_network_uses_the_saved_account() {
        let dir = scratch("login");
        let offline = Mock::new(vec![]);
        let r = exchange(&offline, &dir, &account(), &Pending::default(), &req("r=login2&u=Alex&t=Tk"));
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!((v["Success"].as_bool(), v["Token"].as_str()), (Some(true), Some("Tk")));
        // A password login (Settings) is never answered for the server, nor is a refused token.
        assert!(exchange(&offline, &dir, &account(), &Pending::default(), &req("r=login2&u=Alex&p=pw")).failed());
        let refused = Mock::new(vec![(
            "login2",
            Response {
                status: 401,
                body: br#"{"Success":false,"Code":"invalid_credentials"}"#.to_vec(),
            },
        )]);
        assert_eq!(
            exchange(&refused, &dir, &account(), &Pending::default(), &req("r=login2&u=Alex&t=Old")).status,
            401
        );
        // Anything else is passed through as it came.
        assert!(exchange(&offline, &dir, &account(), &Pending::default(), &req("r=ping&u=Alex&t=Tk&g=1")).failed());
    }

    #[test]
    fn waiting_unlocks_belong_to_one_account_and_go_when_sent() {
        let dir = scratch("pending");
        let mut p = Pending::load(&dir, "Alex");
        assert!(p.unlocks.is_empty());
        p.add(5501, "abc", 10);
        p.add(5501, "abc", 11);
        p.add(5502, "abc", 12);
        assert_eq!(p.unlocks.len(), 2, "the same unlock once");
        p.save(&dir);
        assert_eq!(Pending::load(&dir, "Alex"), p);
        assert!(Pending::load(&dir, "Someone").unlocks.is_empty(), "not sent with another account's token");
        assert!(p.remove(5501) && !p.remove(5501));
        p.remove(5502);
        p.save(&dir);
        assert!(!Pending::path(&dir).exists(), "nothing left, no file");
        assert_eq!(param("r=login2&u=Alex&t=Tk", "u"), Some("Alex"));
        assert_eq!(param("r=login2&u=Alex", "t"), None);
    }
}
