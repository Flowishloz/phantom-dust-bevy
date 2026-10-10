//! `pd_convert stagefx <Map dir> <cache dir> [st06,st03,...]`
//!
//! Arena-object effects (docs/FORMATS-STAGES.md "Object effects"): for each
//! stage `.dat`, the per-type `EF00` records go to
//! `cache/fx/stage/stNN.objfx.json`, and every aux model they (or the game's
//! hit code) use is written as its own effect: `cache/fx/stage/stNN_sSS.glb`
//! + `.fx.json` (SS = the MD00 slot; same format as skill effects, the model
//! in event slot 0), so `effects.rs` can play it like a skill effect.

use anyhow::{Context, Result};
use pd_formats::{alr::Alr, effect, effect_archive::EffectArchive, gltf, stage_effects as se, stagedat::StageDat};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// MD00 slots always exported (the game's generic hit effects), besides the
/// ones the records name.
const ALWAYS: [usize; 4] = [se::MD00_HIT_IRON, se::MD00_HIT_STONE, se::MD00_HIT_SOIL, se::MD00_HIT_SMOKE];

/// `pd_convert stagefx --rate <Effect dir> <common.bin> <skills.json>`:
/// frame-rate probe. For every skill with a launch sound (s13), the start
/// delays of the sound's layers next to the frames of its cast effect
/// (slot 0 last frame, slot 0 spawner emitters). A layer that starts when
/// the charge-up ends pins frames to seconds.
fn rate_probe(fx_dir: &Path, bank: &Path, skills: &Path) -> Result<()> {
    let d = std::fs::read(bank)?;
    let b = pd_formats::sound::parse(&d)?;
    let sj: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(skills)?)?;
    let mut ratios = Vec::new();
    for s in sj["skills"].as_array().into_iter().flatten() {
        let (Some(text), Some(snd)) = (s["text_id"].as_u64(), s["raw"][13].as_i64()) else { continue };
        if !(300..400).contains(&snd) {
            continue;
        }
        let stem = fx_dir.join(format!("eft{text:04}"));
        let (Ok(eft), Ok(a)) = (std::fs::read(stem.with_extension("eft")), Alr::load(&stem.with_extension("alr"))) else { continue };
        let def = effect::parse_eft(&eft)?;
        let pieces = effect::piece_params(&a)?;
        let ems = effect::emitters(&a)?;
        let end0 = pieces.iter().filter(|p| def.slots[0].contains(&(p.model as u16))).map(|p| p.end).fold(0.0f32, f32::max);
        let spawn: Vec<f32> = ems.iter().filter(|e| def.slots[0].contains(&(e.params.model as u16)) && e.kind[0] == 0x90e7_fd3e).map(|e| e.params.start).collect();
        let Some(Some(sound)) = b.sounds.get(snd as usize) else { continue };
        let mut delays: Vec<i32> = sound.tracks.iter().flatten().flat_map(|t| &t.events).filter(|e| e.kind == 0 && e.clip.is_some()).map(|e| e.delay_ms).collect();
        delays.sort();
        delays.dedup();
        let last = delays.last().copied().unwrap_or(0);
        if last > 0 && end0 > 0.0 {
            ratios.push(end0 / (last as f32 / 1000.0));
        }
        println!("{:24} eft{text:04} sound {snd}: layer starts {delays:?} ms | cast effect ends frame {end0} | spawners at {spawn:?}", s["name"].as_str().unwrap_or("?"));
    }
    ratios.sort_by(f32::total_cmp);
    if !ratios.is_empty() {
        println!("cast-effect end frame / last layer start (s): median {:.1} over {} skills", ratios[ratios.len() / 2], ratios.len());
    }
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("--rate") {
        let [fx, bank, skills] = [1, 2, 3].map(|i| args.get(i).map(Path::new));
        let (Some(fx), Some(bank), Some(skills)) = (fx, bank, skills) else { anyhow::bail!("usage: pd_convert stagefx --rate <Effect dir> <common.bin> <skills.json>") };
        return rate_probe(fx, bank, skills);
    }
    let (Some(map), Some(cache)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: pd_convert stagefx <Map dir> <cache dir> [st06,st03,...]");
    };
    let only: Vec<String> = args.get(2).map(|s| s.split(',').map(String::from).collect()).unwrap_or_default();
    let out = Path::new(cache).join("fx").join("stage");
    std::fs::create_dir_all(&out)?;
    let mut ids: Vec<String> = std::fs::read_dir(map)?
        .filter_map(|e| e.ok()?.path().file_name()?.to_str().map(String::from))
        .filter(|n| n.starts_with("st") && n.ends_with(".dat"))
        .map(|n| n.trim_end_matches(".dat").to_string())
        .filter(|n| only.is_empty() || only.contains(n))
        .collect();
    ids.sort();
    for id in ids {
        if let Err(e) = stage(Path::new(map), &out, &id) {
            println!("{id}: skipped ({e:#})");
        }
    }
    Ok(())
}

