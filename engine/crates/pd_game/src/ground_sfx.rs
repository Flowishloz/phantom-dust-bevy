//! Sounds that depend on the ground or on a stage object: the landing and take-off sound of the
//! floor material, the surface-hit sound of a landing / shot / blast on the stage, and the sound
//! group of a stage object that breaks. Render side only: it reads sim events and sim state, never
//! writes either, and its random bit comes from its own xorshift (like the voice pick).
//!
//! Every table below is READ from `PDUWP.exe` (`engine/docs/audits/P0-SOUND.md` section 11):
//!
//! * landing sound by floor class: `FUN_140254b30`, called at the end of `FUN_140254c80` (a body
//!   touching down from the airborne state 5, or a knock-down flight ending, state 0x11);
//! * take-off sound by floor class: the sibling `FUN_1402549e0`, called from `FUN_14025a0b0`;
//! * surface-hit sound by surface class: `FUN_14021d740`, id `DAT_1402f5da0[class - 2]`, played
//!   when `FUN_14021d3f0` (the stage-contact effect, fed by `FUN_1402021f0` from the contacts of
//!   the "player bullet" probe) spawns an impact decal; classes with no decal type play nothing;
//! * stage object sound groups: header-`0xF8` table of the stage `.dat` (`FUN_1401fc750`, slot 1 =
//!   the object broke) -> `FUN_140201d30` plays `DAT_1402dc640[group * 2 + (rand() >> 14 & 1)]`.

use crate::sfx::Cue;
use crate::surface_ground::SurfaceGround;
use bevy::prelude::*;
use glam::Vec3;
use std::collections::HashMap;

/// Stage objects' sound groups: `DAT_1402dc640`, two ids per group (the random bit picks one;
/// -1 = silent). READ, 47 groups.
const PROP_GROUP_IDS: [[i32; 2]; 47] = [
    [-1, -1], [8, -1], [9, -1], [10, -1], [11, -1], [12, -1],
    [13, -1], [-1, -1], [14, -1], [16, -1], [-1, -1], [-1, -1],
    [-1, -1], [-1, -1], [-1, -1], [-1, -1], [-1, -1], [17, 172],
    [18, 173], [19, 174], [20, 175], [21, 176], [22, 177], [23, 178],
    [179, 180], [181, 182], [183, 184], [185, 186], [187, 188], [189, 190],
    [-1, -1], [-1, -1], [-1, -1], [-1, -1], [-1, -1], [-1, -1],
    [191, 192], [193, 194], [195, 196], [197, 198], [-1, -1], [-1, -1],
    [-1, -1], [-1, -1], [-1, -1], [-1, -1], [-1, -1],
];

/// The common-bank id a sound group plays for a random bit (0 / 1); `None` = silent.
pub fn prop_group_id(group: i32, bit: u32) -> Option<u32> {
    let row = PROP_GROUP_IDS.get(usize::try_from(group).ok()?)?;
    u32::try_from(row[(bit & 1) as usize]).ok()
}

/// Which floor classes share a landing / take-off sound. READ: `FUN_140254b30` and
/// `FUN_1402549e0` have the same switch, so one function gives the family and each table adds
/// its base id.
fn ground_family(class: u8) -> Option<u32> {
    match class {
        2 | 3 | 5 => Some(0),
        4 | 6 => Some(1),
        7..=9 => Some(2),
        10 => Some(5),
        11 => Some(8),
        12 => Some(3),
        13 | 14 => Some(7),
        15 | 32 => Some(6),
        _ => None,
    }
}

/// The landing sound of floor class `class` (`FUN_140254b30`): 70, 71, 72, 75, 78, 73, 77, 76 by
/// family, 151..=160 for classes 21..=30.
pub fn landing_sound(class: u8) -> Option<u32> {
    match class {
        21..=30 => Some(151 + (class as u32 - 21)),
        _ => ground_family(class).map(|f| 70 + f),
    }
}

