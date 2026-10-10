//! `pd_convert chars <Data dir> <cache dir>` -> `<cache>/chars.json`: each
//! playable character's English name, height and dodge move.
//!
//! The dodge comes from the character table (+0x36: 750 Jump, 751 ground
//! move). The table doesn't split the ground move into the guides' tumble
//! (no recovery) and cartwheel (recovery); their DASHEND_A clip does, in the
//! base `pcXXa.alr` file: 12 frames (0.4 s at 30 fps) for JD / Cuff Button
//! (the guide's "rollers"), 25 frames (0.83 s) for Edgar / pH / Sammah (its
//! "cartwheelers") -- confirmed for all five 751 characters, `CARTWHEEL_MIN_END`
//! sits cleanly between the two. Split at `CARTWHEEL_MIN_END`.
//!
//! **This split is cosmetic only** (PROGRAM-RE.md "Dodges and movement
//! skills", phase 3): in `pcXXa<colour>.alr`, the file the game actually
//! plays motions from, DASHEND_A / DASH_B / DASHEND_B collapse to 1-frame
//! stubs for every 751 character -- the whole ground dodge is one clip,
//! DASH_A (40 frames, 1.33 s locked), and the program times and locks it
//! identically for tumblers and cartwheelers; only DASH_A's content differs
//! (e.g. Edgar's clip passes through a handstand). So this label is still
//! useful for picking which look to render, but `pd_game::sim::Dodge`
//! (`Tumble` vs `Cartwheel`, different tick counts and speeds) does not
//! match the real game -- see this lane's Requests.

use anyhow::{Context, Result};
use pd_formats::{alr::Alr, anim, charaset};
use serde_json::json;
use std::path::Path;

/// A ground move whose base-file DASHEND_A lasts longer than this (s) is a
/// cartwheel. Read from the base `pcXXa.alr` (see the module doc): the
/// colour-variant file the game plays from has no DASHEND_A to split on.
const CARTWHEEL_MIN_END: f32 = 0.75;

pub fn run(args: &[String]) -> Result<()> {
    let (Some(data), Some(out)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: pd_convert chars <Data dir> <cache dir>");
    };
    let (data, out) = (Path::new(data), Path::new(out));
    let table = data.join("setup/charaset/charaset_en.dat");
    let bytes = std::fs::read(&table).with_context(|| table.display().to_string())?;
    let end_id = anim::ANIM_NAMES.iter().position(|n| *n == "DASHEND_A").context("DASHEND_A")?;
    let mut list = Vec::new();
    for e in charaset::entries(&bytes)? {
        // Playable versus characters only (their model folder under Player/).
        let alr = data.join("Player").join(&e.model).join(format!("{}.alr", e.model));
        if !e.model.starts_with("pc") || !alr.exists() || list.iter().any(|v: &serde_json::Value| v["id"] == e.model.as_str()) {
            continue;
        }
        // Clip lengths are in frames at 30 fps (FORMATS-ANIMATION.md
        // "Playback rate"; this was 24.0 and every reported length was 25%
        // too long, though it never flipped the tumble/cartwheel split --
        // 12 vs 25 frames stays either side of CARTWHEEL_MIN_END at both rates).
        let end = anim::read_animation(&Alr::load(&alr)?, end_id)?.map_or(0.0, |a| a.length()) / 30.0;
        let dodge = match e.dodge {
            charaset::DODGE_JUMP => "jump",
            charaset::DODGE_GROUND if end > CARTWHEEL_MIN_END => "cartwheel",
            charaset::DODGE_GROUND => "tumble",
            _ => "jump",
        };
        println!("{} {:?}: dodge {} ({dodge}, DASHEND_A {end:.2} s), height {}", e.model, e.name, e.dodge, e.height);
        list.push(json!({ "id": e.model, "name": e.name, "height": e.height, "dodge": dodge }));
    }
    std::fs::write(out.join("chars.json"), serde_json::to_string_pretty(&json!({ "chars": list }))?)?;
    println!("{} characters -> {}", list.len(), out.join("chars.json").display());
    Ok(())
}
