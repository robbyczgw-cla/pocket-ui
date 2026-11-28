use super::*;

/// A manual of scanned pages (`w`, `h`, colour each), as other tests want one (`screenscraper::manual`).
pub(crate) fn scans(pages: &[(u16, u16, [u8; 3])]) -> Vec<u8> {
    let n = pages.len() as u32;
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
    let mut objs = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes()),
    ];
    for (i, &(w, h, rgb)) in pages.iter().enumerate() {
        let (pg, im) = (3 + 2 * i as u32, 4 + 2 * i as u32);
        objs.push((pg, page(2, &format!("/Resources << /XObject << /Im {im} 0 R >> >>"), 2)));
        objs.push((im, image(&jpeg(w, h, rgb), w, h)));
    }
    classic(&objs, 1)
}

/// A small real JPEG (`w` x `h`, one colour).
fn jpeg(w: u16, h: u16, rgb: [u8; 3]) -> Vec<u8> {
    let px: Vec<u8> = (0..w as usize * h as usize).flat_map(|_| rgb).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 80).encode(&px, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    out
}

/// Builds a PDF from numbered objects (`(number, bytes between "obj" and "endobj")`), with a classic table.
fn classic(objs: &[(u32, Vec<u8>)], root: u32) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs = Vec::new();
    for (n, body) in objs {
        offs.push((*n, out.len()));
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let size = objs.iter().map(|(n, _)| n + 1).max().unwrap();
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for n in 1..size {
        match offs.iter().find(|(m, _)| *m == n) {
            Some((_, o)) => out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(format!("trailer\n<< /Size {size} /Root {root} 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    out
}

fn image(j: &[u8], w: u16, h: u16) -> Vec<u8> {
    let mut b = format!(
        "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        j.len()
    )
    .into_bytes();
    b.extend_from_slice(j);
    b.extend_from_slice(b"\nendstream");
    b
}

fn page(parent: u32, res: &str, contents: u32) -> Vec<u8> {
    format!("<< /Type /Page /Parent {parent} 0 R /MediaBox [0 0 552 810] {res} /Contents {contents} 0 R >>").into_bytes()
}

fn content(text: &str) -> Vec<u8> {
    format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()).into_bytes()
}

/// Two scanned pages and one text page (as a manual with a typed last page would be).
fn two_scans_and_text() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (a, b) = (jpeg(8, 12, [200, 30, 30]), jpeg(12, 8, [30, 30, 200]));
    let pdf = classic(
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
            (2, b"<< /Type /Pages /Kids [3 0 R 4 0 R 9 0 R] /Count 3 >>".to_vec()),
            (3, page(2, "/Resources << /XObject << /Im0 5 0 R >> >>", 7)),
            // Resources inherited from an intermediate node, Length as a reference.
            (4, b"<< /Type /Pages /Parent 2 0 R /Kids [10 0 R] /Count 1 /Resources << /XObject << /Im1 6 0 R >> >> >>".to_vec()),
            (5, image(&a, 8, 12)),
            (6, {
                let mut s = b"<< /Subtype /Image /Width 12 /Height 8 /Filter [/DCTDecode] /Length 11 0 R >>\nstream\n".to_vec();
                s.extend_from_slice(&b);
                s.extend_from_slice(b"\nendstream");
                s
            }),
            (7, content("q 552 0 0 810 0 0 cm /Im0 Do Q")),
            (8, content("BT /F1 12 Tf (Hello) Tj ET")),
            (9, page(2, "/Resources << /Font << /F1 12 0 R >> >>", 8)),
            (10, page(4, "", 7)),
            (11, format!("{}", b.len()).into_bytes()),
            (12, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec()),
        ],
        1,
    );
    (pdf, a, b)
}

#[test]
fn every_scanned_page_comes_back_as_its_jpeg_in_reading_order() {
    let (pdf, a, b) = two_scans_and_text();
    let p = pages(&pdf).expect("a readable manual");
    assert_eq!(p.len(), 3);
    let Page::Jpeg { start, len, width, height } = p[0].clone() else { panic!("page 1 is a scan: {:?}", p[0]) };
    assert_eq!((&pdf[start..start + len], width, height), (&a[..], 8, 12));
    let Page::Jpeg { start, len, width, height } = p[1].clone() else { panic!("page 2 is a scan: {:?}", p[1]) };
    assert_eq!((&pdf[start..start + len], width, height), (&b[..], 12, 8), "inherited resources, indirect length");
    assert_eq!(p[2], Page::Other, "a text page is shown as 'can't show'");
}

#[test]
fn a_broken_table_still_finds_the_pages() {
    let (mut pdf, a, _) = two_scans_and_text();
    let at = pdf.windows(9).rposition(|w| w == b"startxref").unwrap();
    pdf.truncate(at);
    pdf.extend_from_slice(b"startxref\n999999999\n%%EOF\n");
    let p = pages(&pdf).expect("found by looking for objects");
    assert_eq!(p.len(), 3);
    let Page::Jpeg { start, len, .. } = p[0] else { panic!() };
    assert_eq!(&pdf[start..start + len], &a[..]);
}

/// The layout ScreenScraper's manuals have (Ghostscript, PDF 1.7): page tree in a compressed object stream, a
/// cross-reference stream with `/W [1 3 1]` and `/Predictor 12`, the page pictures as plain objects.
#[test]
fn a_ghostscript_layout_with_object_and_xref_streams_is_read() {
    let pics = [jpeg(16, 24, [10, 200, 10]), jpeg(24, 16, [250, 250, 0])];
    // Objects 1..=4 go into the object stream 5; pictures are 6, 7.
    let inner = [
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string()),
        (3, "<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im6 6 0 R >> >> >>".to_string()),
        (4, "<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im7 7 0 R >> >> >>".to_string()),
    ];
    let mut head = String::new();
    let mut body = String::new();
    for (n, s) in &inner {
        head.push_str(&format!("{n} {} ", body.len()));
        body.push_str(s);
        body.push(' ');
    }
    let objstm = format!("{head}{body}");
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(objstm.as_bytes(), 6);

    let mut out = b"%PDF-1.7\n%\xC7\xEC\x8F\xA2\n".to_vec();
    let mut at = std::collections::BTreeMap::new();
    at.insert(5u32, out.len());
    out.extend_from_slice(
        format!("5 0 obj\n<< /Type /ObjStm /Length {} /Filter /FlateDecode /N 4 /First {} >>\nstream\n", packed.len(), head.len())
            .as_bytes(),
    );
    out.extend_from_slice(&packed);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    for (i, j) in pics.iter().enumerate() {
        let n = 6 + i as u32;
        at.insert(n, out.len());
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        let (w, h) = if i == 0 { (16, 24) } else { (24, 16) };
        out.extend_from_slice(&image(j, w, h));
        out.extend_from_slice(b"\nendobj\n");
    }
    // Cross-reference stream (object 8), rows: type, 3-byte field 2, 1-byte field 3; PNG "Up" predictor.
    let xref_at = out.len();
    at.insert(8, xref_at);
    let mut rows: Vec<[u8; 5]> = vec![[0, 0, 0, 0, 0xff]];
    for n in 1..=8u32 {
        let r = if n <= 4 {
            [2, 0, 0, 5, (n - 1) as u8]
        } else {
            let o = at[&n] as u32;
            [1, (o >> 16) as u8, (o >> 8) as u8, o as u8, 0]
        };
        rows.push(r);
    }
    let mut pred = Vec::new();
    let mut prev = [0u8; 5];
    for r in &rows {
        pred.push(2u8);
        for x in 0..5 {
            pred.push(r[x].wrapping_sub(prev[x]));
        }
        prev = *r;
    }
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&pred, 6);
    out.extend_from_slice(
        format!(
            "8 0 obj\n<< /Type /XRef /Length {} /Filter /FlateDecode /DecodeParms << /Columns 5 /Predictor 12 >> /W [ 1 3 1 ] /Root 1 0 R /Size 9 >>\nstream\n",
            packed.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&packed);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());

    let p = pages(&out).expect("readable");
    assert_eq!(p.len(), 2);
    for (i, page) in p.iter().enumerate() {
        let Page::Jpeg { start, len, .. } = *page else { panic!("page {i}: {page:?}") };
        assert_eq!(&out[start..start + len], &pics[i][..]);
    }
}

