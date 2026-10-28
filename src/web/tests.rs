//! The web companion's tests: the HTTP parser, the address plan and the connection checks, tailscaled's answers,
//! pairing, sessions and CSRF, every rule of the uploads, and the JSON API end to end over a real socket (with a
//! stand-in tailscaled and a stand-in App).

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::api::{self, Ctx, Tailnet};
use super::auth::{self, Pairing, Sessions, Tried};
use super::files::{self, Refused, Roots};
use super::http::{self, BadRequest};
use super::net::{self, Iface, Listen, SelfNode, Whois};
use super::{Edit, Prefs, Shared};
use crate::testutil::TestDir;

fn tmp(name: &str) -> crate::testutil::TestDir {
    TestDir::new(&format!("web-{name}"))
}

// --- HTTP ------------------------------------------------------------------------------------------------------

fn head(raw: &str) -> Result<http::Request, BadRequest> {
    http::read_head(&mut BufReader::new(raw.as_bytes()))
}

#[test]
fn a_request_head_is_read_with_its_query_cookies_and_host() {
    let r = head("GET /api/games?system=gba&q=mario%20kart+64 HTTP/1.1\r\nHost: Pocket:80\r\nCookie: a=1; pocket_session=tok\r\n\r\n").unwrap();
    assert_eq!(r.method, "GET");
    assert_eq!(r.path, "/api/games");
    assert_eq!(r.query("system"), Some("gba"));
    assert_eq!(r.query("q"), Some("mario kart 64"));
    assert_eq!(r.cookie("pocket_session"), Some("tok"));
    assert_eq!(r.host().as_deref(), Some("pocket"));
    let v6 = head("GET / HTTP/1.1\r\nHost: [fd7a:115c:a1e0::1]:80\r\n\r\n").unwrap();
    assert_eq!(v6.host().as_deref(), Some("fd7a:115c:a1e0::1"));
    let dot = head("GET / HTTP/1.1\r\nHost: pocket.tail1234.ts.net.\r\n\r\n").unwrap();
    assert_eq!(dot.host().as_deref(), Some("pocket.tail1234.ts.net"));
}

