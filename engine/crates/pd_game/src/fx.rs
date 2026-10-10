//! Combat objects in the world: capsules, projectiles, barriers and the
//! game's own skill effects (effects.rs), and the arena objects' own hit and
//! break effects (`object_effects`, registered by `plugin`). Reads sim state
//! only. Capsules and skills without an effect in a slot still use plain
//! stand-in shapes.

use crate::combat::{Capsule, CombatEvent, AURA_PARTICLE, CHEST, MAX_PLAYERS};
use crate::effects::{self, FxLibrary, SLOT_BLOCK, SLOT_CAST, SLOT_HIT, SLOT_IMPACT, SLOT_PROJECTILE};

/// Slot 5 of an effect: the defence break, the charge release, a status loop's end,
/// the Rain cloud (`FUN_14026cd70` code 10, `FUN_140259140`, `FUN_14025cbd0`,
/// `FUN_14026b480`). effects.rs names slots 0-4 only.
const SLOT_BREAK: usize = 5;
use crate::frontend::state::MatchScene;
use crate::capsule_birth;
use crate::orb_material;
use crate::{PlayerVisual, World};
use bevy::prelude::*;
use std::collections::HashMap;

/// Skill id -> its effect(s) ("eft0042"): ground and mid-air-cast action
/// rows can name different effects (`action.ground.effect` /
/// `action.air.effect` in `cache/skills.json`, the action row's w2 -
/// FORMATS-EFFECTS.md "Skill -> effect"), not the skill's text id. No
/// borrowing from other skills: a side with no effect (or whose effect has
/// no exported file) is simply not shown.
#[derive(Resource, Default)]
pub struct SkillFx(HashMap<i16, (Option<String>, Option<String>)>);

impl SkillFx {
    pub fn load(skills_json: &std::path::Path) -> Self {
        let v: serde_json::Value = std::fs::read_to_string(skills_json).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let fx_dir = skills_json.with_file_name("fx");
        let exists = |name: &str| fx_dir.join(format!("{name}.fx.json")).exists();
        let effect_of = |s: &serde_json::Value, side: &str| s["action"][side]["effect"].as_i64().map(|n| format!("eft{n:04}")).filter(|n| exists(n));
        let mut map = HashMap::new();
        for s in v["skills"].as_array().into_iter().flatten() {
            let Some(id) = s["id"].as_i64() else { continue };
            map.entry(id as i16).or_insert_with(|| (effect_of(s, "ground"), effect_of(s, "air")));
        }
        SkillFx(map)
    }

    /// `airborne`: whether the caster was off the ground, to prefer the
    /// mid-air-cast row's effect; falls back to the skill's other side if
    /// the preferred one has none (still the same skill, not a borrow).
    pub(crate) fn name(&self, skill: i16, airborne: bool) -> Option<&str> {
        let (ground, air) = self.0.get(&skill)?;
        if airborne { air.as_deref().or(ground.as_deref()) } else { ground.as_deref().or(air.as_deref()) }
    }
}

/// Load the effects of every skill in this match's arsenals (both cast sides) and the status loops
/// up front and keep them (`effects::FxPreload`), so a cast does not load its effect from disk.
pub fn preload_match_effects(world: Res<World>, map: Res<SkillFx>, assets: Res<AssetServer>, mut lib: ResMut<FxLibrary>, mut preload: ResMut<effects::FxPreload>, mut seen: Local<Vec<Vec<i16>>>) {
    if !effects::FxPreload::enabled() {
        return;
    }
    let c = &world.0.combat;
    // Only when the arsenals change (a new match), not every frame.
    if seen.len() == c.arsenals.len() && seen.iter().zip(&c.arsenals).all(|(a, b)| a == b) {
        return;
    }
    *seen = c.arsenals.to_vec();
    let mut names: Vec<String> = STATUS_LOOPS.iter().map(|(n, _)| n.to_string()).collect();
    // The Level aura (`aura_fx`): its definition is 3.4 MB of JSON, read and parsed on the spot the first time a
    // fighter's aura grew a tier (a 73 ms frame in a profiler trace, 2026-10-08). Read with the rest instead.
    names.push(crate::aura_fx::AURA_EFFECT.to_string());
    for &skill in c.arsenals.iter().flatten() {
        for airborne in [false, true] {
            if let Some(n) = map.name(skill, airborne) {
                names.push(n.to_string());
            }
        }
    }
    preload.hold(names, &assets, &mut lib);
}

/// The scale of a blast's slot-2 impact effect: the blast radius (world units)
/// x 0.2 (`DAT_1402f7d08`, `FUN_14026cd70` cases 1-3 / 8), so a 30 u Bomb
/// explodes at x6, a 40 u Big Bomb or Photon Wave at x8, a 50 u Timer Mine x10.
pub fn impact_scale(radius: f32) -> f32 {
    radius * 0.2
}

/// Casts, hits and blocks start effect slots on the players (as children, so
/// they follow them); impacts on the ground play where they happen.
/// One-shot; they despawn when done.
#[allow(clippy::too_many_arguments)]
pub fn skill_effects(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut lib: ResMut<FxLibrary>,
    map: Res<SkillFx>,
    world: Res<World>,
    visuals: Query<(Entity, &PlayerVisual, &GlobalTransform)>,
    mut charge: Local<[Vec<Entity>; MAX_PLAYERS]>,
    mut seen_cast: Local<[u16; MAX_PLAYERS]>,
    mut seen_event: Local<u32>,
    mut prev_shots: Local<Vec<(u32, Transform, i16, usize)>>,
) {
    let c = &world.0.combat;
    if effects::env_once("PD_FX_LOG").is_some() {
        // The two-player line is unchanged (logs diff across versions); seats
        // 3 and 4, when playing, add their own health and distance to player 1.
        let f = &c.fighters[0];
        eprintln!("[fxlog] tick {} p1 {:?}/{} motion {:?} skill {} seq {} health {} / {} dist {:.1} shots {}", world.0.tick, f.action, f.action_ticks, world.0.players[0].motion, f.cast_skill, f.cast_seq, f.health, c.fighters[1].health, world.0.players[0].pos.distance(world.0.players[1].pos), c.projectiles.len());
        for j in (2..MAX_PLAYERS).filter(|&j| c.roster.is_active(j)) {
            eprintln!("[fxlog]   p{} health {} dist {:.1}", j + 1, c.fighters[j].health, world.0.players[0].pos.distance(world.0.players[j].pos));
        }
        for p in &c.projectiles {
            eprintln!("[fxlog]   shot {} skill {} at {:.0?} (p1 {:.0?}, p2 {:.0?})", p.id, p.skill, p.pos, world.0.players[0].pos, world.0.players[1].pos);
        }
    }
    let visual = |i: usize| visuals.iter().find(|(_, v, _)| v.0 == i).map(|(e, _, _)| e);
    // Ground vs mid-air-cast effect (FORMATS-EFFECTS.md "Skill -> effect"):
    // every play site below passes whether the caster was off the ground.
    let play_from = |commands: &mut Commands, lib: &mut FxLibrary, skill: i16, airborne: bool, slot: usize, on: usize, from: f32| {
        let (Some(name), Some(parent)) = (map.name(skill, airborne), visual(on)) else { return };
        if let Some(e) = effects::play_from(commands, &assets, lib, name, slot, Transform::IDENTITY, false, None, from) {
            // On the player: its pieces ride on the bones their joints name
            // (Psycho Knife's blade on RSWORD, hit sparks on BODY2, ...).
            commands.entity(e).insert((ChildOf(parent), effects::FxHost(parent)));
        }
    };
    let play = |commands: &mut Commands, lib: &mut FxLibrary, skill: i16, airborne: bool, slot: usize, on: usize| play_from(commands, lib, skill, airborne, slot, on, 0.0);
    // The hit effect (slot 3) is made with a COPY of the victim's matrix taken when it
    // starts: only slot 0 of an effect follows the live skeleton root (`FUN_14018a040`,
    // `FUN_14018a8b0`); pieces that name a joint still ride that bone (`FxHost`).
    let play_snapshot = |commands: &mut Commands, lib: &mut FxLibrary, skill: i16, airborne: bool, slot: usize, on: usize| {
        let (Some(name), Some((parent, _, gt))) = (map.name(skill, airborne), visuals.iter().find(|(_, v, _)| v.0 == on)) else { return };
        let t = gt.compute_transform();
        let at = Transform::from_translation(t.translation).with_rotation(t.rotation);
        if let Some(e) = effects::play(commands, &assets, lib, name, slot, at, false, None) {
            commands.entity(e).insert((effects::FxHost(parent), MatchScene));
        }
    };
    // A looping effect on a player (charge cues, status loops), riding the player.
    let play_loop = |commands: &mut Commands, lib: &mut FxLibrary, skill: i16, airborne: bool, slot: usize, on: usize| -> Option<Entity> {
        let (name, parent) = (map.name(skill, airborne)?, visual(on)?);
        let e = effects::play(commands, &assets, lib, name, slot, Transform::IDENTITY, true, None)?;
        commands.entity(e).insert((ChildOf(parent), effects::FxHost(parent)));
        Some(e)
    };
    let play_at = |commands: &mut Commands, lib: &mut FxLibrary, skill: i16, airborne: bool, slot: usize, at: Transform| {
        if let Some(e) = map.name(skill, airborne).and_then(|name| effects::play(commands, &assets, lib, name, slot, at, false, None)) {
            commands.entity(e).insert(MatchScene);
        }
    };
    for i in c.roster.slots() {
        let f = &c.fighters[i];
        // Back to 0 = a new round or match reset the fighter, not a cast.
        if f.cast_seq != seen_cast[i] {
            seen_cast[i] = f.cast_seq;
            // The program plays the cast effect (slot 0) from cast start --
            // through a short-range skill's wind-up and dash and into the
            // strike -- the same moment the cast clip itself now starts
            // (main.rs::cast_clip_now, "Blades" lane): PROGRAM-RE.md's Psycho
            // Knife timeline gives "slot 0 at cast start", not at the strike.
            // Starting it here, at frame 0, lets the effect's own early
            // frames (a wind-up glow before the swing proper) show, and
            // still lands its authored swing frames at the row's fire tick
            // (`Combat::lunge`), because our sim now holds that same fixed
            // tick schedule. Previously this waited for the dash to end and
            // skipped ahead to the fire tick (`fx::strike_lead_frames`,
            // removed), which discarded every piece windowed before it.
            if f.cast_seq != 0 {
                play(&mut commands, &mut lib, f.cast_skill, c.airborne[i], SLOT_CAST, i);
            }
        }
    }
    // A new match starts its event numbers again from 1.
    if c.events.last().map_or(0, |e| e.0) < *seen_event {
        *seen_event = 0;
    }
    // Skills that hit, were blocked or burst this frame: their vanished
    // shots are accounted for.
    let mut explained: Vec<i16> = Vec::new();
    let alive: Vec<u32> = c.projectiles.iter().map(|p| p.id).collect();
    let vanished: Vec<(Transform, i16, usize)> = prev_shots.iter().filter(|(id, _, _, _)| !alive.contains(id)).map(|&(_, at, skill, owner)| (at, skill, owner)).collect();
    let burst: Vec<i16> = c.events.iter().filter(|(seq, _)| *seq > *seen_event).filter_map(|(_, ev)| if let CombatEvent::Burst { skill, .. } = ev { Some(*skill) } else { None }).collect();
    // Slot 2 is the impact at the contact point: on a hit and on a block as
    // well as on a wall (PROGRAM-RE.md "end / impact", codes 1-3). Where the
    // shot was last seen, or the victim's chest for a strike.
    let contact = |victim: usize, skill: i16| -> Transform {
        let chest = world.0.players[victim].pos + Vec3::Y * CHEST;
        vanished.iter().filter(|(_, s, _)| *s == skill).min_by(|a, b| a.0.translation.distance_squared(chest).total_cmp(&b.0.translation.distance_squared(chest))).map_or(Transform::from_translation(chest), |(at, _, _)| *at)
    };
    // Best-effort: whether the skill's cast was in the air, for effects
    // (hit/impact/burst) that don't carry the caster themselves. `owner`
    // when known (the event's `attacker`); else the caster is whoever is
    // still casting the skill -- `prefer` (a burst's owner) first, then the
    // other active players in slot order -- and a skill nobody is casting
    // any more counts as grounded.
    let airborne_of = |owner: Option<usize>, prefer: Option<usize>, skill: i16| -> bool {
        owner.or_else(|| prefer.filter(|&i| c.roster.is_active(i) && c.fighters[i].cast_skill == skill).or_else(|| c.roster.slots().find(|&i| c.fighters[i].cast_skill == skill))).is_some_and(|i| c.airborne[i])
    };
    // The blast the last `BlastStarted` event announced: (skill, centre, radius).
    let mut blast: Option<(i16, Vec3, f32)> = None;
    for &(seq, ev) in &c.events {
        if seq <= *seen_event {
            continue;
        }
        *seen_event = seq;
        if effects::env_once("PD_FX_LOG").is_some() {
            eprintln!("[fxlog] event {ev:?}");
        }
        match ev {
            CombatEvent::Hit { attacker, victim, skill } => {
                explained.push(skill);
                let air = airborne_of(Some(attacker), None, skill);
                if !burst.contains(&skill) {
                    play_at(&mut commands, &mut lib, skill, air, SLOT_IMPACT, contact(victim, skill));
                }
                play_snapshot(&mut commands, &mut lib, skill, air, SLOT_HIT, victim)
            }
            CombatEvent::Blocked { attacker, victim, skill, broken } => {
                explained.push(skill);
                let air = airborne_of(Some(attacker), None, skill);
                if !burst.contains(&skill) {
                    play_at(&mut commands, &mut lib, skill, air, SLOT_IMPACT, contact(victim, skill));
                }
                // A block starts slot 4 of the defence's effect (code 9); a break starts
                // slot 5 instead (code 10, `0x14026d527`), never both.
                let barrier = c.fighters[victim].cast_skill;
                play(&mut commands, &mut lib, barrier, c.airborne[victim], if broken { SLOT_BREAK } else { SLOT_BLOCK }, victim);
            }
            // An exploding shot or an area going off: its ground impact and
            // hit effects where it went off.
            CombatEvent::Burst { attacker, pos, skill } => {
                explained.push(skill);
                let air = airborne_of(None, Some(attacker), skill);
                // A blast's impact effect is scaled by its radius x 0.2 (`FUN_14026cd70`
                // cases 1-3 and 8): Bomb x6, Big Bomb x8, Timer Mine x10, Photon Wave x8.
                // `BlastStarted` is logged right before its `Burst`.
                let scale = blast.filter(|b| b.0 == skill && b.1.distance(pos) < 0.01).map_or(1.0, |b| impact_scale(b.2));
                play_at(&mut commands, &mut lib, skill, air, SLOT_IMPACT, Transform::from_translation(pos).with_scale(Vec3::splat(scale)));
                // (The hit effect, slot 3, belongs to a victim: it comes with their `Hit`.)
            }
            CombatEvent::BlastStarted { skill, pos, radius, .. } => blast = Some((skill, pos, radius)),
            // A charge skill reached the row's charge-cycle tick: two slot-4 copies of its
            // effect loop on the caster until the button is let go (`FUN_140259540`); the
            // release ends them and plays slot 5 on the caster (`FUN_140259670`,
            // `FUN_140259140`). (The second copy is not phase-shifted by half a loop: that
            // needs the effect length, private to effects.rs.)
            CombatEvent::ChargeHeld { who, skill } => {
                let air = c.airborne[who];
                for _ in 0..2 {
                    if let Some(e) = play_loop(&mut commands, &mut lib, skill, air, SLOT_BLOCK, who) {
                        charge[who].push(e);
                    }
                }
            }
            CombatEvent::ChargeReleased { who, skill } => {
                for e in charge[who].drain(..) {
                    commands.entity(e).despawn();
                }
                play(&mut commands, &mut lib, skill, c.airborne[who], SLOT_BREAK, who);
            }
            // Rain: each drop starts slot 4 of the skill's effect where it begins to fall
            // (`FUN_14026b8d0`); the cloud itself is slot 5 (`sync_combat_visuals`).
            CombatEvent::RainDrop { attacker, pos, skill } => {
                let air = c.airborne[attacker];
                play_at(&mut commands, &mut lib, skill, air, SLOT_BLOCK, Transform::from_translation(pos));
            }
            CombatEvent::Denied { .. } => {}
            // A capsule capture changes the HUD (the slot's new skill) and
            // plays a sound, but spawns no effect: the capsule orb itself is
            // already on the field and simply goes away. Nothing to draw.
            CombatEvent::Capsule { .. } => {}
            _ => {}
        }
    }
    for i in 0..MAX_PLAYERS {
        if !charge[i].is_empty() && c.fighters[i].charge.is_none() {
            for e in charge[i].drain(..) {
                commands.entity(e).despawn();
            }
        }
    }
    // A shot that ended without hitting anyone (a wall, the floor, out of
    // range): its ground impact (slot 2) where it was last seen.
    for &(at, skill, owner) in &vanished {
        if !explained.contains(&skill) && c.round_over.is_none() {
            play_at(&mut commands, &mut lib, skill, c.airborne[owner], SLOT_IMPACT, at);
        }
    }
    // Facing its travel, like the projectile effect (the impact rings stand across the flight).
    *prev_shots = c.projectiles.iter().filter(|p| p.prop.is_none()).map(|p| (p.id, Transform::from_translation(p.pos + p.lead()).with_rotation(Quat::from_rotation_arc(Vec3::Z, (p.pos - p.prev_pos).normalize_or(Vec3::Z))), p.skill, p.owner)).collect();
}