#[test]
fn something_else_than_a_pdf_or_a_cut_file_gives_nothing_and_never_panics() {
    assert_eq!(pages(b""), None);
    assert_eq!(pages(b"<html>not found</html>"), None);
    let (pdf, ..) = two_scans_and_text();
    for cut in (0..pdf.len()).step_by(37) {
        let _ = pages(&pdf[..cut]);
    }
}

/// The real manuals of the study (not in the repository: ScreenScraper's media, CC BY-NC-SA). Run with
/// `POCKETUI_MANUAL_SAMPLES=<dir of PDFs> cargo test --lib manual_pdf -- --ignored` (and `POCKETUI_MANUAL_OUT`, below).
#[test]
#[ignore]
fn real_manuals_every_page_is_a_jpeg_that_decodes() {
    let dir = std::env::var("POCKETUI_MANUAL_SAMPLES").expect("POCKETUI_MANUAL_SAMPLES");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.extension().is_none_or(|x| x != "pdf") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let t = std::time::Instant::now();
        let p = pages(&bytes).unwrap_or_else(|| panic!("{} unreadable", path.display()));
        let took = t.elapsed();
        for (i, page) in p.iter().enumerate() {
            let Page::Jpeg { start, len, width, height } = *page else { panic!("{} page {i}: {page:?}", path.display()) };
            let mut d = jpeg_decoder::Decoder::new(&bytes[start..start + len]);
            d.read_info().unwrap();
            let info = d.info().unwrap();
            assert_eq!((info.width as u32, info.height as u32), (width, height), "{} page {i}", path.display());
        }
        eprintln!("{}: {} pages in {took:?}", path.display(), p.len());
        // POCKETUI_MANUAL_OUT=<dir>: the pages kept as the program keeps them (`screenscraper::manual::save_pages`), for
        // snapshots of real manuals.
        if let Ok(out) = std::env::var("POCKETUI_MANUAL_OUT") {
            let dir = std::path::Path::new(&out).join("manuals/kept").join(path.file_stem().unwrap()).join("us");
            assert_eq!(crate::screenscraper::manual::save_pages(&bytes, &dir).unwrap(), Some(p.len()));
        }
        n += 1;
    }
    assert!(n > 0);
}

