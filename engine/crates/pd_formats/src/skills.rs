//! Skill table (`Data/gstorage/gsdata_<lang>.dat`) and arsenals (`Data/setup/deckdata_*`).
//!
//! Layout and evidence: docs/FORMATS-SKILLS.md. In short:
//! - 160-byte header; `[1]` + 0x19860 points at the text index.
//! - 144-byte skill records from offset 160 (a leading `u32`, then 70 `i16`).
//!   A record with text id 9999 ends the table.
//! - Text index: `u32 unk, u32 count, u32 block size, u32, u32`, then `count` x
//!   `(u32 text id, u32 name offset, u32 note offset)`; the strings follow the
//!   index and are NUL-terminated.
//!
//! Only fields with evidence get names; everything else stays in `raw`.

use crate::bytes::{i16_at, u32_at};
use anyhow::{bail, Result};

pub const HEADER_SIZE: usize = 160;
pub const RECORD_SIZE: usize = 144;
const TEXT_INDEX_BIAS: usize = 0x19860;
const END_TEXT_ID: i16 = 9999;

/// Short indices inside a record (short k lives at record offset 4 + 2k).
pub mod field {
    pub const TEXT_ID: usize = 2;
    pub const SKILL_ID: usize = 3;
    pub const CATALOG_NO: usize = 5;
    pub const RARITY: usize = 6;
    pub const CAPSULE: usize = 9;
    pub const SCHOOL: usize = 11;
    pub const COST: usize = 22;
    pub const LEVEL_PENALTY: usize = 23;
    pub const COST_PER_USE: usize = 24;
    pub const HEALTH_COST: usize = 25;
    /// Number of uses on the button; 0 = unlimited (probable, FORMATS-SKILLS.md s26).
    pub const USES: usize = 26;
    pub const REQUIREMENT: usize = 38;
    pub const REQUIREMENT_AMOUNT: usize = 39;
    /// Family / motion category (10 short-range, 14-15 [Arc], 80 [Barrier]...).
    pub const FAMILY: usize = 42;
    /// Defence skills: which kind (0 Barrier, 1 Shelter, 2 Course, 3 Brush,
    /// 4 Reflect, 5 Absorb, 6 About Face, 7 automatic Shelter). Matches every note tag.
    pub const DEFENCE_KIND: usize = 44;
    pub const STRENGTH_RULE: usize = 45;
    /// Attack STR. On defence skills this field holds a time instead (see FORMATS-SKILLS.md).
    pub const STRENGTH: usize = 46;
    /// Defence skills: DEF.
    pub const DEFENCE: usize = 47;
    /// Defence skills: 1 = Anti-Pierce (s49: exactly the six "Anti-Pierce"
    /// notes; an earlier reading of s48 was a column slip, fixed 2026-09-26).
    pub const ANTI_PIERCE: usize = 49;
    /// Defence skills: 1 = its "Add: Erases" only works on short-range attacks.
    pub const SHORT_RANGE_ONLY: usize = 50;
    /// 2 = "Can't be erased or overwritten" (Orb, Seal of Death, Sign of Saints — exactly those three).
    pub const PROTECTED: usize = 41;
    /// Extra effect kind and its amount/time ("Add: ..."); codes differ per capsule type.
    pub const ADD_EFFECT: usize = 51;
    pub const ADD_AMOUNT: usize = 52;
    /// Attacks: 1 = Pierce, 2 = penetrates defensive skills.
    pub const PIERCE: usize = 54;
    /// Attacks: 3 = penetrates obstacles.
    pub const OBSTACLES: usize = 55;
    /// How an attack travels (0 shell, 1 [Parabola], 2 [Fall], 5 [Crawl], 6 [Arc]...).
    pub const MOTION: usize = 63;
    /// First of six motion parameters (s64..s69); their meaning depends on MOTION.
    pub const MOTION_PARAMS: usize = 64;
    /// Range band for aimed attacks (Arc, Parabola, Fall, both Rains, Hole,
    /// motion-4 instants): 0 -> aim clamped 80-300 u, 1 -> 300-600 u, 2 -> no
    /// limit. Same short index as `DEFENCE_KIND` (defence skills don't use it
    /// this way). PROGRAM-RE.md "Things that change how the whole table is read".
    pub const RANGE_BAND: usize = 44;
    /// Row of the action table (`tables::action_table`) used for a normal /
    /// short-range cast (PROGRAM-RE.md "Action table"; `AIR_ROW` below is
    /// used instead for a mid-air cast).
    pub const GROUND_ROW: usize = 12;
    /// Row of the action table used when the skill is cast in mid-air
    /// (s40 = 1 or 3, or s40 = 2 while airborne). **Not a sound id** despite
    /// looking like one for laser-family skills (s13 = 305): 305 is an
    /// action-table row (the lasers' mid-air cast row), and the `common`
    /// sound actually numbered 305 is unrelated (Fire Storm's cast sound).
    pub const AIR_ROW: usize = 13;
    /// Reaction-table row (`tables::reaction_table`) for a light hit (< 3
    /// damage in the last 3 s, this hit included).
    pub const REACTION_LIGHT: usize = 57;
    /// Reaction-table row for a medium hit (3-4 damage in the last 3 s).
    pub const REACTION_MEDIUM: usize = 58;
    /// Reaction-table row for a heavy hit (>= 5 damage in the last 3 s).
    pub const REACTION_HEAVY: usize = 59;
    /// Reaction-table row played after the hit's Add effect lands (skill
    /// erased, aura drain, status up/down, reset, confusion, binds).
    pub const REACTION_AFTER_ADD: usize = 60;
    /// Reaction-table row for a hit while the target is airborne (every
    /// skill has this at 0, which reads as row 27, a knock-down).
    pub const REACTION_AIR: usize = 61;
    /// Mask of target states the attack can hit (0 reads as bit 1): 1 normal,
    /// 2 knocked down and airborne, 4 lying on the ground, 8 during get-up
    /// invulnerability.
    pub const TARGET_STATES: usize = 62;
}

