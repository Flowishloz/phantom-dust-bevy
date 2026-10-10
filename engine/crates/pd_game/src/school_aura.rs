//! The school aura's rules, as the original's program has them -- the pure
//! half (no Bevy): who has an aura, which tier, which colour. `aura_fx.rs`
//! plays it.
//!
//! **What the aura is** (read from `PDUWP.exe`, `decomp_all.c`, 2026-09-30 --
//! `docs/audits/P0-AURA.md`): effect 600 (`pdEftAura01`), additive wisp
//! meshes riding the fighter's arm bones, not a silhouette rim. Every
//! fighter tick (`FUN_14024ec80`, tail) feeds the aura object
//! `DAT_1404bbee0` the fighter's Level (`FUN_14019e6c0(aura, player, level)`);
//! **Level is forced to 0** while status 8 (invisible) is on
//! (`FUN_14024d4d0(stats, 8) != 0`) or the fighter's +0x20 / +0x24 activity
//! words are zero (what those mean is unread -- not modelled here).
//!
//! **Tier** (`FUN_14019e6c0`, and the same arithmetic in `FUN_14019e430`):
//! `min(3, (Level + 3) / 4)` -- Level 0 nothing; 1-4 tier 1; 5-8 tier 2;
//! 9 and up tier 3. The effect is started with sub-slot
//! `DAT_1402dbb04[tier]` = bytes `00 00 04 05` -> **0 / 4 / 5**, which in
//! `cache/fx/eft0600.fx.json` (`slots = [[0],[],[],[],[1],[2]]`) are the
//! three models m0 / m1 / m2 (hands + wrists; + upper arm and bigger;
//! biggest, white). A tier change stops the running effect only when the NEW
//! tier is below 3, and a new one starts only when nothing is running, so
//! going from tier 2 to tier 3 mid-round does **not** swap the model
//! ([`step_tier`], [`TIER3_NEEDS_RESTART`]).
//!
//! **Colour** (`FUN_14019e280` builds a 6-entry table at `0x140599080` from
//! `.rdata` floats; `FUN_14019e430` stores the fighter's school index
//! (fighter +0x60) and the table row is the effect's colour override, which
//! `FUN_14011b450` multiplies into every piece's RGB): index 0 (none) and 4
//! (Ki) `0x1402f8920`, 1 (Psycho) `0x1402f8860`, 2 (Optical) `0x1402f8870`,
//! 3 (Nature) `0x1402f8880`, 5 (Faith) `0x1402f8910` -- see [`school_rgb`].
//! An arsenal with no schooled skill therefore gets the **blue** aura.
//!
//! **School index** (`FUN_14024d9d0`): count the 30 arsenal entries by the
//! skill record's +0x12 (our s11 SCHOOL, 0-5), then walk schools 1..5
//! keeping a school only when its count is strictly greater than the best so
//! far -- ties go to the lower id; no schooled skill leaves index 0
//! ([`school_index`]).
//!
//! **Alpha**: the same pass multiplies each piece's alpha by the fighter's
//! model opacity / 255 (`*(DAT_1404bbd50 + player * 0x120 + 0x1d0)`), the
//! per-view value `FUN_14024f2f0` computes for the dematerialising fade --
//! `aura_fx.rs` feeds it from the wall-dissolve coverage.
//!
//! Replaces the inflated body-mesh shell (v1-v6, all rejected: handoff
//! H3-school-aura-handoff.md) -- nothing in the original draws the body
//! twice.

use crate::combat::SkillDef;

/// Faithful to the program: a tier change stops the running effect only when
/// the new tier is below 3 (`FUN_14019e6c0`: `iVar7 < 3`), so a fighter who
/// reaches Level 9 during a round keeps the tier-2 model until the aura is
/// restarted (round start, `FUN_14019e430`; or the tier drops and returns).
/// Set `false` for "swap the model on any tier change".
pub const TIER3_NEEDS_RESTART: bool = true;

/// Tier for a Level: `min(3, (Level + 3) / 4)` (`FUN_14019e6c0`; the
/// decompile's `(x + (x >> 31 & 3)) >> 2` is C division by 4, truncating
/// toward zero, which is exactly Rust's `/`).
pub fn tier_for_level(level: i32) -> u8 {
    ((level + 3) / 4).clamp(0, 3) as u8
}

/// The Level the aura object is fed: 0 while invisible (status 8,
/// `FUN_14024ec80`'s `FUN_14024d4d0(stats, 8)` test), else the fighter's.
pub fn aura_level(level: i32, invisible: bool) -> i32 {
    if invisible { 0 } else { level }
}

/// `DAT_1402dbb04` (bytes `00 00 04 05`, read from the exe): tier -> the
/// effect's sub-slot. Tier 0 plays nothing.
pub const TIER_SLOT_BYTES: [u8; 4] = [0, 0, 4, 5];