fn stage(map: &Path, out: &Path, id: &str) -> Result<()> {
    let d = std::fs::read(map.join(format!("{id}.dat")))?;
    let dat = StageDat::parse(&d)?;
    if dat.types.is_empty() {
        anyhow::bail!("no object types");
    }
    let (types, stage_block) = se::parse_stage(&d, &dat.types)?;
    let alr = Alr::load(&map.join(format!("{id}.alr"))).context("stage alr")?;

    // Aux models follow everything else in the ALR, in MD00 order, NOUSE skipped.
    let used: Vec<(usize, &String)> = dat.aux_model_names.iter().enumerate().filter(|(_, n)| n.as_str() != "NOUSE").collect();
    let base = alr.models.len().checked_sub(used.len()).context("fewer models than MD00 names")?;
    let slot_model: BTreeMap<usize, (usize, String)> = used.iter().enumerate().map(|(k, (slot, name))| (*slot, (base + k, name.to_string()))).collect();

    // Which slots to export: named by records, plus the hit effects.
    let mut wanted: std::collections::BTreeSet<usize> = ALWAYS.into_iter().collect();
    let mut kinds: BTreeMap<u8, usize> = BTreeMap::new();
    for r in types.iter().flat_map(|t| t.main.iter().chain(&t.extra)) {
        *kinds.entry(r.kind).or_default() += 1;
        if let Some((s, _)) = r.debris() {
            wanted.insert(s);
        }
        if let Some((s, _)) = r.model() {
            wanted.insert(s);
        }
    }

    let params = effect::piece_params(&alr)?;
    let emitters = effect::emitters(&alr)?;
    let mut slots = serde_json::Map::new();
    for (slot, (model, name)) in &slot_model {
        let m = &alr.models[*model];
        let timed = params.iter().filter(|p| p.model == *model).count();
        let mut entry = json!({ "name": name, "model": model, "draws": m.draws.len(), "timed_pieces": timed, "file": null });
        if wanted.contains(slot) && !m.draws.is_empty() {
            let file = format!("stage/{id}_s{slot:02}");
            let skip: Vec<usize> = (0..alr.models.len()).filter(|i| i != model).collect();
            let (glb, _) = gltf::export_glb(&alr, &gltf::ExportOptions { apply_transforms: true, skip_models: skip, ..Default::default() })?;
            std::fs::write(out.join(format!("{id}_s{slot:02}.glb")), glb)?;
            let mut def_slots: [Vec<u16>; effect::SLOTS] = Default::default();
            def_slots[0] = vec![*model as u16];
            let def = effect::EffectDef { model_flags: Vec::new(), slots: def_slots };
            let own: Vec<_> = params.iter().filter(|p| p.model == *model).cloned().collect();
            let own_em: Vec<_> = emitters.iter().filter(|e| e.params.model == *model).cloned().collect();
            let mut j = effect::to_json(&def, &own, &own_em);
            // Untimed models (fragment sets): their draw nodes, for the game to scatter.
            j["fragments"] = json!((0..m.draws.len()).map(|di| format!("m{model}_d{di}")).collect::<Vec<_>>());
            // Timed draws in stage files are shader-drawn particle emitters:
            // few or no indices, no UVs, a texture and an emitter block (the
            // 0x13 words). Their textures go beside the glb.
            let mut sprites = Vec::new();
            for p in &own {
                let Some(draw) = m.draws.get(p.draw) else { continue };
                let tex = m.materials.get(draw.material_idx as usize).map(|mt| mt.texture_idx as usize);
                let png = format!("{id}_s{slot:02}_d{}.png", p.draw);
                let ok = tex.and_then(|t| alr.textures.get(t)).and_then(|e| pd_formats::texture::decode(&alr, e).and_then(|img| img.to_png()).ok()).map(|b| std::fs::write(out.join(&png), b)).transpose()?.is_some();
                let f = |k: usize| f32::from_bits(p.raw[k]);
                sprites.push(json!({
                    "node": format!("m{model}_d{}", p.draw),
                    "texture": ok.then(|| format!("fx/stage/{png}")),
                    "indices": draw.indices.len(),
                    "vertex_format": m.vertbufs.get(draw.vertex_buf as usize).map(|v| v.format),
                    "params": (0..12).map(f).collect::<Vec<_>>(),
                }));
            }
            j["sprites"] = json!(sprites);
            std::fs::write(out.join(format!("{id}_s{slot:02}.fx.json")), j.to_string())?;
            entry["file"] = json!(file);
        }
        slots.insert(slot.to_string(), entry);
    }

    let doc = json!({
        "stage": id,
        "coords": "record positions/rotations are in the object's own left-handed space (mirror z for the right-handed export)",
        "aux_base": base,
        "slots": slots,
        "types": dat.types.iter().zip(&types).enumerate().map(|(i, (t, e))| json!({
            "id": i,
            "name": t.name,
            "main": e.main.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
            "extra": e.extra.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "stage_block": stage_block.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
    });
    std::fs::write(out.join(format!("{id}.objfx.json")), serde_json::to_string_pretty(&doc)?)?;
    let exported = slots.values().filter(|v| !v["file"].is_null()).count();
    println!("{id}: {} types, {} records {kinds:?}, {} stage records, aux base {base}, {exported} aux effects written", types.len(), types.iter().map(|t| t.main.len() + t.extra.len()).sum::<usize>(), stage_block.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// `pd_convert effects`: skill effects (`Data/Effect/eftNNNN.alr`/`.eft`),
// docs/FORMATS-EFFECTS.md. The game loads every one of these from the packed
// archive (`e.i` + `a<k>.e`, `pd_formats::effect_archive`;
// docs/PROGRAM-RE.md "Packed effects"), not from the loose files, so that's
// our primary source; a loose file is used only as a fallback for a number
// that isn't in the archive (the shared/status effects 500-517ish that live
// outside the per-skill archive; reported as "loose-only" below).

/// Whether a 0x14 emitter's kind is one whose texture the game actually
/// draws as a sprite (trails, plus the sprite / puff particle emitters
/// FORMATS-EFFECTS.md documents): trail is an exact kind match; the sprite
/// and puff kinds vary in their second word (`a07ac685 8891f680` / `…fcc0`,
/// `90e7fd3e 760929ea` / `…e9`), so those match on the first word only.
/// Without these, `pd_convert effects` used to write only the trail
/// textures; the sprite / puff emitters' textures had to be written by a
/// one-off tool.
pub(crate) fn is_sprite_emitter(kind: [u32; 2]) -> bool {
    const SPRITE: u32 = 0xa07a_c685;
    const PUFF: u32 = 0x90e7_fd3e;
    // The kind is the emitter's packed name: BLADE01 / BLADE02 (4ca477d9 +
    // 0c350000 / 124f8000) are the swords' swing ribbons.
    const BLADE: u32 = 0x4ca4_77d9;
    // FLAIR01 (650e9921): a burst of flat rays (the strike's starburst).
    const FLAIR: u32 = 0x650e_9921;
    kind == pd_formats::effect::KIND_TRAIL || kind[0] == SPRITE || kind[0] == PUFF || kind[0] == BLADE || kind[0] == FLAIR
}

/// Whether a material shader samples more than the glb's own texture (slot 0):
/// every effect pixel shader but the plain one-texture `PDEFTBB01/01R/01C/01P`,
/// `PDEFTBB04` and the distortion family (hidden anyway).
pub(crate) fn needs_slot_textures(shader: &str) -> bool {
    !(shader.starts_with("PDEFTBB01") || shader == "PDEFTBB04" || shader.starts_with("PDEFTOPT") || shader.is_empty())
}

/// The distortion shaders `pd_game` builds (`PDEFTOPT03` / `PDEFTOPT05`: they share one vertex
/// and pixel stage); their slot-0 texture is a bump map.
pub(crate) fn is_distortion_shader(shader: &str) -> bool {
    shader == "PDEFTOPT05" || shader == "PDEFTOPT03"
}

/// `cache/fx/bones.json`: for every character folder under `Data/Player`,
/// its named body joints -> joint index, i.e. the glb node `p0j<index>` that
/// `pd_convert character` writes (same source file: the colour-0 sibling,
/// and the same body model: the first skinned one). Effect pieces attach to
/// the bone of their joint's name (FORMATS-EFFECTS.md "Where pieces attach").
fn export_bones(player_dir: &Path, out: &Path) -> Result<usize> {
    let mut all = serde_json::Map::new();
    let mut dirs: Vec<_> = std::fs::read_dir(player_dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        let Some(id) = d.file_name().and_then(|s| s.to_str()).map(String::from) else { continue };
        let base = d.join(format!("{id}.alr"));
        if !base.exists() {
            continue;
        }
        let Ok(a) = Alr::load(&crate::resolve_character_source(&base, None)) else { continue };
        let Some(body) = a.models.iter().find(|m| m.has_skeleton && m.draws.iter().any(|dr| !dr.palette.is_empty())) else { continue };
        let mut bones = serde_json::Map::new();
        for (i, j) in body.joints.iter().enumerate() {
            if let Some(n) = pd_formats::alr::joint_name(j.name).filter(|n| !n.is_empty()) {
                bones.entry(n).or_insert(json!(i));
            }
        }
        all.insert(id, serde_json::Value::Object(bones));
    }
    let n = all.len();
    std::fs::write(out.join("bones.json"), serde_json::Value::Object(all).to_string())?;
    Ok(n)
}

/// `pd_convert effects <Data/Effect dir> <out dir> [ids like 1,42,115]`:
/// `eftNNNN.glb` + `eftNNNN.fx.json` for every effect the archive's `e.i`
/// lists (496), plus any effect number that exists only as a loose file.
pub fn export_skill_effects(dir: &Path, out: &Path, only: &[u32]) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let archive = EffectArchive::open(dir).ok();
    let packed: BTreeSet<u32> = archive.as_ref().map(|a| a.entries().iter().map(|e| e.number as u32).collect()).unwrap_or_default();
    let mut loose: BTreeSet<u32> = BTreeSet::new();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("eft")) {
            continue;
        }
        if let Some(id) = p.file_stem().and_then(|s| s.to_str()?.strip_prefix("eft")).and_then(|n| n.parse::<u32>().ok()) {
            loose.insert(id);
        }
    }
    let packed_only: Vec<u32> = packed.iter().copied().filter(|n| !loose.contains(n)).collect();
    let loose_only: Vec<u32> = loose.iter().copied().filter(|n| !packed.contains(n)).collect();
    let mut ids: Vec<u32> = packed.union(&loose).copied().filter(|n| only.is_empty() || only.contains(n)).collect();
    ids.sort_unstable();

    let (mut ok, mut pieces, mut from_archive, mut from_loose, mut failed) = (0, 0, 0, 0, Vec::new());
    for id in ids {
        let stem = format!("eft{id:04}");
        let from_archive_entry = archive.as_ref().and_then(|a| a.read(id as u16).ok().flatten());
        let (a, eft_bytes) = if let Some((a, eft)) = from_archive_entry {
            from_archive += 1;
            (a, eft.to_vec())
        } else {
            let Ok(eft) = std::fs::read(dir.join(format!("{stem}.eft"))) else {
                failed.push(format!("{stem} (not in archive, no loose .eft)"));
                continue;
            };
            let a = match Alr::load(&dir.join(format!("{stem}.alr"))) {
                Ok(a) => a,
                Err(e) => {
                    failed.push(format!("{stem} (loose .alr: {e:#})"));
                    continue;
                }
            };
            from_loose += 1;
            (a, eft)
        };
        let def = match effect::parse_eft(&eft_bytes) {
            Ok(d) => d,
            Err(e) => {
                failed.push(format!("{stem} (.eft: {e:#})"));
                continue;
            }
        };
        let params = effect::piece_params(&a)?;
        let emitters = effect::emitters(&a)?;
        let (glb, _) = gltf::export_glb(&a, &gltf::ExportOptions { apply_transforms: true, vertex_colours: true, ..Default::default() })?;
        std::fs::write(out.join(format!("{stem}.glb")), glb)?;
        std::fs::write(out.join(format!("{stem}.fx.json")), effect::to_json(&def, &params, &emitters).to_string())?;
        // Textures of shader-drawn pieces (trails, sprite/puff emitters)
        // aren't in the glb: write them beside it, as `{stem}_tN.png`.
        let mut done = BTreeSet::new();
        for t in emitters.iter().filter(|e| is_sprite_emitter(e.kind)).filter_map(|e| e.texture) {
            if let Some(entry) = a.textures.get(t as usize).filter(|_| done.insert(t)) {
                if let Ok(png) = pd_formats::texture::decode(&a, entry).and_then(|img| img.to_png()) {
                    std::fs::write(out.join(format!("{stem}_t{t}.png")), png)?;
                }
            }
        }
        // Textures a piece's pixel shader samples beyond the glb's own (slot 0):
        // the multi-texture families (`PDMTEX*`, `PDEFTBB02/05/07/08`, `PDEFTAURA01`,
        // `msEFTBB01S` = t3, ...) read their texture slots `t0..t3` from the
        // material's four indices (`PieceParams::mat_tex`). Written as
        // `{stem}_t{index}.png`, like the emitter textures above.
        for p in params.iter().filter(|p| needs_slot_textures(&p.shader)) {
            for &t in &p.mat_tex {
                if let Some(entry) = a.textures.get(t as usize).filter(|_| done.insert(t)) {
                    if let Ok(png) = pd_formats::texture::decode(&a, entry).and_then(|img| img.to_png()) {
                        std::fs::write(out.join(format!("{stem}_t{t}.png")), png)?;
                    }
                }
            }
        }
        // The screen-distortion pieces' bump map (`PDEFTOPT03/05`, texture slot 0, pixel format
        // code 8 = two-byte luminance + alpha): not in the glb, written as `{stem}_t{index}.png`.
        for p in params.iter().filter(|p| is_distortion_shader(&p.shader)) {
            if let Some(&t) = p.mat_tex.first() {
                if let Some(entry) = a.textures.get(t as usize).filter(|_| done.insert(t)) {
                    if let Ok(png) = pd_formats::texture::decode_bump(&a, entry).and_then(|img| img.to_png()) {
                        std::fs::write(out.join(format!("{stem}_t{t}.png")), png)?;
                    }
                }
            }
        }
        ok += 1;
        pieces += params.len();
    }
    println!("{ok} effects written to {} ({from_archive} from the packed archive, {from_loose} loose-only), {pieces} timed pieces", out.display());
    match dir.parent().map(|p| p.join("Player")).filter(|p| p.is_dir()) {
        Some(player) => println!("bones.json: named bones of {} characters", export_bones(&player, out)?),
        None => println!("bones.json not written: no Data/Player next to {}", dir.display()),
    }
    if !packed_only.is_empty() {
        println!("packed-only (no loose file), {}: {:?}", packed_only.len(), packed_only);
    }
    if !loose_only.is_empty() {
        println!("loose-only (not in e.i), {}: {:?}", loose_only.len(), loose_only);
    }
    if !failed.is_empty() {
        println!("failed, {}: {:?}", failed.len(), failed);
    }
    Ok(())
}

/// Every effect's `SHOTnn` markers (`effect::shot_markers`), keyed by effect
/// number, read from the packed archive first and a loose file as fallback
/// (the same source order as `export_skill_effects`). Effects without a
/// marker are left out.
pub fn shot_points(dir: &Path) -> Result<BTreeMap<u32, Vec<effect::ShotMarker>>> {
    let archive = EffectArchive::open(dir).ok();
    let mut ids: BTreeSet<u32> = archive.as_ref().map(|a| a.entries().iter().map(|e| e.number as u32).collect()).unwrap_or_default();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("eft")) {
            if let Some(id) = p.file_stem().and_then(|s| s.to_str()?.strip_prefix("eft")).and_then(|n| n.parse::<u32>().ok()) {
                ids.insert(id);
            }
        }
    }
    let mut out = BTreeMap::new();
    for id in ids {
        let loaded = match archive.as_ref().and_then(|a| a.read(id as u16).ok().flatten()) {
            Some((a, eft)) => Some((a, eft.to_vec())),
            None => match (Alr::load(&dir.join(format!("eft{id:04}.alr"))), std::fs::read(dir.join(format!("eft{id:04}.eft")))) {
                (Ok(a), Ok(e)) => Some((a, e)),
                _ => None,
            },
        };
        let Some((a, eft)) = loaded else { continue };
        let Ok(def) = effect::parse_eft(&eft) else { continue };
        let m = effect::shot_markers(&a, &def);
        if !m.is_empty() {
            out.insert(id, m);
        }
    }
    Ok(out)
}

