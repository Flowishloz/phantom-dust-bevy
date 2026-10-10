//! Stage layout files `Map/stNN.dat` ("ST00"): where the arena's objects
//! (crates, railings, desks, jump zones, ...) stand, what each object type
//! looks like and collides with, and the player/capsule start points.
//!
//! Evidence and confidence for every field: `engine/docs/FORMATS-STAGES.md`.
//! Layout knowledge started from the community notes in Esper-RE `st00.h`
//! (GPL, read as reference only); everything here was re-established with
//! byte dumps across all stages (`pd_convert stagedat`).
//!
//! File shape: a 0x140-byte header of i32 offsets (-1 = absent). Every
//! section is preceded by a 4-byte tag ("PS0/", "PS01", "NM00", "OC00", ...)
//! and the header offset points just past the tag.
//!
//! - `PS0/` / `PS01` / `PS02`: placement lists (0x24 bytes per object).
//! - `NM00`: object type names, NUL-terminated, indexed by type id.
//! - `OB00`: one 0x38-byte record per type: models (intact / broken /
//!   damaged, indices into the stage ALR's model list) and offsets of its
//!   collision hull (`OC00`), collapsed-state hull (`OC01`), collapse
//!   animation (`OA01`) and effect blocks (`OE00`).
//! - `CP00` (six sets): start points, 0x50 bytes each: player position,
//!   facing, and three skill-capsule spawn points.

use crate::bytes::*;
use anyhow::{bail, Result};

/// Header offsets (bytes from file start). All are "offset just past the
/// section's 4-byte tag"; -1 means absent.
pub mod hdr {
    pub const PS01_OFFSET: usize = 0x0C;
    pub const PS02_OFFSET: usize = 0x10;
    pub const PS00_COUNT: usize = 0x30;
    pub const PS01_COUNT: usize = 0x38;
    pub const PS02_COUNT: usize = 0x3C;
    pub const NM00_OFFSET: usize = 0x5C;
    pub const NM00_COUNT: usize = 0x60;
    pub const OB00_OFFSET: usize = 0x90;
    /// The six CP00 start-point sets (0xB4 is always -1 in shipped files).
    pub const CP00_OFFSETS: [usize; 6] = [0xA4, 0xA8, 0xAC, 0xB0, 0xB8, 0xBC];
    pub const MD00_OFFSET: usize = 0xA0;
    /// The per-type sound table: one 16-byte record (8 i16) per NM00 type, read by
    /// `FUN_1401fc750` / `FUN_1401fc710` / `FUN_1401df340` as `*(dat + 0xF8) + type * 16`; -1 =
    /// none. Present when the offset is not -1 (st01, st07, st09 and st32 have none).
    pub const SOUND_OFFSET: usize = 0xF8;
    /// Header size; the `PS0/` list starts here.
    pub const SIZE: usize = 0x140;
}

pub const PLACEMENT_SIZE: usize = 0x24;
pub const TYPE_RECORD_SIZE: usize = 0x38;
pub const START_SIZE: usize = 0x50;

/// One object placed in the arena.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// Index into [`StageDat::types`].
    pub type_id: i32,
    /// Object class (byte +4): 2 = physical prop, 4 = trigger zone (jump
    /// zones, boundaries), 1 = fixture/sign, 3 = door/event (st09 hub).
    pub class: u8,
    /// Byte +5: behaviour code. Meaning partly unknown (see FORMATS-STAGES.md);
    /// for jump zones it is 16.
    pub kind: u8,
    /// u16 +6: parameter. Jump zones: launch strength (100..250);
    /// Sein (st08) collapsing blocks: 0..75 in steps of 5 (collapse order?).
    pub param: u16,
    /// u32 +8: always 0 in shipped files.
    pub reserved: u32,
    /// Game (left-handed) world position.
    pub position: [f32; 3],
    /// Euler X/Y/Z radians, same convention as ALR joints (R = Rz·Ry·Rx).
    pub rotation: [f32; 3],
}

/// Convex-ish collision hull of an object type, in object-local space.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hull {
    /// First byte of the face chunk (1 for most types; 6 on st06 contaB).
    pub flags: u8,
    pub centre: [f32; 3],
    pub radius: f32,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    /// Triangles as vertex indices, with each face's stored u16 (unknown;
    /// 0x77FC on most st06 faces, maybe a surface/material id) and normal.
    pub faces: Vec<HullFace>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HullFace {
    pub idx: [u16; 3],
    pub attr: u16,
    pub normal: [f32; 3],
    /// The face chunk's flags byte: the program's surface class of the face
    /// (COLLISION.md "Per-triangle class").
    pub class: u8,
}

/// One object type (NM00 name + OB00 record).
#[derive(Debug, Clone, Default)]
pub struct ObjectType {
    pub name: String,
    /// ALR model shown while intact (None = invisible: zones, triggers).
    pub model: Option<u32>,
    /// ALR model after it breaks apart (pieces / wreck).
    pub broken_model: Option<u32>,
    /// ALR model for the damaged/knocked state (same object bent or tilted).
    pub damaged_model: Option<u32>,
    pub hull: Option<Hull>,
    /// Hull after collapse (OC01; only on types with a collapse animation).
    pub collapsed_hull: Option<Hull>,
    /// File offset of the collapse animation (OA01 record), if any.
    pub collapse_anim_offset: Option<u32>,
    /// File offset of the type's effect blocks (OE00), if any.
    pub effects_offset: Option<u32>,
    /// All 14 raw i32 of the OB00 record (-1 = none).
    pub raw: [i32; 14],
    /// The type's row of the header-`0xF8` sound table (8 i16, -1 = none; all -1 when the
    /// stage has no table). READ (`FUN_1401fc750`): slot = the event passed by the object
    /// handlers (0 = damaged: -1 in every shipped file; 1 = broken / destroyed) -> a SOUND GROUP
    /// that `FUN_140201d30` turns into a common-bank id (`DAT_1402dc640[group * 2 + random bit]`).
    /// Slot 2 (`FUN_1401fc710`) is stored on the debris the object spawns; slot 4 is a collapse
    /// delay in seconds (`FUN_1401df340`, x 30 ticks; st08's blocks), not a sound.
    pub sound_slots: [i16; 8],
}

/// "No sound" in [`ObjectType::sound_slots`].
pub const NO_SOUND_SLOTS: [i16; 8] = [-1; 8];