pub const CAPSULE_NAMES: [&str; 7] = ["?", "attack", "defence", "erase", "status", "special", "environment"];
pub const SCHOOL_NAMES: [&str; 6] = ["none", "psycho", "optical", "nature", "ki", "faith"];

#[derive(Clone, Debug)]
pub struct Skill {
    /// Index of the record in the file.
    pub record: usize,
    /// The id arsenals (decks) store.
    pub skill_id: i16,
    pub text_id: i16,
    pub name: String,
    /// Extra line shown with the skill ("Requires: ...", "Penalty: ...").
    pub note: String,
    pub raw: [i16; 70],
    /// s70-s73 (PROGRAM-RE.md "Hidden fields s70-s73"): the program's record
    /// is 0x90 bytes from file offset 0x18, 8 bytes longer than our 144-byte
    /// framing, so its last 8 bytes are our "u32 always 0" + s0 + s1 of the
    /// *next* record. Read as i16: s70/s71 = that u32's low/high half,
    /// s72/s73 = the next record's s0/s1. Zero on the last record (or when
    /// the file doesn't hold a following record at all).
    pub s70_73: [i16; 4],
}

impl Skill {
    pub fn get(&self, f: usize) -> i16 {
        self.raw[f]
    }
    pub fn capsule(&self) -> &'static str {
        CAPSULE_NAMES.get(self.get(field::CAPSULE) as usize).copied().unwrap_or("?")
    }
    pub fn school(&self) -> &'static str {
        SCHOOL_NAMES.get(self.get(field::SCHOOL) as usize).copied().unwrap_or("?")
    }
    /// Has a catalogue number, i.e. can appear in a player's collection.
    pub fn registered(&self) -> bool {
        self.get(field::CATALOG_NO) != 0
    }
}

pub struct SkillTable {
    pub version: u32,
    pub skills: Vec<Skill>,
    /// Every text entry (id, name, note), including ones no record uses (e.g. 301 "Aura Particle").
    pub texts: Vec<(u32, String, String)>,
}

impl SkillTable {
    pub fn parse(d: &[u8]) -> Result<SkillTable> {
        let size = u32_at(d, 0)? as usize;
        if size != d.len() {
            bail!("gsdata: header says {size} bytes, file has {}", d.len());
        }
        let version = u32_at(d, 16)?;
        let texts = parse_texts(d, u32_at(d, 4)? as usize + TEXT_INDEX_BIAS)?;
        let text = |id: i16| texts.iter().find(|t| t.0 == id as u32);
        let count = u32_at(d, 8)? as usize;
        let mut skills = Vec::new();
        for record in 0..count {
            let off = HEADER_SIZE + record * RECORD_SIZE;
            if off + RECORD_SIZE > d.len() {
                break;
            }
            let mut raw = [0i16; 70];
            for (k, v) in raw.iter_mut().enumerate() {
                *v = i16_at(d, off + 4 + 2 * k)?;
            }
            if raw[field::TEXT_ID] == END_TEXT_ID {
                break;
            }
            if raw[field::SKILL_ID] == 0 {
                continue; // unused slot
            }
            let (name, note) = text(raw[field::TEXT_ID]).map(|t| (t.1.clone(), t.2.clone())).unwrap_or_default();
            let s70_73 = hidden_fields(d, record);
            skills.push(Skill { record, skill_id: raw[field::SKILL_ID], text_id: raw[field::TEXT_ID], name, note, raw, s70_73 });
        }
        Ok(SkillTable { version, skills, texts })
    }

