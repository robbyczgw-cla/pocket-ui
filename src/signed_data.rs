//! Shared trust anchor and Ed25519/SHA-256 verification for stores and every platform update channel.
use base64::Engine;
use std::path::Path;
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Public half of the catalog signing key (`store-tool keygen`); the private half never leaves
/// the build machine.
pub const PUBLIC_KEY: [u8; 32] = [
    0xf1, 0x86, 0x0a, 0xf1, 0xb2, 0xf1, 0xb3, 0xc6, 0x0b, 0x24, 0x22, 0xcd, 0xfc, 0xa9, 0xf3, 0xc0,
    0xb8, 0x49, 0x52, 0x07, 0xf5, 0x11, 0x96, 0xe1, 0x75, 0x84, 0x65, 0x32, 0x4f, 0x83, 0xe0, 0x1f,
];

/// Checks that `data` was signed by `key` (base64 Ed25519 signature, as `store-tool sign` writes it).
pub fn verify_signed(data: &[u8], sig_b64: &str, key: &[u8; 32]) -> Result<(), String> {
    let key = VerifyingKey::from_bytes(key).map_err(|e| format!("bad public key: {e}"))?;
    let sig = base64::engine::general_purpose::STANDARD.decode(sig_b64.trim()).map_err(|_| "signature is not base64".to_string())?;
    let sig = Signature::from_slice(&sig).map_err(|_| "signature has the wrong length".to_string())?;
    key.verify(data, &sig).map_err(|_| "signature doesn't match".to_string())
}


pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file read in chunks, for downloads too big to hold in memory (OS images).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}


