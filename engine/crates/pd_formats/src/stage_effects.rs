//! Arena-object effects: the `EF00` blocks in `Map/stNN.dat` (`OE00` /
//! `OE01` per object type, one more for the whole stage at header 0x98).
//!
//! Evidence and confidence per field: docs/FORMATS-STAGES.md "Object
//! effects (OE00 / EF00)". Established 2026-09-27 with byte surveys of all
//! 30 stage files (every block parses; `size == count * 92` everywhere).
//!
//! Block: `"EF00"`, u16 record count, u16 byte size (= count * 92), then
//! the records. A record is one emitter/action, placed in the object's own
//! space (same left-handed space as the object model):
//!
//! | Off | Field |
//! |---|---|
//! | 0 | u8 kind (3 smoke, 8 smoke variant, 9 debris sprites, 10 model, 11 sound, 16 shake, ...) |
//! | 1 | u8 trigger: 0 always / ambient, 1 on hit, 2 on break, 3 scripted collapse |
//! | 2, 3 | u8 0xFF, 0 (a few stage-level records differ) |
//! | 4 | f32 x3 position |
//! | 16 | f32 x3 rotation (Euler XYZ, radians, R = Rz·Ry·Rx like placements) |
//! | 28 | f32 x3 scale |
//! | 40 | 12 kind-specific bytes |
//! | 52 | 10 kind-specific f32 |

use crate::bytes::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};

pub const RECORD_SIZE: usize = 92;
/// Header offset of the stage-wide EF00 block (ambient emitters).
pub const STAGE_BLOCK_OFFSET: usize = 0x98;

pub const KIND_SMOKE: u8 = 3;
pub const KIND_SMOKE_B: u8 = 8;
pub const KIND_DEBRIS: u8 = 9;
pub const KIND_MODEL: u8 = 10;
pub const KIND_SOUND: u8 = 11;
pub const KIND_SHAKE: u8 = 16;

pub const TRIGGER_ALWAYS: u8 = 0;
pub const TRIGGER_HIT: u8 = 1;
pub const TRIGGER_BREAK: u8 = 2;
pub const TRIGGER_COLLAPSE: u8 = 3;

/// MD00 (aux model table) slots: 0-19 "mdl" models (fragment sets,
/// particle systems), 20-39 "crs" debris sprites, 40-42 hit effects
/// (iron / stone / soil), 45 hit smoke.
pub const MD00_DEBRIS_BASE: usize = 20;
pub const MD00_HIT_IRON: usize = 40;
pub const MD00_HIT_STONE: usize = 41;
pub const MD00_HIT_SOIL: usize = 42;
pub const MD00_HIT_SMOKE: usize = 45;

#[derive(Debug, Clone, PartialEq)]
pub struct EffectRecord {
    pub kind: u8,
    pub trigger: u8,
    pub flags: [u8; 2],
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub bytes: [u8; 12],
    pub params: [f32; 10],
}

impl EffectRecord {
    /// Kind 9: the MD00 slot of the debris sprite it throws (20 + byte 0)
    /// and the pieces' life in frames (byte 2; 45/60/70/75/90/120).
    pub fn debris(&self) -> Option<(usize, u8)> {
        (self.kind == KIND_DEBRIS).then(|| (MD00_DEBRIS_BASE + self.bytes[0] as usize, self.bytes[2]))
    }

    /// Kind 10: the MD00 "mdl" slot it plays (byte 0, 0..19) and for how
    /// many frames (param 0; 0 = the model's own length).
    pub fn model(&self) -> Option<(usize, f32)> {
        (self.kind == KIND_MODEL).then(|| (self.bytes[0] as usize, self.params[0]))
    }