    /// First record with this arsenal id (ids 257 and 294 appear twice; the later copies are variants).
    pub fn by_id(&self, skill_id: i16) -> Option<&Skill> {
        self.skills.iter().find(|s| s.skill_id == skill_id)
    }
}

/// s70-s73 for `record`: the leading `u32` (as two i16) + s0/s1 of the next
/// record in file order (see the `Skill::s70_73` doc comment). Out-of-range
/// (the last record, or a short/synthetic file) reads as zero, matching the
/// program's own framing which has nothing to read there either.
fn hidden_fields(d: &[u8], record: usize) -> [i16; 4] {
    let off = HEADER_SIZE + (record + 1) * RECORD_SIZE;
    let mut out = [0i16; 4];
    for (k, v) in out.iter_mut().enumerate() {
        *v = i16_at(d, off + 2 * k).unwrap_or(0);
    }
    out
}

/// The action table (`gsdata` offset 0x1B00C) and the hit-reaction table
/// (offset 0x2B00C): both are arrays of 0x72-byte rows of 57 `i16` each.
/// A skill's s12/s13 pick a row of the action table (ground / mid-air cast);
/// s57-s61 pick a row of the reaction table. Column meanings and evidence:
/// docs/PROGRAM-RE.md ("Action table", "Hit reactions"). Reference reader
/// (our own tool, not shipped): `engine/re/action_table.py`, `state_table.py`.
pub mod tables {
    use crate::bytes::i16_at;
    use anyhow::Result;

    pub const ACTION_TABLE_OFFSET: usize = 0x1B00C;
    pub const ACTION_TABLE_ROWS: usize = 574;
    pub const REACTION_TABLE_OFFSET: usize = 0x2B00C;
    /// 57 `i16` = 0x72 bytes, the row size of both tables.
    pub const ROW_WORDS: usize = 57;
    pub const ROW_SIZE: usize = ROW_WORDS * 2;

    /// One row, 57 `i16` fields. Only fields with evidence get named
    /// accessors below; everything else stays in `raw` (same convention as
    /// `Skill::raw`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TableRow {
        pub raw: [i16; ROW_WORDS],
    }

    impl Default for TableRow {
        fn default() -> Self {
            TableRow { raw: [0; ROW_WORDS] }
        }
    }

    impl TableRow {
        fn at(d: &[u8], table_offset: usize, i: usize) -> Result<TableRow> {
            let off = table_offset + i * ROW_SIZE;
            let mut raw = [0i16; ROW_WORDS];
            for (k, v) in raw.iter_mut().enumerate() {
                *v = i16_at(d, off + 2 * k)?;
            }
            Ok(TableRow { raw })
        }

        /// w0: 1 = row in use (action table).
        pub fn valid(&self) -> bool {
            self.raw[0] != 0
        }
        /// Any field past w0 is non-zero, i.e. the row holds real data
        /// (reaction table rows have no separate "in use" flag).
        pub fn used(&self) -> bool {
            self.raw[1..].iter().any(|&x| x != 0)
        }

        // --- Action table (PROGRAM-RE.md "Action table columns") ---
        pub fn effect(&self) -> i16 {
            self.raw[2]
        }
        /// `common` bank cast sound id, played at the caster at cast start.
        pub fn cast_sound(&self) -> i16 {
            self.raw[3]
        }
        pub fn voice_group(&self) -> i16 {
            self.raw[4]
        }
        pub fn anim_id(&self) -> i16 {
            self.raw[38]
        }
        pub fn anim_name(&self) -> &'static str {
            super::ANIM_NAMES.get(self.anim_id() as usize).copied().unwrap_or("?")
        }
        /// Hold-to-charge skills: ticks per charge step while the button is held.
        pub fn charge_cycle(&self) -> i16 {
            self.raw[39]
        }
        /// Animation frame the clip restarts at on a charge release.
        pub fn start_frame(&self) -> i16 {
            self.raw[40]
        }
        /// Short-range: dash start/end, ticks after cast start.
        pub fn dash_start(&self) -> i16 {
            self.raw[41]
        }
        pub fn dash_end(&self) -> i16 {
            self.raw[42]
        }
        /// Ticks from cast start to the attack object being created.
        pub fn fire_tick(&self) -> i16 {
            self.raw[43]
        }
        /// Short-range: hit window ends (object lives `hit_window_end - fire_tick` ticks).
        pub fn hit_window_end(&self) -> i16 {
            self.raw[44]
        }
        /// Combo offset: subtracted from the fire tick on later combo hits.
        pub fn combo_offset(&self) -> i16 {
            self.raw[48]
        }

        // --- Reaction table (PROGRAM-RE.md "Hit reactions") ---
        /// Reaction kind (see `state_table.py` KINDS: 0-11).
        pub fn kind(&self) -> i16 {
            self.raw[38]
        }
        /// Knock-down launch speed, 1/10.8 u per tick.
        pub fn launch_speed(&self) -> i16 {
            self.raw[39]
        }
        /// Knock-down launch angle, degrees.
        pub fn launch_angle(&self) -> i16 {
            self.raw[40]
        }
        /// w41..w48: animation ids / tick counts, meaning depends on `kind()`.
        pub fn w(&self, i: usize) -> i16 {
            self.raw[41 + i]
        }
    }

    /// The 574-row action table. A skill's s12 (ground) / s13 (mid-air) is a
    /// 1-based index into this (0 = no row).
    pub fn action_table(d: &[u8]) -> Result<Vec<TableRow>> {
        (0..ACTION_TABLE_ROWS).map(|i| TableRow::at(d, ACTION_TABLE_OFFSET, i)).collect()
    }

    /// The hit-reaction table: every full row the file holds from its offset
    /// onward (no separate row count is documented; PROGRAM-RE.md's
    /// `state_table.py` just reads however many rows it's asked for). A
    /// skill's s57-s59 (light/medium/heavy), s60 (after an Add effect) and
    /// s61 (hit in the air) index into this, as do the fixed ids 26 (brushed)
    /// and 27 (any hit in the air).
    pub fn reaction_table(d: &[u8]) -> Result<Vec<TableRow>> {
        let rows = d.len().saturating_sub(REACTION_TABLE_OFFSET) / ROW_SIZE;
        (0..rows).map(|i| TableRow::at(d, REACTION_TABLE_OFFSET, i)).collect()
    }
}

