//! Skill effects: `Data/Effect/eftNNNN.alr` (geometry + per-piece timing) and
//! `eftNNNN.eft` (which effect models play at which event slot).
//!
//! NNNN is the skill's text id (gsdata field s2). Layout notes, evidence and
//! open questions: docs/FORMATS-EFFECTS.md.

use crate::alr::{Alr, ID_INDICES, ID_MATERIAL};
use crate::bytes::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};

pub const ID_PIECE_PARAMS: u32 = 0x13;
pub const ID_EMITTER: u32 = 0x14;
pub const SLOTS: usize = 6;

/// The 412-byte `.eft` file.
#[derive(Debug, Clone)]
pub struct EffectDef {
    /// One word per effect model (0x1D or 0x1B; meaning unknown).
    pub model_flags: Vec<u32>,
    /// Event slot -> effect models it shows. Attack skills use 0-3,
    /// defences 0/4/5 (see FORMATS-EFFECTS.md for what each slot seems to be).
    pub slots: [Vec<u16>; SLOTS],
}

pub fn parse_eft(d: &[u8]) -> Result<EffectDef> {
    if d.len() != 412 {
        bail!("eft is {} bytes, expected 412", d.len());
    }
    let mut model_flags = Vec::new();
    for i in 0..12 {
        let w = u32_at(d, i * 20 + 4)?;
        if w != 0 {
            model_flags.push(w);
        }
    }
    let mut slots: [Vec<u16>; SLOTS] = Default::default();
    for (s, slot) in slots.iter_mut().enumerate() {
        let e = 244 + s * 28;
        if i16_at(d, e)? == 0 {
            continue;
        }
        for k in 2..14 {
            let m = i16_at(d, e + k * 2)?;
            if m >= 0 {
                slot.push(m as u16);
            }
        }
    }
    Ok(EffectDef { model_flags, slots })
}

/// Timing/colour/scale for one effect draw, from the 0x13 chunk right before
/// its 0x2 index buffer (32 words). Only fields with evidence are named.
#[derive(Debug, Clone)]
pub struct PieceParams {
    pub model: usize,
    /// Index of the draw within its model (same order as `Model::draws`).
    pub draw: usize,
    /// Flipbook grid (columns, rows); 1x1 = plain texture.
    pub grid: [f32; 2],
    /// Visible from `start` to `end` (frames).
    pub start: f32,
    pub end: f32,
    /// Word 7: spin (unconfirmed units).
    pub spin: f32,
    /// Words 12..15: ARGB colours. 12 = at start, 13 = at end.
    pub colors: [u32; 4],
    /// Words 16..18: scale added over the lifetime (unconfirmed).
    pub grow: [f32; 3],
    /// Words 24..27: flipbook frame range (first, last) twice.
    pub frames: [f32; 4],
    pub raw: [u32; 32],
    /// The draw's material flags (u16 at the 0x1 entry's +0x0C) and blend
    /// state word (u16 at +0x14): bit 0x10 of the flags turns blending on and
    /// the state word then picks the equation (`BlendState::decode`).
    pub mat_flags: u16,
    pub mat_state: u16,
    /// The draw's material shader name ("PDEFTBB01", "PDEFTOPT05", ...),
    /// from the first two words of its 0x1 material entry.
    pub shader: String,
    /// The material's four texture slots (u16 at its +0x18 / +0x1A / +0x1C /
    /// +0x1E = the pixel shader's `t0`..`t3`): indices into the effect's
    /// texture table. Each shader family samples its own subset
    /// (`PDEFTBB07` t1 + t2, `msEFTBB01S` t3, `PDMTEX01` t0 + t1, ...).
    pub mat_tex: [u16; 4],
    /// The material's four sampler address words (u16 at +0x3C ..+0x42;
    /// 0 = default 2, `FUN_1401a5e90`).
    pub mat_addr: [u16; 4],
    /// The draw's vertex buffer format (`alr::vertex_layout`), and what its
    /// vertices carry beyond a position and UV: "white" = every vertex colour
    /// is 0xFFFFFFFF (or the format has none), "varied" = they differ.
    pub vertex_format: u8,
    pub vertex_colour: &'static str,
    /// The draw's (or 0x14 emitter's) transform joint in the effect model.
    pub joint: Option<usize>,
    /// That joint's name ("RHAND", "RSWORD", "BODY2", "SHOT01", ...; None =
    /// unnamed). A named joint follows the character bone of the same name
    /// (docs/FORMATS-EFFECTS.md "Where pieces attach").
    pub joint_name: Option<String>,
    /// A skinned draw (`alr::VERTFMT_EFFECT_SKIN`, the school aura's wisps): its mesh with the two
    /// bone slots and weights of every vertex, which the aura class poses itself.
    pub skin: Option<SkinMesh>,
}

