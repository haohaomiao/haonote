//! Verify CI-generated update packages against the public key embedded in the app.
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{fs, path::Path};

fn decode(value: &str) -> Result<String> {
    Ok(String::from_utf8(STANDARD.decode(value.trim())?)?)
}

fn verify_directory(directory: &Path, key: &PublicKey) -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            count += verify_directory(&path, key)?;
        } else if entry.file_type()?.is_file() && path.extension().is_some_and(|ext| ext == "sig") {
            let signature = Signature::decode(&decode(&fs::read_to_string(&path)?)?)?;
            key.verify(&fs::read(path.with_extension(""))?, &signature, true)
                .with_context(|| {
                    format!("Signature does not match embedded key: {}", path.display())
                })?;
            count += 1;
        }
    }
    Ok(count)
}

fn main() -> Result<()> {
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .context("Missing public key")?;
    let key = PublicKey::decode(&decode(key)?)?;
    let directory = std::env::args()
        .nth(1)
        .context("Usage: verify_updates <bundle directory>")?;
    let count = verify_directory(Path::new(&directory), &key)?;
    ensure!(count > 0, "No update signatures found");
    println!("Verified {count} update package signatures against the embedded public key");
    Ok(())
}
