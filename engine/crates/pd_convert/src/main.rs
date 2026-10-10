//! pd_convert - turn Phantom Dust data into standard files in a local cache.
//!
//!   pd_convert info  <file.alr>
//!   pd_convert model <file.alr> <out.glb> [--no-transforms] [--unlit]
//!   pd_convert music <file.stx> <out.wav>
//!   pd_convert scan  <data dir>      (parse every .alr, report failures)
//!   pd_convert stages <Map dir> <cache dir>  (convert every stage + write stages.json)

use anyhow::{bail, Context, Result};
use pd_formats::{alr::Alr, anim, gltf, stx};
use std::path::{Path, PathBuf};

mod chars;
mod menus;
mod skyscroll;
mod effects;
mod sounds;
mod collcompare;
mod walkprobe;
mod tracecheck;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("menus") => menus::run(&args[1..])?, // front-end menu art, font, bios -> cache/ui/menu (menus.rs)
        Some("skyscroll") => skyscroll::run(&args[1..])?, // per-stage sky UV-scroll fields -> stNN.sky.json (skyscroll.rs)
        Some("sounds") => sounds::run(&args[1..])?, // sound effects + voices -> WAVs (sounds.rs)
        Some("cpu") => cpu_files(&args[1..])?, // CPU opponent data: eneprog006.ssb, enemyparam.epa, stage nav graphs -> cache/com
        Some("chars") => chars::run(&args[1..])?, // character table -> chars.json (chars.rs)
        Some("stagefx") => effects::run(&args[1..])?, // arena-object effects (effects.rs)
        Some("shotpoints") => effects::shot_points_run(&args[1..])?, // SHOTnn markers + right-hand paths for the sim (effects.rs)
        Some("fxgap") => {
            // pd_convert fxgap <Effect dir> [skills.json] -- one-off audit,
            // 2026-09-28: which effects have a draw in VERTFMT_EFFECT_SKIN
            // (0x20), the skinned/palette format the exporter used to drop
            // silently (see pd_formats::alr, docs/FORMATS-EFFECTS.md
            // "Skinned/palette effect pieces"). Cross-references
            // cache/skills.json (skill -> effect via action.ground/air.effect,
            // same lookup as pd_game's SkillFx::load) to flag any skill that
            // used one.
            let dir = Path::new(&args[1]);
            let archive = pd_formats::effect_archive::EffectArchive::open(dir).ok();
            let packed: std::collections::BTreeSet<u32> = archive.as_ref().map(|a| a.entries().iter().map(|e| e.number as u32).collect()).unwrap_or_default();
            let mut loose: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for e in std::fs::read_dir(dir)? {
                let p = e?.path();
                if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("eft")) {
                    continue;
                }
                if let Some(id) = p.file_stem().and_then(|s| s.to_str()?.strip_prefix("eft")).and_then(|n| n.parse::<u32>().ok()) {
                    loose.insert(id);
                }
            }
            let mut ids: Vec<u32> = packed.union(&loose).copied().collect();
            ids.sort_unstable();
            let mut affected = Vec::new();
            for id in &ids {
                let a = if let Some((a, _eft)) = archive.as_ref().and_then(|ar| ar.read(*id as u16).ok().flatten()) {
                    a
                } else if let Ok(a) = Alr::load(&dir.join(format!("eft{id:04}.alr"))) {
                    a
                } else {
                    continue;
                };
                if a.models.iter().any(|m| m.draws.iter().any(|d| m.vertbufs.get(d.vertex_buf as usize).is_some_and(|v| v.format == 0x20))) {
                    affected.push(*id);
                }
            }
            println!("{} of {} effects had a format-0x20 draw dropped before this fix: {:?}", affected.len(), ids.len(), affected);
            if let Some(sp) = args.get(2) {
                let sj: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sp)?)?;
                println!("skills using an affected effect:");
                for s in sj["skills"].as_array().into_iter().flatten() {
                    for side in ["ground", "air"] {
                        if let Some(e) = s["action"][side]["effect"].as_i64() {
                            if affected.contains(&(e as u32)) {
                                println!("  skill id {:?} {:?} ({side}) -> eft{e:04}", s["id"], s["name"]);
                            }
                        }
                    }
                }
            }
        }
        Some("info") => {
            let a = Alr::load(Path::new(&args[1]))?;
            print!("{}", gltf::describe(&a));
        }
        Some("model") => {
            // Stage objects from the sibling .dat are placed unless --no-props.
            let a = Alr::load(Path::new(&args[1]))?;
            let props = if args.iter().any(|x| x == "--no-props") { None } else { stage_props(Path::new(&args[1]), &a) };
            let mut opts = gltf::ExportOptions { apply_transforms: !args.iter().any(|x| x == "--no-transforms"), include_untextured: args.iter().any(|x| x == "--untextured"), ..Default::default() };
            if let Some(p) = &props {
                opts.skip_models = p.skip_models.clone();
                opts.instances = p.instances.clone();
            }
            // Stage lighting from the sibling .dat's light script (--unlit: off).
            let lights = if args.iter().any(|x| x == "--unlit") { None } else { stage_lights(Path::new(&args[1])) };
            opts.lighting = lights.as_ref().map(|l| l.sets[0]);
            let (glb, s) = gltf::export_glb(&a, &opts)?;
            let out = PathBuf::from(&args[2]);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&out, glb)?;
            if let Some(l) = &lights {
                std::fs::write(out.with_extension("light.json"), serde_json::to_string_pretty(&l.to_json())?)?;
            }
            if let Some(p) = &props {
                // <out>.props.json + <out>.props.glb next to the glb.
                let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                write_props(&a, p, out.parent().unwrap_or(Path::new(".")), &stem, opts.lighting)?;
                println!("placed {} objects from layout {}", p.instances.len(), p.layout);
            }
            let bounds = serde_json::json!({ "min": s.core_min, "max": s.core_max, "full_min": s.bounds_min, "full_max": s.bounds_max });
            std::fs::write(out.with_extension("bounds.json"), bounds.to_string())?;
            println!(
                "{}: {} models, {} draws, {} triangles, textures {} ok / {} failed, core bounds {:?}..{:?}",
                out.display(),
                s.models,
                s.draws,
                s.triangles,
                s.textures_ok,
                s.textures_failed,
                s.core_min,
                s.core_max
            );
        }
        Some("capsules") => {
            // pd_convert capsules <Map/st00.alr> <Map/st00.dat> <out_dir>
            //
            // st00.alr's 24 model slots are the game's capsule and Aura
            // Particle art (docs/CAPSULE-VISUALS.md "Program read
            // 2026-09-28" -> "Where the capsule art lives"), not the
            // placeholder sphere fx.rs draws today. One named .glb per slot
            // (not a merged scene, so pd_game can pick the one for a
            // capsule's kind); point-sprite draws (vertex format 0x18: the
            // materialise-sparkle / pickup-burst / erase-burst pieces in
            // slots 0/2/3) have no UV, so the normal mesh exporter drops
            // them -- they come out as a `.points.json` sidecar instead
            // (`gltf::point_sprites`). `st00.dat`'s per-slot animation
            // length (frame count only, see `st00_dat_frame_lengths`) is
            // recorded for slots 0-3 so pd_game can time their playback;
            // decoding the actual keyframe curves is not attempted (see the
            // function's doc comment).
            let a = Alr::load(Path::new(&args[1]))?;
            let dat = std::fs::read(Path::new(&args[2])).unwrap_or_default();
            let frames = st00_dat_frame_lengths(&dat);
            let out_dir = PathBuf::from(&args[3]);
            std::fs::create_dir_all(&out_dir)?;
            let mut manifest = serde_json::json!({ "slots": [] });
            for (i, name) in ST00_SLOT_NAMES.iter().enumerate() {
                if i >= a.models.len() || a.models[i].draws.is_empty() {
                    continue;
                }
                // Dedicated capsule exporter (docs/CAPSULE-VISUALS.md "Draw
                // recipe" build step 1): carries TEXCOORD_1/TANGENT/vertex
                // colour, which the generic `export_glb` path doesn't, and
                // records which glTF texture index each draw resolved to so
                // `pd_game`'s per-kind shading table can find them.
                let (glb, s, draw_tex) = gltf::export_capsule_slot_glb(&a, i)?;
                let stem = format!("cap{i:02}_{name}");
                let has_mesh = s.draws > 0;
                if has_mesh {
                    std::fs::write(out_dir.join(format!("{stem}.glb")), &glb)?;
                }
                let points = gltf::point_sprites(&a, &a.models[i]);
                if !points.is_empty() {
                    let arr: Vec<_> = points.iter().map(|(p, c)| serde_json::json!({ "pos": [p.x, p.y, p.z], "color": [c[0], c[1], c[2], c[3]] })).collect();
                    std::fs::write(out_dir.join(format!("{stem}.points.json")), serde_json::to_string(&arr)?)?;
                }
                // Slots 0-3: the same points with their joints' animation baked per
                // frame, the draw's size / start / end and each point's UV
                // (`pd_formats::capsprites`; docs/CAPSULE-VISUALS.md "Regeneration
                // motes") -- the regeneration motes, pickup and erase bursts.
                let sprites = if i < 4 && !points.is_empty() {
                    match pd_formats::capsprites::build_cloud(&a, i, &dat) {
                        Ok(c) if !c.draws.is_empty() => {
                            let draws: Vec<_> = c
                                .draws
                                .iter()
                                .map(|d| {
                                    serde_json::json!({
                                        "joint": d.joint,
                                        "size": d.size,
                                        "start": d.start,
                                        "end": d.end,
                                        "points": d.points.iter().map(|p| [p.pos[0], p.pos[1], p.pos[2], p.uv[0], p.uv[1]]).collect::<Vec<_>>(),
                                        "m": d.matrices,
                                    })
                                })
                                .collect();
                            let name = format!("{stem}.sprites.json");
                            std::fs::write(out_dir.join(&name), serde_json::to_string(&serde_json::json!({ "frames": c.frames, "draws": draws }))?)?;
                            Some(name)
                        }
                        Ok(_) => None,
                        Err(e) => {
                            println!("slot {i}: sprites not baked: {e:#}");
                            None
                        }
                    }
                } else {
                    None
                };
                let anim_frames = frames.get(i).copied();
                let draws_json: Vec<_> = draw_tex.iter().map(|d| serde_json::json!({ "base": d.base, "secondary": d.secondary, "tertiary": d.tertiary, "quaternary": d.quaternary })).collect();
                manifest["slots"].as_array_mut().unwrap().push(serde_json::json!({
                    "slot": i,
                    "name": name,
                    "glb": has_mesh.then(|| format!("{stem}.glb")),
                    "points": (!points.is_empty()).then(|| format!("{stem}.points.json")),
                    "sprites": sprites,
                    "anim_frames": anim_frames,
                    "draws": draws_json,
                }));
                println!("slot {i:2} {name:18} draws {:3} triangles {:5} points {:4}{}", s.draws, s.triangles, points.len(), anim_frames.map(|f| format!(" anim {f:.0}f")).unwrap_or_default());
            }
            // Textures for the point-sprite pieces (slots 0/2/3 have no
            // material of their own, since their draws carry no UV): the
            // sheet at docs/CAPSULE-VISUALS.md "What an idle orb is made of"
            // names tex0/tex1 (slot 0's sparkles), tex2 (slot 1's flare
            // quad, already in its own .glb -- dumped too, for a tinted
            // billboard alternative), tex4 (slots 2/3's gold sparks).
            let mut textures = serde_json::Map::new();
            for ti in [0usize, 1, 2, 4] {
                let Some(t) = a.textures.get(ti) else { continue };
                let Ok(img) = pd_formats::texture::decode(&a, t) else { continue };
                let Ok(png) = img.to_png() else { continue };
                let name = format!("tex{ti:02}.png");
                std::fs::write(out_dir.join(&name), png)?;
                textures.insert(ti.to_string(), serde_json::Value::String(name));
            }
            manifest["textures"] = serde_json::Value::Object(textures);
            std::fs::write(out_dir.join("capsules.json"), serde_json::to_string_pretty(&manifest)?)?;
            println!("wrote {}", out_dir.join("capsules.json").display());
        }
        Some("character") => {
            // pd_convert character <file.alr> <out.glb> [--order zyx|xyz] [--anims 0,7,12] [--colour 0|1|2|3|base]
            // Defaults to the colour-0 sibling file (<id>0.alr) when one
            // sits next to the requested file: the game itself never plays
            // the base <id>.alr, only pcXXa<colour>.alr / enmXXa<colour>.alr
            // (PROGRAM-RE.md "Animation files" -- charaset +0x3A colour
            // count is non-zero for every playable character; every enemy
            // folder ships the same 0..3 siblings). The four colours are
            // motion-identical (checked: pc00a0 vs pc00a1 and pc01a0 vs
            // pc01a1 give byte-identical DASH_A/DASHEND_A/DASH_B/DASHEND_B/
            // JUMP_*/WALK track lengths; only the textures differ), so
            // colour 0 is as good as any. `--colour base` forces the old
            // behaviour (the exact path given) for comparison; folders
            // without a `<id><colour>.alr` sibling (e.g. `evpc00a`) fall
            // back to it automatically.
            let colour_arg = args.iter().position(|x| x == "--colour").and_then(|i| args.get(i + 1)).map(String::as_str);
            let src = resolve_character_source(Path::new(&args[1]), colour_arg);
            let a = Alr::load(&src)?;
            let order = args.iter().position(|x| x == "--order").and_then(|i| args.get(i + 1)).and_then(|s| pd_formats::pose::Order::parse(s)).unwrap_or(pd_formats::pose::Order::GAME);
            let anims = args
                .iter()
                .position(|x| x == "--anims")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
                .unwrap_or_default();
            // The coat rig (if any) is looked up by the character's own id
            // (args[1]'s stem, e.g. "pc00a"), not the resolved colour file.
            let opts = pd_formats::character::CharacterOptions { order, anims, extra_anim_sources: Vec::new(), coat_rig: if args.iter().any(|x| x == "--no-coat-rig") { None } else { Path::new(&args[1]).file_stem().and_then(|s| pd_formats::skirt::coat_rig(&s.to_string_lossy())) } };
            let (glb, s) = pd_formats::character::export_character_glb(&a, &opts)?;
            let out = PathBuf::from(&args[2]);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&out, glb)?;
            let bounds = serde_json::json!({ "min": s.core_min, "max": s.core_max });
            std::fs::write(out.with_extension("bounds.json"), bounds.to_string())?;
            // Per-clip root-joint drift (character.rs::root_motion), for the
            // sim lane that will replace our in-place playback with it; see
            // its doc comment. We keep playing every clip in place today.
            let root_motion = pd_formats::character::root_motion(&a, &opts)?;
            let root_motion_clips = root_motion["clips"].as_object().map_or(0, |c| c.len());
            std::fs::write(out.with_extension("rootmotion.json"), serde_json::to_string_pretty(&root_motion)?)?;
            println!(
                "{} (from {}): {} draws, {} triangles, {} animations, coat chains {}, textures {} ok / {} failed, {} clips with root motion",
                out.display(),
                src.display(),
                s.draws,
                s.triangles,
                s.animations,
                s.coat_chains,
                s.textures_ok,
                s.textures_failed,
                root_motion_clips
            );
            // Costume-colour textures (PIPELINE.md "Player/<id>/<id>0..3.alr";
            // FRONTEND.md Requests "Colour-variant character textures"): the
            // game tells P2 apart from an identical P1 pick by swapping to
            // colour 1's version of this one texture, not a runtime tint
            // (DECISIONS.md 2026-09-27). Export every colour sibling's
            // base-colour texture (same index in every sibling -- they share
            // layout, only this texture's bytes differ) as a small PNG next
            // to the glb, e.g. `cache/chars/pc00a.c0.png`..`.c3.png`, instead
            // of a full duplicate model per colour; the game swaps this PNG
            // onto the one glb at runtime. Including colour 0 (already
            // baked into the glb) is cheap and gives every colour the same
            // side-by-side file, useful for the sheet and for anything that
            // wants a colour swap without re-reading the glb. `--no-colour-
            // textures` skips this (probing, or a character with no siblings).
            if !args.iter().any(|x| x == "--no-colour-textures") {
                if let Some(tex_index) = pd_formats::character::base_colour_texture_index(&a) {
                    for colour in 0u8..=3 {
                        let Some(sibling) = colour_sibling(Path::new(&args[1]), colour) else { continue };
                        let ca = match Alr::load(&sibling) {
                            Ok(ca) => ca,
                            Err(e) => {
                                eprintln!("colour {colour} ({}): {e:#}", sibling.display());
                                continue;
                            }
                        };
                        match pd_formats::character::export_base_colour_png(&ca, tex_index) {
                            Ok(png) => {
                                let cout = out.with_file_name(format!("{}.c{colour}.png", out.file_stem().unwrap_or_default().to_string_lossy()));
                                std::fs::write(&cout, &png)?;
                                println!("{}: colour {colour} base texture ({} bytes)", cout.display(), png.len());
                            }
                            Err(e) => eprintln!("colour {colour} texture ({}): {e:#}", sibling.display()),
                        }
                    }
                }
            }
        }
        Some("collision") => {
            // pd_convert collision <stage.alr> <out.pdcl>  â€” also prints a spawn survey
            let a = Alr::load(Path::new(&args[1]))?;
            let props = if args.iter().any(|x| x == "--no-props") { None } else { stage_props(Path::new(&args[1]), &a) };
            let (tris, bytes) = match &props {
                Some(p) => stage_collision(&a, p, std::fs::read(Path::new(&args[1]).with_extension("dat")).ok().as_deref(), collision_source(&args)),
                None => {
                    let t = gltf::world_triangles(&a);
                    let b = pd_formats::collision::Collision::to_bytes(&t);
                    (t, b)
                }
            };
            std::fs::write(&args[2], &bytes)?;
            if let (Some(p), CollisionSource::Mesh094) = (&props, collision_source(&args)) {
                if let Some(cam) = stage_camcol(&a, p, std::fs::read(Path::new(&args[1]).with_extension("dat")).ok().as_deref()) {
                    std::fs::write(Path::new(&args[2]).with_extension("camcol"), cam)?;
                }
            }
            let col = pd_formats::collision::Collision::from_bytes(&bytes)?;
            println!("{}: {} triangles, {} standable", args[2], tris.len(), col.triangle_count());
            // Survey: ground heights on a coarse grid around the origin.
            let mut hist = std::collections::BTreeMap::<i32, usize>::new();
            for gx in -20..=20 {
                for gz in -20..=20 {
                    if let Some(y) = col.ground_below(gx as f32 * 25.0, gz as f32 * 25.0, 400.0) {
                        *hist.entry((y / 10.0).round() as i32 * 10).or_default() += 1;
                    }
                }
            }
            println!("ground height histogram (y: samples): {hist:?}");
            // All layers, and a top-down map of the highest surface below y=150.
            let mut all = std::collections::BTreeMap::<i32, usize>::new();
            for gx in -40..=40 {
                for gz in -40..=40 {
                    for y in col.layers_at(gx as f32 * 12.5, gz as f32 * 12.5) {
                        *all.entry((y / 5.0).round() as i32 * 5).or_default() += 1;
                    }
                }
            }
            println!("all layers (y: samples): {:?}", all.iter().filter(|(_, n)| **n > 20).collect::<Vec<_>>());
            let chars = " .:-=+*#%@";
            for gz in (-24..=24).rev() {
                let row: String = (-40..=40)
                    .map(|gx| match col.ground_below(gx as f32 * 15.0, gz as f32 * 25.0, 450.0) {
                        None => ' ',
                        Some(y) if y < 200.0 => '~',
                        Some(y) => chars.chars().nth((((y - 200.0) / 25.0) as usize).clamp(1, 9)).unwrap(),
                    })
                    .collect();
                println!("|{row}|");
            }
        }
        // `pd_convert musictracks <BGM dir> <out dir>`: every track the game plays (the stages' and the
        // menus' / results') as `<basename>.pdm` (samples + the loop region the STX header carries).
        Some("musictracks") => {
            let (dir, out) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
            std::fs::create_dir_all(&out)?;
            let mut n = 0;
            for e in std::fs::read_dir(&dir)?.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let Some(stem) = name.strip_suffix(".stx") else { continue };
                // Numbered tracks 1-38 and 60 (stages, bosses, menus, results); not the tutorial voice-overs.
                let num: u32 = stem.split('_').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                if num == 0 || !(num <= 41 || num == 60) {
                    continue;
                }
                let m = stx::decode(&std::fs::read(e.path())?)?;
                std::fs::write(out.join(format!("{stem}.pdm")), stx::to_pdm(&m))?;
                println!("{stem}.pdm: {:.1} s, loop {:?}", m.samples.len() as f64 / 2.0 / m.sample_rate as f64, m.loop_frames());
                n += 1;
            }
            println!("{n} tracks");
        }
        Some("music") => {
            let d = std::fs::read(&args[1]).context("reading stx")?;
            let m = stx::decode(&d)?;
            let out = PathBuf::from(&args[2]);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&out, stx::to_wav(&m))?;
            println!(
                "{}: {} Hz, {:.1} s, loop blocks {}..{}",
                out.display(),
                m.sample_rate,
                m.samples.len() as f64 / 2.0 / m.sample_rate as f64,
                m.loop_start_block,
                m.loop_end_block
            );
        }
        Some("skills") => {
            // pd_convert skills <gsdata_en.dat> <out.json> [--decks <deckdata_debug>]  â€” also writes <out>.txt
            skills(Path::new(&args[1]), Path::new(&args[2]), args.iter().position(|x| x == "--decks").and_then(|i| args.get(i + 1)).map(Path::new))?
        }
        Some("effects") => {
            // pd_convert effects <Data/Effect dir> <out dir> [ids like 1,42,115]  â€” eftNNNN.glb + eftNNNN.fx.json each, from the packed archive (effects.rs)
            let only: Vec<u32> = args.get(3).map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            effects::export_skill_effects(Path::new(&args[1]), Path::new(&args[2]), &only)?
        }
        Some("charcolours") => {
            // pd_convert charcolours <Data/Player dir> <cache/chars dir>
            // Costume-colour textures only (<id>.c0..c3.png) for every
            // pcXXa/enmXXa folder with colour siblings -- no glb/bounds/
            // rootmotion rewrite, so a cache regenerated by another lane
            // isn't touched by anything but the new files (PIPELINE.md
            // "Player/<id>/<id>0..3.alr"; FRONTEND.md Requests
            // "Colour-variant character textures"). Cheap to (re-)run after
            // this lane's converter change or a game-files update.
            let players_dir = Path::new(&args[1]);
            let out_dir = Path::new(&args[2]);
            std::fs::create_dir_all(out_dir)?;
            let mut written = 0usize;
            let mut entries: Vec<_> = std::fs::read_dir(players_dir)?.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().to_string();
                let base = entry.path().join(format!("{id}.alr"));
                let Some(colour0) = colour_sibling(&base, 0) else { continue }; // no colour siblings (e.g. evpc00a)
                let tex_index = match Alr::load(&colour0).ok().and_then(|a| pd_formats::character::base_colour_texture_index(&a)) {
                    Some(i) => i,
                    None => continue,
                };
                for colour in 0u8..=3 {
                    let Some(sibling) = colour_sibling(&base, colour) else { continue };
                    let ca = match Alr::load(&sibling) {
                        Ok(a) => a,
                        Err(e) => {
                            eprintln!("{id} colour {colour} ({}): {e:#}", sibling.display());
                            continue;
                        }
                    };
                    match pd_formats::character::export_base_colour_png(&ca, tex_index) {
                        Ok(png) => {
                            std::fs::write(out_dir.join(format!("{id}.c{colour}.png")), &png)?;
                            written += 1;
                        }
                        Err(e) => eprintln!("{id} colour {colour} texture ({}): {e:#}", sibling.display()),
                    }
                }
            }
            println!("{written} costume-colour textures written to {}", out_dir.display());
        }
        Some("scan") => scan(Path::new(&args[1]))?,
        Some("stages") => stages(Path::new(&args[1]), Path::new(&args[2]), collision_source(&args))?,
        Some("propcheck") => propcheck(Path::new(&args[1]), Path::new(&args[2]))?,
        Some("walkprobe") => walkprobe::run(&args)?,
        Some("collcompare") => collcompare::run(&args)?,
        Some("tracecheck") => tracecheck::run(&args)?,
        Some("stagedat") =>stagedat_probe(Path::new(&args[1]), args.get(2).map(String::as_str))?,
        Some("texprobe") => {
            // pd_convert texprobe <file.alr>  â€” per texture: format, size, mean colour, how many draws use it
            let a = Alr::load(Path::new(&args[1]))?;
            let mut uses = std::collections::HashMap::<usize, (usize, std::collections::BTreeSet<u8>)>::new();
            for m in &a.models {
                for d in &m.draws {
                    if let Some(mat) = m.materials.get(d.material_idx as usize) {
                        let e = uses.entry(mat.texture_idx as usize).or_default();
                        e.0 += d.indices.len();
                        e.1.insert(mat.vertbuf_format);
                    }
                }
            }
            for (i, t) in a.textures.iter().enumerate() {
                let mean = pd_formats::texture::decode(&a, t).map(|img| {
                    let n = (img.pixels.len() / 4) as f32;
                    let mut s = [0f32; 4];
                    for p in img.pixels.chunks_exact(4) {
                        for k in 0..4 {
                            s[k] += p[k] as f32;
                        }
                    }
                    s.map(|v| (v / n) as u8)
                });
                let u = uses.get(&i);
                println!(
                    "tex {i:3} {:?} {}x{} mips {} cube {} ptr 0x{:X} mean {:?} used-indices {} vfmts {:?}",
                    t.format,
                    t.width,
                    t.height,
                    t.mip_count,
                    t.is_cubemap,
                    t.data_ptr,
                    mean.ok(),
                    u.map(|x| x.0).unwrap_or(0),
                    u.map(|x| &x.1)
                );
            }
            let mut missing = 0;
            for m in &a.models {
                for d in &m.draws {
                    let ti = m.materials.get(d.material_idx as usize).map(|x| x.texture_idx as usize);
                    if ti.is_none_or(|t| t >= a.textures.len()) {
                        missing += d.indices.len();
                    }
                }
            }
            println!("indices drawn with out-of-range/missing texture: {missing}");
        }
        Some("joints") => {
            // pd_convert joints <file.alr> <model index>
            let a = Alr::load(Path::new(&args[1]))?;
            let mi: usize = args[2].parse()?;
            let m = &a.models[mi];
            let world = pd_formats::pose::world_matrices(m, &pd_formats::pose::bind_locals(m), pd_formats::pose::Order::GAME);
            for (j, jt) in m.joints.iter().enumerate() {
                let w = world[j].w_axis;
                println!("j{j:3} parent {:4} name {:08X} local {:?} world [{:6.2} {:6.2} {:6.2}]", jt.parent, jt.name, jt.position, w.x, w.y, w.z);
            }
            for d in &m.draws {
                let vb = &m.vertbufs[d.vertex_buf as usize];
                println!("draw xform {} vb fmt 0x{:X} verts {} palette {:?}", d.transform_idx, vb.format, vb.vertex_count, d.palette);
                if vb.format == 0x11 {
                    // Which registers the vertices use, and where those vertices are.
                    let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
                    let mut acc = std::collections::BTreeMap::<(u16, u16), (usize, [f32; 3])>::new();
                    for v in raw.chunks_exact(vb.vertex_size as usize) {
                        let p = pd_formats::bytes::vec3_at(v, 0)?;
                        let r = (u16::from_le_bytes([v[20], v[21]]), u16::from_le_bytes([v[22], v[23]]));
                        let e = acc.entry(r).or_insert((0, [0.0; 3]));
                        e.0 += 1;
                        for k in 0..3 {
                            e.1[k] += p[k];
                        }
                    }
                    for ((r0, r1), (n, s)) in acc {
                        let slot = |r: u16| if r >= 120 && (r - 120) % 3 == 0 { d.palette.get(((r - 120) / 3) as usize).map(|j| *j as i32).unwrap_or(-1) } else { -2 };
                        println!("  regs ({r0},{r1}) -> joints ({},{}) n {n:4} centroid [{:6.2} {:6.2} {:6.2}]", slot(r0), slot(r1), s[0] / n as f32, s[1] / n as f32, s[2] / n as f32);
                    }
                }
            }
        }
        Some("clothprobe") => {
            // pd_convert clothprobe <Player dir>  â€” per character: skinned models, and body
            // joints that the skin uses but no animation ever moves (likely simulated cloth/hair).
            let mut dirs: Vec<_> = std::fs::read_dir(&args[1])?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs {
                let id = d.file_name().unwrap().to_string_lossy().to_string();
                let Ok(a) = Alr::load(&d.join(format!("{id}.alr"))) else { continue };
                let skinned: Vec<_> = a.models.iter().filter(|m| m.draws.iter().any(|x| !x.palette.is_empty())).collect();
                let Some(body) = skinned.first() else { continue };
                let mut animated = std::collections::BTreeSet::new();
                for aid in 0..129 {
                    if let Some(an) = anim::read_animation(&a, aid)? {
                        for t in &an.tracks {
                            animated.insert(t.joint);
                        }
                    }
                }
                let mut used = std::collections::BTreeSet::new();
                for dr in &body.draws {
                    used.extend(dr.palette.iter().map(|&j| j as u16));
                }
                let cloth: Vec<u16> = used.iter().copied().filter(|j| !animated.contains(j)).collect();
                // Group into chains: unanimated joints whose parent is also unanimated-and-skinned.
                let roots: Vec<u16> = cloth.iter().copied().filter(|&j| {
                    let p = body.joints[j as usize].parent;
                    p < 0 || !cloth.contains(&(p as u16))
                }).collect();
                println!(
                    "{id:8} parts {} (joints {:?}) | body skinned joints {} | never animated {} in {} chains, roots {:?}",
                    skinned.len() - 1,
                    skinned.iter().skip(1).map(|m| m.joints.len()).collect::<Vec<_>>(),
                    used.len(),
                    cloth.len(),
                    roots.len(),
                    roots
                );
            }
        }
        Some("uvmap") => {
            // pd_convert uvmap <file.alr> <out.png> <joint,joint,...>
            // Texture 0 with every body vertex skinned to the given joints
            // plotted at its UV: colour by height (red low .. green high).
            let a = Alr::load(Path::new(&args[1]))?;
            let joints: Vec<usize> = args[3].split(',').filter_map(|s| s.parse().ok()).collect();
            let m = a.models.iter().find(|m| m.draws.iter().any(|d| !d.palette.is_empty())).unwrap();
            let mut img = pd_formats::texture::decode(&a, &a.textures[0])?;
            let (w, h) = (img.width as i32, img.height as i32);
            for d in &m.draws {
                let vb = &m.vertbufs[d.vertex_buf as usize];
                if vb.format != 0x11 || d.palette.is_empty() {
                    continue;
                }
                let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
                for &vi in d.indices.iter().collect::<std::collections::BTreeSet<_>>() {
                    let v = &raw[vi as usize * vb.vertex_size as usize..][..vb.vertex_size as usize];
                    let reg = u16::from_le_bytes([v[20], v[21]]);
                    let Some(j) = reg.checked_sub(120).filter(|r| r % 3 == 0).and_then(|r| d.palette.get((r / 3) as usize)) else { continue };
                    if !joints.contains(&(*j as usize)) {
                        continue;
                    }
                    let y = pd_formats::bytes::f32_at(v, 4)?;
                    let u = i16::from_le_bytes([v[12], v[13]]) as f32 / 32767.0;
                    let t = i16::from_le_bytes([v[14], v[15]]) as f32 / 32767.0;
                    let (px, py) = (((u.rem_euclid(1.0)) * w as f32) as i32, ((t.rem_euclid(1.0)) * h as f32) as i32);
                    let g = ((y / 12.0).clamp(0.0, 1.0) * 255.0) as u8;
                    for dx in -2..=2 {
                        for dy in -2..=2 {
                            let (x, yy) = (px + dx, py + dy);
                            if x >= 0 && yy >= 0 && x < w && yy < h {
                                let o = ((yy * w + x) * 4) as usize;
                                img.pixels[o..o + 4].copy_from_slice(&[255 - g, g, 0, 255]);
                            }
                        }
                    }
                }
            }
            std::fs::write(&args[2], img.to_png()?)?;
        }
        Some("capprobe") => {
            // pd_convert capprobe <st00.alr> <slot>  -- research probe (docs/CAPSULE-VISUALS.md
            // "Regeneration motes"): one model slot's joints, materials, draws and every
            // chunk in its range with the odd chunks hex-dumped.
            let a = Alr::load(Path::new(&args[1]))?;
            let slot: usize = args[2].parse()?;
            let m = &a.models[slot];
            for (i, j) in m.joints.iter().enumerate() {
                println!("joint {i}: parent {} flags 0x{:X} name {:?} pos {:?} rot {:?} scale {:?}", j.parent, j.flags, pd_formats::alr::joint_name(j.name), j.position, j.rotation, j.scale);
            }
            for (i, mt) in m.materials.iter().enumerate() {
                println!("material {i}: flags 0x{:X} vfmt 0x{:X} tex {} {} {} {}", mt.flags, mt.vertbuf_format, mt.texture_idx, mt.normal_idx, mt.texture_idx2, mt.texture_idx3);
            }
            for (i, d) in m.draws.iter().enumerate() {
                let vb = &m.vertbufs[d.vertex_buf as usize];
                println!("draw {i}: xform {} material {} vb {} fmt 0x{:X} size {} count {} indices {} prim {:?}", d.transform_idx, d.material_idx, d.vertex_buf, vb.format, vb.vertex_size, vb.vertex_count, d.indices.len(), d.primitive);
            }
            let mut model = None::<usize>;
            let mut n_mat = 0usize;
            for c in &a.chunks {
                if c.id == pd_formats::alr::ID_MATERIAL {
                    model = Some(n_mat);
                    n_mat += 1;
                }
                if model != Some(slot) {
                    continue;
                }
                print!("chunk 0x{:X} @0x{:X} size {}", c.id, c.offset, c.size);
                if !matches!(c.id, 0x1 | 0x2 | 0x3 | 0x16) {
                    let n = c.size.min(0x80);
                    print!(" :");
                    for (k, b) in a.data[c.offset..c.offset + n].iter().enumerate() {
                        if k % 16 == 0 {
                            print!("\n    {k:03x}:");
                        }
                        print!(" {b:02x}");
                    }
                }
                println!();
            }
        }
        Some("drawinfo") => {
            // pd_convert drawinfo <file.alr> <model> <draw index>  â€” one draw's material and vertex extent
            let a = Alr::load(Path::new(&args[1]))?;
            let m = &a.models[args[2].parse::<usize>()?];
            let d = &m.draws[args[3].parse::<usize>()?];
            let vb = &m.vertbufs[d.vertex_buf as usize];
            let mat = m.materials.get(d.material_idx as usize);
            println!("xform {} vb {} fmt 0x{:X} material {} (vfmt {:?} tex {:?} normal {:?}) palette {:?} indices {}", d.transform_idx, d.vertex_buf, vb.format, d.material_idx, mat.map(|x| x.vertbuf_format), mat.map(|x| x.texture_idx), mat.map(|x| x.normal_idx), d.palette, d.indices.len());
            let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
            let used: std::collections::BTreeSet<u16> = d.indices.iter().copied().collect();
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for &vi in &used {
                let v = &raw[vi as usize * vb.vertex_size as usize..][..vb.vertex_size as usize];
                let p = pd_formats::bytes::vec3_at(v, 0)?;
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
                if used.len() < 40 {
                    println!("  v{vi}: pos {:?} regs ({},{}) w ({},{})", p, u16::from_le_bytes([v[20], v[21]]), u16::from_le_bytes([v[22], v[23]]), v[18], v[17]);
                }
            }
            println!("  {} vertices, bind bbox {:?}..{:?}", used.len(), lo, hi);
        }
        Some("dumptex") => {
            // pd_convert dumptex <file.alr> <out dir>  â€” every texture as PNG (debugging only; output stays local)
            let a = Alr::load(Path::new(&args[1]))?;
            std::fs::create_dir_all(&args[2])?;
            for (i, t) in a.textures.iter().enumerate() {
                match pd_formats::texture::decode(&a, t).and_then(|img| img.to_png()) {
                    Ok(png) => std::fs::write(Path::new(&args[2]).join(format!("tex{i:02}_{:?}_{}x{}.png", t.format, t.width, t.height)), png)?,
                    Err(e) => println!("tex {i}: {e}"),
                }
            }
            for (mi, m) in a.models.iter().enumerate() {
                for (k, mat) in m.materials.iter().enumerate() {
                    println!("model {mi} material {k}: vfmt 0x{:X} texture {}", mat.vertbuf_format, mat.texture_idx);
                }
            }
        }
        Some("hudtex") => {
            // pd_convert hudtex <Menu/BtMenuEn.alr> <out dir>  â€” the battle HUD atlases as hud0.png, hud1.png, ...
            let a = Alr::load(Path::new(&args[1]))?;
            let out = Path::new(&args[2]);
            std::fs::create_dir_all(out)?;
            for (i, t) in a.textures.iter().enumerate() {
                let img = pd_formats::texture::decode(&a, t)?;
                std::fs::write(out.join(format!("hud{i}.png")), img.to_png()?)?;
                println!("hud{i}.png: {}x{}", img.width, img.height);
            }
        }
        Some("sprites") => {
            // pd_convert sprites <file.alr> <texture index> [min alpha]  â€” bounding boxes of the separate
            // pieces in a UI texture atlas (connected pixels with alpha >= min, default 8)
            let a = Alr::load(Path::new(&args[1]))?;
            let t = a.textures.get(args[2].parse::<usize>()?).context("no such texture")?;
            let img = pd_formats::texture::decode(&a, t)?;
            let min: u8 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
            let (w, h) = (img.width as usize, img.height as usize);
            let solid = |x: usize, y: usize| img.pixels[(y * w + x) * 4 + 3] >= min;
            let mut seen = vec![false; w * h];
            let mut boxes = Vec::new();
            for start in 0..w * h {
                if seen[start] || !solid(start % w, start / w) {
                    continue;
                }
                let (mut x0, mut y0, mut x1, mut y1, mut n) = (w, h, 0, 0, 0usize);
                let mut stack = vec![start];
                seen[start] = true;
                while let Some(p) = stack.pop() {
                    let (x, y) = (p % w, p / w);
                    (x0, y0, x1, y1, n) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y), n + 1);
                    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let q = ny as usize * w + nx as usize;
                            if !seen[q] && solid(nx as usize, ny as usize) {
                                seen[q] = true;
                                stack.push(q);
                            }
                        }
                    }
                }
                if n >= 12 {
                    boxes.push((y0, x0, x1 - x0 + 1, y1 - y0 + 1, n));
                }
            }
            boxes.sort();
            println!("{} pieces in {}x{} (x y w h pixels):", boxes.len(), w, h);
            for (y, x, bw, bh, n) in boxes {
                println!("  {x:4} {y:4} {bw:4} {bh:4}  {n}");
            }
        }
        Some("keys") => {
            // pd_convert keys <file.alr> <anim id> <joint>  â€” print one track's keys
            let a = Alr::load(Path::new(&args[1]))?;
            let id: usize = args[2].parse()?;
            let joint: u16 = args[3].parse()?;
            if let Some(an) = anim::read_animation(&a, id)? {
                for t in an.tracks.iter().filter(|t| t.joint == joint) {
                    println!("settings {:02X?} len {}", t.settings, t.length);
                    for k in &t.translation {
                        println!("  T f{:5.1} {:?}", k.frame, &k.value[..k.components as usize]);
                    }
                    for k in t.rotation.iter().step_by(4) {
                        println!("  R f{:5.1} {:?}", k.frame, &k.value[..k.components as usize]);
                    }
                }
            }
        }
        Some("eulertest") => {
            // Score every Euler convention by how often knees and elbows bend
            // the impossible way across all animations. Game space: -Z is the
            // character's front (verified: rendered face points to game -Z).
            use pd_formats::pose::{self, Order};
            let a = Alr::load(Path::new(&args[1]))?;
            let m = a.models.iter().find(|m| m.joints.len() > 90).unwrap();
            let mut anims = Vec::new();
            for id in 0..129 {
                if let Some(an) = anim::read_animation(&a, id)? {
                    anims.push(an);
                }
            }
            // (upper, middle, end, frame-reference joint, expected sign of middle offset along ref +Z)
            let limbs = [(69usize, 71usize, 73usize, 68usize, -1.0f32), (83, 85, 87, 68, -1.0), (12, 14, 17, 10, 1.0), (34, 35, 38, 10, 1.0)];
            let mut results = Vec::new();
            for order in Order::all() {
                let (mut bad, mut total) = (0usize, 0usize);
                let mut facing_bad = 0usize;
                for an in &anims {
                    let len = an.length().max(1.0) as usize;
                    for f in (0..=len).step_by(2) {
                        let locals = pose::animated_locals(m, an, f as f32);
                        let w = pose::world_matrices(m, &locals, order);
                        for &(u, mid, e, r, sign) in &limbs {
                            let (pu, pm, pe) = (w[u].w_axis.truncate(), w[mid].w_axis.truncate(), w[e].w_axis.truncate());
                            let line = pe - pu;
                            if line.length() < 1e-3 {
                                continue;
                            }
                            let t = ((pm - pu).dot(line) / line.length_squared()).clamp(0.0, 1.0);
                            let off = pm - (pu + line * t);
                            let fwd = w[r].transform_vector3(glam::Vec3::Z).normalize_or_zero();
                            let d = off.dot(fwd) * sign;
                            total += 1;
                            if d < -0.4 {
                                bad += 1;
                            }
                        }
                        // Head should face roughly the same way as the pelvis.
                        let hf = w[57].transform_vector3(glam::Vec3::Z);
                        let pf = w[68].transform_vector3(glam::Vec3::Z);
                        if hf.dot(pf) < 0.0 {
                            facing_bad += 1;
                        }
                    }
                }
                results.push((bad as f32 / total.max(1) as f32, facing_bad, order));
            }
            results.sort_by(|x, y| x.0.total_cmp(&y.0));
            for (score, fb, o) in results.iter().take(12) {
                println!("{:5.1}% bad bends, {fb:4} head-backwards  {o:?}", score * 100.0);
            }
            println!("...current Xyz: {:?}", results.iter().find(|r| r.2 == Order::Custom { apply: [2, 1, 0], neg: 0 }).map(|r| r.0));
        }
        Some("skinprobe") => {
            // Match skinning palette slots to joints by comparing vertex
            // centroids with bind-pose joint positions.
            let a = Alr::load(Path::new(&args[1]))?;
            let m = &a.models[0];
            let world: Vec<[f32; 3]> = (0..m.joints.len()).map(|j| gltf::joint_world_lh(&m.joints, j).w_axis.truncate().into()).collect();
            let d = &a.data;
            // Per-draw summary for skinned buffers.
            for c in a.chunks.iter().filter(|c| c.id == 2) {
                let h = c.offset;
                let vb = pd_formats::bytes::u16_at(d, h + 68)? as usize;
                if m.vertbufs.get(vb).map(|v| v.format) != Some(0x11) {
                    continue;
                }
                let raw: Vec<u16> = (0..6).map(|i| pd_formats::bytes::u16_at(d, h + 52 + i * 2).unwrap()).collect();
                let unk1 = pd_formats::bytes::u32_at(d, h + 60)?;
                let unk5 = pd_formats::bytes::u32_at(d, h + 48)?;
                let pad: Vec<u32> = (0..5).map(|i| pd_formats::bytes::u32_at(d, h + 84 + i * 4).unwrap()).collect();
                let unkf = pd_formats::bytes::f32_at(d, h + 20)?;
                println!("draw @0x{h:X} vb {vb} xform {} unk5 {unk5:08X} b52.. {raw:?} unk1 {unk1:08X} unkf {unkf} pad {pad:?}", pd_formats::bytes::u16_at(d, h + 66)?);
            }
            let sk = a.chunks.iter().find(|c| c.id == 3).unwrap().offset;
            for (j, w) in world.iter().enumerate() {
                let e = sk + 16 + j * 0x40;
                println!(
                    "joint {j:3} parent {:4} b36 {:02X} b37 {:02X} b40 {:04X} name {:08X} world [{:6.2} {:6.2} {:6.2}]",
                    m.joints[j].parent,
                    d[e + 36],
                    d[e + 37],
                    pd_formats::bytes::u16_at(d, e + 40)?,
                    m.joints[j].name,
                    w[0],
                    w[1],
                    w[2]
                );
            }
            // Other chunk ids present between models.
            let mut ids = std::collections::BTreeMap::<u32, usize>::new();
            for c in &a.chunks {
                *ids.entry(c.id).or_default() += 1;
            }
            println!("chunk ids: {ids:?}");
            // Per draw: which palette registers its vertices use, and where.
            for draw in m.draws.iter() {
                let Some(vb) = m.vertbufs.get(draw.vertex_buf as usize) else { continue };
                if vb.format != 0x11 {
                    continue;
                }
                let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
                let verts: std::collections::BTreeSet<u16> = draw.indices.iter().copied().collect();
                let mut acc = std::collections::BTreeMap::<u16, ([f32; 3], usize)>::new();
                for &vi in &verts {
                    let v = &raw[vi as usize * vb.vertex_size as usize..][..vb.vertex_size as usize];
                    let p = pd_formats::bytes::vec3_at(v, 0)?;
                    let (w0, w1) = (v[18], v[17]);
                    let i0 = u16::from_le_bytes([v[20], v[21]]);
                    let i1 = u16::from_le_bytes([v[22], v[23]]);
                    let slot = if w0 >= w1 { i0 } else { i1 };
                    let e = acc.entry(slot).or_insert(([0.0; 3], 0));
                    for k in 0..3 {
                        e.0[k] += p[k];
                    }
                    e.1 += 1;
                }
                println!("draw xform {} ({} verts)", draw.transform_idx, verts.len());
                for (slot, (s, n)) in acc {
                    let c = [s[0] / n as f32, s[1] / n as f32, s[2] / n as f32];
                    println!("   reg {slot:3} slot {:2} n {n:4} centroid [{:6.2} {:6.2} {:6.2}]", (slot as i32 - 120) / 3, c[0], c[1], c[2]);
                }
            }
            for (vi, vb) in m.vertbufs.iter().enumerate().filter(|(_, v)| v.format == 0x11) {
                let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
                let mut acc = std::collections::BTreeMap::<u16, ([f32; 3], usize)>::new();
                for v in raw.chunks_exact(vb.vertex_size as usize) {
                    let p = pd_formats::bytes::vec3_at(v, 0)?;
                    let i0 = u16::from_le_bytes([v[20], v[21]]);
                    let e = acc.entry(i0).or_insert(([0.0; 3], 0));
                    for k in 0..3 {
                        e.0[k] += p[k];
                    }
                    e.1 += 1;
                }
                println!("vb {vi}:");
                for (slot, (sum, n)) in acc {
                    let c = [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32];
                    let mut best: Vec<(f32, usize)> = world
                        .iter()
                        .enumerate()
                        .map(|(j, w)| (((w[0] - c[0]).powi(2) + (w[1] - c[1]).powi(2) + (w[2] - c[2]).powi(2)).sqrt(), j))
                        .collect();
                    best.sort_by(|x, y| x.0.total_cmp(&y.0));
                    println!(
                        "  reg {slot:3} (slot {:3}) n {n:4} centroid [{:6.2} {:6.2} {:6.2}] nearest joints {:?}",
                        (slot as i32 - 96) / 3,
                        c[0],
                        c[1],
                        c[2],
                        best.iter().take(3).map(|(dd, j)| format!("{j}@{dd:.1}")).collect::<Vec<_>>()
                    );
                }
            }
        }
        Some("anims") => {
            // pd_convert anims <file.alr> [anim id]  - dump skeleton + animation tracks
            let a = Alr::load(Path::new(&args[1]))?;
            for (mi, m) in a.models.iter().enumerate() {
                println!("model {mi}: {} joints (has_joints={}), {} vertbufs, {} draws", m.joints.len(), m.has_skeleton, m.vertbufs.len(), m.draws.len());
                let used: std::collections::BTreeSet<u16> = m.draws.iter().map(|d| d.transform_idx).collect();
                println!("  draw transform_idx used: {used:?}");
            }
            for (mi, m) in a.models.iter().enumerate() {
                for (vi, vb) in m.vertbufs.iter().enumerate() {
                    if vb.format != 0x11 {
                        continue;
                    }
                    let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
                    let mut idx = std::collections::BTreeMap::<(u16, u16), usize>::new();
                    let mut samples = Vec::new();
                    for (n, v) in raw.chunks_exact(vb.vertex_size as usize).enumerate() {
                        let w0 = u16::from_le_bytes([v[16], v[17]]);
                        let w1 = u16::from_le_bytes([v[18], v[19]]);
                        let i0 = u16::from_le_bytes([v[20], v[21]]);
                        let i1 = u16::from_le_bytes([v[22], v[23]]);
                        *idx.entry((i0, i1)).or_default() += 1;
                        if n % 200 == 0 {
                            samples.push((w0, w1, i0, i1));
                        }
                    }
                    println!("model {mi} vb {vi}: {} verts, stride {}", vb.vertex_count, vb.vertex_size);
                    println!("  index pairs: {:?}", idx.iter().take(40).collect::<Vec<_>>());
                    println!("  samples (w0,w1,i0,i1): {samples:?}");
                }
            }
            if let Some(id) = args.get(2).and_then(|s| s.parse::<usize>().ok()) {
                let m = a.models.first();
                if let Some(an) = anim::read_animation(&a, id)? {
                    println!("anim {id} {} : {} tracks, length {}", anim::ANIM_NAMES.get(id).unwrap_or(&"?"), an.tracks.len(), an.length());
                    for t in &an.tracks {
                        let j = m.and_then(|m| m.joints.get(t.joint as usize));
                        println!(
                            "  joint {:3} parent {:4} settings {:02X?} T[{}x{}] R[{}x{}] S{} len {}  bindpos {:?} bindrot {:?}  T0 {:?} R0 {:?}",
                            t.joint,
                            j.map(|j| j.parent).unwrap_or(-99),
                            t.settings,
                            t.translation.len(),
                            t.translation_key_size,
                            t.rotation.len(),
                            t.rotation_key_size,
                            t.scale_key_count,
                            t.length,
                            j.map(|j| j.position),
                            j.map(|j| j.rotation),
                            t.translation.first().map(|k| (k.frame, k.value)),
                            t.rotation.first().map(|k| (k.frame, k.value)),
                        );
                    }
                }
            }
        }
        _ => bail!("usage: pd_convert info|model|music|skills|scan ..."),
    }
    Ok(())
}