/// The eft0600 slot to play for `tier` (None = no aura).
pub fn slot_for_tier(tier: u8) -> Option<usize> {
    (tier > 0).then(|| TIER_SLOT_BYTES[tier.min(3) as usize] as usize)
}

/// What the aura object does on one fighter tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TierStep {
    /// Stop the running effect (`FUN_14018d3d0`).
    pub stop: bool,
    /// Start the effect on this eft0600 slot (`FUN_14018d2f0`).
    pub start: Option<usize>,
}

/// One call of `FUN_14019e6c0`: `stored` = the tier remembered at +0x48,
/// `running` = a live handle at +0x40, `new` = the tier of this tick's Level.
/// The caller then stores `new` as the remembered tier.
///
/// ```text
/// if (stored != new && running && new < 3) stop;      // running = false
/// if (!running && new > 0) start slot[new];
/// stored = new;
/// ```
/// With `quirk` false the `new < 3` test is dropped.
pub fn step_tier(stored: u8, running: bool, new: u8, quirk: bool) -> TierStep {
    let stop = stored != new && running && (new < 3 || !quirk);
    let running = running && !stop;
    TierStep { stop, start: if !running { slot_for_tier(new) } else { None } }
}

/// School ids, `arsenal.rs`'s own convention (1 Psycho .. 5 Faith); 0 = none.
pub const NO_SCHOOL: usize = 0;
#[cfg(test)]
const PSYCHO: i16 = 1;
#[cfg(test)]
const OPTICAL: i16 = 2;
#[cfg(test)]
const NATURE: i16 = 3;
#[cfg(test)]
const KI: i16 = 4;
#[cfg(test)]
const FAITH: i16 = 5;

/// The aura's school index for an arsenal (`FUN_14024d9d0`): the school
/// (1..=5) with the most skills among the deck's entries, ties to the lower
/// id (strictly-greater walk from 1 to 5), or 0 when no entry has a school.
/// `arsenal` is `Combat::arsenals[i]`, the fighter's full 30-card deck (the
/// program counts its fixed 30-entry array, not the not-yet-drawn rest).
pub fn school_index(arsenal: &[i16], skills: &[SkillDef]) -> usize {
    let mut counts = [0i32; 6];
    for &id in arsenal {
        if let Some(s) = skills.iter().find(|s| s.id == id) {
            let sc = s.school();
            if (0..=5).contains(&sc) {
                counts[sc as usize] += 1;
            }
        }
    }
    school_from_counts(&counts)
}

/// The walk itself, on the six counters (index 0 = no school, never wins).
pub fn school_from_counts(counts: &[i32; 6]) -> usize {
    let (mut best, mut idx) = (0, NO_SCHOOL);
    for sc in 1..=5 {
        if best < counts[sc] {
            idx = sc;
            best = counts[sc];
        }
    }
    idx
}

