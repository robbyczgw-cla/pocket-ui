//! The sign-in helper end to end, against a stand-in sign-in page on 127.0.0.1 (`scripts/portal-standin.py`): the real
//! `net_check` request finds the redirect, the real `portal::Session` loads the page with the real `curl` (redirect,
//! session cookie), `portal_form` fills in the form the way the buttons do, the form is sent, and the check answers
//! online. Also: a wrong answer brings the page back with its message, a "please wait" interstitial forwards to online
//! by itself (a timer's literal address, then the hidden form the page submits on load), a JavaScript-only page and
//! endless redirects end with a plain answer, a form that posts to another site is never sent. The only difference
//! from the device: the stand-in runs on the Pocket's own address, which the helper refuses unless the test's rules
//! allow it.
//!
//! Needs python3 and curl; without them it says SKIPPED and passes. Nothing outside 127.0.0.1 is touched.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use pocket_ui::html_form::Kind;
use pocket_ui::net_check::{self, Net};
use pocket_ui::pad::Button;
use pocket_ui::portal::{Failure, Outcome, Rules, Session};
use pocket_ui::portal_form::{Ask, SignIn, State};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

struct Standin {
    child: Child,
    port: u16,
    log: PathBuf,
    /// The standin's folder (port file, log): removed after the child is stopped (see `Drop`).
    _dir: tempfile::TempDir,
}

