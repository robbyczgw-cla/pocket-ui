//! Single requests outside a game, built and read by rcheevos' rapi (src/rc_shim.c): the login in Settings, looking a
//! game up for its card, and sending unlocks an earlier session couldn't. Each blocks its (background) thread for one
//! request through a [`Transport`].

use std::ffi::{c_char, c_int, c_void, CString};

use super::account::Account;
use super::ffi;
use super::http::{Request, Response, Transport};

/// Why a login failed, as the Settings page tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginError {
    /// User name or password wrong (the server's words).
    Credentials(String),
    /// No answer: no network, or the server is down.
    Unreachable,
    /// Any other answer the server gave.
    Server(String),
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginError::Credentials(m) | LoginError::Server(m) => f.write_str(m),
            LoginError::Unreachable => f.write_str("Couldn't reach RetroAchievements. Is the Pocket online?"),
        }
    }
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

unsafe extern "C" fn take_request(userdata: *mut c_void, url: *const c_char, post: *const c_char, ctype: *const c_char) {
    let out = &mut *(userdata as *mut Option<Request>);
    *out = Some(Request {
        url: ffi::text(url),
        post_data: (!post.is_null()).then(|| ffi::text(post)),
        content_type: (!ctype.is_null()).then(|| ffi::text(ctype)),
    });
}

/// Builds a request with one of the shim's `pocket_rc_*_request` functions.
fn build(f: impl FnOnce(ffi::RequestSink, *mut c_void) -> c_int) -> Option<Request> {
    let mut out: Option<Request> = None;
    let rc = f(take_request, &mut out as *mut Option<Request> as *mut c_void);
    (rc == ffi::RC_OK).then_some(out).flatten()
}

/// The server's error text out of the shim's buffer.
fn error_of(buf: &[u8]) -> String {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// The login request for `username` and `password` (the password goes into the POST body only).
pub fn login_request(username: &str, password: &str) -> Option<Request> {
    let (u, p) = (cstr(username), cstr(password));
    build(|sink, ud| unsafe { ffi::pocket_rc_login_request(u.as_ptr(), p.as_ptr(), sink, ud) })
}

/// Reads the server's answer to a login.
pub fn login_response(resp: &Response) -> Result<Account, LoginError> {
    if resp.failed() {
        return Err(LoginError::Unreachable);
    }
    unsafe extern "C" fn sink(ud: *mut c_void, user: *const c_char, display: *const c_char, token: *const c_char, _score: u32, _soft: u32) {
        let out = &mut *(ud as *mut Option<Account>);
        let username = ffi::text(user);
        let display = ffi::text(display);
        *out = Some(Account {
            display_name: if display == username { String::new() } else { display },
            username,
            token: ffi::text(token),
        });
    }
    let mut out: Option<Account> = None;
    let mut err = [0u8; 256];
    let rc = unsafe {
        ffi::pocket_rc_login_response(
            resp.body.as_ptr() as *const c_char,
            resp.body.len(),
            resp.status,
            sink,
            &mut out as *mut Option<Account> as *mut c_void,
            err.as_mut_ptr() as *mut c_char,
            err.len(),
        )
    };
    match (rc, out) {
        (ffi::RC_OK, Some(a)) => Ok(a),
        (ffi::RC_INVALID_CREDENTIALS | ffi::RC_EXPIRED_TOKEN | ffi::RC_ACCESS_DENIED, _) => Err(LoginError::Credentials(error_of(&err))),
        (ffi::RC_API_FAILURE, _) if resp.status == 401 || resp.status == 403 => Err(LoginError::Credentials(error_of(&err))),
        (ffi::RC_NO_RESPONSE, _) => Err(LoginError::Unreachable),
        _ => Err(LoginError::Server(error_of(&err))),
    }
}

/// Signs in with the password once and returns the account with its token. The password is not kept.
pub fn login(transport: &dyn Transport, username: &str, password: &str) -> Result<Account, LoginError> {
    let req = login_request(username.trim(), password).ok_or_else(|| LoginError::Server("couldn't build the request".into()))?;
    login_response(&transport.call(&req))
}

/// The RetroAchievements game id of a hash; Ok(0) when the server knows no game for it. Err: no answer.
pub fn resolve_hash(transport: &dyn Transport, hash: &str) -> Result<u32, String> {
    let h = cstr(hash);
    let req = build(|sink, ud| unsafe { ffi::pocket_rc_resolve_hash_request(h.as_ptr(), sink, ud) }).ok_or("request")?;
    let resp = transport.call(&req);
    if resp.failed() {
        return Err("unreachable".into());
    }
    let mut id = 0u32;
    let mut err = [0u8; 256];
    let rc = unsafe {
        ffi::pocket_rc_resolve_hash_response(
            resp.body.as_ptr() as *const c_char,
            resp.body.len(),
            resp.status,
            &mut id,
            err.as_mut_ptr() as *mut c_char,
            err.len(),
        )
    };
    if rc == ffi::RC_OK {
        Ok(id)
    } else {
        Err(error_of(&err))
    }
}

/// One game's numbers from the all-progress list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub game_id: u32,
    pub total: u32,
    pub unlocked: u32,
}