// ---------------------------------------------------------------------------
// Arena-object effects: the stage's own `EF00` records (`pd_convert stagefx`
// -> cache/fx/stage/stNN.objfx.json; docs/FORMATS-STAGES.md "Object
// effects"), played when an object in `Combat::props` is hit or breaks.
// Read-only on the sim.

/// Stage emitter numbers are per frame. Played at the skill effects' rate
/// (effects::FX_FPS, 30: see FORMATS-EFFECTS.md "Frame rate"). The arcs are
/// the same shape at any rate (speed, gravity and life are all per frame).
const STAGE_FX_FPS: f32 = effects::FX_FPS;
/// Debris sprite size per unit of its size word (0.03 -> 3 units, a chip
/// about a hand across; unconfirmed).
const DEBRIS_SIZE_SCALE: f32 = 100.0;
/// Fragment models (table / pole / floor pieces) fly out at this speed.
const FRAGMENT_SPEED: f32 = 45.0;
const FRAGMENT_GRAVITY: f32 = 200.0;

pub fn plugin(app: &mut App) {
    use crate::frontend::state::AppState;
    effects::plugin(app);
    orb_material::plugin(app);
    app.init_resource::<ObjectFx>().init_resource::<OrbAlphaMemory>().add_systems(Startup, effects::setup_particles).add_systems(
        Update,
        (
            (object_effects, effects::attach_fragments, effects::tick_fragments, effects::tick_particles).chain(),
            status_loops,
            (fade_capsule_orbs, tint_flares, force_orb_texture_repeat, orb_motes, tick_despawn_after, capsule_preview, isolate_preview_cameras),
        )
            .run_if(in_state(AppState::Match)),
    );
}

/// The persistent status loops (`FUN_14025cbd0`, table `0x1403e2158`): while status 8
/// (invisible: Optical Camo), 11 (float: Skywalk, Levitate, Demon's Wing) or 33 (Void)
/// is up, slot 4 of effect 518 / 519 / 521 loops on the player; when it ends, slot 5 of
/// the same effect plays once. The effect number is the program's shared one (the
/// skills' own effect files do not hold these).
const STATUS_LOOPS: [(&str, usize); 3] = [("eft0518", 0), ("eft0519", 1), ("eft0521", 2)];

/// Which of the three statuses is up on a fighter (index = `STATUS_LOOPS`).
pub fn status_loops_up(f: &crate::combat::Fighter) -> [bool; 3] {
    [f.status.invisible_ticks > 0, f.status_hover && f.status.hover_ticks > 0, f.status.immune_ticks > 0]
}