/// `pd_convert shotpoints <Data dir> <cache dir> [--list]`: the shot spawn
/// points (docs/FORMATS-EFFECTS.md "Shot markers"), two files the game's
/// sim reads (it must not read animation state):
///
/// * `<cache>/shotpoints.json` -- every effect's `SHOTnn` marker offsets
///   (effect number -> positions in the caster's frame, in marker order);
/// * `<cache>/chars/<id>.hand.json` -- for every character, its right hand
///   (the program's fallback spawn / melee point) through each `ATK_*` clip
///   (`pd_formats::character::hand_paths`).
///
/// `--list` also prints every marker.
pub fn shot_points_run(args: &[String]) -> Result<()> {
    let usage = "usage: pd_convert shotpoints <Data dir> <cache dir> [--list]";
    let data = args.first().map(Path::new).context(usage)?;
    let cache = args.get(1).map(Path::new).context(usage)?;
    let list = args.iter().any(|a| a == "--list");
    let markers = shot_points(&data.join("Effect"))?;
    let mut doc = serde_json::Map::new();
    for (id, ms) in &markers {
        // Marker k = the program's slot k-1; a missing number leaves the slot at the
        // origin like the program's untouched array does (never happens in the data).
        let n = ms.iter().map(|m| m.index).max().unwrap_or(0) as usize;
        let mut pts = vec![[0.0f32; 3]; n];
        for m in ms {
            pts[m.index as usize - 1] = m.position;
        }
        doc.insert(id.to_string(), json!(pts));
        if list {
            let l: Vec<String> = ms.iter().map(|m| format!("SHOT{:02} m{} slots{:?} ({:.2}, {:.2}, {:.2})", m.index, m.model, m.slots, m.position[0], m.position[1], m.position[2])).collect();
            println!("eft{id:04}: {}", l.join(" | "));
        }
    }
    std::fs::create_dir_all(cache.join("chars"))?;
    let out = json!({
        "note": "effect number -> SHOT01.. offsets in the caster's right-handed frame (x right, y up, z forward at yaw 0); pd_convert shotpoints",
        "markers": serde_json::Value::Object(doc),
    });
    std::fs::write(cache.join("shotpoints.json"), serde_json::to_string_pretty(&out)?)?;
    println!("shotpoints.json: {} effects with SHOT markers", markers.len());
    let mut dirs: Vec<_> = std::fs::read_dir(data.join("Player"))?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let mut n = 0;
    for d in dirs {
        let Some(id) = d.file_name().and_then(|s| s.to_str()).map(String::from) else { continue };
        let base = d.join(format!("{id}.alr"));
        if !base.exists() {
            continue;
        }
        let Ok(a) = Alr::load(&crate::resolve_character_source(&base, None)) else { continue };
        let opts = pd_formats::character::CharacterOptions { order: pd_formats::pose::Order::GAME, anims: Vec::new(), extra_anim_sources: Vec::new(), coat_rig: None };
        match pd_formats::character::hand_paths(&a, &opts) {
            Ok(j) => {
                let clips = j["clips"].as_object().map_or(0, |c| c.len());
                std::fs::write(cache.join("chars").join(format!("{id}.hand.json")), j.to_string())?;
                n += 1;
                if list {
                    println!("{id}: right hand through {clips} ATK_ clips");
                }
            }
            Err(e) => println!("{id}: skipped ({e:#})"),
        }
    }
    println!("chars/<id>.hand.json: {n} characters");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_distortion_shader, needs_slot_textures};

    #[test]
    fn two_distortion_shaders_have_a_bump_map() {
        assert!(is_distortion_shader("PDEFTOPT05") && is_distortion_shader("PDEFTOPT03"));
        assert!(!is_distortion_shader("PDEFTOPT06") && !is_distortion_shader("PDEFTBB01"));
    }

    #[test]
    fn only_the_multi_texture_shaders_need_their_slot_textures() {
        // The plain one-texture shaders and the hidden distortion family sample slot 0 (the glb's own).
        for s in ["PDEFTBB01", "PDEFTBB01R", "PDEFTBB04", "PDEFTOPT05", ""] {
            assert!(!needs_slot_textures(s), "{s}");
        }
        for s in ["PDMTEX01", "PDMTEX02", "PDEFTAURA01", "PDEFTBB02", "PDEFTBB08", "MSEFTBB01S"] {
            assert!(needs_slot_textures(s), "{s}");
        }
    }
}