/// The player's numbers for every game of a console that has a set (one request for the whole console).
pub fn all_progress(transport: &dyn Transport, account: &Account, console_id: u32) -> Result<Vec<Progress>, String> {
    let (u, t) = (cstr(&account.username), cstr(&account.token));
    let req = build(|sink, ud| unsafe { ffi::pocket_rc_all_progress_request(u.as_ptr(), t.as_ptr(), console_id, sink, ud) }).ok_or("request")?;
    let resp = transport.call(&req);
    if resp.failed() {
        return Err("unreachable".into());
    }
    unsafe extern "C" fn sink(ud: *mut c_void, game_id: u32, total: u32, unlocked: u32, unlocked_hc: u32) {
        let out = &mut *(ud as *mut Vec<Progress>);
        // An unlock in hardcore counts as one in softcore too.
        out.push(Progress {
            game_id,
            total,
            unlocked: unlocked.max(unlocked_hc),
        });
    }
    let mut out: Vec<Progress> = Vec::new();
    let mut err = [0u8; 256];
    let rc = unsafe {
        ffi::pocket_rc_all_progress_response(
            resp.body.as_ptr() as *const c_char,
            resp.body.len(),
            resp.status,
            sink,
            &mut out as *mut Vec<Progress> as *mut c_void,
            err.as_mut_ptr() as *mut c_char,
            err.len(),
        )
    };
    if rc == ffi::RC_OK {
        Ok(out)
    } else {
        Err(error_of(&err))
    }
}

/// What became of an unlock sent after the session it happened in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Award {
    /// The server has it (or had it already).
    Done,
    /// No answer: keep it for the next time.
    Later,
    /// The server refused it for good (an achievement that no longer exists, …): drop it.
    Refused(String),
}