/// The program's motion-name table (pointer at 0x1402db400, 136 entries).
/// Index = an action-table row's `anim_id()` (w38). `bas01::WAIT0` ...
/// `atk45::ATK_LASER` ... `bas86::EXMOT5` in the program's own namespacing;
/// we keep them flat since the index is what matters.
pub const ANIM_NAMES: [&str; 136] = [
    "WAIT0", "WAIT1", "WAIT1IN", "WAIT1OUT", "WAIT_ACT0", "WAIT_CON", "WAIT_DAM", "WALK", "WALK_CON", "WALK_DAM",
    "WALK_FAST", "WALK_SLOW", "RUN", "RUN_CON", "RUN_DAM", "RUN_FAST", "RUN_SLOW", "TURN_WALK_R", "TURN_WALK_L",
    "TURN_WALK_BR", "TURN_WALK_BL", "TURN_RUN_R", "TURN_RUN_L", "TURN_RUN_BR", "TURN_RUN_BL", "TURN_WDAM_R",
    "TURN_WDAM_L", "TURN_WDAM_BR", "TURN_WDAM_BL", "TURN_RDAM_R", "TURN_RDAM_L", "TURN_RDAM_BR", "TURN_RDAM_BL",
    "DASH_A", "DASHEND_A", "DASH_B", "DASHEND_B", "JUMP_START", "JUMP_UP", "JUMP_DOWN", "JUMP_LAND0", "JUMP_LAND1",
    "FLOAT_IN", "FLOAT_WAIT", "FLOAT_MOVE", "BLOW0_FRONT", "BLOW1_FRONT", "BLOW2_FRONT", "BLOW0_BACK", "BLOW1_BACK",
    "BLOW_LAND_LF", "BLOW_LAND_MF", "BLOW_LAND_SF", "BLOW_LAND_LB", "BLOW_LAND_MB", "BLOW_LAND_SB", "WALLHIT_FRONT",
    "WALLHIT_FRONT_LOOP", "WALLHIT_BACK", "WALLHIT_BACK_LOOP", "DOWN_FRONT", "DOWN_FRONT_STAND", "DOWN_BACK",
    "DOWN_BACK_STAND", "ATK_SORD", "ATK_SHOT", "DEF_SHIELD", "ATK_WHAND_LASER", "ATK_SKYSHOT_LOW",
    "ATK_SKYSHOT_HIGH", "ATK_KI_SHOT", "ATK_WHAND_SORD", "DEF_BRUSH", "DEF_ABSORB", "ATK_STAMP", "DEF_SHELTER",
    "ATK_LASER", "SPE_DASH", "SPE_WARP", "ATK_SPHERE", "ATK_THROW", "ATK_SPIKE", "ATK_WHAND_SPIKE", "ATK_GROUND",
    "DEF_WHAND_SHIELD", "ATK_DRAIN", "SPE_POWER_UP", "DEF_COURSE", "ATK_SET_BOMB", "SPE_RESET", "ATK_TUKI",
    "ATK_PUNCH", "ATK_SUPER_SHOT", "SPE_HEAL", "ATK_KICK", "ATK_BODY", "ENV_SET", "ATK_FAITH_SHOT", "ATK_PUNCH_2",
    "ATK_PUNCH_3", "ATK_LASER_2", "ATK_LASER_3", "DMG_RAISE_HIGH", "DMG_DRAIN_HIGH", "DMG_SPIKE_HIGH",
    "DMG_BIND_AIR", "DMG_CONFUSION", "DMG_BIND_CROSS", "DMG_BIND_CHAIN", "DMG_BIND_PRESS", "DMG_AURA",
    "DMG_STAMP_FACEUP", "DMG_ABILITY", "DMG_STATUS_UP", "DMG_STATUS_DOWN", "DMG_STATUS_RESET", "DMG_PARARISE",
    "DMG_SHOT_FRONT", "DMG_SHOT_BACK", "DMG_SHOT_LEFT", "DMG_SHOT_RIGHT", "DMG_SPHERE_FRONT", "DMG_SPHERE_BACK",
    "DMG_SORD_FRONT", "DMG_SORD_BACK", "DMG_DRAIN_LOW", "DMG_SPIKE_LOW", "DMG_RAISE_LOW", "DMG_STAMP_FACEDOWN",
    "DMG_SHORT", "EXMOT0", "EXMOT1", "EXMOT2", "EXMOT3", "EXMOT4", "EXMOT5",
];