impl Standin {
    fn start(mode: &str) -> Option<Standin> {
        let guard = tempfile::Builder::new().prefix(&format!("pocketui-portal-e2e-{mode}-")).tempdir().ok()?;
        let dir = guard.path().to_path_buf();
        let (port_file, log) = (dir.join("port"), dir.join("log"));
        let mut child = Command::new("python3")
            .arg("-I")
            .arg(Path::new(ROOT).join("scripts/portal-standin.py"))
            .args([mode, &format!("{ROOT}/testdata/portal"), &port_file.to_string_lossy(), &log.to_string_lossy()])
            .spawn()
            .ok()?;
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if let Some(port) = std::fs::read_to_string(&port_file).ok().and_then(|p| p.trim().parse().ok()) {
                return Some(Standin { child, port, log, _dir: guard });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        None
    }

    fn check(&self) -> String {
        format!("http://127.0.0.1:{}/check_network_status.txt", self.port)
    }

    fn session(&self) -> Session {
        Session::new("curl", &self.check(), Rules { allow_loopback: true, ..Rules::default() }).expect("a session")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for Standin {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn tools() -> bool {
    let ok = |p: &str| Command::new(p).arg("--version").output().is_ok_and(|o| o.status.success());
    if ok("python3") && ok("curl") {
        true
    } else {
        eprintln!("SKIPPED: portal_e2e needs python3 and curl");
        false
    }
}

/// The form view's state.
fn filling(s: &mut SignIn) -> &mut pocket_ui::portal_form::Filling {
    match &mut s.state {
        State::Filling(f) => f,
        _ => panic!("no form to fill in"),
    }
}

/// Moves to the row whose field is called `name` and types `text` with the keyboard (START keeps it).
fn type_into(s: &mut SignIn, name: &str, text: &str) {
    let f = filling(s);
    f.sel = f.rows.iter().position(|r| matches!(r, pocket_ui::portal_form::Row::Field(fi, k) if pocket_ui::portal_form::field_name(&f.page.forms[*fi].fields[*k]) == name)).expect(name);
    assert!(matches!(s.press(Button::East), Ask::Nothing));
    filling(s).typing.as_mut().expect("the keyboard").1.text = text.into();
    assert!(matches!(s.press(Button::Start), Ask::Nothing));
}

fn select(s: &mut SignIn, kind: Kind) {
    let f = filling(s);
    f.sel = f.rows.iter().position(|r| matches!(r, pocket_ui::portal_form::Row::Field(fi, k) if f.page.forms[*fi].fields[*k].kind == kind)).expect("the row");
}

fn load(s: &Session, start: &str) -> SignIn {
    let mut page = SignIn::new("127.0.0.1", 1);
    page.on_outcome(s.open(start), false);
    page
}

#[test]
fn a_hotel_sign_in_with_room_and_surname_goes_online() {
    if !tools() {
        return;
    }
    let portal = Standin::start("hotel").expect("the stand-in portal starts");
    // The real check finds the sign-in and where it is.
    let check = net_check::check_with("curl", &portal.check());
    assert_eq!(check.net, Net::Portal);
    let start = check.portal.expect("the redirect's address");
    assert!(start.ends_with("/portal/start?orig=check"), "{start}");

    let session = portal.session();
    let mut page = load(&session, &start);
    {
        let f = filling(&mut page);
        assert_eq!(f.page.name(), "Internet access for guests");
        assert!(f.page.text.contains("Terms of use"), "the terms are there to read");
        assert!(f.url.ends_with("/portal/login"), "after the cookie and a second redirect: {}", f.url);
    }
    // A wrong surname first: the page comes back with its message, nothing is online.
    type_into(&mut page, "Room number", "412");
    type_into(&mut page, "Surname", "Meier");
    select(&mut page, Kind::Checkbox);
    let _ = page.press(Button::East);
    select(&mut page, Kind::Submit { image: false });
    let Ask::Send { form, submitter, url } = page.press(Button::East) else { panic!("the button sends") };
    page.sending();
    page.on_outcome(session.submit(&url, &form, submitter), false);
    let f = filling(&mut page);
    assert!(f.page.text.starts_with("Room number and surname don't match a booking."), "{}", f.page.text);
    assert!(f.note.is_some(), "says the network still wants a sign-in");
    assert_eq!(net_check::check_with("curl", &portal.check()).net, Net::Portal);

    // The right answer: online, and the check says so too.
    type_into(&mut page, "Room number", "412");
    type_into(&mut page, "Surname", "Müller");
    select(&mut page, Kind::Checkbox);
    let _ = page.press(Button::East);
    select(&mut page, Kind::Submit { image: false });
    let Ask::Send { form, submitter, url } = page.press(Button::East) else { panic!("the button sends") };
    assert!(matches!(session.submit(&url, &form, submitter), Outcome::Online));
    assert_eq!(net_check::check_with("curl", &portal.check()).net, Net::Online);

    // What reached the stand-in: the two forms to the form's own address, in UTF-8, with its hidden field; nothing else
    // was posted anywhere.
    let log = portal.log();
    let posts: Vec<&str> = log.lines().filter(|l| l.starts_with("POST")).collect();
    assert_eq!(posts.len(), 2, "{log}");
    assert!(posts[1].starts_with("POST /portal/login.php csrf=b7e1%269&room=412&surname=M%C3%BCller&accept=1&action=login"), "{}", posts[1]);
}

#[test]
fn a_click_through_page_goes_online_with_one_button() {
    if !tools() {
        return;
    }
    let portal = Standin::start("clickthrough").expect("the stand-in portal starts");
    let start = net_check::check_with("curl", &portal.check()).portal.expect("redirected");
    let session = portal.session();
    let mut page = load(&session, &start);
    let f = filling(&mut page);
    assert_eq!(f.rows.len(), 1, "only the button: {:?}", f.rows);
    assert_eq!(f.page.name(), "Welcome to Café Kranz");
    let Ask::Send { form, submitter, url } = page.press(Button::East) else { panic!("the button sends") };
    assert!(matches!(session.submit(&url, &form, submitter), Outcome::Online));
    assert!(portal.log().contains("GET /opennds_auth/?tok=8f2c1a77&redir="), "a GET form's data in its address");
}

#[test]
fn a_please_wait_interstitial_forwards_to_online_by_itself() {
    if !tools() {
        return;
    }
    let portal = Standin::start("interstitial").expect("the stand-in portal starts");
    let check = net_check::check_with("curl", &portal.check());
    assert_eq!(check.net, Net::Portal);
    let start = check.portal.expect("the redirect's address");
    let session = portal.session();
    // "Internet Access - please wait." forwards with its timer's literal address, then the page's own hidden form
    // goes out on load (as a browser would send it), and the network lets the Pocket in: nothing filled in, pressed
    // or accepted, and no script ran.
    assert!(matches!(session.open(&start), Outcome::Online), "online after the interstitial's own steps");
    assert_eq!(net_check::check_with("curl", &portal.check()).net, Net::Online);
    let log = portal.log();
    assert!(log.contains("GET /portal/wait ") && log.contains("GET /portal/wait2 "), "{log}");
    let posts: Vec<&str> = log.lines().filter(|l| l.starts_with("POST")).collect();
    assert_eq!(posts.len(), 1, "the hidden form and nothing else: {log}");
    assert!(posts[0].starts_with("POST /portal/accept buttonClicked=4&err_flag=0"), "{}", posts[0]);
    assert!(posts[0].contains("mac=00%3A00%3A5E%3A00%3A53%3A7F"), "the hidden values as the browser sends them: {}", posts[0]);
}

#[test]
fn pages_the_helper_cant_do_end_plainly() {
    if !tools() {
        return;
    }
    let js = Standin::start("js").expect("the stand-in portal starts");
    let start = net_check::check_with("curl", &js.check()).portal.expect("redirected");
    match js.session().open(&start) {
        Outcome::NeedsBrowser { why, page } => {
            assert_eq!(why, pocket_ui::html_form::NoForm::NeedsScripts);
            assert_eq!(page.map(|p| p.title), Some("WIFI@DB".to_string()));
        }
        other => panic!("{other:?}"),
    }

    let lp = Standin::start("loop").expect("the stand-in portal starts");
    let start = net_check::check_with("curl", &lp.check()).portal.expect("redirected");
    let t = Instant::now();
    assert!(matches!(lp.session().open(&start), Outcome::Failed(Failure::Loop)));
    assert!(t.elapsed() < Duration::from_secs(10));

    // A form that posts to another site: shown, but never sent.
    let away = Standin::start("elsewhere").expect("the stand-in portal starts");
    let start = net_check::check_with("curl", &away.check()).portal.expect("redirected");
    let session = away.session();
    let mut page = load(&session, &start);
    type_into(&mut page, "Email", "me@example.org");
    select(&mut page, Kind::Submit { image: false });
    let Ask::Send { form, submitter, url } = page.press(Button::East) else { panic!("the button sends") };
    assert!(matches!(session.submit(&url, &form, submitter), Outcome::ElseWhere(h) if h == "198.51.100.7"));
    assert!(!away.log().contains("POST"), "nothing was sent");

    // The real rules refuse the Pocket's own address, before curl runs.
    let strict = Session::new("curl", &away.check(), Rules::default()).unwrap();
    assert!(matches!(strict.open(&start), Outcome::Failed(Failure::Refused(_))));
}