/// The take-off sound of floor class `class` (`FUN_1402549e0`): 61, 62, 63, 66, 69, 64, 68, 67 by
/// family, 141..=150 for classes 21..=30.
pub fn takeoff_sound(class: u8) -> Option<u32> {
    match class {
        21..=30 => Some(141 + (class as u32 - 21)),
        _ => ground_family(class).map(|f| 61 + f),
    }
}

/// `DAT_1402f5da0`, indexed by `class - 2` (33 entries, -1 = none). READ.
const IMPACT_SOUND: [i32; 33] = [-1, -1, 8, 8, 9, 8, 9, 10, 10, 10, 13, 16, 11, 15, 15, -1, -1, -1, -1, -1, -1, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, -1, 10];

/// Whether a stage contact of `class` spawns an impact decal at all (`FUN_14021d3f0`: its switch
/// over `class - 2` leaves every other class without an effect, and so without the sound). The
/// stage-number test is the program's own: `DAT_1403e0304 % 10 == 8` (Sein) has a slightly
/// different list (classes 4 and 24 are not in it).
pub fn impact_effect_type(class: u8, stage_number: u32) -> Option<u8> {
    let sein = stage_number % 10 == 8;
    match class {
        3 | 5 | 6 | 21 | 25 | 27 | 2 => Some(0),
        4 if !sein => Some(0),
        7..=10 | 13 | 26 | 28..=30 | 32 => Some(1),
        24 if !sein => Some(1),
        _ => None,
    }
}

/// The surface-hit sound a stage contact of `class` plays (`FUN_14021d740`, `0x14021dxxx`): the
/// effect must exist for the class, then `DAT_1402f5da0[class - 2]`; the Lane stages (number % 10
/// = 7) play common/199 for every contact.
pub fn impact_sound(class: u8, stage_number: u32) -> Option<u32> {
    impact_effect_type(class, stage_number)?;
    if stage_number % 10 == 7 {
        return Some(199);
    }
    let i = (class as usize).checked_sub(2)?;
    u32::try_from(*IMPACT_SOUND.get(i)?).ok()
}

/// The number of a stage id ("st06" -> 6, "st16" -> 16, "st09_02" -> 9): the program's
/// `DAT_1403e0304`, a stage number (`% 10` picks the family).
pub fn stage_number(stage: &str) -> u32 {
    let digits: String = stage.strip_prefix("st").unwrap_or(stage).chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0)
}

/// The sound-group queue of the stage objects (`FUN_140201ce0` queues, `FUN_140201d30` flushes once
/// per tick): while the 30-program-tick cooldown runs the queue is thrown away; otherwise the
/// first entry plays, the cooldown restarts, and each later entry plays only if it is at least
/// `PROP_SOUND_SPACING` units from every position already played in the burst, at most five in all.
#[derive(Default, Debug)]
pub struct PropSounds {
    cooldown_until: u64,
    pending: Vec<(i32, Vec3)>,
}

/// The cooldown in sim ticks: 30 program ticks (`DAT_140531c5c = 0x1e`) at 30 Hz, on a 60 Hz sim.
pub const PROP_SOUND_COOLDOWN: u64 = 60;
/// `DAT_1402f81b8`, read as 500.0: a queued position closer than this to one already played is skipped.
pub const PROP_SOUND_SPACING: f32 = 500.0;
/// `FUN_140201ce0` holds at most 0x40 entries.
const PROP_QUEUE_MAX: usize = 0x40;
/// At most five sounds per burst (`lVar9 > 4` ends the loop).
const PROP_BURST_MAX: usize = 5;

impl PropSounds {
    /// `FUN_140201ce0`: queue `group` (ignored when negative or when 64 are waiting).
    pub fn push(&mut self, group: i32, pos: Vec3) {
        if group >= 0 && self.pending.len() < PROP_QUEUE_MAX {
            self.pending.push((group, pos));
        }
    }