#[test]
fn bad_heads_are_refused_with_their_status() {
    assert_eq!(head("").unwrap_err(), BadRequest::Closed);
    assert_eq!(head("GET /\r\n\r\n").unwrap_err(), BadRequest::Malformed);
    assert_eq!(head("GET http://evil/ HTTP/1.1\r\n\r\n").unwrap_err(), BadRequest::Malformed, "only origin-form targets");
    assert_eq!(head("get / HTTP/1.1\r\n\r\n").unwrap_err(), BadRequest::Malformed);
    assert_eq!(head("GET / HTTP/1.1\r\nBad Header: x\r\n\r\n").unwrap_err(), BadRequest::Malformed);
    assert_eq!(head("POST /api/upload HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap_err(), BadRequest::LengthRequired);
    assert_eq!(head("POST / HTTP/1.1\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\n").unwrap_err(), BadRequest::Malformed, "no request smuggling");
    assert_eq!(head("GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n").unwrap_err(), BadRequest::Malformed);
    assert_eq!(head("GET / HTTP/1.1\r\nHost: x").unwrap_err(), BadRequest::Malformed, "cut off");
    let big = format!("GET / HTTP/1.1\r\nX-Pad: {}\r\n\r\n", "a".repeat(http::MAX_HEAD));
    assert_eq!(head(&big).unwrap_err(), BadRequest::HeadTooLarge);
    let many: String = (0..80).map(|i| format!("X-{i}: 1\r\n")).collect();
    assert_eq!(head(&format!("GET / HTTP/1.1\r\n{many}\r\n")).unwrap_err(), BadRequest::HeadTooLarge);
    assert_eq!(BadRequest::LengthRequired.status(), 411);
    assert_eq!(BadRequest::HeadTooLarge.status(), 431);
}

#[test]
fn percent_decoding_refuses_broken_escapes() {
    assert_eq!(http::percent_decode("a%2Fb").as_deref(), Some("a/b"));
    assert_eq!(http::percent_decode("%C3%A9").as_deref(), Some("é"));
    assert_eq!(http::percent_decode("%zz"), None);
    assert_eq!(http::percent_decode("%4"), None);
    assert_eq!(http::percent_decode("%ff"), None, "not UTF-8");
}

#[test]
fn bodies_have_a_limit() {
    let r = head("POST / HTTP/1.1\r\nContent-Length: 70000\r\n\r\n").unwrap();
    assert_eq!(http::read_body(&mut &b""[..], &r, http::MAX_JSON).unwrap_err(), 413);
    let r = head("POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\n").unwrap();
    assert_eq!(http::read_body(&mut &b"short"[..], &r, http::MAX_JSON).unwrap_err(), 400);
}

#[test]
fn every_response_carries_the_security_headers() {
    let mut out = Vec::new();
    http::write_response(&mut out, &http::Response::error(404, "x").with("X-Evil", "a\r\nSet-Cookie: b"), false).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
    for h in ["Content-Security-Policy: default-src 'none'; script-src 'self'", "X-Content-Type-Options: nosniff", "X-Frame-Options: DENY", "Referrer-Policy: no-referrer", "Cache-Control: no-store", "Connection: close"] {
        assert!(text.contains(h), "{h}");
    }
    assert!(!text.contains("Set-Cookie"), "a value with a line break is dropped, not sent");
    assert!(!http::SECURITY_HEADERS[0].1.contains("unsafe"), "no inline script or eval");
}

// --- where it listens -------------------------------------------------------------------------------------------

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn iface(name: &str, addr: &str) -> Iface {
    Iface { name: name.into(), addr: ip(addr), index: 7 }
}

#[test]
fn the_tailnet_ranges() {
    for yes in ["100.64.0.1", "100.101.102.103", "100.127.255.254", "fd7a:115c:a1e0::1", "fd7a:115c:a1e0:ab12::99", "::ffff:100.77.1.2"] {
        assert!(net::is_tailnet(ip(yes)), "{yes}");
    }
    for no in ["100.63.255.255", "100.128.0.1", "192.0.2.57", "192.0.2.164", "fd7a:115c:a1e1::1", "fe80::1", "::1", "127.0.0.1", "0.0.0.0"] {
        assert!(!net::is_tailnet(ip(no)), "{no}");
    }
}

#[test]
fn it_listens_only_on_the_tailscale_interface() {
    let ifaces = vec![
        iface("lo", "127.0.0.1"),
        iface("lo", "::1"),
        iface("wlan0", "192.0.2.164"),
        // A phone hotspot handing out a carrier-grade NAT address: the same range as Tailscale, not Tailscale.
        iface("wlan0", "100.88.1.2"),
        iface("wlan0", "fe80::1234"),
        iface("tailscale0", "100.70.1.10"),
        iface("tailscale0", "fd7a:115c:a1e0::4f01:2a3b"),
        iface("tailscale0", "fe80::abcd"),
        // Not Tailscale's range on its interface (should never happen): not taken either.
        iface("tailscale0", "192.168.7.7"),
        iface("eth0", "0.0.0.0"),
    ];
    let plan = net::plan(&ifaces, 80);
    let addrs: Vec<String> = plan.iter().map(|l| l.addr.to_string()).collect();
    assert_eq!(addrs, ["100.70.1.10:80", "[fd7a:115c:a1e0::4f01:2a3b]:80"]);
    assert!(plan.iter().all(|l| l.iface == "tailscale0"));
    assert!(net::plan(&[iface("wlan0", "192.0.2.164"), iface("eth0", "192.0.2.5")], 80).is_empty(), "Tailscale off: nothing at all");
    assert!(net::plan(&[iface("tailscale0", "0.0.0.0"), iface("tailscale0", "::")], 80).is_empty(), "never every address");
}

#[test]
fn connections_must_come_from_the_tailnet() {
    assert!(net::peer_allowed(ip("100.100.5.6")));
    assert!(net::peer_allowed(ip("fd7a:115c:a1e0::9")));
    assert!(!net::peer_allowed(ip("192.0.2.120")), "a LAN device on the tailnet socket (weak host model)");
    assert!(!net::peer_allowed(ip("127.0.0.1")));
    assert!(!net::peer_allowed(ip("8.8.8.8")));
}

#[test]
fn only_the_pockets_own_names_are_served() {
    let names = vec!["pocket".to_string(), "pocket.tail1234.ts.net".to_string()];
    let listening: Vec<SocketAddr> = vec!["100.70.1.10:80".parse().unwrap(), "[fd7a:115c:a1e0::1]:80".parse().unwrap()];
    assert!(net::host_allowed("pocket", &names, &listening));
    assert!(net::host_allowed("POCKET.tail1234.ts.net.", &names, &listening));
    assert!(net::host_allowed("100.70.1.10", &names, &listening));
    assert!(net::host_allowed("fd7a:115c:a1e0::1", &names, &listening));
    assert!(!net::host_allowed("evil.example", &names, &listening), "DNS rebinding");
    assert!(!net::host_allowed("192.0.2.164", &names, &listening), "an address it does not listen on");
    assert!(!net::host_allowed("", &[], &listening));
}

#[test]
fn tailscaled_status_and_whois_are_read() {
    let me = SelfNode::parse(r#"{"Self":{"UserID":4242,"HostName":"pocket","DNSName":"pocket.tail1234.ts.net.","Tags":null},"User":{"4242":{"LoginName":"alex@example.com"}}}"#).unwrap();
    assert_eq!(me.user_id, 4242);
    assert_eq!(me.login, "alex@example.com");
    assert!(!me.tagged);
    assert_eq!(me.names(), ["pocket", "pocket.tail1234.ts.net"]);
    let peer = Whois::parse(r#"{"Node":{"ID":7,"StableID":"nAbC","Name":"alexs-iphone.tail1234.ts.net.","User":4242},"UserProfile":{"ID":4242,"LoginName":"alex@example.com"}}"#).unwrap();
    assert_eq!((peer.user_id, peer.device.as_str(), peer.node.as_str()), (4242, "alexs-iphone", "nAbC"));
    assert!(net::owner_allows(&me, &peer));
    let other = Whois { user_id: 99, ..peer.clone() };
    assert!(!net::owner_allows(&me, &other), "a device of another user of the tailnet");
    let tagged = Whois { tagged: true, ..peer.clone() };
    assert!(!net::owner_allows(&me, &tagged), "a tagged server");
    let tagged_me = SelfNode { tagged: true, ..me.clone() };
    assert!(!net::owner_allows(&tagged_me, &peer), "a tagged Pocket has no owner");
    let nobody = SelfNode { user_id: 0, ..me.clone() };
    assert!(!net::owner_allows(&nobody, &Whois { user_id: 0, ..peer }));
    assert!(Whois::parse("not json").is_none());
}

// --- pairing, sessions, CSRF -----------------------------------------------------------------------------------

#[test]
fn a_pairing_code_has_six_digits_and_five_tries() {
    for _ in 0..50 {
        let p = Pairing::new().unwrap();
        assert!(p.code.len() == 6 && p.code.bytes().all(|b| b.is_ascii_digit()));
    }
    let now = Instant::now();
    let mut p = Some(Pairing { code: "123456".into(), until: now + Duration::from_secs(60), tries_left: auth::PAIRING_TRIES });
    for _ in 0..4 {
        assert_eq!(auth::try_code(&mut p, "000000", now), Tried::Wrong);
    }
    assert_eq!(auth::try_code(&mut p, "123 456", now), Tried::Paired, "spaces as on the screen are fine");
    assert!(p.is_none(), "a code pairs one browser");
    assert_eq!(auth::try_code(&mut p, "123456", now), Tried::NoCode);

    let mut p = Some(Pairing { code: "123456".into(), until: now + Duration::from_secs(60), tries_left: auth::PAIRING_TRIES });
    for _ in 0..5 {
        assert_eq!(auth::try_code(&mut p, "999999", now), Tried::Wrong);
    }
    assert!(p.is_none(), "five wrong tries end it");
    let mut p = Some(Pairing { code: "123456".into(), until: now, tries_left: 5 });
    assert_eq!(auth::try_code(&mut p, "123456", now + Duration::from_secs(1)), Tried::NoCode, "after five minutes");
}

#[test]
fn sessions_are_kept_hashed_and_can_be_revoked() {
    let dir = tmp("sessions");
    let mut s = Sessions::load(&dir);
    let token = s.pair("Safari on iPhone").unwrap();
    let file = std::fs::read_to_string(Sessions::dir(&dir).join("browsers.json")).unwrap();
    assert!(!file.contains(&token), "only the hash is on the SD card");
    assert!(file.contains(&auth::token_hash(&token)));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(Sessions::dir(&dir)).unwrap().permissions().mode() & 0o777, 0o700);
    }
    let b = s.check(&token).unwrap();
    assert_eq!(b.name, "Safari on iPhone");
    assert!(s.check("x").is_none() && s.check(&"A".repeat(43)).is_none());

    let mut again = Sessions::load(&dir);
    assert_eq!(again.check(&token).map(|b| b.id.clone()), Some(b.id.clone()), "survives a restart");
    let csrf = again.csrf(&format!("b:{}", b.id)).unwrap();
    assert_eq!(csrf, s.csrf(&format!("b:{}", b.id)).unwrap(), "the same key after a restart");
    assert!(again.csrf_ok(&format!("b:{}", b.id), Some(&csrf)));
    assert!(!again.csrf_ok("b:other", Some(&csrf)), "one principal's token is not another's");
    assert!(!again.csrf_ok(&format!("b:{}", b.id), None));
    assert!(again.revoke(&b.id));
    assert!(again.check(&token).is_none());
    assert!(Sessions::load(&dir).check(&token).is_none(), "revoked for good");
}

#[test]
fn hmac_and_constant_time_comparison() {
    // RFC 4231, test case 2.
    let mac = auth::hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(super::hex(&mac), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    assert!(auth::ct_eq(b"abc", b"abc"));
    assert!(!auth::ct_eq(b"abc", b"abd"));
    assert!(!auth::ct_eq(b"abc", b"abcd"));
}

#[test]
fn browsers_get_a_name_from_their_user_agent() {
    let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
    assert_eq!(auth::browser_name(safari), "Safari on iPhone");
    let chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
    assert_eq!(auth::browser_name(chrome), "Chrome on Mac");
    assert_eq!(auth::browser_name("Mozilla/5.0 (Windows NT 10.0; rv:131.0) Gecko/20100101 Firefox/131.0"), "Firefox on Windows");
    assert_eq!(auth::browser_name(""), "A browser");
}

// --- uploads ---------------------------------------------------------------------------------------------------

#[test]
fn file_names_are_cleaned() {
    let s = |n: &str| files::sanitize_name(n);
    assert_eq!(s("Tetris (World).gb").as_deref(), Some("Tetris (World).gb"));
    assert_eq!(s("../../etc/passwd").as_deref(), Some("passwd"));
    assert_eq!(s("..\\..\\Windows\\evil.gba").as_deref(), Some("evil.gba"));
    assert_eq!(s(".bashrc").as_deref(), Some("bashrc"), "no hidden files");
    assert_eq!(s("a\u{0}b\nc.gba").as_deref(), Some("a_b_c.gba"));
    assert_eq!(s("what?<>|*.gba").as_deref(), Some("what_____.gba"));
    assert_eq!(s("name. . ").as_deref(), Some("name"));
    // Odd Unicode: a writing-direction override that would show "evil\u{202e}abg.sh" as "evilhs.gba", zero-width
    // characters, slashes that only look like slashes.
    assert_eq!(s("evil\u{202e}abg.sh").as_deref(), Some("evil_abg.sh"));
    assert_eq!(s("a\u{200b}b\u{feff}.gba").as_deref(), Some("a_b_.gba"));
    assert_eq!(s("..\u{2215}..\u{2215}x.gba").as_deref(), Some("_.._x.gba"));
    assert_eq!(s("Pok\u{e9}mon \u{2013} \u{65e5}\u{672c}.gba").as_deref(), Some("Pok\u{e9}mon \u{2013} \u{65e5}\u{672c}.gba"), "letters of any language stay");
    for none in ["", ".", "..", "...", "/", "dir/", " . "] {
        assert_eq!(s(none), None, "{none:?}");
    }
    let long = format!("{}.gba", "é".repeat(300));
    let cut = s(&long).unwrap();
    assert!(cut.len() <= 200 && cut.ends_with(".gba"), "{} bytes", cut.len());
    assert_eq!(files::sanitize_rel("Game/data/pak0.pak"), Some(vec!["Game".into(), "data".into(), "pak0.pak".into()]));
    for bad in ["../x", "a/../b", "/etc/x", "a//b", "a/./b", "\\x", ""] {
        assert_eq!(files::sanitize_rel(bad), None, "{bad:?}");
    }
    assert_eq!(files::sanitize_rel("a/".repeat(13).trim_end_matches('/')), None, "too deep");
}

#[test]
fn a_rom_needs_its_systems_file_type() {
    let exts: Vec<String> = ["gba", "zip", "p8.png", "png"].map(String::from).to_vec();
    assert_eq!(files::matching_ext("Game.GBA", &exts), Some("gba"));
    assert_eq!(files::matching_ext("cart.p8.png", &exts), Some("p8.png"), "the longest ending");
    assert_eq!(files::matching_ext("game.sh", &exts), None);
    assert_eq!(files::matching_ext(".gba", &exts), None);
}

fn sys(id: &str, path: &Path, exts: &[&str]) -> crate::library::SysInfo {
    crate::library::SysInfo { id: id.into(), name: id.to_uppercase(), folder: format!("roms/{id}"), path: path.to_path_buf(), extensions: exts.iter().map(|e| e.to_string()).collect() }
}

#[test]
fn roms_go_only_into_game_folders_inside_the_roms_folder() {
    let root = tmp("romtarget");
    let roms = root.join("roms");
    std::fs::create_dir_all(roms.join("gba")).unwrap();
    let roots = Roots::under(&roms);
    let gba = sys("gba", &roms.join("gba"), &["gba", "zip"]);
    assert_eq!(files::rom_target(&roots, &gba, "Game.gba").unwrap(), roms.join("gba/Game.gba"));
    assert_eq!(files::rom_target(&roots, &gba, "../../x/Evil.gba").unwrap(), roms.join("gba/Evil.gba"), "only the last part");
    assert!(matches!(files::rom_target(&roots, &gba, "evil.sh"), Err(Refused::NotAllowed(_))));
    let ports = sys("ports", &roms.join("ports"), &["sh"]);
    assert!(matches!(files::rom_target(&roots, &ports, "x.sh"), Err(Refused::NotAllowed(_))), "the Pocket runs programs from ports");
    let scripts = sys("tools2", &roms.join("tools2"), &["sh", "zip"]);
    assert!(files::rom_target(&roots, &scripts, "x.zip").is_err(), "a system that runs scripts");
    let bios = sys("bios", &roms.join("bios"), &["bin"]);
    assert!(files::rom_target(&roots, &bios, "x.bin").is_err());
    let outside = sys("etc", &root.join("etc"), &["conf"]);
    assert!(files::rom_target(&roots, &outside, "x.conf").is_err(), "a system folder outside roms");
    let dotdot = sys("gb", &roms.join("gb/../../etc"), &["gb"]);
    assert!(files::rom_target(&roots, &dotdot, "x.gb").is_err());
    #[cfg(unix)]
    {
        std::fs::create_dir_all(root.join("secret")).unwrap();
        std::os::unix::fs::symlink(root.join("secret"), roms.join("nes")).unwrap();
        let linked = sys("nes", &roms.join("nes"), &["nes"]);
        assert!(files::rom_target(&roots, &linked, "x.nes").is_err(), "a folder that is a link to elsewhere");
    }
    // A system folder that does not exist yet is made, one level under roms.
    let gbc = sys("gbc", &roms.join("gbc"), &["gbc"]);
    assert_eq!(files::rom_target(&roots, &gbc, "a.gbc").unwrap(), roms.join("gbc/a.gbc"));
    assert!(roms.join("gbc").is_dir());
    assert!(files::system_folder_ok(&roots, &gba) && !files::system_folder_ok(&roots, &ports) && !files::system_folder_ok(&roots, &outside));
}

#[test]
fn bios_files_go_into_the_bios_folder_and_its_known_subfolders() {
    let root = tmp("biostarget");
    let roots = Roots::under(&root.join("roms"));
    assert!(files::bios_target(&roots, "disksys.rom", None).is_err(), "no ROMs folder (the SD card is out): nothing is made");
    std::fs::create_dir_all(&roots.roms).unwrap();
    assert_eq!(files::bios_target(&roots, "disksys.rom", None).unwrap(), roots.bios.join("disksys.rom"));
    assert_eq!(files::bios_target(&roots, "../disksys.rom", None).unwrap(), roots.bios.join("disksys.rom"));
    assert_eq!(files::bios_target(&roots, "naomi.zip", Some("dc")).unwrap(), roots.bios.join("dc/naomi.zip"));
    assert!(files::bios_target(&roots, "other.zip", Some("dc")).is_err(), "only what hostsys looks for there");
    assert!(files::bios_target(&roots, "x.bin", Some("..")).is_err());
    assert!(files::bios_target(&roots, "naomi.zip", Some("../dc")).is_err());
    assert!(files::bios_target(&roots, "run.sh", None).is_err());
    assert_eq!(files::bios_target(&roots, "naomi.zip", Some("DC")).unwrap(), roots.bios.join("dc/naomi.zip"), "the folder as hostsys spells it");
    // A BIOS folder that is a link out of the ROMs folder takes nothing.
    #[cfg(unix)]
    {
        let root = tmp("bioslink");
        std::fs::create_dir_all(root.join("roms")).unwrap();
        std::fs::create_dir_all(root.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), root.join("roms/bios")).unwrap();
        let roots = Roots::under(&root.join("roms"));
        assert!(matches!(files::bios_target(&roots, "disksys.rom", None), Err(Refused::NotAllowed(_))));
    }
}

#[test]
fn the_bios_check_names_what_is_missing() {
    let dir = tmp("biosreport");
    let report = files::bios_report(&dir);
    let fds = report.iter().find(|n| n.label == "Famicom Disk System").unwrap();
    assert_eq!(fds.missing_line().as_deref(), Some("Missing for Famicom Disk System: disksys.rom"));
    let cd = report.iter().find(|n| n.label == "Sega CD").unwrap();
    assert_eq!(cd.missing_line().as_deref(), Some("Missing for Sega CD: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin"));
    assert_eq!(report.iter().filter(|n| n.label == "Sega CD").count(), 1, "once per console");
    std::fs::write(dir.join("disksys.rom"), b"x").unwrap();
    let report = files::bios_report(&dir);
    let fds = report.iter().find(|n| n.label == "Famicom Disk System").unwrap();
    assert_eq!(fds.found.as_deref(), Some("disksys.rom"));
    assert_eq!(fds.missing_line(), None);
    assert_eq!(files::bios_for("DISKSYS.ROM"), Some("Famicom Disk System"));
    assert_eq!(files::bios_for("dc/naomi.zip"), Some("NAOMI"));
    assert_eq!(files::bios_for("random.bin"), None);
}

/// An installed port that waits for its files in `assets`.
fn port(roms: &Path, rtr: bool) {
    let dir = roms.join("ports/sprout");
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("port.json"),
        format!(r#"{{"items":["SPROUT.sh","sprout"],"attr":{{"title":"Sprout","rtr":{rtr},"inst":"Purchase the game and place the game files in sprout/assets"}}}}"#),
    )
    .unwrap();
}

#[test]
fn port_files_go_into_its_drop_folder_only() {
    let root = tmp("porttarget");
    let roms = root.join("roms");
    port(&roms, false);
    let roots = Roots::under(&roms);
    let needs = files::ports_needing_files(&roots);
    assert_eq!(needs.len(), 1);
    assert!(needs[0].drop && needs[0].folders == ["assets"]);
    assert_eq!(files::port_target(&roots, "sprout", "data.win").unwrap(), roms.join("port-files/sprout/data.win"));
    assert_eq!(files::port_target(&roots, "sprout", "music/a.ogg").unwrap(), roms.join("port-files/sprout/music/a.ogg"));
    assert!(roms.join("port-files/sprout/music").is_dir());
    for bad in ["../x", "a/../../b", "/etc/passwd"] {
        assert!(files::port_target(&roots, "sprout", bad).is_err(), "{bad}");
    }
    assert!(files::port_target(&roots, "../ports", "x").is_err());
    assert!(files::port_target(&roots, "sprout", "run.SH").is_err(), "no scripts");
    assert!(files::port_target(&roots, "sprout", "tools/patch.py").is_err());
    assert!(files::port_target(&roots, "other", "x").is_err(), "not installed");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("elsewhere"), roms.join("port-files/sprout/linked")).unwrap();
        assert!(files::port_target(&roots, "sprout", "linked/x.dat").is_err(), "never through a link");
    }
    port(&roms, true);
    assert!(files::ports_needing_files(&roots).is_empty(), "a port that is ready to run takes no uploads");
    assert!(files::port_target(&roots, "sprout", "data.win").is_err());
}