/// Pick the `.alr` to actually read for `character`: the requested path,
/// unless a sibling colour-variant file exists next to it, in which case
/// that file wins (see the `character` command's doc comment above and
/// PROGRAM-RE.md "Animation files"). `colour_arg` is the `--colour` flag's
/// value: `"base"` forces `requested`; a digit picks that colour; anything
/// else (including no flag) defaults to colour 0.
pub(crate) fn resolve_character_source(requested: &Path, colour_arg: Option<&str>) -> PathBuf {
    if colour_arg == Some("base") {
        return requested.to_path_buf();
    }
    let colour = colour_arg.and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
    colour_sibling(requested, colour).unwrap_or_else(|| requested.to_path_buf())
}

/// `<stem><colour>.alr` next to `requested` (e.g. `pc00a.alr` -> `pc00a1.alr`
/// for colour 1), if it exists. `None` for folders with no colour siblings
/// (e.g. `evpc00a`) or an out-of-range colour.
fn colour_sibling(requested: &Path, colour: u8) -> Option<PathBuf> {
    let stem = requested.file_stem().and_then(|s| s.to_str())?;
    let dir = requested.parent()?;
    let candidate = dir.join(format!("{stem}{colour}.alr"));
    candidate.exists().then_some(candidate)
}

/// One row of the action table as JSON (`skills.json`'s `"action"."ground"` /
/// `"air"`; see `pd_formats::skills::tables::TableRow`).
fn action_row_json(r: &pd_formats::skills::tables::TableRow) -> serde_json::Value {
    serde_json::json!({
        "row": null, // filled in by the caller, which knows the row index
        "effect": r.effect(),
        "cast_sound": r.cast_sound(),
        "voice_group": r.voice_group(),
        "anim_id": r.anim_id(),
        "anim_name": r.anim_name(),
        "charge_cycle": r.charge_cycle(),
        "start_frame": r.start_frame(),
        "dash_start": r.dash_start(),
        "dash_end": r.dash_end(),
        "fire_tick": r.fire_tick(),
        "hit_window_end": r.hit_window_end(),
        "combo_offset": r.combo_offset(),
        "raw": r.raw.to_vec(),
    })
}