#[allow(clippy::too_many_arguments)]
fn status_loops(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut lib: ResMut<FxLibrary>,
    world: Res<World>,
    visuals: Query<(Entity, &PlayerVisual)>,
    alive: Query<&Transform>,
    mut loops: Local<HashMap<(usize, usize), Option<Entity>>>,
) {
    let c = &world.0.combat;
    // (A loop whose slot draws nothing here -- Optical Camo's 518 slot 4 is all screen
    // distortion, hidden until PD_FX_DISTORT -- is still tracked, as `None`.)
    loops.retain(|_, e| e.is_none_or(|e| alive.get(e).is_ok()));
    for i in c.roster.slots() {
        let Some(parent) = visuals.iter().find(|(_, v)| v.0 == i).map(|(e, _)| e) else { continue };
        for (k, up) in status_loops_up(&c.fighters[i]).into_iter().enumerate() {
            let name = STATUS_LOOPS[k].0;
            match (up, loops.get(&(i, k)).copied()) {
                (true, None) => {
                    if effects::env_once("PD_FX_LOG").is_some() {
                        eprintln!("[fxlog] status loop {name} slot 4 start on p{}", i + 1);
                    }
                    let e = effects::play(&mut commands, &assets, &mut lib, name, SLOT_BLOCK, Transform::IDENTITY, true, None);
                    if let Some(e) = e {
                        commands.entity(e).insert((ChildOf(parent), effects::FxHost(parent)));
                    }
                    loops.insert((i, k), e);
                }
                (false, Some(e)) => {
                    if let Some(e) = e {
                        commands.entity(e).despawn();
                    }
                    loops.remove(&(i, k));
                    if effects::env_once("PD_FX_LOG").is_some() {
                        eprintln!("[fxlog] status loop {name} slot 5 end on p{}", i + 1);
                    }
                    if c.round_over.is_none() {
                        if let Some(e) = effects::play(&mut commands, &assets, &mut lib, name, SLOT_BREAK, Transform::IDENTITY, false, None) {
                            commands.entity(e).insert((ChildOf(parent), effects::FxHost(parent)));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

struct ObjRecord {
    kind: u8,
    trigger: u8,
    /// Record -> object, right-handed.
    local: Mat4,
    scale: Vec3,
    bytes: [u8; 12],
    params: [f32; 10],
}

#[derive(Default)]
struct AuxSlot {
    /// "stage/st06_s20": the effect name (glb + fx.json).
    file: Option<String>,
    /// Shader-drawn emitter textures ("fx/stage/st06_s20_d0.png").
    sprites: Vec<Option<String>>,
    /// Draw node names of an untimed model (a fragment set).
    fragments: Vec<String>,
    timed: bool,
}

#[derive(Resource, Default)]
pub struct ObjectFx {
    stage: String,
    /// Placement index -> (type id, object -> world matrix, right-handed).
    objects: HashMap<u32, (usize, Mat4)>,
    types: Vec<Vec<ObjRecord>>,
    type_names: Vec<String>,
    slots: HashMap<usize, AuxSlot>,
}

impl ObjectFx {
    fn load(cache: &std::path::Path, stage: &str) -> Self {
        let read = |p: std::path::PathBuf| -> serde_json::Value { std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() };
        let fxj = read(cache.join(format!("fx/stage/{stage}.objfx.json")));
        let props = read(cache.join(format!("{stage}.props.json")));
        let mut out = ObjectFx { stage: stage.to_string(), ..default() };
        for o in props["objects"].as_array().into_iter().flatten() {
            let (Some(i), Some(ty), Some(m)) = (o["index"].as_u64(), o["type"].as_u64(), o["matrix"].as_array()) else { continue };
            let cols: Vec<f32> = m.iter().filter_map(|v| v.as_f64().map(|x| x as f32)).collect();
            if let Ok(cols) = <[f32; 16]>::try_from(cols) {
                out.objects.insert(i as u32, (ty as usize, Mat4::from_cols_array(&cols)));
            }
        }
        let f3 = |v: &serde_json::Value| Vec3::new(v[0].as_f64().unwrap_or(0.0) as f32, v[1].as_f64().unwrap_or(0.0) as f32, v[2].as_f64().unwrap_or(0.0) as f32);
        for t in fxj["types"].as_array().into_iter().flatten() {
            let recs = t["main"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    let (p, rot) = (f3(&r["position_game"]), f3(&r["rotation_game"]));
                    let lh = Mat4::from_translation(p) * Mat4::from_quat(Quat::from_euler(EulerRot::ZYX, rot.z, rot.y, rot.x));
                    let s = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
                    let mut bytes = [0u8; 12];
                    for (k, b) in bytes.iter_mut().enumerate() {
                        *b = r["bytes"][k].as_u64().unwrap_or(0) as u8;
                    }
                    let mut params = [0f32; 10];
                    for (k, x) in params.iter_mut().enumerate() {
                        *x = r["params"][k].as_f64().unwrap_or(0.0) as f32;
                    }
                    ObjRecord { kind: r["kind"].as_u64().unwrap_or(0) as u8, trigger: r["trigger"].as_u64().unwrap_or(0) as u8, local: s * lh * s, scale: f3(&r["scale"]), bytes, params }
                })
                .collect();
            out.types.push(recs);
            out.type_names.push(t["name"].as_str().unwrap_or_default().to_string());
        }
        for (k, v) in fxj["slots"].as_object().into_iter().flatten() {
            let Ok(slot) = k.parse::<usize>() else { continue };
            let file = v["file"].as_str().map(String::from);
            let mut s = AuxSlot { file: file.clone(), ..default() };
            if let Some(f) = &file {
                let j = read(cache.join(format!("fx/{f}.fx.json")));
                s.sprites = j["sprites"].as_array().into_iter().flatten().map(|sp| sp["texture"].as_str().map(String::from)).collect();
                s.timed = !s.sprites.is_empty();
                s.fragments = j["fragments"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(String::from)).collect();
            }
            out.slots.insert(slot, s);
        }
        out
    }
}

fn srgb_rgba(c: [u8; 4], alpha_scale: f32) -> Vec4 {
    let l = Color::srgba_u8(c[0], c[1], c[2], 255).to_linear();
    Vec4::new(l.red, l.green, l.blue, c[3] as f32 / 255.0 * alpha_scale)
}

/// Play the records of one object whose trigger is in `triggers`.
#[allow(clippy::too_many_arguments)]
fn play_object(commands: &mut Commands, assets: &AssetServer, pm: &effects::ParticleMeshes, mats: &mut effects::PartMats, ofx: &ObjectFx, ty: usize, obj: Mat4, triggers: &[u8], seed: u32) -> usize {
    let Some(recs) = ofx.types.get(ty) else { return 0 };
    let mut played = 0;
    for (k, r) in recs.iter().enumerate().filter(|(_, r)| triggers.contains(&r.trigger)) {
        let seed = seed.wrapping_mul(31).wrapping_add(k as u32);
        let world = obj * r.local;
        let at = world.transform_point3(Vec3::ZERO);
        let p = &r.params;
        let fps = STAGE_FX_FPS;
        let spec = match r.kind {
            // Smoke: colours from the record, fading out; blend byte 0 =
            // normal alpha, else additive (the red ember smoke).
            3 | 8 => {
                let b = &r.bytes;
                let size = r.scale.x.max(1.0);
                effects::BurstSpec {
                    shape: effects::Shape::Puff,
                    texture: None,
                    quadrants: false,
                    count: if r.kind == 8 { 8 } else { 6 },
                    life: p[0].clamp(0.5, 5.0),
                    emit_over: (p[0] * 0.25).clamp(0.1, 0.8),
                    speed: 6.0,
                    spread_deg: 70.0,
                    up: 0.0,
                    gravity: 0.0,
                    rise: 4.0 + p[6].abs() * fps * 2.0,
                    size: [size * 1.2, size * 3.5],
                    colour: [srgb_rgba([b[1], b[2], b[3], b[5]], 0.85), srgb_rgba([b[7], b[8], b[9], b[6]], 0.85)],
                    additive: b[4] != 0,
                    spin: p[5].to_radians() * fps,
                    jitter: size * 0.4,
                }
            }
            // Debris chips from the "crs" sprite of MD00 slot 20 + byte 0.
            9 => {
                let slot = 20 + r.bytes[0] as usize;
                let Some(tex) = ofx.slots.get(&slot).and_then(|s| s.sprites.first().cloned().flatten()) else { continue };
                let life = if r.bytes[2] > 0 { r.bytes[2] as f32 / fps } else { 1.0 };
                effects::BurstSpec {
                    shape: effects::Shape::Chip,
                    texture: Some(assets.load(tex)),
                    quadrants: true,
                    count: ((p[0] * 10.0).round() as u32).clamp(4, 24),
                    life: life.clamp(0.4, 3.0),
                    emit_over: 0.0,
                    speed: p[8].max(0.5) * fps,
                    spread_deg: p[4].max(20.0),
                    up: p[5] * fps,
                    gravity: -p[6] * fps * fps,
                    rise: 0.0,
                    size: [(p[1].max(p[2]) * DEBRIS_SIZE_SCALE).clamp(1.5, 12.0); 2],
                    colour: [Vec4::ONE, Vec4::new(1.0, 1.0, 1.0, 0.0)],
                    additive: false,
                    spin: 8.0,
                    jitter: 2.0,
                }
            }
            // A model from MD00 slot byte 0: a fragment set flies apart, a
            // particle system (slot 16) throws spark streaks.
            10 => {
                let Some(slot) = ofx.slots.get(&(r.bytes[0] as usize)) else { continue };
                if !slot.timed && !slot.fragments.is_empty() {
                    let Some(file) = &slot.file else { continue };
                    let life = if p[0] > 0.0 { p[0] / fps } else { 1.5 }.clamp(0.5, 3.0);
                    let (scale, rot, trans) = world.to_scale_rotation_translation();
                    let root = Transform { translation: trans, rotation: rot, scale: scale * r.scale.x.max(0.1) };
                    let gravity_local = (rot.inverse() * Vec3::NEG_Y * FRAGMENT_GRAVITY) / root.scale.max_element().max(0.01);
                    commands.spawn((
                        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("fx/{file}.glb")))),
                        root,
                        effects::FragmentBurst { nodes: slot.fragments.clone(), life, speed: FRAGMENT_SPEED, seed, gravity_local, age: 0.0 },
                        MatchScene,
                    ));
                    played += 1;
                    continue;
                }
                let Some(tex) = slot.sprites.iter().flatten().next().cloned() else { continue };
                effects::BurstSpec {
                    shape: effects::Shape::Streak,
                    texture: Some(assets.load(tex)),
                    quadrants: false,
                    count: 14,
                    life: 0.6,
                    emit_over: 0.1,
                    speed: 70.0,
                    spread_deg: 80.0,
                    up: 10.0,
                    gravity: 120.0,
                    rise: 0.0,
                    size: [4.0, 1.5],
                    colour: [Vec4::ONE, Vec4::new(1.0, 1.0, 1.0, 0.0)],
                    additive: true,
                    spin: 0.0,
                    jitter: 3.0,
                }
            }
            // 11 = sound (sfx.rs), 16 = camera shake (not built), others unknown.
            _ => continue,
        };
        for e in effects::spawn_burst(commands, pm, mats, &spec, at, seed) {
            commands.entity(e).insert(MatchScene);
        }
        played += 1;
    }
    played
}

/// Watches `Combat::props`: a hit (more wear, still standing) plays the
/// type's trigger-1 records, a break (or a thrown object shattering)
/// trigger 2 and 3. `PD_FX_BREAK_AT=<frame>` (test only) plays the break
/// records of every object within 250 units of player 1 on that frame of
/// the match (counted like `--shot-frame`).
#[allow(clippy::too_many_arguments)]
fn object_effects(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cfg: Res<crate::Config>,
    setup: Option<Res<crate::frontend::state::MatchSetup>>,
    world: Res<World>,
    time: Res<Time>,
    pm: Option<Res<effects::ParticleMeshes>>,
    mut ofx: ResMut<ObjectFx>,
    mut mats: effects::PartMats,
    mut seen: Local<HashMap<u32, (crate::props::PropState, i32)>>,
    mut thrown_at: Local<HashMap<u32, Vec3>>,
    mut test_done: Local<bool>,
    mut frames: Local<u32>,
) {
    use crate::props::PropState;
    let Some(pm) = pm else { return };
    let stage = match setup.as_deref() {
        Some(s) if !cfg.legacy => s.stage.clone(),
        _ => cfg.stage.clone(),
    };
    if ofx.stage != stage {
        *ofx = ObjectFx::load(&cfg.cache, &stage);
        seen.clear();
        thrown_at.clear();
        *test_done = false;
        info!("object fx: {} types with records on {stage}", ofx.types.iter().filter(|t| !t.is_empty()).count());
    }
    let c = &world.0.combat;
    let log = std::env::var_os("PD_PROP_LOG").is_some();
    for (k, prop) in c.props.iter().enumerate() {
        let i = prop.index;
        if prop.state == PropState::Thrown {
            if let Some(p) = c.projectiles.iter().find(|p| p.prop == Some(k)) {
                thrown_at.insert(i, p.pos);
            }
        }
        // First sight (match start, `--props-state`): just remember it.
        let Some((was, wear)) = seen.insert(i, (prop.state, prop.wear)) else { continue };
        let Some(&(ty, mut obj)) = ofx.objects.get(&i) else { continue };
        let triggers: &[u8] = match (was, prop.state) {
            (a, b) if a == b && prop.wear > wear && prop.standing() => &[1],
            (PropState::Intact, PropState::Damaged) => &[1],
            (PropState::Intact | PropState::Damaged, PropState::Broken) => &[2, 3],
            (PropState::Thrown, PropState::Gone) => {
                // Shattered where its flight ended.
                if let Some(at) = thrown_at.get(&i) {
                    obj = Mat4::from_translation(*at - prop.centre()) * obj;
                }
                &[2, 3]
            }
            _ => continue,
        };
        let n = play_object(&mut commands, &assets, &pm, &mut mats, &ofx, ty, obj, triggers, i.wrapping_mul(7919).wrapping_add(time.elapsed_secs() as u32));
        if log {
            eprintln!("[objfx] object {i} {was:?} -> {:?}: {n} effects (triggers {triggers:?})", prop.state);
        }
    }
    // Test hook (render-only): break effects on everything near player 1.
    *frames += 1;
    if let Some(t) = std::env::var("PD_FX_BREAK_AT").ok().and_then(|v| v.parse::<u32>().ok()) {
        if !*test_done && *frames >= t {
            *test_done = true;
            let me = world.0.players[0].pos;
            let mut n = 0;
            // PD_FX_BREAK_TYPE=stNN:<name part>: that object type's break
            // effects (any stage's), just in front of player 1 and to the
            // side (in view of player 1's camera).
            if let Some((st, name)) = std::env::var("PD_FX_BREAK_TYPE").ok().as_deref().and_then(|s| s.split_once(':')) {
                let other = ObjectFx::load(&cfg.cache, st);
                let ahead = (world.0.players[1].pos - me).with_y(0.0).normalize_or(Vec3::X);
                let right = Vec3::Y.cross(ahead).normalize_or(Vec3::Z);
                let spot = me + ahead * 18.0 - right * 14.0;
                for (ty, _) in other.type_names.iter().enumerate().filter(|(_, n)| n.contains(name)) {
                    // Some types (floor sections) sit at the origin with their
                    // records at world positions: centre the records on the spot.
                    let recs = &other.types[ty];
                    let centre = recs.iter().map(|r| r.local.transform_point3(Vec3::ZERO)).sum::<Vec3>() / recs.len().max(1) as f32;
                    let lift = if centre.y.abs() > 40.0 { centre.y - 5.0 } else { 0.0 };
                    let obj = Mat4::from_translation(spot - Vec3::new(centre.x, lift, centre.z));
                    n += play_object(&mut commands, &assets, &pm, &mut mats, &other, ty, obj, &[2, 3], ty as u32);
                }
            } else {
                for prop in c.props.iter().filter(|p| p.breakable() && p.distance_to(me) < 250.0) {
                    if let Some(&(ty, obj)) = ofx.objects.get(&prop.index) {
                        n += play_object(&mut commands, &assets, &pm, &mut mats, &ofx, ty, obj, &[2, 3], prop.index);
                    }
                }
            }
            info!("PD_FX_BREAK_AT: played {n} break effects");
        }
    }
}

#[derive(Resource)]
pub struct CombatAssets {
    projectile: Handle<Mesh>,
    laser: Handle<StandardMaterial>,
    shot: Handle<StandardMaterial>,
    /// Flat disc (unit radius) marking where a Hole / Rain / mine will go off.
    disc: Handle<Mesh>,
    warn: Handle<StandardMaterial>,
    /// The environment crystal: a tall aqua gem ("looks similar to a capsule").
    crystal: Handle<Mesh>,
    crystal_mat: Handle<StandardMaterial>,
}

/// Guide: "Aura (White), Attack (Red), Defense (Blue), Erase (Purple), Status
/// (Green), Environmental (Aqua), and Special (Yellow)." Only used to tint
/// the materialise flare (`st00_cap02_birth2`) now -- the idle orb's own
/// colour comes from its own model's textures (docs/CAPSULE-VISUALS.md
/// "Program read 2026-09-28": "No colour tint -- each kind's colour comes
/// from its own textures"), matching the program's capsule colour table.
const CAPSULE_COLORS: [Color; 7] = [
    Color::srgb(0.95, 0.97, 1.0),
    Color::srgb(1.0, 0.3, 0.25),
    Color::srgb(0.3, 0.55, 1.0),
    Color::srgb(0.75, 0.35, 1.0),
    Color::srgb(0.35, 1.0, 0.4),
    Color::srgb(0.3, 1.0, 0.95),
    Color::srgb(1.0, 0.85, 0.3),
];
/// `CAPSULE_COLORS`' index (aura/attack/defence/erase/status/environment/
/// special) -> the `st00.alr` model slot for your own idle orb (`4 + s9`,
/// docs/CAPSULE-VISUALS.md's slot table). The game's own `s9` numbering
/// (0 aura, 1 attack, 2 defence, 3 erase, 4 status, 5 special, 6
/// environment) puts special/environment in the opposite order from our
/// `CAPSULE_COLORS`/`colour` index (5 environment, 6 special) -- established
/// when the colour table was matched to StrategyWiki; this table bridges the
/// two so the right model is picked.
const COLOUR_TO_ORB_SLOT: [usize; 7] = [4, 5, 6, 7, 8, 10, 9];
/// Model slot for another player's capsule, as it looks in your view
/// (docs/CAPSULE-VISUALS.md: 11 = their Aura Particle, 12 = any of their six
/// skill kinds -- one look for all six, not distinguished by colour).
const OPPONENT_AURA_SLOT: usize = 11;
const OPPONENT_OTHER_SLOT: usize = 12;
/// Ghost entities are keyed apart from the owner's in the live map.
const GHOST_KEY: u32 = 1 << 31;
/// Area marker discs are keyed apart from the area's own effect.
const DISC_KEY: u32 = 1 << 30;

/// Render layers: 0 = the shared world (every view); `1 + i` = things only
/// player `i`'s camera sees (their own capsules); `GHOST_LAYER_BASE + j` =
/// player `j`'s capsules as everyone else sees them -- one ghost entity per
/// capsule on a layer that all cameras but the owner's include
/// (`camera_layers`), instead of one ghost per viewer.
///
/// Numbered 40.. so they can never equal another system's layer
/// (`shadows.rs` 6-9, `aura_glow.rs` 12, 16.., 24..): at 5..8 they shared
/// numbers with the shadow lights' layers and seat 2's camera saw its own
/// capsules' ghosts (`layer_numbers_never_collide`).
pub(crate) const GHOST_LAYER_BASE: usize = 40;

/// The render layers camera `view` of a `players`-player match sees: the
/// shared world (0), its own things (`1 + view`) and every other player's
/// ghosts (`GHOST_LAYER_BASE + j`, `j != view`) -- never its own ghost layer,
/// so the owner's own capsules show only their full-colour model.
pub fn camera_layers(view: usize, players: usize) -> bevy::camera::visibility::RenderLayers {
    let mut layers = vec![0, 1 + view];
    layers.extend((0..players.min(MAX_PLAYERS)).filter(|&j| j != view).map(|j| GHOST_LAYER_BASE + j));
    bevy::camera::visibility::RenderLayers::from_layers(&layers)
}

/// The `OrbTint` an idle orb's `st00.alr` model slot should glow, for
/// `style_orb_materials`' fresnel rim. Own-view slots use `CAPSULE_COLORS`
/// via `COLOUR_TO_ORB_SLOT`'s inverse; the opponent-view slots (11 their Aura
/// Particle, 12 any of their six skill kinds) have no colour table entry in
/// the program (docs/CAPSULE-VISUALS.md: "one look for all six kinds") -- 11
/// reuses white (their texture is a neutral grey/white smoke, `tex28`), 12
/// a pink tint matching its own texture's look (`tex29`, "dark red / pink
/// bands") since nothing in the doc gives it a real colour-table entry.
fn slot_tint(slot: usize) -> Color {
    match COLOUR_TO_ORB_SLOT.iter().position(|&s| s == slot) {
        Some(colour) => CAPSULE_COLORS[colour],
        None if slot == OPPONENT_AURA_SLOT => CAPSULE_COLORS[0],
        None => Color::srgb(0.85, 0.4, 0.55),
    }
}

/// Which (render layer, idle-orb model slot) each of a capsule's two
/// presentation entities gets: index 0 = the owner's own full-colour orb
/// (`COLOUR_TO_ORB_SLOT`) on the owner's own layer (`1 + owner`), index 1 =
/// the dedicated "opponent's" look every other viewer sees (slot 11 Aura
/// Particle, 12 any of the six skill kinds -- one look for all six,
/// docs/CAPSULE-VISUALS.md "Program read" `FUN_14021b840`) on the owner's
/// ghost layer (`GHOST_LAYER_BASE + owner`), which every camera but the
/// owner's includes. One ghost entity serves all of them whatever the player
/// count. Capture ownership is NOT decided here: `combat.rs` owns who can
/// pick what up (`capsule_in_reach`); this is presentation only. A pure
/// mapping (no Bevy types) so tests can pin it for every owner.
fn orb_presentations(owner: usize, colour: usize) -> [(usize, usize); 2] {
    let ghost_slot = if colour == 0 { OPPONENT_AURA_SLOT } else { OPPONENT_OTHER_SLOT };
    [(1 + owner, COLOUR_TO_ORB_SLOT[colour]), (GHOST_LAYER_BASE + owner, ghost_slot)]
}

fn glow(mats: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial { base_color: c, unlit: true, ..default() })
}

// ---------------------------------------------------------------------------
// Capsule and Aura Particle orbs: the original's own models from
// `Map/st00.alr`/`st00.dat` (`pd_convert capsules`, docs/CAPSULE-VISUALS.md
// "Program read 2026-09-28" -> "Build recipe"), replacing the placeholder
// sphere. Render-only: reads `Combat::capsules` and their appearance/
// disappearance between frames, never writes sim state.

/// A point-sprite draw's points (position + colour), read back from its
/// `.points.json` sidecar (docs/CAPSULE-VISUALS.md "Sparkles, pickup and
/// erase bursts": vertex format 0x18, no UV, dropped by the normal mesh
/// exporter).
fn load_points(cache: &std::path::Path, file: &Option<String>) -> Vec<(Vec3, Color)> {
    let Some(file) = file else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(cache.join("fx/capsules").join(file)) else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let pos = p["pos"].as_array()?;
            let c = p["color"].as_array()?;
            let f = |a: &[serde_json::Value], i: usize| a[i].as_f64().unwrap_or(0.0) as f32;
            Some((Vec3::new(f(pos, 0), f(pos, 1), f(pos, 2)), Color::srgba_u8(f(c, 0) as u8, f(c, 1) as u8, f(c, 2) as u8, f(c, 3) as u8)))
        })
        .collect()
}