#[test]
fn uploads_are_atomic_and_never_replace_without_asking() {
    let dir = tmp("receive");
    let target = dir.join("Game.gba");
    let no_wait = &files::Gate::OPEN;
    let w = files::receive(&mut &b"first"[..], &target, 5, false, no_wait).unwrap();
    assert_eq!(w, files::Written { bytes: 5, replaced: false });
    assert_eq!(std::fs::read(&target).unwrap(), b"first");
    assert!(matches!(files::receive(&mut &b"second"[..], &target, 6, false, no_wait), Err(Refused::Exists(_))));
    assert_eq!(std::fs::read(&target).unwrap(), b"first", "kept");
    let w = files::receive(&mut &b"second"[..], &target, 6, true, no_wait).unwrap();
    assert!(w.replaced);
    assert_eq!(std::fs::read(&target).unwrap(), b"second");
    // Cut off: nothing new, no temporary file left behind.
    assert_eq!(files::receive(&mut &b"abc"[..], &dir.join("Cut.gba"), 10, false, no_wait).unwrap_err(), Refused::Short);
    assert!(!dir.join("Cut.gba").exists());
    let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, ["Game.gba"], "only the finished file");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("Game.gba"), dir.join("Link.gba")).unwrap();
        assert!(files::receive(&mut &b"x"[..], &dir.join("Link.gba"), 1, false, no_wait).is_err());
        assert!(files::receive(&mut &b"x"[..], &dir.join("Link.gba"), 1, true, no_wait).is_err(), "never written through a link");
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
    }
}

#[test]
fn leftovers_of_an_earlier_run_are_swept() {
    let dir = tmp("sweep");
    let ours = format!(".Game.gba.{}-3.upload", std::process::id());
    for n in [".Game.gba.1-0.upload", ".Other (USA).gba.4242-17.upload", ours.as_str(), "Game.gba", "notes.upload", ".hidden"] {
        std::fs::write(dir.join(n), b"x").unwrap();
    }
    files::sweep_leftovers(&dir.join("New.gba"));
    let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    let mut want = vec![ours.clone(), ".hidden".into(), "Game.gba".into(), "notes.upload".into()];
    want.sort();
    assert_eq!(left, want, "only another run's temporary files go; one under way stays");
}

#[test]
fn an_upload_waits_while_a_game_runs() {
    let dir = tmp("wait");
    let started = Instant::now();
    let wait = move || started.elapsed() < Duration::from_millis(700);
    let gate = files::Gate { wait: &wait, ..files::Gate::OPEN };
    files::receive(&mut &b"data"[..], &dir.join("a.gba"), 4, false, &gate).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(700), "it did not write before the game ended");
}

#[test]
fn sizes_are_limited() {
    let dir = tmp("fits");
    assert!(matches!(files::fits(&dir.join("x"), files::MAX_BIOS + 1, files::MAX_BIOS), Err(Refused::TooBig(_))));
    assert!(matches!(files::fits(&dir.join("x"), u64::MAX / 2, u64::MAX), Err(Refused::NoSpace(_))));
    assert!(files::fits(&dir.join("x"), 10, files::MAX_BIOS).is_ok());
}

// --- the API end to end ----------------------------------------------------------------------------------------

/// tailscaled as the tests want it: this node of user 1, and the peers named here.
struct FakeTailnet {
    peers: Arc<Mutex<HashMap<IpAddr, Whois>>>,
}

impl Tailnet for FakeTailnet {
    fn self_node(&self) -> Option<SelfNode> {
        Some(SelfNode { user_id: 1, login: "owner@example.com".into(), tagged: false, hostname: "pocket".into(), dns_name: "pocket.tail1234.ts.net".into() })
    }
    fn whois(&self, peer: SocketAddr) -> Option<Whois> {
        self.peers.lock().unwrap().get(&peer.ip()).cloned()
    }
}

struct Rig {
    root: crate::testutil::TestDir,
    shared: Arc<Shared>,
    addr: SocketAddr,
    edits: Arc<Mutex<Vec<Edit>>>,
    /// tailscaled's answers, changeable while the server runs.
    peers: Arc<Mutex<HashMap<IpAddr, Whois>>>,
    /// How often the screen was captured.
    shots: Arc<std::sync::atomic::AtomicUsize>,
    server: Option<super::server::Server>,
}

/// A server on 127.0.0.1 with one GBA game, a port waiting for files, and an App that answers every edit with Ok.
fn rig(name: &str, owner: bool) -> Rig {
    rig_with(name, owner, None)
}

/// A rig whose Stats page reads this play history.
fn rig_with(name: &str, owner: bool, history: Option<super::HistorySource>) -> Rig {
    rig_ctx(name, owner, |ctx| ctx.history = history)
}