fn skills(gsdata: &Path, out: &Path, decks: Option<&Path>) -> Result<()> {
    use pd_formats::skills::{field as f, parse_decks, tables, SkillTable};
    let d = std::fs::read(gsdata).context("reading gsdata")?;
    let t = SkillTable::parse(&d)?;
    let action = tables::action_table(&d)?;
    let reactions = tables::reaction_table(&d)?;
    let mut table = String::from("id   rec  school   capsule      cost dmg  rar  name                       note\n");
    let list: Vec<_> = t
        .skills
        .iter()
        .map(|s| {
            table += &format!(
                "{:4} {:4} {:8} {:12} {:4} {:4} {:3}  {:26} {}\n",
                s.skill_id,
                s.record,
                s.school(),
                s.capsule(),
                s.get(f::COST),
                s.get(f::STRENGTH),
                s.get(f::RARITY),
                s.name,
                s.note
            );
            // Action table rows (PROGRAM-RE.md "Action table"): s12 picks
            // the ground row, s13 the mid-air row; 0 = no row for that case.
            let row_json = |field: usize| -> serde_json::Value {
                let row = s.get(field) as usize;
                match action.get(row).filter(|_| row > 0) {
                    Some(r) => {
                        let mut j = action_row_json(r);
                        j["row"] = serde_json::json!(row);
                        j
                    }
                    None => serde_json::Value::Null,
                }
            };
            serde_json::json!({
                "id": s.skill_id, "record": s.record, "text_id": s.text_id, "name": s.name, "note": s.note,
                "registered": s.registered(), "school": s.school(), "capsule": s.capsule(), "rarity": s.get(f::RARITY),
                "cost": s.get(f::COST), "level_penalty": s.get(f::LEVEL_PENALTY), "cost_per_use": s.get(f::COST_PER_USE),
                "health_cost": s.get(f::HEALTH_COST), "requirement": s.get(f::REQUIREMENT), "requirement_amount": s.get(f::REQUIREMENT_AMOUNT),
                "strength_rule": s.get(f::STRENGTH_RULE), "strength": s.get(f::STRENGTH), "raw": s.raw.to_vec(),
                // Added 2026-09-27 (moves track M1, PROGRAM-RE.md): action-table
                // rows for the ground / mid-air cast, and the hidden s70-s73
                // fields (the next record's tail in our framing). Additive --
                // every field above is unchanged.
                // `chain`: the combo rows s14..s17 (`FUN_14025a3e0`: hit k >= 1 of a chain
                // uses s[14 + (k - 1) % n], n = the leading non-zero ones). Added by the
                // combos lane; empty for every skill that has none.
                "action": { "ground": row_json(f::GROUND_ROW), "air": row_json(f::AIR_ROW), "chain": (14..=17).map(|k| row_json(k)).take_while(|j| !j.is_null()).collect::<Vec<_>>() },
                "s70_73": s.s70_73.to_vec(),
            })
        })
        .collect();
    let decks = match decks {
        Some(p) => parse_decks(&std::fs::read(p).context("reading deckdata")?)?,
        None => Vec::new(),
    };
    for d in &decks {
        let names: Vec<_> = d.skills.iter().map(|&id| t.by_id(id).map_or_else(|| id.to_string(), |s| s.name.clone())).collect();
        table += &format!("\ndeck {:?}: {}", d.name, names.join(", "));
    }
    // Reaction table (PROGRAM-RE.md "Hit reactions"): only the rows that hold
    // data (as `state_table.py` does) -- most of the ~890 slots the file has
    // room for past 0x2B00C are unused padding, not real rows.
    let reaction_list: Vec<_> = reactions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.used())
        .map(|(i, r)| {
            serde_json::json!({
                "row": i,
                "kind": r.kind(),
                "effect": r.effect(),
                "sound": r.cast_sound(),
                "voice_group": r.voice_group(),
                "launch_speed": r.launch_speed(),
                "launch_angle": r.launch_angle(),
                "w": (0..8).map(|k| r.w(k)).collect::<Vec<_>>(),
                "raw": r.raw.to_vec(),
            })
        })
        .collect();
    let json = serde_json::json!({
        "version": t.version,
        "skills": list,
        "texts": t.texts.iter().map(|(id, n, note)| serde_json::json!({ "id": id, "name": n, "note": note })).collect::<Vec<_>>(),
        "decks": decks.iter().map(|d| serde_json::json!({ "name": d.name, "skills": d.skills.to_vec() })).collect::<Vec<_>>(),
        // Added 2026-09-27 (moves track M1): the hit-reaction table, shared by
        // every skill's s57-s61 (docs/PROGRAM-RE.md "Hit reactions").
        "reactions": reaction_list,
        // Added 2026-10-01 (lane castrules F2): the game-rule table (gsdata
        // 0x33040: health, Level range, minutes per rule), the Aura-speed
        // multipliers by number of schools (0x33034) and the eight preset Quick
        // arsenals (0x33804). docs/audits/P0-FRONTEND.md appendix B-D.
        "rules": pd_formats::skills::rule_table(&d)?
            .iter()
            .enumerate()
            .map(|(k, r)| serde_json::json!({ "rule": k, "health": r.health, "level_min": r.level_min, "level_max": r.level_max, "minutes": r.minutes }))
            .collect::<Vec<_>>(),
        "aura_handicap": pd_formats::skills::aura_handicaps(&d)?.to_vec(),
        "quick_decks": pd_formats::skills::quick_decks(&d)?
            .iter()
            .map(|(deck, kind)| serde_json::json!({ "name": deck.name, "skills": deck.skills.to_vec(), "kind": kind }))
            .collect::<Vec<_>>(),
    });
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(&json)?)?;
    std::fs::write(out.with_extension("txt"), &table)?;
    let registered = t.skills.iter().filter(|s| s.registered()).count();
    println!(
        "{}: format {}, {} skill records ({} collectable), {} texts, {} decks, {} action rows, {} reaction rows",
        out.display(),
        t.version,
        t.skills.len(),
        registered,
        t.texts.len(),
        decks.len(),
        action.iter().filter(|r| r.valid()).count(),
        reaction_list.len(),
    );
    Ok(())
}