/// A file from the network that is built to hurt: a zip bomb in an object stream, nesting without end, tables that
/// claim billions of objects, a stream longer than the file, predictor rows wider than anything real, encryption. Each
/// is "not readable" (None) at once, never a panic, a hang or gigabytes of memory.
#[test]
fn hostile_files_are_not_readable_quickly_and_never_panic() {
    let started = std::time::Instant::now();
    let (good, ..) = two_scans_and_text();
    // A zip bomb: an object stream that inflates to 100 MB of zeros (about 100 KB packed).
    let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&vec![b' '; 100 << 20], 9);
    let mut f = b"%PDF-1.7\n".to_vec();
    let at = f.len();
    f.extend_from_slice(format!("1 0 obj\n<< /Type /ObjStm /N 1 /First 4 /Filter /FlateDecode /Length {} >>\nstream\n", bomb.len()).as_bytes());
    f.extend_from_slice(&bomb);
    f.extend_from_slice(format!("\nendstream\nendobj\nxref\n0 2\n0000000000 65535 f \n{at:010} 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n").as_bytes());
    let x = f.windows(4).rposition(|w| w == b"xref").unwrap();
    f.extend_from_slice(format!("{x}\n%%EOF\n").as_bytes());
    assert_eq!(pages(&f), None, "zip bomb");

    // Nesting without end: 100 000 dictionaries and arrays in one another.
    let mut deep = b"%PDF-1.4\n1 0 obj\n".to_vec();
    deep.extend(std::iter::repeat_n(b"<< /K [ ".as_slice(), 100_000).flatten());
    deep.extend_from_slice(b"\nendobj\ntrailer << /Root 1 0 R >>\n%%EOF\n");
    assert_eq!(pages(&deep), None, "nesting");

    // A page tree that is its own child, and one 1000 levels deep.
    let lp = classic(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()), (2, b"<< /Type /Pages /Kids [2 0 R 2 0 R] /Count 2 >>".to_vec())], 1);
    assert_eq!(pages(&lp), None, "a loop in the page tree");
    let mut objs = vec![(1u32, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec())];
    for n in 2..1002u32 {
        objs.push((n, format!("<< /Type /Pages /Kids [{} 0 R] /Count 1 >>", n + 1).into_bytes()));
    }
    assert_eq!(pages(&classic(&objs, 1)), None, "1000 levels of page nodes");

    // Tables that claim too much: billions of objects, an index that wraps, a stream longer than the file, rows of
    // 1 GB, zero-wide rows.
    let mut huge = good.clone();
    let at = huge.windows(5).rposition(|w| w == b"xref\n").unwrap();
    huge.splice(at + 5..at + 5, b"4294967290 4294967295\n".iter().copied());
    let _ = pages(&huge);
    for w in ["[ 1 3 1 ] /Index [ 4294967295 10 ]", "[ 9 9 9 ]", "[ 0 0 0 ]", "[ 1 3 1 ] /DecodeParms << /Columns 1000000000 /Predictor 12 >>"] {
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&[1, 0, 0, 9, 0], 6);
        let mut x = b"%PDF-1.7\n".to_vec();
        let at = x.len();
        x.extend_from_slice(format!("1 0 obj\n<< /Type /XRef /W {w} /Size 2 /Root 1 0 R /Filter /FlateDecode /Length {} >>\nstream\n", packed.len()).as_bytes());
        x.extend_from_slice(&packed);
        x.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{at}\n%%EOF\n").as_bytes());
        assert_eq!(pages(&x), None, "{w}");
    }
    let long = classic(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()), (2, b"<< /Length 99999999999999999999 >>\nstream\nab\nendstream".to_vec())], 1);
    assert!(!pages(&long).is_some_and(|p| p.iter().any(|x| matches!(x, Page::Jpeg { .. }))), "a stream longer than the file: no picture");

    // Encrypted: not readable, even with JPEG-looking pictures in it.
    let mut enc = good.clone();
    let at = enc.windows(7).rposition(|w| w == b"trailer").unwrap();
    let close = at + enc[at..].windows(2).position(|w| w == b">>").unwrap();
    enc.splice(close..close, b" /Encrypt << /Filter /Standard >> ".iter().copied());
    assert_eq!(pages(&enc), None, "encrypted");

    assert!(started.elapsed() < std::time::Duration::from_secs(10), "all of it quickly ({:?})", started.elapsed());
}

