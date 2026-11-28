//! The real manuals against a fake server: which manual is asked for, where its pages land, what an empty or broken
//! answer leaves (nothing half-written, no row that leads nowhere).

use super::*;
use crate::romm::Fetched as F;
use crate::testutil::TestDir;
use std::sync::Mutex;

const BASE: &str = "https://api.screenscraper.test/api2";

fn m(kind: &str, region: &str) -> Media {
    Media { kind: kind.into(), region: region.into(), format: "pdf".into() }
}

#[test]
fn the_players_language_comes_first_then_the_files_region_then_any() {
    let medias = [m("box-2D", "de"), m("manuel", "us"), m("manuel", "de"), m("manuel", "eu"), m("manuel", "jp")];
    assert_eq!(pick(&medias, "de", &[], "Game (Europe)", &|_| false).unwrap().region, "de");
    assert_eq!(pick(&medias, "en", &[], "Game (Europe)", &|_| false).unwrap().region, "eu", "the file's region");
    assert_eq!(pick(&medias, "en", &[], "Game (USA)", &|_| false).unwrap().region, "us");
    assert_eq!(pick(&medias, "es", &[], "Game (Japan)", &|_| false).unwrap().region, "jp", "no Spanish one: the file's");
    assert_eq!(pick(&[m("manuel", "fr")], "en", &[], "Game", &|_| false).unwrap().region, "fr", "any, rather than none");
    assert_eq!(pick(&[m("box-2D", "us")], "en", &[], "Game", &|_| false), None, "no manual: none");
}

struct Pdf {
    asked: Mutex<Vec<String>>,
    answer: Mutex<Vec<(u16, Vec<u8>)>>,
}

impl Pdf {
    fn new(answers: Vec<(u16, Vec<u8>)>) -> Pdf {
        Pdf { asked: Mutex::new(Vec::new()), answer: Mutex::new(answers) }
    }
}

impl Net for Pdf {
    fn call(&self, _: &str, url: &str, _: Option<&str>, _: Option<&str>) -> Answer {
        panic!("no API call for a manual: {}", url.split('?').next().unwrap());
    }
    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, _: &AtomicBool, _: &(dyn Fn(u64) + Sync)) -> F {
        assert_eq!((token, size), ("", MAX_MANUAL_BYTES));
        self.asked.lock().unwrap().push(url.to_string());
        let (status, body) = self.answer.lock().unwrap().remove(0);
        if status == 200 {
            std::fs::write(part, body).unwrap();
            F::Done
        } else {
            F::Status(status)
        }
    }
}

struct NoWait;
impl Clock for NoWait {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
    fn wait(&self, _: Duration, stop: &dyn Fn() -> bool) -> bool {
        !stop()
    }
}

fn dev() -> Dev {
    Dev { id: "devid-test".into(), password: "pw".into(), softname: "pocket-ui-test".into() }
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walk(dir).into_iter().map(|p| p.strip_prefix(dir).unwrap().display().to_string()).collect();
    v.sort();
    v
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
fn a_manual_is_asked_once_and_kept_as_its_pages() {
    let t = TestDir::new("manual-kept");
    let data = t.join("data");
    let rom = t.join("roms/snes/Super Game (USA).sfc");
    let pdf = crate::manual_pdf::tests::scans(&[(40, 60, [200, 0, 0]), (60, 40, [0, 0, 200])]);
    let net = Pdf::new(vec![(200, pdf)]);
    let mut client = Client::new(&net, &NoWait, BASE, dev(), None).unwrap();
    client.keep_day(&data);
    assert_eq!(kept(&data, "snes", &rom, "us"), None, "nothing before");
    let dir = dir_of(&data, "snes", &rom, "us");
    let got = client.manual(4, 2144, &m("manuel", "us"), &dir, &|| false, &|_| {}).unwrap();
    assert_eq!(got, Fetched::Saved(2));
    let asked = net.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].starts_with(&format!("{BASE}/mediaManuelJeu.php?")), "the manual's endpoint");
    assert!(asked[0].contains("&systemeid=4&jeuid=2144&media=manuel%28us%29"), "{}", asked[0].split('?').nth(1).unwrap().split("&softname").nth(1).unwrap_or(""));
    let pages = kept(&data, "snes", &rom, "us").expect("kept");
    assert_eq!(pages.len(), 2);
    let Kept::Jpeg(p0, 40, 60) = &pages[0] else { panic!("{:?}", pages[0]) };
    let bytes = std::fs::read(p0).unwrap();
    let mut d = jpeg_decoder::Decoder::new(bytes.as_slice());
    let px = d.decode().unwrap();
    assert!(px[0] > 150 && px[2] < 60, "page 1 is the red scan");
    assert!(matches!(pages[1], Kept::Jpeg(_, 60, 40)));
    // Only the pages and their list stay: no PDF, no part.
    assert_eq!(files(&data.join(FOLDER)), ["snes/Super Game (USA)/us/001.jpg", "snes/Super Game (USA)/us/002.jpg", "snes/Super Game (USA)/us/pages.txt"]);
    assert_eq!(client.ledger.requests, 1, "counted in the day's ledger like a picture");
}