/// One idle-orb draw, ready to spawn: its behaviour role (from
/// `orb_material::kind_spec`) and the exported mesh/texture handles that
/// role's material needs (`pd_convert capsules`' new dedicated exporter,
/// via `GltfAssetLabel::Primitive`/`Texture` -- the resolved indices come
/// from `capsules.json`'s own `draws[].base`/`secondary`, not this doc's
/// reverse-engineered numbers).
struct OrbDraw {
    role: orb_material::OrbDrawRole,
    mesh: Handle<Mesh>,
    base_tex: Option<Handle<Image>>,
    secondary_tex: Option<Handle<Image>>,
    /// The material's 3rd/4th texture slots (`alr::MaterialEntry::
    /// texture_idx2`/`texture_idx3`) -- real extra layers on Status/
    /// Environment's own 4-texture unlit materials, not padding.
    tertiary_tex: Option<Handle<Image>>,
    quaternary_tex: Option<Handle<Image>>,
}

#[derive(Resource, Default)]
pub struct CapsuleOrbs {
    /// Slot 1's flare scene only -- still a plain Bevy glTF scene +
    /// `StandardMaterial`, tinted at runtime by `tint_flares` (unchanged
    /// from before this pass: the flare is one flat quad, not a
    /// multi-layer shaded orb, so it doesn't need the custom materials
    /// below).
    flare: HashMap<usize, Handle<WorldAsset>>,
    /// Idle-orb draws, slots 4-12 (docs/CAPSULE-VISUALS.md "Draw recipe
    /// (decoded 2026-09-28)"): one entry per file draw, core first then its
    /// rim (cup/disc) layers, in file order -- see `spawn_orb`.
    orb_draws: HashMap<usize, Vec<OrbDraw>>,
    /// Slot 0's materialise sparkles and slot 2's pickup burst, reduced to
    /// (average colour, spatial spread) -- not instanced point-for-point
    /// (slot 0 alone has 1,710 of them: too many one-shot entities for a
    /// per-capsule burst), but real numbers from the actual point-sprite
    /// data (docs/CAPSULE-VISUALS.md's slot table; slot 3's erase burst
    /// isn't wired in -- `Combat` doesn't expose whether a capsule left by
    /// capture or by an Erase skill, only that it's gone, see
    /// `sync_combat_visuals`), not invented constants.
    materialise: PointStats,
    /// Slot 0's own recorded points (position + colour), local to the orb's
    /// origin -- kept in full (not just reduced to `materialise`'s average)
    /// so `materialise_burst` can instance the real gather shape instead of
    /// a procedural spread (VFX-AUDIT/CAPSULE-VISUALS "denser materialise sparkles").
    materialise_points: Vec<(Vec3, Color)>,
    /// The sparkle/spark texture (tex4), for the particle-burst approximation of
    /// slot 0 in `materialise_burst` (slots 2/3 are drawn for real by
    /// `capsule_birth.rs`).
    spark_tex: Option<Handle<Image>>,
    /// Every idle-orb texture handle loaded, so `force_orb_texture_repeat`
    /// can force REPEAT/wrap addressing on each one once it loads (docs
    /// "Draw recipe" step 2 -- belt-and-braces alongside the exporter's own
    /// glTF sampler, since a scrolled UV leaving 0..1 on a clamped sampler
    /// is exactly what produced round 2's "hard horizontal line").
    orb_textures: Vec<Handle<Image>>,
}

/// A point-sprite draw's average colour and spatial spread (root-mean-square
/// distance from centre), used in place of instancing every point.
#[derive(Clone, Copy, Default)]
struct PointStats {
    colour: Vec4,
    spread: f32,
}

impl PointStats {
    fn from(points: &[(Vec3, Color)]) -> Self {
        if points.is_empty() {
            return Self::default();
        }
        let centre = points.iter().map(|(p, _)| *p).sum::<Vec3>() / points.len() as f32;
        let spread = (points.iter().map(|(p, _)| p.distance_squared(centre)).sum::<f32>() / points.len() as f32).sqrt();
        let colour = points.iter().map(|(_, c)| { let l = c.to_linear(); Vec4::new(l.red, l.green, l.blue, l.alpha) }).sum::<Vec4>() / points.len() as f32;
        PointStats { colour, spread: spread.max(0.5) }
    }
}

impl CapsuleOrbs {
    fn load(cache: &std::path::Path, assets: &AssetServer) -> Self {
        let dir = cache.join("fx/capsules");
        let Ok(text) = std::fs::read_to_string(dir.join("capsules.json")) else {
            warn!("no cache/fx/capsules/capsules.json: run `pd_convert capsules` (idle orbs stay a placeholder sphere)");
            return Self::default();
        };
        let manifest: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let mut flare = HashMap::new();
        let mut orb_draws = HashMap::new();
        let mut orb_textures = Vec::new();
        let mut materialise_points = Vec::new();
        for s in manifest["slots"].as_array().into_iter().flatten() {
            let Some(slot) = s["slot"].as_u64().map(|n| n as usize) else { continue };
            let points = s["points"].as_str().map(String::from);
            let Some(glb) = s["glb"].as_str() else { continue };
            let path = format!("fx/capsules/{glb}");
            if slot == 1 {
                // The flare: still one plain scene (a single flat quad,
                // tinted at runtime -- see `CapsuleOrbs::flare`'s doc).
                flare.insert(slot, assets.load(GltfAssetLabel::Scene(0).from_asset(path)));
            } else if (4..=12).contains(&slot) {
                let roles = orb_material::kind_spec(slot);
                let mut draws = Vec::new();
                for (i, draw_json) in s["draws"].as_array().into_iter().flatten().enumerate() {
                    let Some(role) = roles.get(i).copied() else { continue };
                    let mesh = assets.load(GltfAssetLabel::Primitive { mesh: i, primitive: 0 }.from_asset(path.clone()));
                    let mut tex_handle = |key: &str| -> Option<Handle<Image>> {
                        let idx = draw_json[key].as_u64()? as usize;
                        let h = assets.load(GltfAssetLabel::Texture(idx).from_asset(path.clone()));
                        orb_textures.push(h.clone());
                        Some(h)
                    };
                    let base_tex = tex_handle("base");
                    let secondary_tex = tex_handle("secondary");
                    let tertiary_tex = tex_handle("tertiary");
                    let quaternary_tex = tex_handle("quaternary");
                    draws.push(OrbDraw { role, mesh, base_tex, secondary_tex, tertiary_tex, quaternary_tex });
                }
                orb_draws.insert(slot, draws);
            }
            match slot {
                0 => materialise_points = load_points(cache, &points),
                _ => {}
            }
        }
        let tex = |i: &str| manifest["textures"][i].as_str().map(|f| assets.load(format!("fx/capsules/{f}")));
        CapsuleOrbs {
            flare,
            orb_draws,
            materialise: PointStats::from(&materialise_points),
            materialise_points,
            spark_tex: tex("4"),
            orb_textures,
        }
    }
}