/// A player start point with its three capsule spawn points.
#[derive(Debug, Clone, PartialEq)]
pub struct StartPoint {
    pub position: [f32; 3],
    /// Facing, radians about Y: the player faces (sin yaw, 0, cos yaw) in game
    /// (left-handed) space. In some sets (0xA4/0xB8) the capsules lie exactly
    /// 50/100/150 units that way; in others they are spread along paths.
    pub yaw: f32,
    /// Float after yaw (always 0 so far; pitch?).
    pub unk: f32,
    pub capsules: [[f32; 3]; 3],
    /// Remaining 6 u32 (0xFFFFFFFF, 0x80, 3, 0x18, 0, <pointer-like>): unknown.
    pub tail: [u32; 6],
}

/// One CP00 set (the game has six per stage; 4 starts each, one set of 1).
#[derive(Debug, Clone, PartialEq)]
pub struct StartSet {
    /// Header offset that pointed here (0xA4.. 0xBC).
    pub header_slot: usize,
    pub starts: Vec<StartPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct StageDat {
    /// Placement lists in file order: ("PS0/", ..), ("PS01", ..), ("PS02", ..).
    pub layouts: Vec<(String, Vec<Placement>)>,
    pub types: Vec<ObjectType>,
    pub start_sets: Vec<StartSet>,
    /// MD00: names of the extra (effect/scenery) models that follow the
    /// object models in the stage ALR, "NOUSE" for empty slots.
    pub aux_model_names: Vec<String>,
}

fn opt_off(v: i32, len: usize) -> Option<usize> {
    (v > 0 && (v as usize) < len).then_some(v as usize)
}

fn cstr(d: &[u8], off: usize) -> Result<(String, usize)> {
    let Some(n) = d.get(off..).and_then(|s| s.iter().position(|&b| b == 0)) else { bail!("unterminated string at 0x{off:X}") };
    Ok((String::from_utf8_lossy(&d[off..off + n]).into_owned(), off + n + 1))
}

/// MD00 aux model names: "MD00", u32 section size, u32 names size, u32 count, names.
fn read_md00(d: &[u8], off: usize, names: &mut Vec<String>) {
    if slice(d, off, 4).is_ok_and(|t| t == b"MD00") {
        let count = u32_at(d, off + 12).unwrap_or(0) as usize;
        let mut p = off + 16;
        for _ in 0..count.min(1024) {
            let Ok((s, next)) = cstr(d, p) else { break };
            names.push(s);
            p = next;
        }
    }
}

impl StageDat {
    /// Parse an "ST00" file. Area files ("AR0x", st09_01..08) and the
    /// old-layout st00.dat hold no versus objects and return an empty layout.
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < hdr::SIZE {
            bail!("stage .dat too small ({} bytes)", d.len());
        }
        if &d[0..3] == b"AR0" {
            // Area file: no objects, but the MD00 aux-model names are there too, with the
            // section offset in the dword at +0x0C (st09_07: 0x120). Without them the
            // effect-asset models (st09_07's `m_ray` light shafts: tall black-textured
            // strips meant for additive blending) were drawn as opaque black slabs.
            let mut out = Self::default();
            if let Ok(off) = i32_at(d, 0x0C) {
                if let Some(off) = opt_off(off, d.len()) {
                    read_md00(d, off, &mut out.aux_model_names);
                }
            }
            return Ok(out);
        }
        if &d[0..4] != b"ST00" {
            bail!("not a stage .dat (magic {:?})", &d[0..4]);
        }
        // +4 is the offset of the PS0/ list (always 0x140), or -1 when the
        // file has none (st24 only has PS02).
        let ps00_at = i32_at(d, 4)?;
        if ps00_at != hdr::SIZE as i32 && ps00_at != -1 {
            // st00.dat: an older, 0x28-byte header; no objects.
            return Ok(Self::default());
        }
        let h = |o: usize| i32_at(d, o);
        let mut out = Self::default();

        // Placement lists. PS0/ always starts right after the header.
        let lists = [
            ("PS0/", (ps00_at != -1).then_some(hdr::SIZE), h(hdr::PS00_COUNT)?),
            ("PS01", opt_off(h(hdr::PS01_OFFSET)?, d.len()), h(hdr::PS01_COUNT)?),
            ("PS02", opt_off(h(hdr::PS02_OFFSET)?, d.len()), h(hdr::PS02_COUNT)?),
        ];
        for (tag, off, count) in lists {
            let (Some(off), true) = (off, count > 0) else { continue };
            let mut v = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let e = off + i * PLACEMENT_SIZE;
                v.push(Placement {
                    type_id: i32_at(d, e)?,
                    class: u8_at(d, e + 4)?,
                    kind: u8_at(d, e + 5)?,
                    param: u16_at(d, e + 6)?,
                    reserved: u32_at(d, e + 8)?,
                    position: vec3_at(d, e + 12)?,
                    rotation: vec3_at(d, e + 24)?,
                });
            }
            out.layouts.push((tag.to_string(), v));
        }

        // Types: names + OB00 records.
        let n_types = h(hdr::NM00_COUNT)?.max(0) as usize;
        if let Some(mut p) = opt_off(h(hdr::NM00_OFFSET)?, d.len()) {
            let ob = opt_off(h(hdr::OB00_OFFSET)?, d.len());
            for i in 0..n_types {
                let (name, next) = cstr(d, p)?;
                p = next;
                let mut t = ObjectType { name, raw: [-1; 14], sound_slots: NO_SOUND_SLOTS, ..Default::default() };
                if let Some(ob) = ob {
                    for k in 0..14 {
                        t.raw[k] = i32_at(d, ob + i * TYPE_RECORD_SIZE + k * 4)?;
                    }
                    let m = |v: i32| (v >= 0 && v < 0x10000).then_some(v as u32);
                    let o = |v: i32| opt_off(v, d.len());
                    t.model = m(t.raw[0]);
                    t.broken_model = m(t.raw[1]);
                    t.damaged_model = m(t.raw[2]);
                    t.collapse_anim_offset = o(t.raw[4]).map(|v| v as u32);
                    t.hull = o(t.raw[6]).map(|off| parse_hull(d, off)).transpose()?;
                    t.collapsed_hull = o(t.raw[7]).map(|off| parse_hull(d, off)).transpose()?;
                    t.effects_offset = o(t.raw[9]).map(|v| v as u32);
                }
                // The sound table: it must hold a full 16-byte record for every type (the shipped
                // tables end exactly at end of file), else it is ignored rather than guessed at.
                if let Some(table) = opt_off(h(hdr::SOUND_OFFSET)?, d.len()) {
                    let at = table + i * 16;
                    if at + 16 <= d.len() {
                        for k in 0..8 {
                            t.sound_slots[k] = u16_at(d, at + k * 2)? as i16;
                        }
                    }
                }
                out.types.push(t);
            }
        }