    /// `FUN_140201d30`, at sim tick `now`: the ids to play this tick (`bit` supplies the random
    /// bit of each pick).
    pub fn flush(&mut self, now: u64, mut bit: impl FnMut() -> u32) -> Vec<u32> {
        if now < self.cooldown_until {
            self.pending.clear();
            return Vec::new();
        }
        let queued = std::mem::take(&mut self.pending);
        let Some(&(first, first_pos)) = queued.first() else { return Vec::new() };
        self.cooldown_until = now + PROP_SOUND_COOLDOWN;
        let mut out: Vec<u32> = prop_group_id(first, bit()).into_iter().collect();
        let mut played = vec![first_pos];
        for &(group, pos) in &queued[1..] {
            if played.iter().any(|p| p.distance(pos) < PROP_SOUND_SPACING) {
                continue;
            }
            out.extend(prop_group_id(group, bit()));
            played.push(pos);
            if played.len() >= PROP_BURST_MAX {
                break;
            }
        }
        out
    }
}

/// A shot or blast contact closer than this to a surface-hit sound of the same frame is the same
/// impact (the program's decal check, `FUN_1401c9210`, refuses a new decal inside an active one's
/// radius, which comes from the decal model: UNCONFIRMED, so this is only enough to stop one
/// impact from sounding twice).
const SAME_IMPACT_RADIUS: f32 = 30.0;

/// The radius inside which a live impact mark refuses a new one (and so its sound): the bound of the mark
/// models `mdl_hit_iron / stone / soil` (MD00 slots 40-42, program model slots 0x40-0x42), |centre| +
/// radius of their mesh's stored sphere, the largest over its meshes (`FUN_1401c8c10`, type entry +0x58).
/// READ: 24.57 for all three models on every stage that has them.
pub const MARK_RADIUS: f32 = 24.57;

/// Live impact marks kept at once (`FUN_1401c9210`: a ring of 32, the oldest overwritten).
pub const MAX_MARKS: usize = 32;

/// Impact marks refuse later surface-hit sounds inside `MARK_RADIUS`, kept until the ring overwrites them
/// (`PD_SFX_NO_MARKS=1` turns this off). UNCONFIRMED: how long a mark lives (`FUN_1401c8e60` ages it only
/// when its model has an animation; the hit models appear to have none, so it lives until overwritten or the
/// camera enters its box; the camera case is not built).
pub fn marks_on() -> bool {
    std::env::var_os("PD_SFX_NO_MARKS").is_none()
}

/// The stage's ground data for the sound systems.
#[derive(Resource, Default)]
pub struct StageSound {
    stage: String,
    surf: Option<SurfaceGround>,
    /// Placement index -> the sound group of its type's slot 1 (the object broke).
    break_group: HashMap<u32, i32>,
    props: PropSounds,
    /// Surface-hit sounds played this tick: (position, sim tick).
    recent: Vec<(Vec3, u64)>,
    /// The live impact marks (newest last, at most `MAX_MARKS`).
    marks: Vec<Vec3>,
    rng: u32,
    /// `PD_SFX_LOG=1`: log each stage contact with its class.
    log: bool,
}

impl StageSound {
    /// Load the stage's surface sidecar and its objects' sound groups (`<stage>.props.json`,
    /// `types[].sound_slots`) when the stage changed. Missing files leave the lookups empty.
    pub fn ensure(&mut self, cache: &std::path::Path, stage: &str) {
        if self.stage == stage {
            return;
        }
        self.stage = stage.to_string();
        self.log = std::env::var_os("PD_SFX_LOG").is_some();
        self.surf = SurfaceGround::load(cache, stage);
        self.break_group.clear();
        self.props = PropSounds::default();
        self.recent.clear();
        self.marks.clear();
        let props: serde_json::Value = std::fs::read_to_string(cache.join(format!("{stage}.props.json"))).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for o in props["objects"].as_array().into_iter().flatten() {
            let (Some(i), Some(ty)) = (o["index"].as_u64(), o["type"].as_u64()) else { continue };
            if let Some(g) = props["types"][ty as usize]["sound_slots"][1].as_i64().filter(|&g| g >= 0) {
                self.break_group.insert(i as u32, g as i32);
            }
        }
    }