fn parse_texts(d: &[u8], at: usize) -> Result<Vec<(u32, String, String)>> {
    let count = u32_at(d, at + 4)? as usize;
    let base = at + 20 + 12 * count;
    let cstr = |off: usize| -> Result<String> {
        let start = base + off;
        let Some(len) = d.get(start..).and_then(|s| s.iter().position(|&b| b == 0)) else {
            bail!("gsdata: text at 0x{start:X} is unterminated");
        };
        Ok(String::from_utf8_lossy(&d[start..start + len]).into_owned())
    };
    (0..count)
        .map(|k| {
            let e = at + 20 + 12 * k;
            Ok((u32_at(d, e)?, cstr(u32_at(d, e + 4)? as usize)?, cstr(u32_at(d, e + 8)? as usize)?))
        })
        .collect()
}

/// A 30-skill arsenal from `deckdata_*`.
#[derive(Clone, Debug)]
pub struct Deck {
    pub name: String,
    pub skills: [i16; 30],
}

const DECK_SIZE: usize = 100;

/// `deckdata_debug`: a 4-byte header, then 100-byte decks
/// (`u32, char name[16], i16 skills[30], u16 school count, u16, u32 stats[4]`).
pub fn parse_decks(d: &[u8]) -> Result<Vec<Deck>> {
    if d.len() < 4 || (d.len() - 4) % DECK_SIZE != 0 {
        bail!("deckdata: size {} is not 4 + n x {DECK_SIZE}", d.len());
    }
    (0..(d.len() - 4) / DECK_SIZE)
        .map(|k| {
            let o = 4 + k * DECK_SIZE;
            let name = d[o + 4..o + 20].split(|&b| b == 0).next().unwrap_or_default();
            let mut skills = [0i16; 30];
            for (j, s) in skills.iter_mut().enumerate() {
                *s = i16_at(d, o + 20 + 2 * j)?;
            }
            Ok(Deck { name: String::from_utf8_lossy(name).into_owned(), skills })
        })
        .collect()
}

/// One row of the game-rule table (`gsdata` at 0x33040, stride 0x10, four
/// little-endian `i32`s; read by `FUN_14024d8b0` / `FUN_14024d970` /
/// `FUN_14024e790` -> `FUN_14024d020` for health and Level, `FUN_140128cc0`
/// for the time limit: `minutes x 0x708` program ticks). Row index = rule id
/// (`DAT_1404bc5dc`): 0 Standard, 1 Ante, 2 Quick, 3 Puny, 4 Heavy, 5 Special
/// Environment, 6 Single School, 7 Highlander, 8 Puny Only, 9 Heavy Only,
/// 10 KO Battle, 11 Annihilate the Odd One (the others); row 12 is the Odd
/// One's own (used for rule 11 and player 0, `FUN_14024e790`). Evidence:
/// docs/audits/P0-FRONTEND.md appendix B, re-read from the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleRow {
    pub health: i32,
    pub level_min: i32,
    pub level_max: i32,
    /// Time limit in minutes (0 = none).
    pub minutes: i32,
}