/// A skinned effect draw's mesh, in our right-handed space (z negated, winding flipped like the
/// glb export) with each vertex's two bone slots (the palette slot index: register `120 + 3k`
/// = slot `k`; 255 = none) and blend weights (`SWBOS`-style bytes / 255).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinMesh {
    pub positions: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub slots: Vec<[u8; 2]>,
    pub weights: Vec<[f32; 2]>,
    /// Triangle list, three per triangle.
    pub indices: Vec<u32>,
}

/// Read a skinned draw's mesh (None for any other vertex format, or an empty draw).
pub fn skin_mesh(a: &Alr, model: usize, draw: usize) -> Option<SkinMesh> {
    let m = a.models.get(model)?;
    let dr = m.draws.get(draw)?;
    let vb = m.vertbufs.get(dr.vertex_buf as usize)?;
    if vb.format != crate::alr::VERTFMT_EFFECT_SKIN || dr.palette.is_empty() {
        return None;
    }
    let verts = crate::alr::read_skin_vertices(a, vb).ok()?;
    let slot = |reg: u16| -> u8 {
        match reg.checked_sub(crate::alr::PALETTE_BASE_REGISTER) {
            Some(off) if off % 3 == 0 && ((off / 3) as usize) < dr.palette.len() => (off / 3) as u8,
            _ => 255,
        }
    };
    let n = verts.len();
    let mut indices = Vec::new();
    for t in crate::alr::triangulate(dr).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < n)) {
        indices.extend([t[0] as u32, t[2] as u32, t[1] as u32]);
    }
    if verts.is_empty() || indices.is_empty() {
        return None;
    }
    Some(SkinMesh {
        positions: verts.iter().map(|v| [v.pos[0], v.pos[1], -v.pos[2]]).collect(),
        uv: verts.iter().map(|v| v.uv).collect(),
        slots: verts.iter().map(|v| [slot(v.regs[0]), slot(v.regs[1])]).collect(),
        weights: verts.iter().map(|v| v.weights).collect(),
        indices,
    })
}

/// The game's packed names: 6 characters per u32 in base 40 (0 = end,
/// 1-10 = digits, 11-36 = letters, 37 = '_'), most significant first. Used
/// for material shader names (two words = up to 12 characters), which match
/// the game's `<name>_ps.cso` / `_vs.cso` shader files.
pub fn packed_name(words: &[u32]) -> String {
    let mut out = String::new();
    for &w in words {
        let mut v = w;
        let mut chars = [0u8; 6];
        for c in chars.iter_mut().rev() {
            *c = (v % 40) as u8;
            v /= 40;
        }
        for c in chars {
            match c {
                1..=10 => out.push((b'0' + c - 1) as char),
                11..=36 => out.push((b'A' + c - 11) as char),
                37 => out.push('_'),
                _ => {}
            }
        }
    }
    out
}

/// Material shaders that distort what is behind them (a bump-offset lookup
/// into a second texture the engine fills in: `PDEFTOPT05_ps` samples t0 as
/// signed offsets, then t2 at the offset UV) rather than add colour.
pub fn is_distortion_shader(name: &str) -> bool {
    name.starts_with("PDEFTOPT") || name.starts_with("YKREFR")
}

