//! Capsule point-sprite clouds (`Map/st00.alr` slots 0, 2, 3: the
//! regeneration motes, the pickup burst, the erase burst) with their joint
//! animation from `Map/st00.dat` baked per frame.
//! docs/CAPSULE-VISUALS.md "Regeneration motes (2026-10-01)".
//!
//! What the program draws (all READ from `PDUWP.exe`, see that section):
//! - Each draw is a list of points (vertex format 0x18: `f32 x3` position then
//!   two `s16` UVs) attached to one joint (`IndexBuffer::transform_idx`).
//! - Its material hash decodes (base-40) to `vkPSPRITE`: the VS offsets each
//!   point into a screen-aligned quad, the PS multiplies `t0` (colour strip,
//!   sampled at the point's own UV + a per-draw scroll) by `t3` (the dot).
//! - The 0x13 chunk before each draw holds three floats: +0x08 size, +0x0C
//!   end frame, +0x10 start frame; the scroll is
//!   `(frame - start) / (end - start)` clamped at 0 (`FUN_140235da0`).
//! - Joints are posed from the slot's `st00.dat` tracks at the current frame.

use crate::alr::{Alr, ID_INDICES, ID_MATERIAL};
use crate::bytes::*;
use crate::pose::{self, Local, Order};
use anyhow::{bail, Result};
use glam::{Mat4, Vec3};

/// One animated joint's keys from `st00.dat` (any of the three may be empty).
#[derive(Debug, Clone, Default)]
pub struct DatTrack {
    pub joint: u16,
    pub length: f32,
    pub translation: Vec<(f32, [f32; 3])>,
    pub rotation: Vec<(f32, [f32; 3])>,
    pub scale: Vec<(f32, [f32; 3])>,
}

fn read_key(d: &[u8], off: usize, size: usize) -> Result<(f32, [f32; 3])> {
    match size {
        // u16 frame + three s16 angles (value / 32767 * 2 pi). Seen on every
        // rotation track of slots 0-3; first key of joint 3 equals its bind
        // rotation to 4 digits, which settled the layout.
        8 => {
            let f = u16_at(d, off)? as f32;
            let mut v = [0.0; 3];
            for (i, x) in v.iter_mut().enumerate() {
                *x = i16_at(d, off + 2 + i * 2)? as f32 / i16::MAX as f32 * std::f32::consts::TAU;
            }
            Ok((f, v))
        }
        // f32 frame + three f32 (scale keys; joint 1's first key equals its
        // bind scale).
        16 => Ok((f32_at(d, off)?, [f32_at(d, off + 4)?, f32_at(d, off + 8)?, f32_at(d, off + 12)?])),
        _ => bail!("unsupported key size {size}"),
    }
}

/// The tracks of slot `slot` in `st00.dat` (`ST00`, count at +0x28, then one
/// size-prefixed block per slot).
pub fn parse_slot_tracks(dat: &[u8], slot: usize) -> Result<Vec<DatTrack>> {
    if dat.len() < 0x2C || &dat[0..4] != b"ST00" {
        bail!("not an st00.dat");
    }
    let count = u32_at(dat, 0x28)? as usize;
    if slot >= count {
        bail!("slot {slot} of {count}");
    }
    let mut pos = 0x2C;
    for _ in 0..slot {
        pos += 4 + u32_at(dat, pos)? as usize;
    }
    let block = u32_at(dat, pos)? as usize;
    pos += 4;
    let end = pos + block;
    let mut out = Vec::new();
    while pos + 0x20 <= end && u32_at(dat, pos)? == 5 {
        let size = u32_at(dat, pos + 4)? as usize;
        if size < 0x20 {
            break;
        }
        let rsize = u16_at(dat, pos + 14)? as usize;
        let tcount = u32_at(dat, pos + 16)? as usize;
        let rcount = u32_at(dat, pos + 20)? as usize;
        let scount = u32_at(dat, pos + 24)? as usize;
        let tsize = u16_at(dat, pos + 30)? as usize;
        let mut t = DatTrack { joint: u16_at(dat, pos + 12)?, length: f32_at(dat, pos + 8)?, ..Default::default() };
        let mut k = pos + 0x20;
        for _ in 0..tcount {
            t.translation.push(read_key(dat, k, tsize)?);
            k += tsize;
        }
        for _ in 0..rcount {
            t.rotation.push(read_key(dat, k, rsize)?);
            k += rsize;
        }
        for _ in 0..scount {
            t.scale.push(read_key(dat, k, 16)?);
            k += 16;
        }
        out.push(t);
        pos += size;
    }
    Ok(out)
}