    pub fn stage_number(&self) -> u32 {
        stage_number(&self.stage)
    }

    /// The floor class under a body at `pos` (the feet, a hair of tolerance for rounding).
    pub fn feet_class(&self, pos: Vec3) -> Option<u8> {
        let class = self.surf.as_ref()?.class_below(pos.x, pos.z, pos.y + 1.5);
        if self.log {
            info!("sfx: feet at {pos:?} class {class:?}");
        }
        class
    }

    /// The surface-hit sound for a contact of the stage within `reach` of `at`, once per impact.
    pub fn impact_at(&mut self, at: Vec3, reach: f32, tick: u64) -> Option<u32> {
        self.recent.retain(|&(_, t)| t == tick);
        let class = self.surf.as_ref()?.class_nearest(at, reach)?;
        if self.log {
            info!("sfx: stage contact at {at:?} reach {reach} class {class}");
        }
        self.contact_sound(class, at, tick)
    }

    /// What the program's sphere probe (`FUN_140163b50`: a knock-down landing, a heavy airborne landing, a
    /// blast) touches: the first triangle in stage-file order within `radius` of `centre`, as (index,
    /// class). `PD_SFX_NEAREST_CONTACT=1` gives the old nearest-face answer (A/B).
    pub fn sphere_probe(&self, centre: Vec3, radius: f32) -> Option<(usize, u8)> {
        let surf = self.surf.as_ref()?;
        let hit = if std::env::var_os("PD_SFX_NEAREST_CONTACT").is_some() {
            surf.class_nearest(centre, radius).map(|c| (0, c))
        } else {
            surf.first_touching(centre, radius)
        };
        if self.log {
            info!("sfx: sphere probe at {centre:?} radius {radius} -> {hit:?}");
        }
        hit
    }

    /// The surface-hit sound of a stage contact of `class` at `at`, once per impact.
    pub fn contact_sound(&mut self, class: u8, at: Vec3, tick: u64) -> Option<u32> {
        self.recent.retain(|&(_, t)| t == tick);
        let id = impact_sound(class, self.stage_number())?;
        if marks_on() {
            // `FUN_1402422a0` plays the sound only when `FUN_1401c9210` placed the mark, and that refuses a
            // mark inside a live one's radius (the mark model's bound, `MARK_RADIUS`).
            if self.marks.iter().any(|p| p.distance(at) < MARK_RADIUS) {
                if self.log {
                    info!("sfx: mark at {at:?} refused (a live mark is within {MARK_RADIUS})");
                }
                return None;
            }
            if self.marks.len() == MAX_MARKS {
                self.marks.remove(0);
            }
            self.marks.push(at);
            return Some(id);
        }
        if self.recent.iter().any(|&(p, _)| p.distance(at) < SAME_IMPACT_RADIUS) {
            return None;
        }
        self.recent.push((at, tick));
        Some(id)
    }

    /// The surface-hit sound of a shot that ended on the stage: `from` is where it was last seen
    /// flying (its program position), `step` the program step it was about to make. Once per impact
    /// (the same-frame rule of `impact_at`).
    pub fn shot_contact(&mut self, from: Vec3, step: Vec3, tick: u64) -> Option<u32> {
        self.recent.retain(|&(_, t)| t == tick);
        let surf = self.surf.as_ref()?;
        // READ (`FUN_1401633b0`): only a step that crosses a face makes a contact, so a shot that ends standing
        // still (a crawler whose life ran out) touches nothing. Our end point can stop short of the face the
        // program's step crossed, hence the short reach past the step (UNCONFIRMED geometry slack).
        if step.length() <= 0.01 {
            return None;
        }
        let class = surf.class_along(from, from + step * 1.5).or_else(|| surf.class_nearest(from + step, SHOT_CONTACT_REACH))?;
        if self.log {
            info!("sfx: shot contact at {from:?} step {step:?} class {class}");
        }
        self.contact_sound(class, from, tick)
    }

