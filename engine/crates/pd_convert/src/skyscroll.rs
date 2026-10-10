//! `pd_convert skyscroll <Map dir> <out dir>` -- writes `<out>/stNN.sky.json`
//! for every stage `.alr`: the per-material UV-scroll fields the original
//! feeds its scrolling-layer shaders (docs/GAME.md "The sky drifts").
//!
//! READ (`FUN_1402a07b0`, `0x1402a0a6c`): six int16 fields of the material
//! record at +0x3c, +0x3e, +0x40, +0x42, +0x44, +0x46, each times the global
//! frame counter (30 Hz program tick) times 1e-5, become vertex-shader
//! constants c90.xyzw and c91.xy: three (u, v) texture offsets, one per
//! texture slot (slot 0 = the colour layer). The file keys each draw by its
//! glb node name `m<model>_d<draw>` so the game can find the sky shell.
//! Only draws that are not shadow volumes (flags 0x2000, where +0x3c is a
//! float) and whose first pair is non-zero are written.

use anyhow::{Context, Result};
use pd_formats::alr::Alr;
use std::path::Path;

pub fn run(args: &[String]) -> Result<()> {
    let usage = "usage: pd_convert skyscroll <Map dir> <out dir>";
    let dir = Path::new(args.first().context(usage)?);
    let out = Path::new(args.get(1).context(usage)?);
    std::fs::create_dir_all(out)?;
    let mut names: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("alr"))).collect();
    names.sort();
    for p in names {
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        if stem.ends_with("cam") || !stem.starts_with("st") {
            continue;
        }
        let Ok(a) = Alr::load(&p) else { continue };
        let mut draws = serde_json::Map::new();
        for (mi, m) in a.models.iter().enumerate() {
            for (di, d) in m.draws.iter().enumerate() {
                let Some(mat) = m.materials.get(d.material_idx as usize) else { continue };
                if mat.flags & 0x2000 != 0 || (mat.scroll[0] == 0 && mat.scroll[1] == 0) {
                    continue;
                }
                draws.insert(format!("m{mi}_d{di}"), serde_json::json!(mat.scroll));
            }
        }
        let doc = serde_json::json!({ "counter_hz": 30.0, "scale": 1e-5, "draws": draws });
        std::fs::write(out.join(format!("{stem}.sky.json")), serde_json::to_string_pretty(&doc)?)?;
        println!("{stem}.sky.json: {} draws", doc["draws"].as_object().map_or(0, |o| o.len()));
    }
    Ok(())
}