pub const RULE_TABLE_OFFSET: usize = 0x33040;
pub const RULE_ROWS: usize = 13;
/// The three Aura-speed multipliers by declared number of schools (1, 2, 3).
pub const AURA_HANDICAP_OFFSET: usize = 0x33034;
/// The eight preset "Quick" arsenals (`Q000`..`Q007`, `FUN_14017df00`): 0x80
/// bytes each, name at +4 (16 bytes), 30 `i16` skills at +0x14 (0 or -1 =
/// empty), declared school kind (`u16`) at +0x50.
pub const QUICK_DECK_OFFSET: usize = 0x33804;
pub const QUICK_DECK_STRIDE: usize = 0x80;
pub const QUICK_DECKS: usize = 8;

/// The 13 rule rows.
pub fn rule_table(d: &[u8]) -> Result<Vec<RuleRow>> {
    (0..RULE_ROWS)
        .map(|k| {
            let o = RULE_TABLE_OFFSET + 16 * k;
            let v = |j: usize| -> Result<i32> { Ok(u32_at(d, o + 4 * j)? as i32) };
            Ok(RuleRow { health: v(0)?, level_min: v(1)?, level_max: v(2)?, minutes: v(3)? })
        })
        .collect()
}

/// The Aura-speed multipliers for 1 / 2 / 3 schools (1.3, 1.0, 0.7 in the file).
pub fn aura_handicaps(d: &[u8]) -> Result<[f32; 3]> {
    let mut out = [1.0; 3];
    for (k, v) in out.iter_mut().enumerate() {
        *v = f32::from_bits(u32_at(d, AURA_HANDICAP_OFFSET + 4 * k)?);
    }
    Ok(out)
}