    /// The object `index` broke at `pos`: queue its type's sound group.
    pub fn prop_broke(&mut self, index: u32, pos: Vec3) {
        if let Some(&g) = self.break_group.get(&index) {
            self.props.push(g, pos);
        }
    }

    /// Flush the object-sound queue for sim tick `now`.
    pub fn flush_props(&mut self, now: u64) -> Vec<u32> {
        let mut rng = if self.rng == 0 { 0x9E37_79B9 } else { self.rng };
        let ids = self.props.flush(now, || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng >> 14) & 1
        });
        self.rng = rng;
        ids
    }
}

/// `true` when the environment variable `name` is set: the A/B switches of the film comparison
/// (`PD_SFX_NO_SHOT_SURFACE` / `PD_SFX_NO_BURST_SURFACE` / `PD_SFX_NO_MARKS` turn the shot / burst surface sounds and the impact marks OFF; `PD_SFX_OLD_LANDING`); inert otherwise.
pub fn switched_off(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// The stage id the sound systems work on (the match's stage, else the viewer's).
pub fn current_stage(cfg: &crate::Config, setup: Option<&crate::frontend::state::MatchSetup>) -> String {
    match setup {
        Some(s) if !cfg.legacy => s.stage.clone(),
        _ => cfg.stage.clone(),
    }
}

/// What the combat events of this frame sound like where it depends on the ground: the knock-down
/// landing and the airborne landing (floor-material landing sound, plus the surface-hit sound of
/// the shock a heavy landing sends into the stage), a skill jump's take-off, and a blast's contact
/// with the stage. Pushes into the shared `CueQueue`; `play_sfx` plays them.
#[allow(clippy::too_many_arguments)]
pub fn ground_cues(
    world: Res<crate::World>,
    cfg: Res<crate::Config>,
    setup: Option<Res<crate::frontend::state::MatchSetup>>,
    lib: Res<crate::sfx::SfxLib>,
    mut ground: ResMut<StageSound>,
    mut queue: ResMut<crate::sfx::CueQueue>,
    mut seen_event: Local<u32>,
    mut apex: Local<[f32; crate::combat::MAX_PLAYERS]>,
) {
    use crate::combat::CombatEvent;
    let c = &world.0.combat;
    ground.ensure(&cfg.cache, &current_stage(&cfg, setup.as_deref()));
    if c.events.last().map_or(0, |e| e.0) < *seen_event {
        *seen_event = 0;
    }
    let tick = world.0.tick;
    let vol = crate::sfx::VOL_SFX;
    let mut ids: Vec<u32> = Vec::new();
    // The sphere probe's one contact this tick (`FUN_1402021f0` keeps one per probe, and every landing and
    // blast shares the probe `DAT_1404becb8`): the first chunk in file order any of them touched.
    let mut sphere: Option<(usize, u8, Vec3)> = None;
    let mut probe = |ground: &StageSound, centre: Vec3, radius: f32| {
        if let Some((i, c)) = ground.sphere_probe(centre, radius) {
            if sphere.is_none_or(|s| i < s.0) {
                sphere = Some((i, c, centre));
            }
        }
    };
    let from_seq = *seen_event;
    for &(seq, ev) in c.events.iter().filter(|(seq, _)| *seq > from_seq) {
        *seen_event = seq;
        match ev {
            // A knock-down flight ends (82 is `play_sfx`'s): the floor material's landing sound and
            // the contact of the landing shock (probe radius 10 / 15 / 20 by fall class, `0x140254c80`).
            CombatEvent::KnockLanded { who } => {
                let pos = world.0.players[who].pos;
                let class = ground.feet_class(pos);
                // No surface under the feet in the sidecar: the sound the program played in 35 of 36
                // recorded landings (class 2 / 3 / 5), as before this lane.
                ids.extend(class.and_then(landing_sound).or(class.is_none().then_some(crate::sfx::SFX_KNOCK_GROUND)));
                // The shock's probe sphere: radius by fall class around the body centre (`FUN_140254c80`).
                let (centre, reach) = if switched_off("PD_SFX_OLD_LANDING") { (pos, 20.0) } else { knock_landing_probe(pos, apex[who] - pos.y) };
                probe(&ground, centre, reach);
            }
            // An airborne landing: the same landing sound; the shock (and its surface-hit sound)
            // only for a fall over 80 units (`FUN_140254c80` state 5, `DAT_1402f8040`). A drop under the
            // step height is the sim following the ground (or a teleport to a start point), not a fall:
            // UNCONFIRMED where the program's own airborne state begins.
            CombatEvent::Landed { who, fall } if fall >= crate::sim::STEP_HEIGHT => {
                let pos = world.0.players[who].pos;
                let class = ground.feet_class(pos);
                ids.extend(class.and_then(landing_sound).or(class.is_none().then_some(crate::sfx::SFX_KNOCK_GROUND)));
                if fall > crate::sim::LAND_FALL_HEAVY {
                    // State 5, fall over 80: `FUN_140163b50(.., body position, DAT_1402f7f7c = 10, 1)`.
                    probe(&ground, pos, AIRBORNE_SHOCK_REACH);
                }
            }
            CombatEvent::JumpTakeoff { who } => {
                let class = ground.feet_class(world.0.players[who].pos);
                ids.extend(class.and_then(takeoff_sound).or(class.is_none().then_some(crate::sfx::SFX_JUMP_TAKEOFF)));
            }
            // A blast tests the stage with a sphere of its own radius (`FUN_14026dbd0` ->
            // `FUN_140163b50`).
            CombatEvent::BlastStarted { pos, radius, .. } => probe(&ground, pos, radius.max(1.0)),
            // A drop of a Rain cloud (or a shell) bursting where it ended: its sweep touched the stage
            // there (`FUN_14026ca50`), a contact like a shot's.
            CombatEvent::Burst { pos, skill, .. } => {
                if ground.log {
                    info!("sfx: burst skill {skill} at {:.2},{:.2},{:.2} tick {tick}", pos.x, pos.y, pos.z);
                }
                // READ (P0-SOUND section 13): a rain drop or shell's last step crossing the stage is a sweep contact.
    if !switched_off("PD_SFX_NO_BURST_SURFACE") {
                    ids.extend(ground.impact_at(pos, BURST_CONTACT_REACH, tick));
                }
            }
            _ => {}
        }
    }
    if let Some((i, class, centre)) = sphere {
        let at = ground.surf.as_ref().and_then(|s| s.mark_point(i, centre)).unwrap_or(centre);
        ids.extend(ground.contact_sound(class, at, tick));
    }
    for id in ids {
        if lib.has_common(id) {
            queue.0.push(Cue::Sfx(id, vol));
        }
    }
    // The highest point of each body's current flight (`FUN_140254c80` measures a fall from it): kept
    // while the body is off the ground or in a knock-down flight, the floor under it otherwise.
    for i in world.0.combat.roster.slots() {
        let (p, f) = (&world.0.players[i], &world.0.combat.fighters[i]);
        apex[i] = if p.grounded && !f.flying { p.pos.y } else { apex[i].max(p.pos.y) };
    }
}

/// The program's fall classes for a knock-down landing (`FUN_140254c80`, state 0x11): a fall under 40
/// units probes with radius 10, up to 80 with 15, over 80 with 20 (`DAT_1402f7f7c`, `7fb0`, `7fc4`).
pub fn landing_probe_radius(fall: f32) -> f32 {
    if fall < crate::sim::LAND_FALL_MIN {
        10.0
    } else if fall <= crate::sim::LAND_FALL_HEAVY {
        15.0
    } else {
        20.0
    }
}

/// The centre height of a playable body above its feet, 9 units (charaset record +0x12 / 10, stored at
/// fighter `+0x12e98`: PROGRAM-RE.md "Character table in memory"). The knock-down landing probe is
/// centred `pos - axis * 9` (`FUN_140254c80`); the axis sign is not read, but the floor, the only face a
/// landing touches, is 9 units from the centre either way.
pub const CENTRE_HEIGHT: f32 = 9.0;

/// Where the knock-down landing's probe sphere is, and how big: (centre, radius).
pub fn knock_landing_probe(feet: Vec3, fall: f32) -> (Vec3, f32) {
    (feet + Vec3::Y * CENTRE_HEIGHT, landing_probe_radius(fall))
}

/// A shot's stage contact when its sweep crosses no face of the sidecar: the nearest face this close to
/// the point it would have reached (a shot's own radius is 5, `Shot::radius`).
const SHOT_CONTACT_REACH: f32 = 5.0;

/// How near a burst's point must be to a face of the stage to count as touching it: the shot's own radius.
const BURST_CONTACT_REACH: f32 = 5.0;

/// The probe radius of a heavy airborne landing (state 5, fall over 80): `DAT_1402f7f7c` = 10.
const AIRBORNE_SHOCK_REACH: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// `FUN_140254b30` / `FUN_1402549e0`, case by case.
    #[test]
    fn landing_and_takeoff_follow_the_floor_class() {
        let land = |c| landing_sound(c);
        let off = |c| takeoff_sound(c);
        for (class, l, t) in [(2, 70, 61), (3, 70, 61), (5, 70, 61), (4, 71, 62), (6, 71, 62), (7, 72, 63), (8, 72, 63), (9, 72, 63), (10, 75, 66), (11, 78, 69), (12, 73, 64), (13, 77, 68), (14, 77, 68), (15, 76, 67), (32, 76, 67)] {
            assert_eq!((land(class), off(class)), (Some(l), Some(t)), "class {class}");
        }
        assert_eq!(land(21), Some(151));
        assert_eq!(land(30), Some(160));
        assert_eq!(off(21), Some(141));
        assert_eq!(off(30), Some(150));
        for class in [0, 1, 16, 17, 18, 19, 20, 31, 33, 255] {
            assert_eq!((land(class), off(class)), (None, None), "class {class} is silent");
        }
    }

    /// `FUN_14021d3f0`'s switch decides which classes spawn a decal; `DAT_1402f5da0` the id.
    #[test]
    fn surface_hit_sound_by_class() {
        // st06's classes: 4 and 5 -> 8, 6 -> 9, 26 and 27 -> 10; 1, 2, 16, 18 -> nothing.
        for (class, id) in [(4, Some(8)), (5, Some(8)), (6, Some(9)), (26, Some(10)), (27, Some(10)), (1, None), (2, None), (16, None), (18, None)] {
            assert_eq!(impact_sound(class, 6), id, "class {class}");
        }
        // The rest of the table: 7 -> 8, 8 -> 9, 9 / 10 -> 10, 13 -> 16, 24..=30 -> 10, 32 -> 10.
        for (class, id) in [(7, 8), (8, 9), (9, 10), (10, 10), (13, 16), (28, 10), (29, 10), (30, 10), (32, 10), (24, 10), (25, 10), (27, 10)] {
            assert_eq!(impact_sound(class, 2), Some(id), "class {class}");
        }
        // Classes 11, 12, 14, 15 are in the table but not in the effect switch: silent; class 21 is in
        // the switch but its table entry is -1.
        for class in [11, 12, 14, 15, 17, 21, 22, 23, 31, 33, 34] {
            assert_eq!(impact_sound(class, 2), None, "class {class}");
        }
        // Sein's variant drops class 4 and 24; Lane plays 199 for every contact.
        assert_eq!(impact_sound(4, 8), None);
        assert_eq!(impact_sound(5, 8), Some(8));
        assert_eq!(impact_sound(5, 17), Some(199));
        assert_eq!(impact_sound(1, 17), None);
    }

    /// `FUN_140254c80`, state 0x11: a fall under 40 probes with radius 10, up to 80 with 15, over 80 with 20,
    /// from the body centre 9 units above the feet.
    #[test]
    fn the_knock_down_landing_probe_follows_the_fall_class() {
        assert_eq!(landing_probe_radius(0.0), 10.0);
        assert_eq!(landing_probe_radius(7.7), 10.0);
        assert_eq!(landing_probe_radius(39.9), 10.0);
        assert_eq!(landing_probe_radius(40.0), 15.0);
        assert_eq!(landing_probe_radius(80.0), 15.0);
        assert_eq!(landing_probe_radius(80.1), 20.0);
        let (centre, r) = knock_landing_probe(Vec3::new(1.0, 330.0, 2.0), 65.9);
        assert_eq!((centre, r), (Vec3::new(1.0, 339.0, 2.0), 15.0));
    }

    #[test]
    fn stage_numbers() {
        assert_eq!(stage_number("st06"), 6);
        assert_eq!(stage_number("st16"), 16);
        assert_eq!(stage_number("st09_02"), 9);
        assert_eq!(stage_number("nope"), 0);
    }

    #[test]
    fn group_ids_pick_by_random_bit() {
        assert_eq!(prop_group_id(1, 0), Some(8));
        assert_eq!(prop_group_id(1, 1), None, "group 1's second slot is -1: silent half the time");
        assert_eq!(prop_group_id(17, 0), Some(17));
        assert_eq!(prop_group_id(17, 1), Some(172));
        assert_eq!(prop_group_id(29, 1), Some(190));
        assert_eq!(prop_group_id(36, 0), Some(191));
        assert_eq!(prop_group_id(-1, 0), None);
        assert_eq!(prop_group_id(47, 0), None);
    }

    /// The flush: cooldown, first entry always, later ones only 500+ units from every played one, <= 5.
    #[test]
    fn object_sound_queue_rules() {
        let mut q = PropSounds::default();
        q.push(29, Vec3::ZERO);
        q.push(17, Vec3::new(100.0, 0.0, 0.0)); // too close to the first
        q.push(22, Vec3::new(600.0, 0.0, 0.0)); // far enough
        q.push(23, Vec3::new(650.0, 0.0, 0.0)); // close to the third
        assert_eq!(q.flush(10, || 0), vec![189, 22]);
        // Cooling down: a new break is thrown away, not kept for later.
        q.push(17, Vec3::ZERO);
        assert!(q.flush(11, || 0).is_empty());
        assert!(q.flush(10 + PROP_SOUND_COOLDOWN - 1, || 0).is_empty());
        assert!(q.pending.is_empty());
        q.push(17, Vec3::ZERO);
        assert_eq!(q.flush(10 + PROP_SOUND_COOLDOWN, || 1), vec![172]);
        // Negative groups are never queued; at most five play.
        let mut q = PropSounds::default();
        q.push(-1, Vec3::ZERO);
        assert!(q.flush(1000, || 0).is_empty());
        for i in 0..8 {
            q.push(17, Vec3::new(i as f32 * 1000.0, 0.0, 0.0));
        }
        assert_eq!(q.flush(2000, || 0).len(), 5);
        // A silent first pick still starts the cooldown.
        let mut q = PropSounds::default();
        q.push(1, Vec3::ZERO);
        assert!(q.flush(5000, || 1).is_empty());
        q.push(17, Vec3::ZERO);
        assert!(q.flush(5001, || 0).is_empty());
    }
}