#[test]
fn no_manual_an_error_page_or_a_refusal_leave_nothing_on_the_card() {
    let t = TestDir::new("manual-none");
    let data = t.join("data");
    let rom = t.join("roms/gba/Game.gba");
    let dir = dir_of(&data, "gba", &rom, "us");
    for (answer, want) in [
        ((200, b"NOMEDIA".to_vec()), Ok(Fetched::Nothing)),
        ((404, Vec::new()), Ok(Fetched::Nothing)),
        ((200, b"<html>busy</html>".to_vec()), Ok(Fetched::NoAnswer)),
        ((430, Vec::new()), Err(Refusal::QuotaToday)),
    ] {
        let net = Pdf::new(vec![answer.clone()]);
        let mut client = Client::new(&net, &NoWait, BASE, dev(), None).unwrap();
    client.keep_day(&data);
        let got = client.manual(12, 1, &m("manuel", "us"), &dir, &|| false, &|_| {});
        assert_eq!(got, want, "{:?}", String::from_utf8_lossy(&answer.1));
        assert_eq!(kept(&data, "gba", &rom, "us"), None);
        assert!(files(&data.join(FOLDER)).is_empty(), "nothing left: {:?}", files(&data.join(FOLDER)));
    }
}

/// A PDF that is no scan (text pages, encrypted, broken): "can't be read here", marked, its row gone, nothing else left.
#[test]
fn an_unreadable_manual_is_marked_and_its_row_goes() {
    let t = TestDir::new("manual-unreadable");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game.sfc");
    let mut index = Index::default();
    let id = Identity { crc: 1, size: 1, alt: None, name: String::new() };
    index.set("Game", &id, Entry::Found { game: 9, system: 0, medias: vec![m("manuel", "us")], rom_regions: vec![], series: Series::default() });
    index.save(&data, "snes").unwrap();
    assert_eq!(listed(&data, "snes").len(), 1);
    let text = b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Resources << /Font << >> >> >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    let net = Pdf::new(vec![(200, text.to_vec())]);
    let mut client = Client::new(&net, &NoWait, BASE, dev(), None).unwrap();
    client.keep_day(&data);
    assert_eq!(client.manual(4, 9, &m("manuel", "us"), &dir_of(&data, "snes", &rom, "us"), &|| false, &|_| {}), Ok(Fetched::Unreadable));
    assert_eq!(kept(&data, "snes", &rom, "us"), None);
    assert!(listed(&data, "snes").is_empty(), "the row goes");
    assert_eq!(files(&data.join(FOLDER)), ["snes/Game/us.unreadable"], "only the mark");
}

