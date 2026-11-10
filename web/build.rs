//! The browser demo's build script: the root build.rs (clock stamp, libretro log shim, rcheevos) with the crate root's
//! paths (`../src`, `../vendor`). cc uses emcc for wasm32-unknown-emscripten.
fn main() {
    // The browser demo shows a few systems' games (web/demo/storage/roms): their part of the offline game facts is
    // enough, and the rest of assets/gamedb.bin would be a megabyte more to download (src/gamedb.rs reads this one).
    println!("cargo:rerun-if-changed=../assets/gamedb.bin");
    println!("cargo:rerun-if-changed=demo/storage/roms");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("gamedb-web.bin");
    std::fs::write(&out, gamedb_for_demo(&std::fs::read("../assets/gamedb.bin").expect("assets/gamedb.bin"))).unwrap();
    println!("cargo:rustc-env=POCKETUI_WEB_GAMEDB={}", out.display());
    // When this was built, for the wrong-clock check (src/clock_check.rs): a clock before it is wrong. Set when this
    // script runs (a fresh build, or after build.rs changed); a later incremental build keeps the older date, which
    // only makes the check more forgiving, never wrong.
    println!("cargo:rerun-if-changed=build.rs");
    let built = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    println!("cargo:rustc-env=POCKETUI_BUILD_UNIX={built}");
    println!("cargo:rerun-if-changed=src/log_shim.c");
    cc::Build::new().file("../src/log_shim.c").warnings(true).compile("pocket_log_shim");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") { return; }

    // rcheevos: the client (rc_client), the server API (rapi), the achievement runtime, hashing (rhash) and the libretro
    // memory-map helpers. 3DS decryption is left out (RC_HASH_NO_ENCRYPTED): pocket-core hosts no 3DS.
    let rc = "../vendor/rcheevos";
    println!("cargo:rerun-if-changed={rc}");
    println!("cargo:rerun-if-changed=src/rc_shim.c");
    let mut files: Vec<String> = ["rc_client.c", "rc_compat.c", "rc_util.c", "rc_version.c", "rc_libretro.c"]
        .iter()
        .map(|f| format!("{rc}/src/{f}"))
        .collect();
    for dir in ["rapi", "rcheevos", "rhash"] {
        let mut in_dir: Vec<String> = std::fs::read_dir(format!("{rc}/src/{dir}"))
            .expect("vendor/rcheevos")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "c"))
            .filter(|p| !p.ends_with("hash_encrypted.c") && !p.ends_with("aes.c"))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        in_dir.sort();
        files.extend(in_dir);
    }
    cc::Build::new()
        .files(&files)
        .file("../src/rc_shim.c")
        .include(format!("{rc}/include"))
        .include(format!("{rc}/src"))
        .include(format!("{rc}/libretro"))
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        // Upstream code: its warnings are upstream's business; ours (the shim) compile in the same unit list.
        .warnings(false)
        .compile("pocket_rcheevos");
}

/// assets/gamedb.bin with only the systems that have a folder in web/demo/storage/roms (all of them when there is none).
/// The format is src/gamedb.rs's: "PGDB1", one "<system> <offset> <length>" line per system, a blank line, the streams.
fn gamedb_for_demo(db: &[u8]) -> Vec<u8> {
    let keep: Vec<String> = std::fs::read_dir("demo/storage/roms")
        .map(|d| d.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    let end = db.windows(2).position(|w| w == b"\n\n").expect("gamedb header");
    let header = std::str::from_utf8(&db[..end]).expect("gamedb header");
    let mut lines = header.lines();
    let (mut head, mut body) = (format!("{}\n", lines.next().unwrap()), Vec::new());
    for line in lines {
        let parts: Vec<&str> = line.split(' ').collect();
        let (off, len): (usize, usize) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
        if keep.is_empty() || keep.iter().any(|k| k == parts[0]) {
            head.push_str(&format!("{} {} {}\n", parts[0], body.len(), len));
            body.extend_from_slice(&db[end + 2 + off..end + 2 + off + len]);
        }
    }
    let mut out = head.into_bytes();
    out.push(b'\n');
    out.extend(body);
    out
}