/// Spawns one idle orb (slots 4-12): a root entity carrying `transform`,
/// `OrbRoot`, `OrbTint(tint)` (read by `orb_motes`; the core/rim materials
/// below don't use it -- docs/CAPSULE-VISUALS.md "Program read": "No colour
/// tint -- each kind's colour comes from its own textures") and every
/// component in `extra`, with one child per draw
/// (`orb_material::kind_spec`'s per-draw recipe: core first, then its rim
/// layers). Returns `None` if the slot has no cached data (older cache /
/// `pd_convert capsules` not run).
///
/// `layer`: the render layer this orb is on (`view_layer` / `ghost_layer`). Bevy 0.19 culls each
/// entity by its OWN `RenderLayers` (`bevy_camera`'s culling query reads each
/// entity's own mask, no walk up to the parent -- and a spawned `ChildOf`
/// chain inherits nothing, see `frontend/charselect.rs`'s `SLOT_GAP` comment), so the layer must go on every draw child, not just
/// the root: with it only on the root, both split-screen cameras render both
/// the own-model AND the opponent-model of every capsule, superimposed --
/// each player's own (capturable) capsules read dark/smoky and the other
/// player's read bright, i.e. exactly backwards. `None` = visible in every
/// view (the `PD_CAPSULE_PREVIEW=1` line-up only).
fn spawn_orb(commands: &mut Commands, orbs: &CapsuleOrbs, orb_mats: &mut orb_material::OrbMats, slot: usize, transform: Transform, tint: Color, layer: Option<usize>, extra: impl Bundle) -> Option<Entity> {
    let draws = orbs.orb_draws.get(&slot)?;
    let root = commands.spawn((transform, OrbRoot, OrbTint(tint), extra)).id();
    if let Some(l) = layer {
        commands.entity(root).insert(bevy::camera::visibility::RenderLayers::layer(l));
    }
    for d in draws {
        let Some(base) = d.base_tex.clone() else { continue };
        let child = match d.role {
            orb_material::OrbDrawRole::CoreLit { billboard, spec, has_bump, layers } => {
                let has_bump = has_bump && d.secondary_tex.is_some();
                let bump = d.secondary_tex.clone().unwrap_or_else(|| base.clone());
                let mat = orb_mats.lit.add(orb_material::sphere_lit_material(billboard, spec, has_bump, layers, base, bump));
                commands.spawn((Mesh3d(d.mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, ChildOf(root))).id()
            }
            orb_material::OrbDrawRole::CoreUnlit { layers, formula } => {
                let mat = orb_mats.unlit.add(orb_material::sphere_unlit_material(layers, formula, base, d.secondary_tex.clone(), d.tertiary_tex.clone(), d.quaternary_tex.clone()));
                commands.spawn((Mesh3d(d.mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, ChildOf(root))).id()
            }
            orb_material::OrbDrawRole::Rim { layers, blend } => {
                let mat = orb_mats.rim.add(orb_material::rim_material(layers, blend, base));
                commands.spawn((Mesh3d(d.mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, ChildOf(root))).id()
            }
        };
        if let Some(l) = layer {
            commands.entity(child).insert(bevy::camera::visibility::RenderLayers::layer(l));
        }
    }
    Some(root)
}

/// Forces REPEAT/wrap addressing on every idle-orb texture once it loads
/// (docs/CAPSULE-VISUALS.md "Draw recipe" step 2). Idempotent and cheap
/// (skips anything already marked or not yet loaded), so it just runs every
/// frame rather than needing its own completion tracking beyond the set.
fn force_orb_texture_repeat(orbs: Option<Res<CapsuleOrbs>>, mut images: ResMut<Assets<Image>>, mut done: Local<std::collections::HashSet<AssetId<Image>>>) {
    let Some(orbs) = orbs else { return };
    for h in &orbs.orb_textures {
        if done.contains(&h.id()) {
            continue;
        }
        let Some(mut img) = images.get_mut(h) else { continue };
        // `..default()` alone leaves mag/min/mipmap filtering at
        // `ImageSamplerDescriptor`'s own default (Nearest) -- overriding the
        // exporter's own glTF sampler (`gltf.rs::Materials::write_into`,
        // already Linear/Linear) with a *blockier* one than before this
        // system existed. Linear here restores it (this system's whole job
        // is forcing wrap addressing; it shouldn't also downgrade filtering).
        img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
            address_mode_u: bevy::image::ImageAddressMode::Repeat,
            address_mode_v: bevy::image::ImageAddressMode::Repeat,
            mag_filter: bevy::image::ImageFilterMode::Linear,
            min_filter: bevy::image::ImageFilterMode::Linear,
            mipmap_filter: bevy::image::ImageFilterMode::Linear,
            ..default()
        });
        done.insert(h.id());
    }
}

/// A spawned idle-orb entity: which views see it (a bit per camera, for the
/// camera-near fade: it fades when any of them is within 10 u) and its fade level (1 = fully visible, faded toward ~0 within 10 u
/// of that view's camera -- docs/CAPSULE-VISUALS.md `FUN_140196c40`, ±16 per
/// tick at 30 fps; approximated here as scaling the whole orb down rather
/// than a true material alpha, since the model's three layers would each
/// need their own runtime-cloned material to fade independently).
#[derive(Component)]
struct OrbFade {
    viewers: u8,
    alpha: f32,
    /// The capsule's spot and look (own / ghost): the program's per-viewer fade byte
    /// belongs to the slot's render object, which lives across respawns (`OrbAlphaMemory`).
    key: OrbSpot,
    /// A respawn picks the byte up where the last take left it instead of at 255.
    resume: bool,
}

/// One capsule spot's presentation: (owner, spot x and z in tenths of a unit, ghost look).
type OrbSpot = (usize, i32, i32, bool);

fn orb_spot(owner: usize, pos: Vec3, ghost: bool) -> OrbSpot {
    (owner, (pos.x * 10.0).round() as i32, (pos.z * 10.0).round() as i32, ghost)
}

/// The last fade level of each capsule spot's orb. READ: the program's near-camera fade
/// (`FUN_140196c40`, +/-16 per tick on a per-viewer byte at the render object +0x78) is
/// set to 255 only when the round's slots are built (`FUN_14014ee70` -> `FUN_14021b3d0`,
/// its only callers); a take (`FUN_14014fd10`) and the respawn never touch it, and the
/// byte only moves while the orb is drawn (states 1-2). So an orb that returns after a
/// take made from close by (the byte had faded toward 0 inside 10 u) climbs back at 16
/// per tick (about half a second), while one that returns after an erase from afar is
/// at full strength at once. Replaces the flat 0.05 start every respawn used to get.
#[derive(Resource, Default)]
pub struct OrbAlphaMemory(HashMap<OrbSpot, f32>);

/// Every spawned idle-orb / flare instance (real capsules and the
/// `PD_CAPSULE_PREVIEW` debug line-up alike), so `style_orb_materials` finds
/// both without needing the per-viewer `OrbFade` the preview doesn't have.
#[derive(Component)]
struct OrbRoot;

/// The capsule colour (`CAPSULE_COLORS`) this orb's core rim should glow --
/// set at spawn time (both real capsules and `PD_CAPSULE_PREVIEW`), read by
/// `style_orb_materials` when it builds the core's `OrbMaterial`. Defaults to
/// white (`CAPSULE_COLORS[0]`, Aura Particle) if an orb root has none.
#[derive(Component, Clone, Copy)]
struct OrbTint(Color);

/// Despawns its entity once `secs` have elapsed (the materialise flare and
/// the point-sprite bursts are all one-shot).
#[derive(Component)]
struct DespawnAfter(f32);

fn tick_despawn_after(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut DespawnAfter)>) {
    for (e, mut d) in &mut q {
        d.0 -= time.delta_secs();
        if d.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Camera-near fade (docs/CAPSULE-VISUALS.md `FUN_140196c40`): the program
/// steps an 0-255 alpha byte by ±16 per tick (30 fps) while within/outside
/// 10 u (squared distance 100) of that orb's viewer -- about 1.88 (of our
/// 0-1 alpha) per second, a ~0.5 s fade.
const ORB_FADE_PER_SEC: f32 = 16.0 * 30.0 / 255.0;

fn fade_capsule_orbs(time: Res<Time>, cams: Query<(&crate::PlayerCamera, &GlobalTransform)>, mut q: Query<(&mut OrbFade, &mut Transform)>, mut memory: ResMut<OrbAlphaMemory>) {
    let dt = time.delta_secs();
    for (mut fade, mut t) in &mut q {
        if fade.resume {
            fade.resume = false;
            fade.alpha = memory.0.get(&fade.key).copied().unwrap_or(1.0);
        }
        let near = cams.iter().any(|(c, gt)| fade.viewers >> c.0 & 1 == 1 && gt.translation().distance_squared(t.translation) < 100.0);
        fade.alpha = (fade.alpha + if near { -ORB_FADE_PER_SEC } else { ORB_FADE_PER_SEC } * dt).clamp(0.05, 1.0);
        t.scale = Vec3::splat(fade.alpha);
        memory.0.insert(fade.key, fade.alpha);
    }
}

/// Slot 0's materialise sparkles (approximated: the real 429-frame animation
/// gathers sparkles over the whole 14 s respawn wait, which `pd_game` can't
/// see -- `Combat::capsules` only lists a capsule once it's already present,
/// see `sync_combat_visuals` -- so this plays a ~1.5 s gather/pop right as
/// the capsule appears, matching the visible tail of Laurence's footage
/// `mat_close.png` rather than the full wait).
fn materialise_burst(commands: &mut Commands, pm: &effects::ParticleMeshes, mats: &mut effects::PartMats, orbs: &CapsuleOrbs, at: Vec3, seed: u32) {
    if orbs.materialise_points.is_empty() {
        // No `.points.json` sidecar (older cache): fall back to the old
        // averaged-stats spread rather than showing nothing.
        let spec = effects::BurstSpec {
            shape: effects::Shape::Puff,
            texture: orbs.spark_tex.clone(),
            quadrants: false,
            count: 24,
            life: 1.4,
            emit_over: 1.0,
            speed: 2.5,
            spread_deg: 180.0,
            up: 0.0,
            gravity: 0.0,
            rise: 0.0,
            size: [1.4, 0.2],
            colour: [orbs.materialise.colour, orbs.materialise.colour.with_w(0.0)],
            additive: true,
            spin: 0.0,
            jitter: orbs.materialise.spread,
        };
        for e in effects::spawn_burst(commands, pm, mats, &spec, at, seed) {
            commands.entity(e).insert(MatchScene);
        }
        return;
    }
    // The real recorded gather (~1,700 points for the attack kind): instance
    // a dense, evenly-sampled subset of the actual data instead of a
    // procedural spread (CAPSULE-VISUALS.md "Sparkles, pickup and erase
    // bursts" -- a first pass's `PointStats`-derived spread read as "a thin
    // scatter of sparkle points rather than the footage's dense gold cloud").
    for e in effects::spawn_point_burst(commands, pm, mats, &orbs.materialise_points, at, 1.4, 0.5, orbs.spark_tex.clone(), seed, 350) {
        commands.entity(e).insert(MatchScene);
    }
}

/// Slot 1's flare (`st00_cap02_birth2`): a 20x20 u glow quad, tinted with
/// `CAPSULE_COLORS`, played for its ~2 s (59-frame) animation as the capsule
/// appears.
fn flare_burst(commands: &mut Commands, orbs: &CapsuleOrbs, at: Vec3, colour: usize) {
    let Some(scene) = orbs.flare.get(&1).cloned() else { return };
    // No `OrbRoot`: `tint_flares` alone handles this scene's one draw
    // (`style_orb_materials`'s sphere/shell-by-draw-index split doesn't apply
    // to a single flat quad, and running both would fight over the material
    // each frame).
    commands.spawn((WorldAssetRoot(scene), Transform::from_translation(at), DespawnAfter(59.0 / 30.0), MatchScene, FlareTint(CAPSULE_COLORS[colour])));
}

/// Tags a freshly spawned flare scene so its material can be tinted once its
/// mesh has loaded in (the scene loads asynchronously; `tint_flares` retries
/// until the material shows up, the same pattern `effects::attach_pieces` uses).
#[derive(Component)]
struct FlareTint(Color);

fn tint_flares(mut commands: Commands, roots: Query<(Entity, &FlareTint), Without<FlareTinted>>, children: Query<&Children>, mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for (root, tint) in &roots {
        let Some(mat_h) = children.iter_descendants(root).find_map(|c| mesh_mats.get(c).ok()) else { continue };
        let Some(mut m) = mats.get_mut(&mat_h.0) else { continue };
        m.base_color = tint.0;
        m.unlit = true;
        m.alpha_mode = AlphaMode::Add;
        m.double_sided = true;
        m.cull_mode = None;
        commands.entity(root).insert(FlareTinted);
    }
}

#[derive(Component)]
struct FlareTinted;

/// Idle orb rendering: `spawn_orb` (see `CapsuleOrbs`'s doc comment) builds
/// every draw's entity+material directly from the loaded mesh/texture
/// handles the moment a capsule appears, so there is no more deferred
/// "style once loaded" pass here (rounds 1/2 needed one because they waited
/// on a whole glTF *scene* to finish loading before reading its own node
/// names/materials back out; loading individual primitives/textures by
/// index up front avoids that). See `orb_material.rs`'s module doc and
/// docs/CAPSULE-VISUALS.md's "Draw recipe (decoded 2026-09-28)" for what
/// replaced rounds 1/2's billboarded-quad approximation and why.
///
/// Small drifting motes around every idle orb (Laurence, footage
/// `red_idle.png`: "a few tiny sparks drifting around it", and again after
/// the round-1 sheet: "missing the small glowing motes ... coloured like the
/// capsule"). Checked first, not assumed: `cache/fx/capsules/capsules.json`
/// records `points: null` for every idle-orb slot (4-12) -- the idle state
/// has no point-sprite data of its own in the exported archive (only slots
/// 0/2/3, the materialise/pickup/erase bursts, do) -- so this is a small
/// procedural emitter matched to the footage by eye, not recovered per-orb
/// point data; the spawn rate/spread/life below are marked unconfirmed.
fn orb_motes(
    mut commands: Commands,
    time: Res<Time>,
    pm: Option<Res<effects::ParticleMeshes>>,
    mut mats: effects::PartMats,
    q: Query<(Entity, &GlobalTransform, &OrbTint), With<OrbRoot>>,
    mut timers: Local<HashMap<Entity, f32>>,
) {
    // Off by default (`docs/CAPSULE-VISUALS.md` "Draw recipe": the idle-orb
    // slots' own exported data carries no point-sprite/mote draws at all --
    // "the recipe found no motes in the idle orb data" -- this is still an
    // unrecovered, matched-by-eye procedural stand-in, not file data).
    // `PD_ORB_MOTES=1` turns it back on for A/B comparison against footage.
    if std::env::var_os("PD_ORB_MOTES").is_none() {
        return;
    }
    let Some(pm) = pm else { return };
    timers.retain(|e, _| q.contains(*e));
    for (e, gt, tint) in &q {
        let t = timers.entry(e).or_insert(0.0);
        *t -= time.delta_secs();
        if *t > 0.0 {
            continue;
        }
        *t = 0.25;
        let c = tint.0.to_linear();
        let colour = Vec4::new(c.red, c.green, c.blue, 1.0);
        let spec = effects::BurstSpec {
            shape: effects::Shape::Puff,
            texture: None,
            quadrants: false,
            count: 1,
            life: 1.1,
            emit_over: 0.0,
            speed: 1.1,
            spread_deg: 180.0,
            up: 0.5,
            gravity: 0.0,
            rise: 0.4,
            size: [0.3, 0.04],
            colour: [colour, colour.with_w(0.0)],
            additive: true,
            spin: 0.0,
            jitter: 5.0,
        };
        let seed = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (e, time.elapsed_secs() as u32).hash(&mut h);
            h.finish() as u32
        };
        // Spawned below the orb, not at its centre: a mote starting dead
        // centre renders hidden behind the opaque core billboard (a flat,
        // camera-facing plane -- offsetting in world X/Z instead of down
        // moves a mote toward/away from the camera as much as sideways,
        // depending on the viewing angle, which just as often put it
        // *behind* that plane's depth as beside it). Matches the footage
        // (Laurence: "around and below it") without the depth ambiguity.
        let side = -3.0 + (seed % 61) as f32 * 0.1;
        let down = 2.0 + (seed % 40) as f32 * 0.1;
        let fwd = -1.5 + (seed / 61 % 31) as f32 * 0.1;
        let at = gt.translation() + Vec3::new(side, -down, fwd);
        for spawned in effects::spawn_burst(&mut commands, &pm, &mut mats, &spec, at, seed) {
            commands.entity(spawned).insert(MatchScene);
        }
    }
}

/// A camera the capsule preview owns outright (`spawn_preview_camera`),
/// framed directly on the orb(s) being calibrated. The normal gameplay
/// cameras (`main.rs::follow_cameras`) always point at the locked-on
/// opponent (`sim.rs`: `locked: true` by default, and nothing in `--demo`
/// untoggles it) -- fine for a battle screenshot, useless for "one orb, full
/// frame, unobstructed" (docs/CAPSULE-VISUALS.md's brief): the orb would sit
/// wherever the fight happens to be looking, not centred or any particular
/// size. `isolate_preview_cameras` deactivates every other camera (3D and
/// the HUD's 2D one) so only this one renders.
#[derive(Component)]
struct PreviewCamera;

/// `std::env::var(name)` parsed as `f32`, or `default` if unset/unparseable
/// (mirrors `main.rs`'s private helper of the same name -- calibration-only
/// overrides, not worth sharing across the module boundary for).
fn env_f32(name: &'static str, default: f32) -> f32 {
    effects::env_once(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Points a fresh camera at `at` from `eye`, active in place of the
/// gameplay/HUD cameras for the rest of this run (`isolate_preview_cameras`).
/// Both are world positions the caller works out itself -- ground level
/// isn't clear of scenery/the player's own body in every direction (an
/// early version placed the eye a fixed distance directly behind the orb,
/// along the player's own facing, and ended up looking through the
/// player's own legs), so callers here float both well above head height,
/// over open air, instead of guessing a ground-level offset that happens to
/// be clear.
fn spawn_preview_camera(commands: &mut Commands, eye: Vec3, at: Vec3) {
    // Sees every layer: live and preview orbs now carry their viewer's own
    // layer on every draw (`spawn_orb`'s `view`), so a layer-0-only camera
    // would render an empty frame.
    commands.spawn((Camera3d::default(), Camera { order: 0, ..default() }, bevy::camera::visibility::RenderLayers::from_layers(&[0, 1, 2]), Transform::from_translation(eye).looking_at(at, Vec3::Y), PreviewCamera));
}

/// Runs every frame a preview camera exists: keeps it the only thing
/// rendering, regardless of what order the gameplay/HUD cameras were spawned
/// in relative to it.
fn isolate_preview_cameras(previews: Query<(), With<PreviewCamera>>, mut others: Query<&mut Camera, Without<PreviewCamera>>) {
    if previews.is_empty() {
        return;
    }
    for mut c in &mut others {
        c.is_active = false;
    }
}

/// `PD_CAPSULE_PREVIEW=1` (screenshot/proof tool only): lines up every idle-
/// orb kind (own view slots 4-12) 15 u in front of player 1's start, once,
/// independent of `Combat::capsules` -- purely additive rendering, reads no
/// sim state and writes none, for A/B-checking the model/material work
/// against Laurence's footage without waiting on a real capsule to spawn.
/// `=ghostcheck` instead lines up what each view shows for one capsule (view
/// 0: slot 5 own attack vs slot 12 foe's; view 1: slot 4 own aura vs slot 11
/// foe's) through the same `spawn_orb` + per-view layer path the live match
/// uses.
fn capsule_preview(
    mut commands: Commands,
    orbs: Option<Res<CapsuleOrbs>>,
    pm: Option<Res<effects::ParticleMeshes>>,
    mut mats: effects::PartMats,
    mut orb_mats: orb_material::OrbMats,
    world: Res<World>,
    mut done: Local<bool>,
) {
    let Some(mode) = std::env::var("PD_CAPSULE_PREVIEW").ok() else { return };
    if *done {
        return;
    }
    let Some(orbs) = orbs else { return };
    *done = true;
    let facing = Quat::from_rotation_y(world.0.players[0].yaw) * Vec3::Z;
    let right = Vec3::Y.cross(facing).normalize_or(Vec3::X);
    if mode == "mat" {
        // PD_CAPSULE_PREVIEW=mat: one attack capsule appearing close up (the
        // flare + gathered sparkles over its idle orb), for a materialise
        // sequence proof without waiting on `Combat`'s own 14 s respawn wait.
        let Some(pm) = pm else { return };
        let pos = world.0.players[0].pos + facing * 10.0 + Vec3::Y * 5.0;
        spawn_orb(&mut commands, &orbs, &mut orb_mats, 5, Transform::from_translation(pos), slot_tint(5), Some(1), MatchScene);
        materialise_burst(&mut commands, &pm, &mut mats, &orbs, pos, 1);
        flare_burst(&mut commands, &orbs, pos, 1);
        return;
    }
    if mode == "pickup" || mode == "erase" {
        // PD_CAPSULE_PREVIEW=pickup|erase: slot 2 / slot 3's burst alone, close up
        // (docs/CAPSULE-VISUALS.md "Pickup and erase bursts"), for a collect-moment
        // proof without waiting for a real capture.
        let pos = world.0.players[0].pos + facing * 10.0 + Vec3::Y * capsule_birth::ORB_LIFT;
        commands.spawn(capsule_birth::capsule_burst(if mode == "erase" { 3 } else { 2 }, pos, world.0.tick, 1));
        return;
    }
    // PD_CAPSULE_PREVIEW=close_attack|close_aura: that one idle orb alone,
    // close up and steady (no burst), for a plain look-at-it screenshot.
    if let Some(slot) = match mode.as_str() {
        "close_aura" => Some(4),
        "close_attack" => Some(5),
        "close_defence" => Some(6),
        "close_erase" => Some(7),
        "close_status" => Some(8),
        "close_special" => Some(9),
        "close_environment" => Some(10),
        "close_opp_aura" => Some(11),
        "close_opp_other" => Some(12),
        _ => None,
    } {
        // A fixed point well above player 1's start (world position only --
        // never read again after this frame, so the demo AI wandering off
        // doesn't matter), floated up on both the orb and the camera so
        // neither ends up looking through the player's own body or the
        // plaza's low walls/railings (an early version put the camera at
        // ground level directly behind the orb and looked straight through
        // the player's legs) -- open air above head height is clear on every
        // stage this runs on. `PD_ORB_CAM_DIST`/`_HEIGHT` (env, calibration
        // only) let a sheet nudge framing without a rebuild; defaults were
        // picked by eye against the sphere's own ~4.5 u radius / ~8 u halo
        // (docs/CAPSULE-VISUALS.md).
        let float = env_f32("PD_ORB_HEIGHT", 14.0);
        let pos = world.0.players[0].pos + facing * 20.0 + Vec3::Y * float;
        // `PD_ORB_YAW` (env, calibration only, degrees): with the recipe's
        // real sphere geometry and normal mapping this is no longer working
        // around a broken UV hemisphere (that was a round-1/2 issue); kept
        // as a framing knob so a calibration shot can turn the orb's best
        // side to camera without a rebuild.
        let yaw = Quat::from_rotation_y(env_f32("PD_ORB_YAW", 150.0).to_radians());
        spawn_orb(&mut commands, &orbs, &mut orb_mats, slot, Transform::from_translation(pos).with_rotation(yaw), slot_tint(slot), Some(1), MatchScene);
        let dist = env_f32("PD_ORB_CAM_DIST", 15.0);
        let height = env_f32("PD_ORB_CAM_HEIGHT", 2.0);
        let eye = world.0.players[0].pos + facing * (20.0 - dist) + Vec3::Y * (float + height);
        spawn_preview_camera(&mut commands, eye, pos);
        return;
    }
    // PD_CAPSULE_PREVIEW=1 (any other value except "ghostcheck", below):
    // every idle-orb kind (own view slots 4-12) lined up in front of player
    // 1's start, its own pulled-back camera so the whole row is in frame
    // regardless of the fight's own lock-on camera.
    if mode != "ghostcheck" {
        let float = env_f32("PD_ORB_HEIGHT", 14.0);
        let base = world.0.players[0].pos + facing * 32.0 + Vec3::Y * float;
        for (i, &slot) in [4usize, 5, 6, 7, 8, 9, 10, 11, 12].iter().enumerate() {
            let pos = base + right * (i as f32 - 4.0) * 16.0;
            spawn_orb(&mut commands, &orbs, &mut orb_mats, slot, Transform::from_translation(pos), slot_tint(slot), None, MatchScene);
        }
        let dist = env_f32("PD_ORB_CAM_DIST", 46.0);
        let height = env_f32("PD_ORB_CAM_HEIGHT", 12.0);
        let eye = world.0.players[0].pos + facing * (32.0 - dist) + Vec3::Y * (float + height);
        spawn_preview_camera(&mut commands, eye, base);
        return;
    }
    // PD_CAPSULE_PREVIEW=ghostcheck: what each view shows for one capsule,
    // side by side through the real `spawn_orb` + per-view layer path (not a
    // copy of it): view 0 sees an attack capsule as slot 5 when it is P1's
    // own and slot 12 when it is P2's; view 1 sees an Aura Particle as slot
    // 4 when it is P2's own and slot 11 when it is P1's. The preview camera
    // sees all layers, so all four render in one frame for direct comparison.
    let float = env_f32("PD_ORB_HEIGHT", 14.0);
    let base = world.0.players[0].pos + facing * 32.0 + Vec3::Y * float;
    for (i, &(slot, view)) in [(5usize, 0), (12, 0), (4, 1), (11, 1)].iter().enumerate() {
        let pos = base + right * (i as f32 - 1.5) * 16.0;
        spawn_orb(&mut commands, &orbs, &mut orb_mats, slot, Transform::from_translation(pos), slot_tint(slot), Some(1 + view), MatchScene);
    }
    let dist = env_f32("PD_ORB_CAM_DIST", 46.0);
    let height = env_f32("PD_ORB_CAM_HEIGHT", 12.0);
    let eye = world.0.players[0].pos + facing * (32.0 - dist) + Vec3::Y * (float + height);
    spawn_preview_camera(&mut commands, eye, base);
}

pub fn spawn_fx(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(CapsuleOrbs::load(&cfg.cache, &assets));
    commands.insert_resource(CombatAssets {
        projectile: meshes.add(Sphere::new(2.5)),
        laser: glow(&mut mats, Color::srgb(1.0, 0.95, 0.55)),
        shot: glow(&mut mats, Color::srgb(1.0, 0.45, 1.0)),
        disc: meshes.add(Cylinder::new(1.0, 0.4)),
        warn: mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.35, 0.2, 0.18), unlit: true, alpha_mode: AlphaMode::Blend, ..default() }),
        crystal: meshes.add(Cuboid::new(3.0, 3.0, 3.0)),
        crystal_mat: glow(&mut mats, Color::srgb(0.35, 1.0, 0.95)),
    });
    // (No stand-in bubble for a guard without a cast effect: the program draws nothing for
    // the guard object itself -- `FUN_140264020` / `FUN_140264340` call no effect or draw
    // routine; a guard shows only through its effect's slot 0 at the cast, slot 4 on a block
    // and slot 5 on a break, `FUN_14026cd70` codes 9 / 10, PROGRAM-RE.md "Defence guard
    // object". Six defence skills have no slot 0: Auto Shell, Guard of Water, Mind's Eye,
    // Guardian Angel, Protecting Air and the unnamed 733.)
}