/// Sol 26.11.2026 (P1): one picture used on every page was written out again for each page; a small file could fill the
/// card. Pages that together are more than the file are not readable, and nothing is left behind.
#[test]
fn one_picture_on_many_pages_is_not_written_again_and_again() {
    let t = TestDir::new("manual-reuse");
    let one = crate::manual_pdf::tests::scans(&[(200, 300, [9, 9, 9])]);
    // The same picture (object 4) as the page's on 500 pages.
    let kids: Vec<String> = (0..500).map(|_| "3 0 R".to_string()).collect();
    let text = String::from_utf8_lossy(&one).into_owned();
    let pdf_text = text.replacen("/Kids [3 0 R] /Count 1", &format!("/Kids [{}] /Count 500", kids.join(" ")), 1);
    let mut pdf = pdf_text.into_bytes();
    // The table's offsets moved: drop it, the reader finds the objects without it.
    let at = pdf.windows(4).rposition(|w| w == b"xref").unwrap();
    pdf.truncate(at);
    pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    let dir = t.join("out/Game/us");
    assert_eq!(save_pages(&pdf, &dir).unwrap(), None);
    assert!(files(&t.join("out")).is_empty(), "nothing written: {:?}", files(&t.join("out")));
}

/// Sol 26.11.2026 (P2): a 430 ends ScreenScraper's day in the ledger, as in the pictures' run: opening the manual again
/// sends nothing more today.
#[test]
fn a_430_ends_the_day_and_nothing_more_goes_today() {
    let t = TestDir::new("manual-430");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game.sfc");
    let net = Pdf::new(vec![(430, Vec::new())]);
    let mut client = Client::new(&net, &NoWait, BASE, dev(), None).unwrap();
    client.keep_day(&data);
    assert_eq!(client.manual(4, 1, &m("manuel", "us"), &dir_of(&data, "snes", &rom, "us"), &|| false, &|_| {}), Err(Refusal::QuotaToday));
    // A new client on the same card (the manual opened again): refused before anything goes.
    let net2 = Pdf::new(Vec::new());
    let mut again = Client::new(&net2, &NoWait, BASE, dev(), None).unwrap();
    again.keep_day(&data);
    assert!(again.manual(4, 1, &m("manuel", "us"), &dir_of(&data, "snes", &rom, "us"), &|| false, &|_| {}).is_err());
    assert!(net2.asked.lock().unwrap().is_empty(), "no request after the day ended");
}

#[test]
fn a_half_written_manual_is_never_read() {
    let t = TestDir::new("manual-half");
    let data = t.join("data");
    let rom = t.join("roms/psx/Game.chd");
    let dir = dir_of(&data, "psx", &rom, "us");
    // Pages without their list (the program stopped while it wrote them): not a manual.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("001.jpg"), b"\xff\xd8").unwrap();
    assert_eq!(kept(&data, "psx", &rom, "us"), None);
    // Written again whole: read.
    let pdf = crate::manual_pdf::tests::scans(&[(20, 20, [9, 9, 9])]);
    assert_eq!(save_pages(&pdf, &dir).unwrap(), Some(1));
    assert_eq!(kept(&data, "psx", &rom, "us").map(|p| p.len()), Some(1));
}

/// The two files this module reads (the list of a kept manual's pages, the downloaded PDF) through every fault: missing,
/// empty, cut, flipped bytes, not UTF-8. Never a panic; a broken list is no manual (fetched again) or a page that says
/// it can't be shown, never a page that points nowhere.
#[test]
fn file_source_rule_manual_pages_list_and_pdf() {
    let t = TestDir::new("manual-faults");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game.sfc");
    let dir = dir_of(&data, "snes", &rom, "us");
    std::fs::create_dir_all(&dir).unwrap();
    for (list, want) in [
        (&b""[..], None),
        (&b"jpg 300"[..], Some(vec![Kept::Other])),
        (&b"jpg 3x0 400\n"[..], Some(vec![Kept::Other])),
        (&b"\xff\xfe\x00jpg"[..], None),
        (&b"jpg 300 400\nother\n"[..], Some(vec![Kept::Jpeg(dir.join("001.jpg"), 300, 400), Kept::Other])),
    ] {
        std::fs::write(dir.join("pages.txt"), list).unwrap();
        assert_eq!(kept(&data, "snes", &rom, "us"), want, "{:?}", String::from_utf8_lossy(list));
    }
    // The PDF: every cut and a flipped byte in every 97th place, read as the download would be.
    let pdf = crate::manual_pdf::tests::scans(&[(30, 40, [200, 10, 10]), (40, 30, [10, 10, 200])]);
    let out = dir_of(&data, "snes", &t.join("roms/snes/Other.sfc"), "us");
    for i in (0..pdf.len()).step_by(97) {
        let mut flipped = pdf.clone();
        flipped[i] ^= 0x5a;
        let _ = save_pages(&flipped, &out);
        let _ = save_pages(&pdf[..i], &out);
    }
    assert_eq!(save_pages(&pdf, &out).unwrap(), Some(2), "the whole file still reads");
}