/// Sol 26.11.2026 (P1): an object stream whose `/Length` is an object inside itself read that stream again for the
/// length, and again, until the stack overflowed and the program ended. Now at most a few hops, then `endstream`.
#[test]
fn an_indirect_length_that_leads_back_into_its_own_stream_ends() {
    // Object 5: an object stream holding object 6; its /Length is 6 0 R. The cross-reference stream (7) puts 6 in 5.
    let body = b"1 0 0 ";
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(body, 6);
    let mut out = b"%PDF-1.7\n".to_vec();
    let at5 = out.len();
    out.extend_from_slice(b"5 0 obj\n<< /Type /ObjStm /N 1 /First 4 /Filter /FlateDecode /Length 6 0 R >>\nstream\n");
    out.extend_from_slice(&packed);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    let at7 = out.len();
    let rows: Vec<[u8; 5]> = vec![
        [0, 0, 0, 0, 0xff],
        [0; 5], [0; 5], [0; 5], [0; 5],
        [1, (at5 >> 16) as u8, (at5 >> 8) as u8, at5 as u8, 0],
        [2, 0, 0, 5, 0],
        [1, (at7 >> 16) as u8, (at7 >> 8) as u8, at7 as u8, 0],
    ];
    let raw: Vec<u8> = rows.concat();
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    out.extend_from_slice(format!("7 0 obj\n<< /Type /XRef /W [ 1 3 1 ] /Size 8 /Root 6 0 R /Filter /FlateDecode /Length {} >>\nstream\n", packed.len()).as_bytes());
    out.extend_from_slice(&packed);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{at7}\n%%EOF\n").as_bytes());
    // In a thread with a small stack: an endless descent would overflow it at once.
    let r = std::thread::Builder::new().stack_size(256 * 1024).spawn(move || pages(&out).is_some()).unwrap().join();
    assert!(r.is_ok(), "no stack overflow");
}

