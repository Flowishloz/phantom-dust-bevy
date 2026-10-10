//! Character table `setup/charaset/charaset_en.dat` (layout: community
//! modding wiki "charaset.dat"; checked against the file 2026-09-27).
//!
//! Header: u32 size, u32 count, u32 string-pool offset, u32 padding; then
//! `count` records of 0x4C bytes. Strings are offsets into the pool.

use crate::bytes::{u16_at, u32_at};
use anyhow::Result;

pub const RECORD: usize = 0x4C;

/// The dodge button's move (record +0x36): 750 = Jump, 751 = the ground
/// move (tumble or cartwheel). Wiki: "Jump Skill ID (750=Jump, 751=Cartwheel)".
pub const DODGE_JUMP: u16 = 750;
pub const DODGE_GROUND: u16 = 751;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Model folder, e.g. `pc01a`.
    pub model: String,
    /// English name (`Edgar`), when present.
    pub name: Option<String>,
    /// +0x0C: 180 Hero, 200 Meister / JD / Cuff Button, 120-160 the
    /// children -- looks like height in cm, but a fan guide calls JD and Cuff
    /// Button short. Unknown; not used.
    pub height: u16,
    /// +0x36 (`DODGE_JUMP` / `DODGE_GROUND`).
    pub dodge: u16,
    /// +0x3C: battle voice bank file.
    pub voice: Option<String>,
}

pub fn entries(d: &[u8]) -> Result<Vec<Entry>> {
    let count = u32_at(d, 4)? as usize;
    let pool = u32_at(d, 8)? as usize;
    let text = |off: u32| -> Option<String> {
        let rest = d.get(pool.checked_add(off as usize)?..)?;
        let end = rest.iter().position(|&c| c == 0)?;
        let s = std::str::from_utf8(&rest[..end]).ok()?;
        (!s.is_empty()).then(|| s.to_string())
    };
    let mut out = Vec::new();
    for i in 0..count {
        let e = 0x10 + i * RECORD;
        let Some(model) = text(u32_at(d, e)?) else { continue };
        out.push(Entry {
            model,
            name: text(u32_at(d, e + 8)?),
            height: u16_at(d, e + 0x0C)?,
            dodge: u16_at(d, e + 0x36)?,
            voice: text(u32_at(d, e + 0x3C)?),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_model_name_height_and_dodge() {
        let pool = 0x10 + RECORD;
        let mut d = vec![0u8; pool];
        d[4..8].copy_from_slice(&1u32.to_le_bytes());
        d[8..12].copy_from_slice(&(pool as u32).to_le_bytes());
        d.extend_from_slice(b"pc01a\0Edgar\0");
        let e = 0x10;
        d[e + 8..e + 12].copy_from_slice(&6u32.to_le_bytes());
        d[e + 0x0C..e + 0x0E].copy_from_slice(&190u16.to_le_bytes());
        d[e + 0x36..e + 0x38].copy_from_slice(&DODGE_GROUND.to_le_bytes());
        d[e + 0x3C..e + 0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        let list = entries(&d).unwrap();
        assert_eq!(list, vec![Entry { model: "pc01a".into(), name: Some("Edgar".into()), height: 190, dodge: DODGE_GROUND, voice: None }]);
    }
}