/// Material flag (u16 at the 0x1 entry's +0x0C) that turns blending on. Without
/// it the draw is opaque with only an alpha test (`FUN_1402a6010`: bit `0x10`
/// selects `FUN_14028e4e0`, else blending is switched off).
pub const MAT_BLEND_ON: u16 = 0x10;

/// A blend factor, as the program's table at `0x1402f77c0` lists them by the
/// 4-bit index in the state word (GL enum values; `FUN_140020420` turns them
/// into D3D11 factors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    DstColor,
    OneMinusDstColor,
    SrcAlphaSaturate,
    /// Table entries 11-14 (GL `CONSTANT_COLOR` ...): `FUN_140020420` maps
    /// them to D3D11's dual-source factors, fed by the pixel shader's second
    /// output (`blend_colour`, a per-draw constant we haven't read). Not seen
    /// in any skill effect; rendered as `One`.
    Constant,
}

/// The blend operation (table `0x1402f7798`, through `FUN_1400204f0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

/// One draw's blend equation: `result = src_factor * source (op) dst_factor *
/// destination`, from the material's flags and state word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlendState {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub op: BlendOp,
}

impl BlendFactor {
    /// Factor table `0x1402f77c0`, index = 4 bits of the state word.
    pub fn from_index(i: u16) -> Self {
        match i & 0xF {
            0 => Self::Zero,
            1 => Self::One,
            2 => Self::SrcColor,
            3 => Self::OneMinusSrcColor,
            4 => Self::SrcAlpha,
            5 => Self::OneMinusSrcAlpha,
            6 => Self::DstAlpha,
            7 => Self::OneMinusDstAlpha,
            8 => Self::DstColor,
            9 => Self::OneMinusDstColor,
            10 => Self::SrcAlphaSaturate,
            11..=14 => Self::Constant,
            _ => Self::One,
        }
    }
}

impl BlendOp {
    /// Operation table `0x1402f7798`: add, subtract, reverse subtract, min,
    /// max; entries 5 and 6 (`0xf006` / `0xf005`, "no addsigned" / "no
    /// revsubsigned") and 7 end up as plain add in the D3D11 port.
    pub fn from_index(i: u16) -> Self {
        match i & 7 {
            1 => Self::Subtract,
            2 => Self::ReverseSubtract,
            3 => Self::Min,
            4 => Self::Max,
            _ => Self::Add,
        }
    }
}

impl BlendState {
    /// `FUN_1402a6010`: blending is on only when the material flags carry
    /// `MAT_BLEND_ON`; then `FUN_14028e4e0` reads source factor = state bits
    /// 8-11, destination factor = bits 12-15, operation = bits 5-7.
    /// `None` = no blending (opaque, alpha test only).
    pub fn decode(flags: u16, state: u16) -> Option<Self> {
        (flags & MAT_BLEND_ON != 0).then(|| BlendState { src: BlendFactor::from_index(state >> 8), dst: BlendFactor::from_index(state >> 12), op: BlendOp::from_index(state >> 5) })
    }

    /// Plain `ONE, ONE` addition (state `0x1100`): the most common state in
    /// the skill effects.
    pub fn is_add(&self) -> bool {
        self.src == BlendFactor::One && self.dst == BlendFactor::One && self.op == BlendOp::Add
    }
}

/// A 0x14 chunk: a piece drawn by a game shader rather than from geometry
/// (trails, lights, ...). Kind = the shader-name hash pair. Its timing block
/// is the 0x13 before it, read like a piece's.
#[derive(Debug, Clone)]
pub struct Emitter {
    pub kind: [u32; 2],
    /// Material (in the model's 0x1 table) and the texture it points at.
    pub material: u16,
    pub texture: Option<u16>,
    pub params: PieceParams,
    /// Its joint's transform (right-handed, column-major; see
    /// `joint_matrix_rh`): where on the effect / bone the emitter sits.
    pub joint_matrix: Option<[f32; 16]>,
}