/// Sol 26.11.2026 (P1): a cross-reference stream of one-byte rows could claim millions of objects; the count was checked
/// only after all of them were in the table. Now every one is counted before it goes in, and plain streams count
/// against the same budget as inflated ones.
#[test]
fn a_table_of_millions_of_compact_rows_stops_at_the_limit() {
    let rows = vec![1u8; 5_000_000];
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&rows, 6);
    let mut x = b"%PDF-1.7\n".to_vec();
    let at = x.len();
    x.extend_from_slice(format!("1 0 obj\n<< /Type /XRef /W [ 1 0 0 ] /Index [ 0 5000000 ] /Size 5000000 /Root 2 0 R /Filter /FlateDecode /Length {} >>\nstream\n", packed.len()).as_bytes());
    x.extend_from_slice(&packed);
    x.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{at}\n%%EOF\n").as_bytes());
    let started = std::time::Instant::now();
    assert_eq!(pages(&x), None);
    assert!(started.elapsed() < std::time::Duration::from_secs(3), "{:?}", started.elapsed());
}

/// Sol 26.11.2026 (P1): a dictionary of millions of entries, or one set of resources shared by every page, could take
/// memory out of all proportion to the file. Every value is counted against one budget before it is made, and the
/// resources a page inherits are shared, not copied per page.
#[test]
fn huge_dictionaries_and_shared_resources_stay_within_the_budget() {
    let started = std::time::Instant::now();
    // 2 million distinct entries in one resources dictionary, and an array of 5 million cheap values in the trailer.
    let mut big = String::with_capacity(32 << 20);
    big.push_str("<< ");
    for i in 0..2_000_000u32 {
        big.push_str(&format!("/a{i} 1 "));
    }
    big.push_str(">>");
    let pdf = classic(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()), (2, b"<< /Type /Pages /Kids [4 0 R] /Count 1 /Resources 3 0 R >>".to_vec()), (3, big.into_bytes()), (4, page(2, "", 2))], 1);
    assert_eq!(pages(&pdf), None, "a dictionary over the budget: not readable");
    let (mut wide, ..) = two_scans_and_text();
    let at = wide.windows(7).rposition(|w| w == b"trailer").unwrap();
    let close = at + wide[at..].windows(2).position(|w| w == b">>").unwrap();
    let mut arr = b" /Junk [".to_vec();
    arr.extend(std::iter::repeat_n(b" 0".as_slice(), 5_000_000).flatten());
    arr.extend_from_slice(b" ] ");
    wide.splice(close..close, arr);
    assert_eq!(pages(&wide), None, "an array over the budget: not readable");
    // 2000 pages sharing one resources dictionary of 20 000 entries: read, every page "other", quickly.
    let mut res = String::from("<< ");
    for i in 0..20_000u32 {
        res.push_str(&format!("/r{i} 1 "));
    }
    res.push_str(">>");
    let mut objs = vec![(1u32, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()), (3, res.into_bytes())];
    let kids: Vec<String> = (0..2000u32).map(|i| format!("{} 0 R", 10 + i)).collect();
    objs.push((2, format!("<< /Type /Pages /Kids [{}] /Count 2000 /Resources 3 0 R >>", kids.join(" ")).into_bytes()));
    for i in 0..2000u32 {
        objs.push((10 + i, page(2, "", 2)));
    }
    objs.sort_by_key(|(n, _)| *n);
    let p = pages(&classic(&objs, 1)).expect("readable, the pages say they can't be shown");
    assert_eq!(p.len(), 2000);
    assert!(p.iter().all(|x| *x == Page::Other));
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
}

