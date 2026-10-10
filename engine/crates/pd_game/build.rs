//! With the `steam` feature: put `steam_api64.dll` (Valve's redistributable)
//! and `steam_appid.txt` (`480`) next to the built exe in `target/<profile>/`,
//! so `pd_game.exe` runs straight from there. Both are tracked in
//! `engine/vendor/steam/` (private repo; Valve's redistributable is fine
//! there), NOT in `engine/play`, which is Laurence's untracked game copy and
//! is refreshed only by the orchestrator (copy both files next to
//! `pd_game.exe` there). If the vendor copy is missing, the DLL that the
//! `steamworks-sys` crate ships is used instead.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_STEAM").is_none() || !std::env::var("TARGET").unwrap_or_default().contains("windows") {
        return;
    }
    // OUT_DIR = target/<triple?>/<profile>/build/pd_game-<hash>/out
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(build_dir) = out.ancestors().nth(2).map(Path::to_path_buf) else { return };
    let Some(profile_dir) = build_dir.parent().map(Path::to_path_buf) else { return };
    // steamworks-sys copies the SDK's dll into its own OUT_DIR (its build
    // script runs before ours); take the newest copy.
    let dll = std::fs::read_dir(&build_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("steamworks-sys-"))
        .map(|e| e.path().join("out").join("steam_api64.dll"))
        .filter(|p| p.is_file())
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
    let target_dll = profile_dir.join("steam_api64.dll");
    let vendor = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../vendor/steam");
    let vendor_dll = vendor.join("steam_api64.dll");
    let vendor_appid = vendor.join("steam_appid.txt");
    println!("cargo:rerun-if-changed={}", vendor_dll.display());
    println!("cargo:rerun-if-changed={}", vendor_appid.display());
    let dll = if vendor_dll.is_file() { Some(vendor_dll) } else { dll };
    match dll {
        Some(src) => {
            if std::fs::copy(&src, &target_dll).is_err() {
                println!("cargo:warning=could not copy steam_api64.dll to {}", profile_dir.display());
            }
        }
        None => println!("cargo:warning=steam_api64.dll not found under {}; copy it next to pd_game.exe by hand", build_dir.display()),
    }
    let appid = profile_dir.join("steam_appid.txt");
    if vendor_appid.is_file() {
        let _ = std::fs::copy(&vendor_appid, &appid);
    } else if !appid.exists() {
        let _ = std::fs::write(&appid, "480");
    }
    // Missing files count as changed, so a deleted copy is put back.
    println!("cargo:rerun-if-changed={}", target_dll.display());
    println!("cargo:rerun-if-changed={}", appid.display());
}