/// Emitter kind drawn as a trail behind a projectile (100 uses, all on
/// projectile skills; Laser's grows (1, 1, 20) = long and thin).
pub const KIND_TRAIL: [u32; 2] = [0xb8de_89ea, 0];

pub fn piece_params(a: &Alr) -> Result<Vec<PieceParams>> {
    Ok(read_pieces(a)?.0)
}

pub fn emitters(a: &Alr) -> Result<Vec<Emitter>> {
    Ok(read_pieces(a)?.1)
}

/// What `read_pieces` keeps of one 0x1 material entry (0x4C bytes).
#[derive(Clone, Copy, Default)]
struct MatInfo {
    flags: u16,
    state: u16,
    tex: [u16; 4],
    addr: [u16; 4],
}

fn read_pieces(a: &Alr) -> Result<(Vec<PieceParams>, Vec<Emitter>)> {
    let d = &a.data;
    let mut out = Vec::new();
    let mut emitters = Vec::new();
    let mut model: Option<usize> = None;
    let mut draw = 0usize;
    let mut pending: Option<[u32; 32]> = None;
    // Shader name, flags (+0x0C) and blend state (+0x14) of each material
    // entry of the current model.
    let mut shaders: Vec<String> = Vec::new();
    let mut mats: Vec<MatInfo> = Vec::new();
    for c in &a.chunks {
        match c.id {
            ID_MATERIAL => {
                model = Some(model.map_or(0, |m| m + 1));
                draw = 0;
                pending = None;
                let n = u16_at(d, c.offset + 8)? as usize;
                shaders = (0..n).map(|i| Ok(packed_name(&[u32_at(d, c.offset + 12 + i * 0x4C)?, u32_at(d, c.offset + 16 + i * 0x4C)?]))).collect::<Result<_>>()?;
                mats = (0..n)
                    .map(|i| {
                        let e = c.offset + 12 + i * 0x4C;
                        let mut m = MatInfo { flags: u16_at(d, e + 0x0C)?, state: u16_at(d, e + 0x14)?, ..Default::default() };
                        for k in 0..4 {
                            m.tex[k] = u16_at(d, e + 0x18 + k * 2)?;
                            m.addr[k] = u16_at(d, e + 0x3C + k * 2)?;
                        }
                        Ok(m)
                    })
                    .collect::<Result<_>>()?;
            }
            ID_PIECE_PARAMS if c.size >= 8 + 128 => {
                let mut raw = [0u32; 32];
                for (k, w) in raw.iter_mut().enumerate() {
                    *w = u32_at(d, c.offset + 8 + k * 4)?;
                }
                pending = Some(raw);
            }
            ID_EMITTER if c.size >= 8 + 20 => {
                if let (Some(m), Some(raw)) = (model, pending.take()) {
                    let material = u16_at(d, c.offset + 24)?;
                    // u16 after the material: the emitter's transform joint
                    // (eft0003: BLADE01 on joint 5 = RSWORD, PARTICLE01 on 2 = RHAND).
                    let joint = u16_at(d, c.offset + 26)? as usize;
                    let texture = a.models.get(m).and_then(|md| md.materials.get(material as usize)).map(|mt| mt.texture_idx);
                    let kind = [u32_at(d, c.offset + 16)?, u32_at(d, c.offset + 20)?];
                    let shader = shaders.get(material as usize).cloned().unwrap_or_default();
                    let mat = mats.get(material as usize).copied().unwrap_or_default();
                    emitters.push(Emitter { kind, material, texture, params: params(a, m, draw, raw, shader, mat, Some(joint)), joint_matrix: joint_matrix_rh(a, m, joint) });
                }
            }
            ID_INDICES => {
                if let (Some(m), Some(raw)) = (model, pending.take()) {
                    let mat = u16_at(d, c.offset + 64)? as usize;
                    let joint = u16_at(d, c.offset + 66)? as usize;
                    out.push(params(a, m, draw, raw, shaders.get(mat).cloned().unwrap_or_default(), mats.get(mat).copied().unwrap_or_default(), Some(joint)));
                }
                draw += 1;
            }
            _ => {}
        }
    }
    Ok((out, emitters))
}