        // Start points.
        for slot in hdr::CP00_OFFSETS {
            let Some(off) = opt_off(h(slot)?, d.len()) else { continue };
            if slice(d, off, 4)? != b"CP00" {
                continue;
            }
            let n = u16_at(d, off + 4)? as usize;
            let mut starts = Vec::with_capacity(n);
            for i in 0..n {
                let e = off + 8 + i * START_SIZE;
                let mut tail = [0u32; 6];
                for (k, t) in tail.iter_mut().enumerate() {
                    *t = u32_at(d, e + 0x38 + k * 4)?;
                }
                starts.push(StartPoint {
                    position: vec3_at(d, e)?,
                    yaw: f32_at(d, e + 12)?,
                    unk: f32_at(d, e + 16)?,
                    capsules: [vec3_at(d, e + 20)?, vec3_at(d, e + 32)?, vec3_at(d, e + 44)?],
                    tail,
                });
            }
            out.start_sets.push(StartSet { header_slot: slot, starts });
        }

        // MD00 aux model names: "MD00", u32 section size, u32 names size, u32 count.
        if let Some(off) = opt_off(h(hdr::MD00_OFFSET)?, d.len()) {
            read_md00(d, off, &mut out.aux_model_names);
        }
        Ok(out)
    }

    /// The layout used for versus rendering: `PS0/` if present, else the
    /// first list the file has (st24 only has `PS02`).
    pub fn default_layout(&self) -> Option<&(String, Vec<Placement>)> {
        self.layouts.first()
    }

    /// ALR indices of the MD00 aux models: the last `used` models of the file, where
    /// `used` = the names that are not "NOUSE" (the same rule `pd_convert stagefx`
    /// uses to export them). They are effect assets -- the per-object glow halos
    /// (`stNN_mdl_glow_*`: a black-to-orange gradient the game draws additively around
    /// an object), debris and hit sprites, tower/fall/particle emitters. The game draws
    /// them only through effect records, never as static stage geometry; drawn in
    /// place they covered st08 in huge black shapes (the gradient's black part drawn
    /// opaque, `docs/FORMATS-STAGES.md` "Aux models are not scenery").
    /// Empty when the count does not fit (more names than models, or an object
    /// template inside the aux range), so a mismatch can never hide real stage models.
    pub fn aux_models(&self, model_count: usize) -> Vec<usize> {
        let used = self.aux_model_names.iter().filter(|n| n.as_str() != "NOUSE").count();
        let Some(base) = model_count.checked_sub(used) else { return Vec::new() };
        if self.template_models().into_iter().any(|m| m as usize >= base && (m as usize) < model_count) {
            return Vec::new();
        }
        (base..model_count).collect()
    }

    /// Every ALR model index an object type uses (intact, broken, damaged).
    /// These are templates: the stage export must not also draw them at the
    /// origin.
    pub fn template_models(&self) -> std::collections::BTreeSet<u32> {
        self.types.iter().flat_map(|t| [t.model, t.broken_model, t.damaged_model]).flatten().collect()
    }
}

/// Short display name of a type: "st06_oilcanA_" -> "oilcanA".
pub fn short_name(name: &str) -> String {
    let s = name.trim_end_matches('_');
    let s = s.strip_prefix("st").map(|r| r.trim_start_matches(|c: char| c.is_ascii_digit())).unwrap_or(s);
    s.trim_start_matches('_').to_string()
}

/// Everything the converter needs to put a stage's objects into its glTF,
/// collision and `props.json`.
pub struct StageProps {
    /// Which placement list was used ("PS0/", "PS01" or "PS02").
    pub layout: String,
    /// Models to leave out of the static stage export (object templates).
    pub skip_models: Vec<usize>,
    /// One glTF instance per visible placed object (node name `propNNN_<short name>`).
    pub instances: Vec<crate::gltf::ModelInstance>,
    /// Right-handed collision triangles of the solid objects.
    pub collision: Vec<[glam::Vec3; 3]>,
    /// For each of `collision`'s triangles, the object (placement index) it belongs to.
    pub collision_owner: Vec<u32>,
    /// For each of `collision`'s triangles, its surface class (the hull chunk's
    /// flags byte; 0 for a model triangle standing in for a missing hull).
    pub collision_class: Vec<u8>,
    /// CAMERA-ONLY stand-ins (never collision): the model triangles of every
    /// visible object that has no hull, which the stage collision carried until
    /// 2026-10-03 and the battle camera's wall pull-in still treats as solid
    /// (`pd_game` `cam_occluders.rs`), with their owning object.
    pub camera_extra: Vec<[glam::Vec3; 3]>,
    /// For each of `camera_extra`'s triangles, the object it belongs to.
    pub camera_extra_owner: Vec<u32>,
    /// The `stNN.props.json` document.
    pub json: serde_json::Value,
}

/// A hull's triangles placed with `world_lh` (left-handed), in right-handed space.
pub fn hull_triangles_rh(h: &Hull, world_lh: glam::Mat4) -> Vec<[glam::Vec3; 3]> {
    let m = crate::gltf::to_rh(world_lh);
    h.faces
        .iter()
        .map(|f| f.idx.map(|i| m.transform_point3(crate::gltf::point_to_rh(h.vertices[i as usize]))))
        .collect()
}

fn aabb(tris: &[[glam::Vec3; 3]]) -> Option<([f32; 3], [f32; 3])> {
    let mut it = tris.iter().flatten();
    let first = *it.next()?;
    let (mn, mx) = it.fold((first, first), |(a, b), p| (a.min(*p), b.max(*p)));
    Some((mn.to_array(), mx.to_array()))
}

fn round3(v: [f32; 3]) -> serde_json::Value {
    serde_json::json!(v.map(|x| (x * 1000.0).round() / 1000.0))
}