/// `sync_combat_visuals`' memory of the capsules between frames (one `Local`:
/// the system is at Bevy's parameter limit).
#[derive(Default)]
pub struct CapState {
    /// Last frame's capsules (id, centre).
    prev: Vec<(u32, Vec3)>,
    seen_event: u32,
    respawned: Vec<Vec3>,
    /// Where a skill erased a capsule since the last frame (`CapsuleErased`): a capsule
    /// that vanishes there plays slot 3's break burst, any other vanish slot 2's pickup.
    erased: Vec<Vec3>,
    /// The sim tick of the last frame: a smaller one is a new match (forget `prev`).
    last_tick: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn sync_combat_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut lib: ResMut<FxLibrary>,
    map: Res<SkillFx>,
    world: Res<World>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    assets: Option<Res<CombatAssets>>,
    orbs: Option<Res<CapsuleOrbs>>,
    mut orb_mats: orb_material::OrbMats,
    mut live: Local<HashMap<u32, Entity>>,
    mut caps: Local<CapState>,
    mut items: Query<&mut Transform>,
) {
    let Some(assets) = assets else { return };
    // Leaving a match despawned these (they're `MatchScene`); the next match
    // numbers its capsules and shots from 1 again, so forget the old ids.
    live.retain(|_, e| items.contains(*e));
    let c = &world.0.combat;
    let a = fixed.overstep_fraction();
    let bob = (time.elapsed_secs() * 3.0).sin() * 1.0;
    let mut seen = Vec::new();
    // Orbs that appeared on a respawn (`CapsuleAppeared { respawn: true }`) get the
    // materialise flare and the fade-in; the opening fill of a round just stands
    // there (`FUN_14014ee70` writes state 2 directly, `FUN_14014ed10` is the only
    // appearing phase).
    if c.events.last().map_or(0, |e| e.0) < caps.seen_event {
        caps.seen_event = 0;
    }
    caps.respawned.clear();
    caps.erased.clear();
    if world.0.tick < caps.last_tick {
        caps.prev.clear();
    }
    caps.last_tick = world.0.tick;
    let from = caps.seen_event;
    for &(seq, ev) in c.events.iter().filter(|(seq, _)| *seq > from) {
        caps.seen_event = seq;
        match ev {
            CombatEvent::CapsuleAppeared { pos, respawn: true, .. } => caps.respawned.push(pos),
            CombatEvent::CapsuleErased { pos, .. } => caps.erased.push(pos),
            _ => {}
        }
    }
    let respawned = std::mem::take(&mut caps.respawned);
    let is_respawn = |p: Vec3| respawned.iter().any(|r| r.distance_squared(p) < 0.25);
    // Its own idle orb model per kind (`CAPSULE-VISUALS.md`'s slot table);
    // your own capsules can be captured ("You can only capture your own
    // capsules", tutorial text) so the other player's show their dedicated
    // "opponent's view" model (slot 11 Aura, 12 any skill kind) instead.
    for cap in &c.capsules {
        let pos = cap.pos + Vec3::Y * (5.0 + bob);
        let colour = match c.skill(cap.skill).map(|s| s.capsule) {
            _ if cap.skill == AURA_PARTICLE => 0,
            Some(Capsule::Attack) => 1,
            Some(Capsule::Defence) => 2,
            Some(Capsule::Erase) => 3,
            Some(Capsule::Status) => 4,
            Some(Capsule::Environment) => 5,
            _ => 6,
        };
        // The program yaws each capsule slot's render object by slot index x
        // 90 deg (docs/CAPSULE-VISUALS.md); `Combat::capsules` doesn't carry
        // which of the 4 skill slots a capsule is for, so this uses the
        // capsule's own id instead -- a stable per-capsule rotation, not
        // necessarily the same one the original shows for that slot.
        let yaw = Quat::from_rotation_y((cap.id % 4) as f32 * std::f32::consts::FRAC_PI_2);
        for ghost in [false, true] {
            let key = if ghost { cap.id | GHOST_KEY } else { cap.id };
            seen.push(key);
            match live.get(&key).and_then(|e| items.get_mut(*e).ok()) {
                Some(mut t) => t.translation = pos,
                None => {
                    let (layer, slot) = orb_presentations(cap.owner, colour)[ghost as usize];
                    // Who sees it: the owner (own look) or everyone else (ghost).
                    let all: u8 = c.roster.slots().fold(0, |m, j| m | 1 << j);
                    let viewers = if ghost { all & !(1 << cap.owner) } else { 1 << cap.owner };
                    if std::env::var_os("PD_ORB_LOG").is_some() {
                        eprintln!("[orb] cap {} owner {} colour {} ghost {} -> layer {} (viewers {:04b}) slot {} at {:?}", cap.id, cap.owner, colour, ghost as u8, layer, viewers, slot, pos);
                    }
                    let Some(orbs) = &orbs else { continue };
                    let extra = (MatchScene, OrbFade { viewers, alpha: 1.0, key: orb_spot(cap.owner, cap.pos, ghost), resume: is_respawn(cap.pos) });
                    let Some(e) = spawn_orb(&mut commands, orbs, &mut orb_mats, slot, Transform::from_translation(pos).with_rotation(yaw).with_scale(Vec3::splat(if is_respawn(cap.pos) { 0.05 } else { 1.0 })), slot_tint(slot), Some(layer), extra) else { continue };
                    live.insert(key, e);
                }
            }
        }
    }
    // First sight of a capsule this match: play its appear flare + gather of
    // sparkles (docs/CAPSULE-VISUALS.md "Materialise and flare" -- see
    // `materialise_burst`'s doc comment for what's approximated). A capsule
    // that's simply vanished (captured, or erased) gets the pickup burst;
    // `CapsuleErased` says which one (slot 2 or 3, `capsule_birth.rs`).
    if let Some(orbs) = &orbs {
        for cap in &c.capsules {
            if !caps.prev.iter().any(|(id, _)| *id == cap.id) && is_respawn(cap.pos) {
                let pos = cap.pos + Vec3::Y * 5.0;
                let colour = match c.skill(cap.skill).map(|s| s.capsule) {
                    _ if cap.skill == AURA_PARTICLE => 0,
                    Some(Capsule::Attack) => 1,
                    Some(Capsule::Defence) => 2,
                    Some(Capsule::Erase) => 3,
                    Some(Capsule::Status) => 4,
                    Some(Capsule::Environment) => 5,
                    _ => 6,
                };
                // The gather of sparkles is no longer a stand-in at the appear: the program
                // draws slot 0 over the whole respawn wait (state 0), which `capsule_birth.rs`
                // now plays from `Combat::pending_capsules`; only the flare is drawn here
                // (states 1-2, `FUN_14021ba60`). `materialise_burst` stays for the
                // `PD_CAPSULE_PREVIEW=mat` calibration shot.
                flare_burst(&mut commands, orbs, pos, colour);
            }
        }
    }
    {
        // A capsule that is gone. The program plays slot 2 `cap03_death` on a normal take
        // and slot 3 `cap04_braek` (+ common/92, played by `sfx.rs`) when a skill erased it
        // (`FUN_14014fd10` -> `FUN_14021b4f0` modes 3 / 4); `CapsuleErased` says which.
        // Both are drawn by `capsule_birth.rs` at the orb, random rotation.
        for &(id, pos) in caps.prev.iter() {
            if !c.capsules.iter().any(|cap| cap.id == id) {
                let slot = if caps.erased.iter().any(|e| e.distance_squared(pos) < 0.25) { 3 } else { 2 };
                commands.spawn(capsule_birth::capsule_burst(slot, pos + Vec3::Y * capsule_birth::ORB_LIFT, world.0.tick, id));
            }
        }
    }
    caps.prev = c.capsules.iter().map(|cap| (cap.id, cap.pos)).collect();
    for p in &c.projectiles {
        seen.push(p.id);
        // `pos` trails the program's position by half a step (`Projectile::lead`).
        let pos = p.prev_pos.lerp(p.pos, a) + p.lead();
        let facing = Quat::from_rotation_arc(Vec3::Z, (p.pos - p.prev_pos).normalize_or(Vec3::Z));
        match live.get(&p.id).and_then(|e| items.get_mut(*e).ok()) {
            Some(mut t) => {
                t.translation = pos;
                t.rotation = facing;
            }
            None => {
                let at = Transform::from_translation(pos).with_rotation(facing);
                let airborne = c.airborne[p.owner];
                if effects::env_once("PD_FX_LOG").is_some() {
                    let census = map.name(p.skill, airborne).and_then(|n| lib.get(n)).map(|d| d.census(SLOT_PROJECTILE));
                    eprintln!("[fxlog] shot of skill {} flight slot [drawn, hidden, trails, sprites] {census:?}", p.skill);
                }
                if let Some(e) = map.name(p.skill, airborne).and_then(|n| effects::play(&mut commands, &asset_server, &mut lib, n, SLOT_PROJECTILE, at, true, None)) {
                    commands.entity(e).insert(MatchScene);
                    live.insert(p.id, e);
                    continue;
                }
                let laser = c.skill(p.skill).is_some_and(|s| matches!(s.family, 14 | 15));
                let mat = if laser { assets.laser.clone() } else { assets.shot.clone() };
                let e = commands.spawn((Mesh3d(assets.projectile.clone()), MeshMaterial3d(mat), Transform::from_translation(pos), MatchScene)).id();
                live.insert(p.id, e);
            }
        }
    }
    // Set areas: their own effect (slot 1, looping) where the game has one,
    // and a faint disc while they still show where they will land ("The area
    // that will be affected by this attack is shown for an instant").
    for area in &c.areas {
        // A Rain cloud is slot 5 of the skill's effect, 100 u above the target, following
        // it (`FUN_14026b480`, refreshed each tick by `FUN_14026bbd0`); every other area
        // plays slot 1 where it was set.
        let cloud = area.spec.kind == crate::moves::area::Kind::RainCloud;
        let pos = if cloud { area.cloud } else { area.pos + Vec3::Y * 0.5 };
        seen.push(area.id);
        if !live.contains_key(&area.id) {
            let at = Transform::from_translation(pos);
            let name = map.name(area.skill, c.airborne[area.owner]);
            if effects::env_once("PD_FX_LOG").is_some() {
                eprintln!("[fxlog] area of skill {} kind {:?} effect {name:?} census slot 1 [drawn, hidden, trails, sprites] {:?}", area.skill, area.spec.kind, name.and_then(|n| lib.get(n)).map(|d| d.census(SLOT_PROJECTILE)));
            }
            let e = name.and_then(|n| effects::play(&mut commands, &asset_server, &mut lib, n, if cloud { SLOT_BREAK } else { SLOT_PROJECTILE }, at, true, None)).unwrap_or_else(|| commands.spawn((at, Visibility::default())).id());
            commands.entity(e).insert(MatchScene);
            live.insert(area.id, e);
        } else if cloud {
            if let Some(mut t) = live.get(&area.id).and_then(|e| items.get_mut(*e).ok()) {
                t.translation = pos;
            }
        }
        // Hole / Rain only: a mine's area is "very difficult to see" (guide).
        // READ (FUN_14026a7f0, the Hole update): during the warning the program writes the object's growing
        // scale (+0x194: 0 -> full over s64 ticks, then the slide) into the matrix of its slot-1 EFFECT
        // handle (+0x30, `FUN_140274b30` then the slot matrix block); it calls no draw routine of its own, so
        // the marker IS the skill's own ground effect. The flat red disc below was a stand-in made from the
        // community guide ("added by us", AUDIT-EFFECTS-SOUND.md); the original footage shows none
        // (compare3 sheets 22 / 86 / 147 / 148 / 312 / 319). `PD_AREA_DISC=1` brings it back.
        if area.warning() && area.spec.place == crate::moves::area::Place::Target && std::env::var_os("PD_AREA_DISC").is_some_and(|v| v == "1") {
            let key = area.id | DISC_KEY;
            seen.push(key);
            if !live.contains_key(&key) {
                let t = Transform::from_translation(pos).with_scale(Vec3::new(area.spec.radius, 1.0, area.spec.radius));
                let e = commands.spawn((Mesh3d(assets.disc.clone()), MeshMaterial3d(assets.warn.clone()), t, MatchScene)).id();
                live.insert(key, e);
            }
        }
    }
    // The environment crystal (aqua capsule colour), spinning slowly.
    if let Some(cr) = &c.crystal {
        seen.push(cr.id);
        let spin = Quat::from_rotation_y(time.elapsed_secs() * 1.5) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_4) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let t = Transform::from_translation(cr.pos + Vec3::Y * (5.0 + bob)).with_rotation(spin).with_scale(Vec3::new(1.0, 1.8, 1.0));
        match live.get(&cr.id).and_then(|e| items.get_mut(*e).ok()) {
            Some(mut old) => *old = t,
            None => {
                let e = commands.spawn((Mesh3d(assets.crystal.clone()), MeshMaterial3d(assets.crystal_mat.clone()), t, MatchScene)).id();
                live.insert(cr.id, e);
            }
        }
    }
    live.retain(|id, e| {
        let keep = seen.contains(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::{behaviour_of, Behaviour};

    /// A respawned orb starts from the fade level its spot's last orb left (`OrbAlphaMemory`),
    /// not from a fixed 0.05; an orb of a fresh round (no `resume`) starts at 1 and records it.
    #[test]
    fn a_respawned_orb_resumes_the_fade_level_its_spot_left() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default()).init_resource::<OrbAlphaMemory>().add_systems(Update, fade_capsule_orbs);
        let (far, near) = (orb_spot(0, Vec3::new(5.0, 0.0, 7.0), false), orb_spot(0, Vec3::new(50.0, 0.0, 7.0), false));
        app.world_mut().resource_mut::<OrbAlphaMemory>().0.insert(near, 0.25);
        let spawn = |app: &mut App, key: OrbSpot, resume: bool| app.world_mut().spawn((OrbFade { viewers: 1, alpha: 1.0, key, resume }, Transform::default())).id();
        let (a, b, c) = (spawn(&mut app, near, true), spawn(&mut app, far, true), spawn(&mut app, far, false));
        app.update();
        let alpha = |app: &App, e| app.world().get::<OrbFade>(e).unwrap().alpha;
        assert_eq!(alpha(&app, a), 0.25, "took the take-from-close level back");
        assert_eq!(alpha(&app, b), 1.0, "a spot that never faded comes back at full strength");
        assert_eq!(alpha(&app, c), 1.0);
        assert_eq!(app.world().resource::<OrbAlphaMemory>().0[&far], 1.0, "the level is remembered for the next respawn");
        assert_eq!(app.world().get::<Transform>(a).unwrap().scale, Vec3::splat(0.25));
    }

    /// The converted cache (`PD_CACHE`, else the main checkout's `engine/cache`
    /// next to this crate); None = not converted on this machine.
    fn cache() -> Option<std::path::PathBuf> {
        let p = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        p.join("skills.json").exists().then_some(p)
    }

    /// Sweep (2026-09-27, moves lane): every skill that travels or strikes,
    /// with what will be visible while it flies / swings. Needs the cache;
    /// prints the table with `-- --nocapture`. Flights whose effect shows
    /// nothing fall back to the stand-in ball (sync_combat_visuals), so a
    /// shot is never invisible; a strike always has a swing.
    #[test]
    fn every_attack_shows_something_in_flight_or_swing() {
        let Some(cache) = cache() else {
            eprintln!("no cache: sweep skipped");
            return;
        };
        let skills = crate::load_skills(&cache.join("skills.json"), false);
        let names = SkillFx::load(&cache.join("skills.json"));
        let mut lib = FxLibrary::new(cache.clone());
        let (mut effect, mut standin, mut swings, mut no_file) = (0, Vec::new(), 0, Vec::new());
        for def in &skills {
            let b = behaviour_of(def);
            let (kind, slot) = match b {
                Behaviour::Melee(_) => ("strike", SLOT_CAST),
                Behaviour::Shoot(_) => ("flight", SLOT_PROJECTILE),
                Behaviour::Place(_) => ("set", SLOT_PROJECTILE),
                _ if def.capsule == crate::combat::Capsule::Attack => ("other", SLOT_CAST),
                _ => continue,
            };
            // Ground cast (airborne = false): the skills proven here are all
            // ground-cast in the sim; a mid-air cast may use a different
            // effect (action.air.effect) but isn't swept here.
            let Some(fx) = names.name(def.id, false).and_then(|n| lib.get(n)) else {
                no_file.push(format!("{} {}", def.id, def.name));
                continue;
            };
            let own = names.name(def.id, false).unwrap_or_default().to_string();
            let [drawn, hidden, trails, sprites] = fx.census(slot);
            let shows = fx.shows(slot, kind != "strike");
            eprintln!("{:>4} {:22} {kind:6} slot {slot}: drawn {drawn:>3} hidden {hidden:>2} trails {trails} sprites {sprites} -> {} ({own})", def.id, def.name, if shows { "effect" } else if kind == "flight" { "STAND-IN" } else { "nothing" });
            match (kind, shows) {
                ("strike", true) => swings += 1,
                ("strike", false) => panic!("{} {} strikes with no swing effect", def.id, def.name),
                ("flight", true) => effect += 1,
                ("flight", false) => standin.push(format!("{} {}", def.id, def.name)),
                _ => {}
            }
        }
        eprintln!("strikes with a swing: {swings}; flights with their own effect: {effect}; flights on the stand-in: {standin:?}; no effect file: {} ({:?})", no_file.len(), no_file);
        // Before 2026-09-27 six flights showed nothing at all (Hell's Fire,
        // Swarm of Moths, Root of Tree, Venom Fang, Crawlers, Snap's trail).
        assert!(effect > 60, "flight effects went missing: {effect}");
    }

    /// Owner/layer/model mapping for every owner (2026-09-29 orb-ownership
    /// lane, widened to four players): each capsule is presented twice -- its
    /// owner's own full-colour orb on the owner's own layer, and the dedicated
    /// "opponent's" look on the owner's ghost layer (one ghost, seen by every
    /// other camera). Uses the actual production helper (`orb_presentations`,
    /// called by `sync_combat_visuals`), not a copy of its logic. The own
    /// slots pin the s9-vs-`CAPSULE_COLORS` ordering bridge
    /// (`COLOUR_TO_ORB_SLOT`: colour 5 = environment -> slot 10, colour 6 =
    /// special -> slot 9).
    #[test]
    fn orb_presentations_every_owner() {
        let own = [4, 5, 6, 7, 8, 10, 9];
        assert_eq!(COLOUR_TO_ORB_SLOT, own, "own-model slot table changed");
        for owner in 0..MAX_PLAYERS {
            for colour in 0..7 {
                let [(layer, slot), (ghost_layer, ghost_slot)] = orb_presentations(owner, colour);
                assert_eq!((layer, slot), (1 + owner, own[colour]), "owner {owner} colour {colour}: own presentation on the owner's layer");
                assert_eq!(ghost_layer, GHOST_LAYER_BASE + owner, "owner {owner} colour {colour}: ghost on the owner's ghost layer");
                assert_eq!(ghost_slot, if colour == 0 { OPPONENT_AURA_SLOT } else { OPPONENT_OTHER_SLOT }, "owner {owner} colour {colour}: ghost model");
                assert_ne!(layer, ghost_layer, "owner {owner} colour {colour}: own and ghost layers must differ");
                assert!(ghost_slot == 11 || ghost_slot == 12, "ghost slot {ghost_slot} outside 11/12");
            }
        }
        // Eight layers in all (four own, four ghost), none shared.
        let mut layers: Vec<usize> = (0..MAX_PLAYERS).flat_map(|o| orb_presentations(o, 0).map(|(l, _)| l)).collect();
        layers.sort_unstable();
        layers.dedup();
        let want: Vec<usize> = (1..=MAX_PLAYERS).chain(GHOST_LAYER_BASE..GHOST_LAYER_BASE + MAX_PLAYERS).collect();
        assert_eq!(layers, want);
    }

    /// Who sees which capsule, for every owner and every camera of a
    /// two-, three- and four-player match: the owner's camera sees only the
    /// own-look orb, every other camera only the ghost -- never both, never
    /// neither, and never another player's own-look model.
    #[test]
    fn every_camera_sees_exactly_one_presentation_of_every_capsule() {
        use bevy::camera::visibility::RenderLayers;
        for players in 2..=MAX_PLAYERS {
            for cam in 0..players {
                let mask = camera_layers(cam, players);
                assert!(mask.intersects(&RenderLayers::layer(0)), "{players} players: camera {cam} sees the shared world");
                for owner in 0..players {
                    let [(own, _), (ghost, _)] = orb_presentations(owner, 1);
                    let (sees_own, sees_ghost) = (mask.intersects(&RenderLayers::layer(own)), mask.intersects(&RenderLayers::layer(ghost)));
                    assert_eq!((sees_own, sees_ghost), (cam == owner, cam != owner), "{players} players: camera {cam}, capsule of {owner}");
                }
            }
        }
    }

    /// The two presentations of one capsule never share a model slot: the
    /// own look (4-10 by kind) and the opponent look (11/12) are disjoint
    /// sets, so no capsule can render "backwards" by slot confusion.
    #[test]
    fn orb_presentation_slots_disjoint() {
        for colour in 0..7 {
            let own_slot = COLOUR_TO_ORB_SLOT[colour];
            assert!((4..=10).contains(&own_slot), "colour {colour}: own slot {own_slot} outside 4-10");
            let ghost_slot = if colour == 0 { OPPONENT_AURA_SLOT } else { OPPONENT_OTHER_SLOT };
            assert_ne!(own_slot, ghost_slot, "colour {colour}: own and ghost share slot {own_slot}");
        }
    }

    /// Drives the test spawn system below (one schedule run = spawn + flush):
    /// for every owner, the own-look orb (slot 5) on the owner's layer and the
    /// ghost (slot 12) on the owner's ghost layer, each at its own x.
    fn spawn_test_orbs(mut commands: Commands, orbs: Res<CapsuleOrbs>, mut orb_mats: orb_material::OrbMats) {
        for owner in 0..MAX_PLAYERS {
            let [(own, own_slot), (ghost, ghost_slot)] = orb_presentations(owner, 1);
            spawn_orb(&mut commands, &orbs, &mut orb_mats, own_slot, Transform::from_xyz(owner as f32 * 100.0, 0.0, 0.0), Color::WHITE, Some(own), ());
            spawn_orb(&mut commands, &orbs, &mut orb_mats, ghost_slot, Transform::from_xyz(owner as f32 * 100.0 + 50.0, 0.0, 0.0), Color::WHITE, Some(ghost), ());
        }
    }

    /// The actual bug behind the 2026-09-29 flip report: the owner/view/slot
    /// mapping above was always right, but `spawn_orb` put the view's
    /// `RenderLayers` only on the mesh-less root while the draw meshes spawn
    /// as `ChildOf` children -- and Bevy 0.19 culls each entity by its OWN
    /// mask (no parent walk; entities without the component belong to layer
    /// 0, which every split-screen camera includes), so every view rendered
    /// both models superimposed. This test drives the REAL `spawn_orb` (not a
    /// copy of its logic) with synthetic cache data carrying the REAL
    /// production per-draw roles (`kind_spec`), flushes `Commands`, then
    /// censuses every actual `Mesh3d` descendant of 4 owners x 2 looks: each
    /// must carry exactly its layer, be rendered by exactly the cameras that
    /// should see it (the owner for the own look, the other three for the
    /// ghost) and by no other. It fails on the old root-only code (children
    /// resolve to layer 0) and passes with the fix.
    #[test]
    fn spawn_orb_children_carry_their_layer_for_four_owners_and_four_cameras() {
        use bevy::camera::visibility::RenderLayers;
        // NOTE: bare `World` here is the game's own match-state resource
        // (`crate::World` via `super::*`); the ECS world needs its full path.
        let mut world = bevy::ecs::world::World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<orb_material::OrbSphereLit>>();
        world.init_resource::<Assets<orb_material::OrbSphereUnlit>>();
        world.init_resource::<Assets<orb_material::OrbRim>>();
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Sphere::new(4.5)));
        let tex = world.resource_mut::<Assets<Image>>().add(Image::default());
        let draws = |slot: usize| {
            orb_material::kind_spec(slot)
                .iter()
                .map(|&role| OrbDraw {
                    role,
                    mesh: mesh.clone(),
                    base_tex: Some(tex.clone()),
                    secondary_tex: Some(tex.clone()),
                    tertiary_tex: None,
                    quaternary_tex: None,
                })
                .collect::<Vec<_>>()
        };
        world.insert_resource(CapsuleOrbs {
            orb_draws: HashMap::from([(5, draws(5)), (12, draws(12))]),
            ..Default::default()
        });
        let mut schedule = bevy::ecs::schedule::Schedule::new(bevy::app::Update);
        schedule.add_systems(spawn_test_orbs);
        schedule.run(&mut world);
        let cameras: Vec<RenderLayers> = (0..MAX_PLAYERS).map(|i| camera_layers(i, MAX_PLAYERS)).collect();
        for owner in 0..MAX_PLAYERS {
            let [(own, own_slot), (ghost, ghost_slot)] = orb_presentations(owner, 1);
            for (slot, layer, x, looker) in [(own_slot, own, owner as f32 * 100.0, true), (ghost_slot, ghost, owner as f32 * 100.0 + 50.0, false)] {
                let want = orb_material::kind_spec(slot).len();
                let root = world
                    .query_filtered::<(Entity, &Transform), With<OrbRoot>>()
                    .iter(&world)
                    .find(|(_, t)| (t.translation.x - x).abs() < 0.01)
                    .map(|(e, _)| e)
                    .expect("spawn_orb produced no root (slot has no draws?)");
                let root_layers = world.get::<RenderLayers>(root).cloned().expect("orb root carries its layer");
                assert!(root_layers == RenderLayers::layer(layer), "root mask (owner {owner} slot {slot})");
                let children = world.get::<Children>(root).expect("orb root has draw children").to_vec();
                let mut meshes = 0;
                for c in children {
                    assert!(world.get::<Mesh3d>(c).is_some(), "every orb child is a draw mesh (slot {slot})");
                    meshes += 1;
                    // Resolved exactly the way the renderer resolves it: no
                    // component = layer 0 = visible in EVERY view (the old bug).
                    let resolved = world.get::<RenderLayers>(c).cloned().unwrap_or_default();
                    assert!(resolved == RenderLayers::layer(layer), "draw child mask (owner {owner} slot {slot})");
                    for (cam, mask) in cameras.iter().enumerate() {
                        let sees = mask.intersects(&resolved);
                        assert_eq!(sees, (cam == owner) == looker, "camera {cam} vs the {} look of player {owner}'s capsule", if looker { "own" } else { "ghost" });
                    }
                }
                assert!(meshes == want && want > 0, "sphere AND rim children spawned, not zero (slot {slot}: {meshes} vs {want})");
            }
        }
    }
    /// Four-player match through `skill_effects` (the cast/hit/block/burst
    /// mapping), run in a real `App`: hits, blocks and bursts between every
    /// pair of seats (attacker 2 and 3 included), then knock-outs down to the
    /// last player standing. It used to panic at the first event whose
    /// victim was seat 3 or 4 (`1 - victim` and two-wide arrays).
    #[test]
    fn four_players_run_through_events_and_knock_outs_without_panicking() {
        use crate::combat::{Combat, CombatEvent, Roster, AURA_PARTICLE};
        use crate::sim::Match;
        use pd_formats::collision::Collision;
        let spawns = [Vec3::new(-100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, 100.0), Vec3::new(-100.0, 0.0, 100.0)];
        let combat = Combat::with_roster(Vec::new(), Roster::free_for_all(4), std::array::from_fn(|_| vec![AURA_PARTICLE; 30]));
        let mut m = Match::with_spawns(&spawns, combat);
        let mut col = Collision::new(Vec::new());
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.insert_resource(FxLibrary::new(std::path::PathBuf::new()));
        // A skill with an effect name: the play paths run (the file is absent, so nothing spawns).
        app.insert_resource(SkillFx(HashMap::from([(7, (Some("eft0007".to_string()), Some("eft0007".to_string())))])));
        for i in 0..MAX_PLAYERS {
            app.world_mut().spawn(PlayerVisual(i));
        }
        app.add_systems(Update, skill_effects);
        for _ in 0..3 {
            m.step([crate::sim::Input::default(); 4], &mut col);
        }
        app.insert_resource(World(m));
        app.update();
        let mut seq = 0;
        let mut push = |app: &mut App, ev: CombatEvent| {
            seq += 1;
            app.world_mut().resource_mut::<World>().0.combat.events.push((seq, ev));
        };
        for attacker in 0..MAX_PLAYERS {
            for victim in (0..MAX_PLAYERS).filter(|&v| v != attacker) {
                push(&mut app, CombatEvent::Hit { attacker, victim, skill: 7 });
                push(&mut app, CombatEvent::Blocked { attacker, victim, skill: 7, broken: false });
            }
            push(&mut app, CombatEvent::Burst { attacker, pos: Vec3::new(5.0, 0.0, 5.0), skill: 7 });
            app.update();
        }
        // Knock-outs: seats 4, 3, 2 in turn; seat 1 is the winner.
        for victim in [3, 2, 1] {
            app.world_mut().resource_mut::<World>().0.combat.change_health(victim, -999);
            let mut col = Collision::new(Vec::new());
            for _ in 0..4 {
                app.world_mut().resource_mut::<World>().0.step([crate::sim::Input::default(); 4], &mut col);
                app.update();
            }
        }
        let c = &app.world().resource::<World>().0.combat;
        assert_eq!(c.round_outcome(), Some(crate::combat::Outcome::Winner { slot: 0, team: c.roster.team[0] }), "the last player standing won");
    }
}