fn params(a: &Alr, model: usize, draw: usize, raw: [u32; 32], shader: String, mat: MatInfo, joint: Option<usize>) -> PieceParams {
    let f = |k: usize| f32::from_bits(raw[k]);
    let joint = joint.filter(|&j| a.models.get(model).is_some_and(|m| j < m.joints.len()));
    // The draw's vertex buffer: format and whether its colours are all white.
    let vb = a.models.get(model).and_then(|m| m.draws.get(draw).and_then(|dr| m.vertbufs.get(dr.vertex_buf as usize)));
    let vertex_format = vb.map_or(0, |v| v.format);
    let vertex_colour = match vb.and_then(|v| a.read_vertex_light(v).ok()) {
        Some(l) if l.iter().any(|v| v.color.is_some_and(|c| c != [255; 4])) => "varied",
        _ => "white",
    };
    let joint_name = joint.and_then(|j| crate::alr::joint_name(a.models[model].joints[j].name)).filter(|n| !n.is_empty());
    PieceParams {
        model,
        draw,
        grid: [f(0), f(1)],
        start: f(2),
        end: f(3),
        spin: f(7),
        colors: [raw[12], raw[13], raw[14], raw[15]],
        grow: [f(16), f(17), f(18)],
        frames: [f(24), f(25), f(26), f(27)],
        raw,
        mat_flags: mat.flags,
        mat_state: mat.state,
        shader,
        mat_tex: mat.tex,
        mat_addr: mat.addr,
        vertex_format,
        vertex_colour,
        joint,
        joint_name,
        skin: skin_mesh(a, model, draw),
    }
}

/// The game's named attach points that stand for two character bones at
/// once (the program's pair table, `FUN_14018aaf0`): an effect joint with one
/// of these names gets both bones' matrices. Ours follows the first.
pub const BONE_PAIRS: [(&str, &str, &str); 14] = [
    ("LHRH", "LHAND", "RHAND"),
    ("LFRF", "LFOOT", "RFOOT"),
    ("RKRH", "RKATA", "RHAND"),
    ("LKLH", "LKATA", "LHAND"),
    ("B1RH", "BODY1", "RHAND"),
    ("B1LH", "BODY1", "LHAND"),
    ("B1RF", "BODY1", "RFOOT"),
    ("B1LF", "BODY1", "LFOOT"),
    ("LARA", "LADDHA", "RADDHA"),
    ("RKRA", "RKATA", "RADDHA"),
    ("B1RA", "BODY1", "RADDHA"),
    ("B1LA", "BODY1", "LADDHA"),
    ("NERA", "NECK", "RADDHA"),
    ("NELA", "NECK", "LADDHA"),
];

/// The joint's own transform (local = world: effect joints have no parents
/// in any file, and the program places each one on its own, `FUN_140189420`)
/// as a right-handed column-major matrix, like the glb's nodes.
pub fn joint_matrix_rh(a: &Alr, model: usize, joint: usize) -> Option<[f32; 16]> {
    let m = a.models.get(model)?;
    (joint < m.joints.len()).then(|| crate::gltf::to_rh(crate::gltf::joint_world_lh(&m.joints, joint)).to_cols_array())
}

/// One `SHOTnn` marker joint of an effect model (the program's shot spawn
/// points, `FUN_14018a310`; docs/FORMATS-EFFECTS.md "Shot markers").
#[derive(Debug, Clone, PartialEq)]
pub struct ShotMarker {
    /// The `nn` of `SHOTnn` (1-based, as the program's `name digits - 0x210`).
    pub index: u32,
    /// The effect model the joint lives in, and the event slots listing it.
    pub model: usize,
    pub slots: Vec<usize>,
    /// The joint's own translation, right-handed (z mirrored like every
    /// other export): the offset from the effect's matrix (the caster's
    /// root) at which the program spawns the shot. Effect joints have no
    /// parents, so local = world.
    pub position: [f32; 3],
}

