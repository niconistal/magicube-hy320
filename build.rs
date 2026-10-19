// Builds the on-device camera grabber (grab/, 32-bit ARM) and embeds it in the binary.
// If that target isn't installed, magicube still builds; only `see`, `--verify` and `map` won't work.
//   rustup target add armv7-unknown-linux-musleabihf
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=grab/src/main.rs");
    println!("cargo:rerun-if-changed=grab/Cargo.toml");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("mc-grab");
    let grab_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("grab");
    let target_dir = PathBuf::from(env::var("OUT_DIR").unwrap()).join("grab-target");
    let ok = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(&grab_dir)
        .env_remove("RUSTFLAGS").env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(["build", "--release", "--target-dir"]).arg(&target_dir)
        .status().map(|s| s.success()).unwrap_or(false);
    let built = target_dir.join("armv7-unknown-linux-musleabihf/release/mc-grab");
    if ok && built.exists() {
        std::fs::copy(&built, &out).unwrap();
    } else {
        println!("cargo:warning=camera grabber not built (rustup target add armv7-unknown-linux-musleabihf); `see`/`--verify`/`map` disabled");
        std::fs::write(&out, b"").unwrap();
    }
}