fn scan(dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    let (mut ok, mut failed, mut tris) = (0, 0, 0usize);
    let mut tex_fail = 0;
    for f in &files {
        let res = Alr::load(f).and_then(|a| gltf::export_glb(&a, &gltf::ExportOptions { apply_transforms: true, ..Default::default() }));
        match res {
            Ok((_, s)) => {
                ok += 1;
                tris += s.triangles;
                tex_fail += s.textures_failed;
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {}: {e:#}", f.display());
            }
        }
    }
    println!("{ok} ALR files converted, {failed} failed, {tris} triangles, {tex_fail} textures not decoded");
    Ok(())
}

/// `pd_convert stages <Map dir> <cache dir>` â€” convert every stage `.alr` in
/// `Map dir` (model + collision) into `cache dir`, and write `cache/stages.json`
/// describing them: display name, whether it's a normal versus arena, and
/// (where we could establish it â€” see PIPELINE.md) which BGM track is its
/// music, converted to `cache/music/<id>.wav`.
///
/// Names come from `setup/stageset/stageset_en.dat` next to `Map/` (see
/// `pd_formats::stageset`) â€” the developer's own stage-select table, not a
/// guess from renders. Stages that aren't in that table (`st09` + its
/// `st09_01..08` pieces, `st32`) are still converted so they're playable for
/// inspection, but marked `"versus": false`.
fn stages(map_dir: &Path, cache_dir: &Path, source: CollisionSource) -> Result<()> {
    // Confirmed by matching BGM track basenames (their romanized Japanese
    // location names) to the internal codes in stageset_en.dat: "YojoPlant" =
    // st06Plant (Refinery family), "KiokuNoMachi" ("Memory Town") =
    // st07TownofMemory (Lane family), "Iseki" ("Ruins") = st08Ruin (Sein).
    // Palace = "ShoppingMall" and Strange City = "JuryokuHanten" ("gravity
    // reversal"): the exe's BGM table gives every track a stage-group byte
    // and these are the two remaining groups; the Palace is a ruined
    // shopping mall (escalators, shop shutters, cafe tables) and Strange
    // City has the flying objects (PIPELINE.md "Music").
    const MUSIC: &[(&str, &str)] = &[
        ("st03", "01_ShoppingMall.stx"),
        ("st13", "02_ShoppingMall1.stx"),
        ("st23", "03_ShoppingMall2.stx"),
        ("st05", "14_JuryokuHanten1.stx"),
        ("st15", "15_JuryokuHanten2.stx"),
        ("st25", "13_JuryokuHanten.stx"),
        ("st02", "05_Highway1.stx"),
        ("st12", "06_Highway2.stx"),
        ("st22", "04_Highway.stx"),
        ("st04", "08_Panorama1.stx"),
        ("st14", "07_Panorama.stx"),
        ("st24", "09_Panorama2.stx"),
        ("st06", "11_YojoPlant1.stx"),
        ("st16", "10_YojoPlant.stx"),
        ("st26", "12_YojoPlant2.stx"),
        ("st07", "16_KiokuNoMachi.stx"),
        ("st17", "17_KiokuNoMachi1.stx"),
        ("st08", "19_Iseki.stx"),
    ];

    let data_dir = map_dir.parent().unwrap_or(map_dir);
    let table = std::fs::read(data_dir.join("setup/stageset/stageset_en.dat")).ok().map(|b| pd_formats::stageset::parse(&b)).unwrap_or_default();
    let lookup = |id: &str| table.iter().find(|e| e.id == id);

    // Discover every real stage model (skip the `stNNcam.alr` camera files).
    let mut ids: Vec<String> = std::fs::read_dir(map_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("alr")))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .filter(|s| !s.ends_with("cam"))
        .collect();
    ids.sort();

    std::fs::create_dir_all(cache_dir)?;
    std::fs::create_dir_all(cache_dir.join("music"))?;
    let mut out = Vec::new();
    for id in &ids {
        let alr = map_dir.join(format!("{id}.alr"));
        let a = Alr::load(&alr).with_context(|| format!("loading {}", alr.display()))?;

        // Objects from the stage layout (.dat): placed into the glb, added to
        // the collision, described in <id>.props.json (+ <id>.props.glb library).
        let props = stage_props(&alr, &a);
        let mut opts = gltf::ExportOptions { apply_transforms: true, ..Default::default() };
        if let Some(p) = &props {
            opts.skip_models = p.skip_models.clone();
            opts.instances = p.instances.clone();
        }
        let lights = stage_lights(&alr);
        opts.lighting = lights.as_ref().map(|l| l.sets[0]);
        let (glb, s) = gltf::export_glb(&a, &opts)?;
        std::fs::write(cache_dir.join(format!("{id}.glb")), &glb)?;
        if let Some(l) = &lights {
            std::fs::write(cache_dir.join(format!("{id}.light.json")), serde_json::to_string_pretty(&l.to_json())?)?;
        }
        let bounds = serde_json::json!({ "min": s.core_min, "max": s.core_max, "full_min": s.bounds_min, "full_max": s.bounds_max });
        std::fs::write(cache_dir.join(format!("{id}.bounds.json")), bounds.to_string())?;

        let col_bytes = match &props {
            Some(p) => {
                write_props(&a, p, cache_dir, id, opts.lighting)?;
                let dat = std::fs::read(map_dir.join(format!("{id}.dat"))).ok();
                if source == CollisionSource::Mesh094 {
                    if let Some(cam) = stage_camcol(&a, p, dat.as_deref()) {
                        std::fs::write(cache_dir.join(format!("{id}.camcol")), cam)?;
                    }
                }
                stage_collision(&a, p, dat.as_deref(), source).1
            }
            None => pd_formats::collision::Collision::to_bytes(&gltf::world_triangles(&a)),
        };
        std::fs::write(cache_dir.join(format!("{id}.pdcl")), &col_bytes)?;
        write_surface_sidecar(map_dir, cache_dir, id);

        let entry = lookup(id);
        let (name, versus, note) = match entry {
            // st01 (Edgar's Dream) and st24 (Panorama Boss) have real names
            // and models but the original's own selectable flag (a 20-byte
            // record field in this same file, docs/MENUS.md "Stage table
            // fact") is 0 for exactly these two of the 19 named slots — the
            // original's versus stage select never offers them. Keep them
            // playable (for the viewer, inspection, or a future story/boss
            // mode) but hide them from versus.
            Some(e) if !e.dummy && (id == "st01" || id == "st24") => (e.name.clone(), false, format!("not versus-selectable in stageset_en.dat's selectable flag (code {})", e.code)),
            Some(e) if !e.dummy => {
                let note = if e.family.is_empty() || e.family == e.name {
                    format!("code {}", e.code)
                } else {
                    format!("{} family, code {}", e.family, e.code)
                };
                (e.name.clone(), true, note)
            }
            Some(_) => (id.clone(), false, "dummy slot in stageset_en.dat (no stage assigned)".to_string()),
            None if id == "st32" => (id.clone(), false, "not listed in stageset_en.dat; flat, unnamed test/practice arena".to_string()),
            None if id.starts_with("st09") => (id.clone(), false, "exploration hub piece (st09 family), not a versus arena".to_string()),
            None => (id.clone(), false, "not listed in stageset_en.dat".to_string()),
        };

        let music = MUSIC.iter().find(|(sid, _)| sid == id).map(|(_, track)| {
            let src = data_dir.join("BGM").join(track);
            let dst = cache_dir.join("music").join(format!("{id}.wav"));
            if let Ok(d) = std::fs::read(&src) {
                if let Ok(m) = stx::decode(&d) {
                    let _ = std::fs::write(&dst, stx::to_wav(&m));
                }
            }
            format!("music/{id}.wav")
        });

        println!(
            "{id}: {} models, {} draws, {} triangles, {} objects ({}), versus {versus}{}",
            s.models,
            s.draws,
            s.triangles,
            s.instances,
            props.as_ref().map(|p| p.layout.as_str()).unwrap_or("no layout"),
            if let Some(m) = &music { format!(", music {m}") } else { String::new() }
        );

        let mut j = serde_json::json!({ "id": id, "name": name, "versus": versus, "note": note });
        if let Some(m) = music {
            j["music"] = serde_json::Value::String(m);
        }
        out.push(j);
    }

    // Versus arenas first (matching the original stage-select grouping), then
    // everything else, both in `stNN` order.
    out.sort_by(|a, b| {
        let av = a["versus"].as_bool().unwrap_or(false);
        let bv = b["versus"].as_bool().unwrap_or(false);
        bv.cmp(&av).then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    std::fs::write(cache_dir.join("stages.json"), serde_json::to_string_pretty(&out)?)?;
    println!("wrote {} ({} versus)", cache_dir.join("stages.json").display(), out.iter().filter(|e| e["versus"].as_bool().unwrap_or(false)).count());
    Ok(())
}

/// A stage's per-surface footstep classes from the `.dat` header-0x94 block:
/// `<id>.surf.json` next to the `.pdcl` (COLLISION.md "The runtime footstep
/// class byte"). A JSON sidecar rather than a `.pdcl` trailer because the
/// 0x94 mesh is a different mesh from the `.pdcl` (simplified stage
/// collision vs ALR geometry, different triangle counts), so no per-triangle
/// array could align with the `.pdcl`. Stages without a usable `.dat`
/// (st09 hub pieces, st32, st00) get no sidecar. Never fails the conversion.
fn write_surface_sidecar(map_dir: &Path, cache_dir: &Path, id: &str) {
    let dat_path = map_dir.join(format!("{id}.dat"));
    let Ok(bytes) = std::fs::read(&dat_path) else { return };
    match pd_formats::surface::SurfaceMesh::parse(&bytes) {
        Ok(m) => {
            let doc = m.to_json(id);
            if std::fs::write(cache_dir.join(format!("{id}.surf.json")), serde_json::to_string(&doc).unwrap_or_default()).is_ok() {
                println!("{id}: surface sidecar: {} faces, {} verts, bad {}", m.faces, m.vertices, m.bad);
            }
        }
        Err(e) => println!("{id}: no surface sidecar ({e:#})"),
    }
}

/// Where a stage's own collision triangles come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CollisionSource {
    /// The stage `.dat` header-0x94 mesh minus class 0x1F faces, wound to
    /// its stored normals (COLLISION.md "The stage collision is the 0x94
    /// block"); falls back to `Alr` for a stage with no usable block.
    Mesh094,
    /// Every ALR render triangle (what the converter did until 2026-09-30;
    /// `--collision alr`, kept for A/B comparison).
    Alr,
    /// The 0x94 mesh as above PLUS the ALR render geometry's steep
    /// triangles (walls: rails, edge closers, machinery), tagged class
    /// `CLASS_RENDER_WALL`. NOT the program's collision and RETIRED as the
    /// shipped build (2026-10-03: the original's fighters stand and walk inside
    /// these walls, COLLISION.md "dat+walls vs the program"); kept for A/B
    /// (`--collision dat+walls`).
    DatPlusWalls,
}

/// `--collision alr|dat` anywhere in `args` (default: the 0x94 mesh).
fn collision_source(args: &[String]) -> CollisionSource {
    match args.iter().position(|x| x == "--collision").and_then(|i| args.get(i + 1)).map(String::as_str) {
        Some("alr") => CollisionSource::Alr,
        Some("dat+walls") => CollisionSource::DatPlusWalls,
        _ => CollisionSource::Mesh094,
    }
}

/// The ALR render geometry's steep triangles (rails, edge closers, machinery):
/// the legacy floor limit, so nothing that was ground before is a wall here.
/// NOT collision. They are what `--collision dat+walls` added, and what the
/// battle camera's wall pull-in saw until 2026-10-03 (`stage_camcol`).
fn steep_render_triangles(a: &Alr, p: &pd_formats::stagedat::StageProps) -> Vec<[glam::Vec3; 3]> {
    gltf::world_triangles_skip(a, &p.skip_models)
        .into_iter()
        .filter(|t| {
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            n.length() > 1e-6 && (n.y / n.length()).abs() < pd_formats::collision::WALKABLE_NORMAL_Y
        })
        .collect()
}

/// `<id>.camcol`: the camera-only occluders (presentation, never gameplay).
/// A `PDCL` triangle file (with the `OBJT` owner tags) of everything the
/// stage collision carried until 2026-10-03 that the program's collision does
/// not have, so the battle camera's wall pull-in sees what Laurence approved:
///
/// * the render's steep triangles (`dat+walls`; only with a usable 0x94 mesh,
///   otherwise the collision already is the render triangles), owner none;
/// * the model triangles of visible objects with no hull (st01's walls and
///   stairs, st04 / st05's signs), each owned by its object so it goes away
///   when the object breaks;
/// * the class-0x1F faces of object hulls (the stage's own 0x1F faces were
///   already dropped from the old file, so they are not here).
///
/// Nothing but the camera reads it (`pd_game` `cam_occluders.rs`); with the
/// program collision it is exactly the old camera collision. `None` when
/// there is nothing to add.
fn stage_camcol(a: &Alr, p: &pd_formats::stagedat::StageProps, dat: Option<&[u8]>) -> Option<Vec<u8>> {
    let usable = matches!(
        dat.map(pd_formats::surface::SurfaceMesh::parse_with_normals),
        Some(Ok((m, _))) if m.bad == 0 && !m.tris.is_empty()
    );
    let mut tris = Vec::new();
    let mut owners = Vec::new();
    if usable {
        tris = steep_render_triangles(a, p);
        owners = vec![pd_formats::collision::NO_OBJECT; tris.len()];
    }
    tris.extend_from_slice(&p.camera_extra);
    owners.extend_from_slice(&p.camera_extra_owner);
    for ((t, o), c) in p.collision.iter().zip(&p.collision_owner).zip(&p.collision_class) {
        if *c == pd_formats::collision::CLASS_HIDDEN {
            tris.push(*t);
            owners.push(*o);
        }
    }
    (!tris.is_empty()).then(|| pd_formats::collision::Collision::to_bytes_tagged(&tris, &owners))
}

/// A stage's collision with its objects: the stage triangles, then each
/// solid object's, tagged with the object's index (`OBJT` trailer) so the
/// game can switch a broken or thrown object's collision off.
///
/// The stage triangles are the `.dat` 0x94 mesh (see [`CollisionSource`]) --
/// the program's own stage collision -- when `dat` parses to a usable mesh;
/// the file then carries the `WIND` marker (signed floors, COLLISION.md).
/// Objects keep their own `OC00` hulls either way.
fn stage_collision(a: &Alr, p: &pd_formats::stagedat::StageProps, dat: Option<&[u8]>, source: CollisionSource) -> (Vec<[glam::Vec3; 3]>, Vec<u8>) {
    use pd_formats::collision::{CLASS_NONE, CLASS_RENDER_WALL};
    let mesh = match (source, dat) {
        (CollisionSource::Mesh094 | CollisionSource::DatPlusWalls, Some(d)) => match pd_formats::surface::SurfaceMesh::parse_with_normals(d) {
            Ok((m, normals)) if m.bad == 0 && !m.tris.is_empty() => Some(m.collision_triangles_classed(&normals)),
            Ok((m, _)) => {
                println!("warning: 0x94 mesh unusable ({} faces, {} bad): using the ALR render geometry", m.faces, m.bad);
                None
            }
            Err(e) => {
                println!("warning: 0x94 mesh: {e:#}: using the ALR render geometry");
                None
            }
        },
        _ => None,
    };
    let signed = mesh.is_some();
    let (mut tris, mut classes) = match mesh {
        Some((mut m, mut c)) => {
            if source == CollisionSource::DatPlusWalls {
                // Steep ALR triangles only (the legacy floor limit, so nothing
                // that was ground before becomes a wall or vice versa).
                let extra = steep_render_triangles(a, p);
                c.extend(std::iter::repeat_n(CLASS_RENDER_WALL, extra.len()));
                m.extend(extra);
            }
            (m, c)
        }
        None => {
            let t = gltf::world_triangles_skip(a, &p.skip_models);
            let c = vec![CLASS_NONE; t.len()];
            (t, c)
        }
    };
    let mut owners = vec![pd_formats::collision::NO_OBJECT; tris.len()];
    tris.extend_from_slice(&p.collision);
    owners.extend_from_slice(&p.collision_owner);
    classes.extend_from_slice(&p.collision_class);
    // The class trailer only when some triangle has a class (a stage with no
    // 0x94 mesh and no hulls stays byte-identical to the old export).
    let classes_out: &[u8] = if classes.iter().any(|c| *c != CLASS_NONE) { &classes } else { &[] };
    let bytes = pd_formats::collision::Collision::to_bytes_classed(&tris, &owners, signed, classes_out);
    (tris, bytes)
}

/// `Map/st00.alr`'s 24 model slots, in container order (docs/CAPSULE-
/// VISUALS.md "Program read 2026-09-28" -> "Where the capsule art lives",
/// the dev-name table read from `PDUWP.exe`'s `0x1403ee490`).
const ST00_SLOT_NAMES: [&str; 24] = [
    "cap01_birth",
    "cap02_birth2",
    "cap03_death",
    "cap04_braek",
    "cap_aura",
    "cap_att",
    "cap_def",
    "cap_era",
    "cap_sta",
    "cap_spe",
    "cap_env",
    "cap_e_aura",
    "cap_e_other",
    "cap_env_c",
    "hit_particle",
    "end_of_world",
    "cap_envc_a",
    "cap_envc_b",
    "cap_envc_c",
    "cap_envc_d",
    "cap_envc_e",
    "mdl_jump_emitter",
    "mdl_jump_particle2",
    "mdl_jump_glow",
];

/// `Map/st00.dat`'s per-slot animation length, in container order. Byte-
/// walked 2026-09-28 (`ST00` magic; +0x24 total file size; +0x28 a u32 slot
/// count, 24, matching `ST00_SLOT_NAMES`); then per slot: a u32 **block
/// size** (bytes of that slot's whole animation block, chunks + terminator),
/// followed by that many bytes -- one size-prefixed 0x5 chunk per animated
/// joint/track (same layout `anim::ID_ANIMATION` chunks use: id, size, then
/// length as an f32 at +8; `anim::read_animation` isn't reused because it
/// indexes a character `.alr`'s own offset table, which `st00.dat` doesn't
/// have), ended by one chunk whose id isn't 5. Walking all 24 slots this way
/// lands exactly on EOF (checked: last slot's block ends at the file's own
/// length), and slot 0's 15 tracks all read length 429 -- exactly docs/
/// CAPSULE-VISUALS.md's "Program read 2026-09-28" table value for
/// `cap01_birth`; slots 1-3 read 59/89/89, also matching that table. We only
/// take the max track length per slot (enough to time slot 0-3's playback in
/// `pd_game`); decoding the per-joint keyframe curves themselves (the
/// sparkle-cloud drift, the flare's fade/scale) was not attempted this pass
/// -- see CAPSULE-VISUALS.md's "Built" note.
fn st00_dat_frame_lengths(dat: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();
    if dat.len() < 0x2C || &dat[0..4] != b"ST00" {
        return out;
    }
    let u32_at = |p: usize| -> Option<u32> { dat.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())) };
    let f32_at = |p: usize| -> Option<f32> { dat.get(p..p + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())) };
    let Some(count) = u32_at(0x28) else { return out };
    let mut pos = 0x2Cusize;
    for _ in 0..(count as usize).min(ST00_SLOT_NAMES.len()) {
        let Some(block_size) = u32_at(pos) else { break };
        pos += 4;
        let block_end = pos + block_size as usize;
        if block_end > dat.len() {
            break;
        }
        let mut inner = pos;
        let mut len = 0f32;
        while inner + 12 <= block_end {
            let (Some(id), Some(size)) = (u32_at(inner), u32_at(inner + 4)) else { break };
            if id != pd_formats::anim::ID_ANIMATION || size < 12 {
                break;
            }
            len = len.max(f32_at(inner + 8).unwrap_or(0.0));
            inner += size as usize;
        }
        out.push(len);
        pos = block_end;
    }
    out
}