/// The program's marker test (`FUN_14018a310`): a packed name whose top three
/// bytes are `0xB3D8C5xx` is `SHOT` + two digits; the index is the two digit
/// characters read as `10 * tens + units` (`DAT_1402f7670` characters).
pub fn shot_index(name: u32) -> Option<u32> {
    if name & 0xFFFF_FF00 != 0xB3D8_C500 {
        return None;
    }
    let s = crate::alr::joint_name(name)?;
    let digits = s.strip_prefix("SHOT")?;
    (digits.len() == 2).then(|| digits.parse::<u32>().ok()).flatten()
}

/// Every `SHOTnn` joint in the effect's models, ordered by model then joint
/// (the order the program meets them; its shot k is the marker with index k).
pub fn shot_markers(a: &Alr, def: &EffectDef) -> Vec<ShotMarker> {
    let mut out = Vec::new();
    for (mi, m) in a.models.iter().enumerate() {
        for (ji, j) in m.joints.iter().enumerate() {
            let Some(index) = shot_index(j.name) else { continue };
            let slots = (0..SLOTS).filter(|&s| def.slots[s].contains(&(mi as u16))).collect();
            let p = joint_matrix_rh(a, mi, ji).map_or([j.position[0], j.position[1], -j.position[2]], |c| [c[12], c[13], c[14]]);
            out.push(ShotMarker { index, model: mi, slots, position: p });
        }
    }
    out
}