/// The preset "Quick" arsenals: `Deck` plus the declared kind stored at +0x50.
pub fn quick_decks(d: &[u8]) -> Result<Vec<(Deck, u16)>> {
    (0..QUICK_DECKS)
        .map(|k| {
            let o = QUICK_DECK_OFFSET + QUICK_DECK_STRIDE * k;
            let name = d.get(o + 4..o + 20).ok_or_else(|| anyhow::anyhow!("gsdata: quick deck {k} is past the end"))?;
            let name = name.split(|&b| b == 0).next().unwrap_or_default();
            let mut skills = [0i16; 30];
            for (j, s) in skills.iter_mut().enumerate() {
                *s = i16_at(d, o + 0x14 + 2 * j)?;
            }
            Ok((Deck { name: String::from_utf8_lossy(name).into_owned(), skills }, i16_at(d, o + 0x50)? as u16))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put16(d: &mut [u8], off: usize, v: i16) {
        d[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(d: &mut [u8], off: usize, v: u32) {
        d[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A tiny synthetic file: two skills, an end marker, a two-entry text index.
    fn sample() -> Vec<u8> {
        let text_at = TEXT_INDEX_BIAS + 16;
        let strings = b"Wave\0STR = Level\0Blade\0\0";
        let mut d = vec![0u8; text_at + 20 + 24 + strings.len()];
        let n = d.len() as u32;
        put32(&mut d, 0, n);
        put32(&mut d, 4, 16);
        put32(&mut d, 8, 3);
        put32(&mut d, 16, 140);
        let rec = |k: usize, f: usize| HEADER_SIZE + k * RECORD_SIZE + 4 + 2 * f;
        for (k, (text, id, school, cost, dmg)) in [(1i16, 2i16, 1i16, 4i16, 1i16), (2, 5, 2, 3, 5)].into_iter().enumerate() {
            put16(&mut d, rec(k, field::TEXT_ID), text);
            put16(&mut d, rec(k, field::SKILL_ID), id);
            put16(&mut d, rec(k, field::CAPSULE), 1);
            put16(&mut d, rec(k, field::SCHOOL), school);
            put16(&mut d, rec(k, field::COST), cost);
            put16(&mut d, rec(k, field::STRENGTH), dmg);
        }
        put16(&mut d, rec(2, field::TEXT_ID), END_TEXT_ID);
        put32(&mut d, text_at + 4, 2);
        for (k, (id, name, note)) in [(1u32, 0u32, 5u32), (2, 17, 23)].into_iter().enumerate() {
            let e = text_at + 20 + 12 * k;
            put32(&mut d, e, id);
            put32(&mut d, e + 4, name);
            put32(&mut d, e + 8, note);
        }
        let s = text_at + 20 + 24;
        d[s..s + strings.len()].copy_from_slice(strings);
        d
    }

    // Also run against the real file (not in the repo) with: pd_convert skills <gsdata_en.dat> <out.json>
    #[test]
    fn parses_records_and_names() {
        let t = SkillTable::parse(&sample()).unwrap();
        assert_eq!(t.version, 140);
        assert_eq!(t.skills.len(), 2);
        let w = t.by_id(2).unwrap();
        assert_eq!((w.name.as_str(), w.note.as_str()), ("Wave", "STR = Level"));
        assert_eq!((w.capsule(), w.school(), w.get(field::COST), w.get(field::STRENGTH)), ("attack", "psycho", 4, 1));
        let b = t.by_id(5).unwrap();
        assert_eq!((b.name.as_str(), b.note.as_str(), b.school()), ("Blade", "", "optical"));
    }

    #[test]
    fn parses_decks() {
        let mut d = vec![0u8; 4 + 2 * DECK_SIZE];
        d[4 + DECK_SIZE + 4..4 + DECK_SIZE + 6].copy_from_slice(b"KI");
        put16(&mut d, 4 + DECK_SIZE + 20, 181);
        let decks = parse_decks(&d).unwrap();
        assert_eq!(decks.len(), 2);
        assert_eq!((decks[1].name.as_str(), decks[1].skills[0]), ("KI", 181));
        assert!(parse_decks(&d[..50]).is_err());
    }

    #[test]
    fn hidden_fields_read_the_next_record() {
        // Two consecutive records: record 0's s70-73 should be record 1's
        // leading "u32 always 0" (as two i16) + its s0, s1 -- not record 0's
        // own trailing bytes (PROGRAM-RE.md "Hidden fields s70-s73").
        let mut d = vec![0u8; HEADER_SIZE + 3 * RECORD_SIZE];
        let rec1 = HEADER_SIZE + RECORD_SIZE;
        put16(&mut d, rec1, 11); // low half of record 1's leading u32
        put16(&mut d, rec1 + 2, 22); // high half
        put16(&mut d, rec1 + 4, 33); // record 1's s0
        put16(&mut d, rec1 + 6, 44); // record 1's s1
        assert_eq!(hidden_fields(&d, 0), [11, 22, 33, 44]);
        // Out of range (no record after the last one) reads as zero, not an error.
        assert_eq!(hidden_fields(&d, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn table_row_layout_constants() {
        // PROGRAM-RE.md "Action table": file offset 0x1B00C, 574 rows of 57
        // i16 (0x72 bytes); the reaction table sits right after it at 0x2B00C.
        assert_eq!(tables::ROW_WORDS, 57);
        assert_eq!(tables::ROW_SIZE, 0x72);
        assert_eq!(tables::ACTION_TABLE_OFFSET, 0x1B00C);
        assert_eq!(tables::ACTION_TABLE_ROWS, 574);
        assert_eq!(tables::REACTION_TABLE_OFFSET, 0x2B00C);
        // The action table's 574 rows end just before the reaction table
        // starts (a 0x64-byte gap between them) -- corroborates both offsets
        // and the row count together, independent of the real file.
        let action_end = tables::ACTION_TABLE_OFFSET + tables::ACTION_TABLE_ROWS * tables::ROW_SIZE;
        assert_eq!(tables::REACTION_TABLE_OFFSET - action_end, 0x64);
    }

    #[test]
    fn action_table_reads_a_row() {
        // A synthetic file holding the full 574-row table, with known values
        // at row 42 (Laser's ground row) and its documented column indices.
        let mut d = vec![0u8; tables::ACTION_TABLE_OFFSET + tables::ACTION_TABLE_ROWS * tables::ROW_SIZE];
        let off = tables::ACTION_TABLE_OFFSET + 42 * tables::ROW_SIZE;
        put16(&mut d, off, 1); // w0: valid
        put16(&mut d, off + 2 * 2, 42); // w2: effect eft0042
        put16(&mut d, off + 3 * 2, 242); // w3: cast sound common/242
        put16(&mut d, off + 4 * 2, 1); // w4: voice group 1
        put16(&mut d, off + 38 * 2, 76); // w38: anim id 76 = ATK_LASER
        put16(&mut d, off + 43 * 2, 13); // w43: fire tick 13
        let rows = tables::action_table(&d).unwrap();
        assert_eq!(rows.len(), 574);
        let r = rows[42];
        assert!(r.valid());
        assert_eq!((r.effect(), r.cast_sound(), r.voice_group()), (42, 242, 1));
        assert_eq!((r.anim_id(), r.anim_name()), (76, "ATK_LASER"));
        assert_eq!(r.fire_tick(), 13);
        assert!(!rows[0].valid());
    }

    #[test]
    fn reaction_table_reads_whatever_the_file_holds() {
        // No documented total row count for the reaction table (unlike the
        // action table's 574): we read as many full rows as fit.
        let d = vec![0u8; tables::REACTION_TABLE_OFFSET + 10 * tables::ROW_SIZE + 5];
        let rows = tables::reaction_table(&d).unwrap();
        assert_eq!(rows.len(), 10);
        // A file that ends before the reaction table even starts reads as empty, not an error.
        let short = vec![0u8; tables::REACTION_TABLE_OFFSET - 1];
        assert!(tables::reaction_table(&short).unwrap().is_empty());
    }

    #[test]
    fn anim_names_match_program_re_examples() {
        // PROGRAM-RE.md: "index = our ANIM_NAMES index (64 ATK_SORD, 65
        // ATK_SHOT, 68 ATK_SKYSHOT_LOW, 76 ATK_LASER)".
        assert_eq!(ANIM_NAMES.len(), 136);
        assert_eq!(ANIM_NAMES[64], "ATK_SORD");
        assert_eq!(ANIM_NAMES[65], "ATK_SHOT");
        assert_eq!(ANIM_NAMES[68], "ATK_SKYSHOT_LOW");
        assert_eq!(ANIM_NAMES[76], "ATK_LASER");
    }

    /// Golden checks against the real game file (PROGRAM-RE.md "The three
    /// moves", "Motion 13"). Skipped (not failed) when `gsdata_en.dat` isn't
    /// present -- it's game data, never committed (AGENTS.md hard rule 1).
    /// Cross-checked against `python engine/re/action_table.py <gsdata_en.dat>
    /// engine/cache/skills.json <id>`.
    #[test]
    fn golden_checks_against_real_gsdata() {
        let path = std::path::Path::new("../../../game_files/Assets/Data/gstorage/gsdata_en.dat");
        let Ok(d) = std::fs::read(path) else {
            eprintln!("skipping golden_checks_against_real_gsdata: {} not found", path.display());
            return;
        };
        let t = SkillTable::parse(&d).unwrap();
        let action = tables::action_table(&d).unwrap();

        // Laser id 65: ground row 42, fire tick 13, ATK_LASER, cast sound common/242.
        let laser = t.by_id(65).unwrap();
        let row = laser.get(field::GROUND_ROW) as usize;
        assert_eq!(row, 42);
        let r = action[row];
        assert_eq!(r.fire_tick(), 13);
        assert_eq!(r.anim_name(), "ATK_LASER");
        assert_eq!(r.cast_sound(), 242);

        // Psycho Knife id 3: row 3, fire tick 18, dash 12..22, hit window 18..22.
        let knife = t.by_id(3).unwrap();
        let row = knife.get(field::GROUND_ROW) as usize;
        assert_eq!(row, 3);
        let r = action[row];
        assert_eq!(r.fire_tick(), 18);
        assert_eq!((r.dash_start(), r.dash_end()), (12, 22));
        assert_eq!((r.fire_tick(), r.hit_window_end()), (18, 22));

        // Blaster id 80: fire tick 20, ATK_SHOT.
        let blaster = t.by_id(80).unwrap();
        let row = blaster.get(field::GROUND_ROW) as usize;
        let r = action[row];
        assert_eq!(r.fire_tick(), 20);
        assert_eq!(r.anim_name(), "ATK_SHOT");

        // Psycho Wave id 1: ATK_SPHERE, fire 20, dash 10..20, hit 20..22.
        let wave = t.by_id(1).unwrap();
        let row = wave.get(field::GROUND_ROW) as usize;
        let r = action[row];
        assert_eq!(r.anim_name(), "ATK_SPHERE");
        assert_eq!(r.fire_tick(), 20);
        assert_eq!((r.dash_start(), r.dash_end()), (10, 20));
        assert_eq!((r.fire_tick(), r.hit_window_end()), (20, 22));
    }

    /// The rule table, Aura handicaps and Quick decks against the real file
    /// (skipped when it is not present): the numbers `combat::RULE_TABLE` is
    /// built from.
    #[test]
    fn rule_table_handicaps_and_quick_decks_against_real_gsdata() {
        let path = std::path::Path::new("../../../game_files/Assets/Data/gstorage/gsdata_en.dat");
        let Ok(d) = std::fs::read(path) else {
            eprintln!("skipping rule_table_handicaps_and_quick_decks_against_real_gsdata: {} not found", path.display());
            return;
        };
        let rules = rule_table(&d).unwrap();
        let row = |health, level_min, level_max, minutes| RuleRow { health, level_min, level_max, minutes };
        assert_eq!(rules[0], row(20, 0, 99, 10), "Standard");
        assert_eq!(rules[3], row(10, 0, 5, 10), "Puny");
        assert_eq!(rules[4], row(40, 5, 99, 10), "Heavy");
        assert_eq!(rules[10], row(5, 0, 99, 3), "KO Battle");
        assert_eq!(rules[11], row(10, 0, 99, 10), "Odd One, the others");
        assert_eq!(rules[12], row(40, 0, 99, 0), "Odd One, the host");
        assert_eq!(aura_handicaps(&d).unwrap(), [1.3, 1.0, 0.7]);
        let quick = quick_decks(&d).unwrap();
        assert_eq!(quick.len(), 8);
        assert_eq!(quick[0].0.name, "Q000");
        assert!(quick.iter().all(|(_, kind)| *kind == 2));
    }
}