#[test]
fn the_index_says_whether_a_game_has_one_without_a_request() {
    let t = TestDir::new("manual-index");
    let data = t.join("data");
    let rom = t.join("roms/snes/Super Game (Europe).sfc");
    let other = t.join("roms/snes/Other.sfc");
    let mut index = Index::default();
    let id = |crc| Identity { crc, size: 10, alt: None, name: String::new() };
    index.set("Super Game (Europe)", &id(1), Entry::Found { game: 77, system: 0, medias: vec![m("box-2D", "eu"), m("manuel", "us"), m("manuel", "de")], rom_regions: vec![], series: Series::default() });
    index.set("Other", &id(2), Entry::Found { game: 78, system: 0, medias: vec![m("box-2D", "eu")], rom_regions: vec![], series: Series::default() });
    index.save(&data, "snes").unwrap();
    assert_eq!(available(&data, "snes", &rom, "de"), Some((4, 77, m("manuel", "de"))));
    assert_eq!(available(&data, "snes", &rom, "en").map(|a| a.2.region), Some("us".into()));
    assert_eq!(available(&data, "snes", &other, "en"), None, "no manual listed");
    assert_eq!(available(&data, "snes", &t.join("roms/snes/Unknown.sfc"), "en"), None, "not looked up");
    assert_eq!(available(&data, "gba", &rom, "en"), None, "another system's index");
    assert_eq!(listed(&data, "snes").into_iter().collect::<Vec<_>>(), ["Super Game (Europe)"]);
    assert!(listed(&data, "gba").is_empty());
    // Sol 26.11.2026 (P2): a broken manual of one region marks that region only; the other one is still offered, and
    // with every region marked the row goes.
    std::fs::create_dir_all(game_dir(&data, "snes", &rom)).unwrap();
    std::fs::write(unreadable_mark(&data, "snes", &rom, "de"), b"").unwrap();
    assert_eq!(available(&data, "snes", &rom, "de").map(|a| a.2.region), Some("us".into()), "the German one is broken: the US one");
    assert_eq!(listed(&data, "snes").len(), 1);
    std::fs::write(unreadable_mark(&data, "snes", &rom, "us"), b"").unwrap();
    assert_eq!(available(&data, "snes", &rom, "de"), None);
    assert!(listed(&data, "snes").is_empty(), "every region marked: no row");
}

/// Sol 26.11.2026 (P2): the cache belongs to the manual picked. A player who switches to German gets the German one;
/// once it is there the English one goes (one manual per game on the card). Stems that differ only after a dot, and
/// a stem that ends like a staging folder, never share or lose a folder.
#[test]
fn the_cache_follows_the_language_and_stems_with_dots_stay_apart() {
    let t = TestDir::new("manual-regions");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game.v1.sfc");
    let pdf = crate::manual_pdf::tests::scans(&[(20, 20, [9, 9, 9])]);
    assert_eq!(save_pages(&pdf, &dir_of(&data, "snes", &rom, "us")).unwrap(), Some(1));
    assert!(kept(&data, "snes", &rom, "us").is_some());
    assert_eq!(kept(&data, "snes", &rom, "de"), None, "the German one is not the English one");
    assert_eq!(save_pages(&pdf, &dir_of(&data, "snes", &rom, "de")).unwrap(), Some(1));
    assert!(kept(&data, "snes", &rom, "de").is_some());
    assert_eq!(kept(&data, "snes", &rom, "us"), None, "the old language went");
    // Other games: "Game.v2" and "Game.part" (a stem that looks like a staging folder) keep theirs.
    let v2 = t.join("roms/snes/Game.v2.sfc");
    let part = t.join("roms/snes/Game.part.sfc");
    save_pages(&pdf, &dir_of(&data, "snes", &v2, "us")).unwrap();
    save_pages(&pdf, &dir_of(&data, "snes", &part, "us")).unwrap();
    save_pages(&pdf, &dir_of(&data, "snes", &rom, "us")).unwrap();
    for r in [&v2, &part] {
        assert!(kept(&data, "snes", r, "us").is_some(), "{}", r.display());
    }
    assert!(files(&data).iter().all(|f| !f.contains("/.")), "no staging left: {:?}", files(&data));
}

