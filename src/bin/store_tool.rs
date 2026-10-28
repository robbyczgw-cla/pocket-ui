//! store-tool: maintains the pocket-store catalog on the build machine.
//!   store-tool keygen <private-key-file>          new key pair; prints the public key for store.rs
//!   store-tool sign <private-key-file> <catalog>  writes <catalog>.sig
//!   store-tool hash <file>                        SHA-256 for a catalog entry
//!   store-tool update <key> <update.json> app|os|sd_image|install_zip <version> <file> [url] [notes]
//!                                                  adds a release to the update channel and signs it
//!   store-tool list <update.json>                 one line per file it names: kind, version, size, sha256, url
//!                                                  (tab-separated, urls resolved; scripts/release-channel.sh --check)
//!   store-tool verify <file> [<private-key-file>]  checks <file>.sig against the key pocket-ui has built in (or
//!                                                  the public half of the given key); exit 1 when it doesn't match

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") if args.len() == 3 => {
            let mut seed = [0u8; 32];
            std::fs::File::open("/dev/urandom")
                .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut seed))
                .expect("read /dev/urandom");
            let key = SigningKey::from_bytes(&seed);
            std::fs::write(&args[2], seed).expect("write private key");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&args[2], std::fs::Permissions::from_mode(0o600));
            }
            let public: Vec<String> = key
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|b| format!("0x{b:02x}"))
                .collect();
            println!("pub const PUBLIC_KEY: [u8; 32] = [{}];", public.join(", "));
        }
        Some("sign") if args.len() == 4 => sign(&args[2], std::path::Path::new(&args[3])),
        Some("hash") if args.len() == 3 => {
            let data = std::fs::read(&args[2]).expect("read file");
            println!(
                "{}  {} bytes",
                pocket_ui::store::sha256_hex(&data),
                data.len()
            );
        }
        // Puts a release into update.json and signs it again.
        Some("update") if (7..=9).contains(&args.len()) && matches!(args[4].as_str(), "app" | "os" | "sd_image" | "install_zip") => {
            let manifest_path = std::path::Path::new(&args[3]);
            let mut manifest: pocket_ui::update::Manifest =
                std::fs::read(manifest_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            manifest.format = 1;
            let file = std::path::Path::new(&args[6]);
            let release = pocket_ui::update::Release {
                version: args[5].clone(),
                url: args.get(7).cloned().unwrap_or_else(|| file.file_name().unwrap().to_string_lossy().into_owned()),
                sha256: pocket_ui::store::sha256_file(file).expect("read release file"),
                size: std::fs::metadata(file).expect("release file").len(),
                notes: args.get(8).cloned().unwrap_or_default(),
            };
            println!("{} {} {}  {} bytes", args[4], release.version, release.sha256, release.size);
            match args[4].as_str() {
                "app" => manifest.app = Some(release),
                "os" => manifest.os = Some(release),
                "sd_image" => manifest.sd_image = Some(release),
                _ => manifest.install_zip = Some(release),
            }
            std::fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).expect("write update.json");
            sign(&args[2], manifest_path);
        }
        Some("list") if args.len() == 3 => {
            let manifest: pocket_ui::update::Manifest =
                serde_json::from_slice(&std::fs::read(&args[2]).expect("read update.json")).expect("parse update.json");
            let entries = [("app", &manifest.app), ("os", &manifest.os), ("sd_image", &manifest.sd_image), ("install_zip", &manifest.install_zip)];
            for (kind, rel) in entries {
                if let Some(r) = rel {
                    println!("{kind}\t{}\t{}\t{}\t{}", r.version, r.size, r.sha256, pocket_ui::update::resolve(&args[2], &r.url));
                }
            }
        }
        Some("verify") if (3..=4).contains(&args.len()) => {
            let public = match args.get(3) {
                Some(key_file) => SigningKey::from_bytes(&read_seed(key_file)).verifying_key().to_bytes(),
                None => pocket_ui::store::PUBLIC_KEY,
            };
            let data = std::fs::read(&args[2]).expect("read file");
            let sig = std::fs::read_to_string(format!("{}.sig", args[2])).unwrap_or_default();
            match pocket_ui::store::verify_signed(&data, &sig, &public) {
                Ok(()) => println!("verified {}", args[2]),
                Err(e) => {
                    let whose = if args.len() == 4 { "the given key" } else { "the key built into pocket-ui (store::PUBLIC_KEY)" };
                    eprintln!("{}: {e} with {whose}", args[2]);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!(
                "usage: store-tool keygen <key> | sign <key> <catalog.json> | hash <file> | verify <file> [<key>] | list <update.json>\n       \
                 store-tool update <key> <update.json> app|os|sd_image|install_zip <version> <file> [url] [notes]"
            );
            std::process::exit(2);
        }
    }
}

/// Writes `<file>.sig`: the base64 Ed25519 signature of the file's exact bytes.
fn sign(key_file: &str, file: &std::path::Path) {
    let seed = read_seed(key_file);
    let data = std::fs::read(file).expect("read file to sign");
    let sig = SigningKey::from_bytes(&seed).sign(&data);
    std::fs::write(
        format!("{}.sig", file.display()),
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()),
    )
    .expect("write signature");
    println!("signed {}", file.display());
}

/// The 32-byte private key file that `keygen` writes.
fn read_seed(key_file: &str) -> [u8; 32] {
    std::fs::read(key_file).expect("read private key").try_into().expect("32-byte key file")
}