fn wrap_pi(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a < -std::f32::consts::PI {
        a += tau;
    }
    a
}

/// Linear sample of a key list (before the first key: the first; after the
/// last: the last); angles take the short way round.
fn sample(keys: &[(f32, [f32; 3])], frame: f32, angular: bool) -> Option<[f32; 3]> {
    let first = keys.first()?;
    let i = keys.iter().position(|k| k.0 > frame);
    let (a, b, f) = match i {
        Some(0) => (first, first, 0.0),
        Some(i) => {
            let (a, b) = (&keys[i - 1], &keys[i]);
            (a, b, ((frame - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0))
        }
        None => (keys.last()?, keys.last()?, 0.0),
    };
    let mut out = [0.0; 3];
    for c in 0..3 {
        out[c] = if angular { a.1[c] + wrap_pi(b.1[c] - a.1[c]) * f } else { a.1[c] + (b.1[c] - a.1[c]) * f };
    }
    Some(out)
}

/// Joint world matrices (file space, left-handed) at `frame`.
pub fn world_at(model: &crate::alr::Model, tracks: &[DatTrack], frame: f32) -> Vec<Mat4> {
    let mut locals = pose::bind_locals(model);
    for t in tracks {
        let Some(l) = locals.get_mut(t.joint as usize) else { continue };
        let l: &mut Local = l;
        if let Some(v) = sample(&t.translation, frame, false) {
            l.t = Vec3::from(v);
        }
        if let Some(v) = sample(&t.rotation, frame, true) {
            l.r = v;
        }
        if let Some(v) = sample(&t.scale, frame, false) {
            l.s = Vec3::from(v);
        }
    }
    pose::world_matrices(model, &locals, Order::GAME)
}

/// The 0x13 chunk before each draw of model `model`: `[size, end, start]`
/// (+0x08, +0x0C, +0x10). A draw with no 0x13 before it gets None.
pub fn draw_params(a: &Alr, model: usize) -> Vec<Option<[f32; 3]>> {
    let mut out = Vec::new();
    let mut n_model: isize = -1;
    let mut pending = None;
    for c in &a.chunks {
        match c.id {
            ID_MATERIAL => {
                n_model += 1;
                pending = None;
            }
            0x13 if n_model == model as isize => {
                pending = (|| -> Result<[f32; 3]> { Ok([f32_at(&a.data, c.offset + 8)?, f32_at(&a.data, c.offset + 12)?, f32_at(&a.data, c.offset + 16)?]) })().ok();
            }
            ID_INDICES if n_model == model as isize => out.push(pending.take()),
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct SpritePoint {
    /// Joint-local position, right-handed (z mirrored like every other export).
    pub pos: [f32; 3],
    /// The point's own UV into the colour strip (`s16 * 8 / 32767`, the VS's
    /// `c_SCALES.z`).
    pub uv: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct SpriteDraw {
    pub joint: u16,
    pub size: f32,
    pub end: f32,
    pub start: f32,
    pub points: Vec<SpritePoint>,
    /// Joint world matrix at integer frames 0..=frames, 3x4 row-major,
    /// conjugated into the right-handed convention (`F M F`, F = diag(1,1,-1)).
    pub matrices: Vec<[f32; 12]>,
}

#[derive(Debug, Clone)]
pub struct SpriteCloud {
    pub frames: f32,
    pub draws: Vec<SpriteDraw>,
}

/// Every point-sprite (format 0x18) draw of `model` with its joint matrices
/// baked at integer frames. `dat` is `st00.dat`; `slot` selects its block.
pub fn build_cloud(a: &Alr, slot: usize, dat: &[u8]) -> Result<SpriteCloud> {
    let model = a.models.get(slot).ok_or_else(|| anyhow::anyhow!("no model {slot}"))?;
    let tracks = parse_slot_tracks(dat, slot)?;
    let frames = tracks.iter().map(|t| t.length).fold(0.0, f32::max);
    let params = draw_params(a, slot);
    let n = frames.ceil() as usize;
    let worlds: Vec<Vec<Mat4>> = (0..=n).map(|f| world_at(model, &tracks, f as f32)).collect();
    let flip = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    let mut draws = Vec::new();
    for (di, d) in model.draws.iter().enumerate() {
        let Some(vb) = model.vertbufs.get(d.vertex_buf as usize) else { continue };
        if vb.format != 0x18 {
            continue;
        }
        let [size, end, start] = params.get(di).copied().flatten().unwrap_or([100.0, frames, 0.0]);
        let raw = a.resource(vb.data_ptr, vb.vertex_size as usize * vb.vertex_count as usize)?;
        let mut points = Vec::new();
        for &vi in &d.indices {
            let v = slice(raw, vi as usize * vb.vertex_size as usize, vb.vertex_size as usize)?;
            let p = vec3_at(v, 0)?;
            let uv = [i16_at(v, 12)? as f32 * 8.0 / i16::MAX as f32, i16_at(v, 14)? as f32 * 8.0 / i16::MAX as f32];
            points.push(SpritePoint { pos: [p[0], p[1], -p[2]], uv });
        }
        let matrices = worlds
            .iter()
            .map(|w| {
                let m = flip * w.get(d.transform_idx as usize).copied().unwrap_or(Mat4::IDENTITY) * flip;
                let c = m.to_cols_array_2d();
                // row-major 3x4 from glam's column-major
                [c[0][0], c[1][0], c[2][0], c[3][0], c[0][1], c[1][1], c[2][1], c[3][1], c[0][2], c[1][2], c[2][2], c[3][2]]
            })
            .collect();
        draws.push(SpriteDraw { joint: d.transform_idx, size, end, start, points, matrices });
    }
    Ok(SpriteCloud { frames, draws })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(rot_key: Option<(u16, [i16; 3])>, scale_key: Option<(f32, [f32; 3])>) -> Vec<u8> {
        let mut file = b"ST00".to_vec();
        file.resize(0x28, 0);
        file.extend_from_slice(&1u32.to_le_bytes());
        let mut chunk = Vec::new();
        let (rc, sc) = (rot_key.is_some() as u32, scale_key.is_some() as u32);
        let size = 0x20 + rc * 8 + sc * 16;
        chunk.extend_from_slice(&5u32.to_le_bytes());
        chunk.extend_from_slice(&size.to_le_bytes());
        chunk.extend_from_slice(&429.0f32.to_le_bytes());
        chunk.extend_from_slice(&3u16.to_le_bytes()); // joint
        chunk.extend_from_slice(&8u16.to_le_bytes()); // rot key size
        chunk.extend_from_slice(&0u32.to_le_bytes());
        chunk.extend_from_slice(&rc.to_le_bytes());
        chunk.extend_from_slice(&sc.to_le_bytes());
        chunk.extend_from_slice(&[0, 0]);
        chunk.extend_from_slice(&16u16.to_le_bytes());
        if let Some((f, v)) = rot_key {
            chunk.extend_from_slice(&f.to_le_bytes());
            for x in v {
                chunk.extend_from_slice(&x.to_le_bytes());
            }
        }
        if let Some((f, v)) = scale_key {
            chunk.extend_from_slice(&f.to_le_bytes());
            for x in v {
                chunk.extend_from_slice(&x.to_le_bytes());
            }
        }
        chunk.extend_from_slice(&0u32.to_le_bytes()); // terminator
        chunk.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        file.extend_from_slice(&chunk);
        file
    }

    #[test]
    fn reads_u16_frame_rotation_and_f32_scale_keys() {
        let dat = block(Some((359, [-15, -1170, 1564])), Some((359.0, [4.9, 1.8, 4.9])));
        let t = parse_slot_tracks(&dat, 0).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].joint, 3);
        let (f, v) = t[0].rotation[0];
        assert_eq!(f, 359.0);
        // joint 3 of cap01_birth: bind rotation (-0.002942, -0.224499, 0.299931)
        assert!((v[1] + 0.2245).abs() < 1e-3 && (v[2] - 0.2999).abs() < 1e-3, "{v:?}");
        assert_eq!(t[0].scale[0], (359.0, [4.9, 1.8, 4.9]));
    }

    #[test]
    fn sampling_holds_ends_and_interpolates() {
        let keys = vec![(10.0, [0.0, 0.0, 0.0]), (20.0, [10.0, 0.0, 0.0])];
        assert_eq!(sample(&keys, 0.0, false).unwrap()[0], 0.0);
        assert_eq!(sample(&keys, 15.0, false).unwrap()[0], 5.0);
        assert_eq!(sample(&keys, 99.0, false).unwrap()[0], 10.0);
        // angles take the short way round
        let a = vec![(0.0, [3.0, 0.0, 0.0]), (1.0, [-3.0, 0.0, 0.0])];
        let mid = sample(&a, 0.5, true).unwrap()[0];
        assert!(mid.abs() > 3.0, "{mid}");
    }
}
