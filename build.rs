//! The C in the crate: the libretro log callback (src/log_shim.c), which has to be C-variadic, and RetroAchievements'
//! rcheevos library (vendor/rcheevos, MIT, see vendor/rcheevos/UPSTREAM) with pocket-ui's glue to it (src/rc_shim.c).
//! `cc` uses the host's compiler, and zig's under `cargo zigbuild` (it sets CC for the target).
fn main() {
    // When this was built, for the wrong-clock check (src/clock_check.rs): a clock before it is wrong. Set when this
    // script runs (a fresh build, or after build.rs changed); a later incremental build keeps the older date, which
    // only makes the check more forgiving, never wrong.
    println!("cargo:rerun-if-changed=build.rs");
    let built = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    println!("cargo:rustc-env=POCKETUI_BUILD_UNIX={built}");
    println!("cargo:rerun-if-changed=src/log_shim.c");
    cc::Build::new().file("src/log_shim.c").warnings(true).compile("pocket_log_shim");

    // rcheevos: the client (rc_client), the server API (rapi), the achievement runtime, hashing (rhash) and the libretro
    // memory-map helpers. 3DS decryption is left out (RC_HASH_NO_ENCRYPTED): pocket-core hosts no 3DS.
    let rc = "vendor/rcheevos";
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
        .file("src/rc_shim.c")
        .include(format!("{rc}/include"))
        .include(format!("{rc}/src"))
        .include(format!("{rc}/libretro"))
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        // Upstream code: its warnings are upstream's business; ours (the shim) compile in the same unit list.
        .warnings(false)
        .compile("pocket_rcheevos");
}
