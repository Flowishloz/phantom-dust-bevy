//! `pd_convert sounds <Data/Sound dir> <out dir> [--clips] [--all-langs] [--wave bank:id,bank:id]`
//!
//! Every STH2 bank (`*.bin`) → one WAV per sound id, mixed the way
//! `pd_formats::sound::render` reads the events: `<out>/<bank>/<id>.wav`
//! (22,050 Hz stereo), plus `<id>_loop<k>.wav` for layers that loop forever
//! (projectile hums). `<out>/index.json` lists every sound with its length
//! and signal stats, and maps character models to their voice bank (from
//! `setup/charaset/charaset_en.dat` next to the Sound folder).
//! Japanese voice banks are skipped unless `--all-langs`.
//! `--clips` also writes the raw clips (`<bank>/clips/<n>.wav`, original rate).
//! `--wave` writes waveform images to `<out>/waves/<bank>_<id>.png`.

use anyhow::{Context, Result};
use pd_formats::sound;
use serde_json::{json, Map, Value};
use std::path::Path;

pub const RATE: u32 = 22050;
/// Highest peak a mixed sound may reach before it is scaled down.
const HEADROOM: f32 = 0.95;

pub fn run(args: &[String]) -> Result<()> {
    let (Some(dir), Some(out)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: pd_convert sounds <Data/Sound dir> <out dir> [--clips] [--all-langs] [--wave bank:id,...]");
    };
    let (dir, out) = (Path::new(dir), Path::new(out));
    let clips = args.iter().any(|a| a == "--clips");
    let all_langs = args.iter().any(|a| a == "--all-langs");
    let waves: Vec<(String, usize)> = args
        .iter()
        .position(|a| a == "--wave")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|p| p.rsplit_once(':').and_then(|(b, i)| Some((b.to_string(), i.parse().ok()?)))).collect())
        .unwrap_or_default();
    std::fs::create_dir_all(out)?;

    let mut files: Vec<_> = std::fs::read_dir(dir).context("reading Sound dir")?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "bin")).collect();
    files.sort();
    let mut banks = Map::new();
    let (mut total, mut suspicious) = (0usize, Vec::new());
    for path in files {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        if !all_langs && stem.ends_with("_ja") {
            continue;
        }
        let d = std::fs::read(&path)?;
        let bank = match sound::parse(&d) {
            Ok(b) => b,
            Err(e) => {
                println!("{stem}: skipped ({e})");
                continue;
            }
        };
        // "BossSE_LastBoss..bin": Windows folders can't end in a dot.
        let name = stem.trim_end_matches('.').to_string() + if stem.ends_with('.') { "_" } else { "" };
        let bdir = out.join(&name);
        std::fs::create_dir_all(&bdir)?;
        let mut entries = Map::new();
        let (mut n, mut secs, mut loops) = (0usize, 0f64, 0usize);
        for id in 0..bank.sounds.len() {
            let Some(mut r) = sound::render(&d, &bank, id, RATE) else { continue };
            // Layers summed at full level can pass full scale (the console's
            // mixer had headroom we don't know): scale down, never up.
            let peak = r.frames.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
            let trim = if peak > HEADROOM { HEADROOM / peak } else { 1.0 };
            r.frames.iter_mut().flatten().for_each(|v| *v *= trim);
            r.loops.iter_mut().flatten().flatten().for_each(|v| *v *= trim);
            std::fs::write(bdir.join(format!("{id}.wav")), sound::wav(RATE, 2, &sound::to_i16(&r.frames)))?;
            for (k, l) in r.loops.iter().enumerate() {
                std::fs::write(bdir.join(format!("{id}_loop{k}.wav")), sound::wav(RATE, 2, &sound::to_i16(l)))?;
            }
            let s = sound::stats(&r.frames, RATE);
            let ms = (r.frames.len() as f64 * 1000.0 / RATE as f64).round() as u64;
            if (s.rms < 1e-4 && !name.starts_with("BattleVo")) || s.clipped > 0 {
                suspicious.push(format!("{name}/{id}: rms {:.5} peak {:.3} clipped {}", s.rms, s.peak, s.clipped));
            }
            entries.insert(
                id.to_string(),
                json!({ "ms": ms, "loops": r.loops.len(), "trim": round3(trim), "rms": round3(s.rms), "peak": round3(s.peak), "zc": s.zero_cross_per_s.round(), "lag1": round3(s.lag1) }),
            );
            if waves.iter().any(|(b, i)| *b == name && *i == id) {
                let wdir = out.join("waves");
                std::fs::create_dir_all(&wdir)?;
                std::fs::write(wdir.join(format!("{name}_{id}.png")), sound::waveform_png(&r.frames, 900, 240)?)?;
            }
            n += 1;
            secs += ms as f64 / 1000.0;
            loops += r.loops.len();
        }
        if clips {
            let cdir = bdir.join("clips");
            std::fs::create_dir_all(&cdir)?;
            // Each clip's format comes from the first event that plays it.
            let mut done = vec![false; bank.clips.len()];
            for ev in bank.sounds.iter().flatten().flat_map(|s| s.tracks.iter().flatten()).flat_map(|t| &t.events) {
                let Some(c) = ev.clip.map(|c| c as usize) else { continue };
                if !ev.playable() || done.get(c) != Some(&false) {
                    continue;
                }
                done[c] = true;
                if let Some(pcm) = sound::clip_pcm(&d, &bank, c, ev.size) {
                    std::fs::write(cdir.join(format!("{c}.wav")), sound::wav(ev.sample_rate, ev.channels as u16, &pcm))?;
                }
            }
        }
        println!("{name}: {n} sounds ({loops} loop layers), {secs:.1} s, {} clips", bank.clips.len());
        total += n;
        banks.insert(name, Value::Object(entries));
    }

    // Character model → voice bank folder.
    let mut voices = Map::new();
    let charaset = dir.join("../setup/charaset/charaset_en.dat");
    match std::fs::read(&charaset).map_err(anyhow::Error::from).and_then(|b| sound::voice_banks(&b)) {
        Ok(list) => {
            for (model, bank) in list {
                let folder = format!("{}_en", bank.trim_end_matches(".bin"));
                if banks.contains_key(&folder) {
                    voices.insert(model, json!(folder));
                }
            }
        }
        Err(e) => println!("no voice map ({}: {e})", charaset.display()),
    }
    let index = json!({ "rate": RATE, "banks": banks, "voices": voices });
    std::fs::write(out.join("index.json"), serde_json::to_string_pretty(&index)?)?;
    println!("{total} sounds, {} voice banks mapped -> {}", voices.len(), out.display());
    if !suspicious.is_empty() {
        println!("{} silent or clipping:", suspicious.len());
        for s in suspicious.iter().take(40) {
            println!("  {s}");
        }
    }
    Ok(())
}

fn round3(v: f32) -> f64 {
    (v as f64 * 1000.0).round() / 1000.0
}
