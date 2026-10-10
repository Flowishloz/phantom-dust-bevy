//! ALR skeletal animation (0x5 chunks).
//!
//! The header chunk's offset table is indexed by animation id (see
//! `ANIM_NAMES`). Each animation is a run of 0x5 chunks, one per animated
//! joint, ended by a chunk with another id. Each 0x5 chunk holds translation
//! keys then rotation keys (then scale keys, not yet seen in use).

use crate::alr::Alr;
use crate::bytes::*;
use anyhow::Result;

pub const ID_ANIMATION: u32 = 0x5;

#[derive(Debug, Clone)]
pub struct Key {
    pub frame: f32,
    pub value: [f32; 3],
    /// How many components the key actually stored.
    pub components: u8,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub joint: u16,
    /// Length of the animation in frames (30 fps; see FORMATS-ANIMATION.md
    /// "Playback rate").
    pub length: f32,
    pub settings: [u8; 2],
    pub translation_key_size: u16,
    pub rotation_key_size: u16,
    pub translation: Vec<Key>,
    pub rotation: Vec<Key>,
    pub scale_key_count: u32,
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub id: usize,
    pub tracks: Vec<Track>,
}

impl Animation {
    pub fn length(&self) -> f32 {
        self.tracks.iter().map(|t| t.length).fold(0.0, f32::max)
    }
}

/// Decode one key. Small keys are a u8 frame plus s16 components that map
/// [-32767, 32767] onto a full turn; float keys are an f32 frame plus floats.
fn read_key(d: &[u8], off: usize, size: u16) -> Result<Key> {
    let (frame, comp_size, float, header) = match size {
        3 | 5 | 7 => (u8_at(d, off)? as f32, 2usize, false, 1usize),
        4 | 8 | 12 | 16 => (f32_at(d, off)?, 4usize, true, 4usize),
        _ => (0.0, 0, false, 0),
    };
    let mut value = [0.0; 3];
    let n = if comp_size == 0 { 0 } else { ((size as usize).saturating_sub(header) / comp_size).min(3) };
    for (i, v) in value.iter_mut().enumerate().take(n) {
        let p = off + header + i * comp_size;
        *v = if float { f32_at(d, p)? } else { i16_at(d, p)? as f32 / i16::MAX as f32 * std::f32::consts::TAU };
    }
    Ok(Key { frame, value, components: n as u8 })
}

pub fn read_animation(a: &Alr, id: usize) -> Result<Option<Animation>> {
    let d = &a.data;
    let count = u32_at(d, 16)? as usize;
    if id >= count {
        return Ok(None);
    }
    let start = i32_at(d, 32 + id * 4)?;
    if start <= 0 {
        return Ok(None);
    }
    let mut pos = start as usize;
    let mut tracks = Vec::new();
    while pos + 0x20 <= d.len() && u32_at(d, pos)? == ID_ANIMATION {
        let size = u32_at(d, pos + 4)? as usize;
        if size < 0x20 {
            break;
        }
        let length = f32_at(d, pos + 8)?;
        let joint = u16_at(d, pos + 12)?;
        let rsize = u16_at(d, pos + 14)?;
        let tcount = u32_at(d, pos + 16)? as usize;
        let rcount = u32_at(d, pos + 20)? as usize;
        let scount = u32_at(d, pos + 24)?;
        let settings = [u8_at(d, pos + 28)?, u8_at(d, pos + 29)?];
        let tsize = u16_at(d, pos + 30)?;

        let mut k = pos + 0x20;
        let mut translation = Vec::with_capacity(tcount);
        for _ in 0..tcount {
            translation.push(read_key(d, k, tsize)?);
            k += tsize as usize;
        }
        let mut rotation = Vec::with_capacity(rcount);
        for _ in 0..rcount {
            rotation.push(read_key(d, k, rsize)?);
            k += rsize as usize;
        }
        tracks.push(Track {
            joint,
            length,
            settings,
            translation_key_size: tsize,
            rotation_key_size: rsize,
            translation,
            rotation,
            scale_key_count: scount,
        });
        pos += size;
    }
    Ok(Some(Animation { id, tracks }))
}