/// The light script of the stage whose `.alr` is at `alr_path`, from the
/// sibling `.dat` (FORMATS-STAGES.md "Lighting script").
fn stage_lights(alr_path: &Path) -> Option<pd_formats::stagedat::StageLights> {
    pd_formats::stagedat::parse_light_script(&std::fs::read(alr_path.with_extension("dat")).ok()?)
}

/// The objects of the stage whose `.alr` is at `alr_path`, from the sibling
/// `.dat` (None if there is no usable one).
fn stage_props(alr_path: &Path, a: &Alr) -> Option<pd_formats::stagedat::StageProps> {
    let dat_path = alr_path.with_extension("dat");
    let bytes = std::fs::read(&dat_path).ok()?;
    let dat = match pd_formats::stagedat::StageDat::parse(&bytes) {
        Ok(d) => d,
        Err(e) => {
            println!("warning: {}: {e:#}", dat_path.display());
            return None;
        }
    };
    let id = alr_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Some(pd_formats::stagedat::build_stage_props(a, &dat, &id))
}

/// `<id>.props.json` and `<id>.props.glb` (every object type's intact /
/// broken / damaged model at the origin, nodes `typeNN_<state>`).
fn write_props(a: &Alr, p: &pd_formats::stagedat::StageProps, cache_dir: &Path, id: &str, lighting: Option<pd_formats::stagedat::LightSet>) -> Result<()> {
    std::fs::write(cache_dir.join(format!("{id}.props.json")), serde_json::to_string_pretty(&p.json)?)?;
    let mut lib = Vec::new();
    for t in p.json["types"].as_array().into_iter().flatten() {
        for (state, key) in [("intact", "model"), ("broken", "broken_model"), ("damaged", "damaged_model")] {
            if let Some(m) = t[key].as_u64() {
                let name = format!("type{:02}_{state}", t["id"].as_u64().unwrap_or(0));
                lib.push(gltf::ModelInstance { model: m as usize, world_lh: glam::Mat4::IDENTITY, name });
            }
        }
    }
    let all: Vec<usize> = (0..a.models.len()).collect();
    // Lit as if standing upright (the placed copies in the stage glb are lit where they stand).
    let (glb, _) = gltf::export_glb(a, &gltf::ExportOptions { apply_transforms: true, skip_models: all, instances: lib, lighting, ..Default::default() })?;
    std::fs::write(cache_dir.join(format!("{id}.props.glb")), glb)?;
    Ok(())
}