/// Sidecar JSON written next to the effect's .glb. Draw nodes in the glb are
/// named `m{model}_d{draw}`.
pub fn to_json(def: &EffectDef, pieces: &[PieceParams], emitters: &[Emitter]) -> Value {
    json!({
        "version": 2,
        "emitters": emitters.iter().map(|e| json!({
            "kind": format!("{:08x}{:08x}", e.kind[0], e.kind[1]),
            "model": e.params.model,
            "texture": e.texture,
            "start": e.params.start,
            "end": e.params.end,
            "colors": e.params.colors,
            "grow": e.params.grow,
            "raw": e.params.raw,
            "mat_flags": e.params.mat_flags,
            "mat_state": e.params.mat_state,
            "mat_tex": e.params.mat_tex,
            "mat_addr": e.params.mat_addr,
            "joint": e.params.joint_name,
            "matrix": e.joint_matrix,
        })).collect::<Vec<_>>(),
        "model_flags": def.model_flags,
        "slots": def.slots,
        "pieces": pieces.iter().map(|p| {
            let mut v = json!({
            "node": format!("m{}_d{}", p.model, p.draw),
            "model": p.model,
            "grid": p.grid,
            "start": p.start,
            "end": p.end,
            "spin": p.spin,
            "colors": p.colors,
            "grow": p.grow,
            "frames": p.frames,
            "raw": p.raw,
            "mat_flags": p.mat_flags,
            "mat_state": p.mat_state,
            "mat_tex": p.mat_tex,
            "mat_addr": p.mat_addr,
            "vertex_format": p.vertex_format,
            "vertex_colour": p.vertex_colour,
            "shader": p.shader,
            "joint": p.joint_name,
            });
            if let Some(k) = &p.skin {
                v["skin"] = json!({ "positions": k.positions, "uv": k.uv, "slots": k.slots, "weights": k.weights, "indices": k.indices });
            }
            v
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eft_slots() {
        let mut d = vec![0u8; 412];
        d[4] = 0x1D;
        d[24] = 0x1B;
        // slot 0 -> models 0, 1; slot 3 -> model 1
        let put = |d: &mut Vec<u8>, off: usize, v: &[i16]| {
            for (k, x) in v.iter().enumerate() {
                d[off + k * 2..off + k * 2 + 2].copy_from_slice(&x.to_le_bytes());
            }
        };
        put(&mut d, 244, &[1, 0, 0, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1]);
        put(&mut d, 244 + 3 * 28, &[1, 0, 1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1]);
        let e = parse_eft(&d).unwrap();
        assert_eq!(e.model_flags, vec![0x1D, 0x1B]);
        assert_eq!(e.slots[0], vec![0, 1]);
        assert_eq!(e.slots[3], vec![1]);
        assert!(e.slots[1].is_empty());
    }

    #[test]
    fn blend_states_decode_like_the_programs_tables() {
        // FUN_1402a6010 / FUN_14028e4e0: bit 0x10 of the flags gates blending;
        // source = state >> 8 & 0xF, dest = state >> 12 & 0xF, op = state >> 5 & 7.
        let b = |flags, state| BlendState::decode(flags, state);
        let s = |src, dst, op| Some(BlendState { src, dst, op });
        use BlendFactor::*;
        // The observed states of the packed archive (P0-EFFECTS.md D1).
        assert_eq!(b(0x11, 0x1100), s(One, One, BlendOp::Add));
        assert_eq!(b(0x4011, 0x1100).unwrap().is_add(), true);
        assert_eq!(b(0x11, 0x1400), s(SrcAlpha, One, BlendOp::Add));
        assert_eq!(b(0x11, 0x1200), s(SrcColor, One, BlendOp::Add));
        assert_eq!(b(0x11, 0x1800), s(DstColor, One, BlendOp::Add));
        assert_eq!(b(0x11, 0x3000), s(Zero, OneMinusSrcColor, BlendOp::Add));
        assert_eq!(b(0x11, 0x2000), s(Zero, SrcColor, BlendOp::Add));
        assert_eq!(b(0x11, 0x5400), s(SrcAlpha, OneMinusSrcAlpha, BlendOp::Add));
        // The operation field: bits 5-7 (0x1240 = reverse subtract, ONE, ONE).
        assert_eq!(b(0x11, 0x1240), s(SrcColor, One, BlendOp::ReverseSubtract));
        assert_eq!(b(0x11, 0x1140), s(One, One, BlendOp::ReverseSubtract));
        assert_eq!(BlendOp::from_index(1), BlendOp::Subtract);
        assert_eq!(BlendOp::from_index(3), BlendOp::Min);
        assert_eq!(BlendOp::from_index(4), BlendOp::Max);
        // No 0x10 flag: blending off, whatever the state word says.
        assert_eq!(b(0x1, 0x1100), None);
        assert_eq!(b(0x9, 0x3000), None);
        assert_eq!(b(0x4001, 0x1800), None);
    }

    #[test]
    fn shot_markers_are_named_shot_and_two_digits() {
        // The program's test is `(name & 0xFFFFFF00) == 0xB3D8C500` (`FUN_14018a310`);
        // SHOT01 = 0xB3D8C5AA, SHOT10 = 0xB3D8C5D1 (checked by packing the names).
        assert_eq!(shot_index(0xB3D8_C5AA), Some(1));
        assert_eq!(shot_index(0xB3D8_C5AB), Some(2));
        assert_eq!(shot_index(0xB3D8_C5B1), Some(8));
        assert_eq!(shot_index(0xB3D8_C5D1), Some(10));
        // RHAND, an unnamed joint and a different prefix are not markers.
        assert_eq!(shot_index(0xADB0_7630), None);
        assert_eq!(shot_index(u32::MAX), None);
        assert_eq!(shot_index(0x7032_0580), None);
    }

    #[test]
    fn packed_names_decode() {
        // Material words seen in eft0001.alr: PDEFTBB01 and PDEFTOPT05.
        assert_eq!(packed_name(&[0xa0e2_eebc, 0x4967_0400]), "PDEFTBB01");
        assert_eq!(packed_name(&[0xa0e2_eec9, 0xa345_ff80]), "PDEFTOPT05");
        assert_eq!(packed_name(&[0xa0eb_13fa, 0x0668_a000]), "PDMTEX01");
        assert!(is_distortion_shader("PDEFTOPT05N"));
        assert!(!is_distortion_shader("PDEFTBB01R"));
    }
}