/// Debug names from the game's animation viewer (PDUWP.exe), indexed by id.
pub const ANIM_NAMES: &[&str] = &[
    "WAIT0", "WAIT1", "WAIT1IN", "WAIT1OUT", "WAIT_ACT0", "WAIT_CON", "WAIT_DAM", "WALK", "WALK_CON", "WALK_DAM",
    "WALK_FAST", "WALK_SLOW", "RUN", "RUN_CON", "RUN_DAM", "RUN_FAST", "RUN_SLOW", "TURN_WALK_R", "TURN_WALK_L",
    "TURN_WALK_BR", "TURN_WALK_BL", "TURN_RUN_R", "TURN_RUN_L", "TURN_RUN_BR", "TURN_RUN_BL", "TURN_WDAM_R",
    "TURN_WDAM_L", "TURN_WDAM_BR", "TURN_WDAM_BL", "TURN_RDAM_R", "TURN_RDAM_L", "TURN_RDAM_BR", "TURN_RDAM_BL",
    "DASH_A", "DASHEND_A", "DASH_B", "DASHEND_B", "JUMP_START", "JUMP_UP", "JUMP_DOWN", "JUMP_LAND0", "JUMP_LAND1",
    "FLOAT_IN", "FLOAT_WAIT", "FLOAT_MOVE", "BLOW0_FRONT", "BLOW1_FRONT", "BLOW2_FRONT", "BLOW0_BACK", "BLOW1_BACK",
    "BLOW_LAND_LF", "BLOW_LAND_MF", "BLOW_LAND_SF", "BLOW_LAND_LB", "BLOW_LAND_MB", "BLOW_LAND_SB", "WALLHIT_FRONT",
    "WALLHIT_FRONT_LOOP", "WALLHIT_BACK", "WALLHIT_BACK_LOOP", "DOWN_FRONT", "DOWN_FRONT_STAND", "DOWN_BACK",
    "DOWN_BACK_STAND", "ATK_SORD", "ATK_SHOT", "DEF_SHIELD", "ATK_WHAND_LASER", "ATK_SKYSHOT_LOW", "ATK_SKYSHOT_HIGH",
    "ATK_KI_SHOT", "ATK_WHAND_SORD", "DEF_BRUSH", "DEF_ABSORB", "ATK_STAMP", "DEF_SHELTER", "ATK_LASER", "SPE_DASH",
    "SPE_WARP", "ATK_SPHERE", "ATK_THROW", "ATK_SPIKE", "ATK_WHAND_SPIKE", "ATK_GROUND", "DEF_WHAND_SHIELD",
    "ATK_DRAIN", "SPE_POWER_UP", "DEF_COURSE", "ATK_SET_BOMB", "SPE_RESET", "ATK_TUKI", "ATK_PUNCH", "ATK_SUPER_SHOT",
    "SPE_HEAL", "ATK_KICK", "ATK_BODY", "ENV_SET", "ATK_FAITH_SHOT", "ATK_PUNCH_2", "ATK_PUNCH_3", "ATK_LASER_2",
    "ATK_LASER_3", "DMG_RAISE_HIGH", "DMG_DRAIN_HIGH", "DMG_SPIKE_HIGH", "DMG_BIND_AIR", "DMG_CONFUSION",
    "DMG_BIND_CROSS", "DMG_BIND_CHAIN", "DMG_BIND_PRESS", "DMG_AURA", "DMG_STAMP_FACEUP", "DMG_ABILITY",
    "DMG_STATUS_UP", "DMG_STATUS_DOWN", "DMG_STATUS_RESET", "DMG_PARARISE", "DMG_SHOT_FRONT", "DMG_SHOT_BACK",
    "DMG_SHOT_LEFT", "DMG_SHOT_RIGHT", "DMG_SPHERE_FRONT", "DMG_SPHERE_BACK", "DMG_SORD_FRONT", "DMG_SORD_BACK",
    "DMG_DRAIN_LOW", "DMG_SPIKE_LOW", "DMG_RAISE_LOW", "DMG_STAMP_FACEDOWN", "DMG_SHORT", "EXMOT0", "EXMOT1", "EXMOT2",
    "EXMOT3", "EXMOT4", "EXMOT5",
];