/// Plan the stage's objects: glTF instances, collision and props.json.
/// `alr` is the stage model file the `.dat` belongs to.
pub fn build_stage_props(alr: &crate::alr::Alr, dat: &StageDat, stage_id: &str) -> StageProps {
    use serde_json::json;
    let mut skip_models: Vec<usize> = dat.template_models().into_iter().map(|m| m as usize).filter(|&m| m < alr.models.len()).collect();
    // The MD00 aux models are effect assets, not scenery (see `StageDat::aux_models`).
    skip_models.extend(dat.aux_models(alr.models.len()));
    skip_models.sort_unstable();
    skip_models.dedup();
    let (layout, placements) = match dat.default_layout() {
        Some((t, p)) => (t.clone(), p.as_slice()),
        None => (String::new(), &[][..]),
    };

    let mut instances = Vec::new();
    let mut collision = Vec::new();
    let mut collision_owner = Vec::new();
    let mut collision_class = Vec::new();
    let mut camera_extra = Vec::new();
    let mut camera_extra_owner = Vec::new();
    let mut objects = Vec::new();
    for (i, p) in placements.iter().enumerate() {
        let ty = usize::try_from(p.type_id).ok().and_then(|t| dat.types.get(t));
        let world = crate::gltf::placement_lh(p.position, p.rotation);
        let name = ty.map(|t| short_name(&t.name)).unwrap_or_else(|| format!("type{}", p.type_id));
        let node = format!("prop{i:03}_{name}");
        let model = ty.and_then(|t| t.model).map(|m| m as usize).filter(|&m| m < alr.models.len() && !alr.models[m].draws.is_empty());
        let visible = model.is_some();
        if let Some(m) = model {
            instances.push(crate::gltf::ModelInstance { model: m, world_lh: world, name: node.clone() });
        }
        // Solid = anything visible WITH a hull. The hull is the OC00 block (the
        // game's own object collision); the program registers a collider for an
        // object only from that hull (`FUN_1401fbbc0` -> `FUN_1401d3ef0`, which
        // returns null for a null hull), so a visible type with no hull (st04's
        // sign boards, st05's sign, st01's wall / stair pieces) has no collision:
        // its model triangles size the prop (bbox) but are not in the `.pdcl`
        // (COLLISION.md "Per-triangle class").
        let hull_tris = ty.and_then(|t| t.hull.as_ref()).map(|h| hull_triangles_rh(h, world)).unwrap_or_default();
        let hull_class: Vec<u8> = if hull_tris.is_empty() { Vec::new() } else { ty.and_then(|t| t.hull.as_ref()).map(|h| h.faces.iter().map(|f| f.class).collect()).unwrap_or_default() };
        let shape_tris = if hull_tris.is_empty() {
            let mut v = Vec::new();
            if let Some(m) = model {
                crate::gltf::model_triangles(alr, m, world, &mut v);
            }
            v
        } else {
            hull_tris
        };
        if visible && !hull_class.is_empty() {
            collision.extend_from_slice(&shape_tris);
            collision_owner.extend(std::iter::repeat_n(i as u32, shape_tris.len()));
            collision_class.extend_from_slice(&hull_class);
        } else if visible {
            camera_extra.extend_from_slice(&shape_tris);
            camera_extra_owner.extend(std::iter::repeat_n(i as u32, shape_tris.len()));
        }
        let bbox = aabb(&shape_tris);
        let mut o = json!({
            "index": i,
            "node": if visible { json!(node) } else { json!(null) },
            "type": p.type_id,
            "name": ty.map(|t| t.name.clone()),
            "class": p.class,
            "class_meaning": match p.class { 1 => "fixture/sign (unconfirmed)", 2 => "physical prop", 3 => "door/event (st09 hub)", 4 => "trigger zone (jump zone, boundary)", _ => "unknown" },
            "kind": p.kind,
            "kind_meaning": if p.class == 4 && p.kind == 16 { "jump zone" } else { "behaviour code, unknown (see FORMATS-STAGES.md)" },
            "param": p.param,
            "param_meaning": if p.class == 4 && p.kind == 16 { "jump zone launch strength" } else if p.param != 0 { "unknown (st08: collapse order?)" } else { "unused" },
            "reserved": p.reserved,
            "position_game": round3(p.position),
            "rotation_game_euler_xyz": round3(p.rotation),
            "position": round3(crate::gltf::point_to_rh(p.position).to_array()),
            "matrix": crate::gltf::to_rh(world).to_cols_array().map(|x| (x * 10000.0).round() / 10000.0),
            "scale": [1.0, 1.0, 1.0],
            "solid": visible,
            "model": model,
            "broken_model": ty.and_then(|t| t.broken_model),
            "damaged_model": ty.and_then(|t| t.damaged_model),
        });
        if let Some((mn, mx)) = bbox {
            o["bbox_min"] = round3(mn);
            o["bbox_max"] = round3(mx);
        }
        objects.push(o);
    }

    let types: Vec<_> = dat
        .types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let lib = |state: &str, m: Option<u32>| m.map(|_| format!("type{i:02}_{state}"));
            json!({
                "id": i,
                "name": t.name,
                "short": short_name(&t.name),
                "model": t.model, "broken_model": t.broken_model, "damaged_model": t.damaged_model,
                "library_nodes": { "intact": lib("intact", t.model), "broken": lib("broken", t.broken_model), "damaged": lib("damaged", t.damaged_model) },
                "hull": t.hull.as_ref().map(|h| json!({ "flags": h.flags, "faces": h.faces.len(), "vertices": h.vertices.len(), "min_game": round3(h.min), "max_game": round3(h.max), "radius": h.radius })),
                "collapsed_hull": t.collapsed_hull.as_ref().map(|h| json!({ "faces": h.faces.len(), "min_game": round3(h.min), "max_game": round3(h.max) })),
                "collapse_anim_offset": t.collapse_anim_offset,
                "effects_offset": t.effects_offset,
                "ob00_raw": t.raw,
                "sound_slots": t.sound_slots,
            })
        })
        .collect();

    let start_sets: Vec<_> = dat
        .start_sets
        .iter()
        .map(|s| {
            json!({
                "header_slot": format!("0x{:X}", s.header_slot),
                "starts": s.starts.iter().map(|p| json!({
                    "position": round3(crate::gltf::point_to_rh(p.position).to_array()),
                    "facing": round3([p.yaw.sin(), 0.0, -p.yaw.cos()]),
                    "capsules": p.capsules.iter().map(|c| round3(crate::gltf::point_to_rh(*c).to_array())).collect::<Vec<_>>(),
                    "position_game": round3(p.position),
                    "yaw_game": p.yaw,
                    "unk": p.unk,
                    "tail": p.tail,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let other_layouts: Vec<_> = dat
        .layouts
        .iter()
        .filter(|(t, _)| *t != layout)
        .map(|(t, list)| {
            json!({ "layout": t, "objects": list.iter().map(|p| json!({
                "type": p.type_id, "class": p.class, "kind": p.kind, "param": p.param,
                "position_game": round3(p.position), "rotation_game_euler_xyz": round3(p.rotation),
            })).collect::<Vec<_>>() })
        })
        .collect();

    let json = json!({
        "stage": stage_id,
        "coords": "positions/matrix/bbox/facing are in the exported right-handed space (same as the stage glb and pdcl); *_game fields are the raw left-handed values",
        "layout": layout,
        "layouts_available": dat.layouts.iter().map(|(t, l)| json!({ "layout": t, "count": l.len() })).collect::<Vec<_>>(),
        "objects": objects,
        "types": types,
        "start_sets": start_sets,
        "other_layouts": other_layouts,
        "aux_models": dat.aux_model_names,
        "library": format!("{stage_id}.props.glb"),
    });
    StageProps { layout, skip_models, instances, collision, collision_owner, collision_class, camera_extra, camera_extra_owner, json }
}

/// Hull chunks: 12-byte header {u8 flags, i8 id, u8, u8, u32 body size, u32 0}.
/// id 3 = faces (10 f32 centre/radius/min/max, u16 count, 3 x u16, then
/// count x {u16 a, b, c, attr, f32 normal[3]}), id 8 = vertices (f32 x3 each),
/// id -1 (size 0) = end of this type's hull.
pub fn parse_hull(d: &[u8], mut off: usize) -> Result<Hull> {
    let mut hull = Hull::default();
    let mut base = 0u16;
    let mut pending_faces: Vec<HullFace> = Vec::new();
    for _ in 0..64 {
        let flags = u8_at(d, off)?;
        let id = u8_at(d, off + 1)? as i8;
        let size = u32_at(d, off + 4)? as usize;
        let body = off + 12;
        match id {
            3 => {
                if hull.faces.is_empty() && pending_faces.is_empty() {
                    hull.flags = flags;
                    hull.centre = vec3_at(d, body)?;
                    hull.radius = f32_at(d, body + 12)?;
                    hull.min = vec3_at(d, body + 16)?;
                    hull.max = vec3_at(d, body + 28)?;
                }
                let n = u16_at(d, body + 40)? as usize;
                for i in 0..n {
                    let e = body + 48 + i * 20;
                    pending_faces.push(HullFace {
                        idx: [u16_at(d, e)?, u16_at(d, e + 2)?, u16_at(d, e + 4)?],
                        attr: u16_at(d, e + 6)?,
                        normal: vec3_at(d, e + 8)?,
                        class: flags,
                    });
                }
            }
            8 => {
                let n = size / 12;
                for i in 0..n {
                    hull.vertices.push(vec3_at(d, body + i * 12)?);
                }
                // Faces index the vertex chunk that follows them.
                for mut f in pending_faces.drain(..) {
                    f.idx = f.idx.map(|i| i + base);
                    hull.faces.push(f);
                }
                base = hull.vertices.len() as u16;
            }
            -1 => break,
            other => bail!("unexpected hull chunk id {other} at 0x{off:X}"),
        }
        off = body + size;
    }
    hull.faces.retain(|f| f.idx.iter().all(|&i| (i as usize) < hull.vertices.len()));
    Ok(hull)
}

// ---------------------------------------------------------------------------
// Stage lighting: the small script embedded in every stNN.dat.
//
// Evidence (2026-09-27, FORMATS-STAGES.md "Lighting script"): the exe names
// `maplight%d.ssb` / class `pdBattleLight`; each .dat carries a short SSB
// byte-code block whose calls take exactly the constants the stage shaders
// read (`SD_vs`: cS0_VEC hemisphere axis, cS0_COLv / cS1_COLv sky / ground
// ambient, cDL_VEC / cDL_COL directional light). Recolour variants differ
// only in these numbers (st03 blue / st13 yellow-green / st23 warm), and the
// fog mode takes D3DFOGMODE values (1 exp, 2 exp2, 3 linear).

/// One light set (the script's call 0x0A, plus its call 0x05 fog). The
/// script defines four; see [`StageLights`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightSet {
    /// Hemisphere ("up") axis, game left-handed space, in the program's
    /// component order (see `read_lights`).
    pub hemi_axis: [f32; 3],
    /// Directional light travel direction (shader lights with N·−dir), same
    /// order.
    pub dir: [f32; 3],
    /// The script's 1st colour (`a[6]`). The program stores it at light block
    /// +8 = the **directional light colour** (`cDL_COL`), not a sky colour:
    /// the handler at 0x140206850 writes a[8] -> +0, a[7] -> +4, a[6] -> +8.
    /// The field keeps its old name for the cache's sake; use [`Self::dl_col`].
    pub sky: [u8; 3],
    /// The 2nd colour (`a[7]`, block +4): ambient for normals against the
    /// axis (`cS1`), see [`Self::hemi_down`].
    pub ground: [u8; 3],
    /// The 3rd colour (`a[8]`, block +0): ambient for normals along the axis
    /// (`cS0`), see [`Self::hemi_up`].
    pub sun: [u8; 3],
    pub fog: Fog,
}

impl LightSet {
    /// `cDL_COL`: the directional light's colour (block +8).
    pub fn dl_col(&self) -> [u8; 3] {
        self.sky
    }
    /// `cS0` (block +0): ambient for normals along the hemisphere axis.
    pub fn hemi_up(&self) -> [u8; 3] {
        self.sun
    }
    /// `cS1` (block +4): ambient for normals against the axis.
    pub fn hemi_down(&self) -> [u8; 3] {
        self.ground
    }
}

/// Fog: D3DFOGMODE (0 none, 1 exp, 2 exp2, 3 linear). Linear: `a` = end,
/// `b` = start (the first number is always the larger). Exp: `a` = density,
/// `b` ❓.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fog {
    pub mode: u8,
    pub a: f32,
    pub b: f32,
    pub color: [u8; 3],
}

/// The stage's light sets. Set 0 lights the stage, set 1 the characters
/// (same directions as set 0, brighter sky colour, no fog), set 2 the sky
/// dome (fog ~13,000 units out), set 3 is a neutral default in every file.
/// ⚠️ roles read from the numbers, see FORMATS-STAGES.md.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StageLights {
    pub sets: [LightSet; 4],
    /// First call 0x02 colour ❓ (clear / backdrop colour candidate).
    pub backdrop: [u8; 3],
}

impl StageLights {
    pub fn to_json(&self) -> serde_json::Value {
        let set = |s: &LightSet| {
            serde_json::json!({
                "hemi_axis": s.hemi_axis, "dir": s.dir, "sky": s.sky, "ground": s.ground, "sun": s.sun,
                "fog": { "mode": s.fog.mode, "a": s.fog.a, "b": s.fog.b, "color": s.fog.color },
            })
        };
        serde_json::json!({
            "note": "stage light script from the .dat (FORMATS-STAGES.md): dirs/axes in game left-handed space in the program's component order (vec_order zyx = already swapped from the file order); colours as the script lists them: sky = directional colour (block +8), sun = hemisphere up (+0), ground = hemisphere down (+4); set 0 stage, 1 characters, 2 sky, 3 default",
            "vec_order": "zyx",
            "stage": set(&self.sets[0]), "characters": set(&self.sets[1]), "sky": set(&self.sets[2]), "default": set(&self.sets[3]),
            "backdrop": self.backdrop,
        })
    }
}

/// A pushed script value: small ints (op 0x0E, i16) or 32-bit raw values
/// (op 0x0F, a u16 copy of the low half then the u32).
#[derive(Debug, Clone, Copy)]
enum Push {
    Small(i16),
    Word(u32),
}

impl Push {
    fn f(self) -> f32 {
        match self {
            Push::Small(v) => v as f32,
            Push::Word(u) => f32::from_bits(u),
        }
    }
    /// 0x00RRGGBB colour.
    fn rgb(self) -> [u8; 3] {
        let u = match self {
            Push::Small(v) => v as u32,
            Push::Word(u) => u,
        };
        [(u >> 16) as u8, (u >> 8) as u8, u as u8]
    }
    fn int(self) -> i64 {
        match self {
            Push::Small(v) => v as i64,
            Push::Word(u) => u as i64,
        }
    }
}

/// Decode byte-code from `off`: pushes then calls `01 fn sub 20`. Stops at
/// the first unknown opcode.
fn decode_calls(d: &[u8], mut off: usize) -> Vec<(u8, Vec<Push>)> {
    let mut out = Vec::new();
    let mut stack = Vec::new();
    while off + 4 <= d.len() {
        match (d[off], d[off + 1]) {
            (0x0E, 0) => {
                stack.push(Push::Small(i16::from_le_bytes([d[off + 2], d[off + 3]])));
                off += 4;
            }
            (0x0F, 0) if off + 8 <= d.len() => {
                stack.push(Push::Word(u32::from_le_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]])));
                off += 8;
            }
            (0x01, f) if d[off + 3] == 0x20 => {
                out.push((f, std::mem::take(&mut stack)));
                off += 4;
            }
            _ => break,
        }
    }
    out
}