/// `pd_convert stagedat <stNN.dat | Map dir>` â€” research probe for the stage
/// layout files: sections, object types (with the model/hull size check
/// that ties OB00 model indices to the ALR), placement statistics, how well
/// placements sit on the stage floor, and start points.
fn stagedat_probe(path: &Path, only: Option<&str>) -> Result<()> {
    use pd_formats::stagedat::StageDat;
    let files: Vec<PathBuf> = if path.is_dir() {
        let mut v: Vec<PathBuf> = std::fs::read_dir(path)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "dat")).collect();
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };
    let verbose = files.len() == 1 || only == Some("-v");
    for f in files {
        let bytes = std::fs::read(&f)?;
        let dat = match StageDat::parse(&bytes) {
            Ok(d) => d,
            Err(e) => {
                println!("== {}: {e:#}", f.display());
                continue;
            }
        };
        let id = f.file_stem().unwrap().to_string_lossy().to_string();
        let layouts: Vec<String> = dat.layouts.iter().map(|(t, l)| format!("{t}={}", l.len())).collect();
        println!("== {id}: {} types, layouts {layouts:?}, {} start sets, {} aux model names", dat.types.len(), dat.start_sets.len(), dat.aux_model_names.len());
        if dat.layouts.is_empty() {
            continue;
        }
        let a = Alr::load(&f.with_extension("alr")).ok();
        // Types: models + does the hull box match the model box? (ties the
        // OB00 model index to the right ALR model).
        let mut box_ok = 0;
        let mut box_total = 0;
        for (i, t) in dat.types.iter().enumerate() {
            let mbox = a.as_ref().zip(t.model).and_then(|(a, m)| {
                let mut tris = Vec::new();
                gltf::model_triangles(a, m as usize, glam::Mat4::IDENTITY, &mut tris);
                let pts: Vec<glam::Vec3> = tris.iter().flatten().map(|p| glam::Vec3::new(p.x, p.y, -p.z)).collect();
                let mn = pts.iter().fold(glam::Vec3::MAX, |a, p| a.min(*p));
                let mx = pts.iter().fold(glam::Vec3::MIN, |a, p| a.max(*p));
                (!pts.is_empty()).then_some((mn, mx))
            });
            let hbox = t.hull.as_ref().map(|h| (glam::Vec3::from(h.min), glam::Vec3::from(h.max)));
            let agree = match (mbox, hbox) {
                (Some(m), Some(h)) => {
                    box_total += 1;
                    let ext = (m.1 - m.0).max(glam::Vec3::splat(1.0));
                    let ok = ((m.0 - h.0).abs() / ext).max_element() < 0.35 && ((m.1 - h.1).abs() / ext).max_element() < 0.35;
                    box_ok += ok as usize;
                    if ok { "match" } else { "DIFFERENT" }
                }
                _ => "-",
            };
            if verbose {
                println!(
                    "  type {i:2} {:24} models {:?}/{:?}/{:?} hull {:>3} faces {} model box {:?} hull box {:?} {agree}",
                    t.name,
                    t.model,
                    t.broken_model,
                    t.damaged_model,
                    t.hull.as_ref().map(|h| h.faces.len()).unwrap_or(0),
                    if t.collapsed_hull.is_some() { "+collapsed" } else { "" },
                    mbox.map(|(a, b)| (a.round().to_array(), b.round().to_array())),
                    hbox.map(|(a, b)| (a.round().to_array(), b.round().to_array())),
                );
            }
        }
        println!("  model box vs hull box agree: {box_ok}/{box_total}");
        // Placements: rotation kinds, reserved field, class/kind histogram,
        // and height above the stage floor (without objects).
        let skip: Vec<usize> = dat.template_models().into_iter().map(|m| m as usize).collect();
        let col = a.as_ref().map(|a| {
            let tris = gltf::world_triangles_skip(a, &skip);
            pd_formats::collision::Collision::from_bytes(&pd_formats::collision::Collision::to_bytes(&tris)).unwrap()
        });
        for (tag, list) in &dat.layouts {
            let mut hist = std::collections::BTreeMap::<(u8, u8, u16), usize>::new();
            let (mut yaw_only, mut multi, mut reserved) = (0, 0, 0);
            let (mut on_floor, mut above, mut below, mut none) = (0, 0, 0, 0);
            // Floor under a placement: the stage, or another object's hull
            // (desks stand on st04's floor-section objects).
            let prop_tris: Vec<Vec<[glam::Vec3; 3]>> = list
                .iter()
                .map(|p| {
                    let ty = usize::try_from(p.type_id).ok().and_then(|t| dat.types.get(t));
                    match ty.and_then(|t| t.hull.as_ref()) {
                        Some(h) if ty.is_some_and(|t| t.model.is_some()) => pd_formats::stagedat::hull_triangles_rh(h, gltf::placement_lh(p.position, p.rotation)),
                        _ => Vec::new(),
                    }
                })
                .collect();
            let floor_under = |gx: f32, gz: f32, me: usize, max_y: f32| -> Option<f32> {
                // Game (x, z) -> exported (x, -z).
                let (x, z) = (gx, -gz);
                let mut best = col.as_ref().and_then(|c| c.ground_below(x, z, max_y));
                for (j, tris) in prop_tris.iter().enumerate().filter(|(j, _)| *j != me) {
                    let _ = j;
                    for t in tris {
                        if let Some(y) = ray_down(t, x, z).filter(|y| *y <= max_y) {
                            best = Some(best.map_or(y, |b: f32| b.max(y)));
                        }
                    }
                }
                best
            };
            for (pi, p) in list.iter().enumerate() {
                *hist.entry((p.class, p.kind, p.param)).or_default() += 1;
                if p.reserved != 0 {
                    reserved += 1;
                }
                if p.rotation[0].abs() > 1e-3 || p.rotation[2].abs() > 1e-3 {
                    multi += 1;
                    if verbose {
                        let name = usize::try_from(p.type_id).ok().and_then(|t| dat.types.get(t)).map(|t| t.name.as_str()).unwrap_or("?");
                        println!("    multi-axis rotation: {name} at {:?} rot {:?}", p.position, p.rotation);
                    }
                } else {
                    yaw_only += 1;
                }
                if col.is_some() {
                    // Lowest point of the placed hull (joint convention), else the origin.
                    let ty = usize::try_from(p.type_id).ok().and_then(|t| dat.types.get(t));
                    let m = gltf::placement_lh(p.position, p.rotation);
                    let base = ty
                        .and_then(|t| t.hull.as_ref())
                        .map(|h| h.vertices.iter().map(|v| m.transform_point3(glam::Vec3::from(*v)).y).fold(f32::MAX, f32::min))
                        .filter(|y| y.is_finite() && *y < f32::MAX)
                        .unwrap_or(p.position[1]);
                    match floor_under(p.position[0], p.position[2], pi, base + 3.0) {
                        Some(y) if (base - y).abs() <= 3.0 => on_floor += 1,
                        Some(y) if base > y => {
                            above += 1;
                            if verbose && only == Some("--floors") {
                                println!("    above floor by {:.1}: {} at {:?}", base - y, ty.map(|t| t.name.as_str()).unwrap_or("?"), p.position);
                            }
                        }
                        Some(_) => below += 1,
                        None => none += 1,
                    }
                }
            }
            // Rotation-order check on genuinely tipped objects (not the
            // (±pi, y, ±pi) upright pattern): which Euler order rests the
            // hull's lowest point on the floor below it?
            if col.is_some() && verbose && tag == "PS0/" {
                use glam::{EulerRot, Quat};
                let orders = [("ZYX (Rz·Ry·Rx, joints)", EulerRot::ZYX), ("XYZ", EulerRot::XYZ), ("YXZ", EulerRot::YXZ), ("YZX", EulerRot::YZX), ("XZY", EulerRot::XZY), ("ZXY", EulerRot::ZXY)];
                let near_flat = |a: f32| a.abs() < 1e-3 || (a.abs() - std::f32::consts::PI).abs() < 1e-3;
                let mut totals = vec![0f32; orders.len()];
                let mut counted = 0;
                for (pi, p) in list.iter().enumerate().filter(|(_, p)| !(near_flat(p.rotation[0]) && near_flat(p.rotation[2]))) {
                    let Some(t) = usize::try_from(p.type_id).ok().and_then(|t| dat.types.get(t)) else { continue };
                    let Some(h) = &t.hull else { continue };
                    let mut gaps = Vec::new();
                    for (_, ord) in orders.iter() {
                        let [rx, ry, rz] = p.rotation;
                        // glam's from_euler(order, a, b, c) applies a first-listed-outermost.
                        let q = match ord {
                            EulerRot::ZYX => Quat::from_euler(*ord, rz, ry, rx),
                            EulerRot::XYZ => Quat::from_euler(*ord, rx, ry, rz),
                            EulerRot::YXZ => Quat::from_euler(*ord, ry, rx, rz),
                            EulerRot::YZX => Quat::from_euler(*ord, ry, rz, rx),
                            EulerRot::XZY => Quat::from_euler(*ord, rx, rz, ry),
                            _ => Quat::from_euler(*ord, rz, rx, ry),
                        };
                        // Per hull vertex: height above the floor right under it;
                        // the object's gap is the smallest (most sunk) of those.
                        let gap = h
                            .vertices
                            .iter()
                            .map(|v| glam::Vec3::from(p.position) + q * glam::Vec3::from(*v))
                            .filter_map(|w| floor_under(w.x, w.z, pi, w.y + 15.0).map(|f| w.y - f))
                            .fold(f32::MAX, f32::min);
                        gaps.push(if gap == f32::MAX { f32::NAN } else { gap });
                    }
                    // Only objects that rest on something under at least one
                    // order count (others lean on walls or hang).
                    let best = gaps.iter().map(|g| g.abs()).fold(f32::MAX, f32::min);
                    if best < 5.0 {
                        counted += 1;
                        for (k, g) in gaps.iter().enumerate() {
                            totals[k] += g.abs().min(50.0);
                        }
                    }
                    println!("    tipped {:18} rot {:?}: gap per order {:?}{}", t.name, p.rotation.map(|x| (x * 100.0).round() / 100.0), gaps.iter().map(|g| (g * 10.0).round() / 10.0).collect::<Vec<_>>(), if best < 5.0 { "" } else { " (not resting, ignored)" });
                }
                for (k, (name, _)) in orders.iter().enumerate() {
                    println!("    order {name}: total |gap| {:.1} over {counted} resting tipped objects", totals[k]);
                }
            }
            println!("  {tag}: {} objects, yaw-only {yaw_only}, multi-axis {multi}, reserved!=0 {reserved}", list.len());
            println!("    (class, kind, param) counts: {hist:?}");
            if col.is_some() {
                println!("    lowest hull point vs floor (stage or other object): within 3 units {on_floor}, above {above}, below {below}, no floor {none}");
            }
        }
        for s in &dat.start_sets {
            let caps_ok = s.starts.iter().all(|p| {
                let dir = glam::Vec3::new(p.yaw.sin(), 0.0, p.yaw.cos());
                p.capsules.iter().enumerate().all(|(k, c)| (glam::Vec3::from(*c) - (glam::Vec3::from(p.position) + dir * 50.0 * (k + 1) as f32)).length() < 1.0)
            });
            println!(
                "  CP00 @0x{:X}: {} starts, capsules on a 50-unit line ahead: {caps_ok}{}",
                s.header_slot,
                s.starts.len(),
                if verbose { format!(" {:?}", s.starts.iter().map(|p| (p.position.map(|x| x.round()), (p.yaw * 100.0).round() / 100.0)).collect::<Vec<_>>()) } else { String::new() }
            );
        }
    }
    Ok(())
}