    /// Kinds 3 / 8: smoke colours (start RGBA, end RGBA) and blend byte.
    /// Bytes: 0, R G B (start), blend (0/1/2), alpha start, alpha end, R G B (end), 0, 0.
    pub fn smoke(&self) -> Option<([u8; 4], [u8; 4], u8)> {
        let b = &self.bytes;
        matches!(self.kind, KIND_SMOKE | KIND_SMOKE_B).then(|| ([b[1], b[2], b[3], b[5]], [b[7], b[8], b[9], b[6]], b[4]))
    }

    /// Kind 11: (sound id in the `common` bank, looped, delay in frames). Unconfirmed.
    pub fn sound(&self) -> Option<(u32, bool, f32)> {
        (self.kind == KIND_SOUND && self.params[1] >= 1.0).then(|| (self.params[1] as u32, self.bytes[0] == 1, self.params[0]))
    }

    pub fn to_json(&self) -> Value {
        let r = |v: f32| (v * 10000.0).round() / 10000.0;
        let mut o = json!({
            "kind": self.kind,
            "trigger": self.trigger,
            "flags": self.flags,
            "position_game": self.position.map(r),
            "rotation_game": self.rotation.map(r),
            "scale": self.scale.map(r),
            "bytes": self.bytes,
            "params": self.params.map(r),
        });
        if let Some((slot, life)) = self.debris() {
            o["debris"] = json!({ "md00_slot": slot, "life_frames": life });
        }
        if let Some((slot, frames)) = self.model() {
            o["model"] = json!({ "md00_slot": slot, "frames": frames });
        }
        if let Some((a, b, blend)) = self.smoke() {
            o["smoke"] = json!({ "start_rgba": a, "end_rgba": b, "blend": blend });
        }
        if let Some((id, looped, delay)) = self.sound() {
            o["sound"] = json!({ "common_id": id, "looped": looped, "delay_frames": delay });
        }
        o
    }
}

/// Parse one `EF00` block at `off` (the offset stored in OB00 word 9/10 or
/// header 0x98, which points at the "EF00" tag itself).
pub fn parse_block(d: &[u8], off: usize) -> Result<Vec<EffectRecord>> {
    if slice(d, off, 4)? != b"EF00" {
        bail!("no EF00 tag at 0x{off:X}");
    }
    let count = u16_at(d, off + 4)? as usize;
    let size = u16_at(d, off + 6)? as usize;
    if size != count * RECORD_SIZE {
        bail!("EF00 at 0x{off:X}: {count} records but {size} bytes");
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let r = off + 8 + i * RECORD_SIZE;
        let mut bytes = [0u8; 12];
        bytes.copy_from_slice(slice(d, r + 40, 12)?);
        let mut params = [0f32; 10];
        for (k, p) in params.iter_mut().enumerate() {
            *p = f32_at(d, r + 52 + k * 4)?;
        }
        out.push(EffectRecord {
            kind: u8_at(d, r)?,
            trigger: u8_at(d, r + 1)?,
            flags: [u8_at(d, r + 2)?, u8_at(d, r + 3)?],
            position: vec3_at(d, r + 4)?,
            rotation: vec3_at(d, r + 16)?,
            scale: vec3_at(d, r + 28)?,
            bytes,
            params,
        });
    }
    Ok(out)
}

/// A type's effects: `OE00` (OB00 word 9; what happens when it is hit /
/// breaks) and `OE01` (word 10; mostly ambient, trigger 0).
#[derive(Debug, Clone, Default)]
pub struct TypeEffects {
    pub main: Vec<EffectRecord>,
    pub extra: Vec<EffectRecord>,
}