/// A rig whose server's `Ctx` is changed by `tune` first (deadlines, history).
fn rig_ctx(name: &str, owner: bool, tune: impl FnOnce(&mut Ctx)) -> Rig {
    let root = tmp(name);
    let roms = root.join("roms");
    std::fs::create_dir_all(roms.join("gba")).unwrap();
    std::fs::write(roms.join("gba/Advance Wars (USA).gba"), b"rom").unwrap();
    port(&roms, false);
    let cfg = root.join("es_systems.cfg");
    std::fs::write(
        &cfg,
        format!(
            "<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba .zip</extension></system>\
             <system><name>ports</name><fullname>Ports</fullname><path>{}</path><extension>.sh</extension></system></systemList>",
            roms.join("gba").display(),
            roms.join("ports").display()
        ),
    )
    .unwrap();
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let shared = Shared::new(data, cfg, Prefs { enabled: true, ..Prefs::default() });
    let edits: Arc<Mutex<Vec<Edit>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = edits.clone();
    *shared.edits.lock().unwrap() = Some(Box::new(move |e, tx| {
        seen.lock().unwrap().push(e);
        tx.send(Ok(())).is_ok()
    }));
    let mut peers = HashMap::new();
    if owner {
        peers.insert(ip("127.0.0.1"), Whois { user_id: 1, login: "owner@example.com".into(), device: "laptop".into(), node: "nOwner".into(), tagged: false });
    }
    let tailnet_peers = Arc::new(Mutex::new(peers));
    let mut ctx = Ctx::new(shared.clone(), Roots::under(&roms), Box::new(FakeTailnet { peers: tailnet_peers.clone() }));
    let shots = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = shots.clone();
    ctx.screenshot = Box::new(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(crate::image::Image { w: 4, h: 4, px: vec![0x00ff_0000; 16] })
    });
    tune(&mut ctx);
    let listen = Listen { addr: "127.0.0.1:0".parse().unwrap(), iface: "lo".into(), index: 0 };
    let server = super::server::Server::start(ctx, Some(vec![listen])).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let addr = loop {
        if let Some(a) = shared.listening.lock().unwrap().first().copied() {
            break a;
        }
        assert!(Instant::now() < deadline, "the server did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    Rig { root, shared, addr, edits, peers: tailnet_peers, shots, server: Some(server) }
}

struct Answer {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| l.split_once(':').filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
    }
}