/// `pd_convert propcheck <stNN.alr> <cache dir>` — do players actually bump
/// into the objects? For every solid object in `<cache>/stNN.props.json`,
/// put a player-sized cylinder (radius 7, height 20) touching each of the
/// four sides of its box and compare the wall push-out with the generated
/// `stNN.pdcl` against the stage alone; also check the box top is standable.
fn propcheck(alr_path: &Path, cache: &Path) -> Result<()> {
    use pd_formats::collision::Collision;
    let id = alr_path.file_stem().unwrap().to_string_lossy().to_string();
    let a = Alr::load(alr_path)?;
    let props: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(cache.join(format!("{id}.props.json")))?)?;
    let with = Collision::from_bytes(&std::fs::read(cache.join(format!("{id}.pdcl")))?)?;
    let skip = stage_props(alr_path, &a).map(|p| p.skip_models).unwrap_or_default();
    let without = Collision::new(gltf::world_triangles_skip(&a, &skip));
    let (mut blocked, mut total, mut tops, mut boxes) = (0, 0, 0, 0);
    let v3 = |v: &serde_json::Value| glam::Vec3::new(v[0].as_f64().unwrap_or(0.0) as f32, v[1].as_f64().unwrap_or(0.0) as f32, v[2].as_f64().unwrap_or(0.0) as f32);
    for o in props["objects"].as_array().into_iter().flatten().filter(|o| o["solid"].as_bool() == Some(true)) {
        let (mn, mx) = (v3(&o["bbox_min"]), v3(&o["bbox_max"]));
        let c = (mn + mx) * 0.5;
        let size = mx - mn;
        if size.y < 6.0 || size.x.min(size.z) < 0.5 {
            continue;
        }
        boxes += 1;
        let mut obj_blocks = false;
        for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let edge = glam::Vec3::new(if dx > 0.0 { mx.x } else if dx < 0.0 { mn.x } else { c.x }, mn.y, if dz > 0.0 { mx.z } else if dz < 0.0 { mn.z } else { c.z });
            let p = edge + glam::Vec3::new(dx, 0.0, dz) * 4.0; // cylinder overlapping the side by 3 units
            let top = size.y.min(20.0) - 1.0;
            let d_with = with.push_out_of_walls(p, 7.0, 1.0, top).distance(p);
            let d_without = without.push_out_of_walls(p, 7.0, 1.0, top).distance(p);
            if d_with > d_without + 0.5 {
                obj_blocks = true;
            }
        }
        total += 1;
        blocked += obj_blocks as usize;
        if with.ground_below(c.x, c.z, mx.y + 1.0).is_some_and(|y| y > mx.y - 3.0) {
            tops += 1;
        }
    }
    println!("{id}: {blocked}/{total} solid objects push a touching player out; {tops}/{boxes} have a standable top");
    Ok(())
}