/// Every object type's effect records, indexed like `StageDat::types`,
/// plus the stage-wide block. Offsets are the raw OB00 words.
pub fn parse_stage(d: &[u8], types: &[crate::stagedat::ObjectType]) -> Result<(Vec<TypeEffects>, Vec<EffectRecord>)> {
    let ok = |v: i32| (v > 0 && (v as usize) < d.len()).then_some(v as usize);
    let mut out = Vec::with_capacity(types.len());
    for t in types {
        let mut e = TypeEffects::default();
        if let Some(o) = ok(t.raw[9]) {
            e.main = parse_block(d, o)?;
        }
        if let Some(o) = ok(t.raw[10]) {
            e.extra = parse_block(d, o)?;
        }
        out.push(e);
    }
    let stage = match d.get(STAGE_BLOCK_OFFSET..STAGE_BLOCK_OFFSET + 4) {
        Some(b) => match ok(i32::from_le_bytes(b.try_into().unwrap())) {
            Some(o) if d.get(o..o + 4) == Some(b"EF00") => parse_block(d, o)?,
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    Ok((out, stage))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8, trigger: u8, pos: [f32; 3], bytes: [u8; 12], params: [f32; 10]) -> Vec<u8> {
        let mut v = vec![kind, trigger, 0xFF, 0];
        for x in pos.iter().chain(&[0.0; 3]).chain(&[1.0; 3]) {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(&bytes);
        for p in params {
            v.extend_from_slice(&p.to_le_bytes());
        }
        assert_eq!(v.len(), RECORD_SIZE);
        v
    }

    /// st06 bombeA's block, rebuilt: two smoke puffs and a debris burst.
    fn bombe() -> Vec<u8> {
        let mut d = b"EF00".to_vec();
        d.extend_from_slice(&3u16.to_le_bytes());
        d.extend_from_slice(&(3 * RECORD_SIZE as u16).to_le_bytes());
        let smoke = [0, 0x32, 0x33, 0x32, 0, 0xFF, 0, 0xB9, 0xC8, 0xB9, 0, 0];
        d.extend(record(3, 2, [0.0, 5.924, 0.0], smoke, [2.5, 0.05, -5.0, 0.15, 0.0, -2.0, 0.03, 0.0, 0.0, 0.0]));
        d.extend(record(3, 2, [0.0, 9.563, 0.0], smoke, [2.5, 0.05, -5.0, 0.15, 0.0, 2.0, 0.03, 0.0, 0.0, 0.0]));
        d.extend(record(9, 2, [0.0, 13.295, 0.0], [0, 0, 0x3C, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0.25, 0.05, 0.05, 0.0, 90.0, 1.0, -0.05, 0.0, 2.0, 1.0]));
        d
    }

    #[test]
    fn parses_records_and_reads_kinds() {
        let r = parse_block(&bombe(), 0).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!((r[0].kind, r[0].trigger), (3, TRIGGER_BREAK));
        assert_eq!(r[1].position, [0.0, 9.563, 0.0]);
        assert_eq!(r[0].smoke(), Some(([0x32, 0x33, 0x32, 0xFF], [0xB9, 0xC8, 0xB9, 0], 0)));
        assert_eq!(r[2].debris(), Some((20, 60)));
        assert_eq!(r[2].model(), None);
        assert_eq!(r[2].params[4], 90.0);
        let j = r[2].to_json();
        assert_eq!(j["debris"]["md00_slot"], 20);
    }

    #[test]
    fn model_and_sound_records() {
        let mut d = b"EF00".to_vec();
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&(2 * RECORD_SIZE as u16).to_le_bytes());
        d.extend(record(10, 2, [0.0; 3], [3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [45.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
        d.extend(record(11, 0, [0.0; 3], [1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0], [0.0, 79.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
        let r = parse_block(&d, 0).unwrap();
        assert_eq!(r[0].model(), Some((3, 45.0)));
        assert_eq!(r[1].sound(), Some((79, true, 0.0)));
    }

    #[test]
    fn bad_blocks_are_errors() {
        let mut d = bombe();
        assert!(parse_block(&d, 1).is_err(), "no tag");
        d[6] = 0; // size no longer count * 92
        assert!(parse_block(&d, 0).is_err());
        let mut short = bombe();
        short.truncate(50);
        assert!(parse_block(&short, 0).is_err(), "truncated");
    }
}