/// Sends one unlock (softcore) that happened `seconds_ago`.
pub fn award(transport: &dyn Transport, account: &Account, achievement_id: u32, hash: &str, seconds_ago: u32) -> Award {
    let (u, t, h) = (cstr(&account.username), cstr(&account.token), cstr(hash));
    let Some(req) = build(|sink, ud| unsafe { ffi::pocket_rc_award_request(u.as_ptr(), t.as_ptr(), achievement_id, h.as_ptr(), seconds_ago, sink, ud) }) else {
        return Award::Refused("request".into());
    };
    let resp = transport.call(&req);
    if resp.failed() {
        return Award::Later;
    }
    let mut err = [0u8; 256];
    let rc = unsafe {
        ffi::pocket_rc_award_response(
            resp.body.as_ptr() as *const c_char,
            resp.body.len(),
            resp.status,
            err.as_mut_ptr() as *mut c_char,
            err.len(),
        )
    };
    let text = error_of(&err);
    match rc {
        ffi::RC_OK => Award::Done,
        // rcheevos treats "already unlocked" as a success with a message; anything else from the server is final,
        // except a refused token, which a new sign-in fixes.
        ffi::RC_INVALID_CREDENTIALS | ffi::RC_EXPIRED_TOKEN | ffi::RC_ACCESS_DENIED | ffi::RC_NO_RESPONSE => Award::Later,
        _ if text.to_lowercase().contains("already") => Award::Done,
        _ => Award::Refused(text),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A server in a box: answers by the request's `r=` and remembers what it was asked.
    pub struct Mock {
        pub answers: Vec<(&'static str, Response)>,
        pub asked: Mutex<Vec<Request>>,
    }

    impl Mock {
        pub fn new(answers: Vec<(&'static str, Response)>) -> Mock {
            Mock {
                answers,
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl Transport for Mock {
        fn call(&self, req: &Request) -> Response {
            self.asked.lock().unwrap().push(req.clone());
            let post = req.post_data.clone().unwrap_or_default();
            self.answers
                .iter()
                .find(|(r, _)| post.starts_with(&format!("r={r}&")) || post == format!("r={r}"))
                .map_or_else(Response::unreachable, |(_, resp)| resp.clone())
        }
    }

    #[test]
    fn login_sends_the_password_once_and_keeps_only_the_token() {
        let mock = Mock::new(vec![(
            "login2",
            Response::ok(
                r#"{"Success":true,"User":"Alex","Token":"ApiToken","Score":12345,"SoftcoreScore":123,"Messages":2,"Permissions":1,"AccountType":"Registered"}"#,
            ),
        )]);
        let a = login(&mock, " Alex ", "Pa$$word").unwrap();
        assert_eq!(
            a,
            Account {
                username: "Alex".into(),
                token: "ApiToken".into(),
                display_name: String::new()
            }
        );
        let asked = mock.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert!(asked[0].url.starts_with("https://retroachievements.org/dorequest.php"), "{}", asked[0].url);
        let post = asked[0].post_data.as_deref().unwrap();
        assert_eq!(post, "r=login2&u=Alex&p=Pa%24%24word");
        assert!(!asked[0].url.contains("Pa"), "the password is never in the address");
    }

    #[test]
    fn a_wrong_password_and_no_network_are_told_apart() {
        let wrong = Mock::new(vec![(
            "login2",
            Response {
                status: 401,
                body: br#"{"Success":false,"Error":"Invalid user/password combination. Please try again.","Code":"invalid_credentials"}"#.to_vec(),
            },
        )]);
        match login(&wrong, "Alex", "nope") {
            Err(LoginError::Credentials(m)) => assert!(m.contains("Invalid user/password"), "{m}"),
            other => panic!("{other:?}"),
        }
        let offline = Mock::new(vec![]);
        assert_eq!(login(&offline, "Alex", "pw"), Err(LoginError::Unreachable));
        assert!(LoginError::Unreachable.to_string().contains("online"));
        let odd = Mock::new(vec![("login2", Response::ok("<html>maintenance</html>"))]);
        assert!(matches!(login(&odd, "Alex", "pw"), Err(LoginError::Server(_))));
    }

    #[test]
    fn a_game_is_looked_up_by_its_hash_and_the_console_in_one_go() {
        let mock = Mock::new(vec![
            ("gameid", Response::ok(r#"{"Success":true,"GameID":1234}"#)),
            (
                "allprogress",
                Response::ok(
                    r#"{"Success":true,"Response":{"1234":{"Achievements":40,"Unlocked":12,"UnlockedHardcore":3},"99":{"Achievements":5,"Unlocked":0,"UnlockedHardcore":5}}}"#,
                ),
            ),
        ]);
        assert_eq!(resolve_hash(&mock, "0123456789abcdef0123456789abcdef"), Ok(1234));
        let account = Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: String::new(),
        };
        let mut list = all_progress(&mock, &account, 7).unwrap();
        list.sort_by_key(|p| p.game_id);
        assert_eq!(
            list,
            vec![
                Progress {
                    game_id: 99,
                    total: 5,
                    unlocked: 5
                },
                Progress {
                    game_id: 1234,
                    total: 40,
                    unlocked: 12
                }
            ]
        );
        let asked = mock.asked.lock().unwrap();
        assert_eq!(asked[1].post_data.as_deref(), Some("r=allprogress&u=Alex&t=Tk&c=7"));
        let unknown = Mock::new(vec![("gameid", Response::ok(r#"{"Success":true,"GameID":0}"#))]);
        assert_eq!(resolve_hash(&unknown, "ffff"), Ok(0));
        assert!(resolve_hash(&Mock::new(vec![]), "ffff").is_err());
    }

    #[test]
    fn a_late_unlock_is_sent_kept_or_dropped() {
        let account = Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: String::new(),
        };
        let ok = Mock::new(vec![(
            "awardachievement",
            Response::ok(r#"{"Success":true,"Score":5432,"SoftcoreScore":777,"AchievementID":5501,"AchievementsRemaining":6}"#),
        )]);
        assert_eq!(award(&ok, &account, 5501, "0123456789abcdef0123456789abcdef", 3600), Award::Done);
        let post = ok.asked.lock().unwrap()[0].post_data.clone().unwrap();
        assert!(
            post.starts_with("r=awardachievement&u=Alex&t=Tk&a=5501&h=0&m=0123456789abcdef0123456789abcdef&o=3600&v="),
            "{post}"
        );
        assert_eq!(award(&Mock::new(vec![]), &account, 5501, "ab", 1), Award::Later);
        let gone = Mock::new(vec![("awardachievement", Response::ok(r#"{"Success":false,"Error":"Achievement not found"}"#))]);
        assert!(matches!(award(&gone, &account, 5501, "ab", 1), Award::Refused(_)));
    }
}
