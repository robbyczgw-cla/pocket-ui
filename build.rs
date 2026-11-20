//! The C in the crate: the libretro log callback (src/log_shim.c), which has to be C-variadic, and RetroAchievements'
//! rcheevos library (vendor/rcheevos, MIT, see vendor/rcheevos/UPSTREAM) with pocket-ui's glue to it (src/rc_shim.c).
//! `cc` uses the host's compiler, and zig's under `cargo zigbuild` (it sets CC for the target).
//!
//! **Fresh by content, not by file time.** cargo decides whether to run this script by file times. Where one target
//! folder serves several source trees at the same path (the Air's build wrapper puts every worktree at `/src`), a C file
//! older than another tree's last build counted as built, and the library linked that tree's shim: a changed C
//! signature then crashed (13.11.2026, `pocket_rc_award_request`). So:
//! - the script also runs whenever a file of the library's Rust changes (the Rust that calls the C is rebuilt then
//!   anyway), and compiles the C only when its content or the compiler setup differs from what OUT_DIR holds
//!   ([`fresh`]); otherwise it only names the archives again, which costs a few milliseconds;
//! - it hands the library the hash of the two shims it compiled (`POCKETUI_C_SHIMS`), and the library refuses to compile
//!   when that isn't the hash of the shims it includes (`achievements::ffi`): a stale shim can't be linked silently.
use std::path::{Path, PathBuf};

/// FNV-1a over `bytes`, continuing from `h` (the same function `achievements::ffi` runs at compile time).
fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h = (h ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    h
}