/// Height where a vertical line at (x, z) crosses triangle `t`, if it does.
fn ray_down(t: &[glam::Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = *t;
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a.y + l2 * b.y + l3 * c.y)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("alr")) {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod skill_and_effect_tests {
    use super::*;
    use crate::effects::is_sprite_emitter;

    #[test]
    fn sprite_emitter_kinds_widen_past_the_trail() {
        // FORMATS-EFFECTS.md's documented kinds: trail is one exact pair;
        // the sprite and puff kinds each have two variant second words, so
        // the filter must match on the first word alone for those two.
        assert!(is_sprite_emitter(pd_formats::effect::KIND_TRAIL));
        assert!(is_sprite_emitter([0xa07a_c685, 0x8891_f680])); // sprite, variant seen on Swarm of Moths
        assert!(is_sprite_emitter([0xa07a_c685, 0x0000_fcc0])); // sprite, the other variant
        assert!(is_sprite_emitter([0x90e7_fd3e, 0x7609_29ea])); // puff, variant seen on crawlers
        assert!(is_sprite_emitter([0x90e7_fd3e, 0x0000_00e9])); // puff, the other variant
        // An unrelated kind (e.g. the point-light emitter, 511 uses) must not match.
        assert!(is_sprite_emitter([0x4ca4_77d9, 0x0c35_0000])); // BLADE01, a sword's swing ribbon
        assert!(is_sprite_emitter([0x650e_9921, 0x0c35_0000])); // FLAIR01, a ray burst
        assert!(!is_sprite_emitter([0x893e_3f31, 0x124f_8000]));
    }

    #[test]
    fn action_row_json_carries_the_documented_fields() {
        // Laser's ground row (PROGRAM-RE.md): row 42, fire tick 13, ATK_LASER,
        // cast sound common/242. Built directly from a `TableRow`, without a
        // real gsdata file, to check the JSON shape `skills()` writes.
        let mut raw = [0i16; pd_formats::skills::tables::ROW_WORDS];
        raw[0] = 1;
        raw[2] = 42;
        raw[3] = 242;
        raw[4] = 1;
        raw[38] = 76; // ATK_LASER
        raw[43] = 13;
        let row = pd_formats::skills::tables::TableRow { raw };
        let j = action_row_json(&row);
        assert_eq!(j["effect"], 42);
        assert_eq!(j["cast_sound"], 242);
        assert_eq!(j["anim_name"], "ATK_LASER");
        assert_eq!(j["fire_tick"], 13);
    }

    #[test]
    fn character_source_prefers_the_colour_variant() {
        // The game never plays the base <id>.alr (PROGRAM-RE.md "Animation
        // files"); `character` should reach for the <id>0.alr sibling when
        // one exists, and fall back to the exact path otherwise.
        let dir = std::env::temp_dir().join("pd_convert_test_character_source");
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("pc00a.alr");
        let colour0 = dir.join("pc00a0.alr");
        let colour2 = dir.join("pc00a2.alr");
        std::fs::write(&base, b"base").unwrap();
        std::fs::write(&colour0, b"colour0").unwrap();
        std::fs::write(&colour2, b"colour2").unwrap();

        assert_eq!(resolve_character_source(&base, None), colour0);
        assert_eq!(resolve_character_source(&base, Some("base")), base);
        assert_eq!(resolve_character_source(&base, Some("2")), colour2);
        // No such colour sibling (e.g. `evpc00a`, or asking for colour 9): falls back.
        let no_variants = dir.join("evpc00a.alr");
        std::fs::write(&no_variants, b"only file").unwrap();
        assert_eq!(resolve_character_source(&no_variants, None), no_variants);
        assert_eq!(resolve_character_source(&base, Some("9")), base);

        std::fs::remove_dir_all(&dir).ok();
    }
}

/// `pd_convert cpu <Data dir> <cache dir>`: copies the computer opponent's files into `<cache>/com/` unchanged (they are read by
/// `pd_game::cpu` in their original formats, docs/FORMATS-CPU.md): `ai/eneprog006.ssb`, `param/enemyparam.epa`, and
/// `stNN/comdata*.cad` for the stage folders 02..08 (the only ones with nav data, FORMATS-CPU 5.1).
fn cpu_files(args: &[String]) -> Result<()> {
    let (src, dst) = match args {
        [s, d, ..] => (Path::new(s).join("com"), Path::new(d).join("com")),
        _ => anyhow::bail!("usage: pd_convert cpu <game Data dir> <cache dir>"),
    };
    let mut n = 0;
    let mut copy = |rel: &str| -> Result<()> {
        let from = src.join(rel);
        if from.exists() {
            let to = dst.join(rel);
            std::fs::create_dir_all(to.parent().unwrap())?;
            std::fs::copy(&from, &to)?;
            n += 1;
        }
        Ok(())
    };
    copy("ai/eneprog006.ssb")?;
    copy("param/enemyparam.epa")?;
    for st in 2..=8 {
        for v in 0..3 {
            copy(&format!("st{st:02}/comdata{v}.cad"))?;
        }
    }
    println!("cpu: copied {n} files into {}", dst.display());
    Ok(())
}