/// Find and read the light script in a `.dat`. The header's script offset
/// (0xD0) is -1 in several files that still carry the script, so it is found
/// by its first call (`01 0A 00 20`, preceded by exactly 10 pushes).
pub fn parse_light_script(d: &[u8]) -> Option<StageLights> {
    let mut from = 0;
    while let Some(q) = d[from..].windows(4).position(|w| w == [0x01, 0x0A, 0x00, 0x20]).map(|p| p + from) {
        for back in (40..=120).step_by(4) {
            let Some(s) = q.checked_sub(back) else { break };
            let calls = decode_calls(d, s);
            let Some((0x0A, args)) = calls.first() else { continue };
            // The first call must end exactly at q.
            let len: usize = args.iter().map(|p| if matches!(p, Push::Small(_)) { 4 } else { 8 }).sum();
            if args.len() != 10 || s + len != q {
                continue;
            }
            return Some(read_lights(&calls));
        }
        from = q + 1;
    }
    None
}

fn read_lights(calls: &[(u8, Vec<Push>)]) -> StageLights {
    let mut l = StageLights::default();
    let mut got_backdrop = false;
    for (f, a) in calls {
        match (*f, a.len()) {
            (0x0A, 10) => {
                let i = a[9].int();
                if let Some(s) = usize::try_from(i).ok().and_then(|i| l.sets.get_mut(i)) {
                    // The program's handler for this call (0x140206850) stores
                    // its arguments last-pushed-first: slot k = a[9 - k], and
                    // writes slots 4,5,6 to the light block's +0x14,+0x18,+0x1c
                    // (the directional light's x, y, z) and slots 7,8,9 to
                    // +0x20,+0x24,+0x28 (the hemisphere axis' x, y, z). So the
                    // script lists each vector as (z, y, x): the light's world
                    // x is the script's THIRD float. (Before 2026-10-01 this
                    // read them in file order, which mirrored every light
                    // across the x = z diagonal; docs/FORMATS-STAGES.md.)
                    s.hemi_axis = [a[2].f(), a[1].f(), a[0].f()];
                    s.dir = [a[5].f(), a[4].f(), a[3].f()];
                    s.sky = a[6].rgb();
                    s.ground = a[7].rgb();
                    s.sun = a[8].rgb();
                }
            }
            (0x05, 5) => {
                let i = a[4].int();
                if let Some(s) = usize::try_from(i).ok().and_then(|i| l.sets.get_mut(i)) {
                    s.fog = Fog { mode: a[3].int().clamp(0, 255) as u8, a: a[0].f(), b: a[1].f(), color: a[2].rgb() };
                }
            }
            (0x02, 2) if !got_backdrop => {
                l.backdrop = a[1].rgb();
                got_backdrop = true;
            }
            _ => {}
        }
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first two calls of st03's script, byte for byte (set 0 light + fog).
    #[test]
    fn reads_light_script() {
        let mut d = vec![0xCDu8; 13];
        let f = |d: &mut Vec<u8>, x: u32| {
            d.extend_from_slice(&[0x0F, 0, x as u8, (x >> 8) as u8]);
            d.extend_from_slice(&x.to_le_bytes());
        };
        for x in [-0.08756f32, 0.9961, 0.007578, -0.4925, -0.6773, 0.5461] {
            f(&mut d, x.to_bits());
        }
        for c in [0x0069_7882u32, 0x0014_1b1e, 0x007d_8287] {
            f(&mut d, c);
        }
        d.extend_from_slice(&[0x0E, 0, 0, 0, 0x01, 0x0A, 0x00, 0x20]);
        f(&mut d, 1999f32.to_bits());
        f(&mut d, (-319f32).to_bits());
        f(&mut d, 0x000f_415a);
        d.extend_from_slice(&[0x0E, 0, 3, 0, 0x0E, 0, 0, 0, 0x01, 0x05, 0x02, 0x20]);
        let l = parse_light_script(&d).unwrap();
        let s = &l.sets[0];
        assert_eq!(s.sky, [105, 120, 130]);
        assert_eq!(s.ground, [20, 27, 30]);
        assert_eq!(s.sun, [125, 130, 135]);
        assert!((s.dir[1] + 0.6773).abs() < 1e-6);
        // 0x140206850 stores the script's vector floats last-pushed-first:
        // world x = the third float (0.5461 in st03's file order -0.4925,
        // -0.6773, 0.5461), world z = the first.
        assert!((s.dir[0] - 0.5461).abs() < 1e-6 && (s.dir[2] + 0.4925).abs() < 1e-6);
        assert!((s.hemi_axis[0] - 0.007578).abs() < 1e-6 && (s.hemi_axis[2] + 0.08756).abs() < 1e-6);
        // Colours: a[6] (first colour) is the directional light's, a[8] the
        // hemisphere "up" colour, a[7] the "down" colour (block +8 / +0 / +4).
        assert_eq!((s.dl_col(), s.hemi_down(), s.hemi_up()), ([105, 120, 130], [20, 27, 30], [125, 130, 135]));
        assert_eq!((s.fog.mode, s.fog.a, s.fog.b, s.fog.color), (3, 1999.0, -319.0, [15, 65, 90]));
        assert!(parse_light_script(b"no script here").is_none());
    }

    fn put_i32(v: &mut Vec<u8>, off: usize, x: i32) {
        v[off..off + 4].copy_from_slice(&x.to_le_bytes());
    }
    fn push_f32s(v: &mut Vec<u8>, xs: &[f32]) {
        for x in xs {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }

    /// A tiny synthetic stage: 2 placements, 2 types (one with a 1-triangle
    /// hull), 1 start set with 1 start point.
    fn synthetic() -> Vec<u8> {
        let mut d = vec![0xFFu8; hdr::SIZE];
        d[0..4].copy_from_slice(b"ST00");
        put_i32(&mut d, 4, hdr::SIZE as i32);
        d[0x13C..0x140].copy_from_slice(b"PS0/");
        put_i32(&mut d, hdr::PS00_COUNT, 2);
        // Placements.
        let place = |d: &mut Vec<u8>, ty: i32, class: u8, kind: u8, param: u16, p: [f32; 3], r: [f32; 3]| {
            d.extend_from_slice(&ty.to_le_bytes());
            d.extend_from_slice(&[class, kind]);
            d.extend_from_slice(&param.to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            push_f32s(d, &p);
            push_f32s(d, &r);
        };
        place(&mut d, 0, 2, 3, 0, [1.0, 2.0, 3.0], [0.0, 1.5, 0.0]);
        place(&mut d, 1, 4, 16, 250, [-5.0, 0.0, 5.0], [0.0; 3]);
        // Names.
        d.extend_from_slice(b"NM00");
        let nm = d.len();
        d.extend_from_slice(b"st99_boxA_\0st99_jump_\0");
        put_i32(&mut d, hdr::NM00_OFFSET, nm as i32);
        put_i32(&mut d, hdr::NM00_COUNT, 2);
        // Hull for type 0: one triangle.
        d.extend_from_slice(b"OC00");
        let oc = d.len();
        d.extend_from_slice(&[1, 3, 3, 0xFF]);
        d.extend_from_slice(&(48u32 + 20).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        push_f32s(&mut d, &[0.0, 1.0, 0.0, 2.0, -1.0, 0.0, -1.0, 1.0, 2.0, 1.0]);
        d.extend_from_slice(&[1, 0, 64, 0, 0, 0, 0, 0]);
        for x in [0u16, 1, 2, 0x77FC] {
            d.extend_from_slice(&x.to_le_bytes());
        }
        push_f32s(&mut d, &[0.0, 0.0, -1.0]);
        d.extend_from_slice(&[35, 8, 3, 0xFF]);
        d.extend_from_slice(&36u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        push_f32s(&mut d, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
        d.extend_from_slice(&[0xFF, 0xFF, 0xCC, 0xCC, 0, 0, 0, 0, 0xCC, 0xCC, 0xCC, 0xCC]);
        // OB00.
        d.extend_from_slice(b"OB00");
        let ob = d.len();
        let rec0 = [8, 20, 30, -1, -1, -1, oc as i32, -1, -1, -1, -1, -1, 0, -1];
        let rec1 = [-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 0, 0];
        for r in [rec0, rec1] {
            for x in r {
                d.extend_from_slice(&(x as i32).to_le_bytes());
            }
        }
        put_i32(&mut d, hdr::OB00_OFFSET, ob as i32);
        // CP00 with one start.
        let cp = d.len();
        d.extend_from_slice(b"CP00");
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&80u16.to_le_bytes());
        push_f32s(&mut d, &[10.0, 20.0, 30.0, 1.5707964, 0.0, 60.0, 20.0, 30.0, 110.0, 20.0, 30.0, 160.0, 20.0, 30.0]);
        for x in [u32::MAX, 0x80, 3, 0x18, 0, 7] {
            d.extend_from_slice(&x.to_le_bytes());
        }
        put_i32(&mut d, hdr::CP00_OFFSETS[0], cp as i32);
        d
    }

    #[test]
    fn parses_synthetic_stage() {
        let s = StageDat::parse(&synthetic()).unwrap();
        assert_eq!(s.layouts.len(), 1);
        let (tag, list) = &s.layouts[0];
        assert_eq!(tag, "PS0/");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(list[0].rotation[1], 1.5);
        assert_eq!((list[1].class, list[1].kind, list[1].param), (4, 16, 250));

        assert_eq!(s.types.len(), 2);
        assert_eq!(s.types[0].name, "st99_boxA_");
        assert_eq!((s.types[0].model, s.types[0].broken_model, s.types[0].damaged_model), (Some(8), Some(20), Some(30)));
        assert_eq!(s.types[1].model, None);
        let hull = s.types[0].hull.as_ref().unwrap();
        assert_eq!(hull.vertices.len(), 3);
        assert_eq!(hull.faces.len(), 1);
        assert_eq!(hull.faces[0].idx, [0, 1, 2]);
        assert_eq!(hull.faces[0].class, 1, "a face carries its chunk's flags byte as its surface class");
        assert_eq!(hull.faces[0].attr, 0x77FC);
        assert_eq!(hull.max, [1.0, 2.0, 1.0]);
        assert!(s.types[1].hull.is_none());
        assert_eq!(s.template_models().into_iter().collect::<Vec<_>>(), vec![8, 20, 30]);

        assert_eq!(s.start_sets.len(), 1);
        let st = &s.start_sets[0].starts[0];
        assert_eq!(st.position, [10.0, 20.0, 30.0]);
        assert_eq!(st.capsules[2], [160.0, 20.0, 30.0]);
        assert_eq!(st.tail[5], 7);
    }

    fn dat_with_aux(names: &[&str], templates: &[u32]) -> StageDat {
        StageDat {
            aux_model_names: names.iter().map(|n| n.to_string()).collect(),
            types: templates
                .iter()
                .map(|&m| ObjectType { name: String::new(), model: Some(m), broken_model: None, damaged_model: None, hull: None, collapsed_hull: None, collapse_anim_offset: None, effects_offset: None, raw: [-1; 14], sound_slots: NO_SOUND_SLOTS })
                .collect(),
            ..Default::default()
        }
    }

    /// The MD00 aux models are the last (non-NOUSE names) models of the file.
    #[test]
    fn aux_models_are_the_last_models_by_non_nouse_names() {
        let d = dat_with_aux(&["glow_a", "NOUSE", "glow_b", "NOUSE", "hit"], &[8, 9]);
        assert_eq!(d.aux_models(14), vec![11, 12, 13]);
        assert_eq!(d.aux_models(2), Vec::<usize>::new(), "more names than models: nothing is hidden");
        // An object template inside the supposed aux range means the count is off: hide nothing.
        let d = dat_with_aux(&["a", "b", "c"], &[12]);
        assert_eq!(d.aux_models(14), Vec::<usize>::new());
        assert_eq!(dat_with_aux(&[], &[8]).aux_models(14), Vec::<usize>::new());
    }

    /// Root cause of the "black shapes on st08" bug (lane FIXST08): the stage export drew the MD00
    /// aux models in place. On st08 those include the 30 `st08_mdl_glow_*` halos (a black-to-orange
    /// gradient texture meant for additive blending), drawn opaque: models 72-81 and 103-122.
    /// Real files; skipped without game files.
    #[test]
    fn st08_glow_halos_are_not_part_of_the_stage_export() {
        let dir = std::env::var_os("PD_GAME_FILES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Map"));
        let (Ok(alr), Ok(bytes)) = (crate::alr::Alr::load(&dir.join("st08.alr")), std::fs::read(dir.join("st08.dat"))) else {
            eprintln!("skipped: no game files at {}", dir.display());
            return;
        };
        let dat = StageDat::parse(&bytes).unwrap();
        let props = build_stage_props(&alr, &dat, "st08");
        for m in (72..=81).chain(103..=122) {
            assert!(props.skip_models.contains(&m), "glow model {m} must be skipped");
        }
        assert_eq!(dat.aux_model_names.iter().filter(|n| n.contains("mdl_glow")).count(), 31, "30 object halos + glow_dummy");
        assert!((0..8).all(|m| !props.skip_models.contains(&m)), "stage slots 0-7 are never skipped");
    }

    /// st09_07's black slabs (lane STAGEFIX): the area file's MD00 (offset at +0x0C) names its four
    /// effect models (`m_ray`, `m_neon`, `m_lamp`, glare); they must be skipped like st08's halos.
    /// Real files; skipped without game files.
    #[test]
    fn st09_07_area_file_aux_models_are_skipped() {
        let dir = std::env::var_os("PD_GAME_FILES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Map"));
        let (Ok(alr), Ok(bytes)) = (crate::alr::Alr::load(&dir.join("st09_07.alr")), std::fs::read(dir.join("st09_07.dat"))) else {
            eprintln!("skipped: no game files at {}", dir.display());
            return;
        };
        let dat = StageDat::parse(&bytes).unwrap();
        assert_eq!(dat.aux_model_names.iter().filter(|n| *n != "NOUSE").count(), 4);
        let props = build_stage_props(&alr, &dat, "st09_07");
        assert_eq!(props.skip_models, vec![10, 11, 12, 13]);
    }

    /// The header-0xF8 sound table: one 8 x i16 record per type; absent (-1) or short = all -1.
    #[test]
    fn sound_table_rows_follow_the_types() {
        let s = StageDat::parse(&synthetic()).unwrap();
        assert!(s.types.iter().all(|t| t.sound_slots == NO_SOUND_SLOTS), "no table in the synthetic file");
        let mut d = synthetic();
        let at = d.len();
        for row in [[-1i16, 17, -1, -1, -1, -1, -1, -1], [-1, 29, 36, -1, 20, -1, -1, -1]] {
            for x in row {
                d.extend_from_slice(&x.to_le_bytes());
            }
        }
        put_i32(&mut d, hdr::SOUND_OFFSET, at as i32);
        let s = StageDat::parse(&d).unwrap();
        assert_eq!(s.types[0].sound_slots, [-1, 17, -1, -1, -1, -1, -1, -1]);
        assert_eq!(s.types[1].sound_slots, [-1, 29, 36, -1, 20, -1, -1, -1]);
        // A table too short for its types is ignored, not half read.
        let mut short = d.clone();
        short.truncate(at + 16);
        let s = StageDat::parse(&short).unwrap();
        assert_eq!(s.types[0].sound_slots, [-1, 17, -1, -1, -1, -1, -1, -1]);
        assert_eq!(s.types[1].sound_slots, NO_SOUND_SLOTS);
    }

    /// st24 layout: no PS0/ (+4 = -1), the list is PS02 at 0x140.
    #[test]
    fn ps02_only_file_uses_ps02() {
        let mut d = synthetic();
        put_i32(&mut d, 4, -1);
        put_i32(&mut d, hdr::PS00_COUNT, -1);
        put_i32(&mut d, hdr::PS02_OFFSET, hdr::SIZE as i32);
        put_i32(&mut d, hdr::PS02_COUNT, 2);
        d[0x13C..0x140].copy_from_slice(b"PS02");
        let s = StageDat::parse(&d).unwrap();
        assert_eq!(s.layouts.len(), 1);
        assert_eq!(s.default_layout().unwrap().0, "PS02");
        assert_eq!(s.default_layout().unwrap().1[1].param, 250);
    }

    #[test]
    fn area_and_empty_files_have_no_objects() {
        let mut ar = vec![0u8; hdr::SIZE];
        ar[0..4].copy_from_slice(b"AR01");
        assert!(StageDat::parse(&ar).unwrap().layouts.is_empty());
        let mut old = vec![0u8; hdr::SIZE];
        old[0..4].copy_from_slice(b"ST00");
        old[4] = 0x28;
        assert!(StageDat::parse(&old).unwrap().types.is_empty());
        assert!(StageDat::parse(b"nope").is_err());
    }

    #[test]
    fn truncated_file_is_an_error_not_a_panic() {
        let mut d = synthetic();
        d.truncate(hdr::SIZE + 10);
        assert!(StageDat::parse(&d).is_err());
    }
}