const FNV_START: u64 = 0xcbf29ce484222325;

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every file under `dir`, sorted (so the hash doesn't depend on the order the folder lists them in).
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// What a compile of `inputs` depends on: their content and the compiler setup cargo and `cc` read.
fn key(inputs: &[PathBuf]) -> String {
    let mut h = FNV_START;
    for p in inputs {
        h = fnv(h, p.to_string_lossy().as_bytes());
        h = fnv(h, &std::fs::read(p).unwrap_or_default());
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    let target_ = target.replace('-', "_");
    for var in ["TARGET", "HOST", "OPT_LEVEL", "DEBUG", "PROFILE", "CC", "CFLAGS", "AR", "CRATE_CC_NO_DEFAULTS"] {
        h = fnv(h, var.as_bytes());
        h = fnv(h, std::env::var(var).unwrap_or_default().as_bytes());
        for v in [format!("{var}_{target}"), format!("{var}_{target_}"), format!("TARGET_{var}")] {
            h = fnv(h, std::env::var(&v).unwrap_or_default().as_bytes());
        }
    }
    // The Brick's hidden C ([`c_flags`]); the replacement header's content is among the inputs.
    h = fnv(h, &[std::env::var_os("CARGO_FEATURE_BRICK").is_some() as u8]);
    format!("{h:016x}")
}

/// Extra C flags, part of [`key`]. The Brick's pocket-core exports its symbols (`--export-dynamic`, for its `eglGetProcAddress`), so
/// the C in it is hidden: rcheevos (`rc_*`, `md5_*`) must never stand in for a core's own copy (PPSSPP exports its own rc_*).
/// `src/rc_export_brick.h` replaces rcheevos' rc_export.h, which marks its public functions visible; it is among the hashed inputs.
fn c_flags(root: &Path) -> Vec<String> {
    if std::env::var_os("CARGO_FEATURE_BRICK").is_none() {
        return Vec::new();
    }
    let header = std::fs::canonicalize(root.join("src/rc_export_brick.h")).expect("src/rc_export_brick.h");
    vec!["-fvisibility=hidden".into(), "-include".into(), header.to_string_lossy().into_owned()]
}

/// Whether OUT_DIR already holds `lib` built from exactly `inputs` (its key file says so): then cargo is told to link
/// it again and true is returned; otherwise the caller compiles it and [`remember`]s the key.
fn fresh(lib: &str, key: &str) -> bool {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let archive = out.join(format!("lib{lib}.a"));
    let kept = std::fs::read_to_string(out.join(format!("{lib}.key"))).unwrap_or_default();
    if kept.trim() == key && archive.is_file() {
        println!("cargo:rustc-link-lib=static={lib}");
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("c-sources: {lib} unchanged ({key}), not compiled again");
        return true;
    }
    false
}

fn remember(lib: &str, key: &str) {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join(format!("{lib}.key")), key).expect("OUT_DIR is writable");
    println!("c-sources: {lib} compiled ({key})");
}

/// The library's own Rust (not the programs in `bin`, which link it but don't change its C calls).
fn watch_library_rust(dir: &Path, bin: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p != bin {
                watch_library_rust(&p, bin);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
}

/// The crate root's own build (`root` "."); android/handheld builds the same with `root` "../.." (its build.rs).
#[allow(dead_code)]
fn main() {
    build(Path::new("."));
}

/// Everything the script does, with the repository's root at `root` (paths in cargo's lines are relative to the crate).
pub fn build(root: &Path) {
    let at = |p: &str| root.join(p);
    // When this was built, for the wrong-clock check (src/clock_check.rs): a clock before it is wrong. Set when this
    // script runs (a fresh build, or a change of the library or its C); an older date only makes the check more
    // forgiving, never wrong.
    println!("cargo:rerun-if-changed={}", at("build.rs").display());
    let built = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    println!("cargo:rustc-env=POCKETUI_BUILD_UNIX={built}");
    watch_library_rust(&at("src"), &at("src/bin"));
    // The Brick: pocket-core's own `eglGetProcAddress` (src/bin/pocket_core.rs) must be in its dynamic symbol table, so a core
    // linked against libEGL (PPSSPP's gl3stubInit) binds to it instead of the vendor's, which returns NULL for core GLES 3
    // functions (glGetStringi: SIGSEGV at the start of every PSP game, 20.11.2026). scripts/brick-make-card.sh checks it.
    // zig's linker has neither --export-dynamic-symbol nor a version script for a program: --export-dynamic, and the C in
    // the crate is compiled hidden ([`c_flags`]), so pocket-core's rcheevos never stands in for a core's own.
    if std::env::var_os("CARGO_FEATURE_BRICK").is_some() {
        println!("cargo:rustc-link-arg-bin=pocket-core=-Wl,--export-dynamic");
    }
    // The compiler setup is part of [`key`]; `cc` names these itself only when it runs, so they are named here always.
    let target = std::env::var("TARGET").unwrap_or_default();
    for var in ["CC", "CFLAGS", "AR", "CRATE_CC_NO_DEFAULTS"] {
        println!("cargo:rerun-if-env-changed={var}");
        println!("cargo:rerun-if-env-changed={var}_{target}");
        println!("cargo:rerun-if-env-changed={var}_{}", target.replace('-', "_"));
        println!("cargo:rerun-if-env-changed=TARGET_{var}");
    }

    // The shims the library declares in Rust: their hash goes to the library, which checks it at compile time.
    let (log_shim, rc_shim) = (at("src/log_shim.c"), at("src/rc_shim.c"));
    let shims = fnv(fnv(FNV_START, &read(&log_shim)), &read(&rc_shim));
    println!("cargo:rustc-env=POCKETUI_C_SHIMS={shims:016x}");

    println!("cargo:rerun-if-changed={}", log_shim.display());
    let log_key = key(std::slice::from_ref(&log_shim));
    if !fresh("pocket_log_shim", &log_key) {
        let mut b = cc::Build::new();
        c_flags(root).iter().for_each(|f| { b.flag(f); });
        b.file(&log_shim).warnings(true).compile("pocket_log_shim");
        remember("pocket_log_shim", &log_key);
    }

    // rcheevos (also for Android: RetroAchievements in the APK's own player, android/src/achievements.rs): the client (rc_client), the server API (rapi), the achievement runtime, hashing (rhash) and the libretro
    // memory-map helpers. 3DS decryption is left out (RC_HASH_NO_ENCRYPTED): pocket-core hosts no 3DS.
    let rc = at("vendor/rcheevos");
    let rc = rc.to_string_lossy();
    println!("cargo:rerun-if-changed={rc}");
    println!("cargo:rerun-if-changed={}", rc_shim.display());
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
    // Everything the compile reads: all of vendor/rcheevos (headers too) and the shim.
    let mut inputs = Vec::new();
    files_under(Path::new(rc.as_ref()), &mut inputs);
    inputs.sort();
    inputs.push(rc_shim.clone());
    inputs.push(at("src/rc_export_brick.h"));
    println!("cargo:rerun-if-changed={}", at("src/rc_export_brick.h").display());
    let rc_key = key(&inputs);
    if !fresh("pocket_rcheevos", &rc_key) {
        let mut b = cc::Build::new();
        c_flags(root).iter().for_each(|f| { b.flag(f); });
        b.files(&files)
            .file(&rc_shim)
            .include(format!("{rc}/include"))
            .include(format!("{rc}/src"))
            .include(format!("{rc}/libretro"))
            .define("RC_HASH_NO_ENCRYPTED", None)
            .define("RC_CLIENT_SUPPORTS_HASH", None)
            // Upstream code: its warnings are upstream's business; ours (the shim) compile in the same unit list.
            .warnings(false)
            .compile("pocket_rcheevos");
        remember("pocket_rcheevos", &rc_key);
    }
}