/// The colour-override row per school index, straight from the exe's `.rdata`
/// floats (RGB; the fourth float of each row is 1.0): `0x140599080 + idx * 16`
/// as `FUN_14019e280` fills it.
pub fn school_rgb(index: usize) -> [f32; 3] {
    match index {
        1 => [0.8745098, 0.15686275, 0.05882353],  // Psycho  0x1402f8860  DF 28 0F
        2 => [0.8784314, 0.7490196, 0.05882353],   // Optical 0x1402f8870  E0 BF 0F
        3 => [0.05882353, 0.90980393, 0.15686275], // Nature  0x1402f8880  0F E8 28
        5 => [0.8745098, 0.02745098, 0.8784314],   // Faith   0x1402f8910  DF 07 E0
        // Ki (0x1402f8920, 0F 30 E0) is row 4; row 0 (no school) reuses it.
        _ => [0.05882353, 0.18823529, 0.8784314],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::{def, f};
    use crate::combat::{Capsule, AURA_PARTICLE};

    fn table() -> Vec<SkillDef> {
        // (id, school); 300 = a school-less skill.
        [(1, PSYCHO), (2, PSYCHO), (65, OPTICAL), (138, NATURE), (200, KI), (251, FAITH), (300, 0)].iter().map(|&(id, school)| def(id, Capsule::Attack, 1, &[(f::SCHOOL, school)])).collect()
    }

    fn q(rgb: [f32; 3]) -> [u8; 3] {
        rgb.map(|v| (v * 255.0).round() as u8)
    }

    /// `FUN_14019e6c0` tier = min(3, (Level + 3) / 4); `DAT_1402dbb04` bytes 00 00 04 05.
    #[test]
    fn tier_table_and_slots_match_fun_14019e6c0() {
        let levels = [0, 1, 4, 5, 8, 9, 50];
        assert_eq!(levels.map(tier_for_level), [0, 1, 1, 2, 2, 3, 3]);
        assert_eq!(levels.map(|l| slot_for_tier(tier_for_level(l))), [None, Some(0), Some(0), Some(4), Some(4), Some(5), Some(5)]);
        // Level can't go negative in the sim, but the C arithmetic truncates
        // toward zero: (-3 + 3) / 4 = 0, (-4 + 3) / 4 = 0.
        assert_eq!((tier_for_level(-3), tier_for_level(-4)), (0, 0));
    }

    /// `FUN_14024ec80`: Level forced to 0 when `FUN_14024d4d0(stats, 8)` (invisible).
    #[test]
    fn invisibility_forces_level_zero_so_no_aura() {
        assert_eq!(tier_for_level(aura_level(9, true)), 0);
        assert_eq!(slot_for_tier(tier_for_level(aura_level(9, true))), None);
        assert_eq!(aura_level(9, false), 9);
    }

    /// The floats at 0x1402f8860 / 70 / 80 / 910 / 920 (read with `pe.py`):
    /// 8-bit forms DF280F, E0BF0F, 0FE828, DF07E0, 0F30E0 (docs/audits/P0-AURA.md).
    #[test]
    fn school_colours_are_the_programs_rows() {
        assert_eq!(q(school_rgb(1)), [0xDF, 0x28, 0x0F], "Psycho 0x1402f8860");
        assert_eq!(q(school_rgb(2)), [0xE0, 0xBF, 0x0F], "Optical 0x1402f8870");
        assert_eq!(q(school_rgb(3)), [0x0F, 0xE8, 0x28], "Nature 0x1402f8880");
        assert_eq!(q(school_rgb(4)), [0x0F, 0x30, 0xE0], "Ki 0x1402f8920");
        assert_eq!(q(school_rgb(5)), [0xDF, 0x07, 0xE0], "Faith 0x1402f8910");
        assert_eq!(school_rgb(0), school_rgb(4), "no school = row 0 = Ki's floats (0x1402f8920)");
        // The exact floats, not just their bytes.
        assert_eq!(school_rgb(1), [0.8745098, 0.15686275, 0.05882353]);
    }

    #[test]
    fn picks_the_school_with_more_skills() {
        let mut arsenal = vec![AURA_PARTICLE; 28];
        arsenal.push(1); // Psycho
        arsenal.push(65); // Optical
        assert_eq!(school_index(&arsenal, &table()), 1);

        let arsenal2: Vec<i16> = vec![65, 65, 65, 1].into_iter().chain(std::iter::repeat_n(AURA_PARTICLE, 26)).collect();
        assert_eq!(school_index(&arsenal2, &table()), 2);
    }

    /// `FUN_14024d9d0`: strictly-greater walk over schools 1..5 -> ties to the lower id.
    #[test]
    fn ties_favour_the_lower_school_id() {
        assert_eq!(school_index(&[1, 65, 138, 200, 251], &table()), 1);
        assert_eq!(school_index(&[65, 138, 200, 251], &table()), 2);
        assert_eq!(school_index(&[200, 251], &table()), 4);
        assert_eq!(school_index(&[251, 251, 200], &table()), 5, "a real majority beats the lower id");
        assert_eq!(school_from_counts(&[7, 0, 2, 2, 1, 2]), 2);
    }

    /// Nothing schooled (or an empty/unknown deck) leaves index 0 -> the blue row.
    #[test]
    fn no_schooled_skill_is_index_zero_which_is_blue() {
        let arsenal = vec![AURA_PARTICLE; 30];
        assert_eq!(school_index(&arsenal, &table()), 0);
        assert_eq!(school_index(&[300; 30], &table()), 0);
        assert_eq!(school_index(&[], &table()), 0);
        assert_eq!(school_rgb(school_index(&arsenal, &table())), school_rgb(4));
    }

    /// `FUN_14019e6c0`'s start/stop rule, including the tier-2 -> 3 quirk.
    #[test]
    fn tier_steps_follow_the_start_stop_rule() {
        // Nothing running, Level 1: start slot 0.
        assert_eq!(step_tier(0, false, 1, true), TierStep { stop: false, start: Some(0) });
        // Nothing running, Level 0: nothing.
        assert_eq!(step_tier(0, false, 0, true), TierStep::default());
        // Running tier 1, same tier: nothing.
        assert_eq!(step_tier(1, true, 1, true), TierStep::default());
        // 1 -> 2: stop and start slot 4 in the same call.
        assert_eq!(step_tier(1, true, 2, true), TierStep { stop: true, start: Some(4) });
        // 2 -> 0: stop only.
        assert_eq!(step_tier(2, true, 0, true), TierStep { stop: true, start: None });
        // 3 -> 2: stop (2 < 3) and start slot 4.
        assert_eq!(step_tier(3, true, 2, true), TierStep { stop: true, start: Some(4) });
        // The quirk: 2 -> 3 keeps the tier-2 model running...
        assert_eq!(step_tier(2, true, 3, true), TierStep::default());
        // ... unless the quirk is switched off.
        assert_eq!(step_tier(2, true, 3, false), TierStep { stop: true, start: Some(5) });
        // A fresh start straight at Level 9 does get slot 5.
        assert_eq!(step_tier(0, false, 3, true), TierStep { stop: false, start: Some(5) });
    }
}