/// Sol 26.11.2026 (P1, round 3 and 4): memory for parsed values is budgeted in bytes as they are made, the clock is
/// read inside the lexer's loop too, a name longer than the PDF standard's ends the reading before it is copied, and
/// no dictionary is copied deeply. Each hostile file is "not readable" at once, with what it parsed counted under
/// [`MAX_PARSED_BYTES`].
#[test]
fn parsed_values_stay_within_a_byte_budget() {
    let (good, ..) = two_scans_and_text();
    let with_trailer = |junk: &[u8]| {
        let mut f = good.clone();
        let at = f.windows(7).rposition(|w| w == b"trailer").unwrap();
        let close = at + f[at..].windows(2).position(|w| w == b">>").unwrap();
        f.splice(close..close, junk.iter().copied());
        f
    };
    let started = std::time::Instant::now();
    // 4 million array values in a giant trailer.
    let mut arr = b" /Junk [".to_vec();
    arr.extend(std::iter::repeat_n(b" 0".as_slice(), 4_000_000).flatten());
    arr.extend_from_slice(b" ] ");
    assert_eq!(pages(&with_trailer(&arr)), None, "4 million values");
    assert!(parsed_bytes() <= MAX_PARSED_BYTES, "{} bytes", parsed_bytes());
    // 40 000 dictionaries of 40 distinct keys each (under the per-container limit, over the byte budget).
    let mut many = b" /Many [".to_vec();
    for i in 0..40_000u32 {
        many.extend_from_slice(b" <<");
        for k in 0..40u32 {
            many.extend_from_slice(format!(" /k{i}x{k} 1").as_bytes());
        }
        many.extend_from_slice(b" >>");
    }
    many.extend_from_slice(b" ] ");
    assert_eq!(pages(&with_trailer(&many)), None, "many small dictionaries");
    assert!(parsed_bytes() <= MAX_PARSED_BYTES, "{} bytes", parsed_bytes());
    // A name of 20 MB of bytes that are not UTF-8 (a lossy copy would be three times as large).
    let mut name = b" /".to_vec();
    name.extend(std::iter::repeat_n(0xffu8, 20 << 20));
    name.extend_from_slice(b" 1 ");
    assert_eq!(pages(&with_trailer(&name)), None, "an endless name");
    assert!(parsed_bytes() < 1 << 20, "the name was not copied ({} bytes)", parsed_bytes());
    // A name just over the standard's 127 bytes ends the reading; 300 000 names of 127 bytes run out of bytes.
    let long = format!(" /{} 1 ", "n".repeat(128));
    assert_eq!(pages(&with_trailer(long.as_bytes())), None, "a name of 128 bytes");
    let mut names = b" /Names [".to_vec();
    for i in 0..300_000u32 {
        names.extend_from_slice(format!(" /{i:0>127}").as_bytes());
    }
    names.extend_from_slice(b" ] ");
    assert_eq!(pages(&with_trailer(&names)), None, "many long names");
    assert!(parsed_bytes() <= MAX_PARSED_BYTES, "{} bytes", parsed_bytes());
    assert!(started.elapsed() < std::time::Duration::from_secs(15), "{:?}", started.elapsed());
    // And the real thing still reads, far under the budget.
    assert_eq!(pages(&good).map(|p| p.len()), Some(3));
    assert!(parsed_bytes() < 1 << 20);
}