/// Sol 26.11.2026 (P1): the region comes from the network and became a folder name; "../.." could have left the
/// manuals' folder, and the folder there would have been removed. Now a region is two or three lowercase letters or
/// "xx", and nothing outside the manuals' folder is ever removed.
#[test]
fn a_region_from_the_network_never_leaves_the_manuals_folder() {
    for (r, want) in [("us", "us"), ("wor", "wor"), ("../..", "xx"), ("/", "xx"), ("..\\", "xx"), ("", "xx"), ("de/..", "xx"), ("ü", "xx"), ("日本", "xx"), ("US", "xx"), ("abcd", "xx"), ("..", "xx")] {
        assert_eq!(region_id(r), want, "{r:?}");
    }
    let t = TestDir::new("manual-region-path");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game.sfc");
    // Something precious beside the manuals: it must stay whatever a region says.
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("Game.srm"), b"save").unwrap();
    let pdf = crate::manual_pdf::tests::scans(&[(20, 20, [9, 9, 9])]);
    let root = data.join(FOLDER);
    for r in ["../..", "/", "..\\", "", "../../../saves", "日本"] {
        let dir = dir_of(&data, "snes", &rom, r);
        assert!(dir.starts_with(&root) && dir.ends_with("xx"), "{r:?}: {}", dir.display());
        assert_eq!(save_pages(&pdf, &dir).unwrap(), Some(1), "{r:?}");
        assert!(unreadable_mark(&data, "snes", &rom, r).starts_with(&root));
    }
    assert_eq!(std::fs::read(saves.join("Game.srm")).unwrap(), b"save", "the save is untouched");
    // And a removal outside the manuals' folder is refused.
    assert!(remove_inside(&data, &saves).is_err());
    assert!(remove_inside(&data, &root).is_err(), "not the manuals' folder itself");
    assert!(saves.join("Game.srm").exists());
}

/// Opening: a manual on the card opens without a request (also offline); one that isn't says it needs Wi-Fi offline.
#[test]
fn opening_takes_the_card_first_and_needs_the_network_only_to_fetch() {
    let t = TestDir::new("manual-open");
    let data = t.join("data");
    let rom = t.join("roms/snes/Game (USA).sfc");
    let mut index = Index::default();
    index.set("Game (USA)", &Identity { crc: 1, size: 1, alt: None, name: String::new() }, Entry::Found { game: 5, system: 0, medias: vec![m("manuel", "us")], rom_regions: vec![], series: Series::default() });
    index.save(&data, "snes").unwrap();
    let net = Pdf::new(Vec::new());
    assert_eq!(open(&net, &data, "snes", &rom, "en", false, &|| false, &|_| {}), Opened::Offline);
    let pdf = crate::manual_pdf::tests::scans(&[(20, 20, [9, 9, 9]), (20, 20, [9, 9, 9])]);
    save_pages(&pdf, &dir_of(&data, "snes", &rom, "us")).unwrap();
    assert!(matches!(open(&net, &data, "snes", &rom, "en", false, &|| false, &|_| {}), Opened::Pages(p) if p.len() == 2));
    assert!(net.asked.lock().unwrap().is_empty(), "no request");
}