impl Rig {
    fn raw(&self, req: &[u8]) -> Answer {
        let mut s = TcpStream::connect(self.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s.write_all(req).unwrap();
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        let at = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(out.len());
        let head = String::from_utf8_lossy(&out[..at]).into_owned();
        let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        Answer { status, head, body: out.get(at + 4..).unwrap_or_default().to_vec() }
    }

    /// A request with the Host the browser would send, extra headers and a body.
    fn req(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Answer {
        let mut r = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", self.addr);
        for (n, v) in headers {
            r.push_str(&format!("{n}: {v}\r\n"));
        }
        if method == "POST" {
            r.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        r.push_str("\r\n");
        let mut bytes = r.into_bytes();
        bytes.extend_from_slice(body);
        self.raw(&bytes)
    }

    /// Pairs with a code made as the Settings page makes it; returns the cookie and the CSRF token.
    fn pair(&self) -> (String, String) {
        *self.shared.pairing.lock().unwrap() = Some(Pairing { code: "246810".into(), until: Instant::now() + Duration::from_secs(60), tries_left: 5 });
        let a = self.req("POST", "/api/pair", &[("Content-Type", "application/json"), ("Sec-Fetch-Site", "same-origin")], br#"{"code":"246810"}"#);
        assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
        let set = a.header("set-cookie").unwrap();
        assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Path=/"), "{set}");
        let cookie = set.split(';').next().unwrap().to_string();
        let s = self.req("GET", "/api/session", &[("Cookie", &cookie)], b"").json();
        assert_eq!(s["signedIn"], true);
        (cookie, s["csrf"].as_str().unwrap().to_string())
    }
}

#[test]
fn the_page_is_served_with_its_policy_and_nothing_else_without_pairing() {
    let r = rig("api-open", false);
    let page = r.req("GET", "/", &[], b"");
    assert_eq!(page.status, 200);
    assert!(page.header("content-security-policy").unwrap().contains("default-src 'none'"));
    let html = String::from_utf8_lossy(&page.body);
    assert!(html.contains("/app.js?v=") && !html.contains("{{v}}"));
    assert!(!html.contains("http://") && !html.contains("https://"), "nothing from another site");
    assert!(!html.contains("<script>"), "no inline script");
    assert_eq!(r.req("GET", "/app.js", &[], b"").status, 200);
    assert_eq!(r.req("GET", "/fonts/body.ttf", &[], b"").header("cache-control").as_deref(), Some("public, max-age=31536000, immutable"));
    for path in ["/api/library", "/api/games?system=gba", "/api/stats", "/api/uploads", "/api/screenshot", "/api/battery", "/api/status"] {
        assert_eq!(r.req("GET", path, &[], b"").status, 401, "{path}");
    }
    assert_eq!(r.req("GET", "/api/session", &[], b"").json()["signedIn"], false);
    assert_eq!(r.req("GET", "/../../etc/passwd", &[], b"").status, 401, "no files outside the page; unknown paths need a login too");
    assert_eq!(r.req("GET", "/app.js/../../etc/passwd", &[], b"").status, 401);
    assert_eq!(r.req("DELETE", "/api/library", &[], b"").status, 405);
}

#[test]
fn a_foreign_host_name_is_refused_dns_rebinding() {
    let r = rig("api-host", false);
    let a = r.raw(b"GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n");
    assert_eq!(a.status, 421);
    let a = r.raw(b"GET /api/session HTTP/1.1\r\n\r\n");
    assert_eq!(a.status, 421, "no Host at all");
}

#[test]
fn pairing_gives_a_cookie_and_wrong_codes_are_refused() {
    let r = rig("api-pair", false);
    let post = |code: &str| r.req("POST", "/api/pair", &[("Content-Type", "application/json")], format!(r#"{{"code":"{code}"}}"#).as_bytes());
    let t = Instant::now();
    let none = post("123456");
    assert_eq!(none.status, 403, "no code on screen");
    *r.shared.pairing.lock().unwrap() = Some(Pairing { code: "135790".into(), until: Instant::now() + Duration::from_secs(60), tries_left: 5 });
    let wrong = post("000000");
    assert_eq!(wrong.status, 403);
    assert!(t.elapsed() >= Duration::from_millis(900), "a second after an answer without a code too (review F5)");
    assert_eq!(wrong.body, none.body, "the same answer whether a code is showing or not: no oracle");
    assert!(wrong.json().get("triesLeft").is_none(), "no count of tries");
    assert_eq!(wrong.json()["error"], api::NOT_PAIRED);
    assert_eq!(post("000001").status, 403);
    assert!(t.elapsed() >= Duration::from_millis(1900), "a second between tries");
    assert_eq!(r.shared.pairing.lock().unwrap().as_ref().map(|p| p.tries_left), Some(3));
    let other_site = r.req("POST", "/api/pair", &[("Origin", "http://evil.example")], br#"{"code":"135790"}"#);
    assert_eq!(other_site.status, 403, "not from another site");
    let (cookie, csrf) = r.pair();
    assert_eq!(csrf.len(), 32);
    assert_eq!(r.req("GET", "/api/library", &[("Cookie", &cookie)], b"").status, 200);
    assert_eq!(r.req("GET", "/api/library", &[("Cookie", "pocket_session=forged-but-long-enough-to-look-like-a-token")], b"").status, 401);
    // Revoked on the Pocket: the cookie is worth nothing.
    let id = r.shared.sessions.lock().unwrap().browsers[0].id.clone();
    r.shared.sessions.lock().unwrap().revoke(&id);
    assert_eq!(r.req("GET", "/api/library", &[("Cookie", &cookie)], b"").status, 401);
}

#[test]
fn every_write_needs_the_csrf_token_and_this_origin() {
    let r = rig("api-csrf", false);
    let (cookie, csrf) = r.pair();
    let lib = r.req("GET", "/api/games?system=gba", &[("Cookie", &cookie)], b"").json();
    let id = lib["games"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(lib["games"][0]["name"], "Advance Wars");
    let body = format!(r#"{{"id":"{id}","on":true}}"#);
    let star = |headers: &[(&str, &str)]| r.req("POST", "/api/game/favorite", headers, body.as_bytes()).status;
    assert_eq!(star(&[("Cookie", &cookie)]), 403, "no token");
    assert_eq!(star(&[("Cookie", &cookie), ("X-CSRF-Token", "0123456789abcdef0123456789abcdef")]), 403, "a wrong token");
    assert_eq!(star(&[("Cookie", &cookie), ("X-CSRF-Token", &csrf), ("Origin", "http://evil.example")]), 403);
    assert_eq!(star(&[("Cookie", &cookie), ("X-CSRF-Token", &csrf), ("Sec-Fetch-Site", "cross-site")]), 403);
    assert_eq!(star(&[("X-CSRF-Token", &csrf)]), 401, "no cookie");
    assert!(r.edits.lock().unwrap().is_empty(), "nothing reached the App");
    let origin = format!("http://{}", r.addr);
    assert_eq!(star(&[("Cookie", &cookie), ("X-CSRF-Token", &csrf), ("Origin", &origin), ("Sec-Fetch-Site", "same-origin")]), 200);
    let path = r.root.join("roms/gba/Advance Wars (USA).gba");
    assert_eq!(r.edits.lock().unwrap().as_slice(), [Edit::Favorite { system: "gba".into(), name: "Advance Wars".into(), path: path.clone(), on: true }]);

    let post = |route: &str, json: String| r.req("POST", route, &[("Cookie", &cookie), ("X-CSRF-Token", &csrf)], json.as_bytes());
    assert_eq!(post("/api/game/status", format!(r#"{{"id":"{id}","status":"completed"}}"#)).status, 200);
    assert_eq!(post("/api/game/status", format!(r#"{{"id":"{id}","status":"won"}}"#)).status, 400);
    assert_eq!(post("/api/game/note", format!(r#"{{"id":"{id}","note":"Chapter 4"}}"#)).status, 200);
    assert_eq!(post("/api/game/sticker", format!(r#"{{"id":"{id}","sticker":3,"on":false}}"#)).status, 200);
    assert_eq!(post("/api/stickers/new", r#"{"name":"Couch"}"#.into()).status, 200);
    assert_eq!(post("/api/stickers/rename", r#"{"sticker":3,"name":"Sofa"}"#.into()).status, 200);
    assert_eq!(post("/api/shelves/delete", r#"{"index":0,"was":"Racing"}"#.into()).status, 200);
    assert_eq!(post("/api/game/note", r#"{"id":"0000000000000000","note":"x"}"#.into()).status, 404, "ids, never paths");
    assert_eq!(post("/api/game/note", "not json".into()).status, 400);
    let edits = r.edits.lock().unwrap().clone();
    assert_eq!(edits[1], Edit::Status { path: path.clone(), status: crate::library_meta::Status::Completed });
    assert_eq!(edits[2], Edit::Note { path: path.clone(), note: "Chapter 4".into() });
    assert_eq!(edits[3], Edit::Sticker { path: path.clone(), id: 3, on: false });
    assert_eq!(edits[4], Edit::NewSticker { name: "Couch".into(), path: None });
    assert_eq!(edits[5], Edit::RenameSticker { id: 3, name: "Sofa".into() });
    assert_eq!(edits[6], Edit::DeleteShelf { index: 0, was: "Racing".into() });
    let big = format!(r#"{{"id":"{id}","note":"{}"}}"#, "x".repeat(70_000));
    assert_eq!(post("/api/game/note", big).status, 413);
    // The rating: 0 to 5 (0 takes it away); anything else is refused, never cut down to another rating.
    assert_eq!(post("/api/game/rating", format!(r#"{{"id":"{id}","rating":4}}"#)).status, 200);
    assert_eq!(post("/api/game/rating", format!(r#"{{"id":"{id}","rating":0}}"#)).status, 200);
    for bad in ["6", "-1", "\"4\"", "2.5", "null"] {
        assert_eq!(post("/api/game/rating", format!(r#"{{"id":"{id}","rating":{bad}}}"#)).status, 400, "{bad}");
    }
    let edits = r.edits.lock().unwrap().clone();
    assert_eq!(edits[edits.len() - 2..], [Edit::Rating { path: path.clone(), rating: 4 }, Edit::Rating { path: path.clone(), rating: 0 }]);
    // A game's row says its rating (0: none).
    let lib = r.req("GET", "/api/games?system=gba", &[("Cookie", &cookie)], b"").json();
    assert!(lib["games"][0]["rating"].is_u64(), "{}", lib["games"][0]);
}

#[test]
fn the_owners_tailnet_devices_get_in_without_a_code() {
    let r = rig("api-owner", true);
    let s = r.req("GET", "/api/session", &[], b"").json();
    assert_eq!(s["signedIn"], true);
    assert_eq!(s["via"], "tailscale");
    assert_eq!(s["name"], "owner@example.com");
    let csrf = s["csrf"].as_str().unwrap().to_string();
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 200);
    assert_eq!(r.req("POST", "/api/stickers/new", &[], br#"{"name":"x"}"#).status, 403, "writes still need the token");
    assert_eq!(r.req("POST", "/api/stickers/new", &[("X-CSRF-Token", &csrf)], br#"{"name":"x"}"#).status, 200);
    // The Pocket's own name works as the Host too.
    let named = r.raw(b"GET /api/library HTTP/1.1\r\nHost: pocket\r\n\r\n");
    assert_eq!(named.status, 200);
    // Switched off on the Pocket: a code for everybody.
    r.shared.prefs.lock().unwrap().trust_tailnet = false;
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 401);
}

#[test]
fn a_stranger_on_the_tailnet_needs_a_code() {
    let r = rig("api-stranger", false);
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 401);
}

#[test]
fn while_a_game_runs_only_small_requests_are_answered() {
    let r = rig("api-playing", false);
    let (cookie, _) = r.pair();
    r.shared.game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    *r.shared.now_playing.lock().unwrap() = Some(super::NowPlaying { system: "gba".into(), name: "Advance Wars".into(), path: r.root.join("roms/gba/Advance Wars (USA).gba"), since: 1 });
    let lib = r.req("GET", "/api/library", &[("Cookie", &cookie)], b"");
    assert_eq!(lib.status, 503);
    assert_eq!(lib.json()["playing"], true);
    let st = r.req("GET", "/api/status", &[("Cookie", &cookie)], b"").json();
    assert_eq!(st["gameRunning"], true);
    assert_eq!(st["playing"]["name"], "Advance Wars");
    let shot = r.req("GET", "/api/screenshot", &[("Cookie", &cookie)], b"");
    assert_eq!(shot.status, 200);
    assert!(shot.body.starts_with(b"\x89PNG"));
    assert_eq!(shot.header("x-shot-age").as_deref(), Some("0"));
    let again = r.req("GET", "/api/screenshot", &[("Cookie", &cookie)], b"");
    assert_eq!(again.body, shot.body, "the same picture within ten seconds, not a new capture");
}

#[test]
fn uploads_through_the_api() {
    let r = rig("api-upload", false);
    let (cookie, csrf) = r.pair();
    let auth = [("Cookie", cookie.as_str()), ("X-CSRF-Token", csrf.as_str())];
    let u = r.req("GET", "/api/uploads", &[("Cookie", &cookie)], b"").json();
    let ids: Vec<&str> = u["systems"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["gba"], "no ports among the targets");
    assert!(u["bios"].as_array().unwrap().iter().any(|b| b["missing"] == "Missing for Famicom Disk System: disksys.rom"));
    assert_eq!(u["ports"][0]["dir"], "sprout");

    assert_eq!(r.req("GET", "/api/upload/check?kind=rom&system=gba&name=x.gba&size=4", &[("Cookie", &cookie)], b"").status, 404, "a check is a write: POST");
    assert_eq!(r.req("POST", "/api/upload/check?kind=rom&system=gba&name=x.gba&size=4", &[("Cookie", &cookie)], b"").status, 403, "with the token");
    let check = r.req("POST", "/api/upload/check?kind=rom&system=gba&name=..%2F..%2FNew.gba&size=4", &auth, b"").json();
    assert_eq!(check["ok"], true);
    assert_eq!(check["exists"], false);
    assert!(check["path"].as_str().unwrap().ends_with("roms/gba/New.gba"));

    let up = |q: &str, body: &[u8]| r.req("POST", &format!("/api/upload?{q}"), &auth, body);
    let a = up("kind=rom&system=gba&name=..%2F..%2FNew.gba", b"data");
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(std::fs::read(r.root.join("roms/gba/New.gba")).unwrap(), b"data");
    assert!(!r.root.join("New.gba").exists());
    let news = r.edits.lock().unwrap().iter().find_map(|e| match e {
        Edit::LibraryChanged(super::Systems(s)) => Some(s.clone()),
        _ => None,
    });
    let news = news.expect("the App gets the new library");
    assert!(news[0].games.iter().any(|g| g.name == "New"), "read with the new game, off the App's thread");
    assert_eq!(up("kind=rom&system=gba&name=New.gba", b"more").status, 409, "there already");
    assert_eq!(up("kind=rom&system=gba&name=New.gba&overwrite=1", b"more").json()["replaced"], true);
    assert_eq!(std::fs::read(r.root.join("roms/gba/New.gba")).unwrap(), b"more");
    assert_eq!(up("kind=rom&system=ports&name=evil.sh", b"#!/bin/sh").status, 403);
    assert_eq!(up("kind=rom&system=gba&name=evil.sh", b"#!/bin/sh").status, 403);
    assert!(!r.root.join("roms/ports/evil.sh").exists() && !r.root.join("roms/gba/evil.sh").exists());
    assert_eq!(up("kind=nope&name=x", b"x").status, 403);

    let b = up("kind=bios&name=disksys.rom", b"bios");
    assert_eq!(b.json()["bios"], "Famicom Disk System");
    assert!(r.root.join("roms/bios/disksys.rom").is_file());
    let big = r.raw(format!("POST /api/upload?kind=bios&name=huge.bin HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Length: {}\r\n\r\n", r.addr, files::MAX_BIOS + 1).as_bytes());
    assert_eq!(big.status, 413, "refused before a byte of the body is read");
    let chunked = r.raw(format!("POST /api/upload?kind=bios&name=c.bin HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\n", r.addr).as_bytes());
    assert_eq!(chunked.status, 411);

    assert_eq!(up("kind=port&dir=sprout&rel=sub%2Fdata.win&name=data.win", b"win").status, 200);
    assert!(r.root.join("roms/port-files/sprout/sub/data.win").is_file());
    assert!(!r.root.join("roms/ports/sprout/assets/data.win").exists(), "the port gets it when it starts, not now");
    assert_eq!(up("kind=port&dir=sprout&rel=..%2F..%2Fports%2Fx.sh&name=x.sh", b"x").status, 403);
    assert_eq!(up("kind=port&dir=..&rel=x&name=x", b"x").status, 403);
    let leftovers: Vec<_> = walk(&r.root.join("roms")).into_iter().filter(|p| p.to_string_lossy().ends_with(".upload")).collect();
    assert!(leftovers.is_empty(), "no temporary files left: {leftovers:?}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn library_lists_search_and_stats() {
    let r = rig("api-lists", false);
    let (cookie, _) = r.pair();
    let get = |p: &str| r.req("GET", p, &[("Cookie", &cookie)], b"");
    let lib = get("/api/library").json();
    assert_eq!(lib["systems"][0]["id"], "gba");
    assert_eq!(lib["systems"][0]["stripe"], "#5b4aa6");
    let hits = get("/api/games?q=advance%20wars").json();
    assert_eq!(hits["total"], 1);
    let id = hits["games"][0]["id"].as_str().unwrap().to_string();
    let game = get(&format!("/api/game?id={id}")).json();
    assert_eq!(game["file"], "Advance Wars (USA).gba");
    assert!(game.get("path").is_none(), "the full path never travels");
    assert_eq!(get("/api/game?id=../../etc").status, 404);
    assert_eq!(get(&format!("/api/cover?id={id}")).status, 404, "no cover downloaded");
    assert_eq!(get("/api/games?system=nes").status, 404);
    assert_eq!(get("/api/games").status, 400);
    let stats = get("/api/stats").json();
    assert_eq!(stats["totalSecs"], 0);
    assert!(stats["history"].is_null(), "no play history registered");
    assert_eq!(get("/api/battery").status, 200);
    let _ = api::SHOT_EVERY;
}

/// The socket is tied to its interface: on Linux, a connection that arrives on another interface (here: loopback,
/// to a socket tied to eth0) never reaches it, whatever address it was sent to. This is what keeps the Wi-Fi off the
/// Tailscale socket.
#[cfg(target_os = "linux")]
#[test]
fn a_socket_tied_to_an_interface_takes_nothing_from_another() {
    let lo = Listen { addr: "127.0.0.1:0".parse().unwrap(), iface: "lo".into(), index: 0 };
    let s = net::bind(&lo).expect("bind to lo");
    let addr = s.local_addr().unwrap();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok(), "the right interface answers");
    let Some(other) = net::interfaces().into_iter().map(|i| i.name).find(|n| n != "lo") else { return };
    let tied = Listen { addr: "127.0.0.1:0".parse().unwrap(), iface: other, index: 0 };
    let s = match net::bind(&tied) {
        Ok(s) => s,
        // Without CAP_NET_RAW on an old kernel the option can't be set: nothing to check here.
        Err(e) if e.raw_os_error() == Some(libc::EPERM) => return,
        Err(e) => panic!("{e}"),
    };
    let addr = s.local_addr().unwrap();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_err(), "a connection over lo must not reach a socket tied to {}", tied.iface);
}

#[test]
fn the_api_answers_only_this_page() {
    let r = rig("api-site", true);
    // The owner's device needs no cookie: a page of another site open in the same browser could aim requests at the
    // Pocket's address. Browsers mark them; they are refused before anything happens.
    for path in ["/api/screenshot", "/api/library", "/api/session"] {
        assert_eq!(r.req("GET", path, &[("Sec-Fetch-Site", "cross-site")], b"").status, 403, "{path}");
        assert_eq!(r.req("GET", path, &[("Origin", "http://evil.example")], b"").status, 403, "{path}");
    }
    assert_eq!(r.req("GET", "/api/library", &[("Sec-Fetch-Site", "same-origin")], b"").status, 200);
    assert_eq!(r.req("GET", "/", &[("Sec-Fetch-Site", "none")], b"").status, 200, "the page itself opens from the address bar");
}

#[test]
fn a_refused_upload_still_says_why() {
    let r = rig("api-refused", false);
    // No cookie, and a body on its way: the browser must read the 401, not a reset connection.
    let body = vec![b'x'; 300 * 1024];
    let a = r.req("POST", "/api/upload?kind=rom&system=gba&name=a.gba", &[], &body);
    assert_eq!(a.status, 401);
    assert_eq!(a.json()["error"], "Pair this browser first.");
}

#[test]
fn a_cut_off_upload_keeps_nothing() {
    let r = rig("api-cut", false);
    let (cookie, csrf) = r.pair();
    let mut s = TcpStream::connect(r.addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "POST /api/upload?kind=rom&system=gba&name=Cut.gba HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Length: 100000\r\n\r\n", r.addr).unwrap();
    s.write_all(&[7u8; 4000]).unwrap();
    // The browser tab closes: the rest never comes.
    s.shutdown(std::net::Shutdown::Write).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 400"), "{out}");
    assert!(out.contains("The upload was cut off. Nothing was kept."), "{out}");
    let gba: Vec<PathBuf> = walk(&r.root.join("roms/gba"));
    assert_eq!(gba, [r.root.join("roms/gba/Advance Wars (USA).gba")], "no half file, no temporary file");
}

#[test]
fn an_empty_library_answers_with_empty_lists() {
    let r = rig("api-empty", false);
    std::fs::remove_file(r.root.join("roms/gba/Advance Wars (USA).gba")).unwrap();
    let (cookie, _) = r.pair();
    let get = |p: &str| r.req("GET", p, &[("Cookie", &cookie)], b"").json();
    let lib = get("/api/library");
    assert_eq!(lib["systems"], serde_json::json!([]));
    assert_eq!(lib["favorites"], 0);
    let stats = get("/api/stats");
    assert_eq!((stats["totalSecs"].as_u64(), stats["played"].as_u64(), stats["sessions"].as_u64()), (Some(0), Some(0), Some(0)));
    assert_eq!(stats["systems"], serde_json::json!([]));
    assert!(stats["history"].is_null());
    let hits = get("/api/games?q=zelda");
    assert_eq!(hits["total"], 0);
    // Uploads still know where games go: every system of es_systems.cfg, with or without games.
    assert_eq!(get("/api/uploads")["systems"][0]["id"], "gba");
}

#[test]
fn pairing_tries_from_several_connections_queue_a_second_apart() {
    let r = Arc::new(rig("api-pairgate", false));
    *r.shared.pairing.lock().unwrap() = Some(Pairing { code: "135790".into(), until: Instant::now() + Duration::from_secs(60), tries_left: 5 });
    let t = Instant::now();
    let tries: Vec<_> = (0..3)
        .map(|i| {
            let r = r.clone();
            std::thread::spawn(move || r.req("POST", "/api/pair", &[("Content-Type", "application/json")], format!(r#"{{"code":"00000{i}"}}"#).as_bytes()).status)
        })
        .collect();
    for t in tries {
        assert_eq!(t.join().unwrap(), 403);
    }
    assert!(t.elapsed() >= Duration::from_millis(1900), "three wrong codes at once take two seconds at least: {:?}", t.elapsed());
    assert_eq!(r.shared.pairing.lock().unwrap().as_ref().map(|p| p.tries_left), Some(2));
}

/// Play history as `play_history::stats_json` gives it (docs/HISTORY.md, "Read API"): full paths in every game.
fn fake_history(data_dir: &Path) -> Option<serde_json::Value> {
    let roms = data_dir.parent()?.join("roms");
    let here = roms.join("gba/Advance Wars (USA).gba").to_string_lossy().into_owned();
    let gone = roms.join("gb/Deleted Since.gb").to_string_lossy().into_owned();
    let game = |path: &str| serde_json::json!({ "name": "Advance Wars", "system": "gba", "path": path, "secs": 3600, "sessions": 2, "days": 1, "new": true });
    let period = serde_json::json!({
        "label": "This week", "first": "2026-10-19", "last": "2026-10-25", "secs": 3600, "sessions": 2, "days_played": 1,
        "games": [game(&here), game(&gone)],
        "systems": [{ "system": "gba", "secs": 3600, "sessions": 2, "games": 1 }],
        "bars": [{ "first": "2026-10-19", "days": 1, "secs": 3600 }],
        "longest_streak": { "days": 1, "from": "2026-10-19", "to": "2026-10-19" }, "current_streak": 1,
        "came_back": [{ "name": "Advance Wars", "system": "gba", "path": here, "gap_days": 90, "date": "2026-10-19" }],
        "completed": [{ "name": "Gone", "system": "gb", "path": gone, "date": "2026-10-20" }],
        "new_games": 1, "not_recorded": false,
        "stray": "/storage/roms/gba/somewhere.gba",
    });
    Some(serde_json::json!({
        "version": 1, "today": "2026-10-23", "counting_since": "2026-03-02", "damaged": false,
        "all_time": { "secs": 7200, "sessions": 4, "games": 1, "carried_secs": 3600 },
        "week": period, "month": period, "year": period,
        "recent": [{ "name": "Advance Wars", "system": "gba", "path": here, "start": 1792648800, "date": "2026-10-22", "secs": 600, "ended": true, "emulator": "pocket-core/mgba" }],
    }))
}

#[test]
fn the_play_history_reaches_the_stats_page_without_a_single_path() {
    let r = rig_with("api-history", false, Some(fake_history));
    let (cookie, _) = r.pair();
    let a = r.req("GET", "/api/stats", &[("Cookie", &cookie)], b"");
    assert_eq!(a.status, 200);
    let text = String::from_utf8_lossy(&a.body);
    assert!(!text.contains("/roms/") && !text.contains(&*r.root.to_string_lossy()) && !text.contains("\"path\""), "no path travels: {text}");
    let h = &a.json()["history"];
    let id = r.req("GET", "/api/games?system=gba", &[("Cookie", &cookie)], b"").json()["games"][0]["id"].clone();
    assert_eq!(h["week"]["games"][0]["id"], id, "a game on the Pocket gets its id, for a link");
    assert!(h["week"]["games"][1].get("id").is_none(), "a game no longer on the Pocket gets none");
    assert_eq!(h["week"]["games"][1]["name"], "Advance Wars", "the rest stays");
    assert_eq!(h["recent"][0]["id"], id);
    assert_eq!(h["week"]["came_back"][0]["id"], id);
    assert!(h["week"]["stray"].is_null(), "any other string that looks like a path goes too");
    assert_eq!(h["week"]["bars"][0]["secs"], 3600);
    assert_eq!(h["counting_since"], "2026-03-02");
}

#[test]
fn a_taken_port_is_reported_not_waited_for() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap();
    let busy = tmp("busy");
    let shared = Shared::new(busy.to_path_buf(), PathBuf::from("/nonexistent/es_systems.cfg"), Prefs { enabled: true, ..Prefs::default() });
    let ctx = Ctx::new(shared.clone(), Roots::under(Path::new("/nonexistent")), Box::new(FakeTailnet { peers: Arc::new(Mutex::new(HashMap::new())) }));
    let _server = super::server::Server::start(ctx, Some(vec![Listen { addr, iface: "lo".into(), index: 0 }])).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared.problem.lock().unwrap().is_none() {
        assert!(Instant::now() < deadline, "no reason given");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.problem.lock().unwrap().as_deref(), Some("Another program on the Pocket uses port 80."));
    assert!(shared.listening.lock().unwrap().is_empty());
}

// --- review 2026-10-24 (docs/WEB-COMPANION.md, "Security self-review") ------------------------------------------

/// F2: a client that trickles a pairing body gets the short deadline, not a minute per byte.
#[test]
fn a_trickled_body_runs_out_of_time() {
    let r = rig_ctx("api-trickle", false, |ctx| ctx.deadline = Duration::from_millis(400));
    let mut s = TcpStream::connect(r.addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let t = Instant::now();
    write!(s, "POST /api/pair HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{", r.addr).unwrap();
    for _ in 0..6 {
        std::thread::sleep(Duration::from_millis(150));
        if s.write_all(b" ").is_err() {
            break;
        }
    }
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out);
    assert!(text.starts_with("HTTP/1.1 400"), "{text}");
    assert!(text.contains("The request was cut off."), "{text}");
    assert!(t.elapsed() < Duration::from_millis(2500), "{:?}", t.elapsed());
}

/// F2: the head's deadline counts from the accept, so a connection that waited in the queue is not given more.
#[test]
fn a_connection_that_waited_in_the_queue_has_had_its_time() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let _client = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    let (stream, peer) = l.accept().unwrap();
    let queued = tmp("queued");
    let shared = Shared::new(queued.to_path_buf(), PathBuf::from("/nonexistent/es_systems.cfg"), Prefs::default());
    let mut ctx = Ctx::new(shared.clone(), Roots::under(Path::new("/nonexistent")), Box::new(FakeTailnet { peers: Arc::new(Mutex::new(HashMap::new())) }));
    ctx.deadline = Duration::from_millis(400);
    let conn = api::Conn { peer, local: stream.local_addr().unwrap(), accepted: Instant::now() - Duration::from_secs(2) };
    let t = Instant::now();
    api::handle(&ctx, &conn, stream);
    assert!(t.elapsed() < Duration::from_millis(200), "it waited {:?} for a head whose time was up", t.elapsed());
}

/// F3: "busy" from the supervisor never waits for a client that reads nothing.
#[test]
fn busy_never_blocks_the_supervisor() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    let (mut server_side, _) = l.accept().unwrap();
    // The client reads nothing: fill what the kernel buffers until a write would block.
    server_side.set_nonblocking(true).unwrap();
    let junk = vec![0u8; 64 * 1024];
    loop {
        match server_side.write(&junk) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("{e}"),
        }
    }
    server_side.set_nonblocking(false).unwrap();
    let t = Instant::now();
    super::server::reply_busy(server_side);
    assert!(t.elapsed() < Duration::from_millis(200), "{:?}", t.elapsed());
    drop(client);
}

/// F3, F7: switching off returns at once with a request hanging on a worker, and an upload under way stops at its
/// next chunk without leaving anything.
#[test]
fn switching_off_stops_an_upload_and_never_waits_for_it() {
    let mut r = rig("api-stop", false);
    let (cookie, csrf) = r.pair();
    // One connection that never finishes its head, holding a worker.
    let mut idle = TcpStream::connect(r.addr).unwrap();
    idle.write_all(b"GET / HTTP/1.1\r\n").unwrap();
    let mut s = TcpStream::connect(r.addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "POST /api/upload?kind=rom&system=gba&name=Stop.gba HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Length: 4000000\r\n\r\n", r.addr).unwrap();
    s.write_all(&vec![1u8; 300_000]).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    r.server.take().unwrap().stop();
    assert!(t.elapsed() < Duration::from_millis(1500), "stop waited {:?}", t.elapsed());
    // More of the file arrives: the worker reads it, sees the switch and stops.
    let _ = s.write_all(&vec![1u8; 600_000]);
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    assert!(out.is_empty() || out.contains("switched off"), "{out}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let left: Vec<PathBuf> = walk(&r.root.join("roms/gba")).into_iter().filter(|p| !p.ends_with("Advance Wars (USA).gba")).collect();
        if left.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "left behind: {left:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(idle);
}

/// F4: no capture while the Pocket shows Settings (the pairing code) or the first-start steps.
#[test]
fn the_screenshot_hides_private_screens() {
    let r = rig("api-private", false);
    let (cookie, _) = r.pair();
    r.shared.screen_private.store(true, std::sync::atomic::Ordering::Relaxed);
    let a = r.req("GET", "/api/screenshot", &[("Cookie", &cookie)], b"");
    assert_eq!(a.status, 200);
    assert_eq!(a.header("x-screen-hidden").as_deref(), Some("1"));
    assert!(a.body.starts_with(b"\x89PNG"), "a placeholder picture");
    assert_eq!(r.shots.load(std::sync::atomic::Ordering::Relaxed), 0, "the screen was not even read");
    r.shared.screen_private.store(false, std::sync::atomic::Ordering::Relaxed);
    let b = r.req("GET", "/api/screenshot", &[("Cookie", &cookie)], b"");
    assert!(b.header("x-screen-hidden").is_none());
    assert_eq!(r.shots.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// F6: a paired browser ends a year after its pairing on the Pocket too, not only in the browser.
#[test]
fn a_paired_browser_expires_on_the_pocket() {
    let dir = tmp("expiry");
    let mut s = Sessions::load(&dir);
    let old = s.pair("Old").unwrap();
    let young = s.pair("Young").unwrap();
    let now = crate::recents::now_secs();
    s.browsers[0].paired = now - auth::SESSION_LIFETIME.as_secs() - 60;
    s.browsers[1].paired = now - auth::SESSION_LIFETIME.as_secs() + 3600;
    assert!(s.check(&old).is_none(), "older than a year");
    assert!(s.check(&young).is_some());
    assert_eq!(s.browsers.len(), 1, "the expired one is forgotten");
    assert_eq!(Sessions::load(&dir).browsers.len(), 1, "for good");
}

/// F8: a cover larger than the limit is never read.
#[test]
fn a_huge_cover_is_not_read() {
    let r = rig("api-cover", false);
    let (cookie, _) = r.pair();
    let id = r.req("GET", "/api/games?system=gba", &[("Cookie", &cookie)], b"").json()["games"][0]["id"].as_str().unwrap().to_string();
    let path = crate::covers::cover_path(&r.shared.data_dir, "gba", &r.root.join("roms/gba/Advance Wars (USA).gba"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let f = std::fs::File::create(&path).unwrap();
    (&f).write_all(b"\x89PNG\r\n\x1a\n").unwrap();
    f.set_len(9 << 20).unwrap();
    assert_eq!(r.req("GET", &format!("/api/cover?id={id}"), &[("Cookie", &cookie)], b"").status, 404);
    f.set_len(100).unwrap();
    assert_eq!(r.req("GET", &format!("/api/cover?id={id}"), &[("Cookie", &cookie)], b"").status, 200);
}

/// The collection's pictures of a game: listed on its page by kind, served read only through `/api/cover?kind=`, a
/// game without them lists none, and "Box art only" hides them.
#[test]
fn a_games_collection_pictures_are_listed_and_served_unless_box_art_only() {
    let r = rig("api-pictures", false);
    let (cookie, _) = r.pair();
    let get = |p: &str| r.req("GET", p, &[("Cookie", &cookie)], b"");
    let id = get("/api/games?system=gba").json()["games"][0]["id"].as_str().unwrap().to_string();
    let rom = r.root.join("roms/gba/Advance Wars (USA).gba");
    assert_eq!(get(&format!("/api/game?id={id}")).json()["pictures"], serde_json::json!([]), "none downloaded: an empty list, no gap");
    assert_eq!(get(&format!("/api/cover?id={id}&kind=snap")).status, 404);
    let snap = crate::game_pictures::path(&r.shared.data_dir, crate::game_pictures::Kind::Snap, "gba", &rom);
    std::fs::create_dir_all(snap.parent().unwrap()).unwrap();
    std::fs::write(&snap, b"\x89PNG\r\n\x1a\nrest").unwrap();
    assert_eq!(get(&format!("/api/game?id={id}")).json()["pictures"], serde_json::json!(["snap"]), "only the one it has");
    assert_eq!(get(&format!("/api/cover?id={id}&kind=snap")).status, 200);
    assert_eq!(get(&format!("/api/cover?id={id}&kind=title")).status, 404, "no title screen");
    assert_eq!(get(&format!("/api/cover?id={id}&kind=../../etc/passwd")).status, 404, "only the kinds it knows");
    crate::settings::Settings { box_art_only: true, onboarded: true, ..Default::default() }.save(&r.shared.data_dir);
    assert_eq!(get(&format!("/api/cover?id={id}&kind=snap")).status, 404, "Box art only");
    assert!(get(&format!("/api/game?id={id}")).json().get("pictures").is_none());
}

/// A game's page lists its screenshots (HOME + A in a game) by time, and `/api/shot` serves exactly those: no path
/// travels, nothing else on the card can be asked for, a missing folder is an empty list.
#[test]
fn a_games_screenshots_are_listed_and_served_read_only() {
    let r = rig("api-shots", false);
    let (cookie, _) = r.pair();
    let get = |p: &str| r.req("GET", p, &[("Cookie", &cookie)], b"");
    let id = get("/api/games?system=gba").json()["games"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(get(&format!("/api/game?id={id}")).json()["shots"], serde_json::json!([]), "no folder yet: an empty list");
    let shots = r.root.join("roms/screenshots/gba");
    std::fs::create_dir_all(&shots).unwrap();
    let png = crate::screenshot::encode_rgb(2, 2, &[9; 12]).unwrap();
    std::fs::write(shots.join("Advance Wars (USA) 2026-10-24 18-05-09.png"), &png).unwrap();
    std::fs::write(shots.join("Advance Wars (USA) 2026-10-25 09-00-00.png"), &png).unwrap();
    std::fs::write(shots.join("Advance Wars (USA) 2 2026-10-26 09-00-00.png"), &png).unwrap(); // another game
    std::fs::write(r.root.join("roms/screenshots/secret.png"), &png).unwrap();
    let game = get(&format!("/api/game?id={id}")).json();
    assert_eq!(game["shots"], serde_json::json!(["2026-10-25 09-00-00", "2026-10-24 18-05-09"]), "newest first, only this game's");
    assert!(!game.to_string().contains("screenshots"), "no path in the page's data: {game}");
    let at = "2026-10-24%2018-05-09";
    let one = get(&format!("/api/shot?id={id}&at={at}"));
    assert_eq!((one.status, one.header("content-type").as_deref()), (200, Some("image/png")));
    assert_eq!(one.body, png);
    for evil in ["../../secret", "..%2F..%2Fsecret", "2026-10-24%2018-05-09.png", "", "2026-01-01%2000-00-00"] {
        assert_eq!(get(&format!("/api/shot?id={id}&at={evil}")).status, 404, "{evil}");
    }
    assert_eq!(get("/api/shot?id=nope&at=2026-10-24%2018-05-09").status, 404);
    assert_eq!(get(&format!("/api/shot?id={id}")).status, 404);
    // A file that is not a PNG is not passed on.
    std::fs::write(shots.join("Advance Wars (USA) 2026-10-27 10-00-00.png"), b"<html>").unwrap();
    assert_eq!(get(&format!("/api/shot?id={id}&at=2026-10-27%2010-00-00")).status, 404);
    // Nothing writes: a POST is no way in.
    assert_ne!(r.req("POST", "/api/shot", &[("Cookie", &cookie)], b"{}").status, 200);
}

/// F9: the reserve holds against uploads at the same time, and a card whose free space can't be read takes nothing.
#[test]
fn room_counts_what_other_uploads_reserved() {
    let r = files::RESERVE;
    assert!(files::room(Some(r + 100), 0, 100, u64::MAX).is_ok());
    assert!(matches!(files::room(Some(r + 100), 1, 100, u64::MAX), Err(Refused::NoSpace(_))), "another upload holds a byte");
    assert!(matches!(files::room(None, 0, 1, u64::MAX), Err(Refused::NoSpace(_))), "unknown free space: refused");
    assert!(matches!(files::room(Some(u64::MAX), 0, 11, 10), Err(Refused::TooBig(_))));
    let dir = tmp("reserve");
    let held = files::reserve(&dir.join("a.gba"), 1000, u64::MAX).unwrap();
    assert!(files::reserved() >= 1000);
    drop(held);
}

/// F10: no native code into a port's folder, by name or by content.
#[test]
fn no_programs_go_into_a_port() {
    let root = tmp("native");
    let roms = root.join("roms");
    port(&roms, false);
    let roots = Roots::under(&roms);
    for bad in ["libgame.so", "lib/libSDL2-2.0.so.0", "lib/libfoo.so.1.2.3", "game.elf", "x.ko", "Game.AppImage", "setup.run"] {
        assert!(files::port_target(&roots, "sprout", bad).is_err(), "{bad}");
    }
    for fine in ["data.win", "music/a.ogg", "game.so.dat", "notes.solution"] {
        assert!(files::port_target(&roots, "sprout", fine).is_ok(), "{fine}");
    }
    let target = files::port_target(&roots, "sprout", "data.win").unwrap();
    let gate = files::Gate { no_programs: true, ..files::Gate::OPEN };
    let elf = b"\x7fELF\x02\x01\x01\x00rest of a program";
    assert!(matches!(files::receive(&mut &elf[..], &target, elf.len() as u64, false, &gate), Err(Refused::NotAllowed(_))));
    assert!(walk(target.parent().unwrap()).is_empty(), "nothing kept");
    assert!(files::receive(&mut &b"GMS\x00data"[..], &target, 8, false, &gate).is_ok(), "game data is fine");
    assert!(files::bios_target(&Roots::under(&roms), "evil.so", None).is_err());
}

/// F12: a system folder that is a link into a folder uploads never go to is refused.
#[cfg(unix)]
#[test]
fn a_link_into_a_forbidden_folder_is_refused() {
    let root = tmp("neverlink");
    let roms = root.join("roms");
    std::fs::create_dir_all(roms.join("ports")).unwrap();
    std::os::unix::fs::symlink(roms.join("ports"), roms.join("gba")).unwrap();
    let roots = Roots::under(&roms);
    let gba = sys("gba", &roms.join("gba"), &["gba"]);
    assert!(matches!(files::rom_target(&roots, &gba, "Game.gba"), Err(Refused::NotAllowed(_))));
    // A link to another game folder is fine: the file goes there.
    std::fs::create_dir_all(roms.join("gb")).unwrap();
    std::os::unix::fs::symlink(roms.join("gb"), roms.join("gbc")).unwrap();
    let gbc = sys("gbc", &roms.join("gbc"), &["gbc"]);
    assert_eq!(files::rom_target(&roots, &gbc, "Game.gbc").unwrap(), roms.canonicalize().unwrap().join("gb/Game.gbc"));
}

/// F11: the folder an upload is written into is resolved again before the rename: a link on the way is refused.
#[cfg(unix)]
#[test]
fn a_folder_that_became_a_link_keeps_nothing() {
    let dir = tmp("swapped");
    std::fs::create_dir_all(dir.join("real")).unwrap();
    std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
    let r = files::receive(&mut &b"data"[..], &dir.join("link/a.gba"), 4, false, &files::Gate::OPEN);
    assert!(matches!(r, Err(Refused::NotAllowed(_))), "{r:?}");
    assert!(walk(&dir.join("real")).is_empty(), "nothing kept, no temporary file");
}

/// F13: an interface made anew (another index, the same name and address) is a change of the plan.
#[test]
fn a_new_tailscale_interface_is_listened_on_anew() {
    let a = net::plan(&[iface("tailscale0", "100.77.1.2")], 80);
    let mut again = iface("tailscale0", "100.77.1.2");
    again.index = 8;
    assert_ne!(a, net::plan(&[again], 80));
}

/// F14: changing "Your Tailscale devices" drops what tailscaled said about each device.
#[test]
fn a_change_of_trust_asks_tailscale_again() {
    let r = rig("api-trust", true);
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 200);
    // The device leaves the owner's account; the answer from a moment ago is still kept for a minute …
    r.peers.lock().unwrap().clear();
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 200);
    // … unless the switch changes: then the next request asks again.
    r.shared.trust_changed();
    assert_eq!(r.req("GET", "/api/library", &[], b"").status, 401);
}

/// F15: whatever looks like a path leaves the history, keys included.
#[test]
fn every_kind_of_path_leaves_the_history() {
    let lib = api::LibCache::build(Vec::new(), Vec::new());
    let mut v = serde_json::json!({
        "games": [{ "name": "Tetris", "file": "roms/gb/Tetris.gb", "note": "played /storage/roms/gb", "emulator": "pocket-core/gambatte", "other": "/home/x" }],
        "by_path": { "/storage/roms/gb/Tetris.gb": 3, "fine": 1 },
        "data": "C:\\pocket\\data.bin",
        "dir": "/var/lib/pocket-data/web",
        "mine": "seen in /var/lib/pocket-data today",
    });
    api::without_paths(&mut v, &lib, &["/var/lib/pocket-data".to_string()]);
    let g = &v["games"][0];
    assert_eq!(g["name"], "Tetris");
    assert_eq!(g["emulator"], "pocket-core/gambatte", "a slash alone is no path");
    for k in ["file", "note", "other"] {
        assert!(g[k].is_null(), "{k}: {}", g[k]);
    }
    assert_eq!(v["by_path"], serde_json::json!({ "fine": 1 }), "keys that are paths go");
    assert!(v["dir"].is_null() && v["mine"].is_null());
    assert_eq!(v["data"], "C:\\pocket\\data.bin", "not a path of the Pocket's");
}

/// The history names a game the way the game list does: two versions of one game stay two games.
#[test]
fn the_history_shows_the_games_list_names() {
    let sys = crate::library::System {
        name: "gb".into(),
        fullname: "Game Boy".into(),
        emulator: None,
        core: None,
        emulators: Vec::new(),
        games: vec![crate::library::Game { name: "Tobu Tobu Girl (Europe)".into(), path: "/storage/roms/gb/Tobu Tobu Girl (Europe).gb".into() }],
    };
    let lib = api::LibCache::build(vec![sys], Vec::new());
    let mut v = serde_json::json!({ "games": [
        { "name": "Tobu Tobu Girl", "path": "/storage/roms/gb/Tobu Tobu Girl (Europe).gb" },
        { "name": "Gone", "path": "/storage/roms/gb/Gone.gb" },
    ] });
    api::without_paths(&mut v, &lib, &[]);
    assert_eq!(v["games"][0]["name"], "Tobu Tobu Girl (Europe)");
    assert!(v["games"][0]["id"].is_string() && v["games"][0].get("path").is_none());
    assert_eq!(v["games"][1]["name"], "Gone", "a game the library does not have keeps the log's name");
}

/// F16: a second cookie of the same name (set by another site for a wider domain) can't hide the real one.
#[test]
fn every_session_cookie_is_tried() {
    let r = rig("api-cookies", false);
    let (cookie, _) = r.pair();
    let both = format!("pocket_session={}; {cookie}", "A".repeat(43));
    assert_eq!(r.req("GET", "/api/library", &[("Cookie", &both)], b"").status, 200);
}

/// F18: an id that doesn't fit is refused, not cut down to another sticker's.
#[test]
fn ids_that_dont_fit_are_refused() {
    let r = rig("api-ids", false);
    let (cookie, csrf) = r.pair();
    let a = r.req("POST", "/api/stickers/delete", &[("Cookie", &cookie), ("X-CSRF-Token", &csrf)], br#"{"sticker":4294967297}"#);
    assert_eq!(a.status, 400);
    assert!(r.edits.lock().unwrap().is_empty(), "sticker 1 was not deleted");
}

/// F19: a picture whose pixels don't match its size is refused, not indexed (a panic would end pocket-ui).
#[test]
fn a_broken_picture_is_refused() {
    assert!(api::shrink_png(&crate::image::Image { w: 4, h: 4, px: vec![0; 3] }).is_none());
    assert!(api::shrink_png(&crate::image::Image { w: 0, h: 4, px: Vec::new() }).is_none());
    assert!(api::shrink_png(&crate::image::Image { w: 1, h: 1, px: vec![0] }).is_some());
}

// --- Hide on the Pocket (app_storage) ----------------------------------------------------------------------------

/// A game hidden on its card is not in the companion's list or its search; shown again (or after any change the App
/// reports, `Shared::library_changed`) it is back at the next request, not five minutes later.
#[test]
fn a_game_hidden_on_the_pocket_is_not_listed_and_comes_back_at_once() {
    let root = tmp("hidden");
    let roms = root.join("roms");
    for g in ["Alpha.gba", "Beta.gba"] {
        std::fs::create_dir_all(roms.join("gba")).unwrap();
        std::fs::write(roms.join("gba").join(g), b"x").unwrap();
    }
    let es = root.join("es_systems.cfg");
    std::fs::write(&es, format!("<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba</extension></system></systemList>", roms.join("gba").display())).unwrap();
    let data = root.join("data");
    let beta = roms.join("gba/Beta.gba");
    let mut meta = crate::library_meta::LibraryMeta::load(&data);
    meta.set_hidden(&beta, true).unwrap();
    let shared = Shared::new(data.clone(), es, Prefs::default());
    let ctx = Ctx::new(shared.clone(), Roots::under(&roms), Box::new(FakeTailnet { peers: Arc::new(Mutex::new(HashMap::new())) }));
    let names = |ctx: &Ctx| ctx.library().systems.iter().flat_map(|s| s.games.iter().map(|g| g.name.clone())).collect::<Vec<_>>();
    assert_eq!(names(&ctx), ["Alpha"]);
    meta.set_hidden(&beta, false).unwrap();
    assert_eq!(names(&ctx), ["Alpha"], "kept for five minutes while nothing is said");
    shared.library_changed();
    assert_eq!(names(&ctx), ["Alpha", "Beta"], "read again at once after the App says so");
}
