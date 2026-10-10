//! Combat rules: health, level/aura, skill slots, capsules, skills, rounds.
//!
//! Part of the deterministic simulation (same contract as sim.rs): integer
//! counters, tick-based timers, a seeded random generator in the state, no
//! clocks, no Bevy types. Skill numbers come from the game's skill table
//! (`cache/skills.json`, see docs/FORMATS-SKILLS.md); values the table does
//! not give are the constants below (docs/GAME.md "Combat").

use crate::moves::area::{self, AreaSpec, Kind, Trigger};
use crate::moves::barrier::Arrival;
use crate::moves::effects::{Crystal, Delivery, Reach, Stat, Status, CRYSTAL_CAPTURE_AFTER};
use crate::moves::environment;
use crate::moves::{self, barrier, body_centre, Behaviour, Launch, Shot, HIT_BODY_CENTRE, HIT_BODY_RADIUS};
use crate::sim::{toward, turn_toward, yaw_of, BodyState, Player, BODY_HEIGHT, DT, LAND_FALL_HEAVY, LAND_FALL_MIN, STEP_HEIGHT, TURN_RATE_DEG_PER_S};
use glam::Vec3;
use crate::props::{self, Prop, PropState};
use pd_formats::collision::{Collision, NO_OBJECT};
use std::collections::HashMap;

mod lock;
mod reactions;
mod roster;
#[cfg(test)]
mod skill_audit;
#[cfg(test)]
mod livefix;
#[cfg(test)]
mod leftovers;
#[cfg(test)]
mod combos;
pub use roster::{FriendlyFire, Roster, DRAW, MAX_PLAYERS};

/// How a decided round ended (`Combat::round_outcome`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `slot` was the last standing player of `team`.
    Winner { slot: usize, team: u8 },
    Draw,
}

/// Every player's feet position and facing, by slot (inactive slots are
/// zeroed and never read).
type Bodies = [(Vec3, f32); MAX_PLAYERS];

fn bodies_of(players: &[Player]) -> Bodies {
    std::array::from_fn(|i| players.get(i).map_or((Vec3::ZERO, 0.0), |p| (p.target_point(), p.yaw)))
}

// Laurence's picks (DECISIONS.md).
pub const START_HEALTH: i32 = 20;
/// The original's opening Level (fan guides, two sources: MATCH-RULES.md §1);
/// the setup screen can change it (`Rules`).
pub const START_LEVEL: i32 = 0;
/// Skills dealt onto the buttons at the start ("4 random skills already on
/// the buttons", MATCH-RULES.md §2).
pub const START_HAND: usize = 4;
/// A captured capsule's spot has its next capsule pickable this many sim ticks
/// later: 420 program ticks hidden (`FUN_14014fd10` sets the slot's timer to
/// `0x1a4` = 420 + status 34) then 15 program ticks appearing (`FUN_14014ed10`:
/// state 0 counts 420 down, state 1 counts 15 more, state 2 is pickable) =
/// 435 ticks of the program's 1/30 s = 870 sim ticks = 14.5 s. The program
/// steps at 30 Hz and the sim at 60 Hz, so every program tick is two sim ticks
/// (`moves::SIM_TICKS_PER_TABLE_TICK`): 2 x 420 hidden, 2 x 15 appearing.
pub const CAPSULE_RESPAWN_TICKS: u16 = 870;
/// Of those, the last 15 program ticks (30 sim ticks): the capsule is there
/// (the appear sound plays, `CapsuleItem::appear` counts down) but can't be
/// picked up yet.
pub const CAPSULE_APPEAR_TICKS: u16 = 30;
/// A capsule can be captured from this close, in 3-D (`FUN_14014f630`:
/// distance from the capsule's ground point to the player <= `0x1402f7fb0`
/// = 15 u; the orb's own height above that point is `0x1402f7f64` = 9 u).
pub const PICKUP_REACH: f32 = 15.0;
/// Level is clamped to 0..99 in every ordinary rule (`FUN_14024d970`:
/// gsdata `0x33044 + rule * 0x10` min, `+ 0x33048` max; rules 0-2, 5-9).
pub const MAX_LEVEL: i32 = 99;
/// Aura is kept as a fixed-point count of this many units per point so the
/// program's refill curve (a few hundredths of a point per tick) needs no
/// rounding: 60,000 units/point = 1000 units per sim tick at 1 point/s.
pub const AURA_TICKS_PER_POINT: i32 = 60_000;
/// The refill curve `max(0.25, 1.25 - Aura / 10)` points per second
/// (`FUN_14024de20`: constants `0x1402f7ddc` 1.25, `0x1402f7f7c` 10.0,
/// `0x1402f7d0c` 0.25, per 30 Hz tick `/ 0x1402f7fd8` = 30).
pub const AURA_CURVE_TOP: f32 = 1.25;
pub const AURA_CURVE_SLOPE: f32 = 10.0;
pub const AURA_CURVE_FLOOR: f32 = 0.25;
/// The same arithmetic per program tick (1/30 s): the refill runs on every
/// second sim tick and adds a whole program tick's worth.
pub const PROGRAM_TICK_DIV: f32 = 30.0;
pub const SLOTS: usize = 4;
/// Without stage capsule spots (Call's anchor, tests), a capsule from your
/// arsenal appears around you this often (5 s: 3 spots x 15 s)...
pub const CAPSULE_EVERY_TICKS: u16 = 300;
/// ...while fewer than this many of yours are lying around.
pub const MAX_CAPSULES_PER_PLAYER: usize = 3;
pub const CAPSULE_MIN_DIST: f32 = 25.0;
pub const CAPSULE_MAX_DIST: f32 = 70.0;
pub const CAST_TICKS: u16 = 24;
/// How long the Aura Particle's green body glow shows, in sim ticks from the press
/// (render-only counter `Fighter::absorb_glow_ticks`; the use itself holds nobody, see MOVES.md
/// "Which casts hold the fighter still").
/// MEASURED on `Orb Collect & Aura Consumed.mp4` (30 fps): the press (Level 00 -> 01 on
/// the HUD dial, the Level sound) is frame 160; the glow first shows at frame 162.5 and
/// is gone by 186.5 = 26.5 frames = 53 ticks (aura_pulse.rs, CHAR-LIGHT.md section 8).
pub const AURA_GLOW_TICKS: u16 = 53;
/// Ordinary hit-stun when the reaction row doesn't give its own tick count
/// (PROGRAM-RE.md "Hit reactions" kinds 0/1/2/3/7: "play one clip for its
/// length" — we don't always know a character's clip length for these, so
/// this flat guess still backs them up).
pub const HIT_STUN_TICKS: u16 = 30;
/// Knock-down timeline, in sim ticks (PROGRAM-RE.md "Knock-down physics",
/// program ticks x `moves::SIM_TICKS_PER_TABLE_TICK`): the flight itself is
/// physics (`Combat::start_knockdown` / `step_knockdown_flight`: launch
/// speed/angle from the reaction row, `KNOCKDOWN_GRAVITY`, real ground
/// collision), so only the ground part is a fixed tick count.
/// 60 program ticks (2 s) lying on the ground before getting up.
pub const KNOCK_LYING_TICKS: u16 = 120;
/// 30 program ticks (1 s), the get-up clip (DOWN_FRONT_STAND / DOWN_BACK_STAND).
pub const KNOCK_GETUP_TICKS: u16 = 60;
/// Ground time after landing (lying + getting up); tests with no reaction
/// table loaded skip the physics flight and use this as the whole knock-down.
pub const KNOCKDOWN_TICKS: u16 = KNOCK_LYING_TICKS + KNOCK_GETUP_TICKS;
/// "Invulnerable ... until (get-up clip + 45 ticks) after starting to get
/// up": 75 program ticks = 150 sim ticks of blinking, untouchable (no skill
/// sets s62 bit 8, the only thing that could reach you then).
pub const POST_GETUP_INVULN_TICKS: u16 = 150;
/// Kind 1 ("clip, then lie", rows 9 / 11 / 13 / 25, ~42 attack-skill slots):
/// the collapse clip runs (state 15, timer = its length, `0x140256e80` case
/// 0xf), then state 18 is entered with timer 0x1e and counts it out before the
/// get-up: 30 + 1 program ticks of lying = 62 sim ticks. With the 50-frame
/// DMG_DRAIN_HIGH and the 30-frame get-up that is ~110 program ticks (3.7 s).
pub const COLLAPSE_LIE_TICKS: u16 = 62;
/// The reaction row a fighter who dies standing (or by a cost, poison, a drop)
/// plays: row 25 = DMG_DRAIN_HIGH then DOWN_BACK (`0x14024ee75`: state 0 /
/// 0x15 / 0x16 -> `FUN_140258690(0x19)`).
pub const KO_COLLAPSE_ROW: i16 = 25;
/// Falling out of the arena puts the body back on its start point protected
/// for 0x5a = 90 program ticks (`0x140251d80`: `+0x6434 = 0x5a`) = 180 sim ticks.
pub const FALL_OUT_PROTECT_TICKS: u16 = 180;
/// A flyer bounces off a wall at most this often per reaction (`+0xc6c` = 3,
/// set by `FUN_140258690`, spent by `FUN_140259b30`)...
pub const WALL_HITS: u8 = 3;
/// ...and not twice within 0x14 = 20 program ticks (40 sim ticks; `+0xc70`,
/// counted down once per program tick in `0x140256e80`).
pub const WALL_COOLDOWN_TICKS: u16 = 40;
/// A wall halves the flyer's horizontal speed (`0x1402f7d40` = 0.5).
pub const WALL_SPEED_KEEP: f32 = 0.5;
/// The ground slide of a landed flyer loses 15% per **program** tick
/// (`0x1402f7d8c` = 0.85, `0x14025c260`), and stops under 0.02 u/tick.
pub const SLIDE_KEEP: f32 = 0.85;
pub const SLIDE_STOP_SPEED: f32 = 0.02 * 30.0;
/// A flyer that has not changed height by more than half a unit per program
/// tick for 0x5a = 90 program ticks is put down (`0x14025c260`).
pub const STUCK_FLIGHT_TICKS: u16 = 180;
/// The program's own knock-down gravity, 0.12 u/(1/30 s)^2 (PROGRAM-RE.md
/// "Knock-down physics") converted to u/s^2 -- separate from the sim's
/// jump/fall gravity (`sim::GRAVITY`), which is tuned for ordinary jumps.
pub const KNOCKDOWN_GRAVITY: f32 = 0.12 * 30.0 * 30.0;
/// A reaction row's `launch_speed` is in the table's speed unit: value /
/// 10.8 u per (1/30 s) program tick = value x this many u/s (PROGRAM-RE.md
/// "Units": "speed 2.78 u/s per point").
pub const LAUNCH_SPEED_UNIT: f32 = 30.0 / 10.8;
/// A hit while airborne always knocks down, regardless of s57-s59 (row 27;
/// PROGRAM-RE.md "Hit reactions": s61 is 0 on every skill).
pub const AIR_KNOCKDOWN_ROW: i16 = 27;
/// The damage accumulator that picks light/medium/heavy empties this many
/// sim ticks (3 s) after the last hit (PROGRAM-RE.md "Damage").
pub const DAMAGE_WINDOW_TICKS: u32 = 180;
/// Fallback knock-down/pinned ids for tests that don't load a reaction
/// table (`Combat::set_reaction_rows`): the pre-PROGRAM-RE.md guess, from
/// every "Knocks down target" / "Requires: Knocked down target" note. Real
/// play reads a row's actual `kind` instead (`reaction_is_knockdown`).
const FALLBACK_KNOCKDOWN_ROWS: [i16; 5] = [3, 13, 45, 48, 61];
const FALLBACK_PINNED_ROW: i16 = 24;
/// "The standard allotted time for a battle is 15 minutes ... When it reaches
/// 0:00 ... the battle must end" (guide). The one with more Health wins.
pub const TIME_LIMIT_TICKS: u32 = 10 * 60 * 60;
/// A second knock-out this soon after the first (0x3c = 60 program ticks =
/// 120 sim ticks: `FUN_14012c510` counts deaths within `earliest - 0x3c` as
/// simultaneous, `FUN_14012c440`) leaves nobody standing: a draw.
pub const SIMULTANEOUS_KO_TICKS: u32 = 120;
/// "when you have run out of skills and no more capsules are spawning ...
/// your life is affected, decreasing your Health" (guide). Rate unconfirmed:
/// one point every this many ticks.
pub const POWERLESS_DRAIN_TICKS: u32 = 3 * 60;
/// Falling out of the arena: "a damage of 3 is incurred" (guide).
pub const FALL_OUT_DAMAGE: i32 = 3;
pub const ROUND_END_TICKS: u16 = 180;
/// "3, 2, 1" before each round, one second each; "START" shows as it ends.
pub const COUNTDOWN_TICKS: u32 = 180;
/// Program ticks a shot turned back by an About Face / Reverse field waits before it moves again
/// (`FUN_14026cd70` code 6: `+0x20 = 0x14`).
const REVERSE_WAIT_TICKS: u16 = 20;
/// Short-range dash: the window comes from the action row (`Combat::action_row`)
/// and the speed and turn from the skill's own s65 / s66 (`moves::melee::Melee`,
/// `FUN_14018f050`: (10 / window) x s65 u a program tick, never over 10).
/// The dash stops this far short of the target's centre (PROGRAM-RE.md: "a
/// quirk" -- the program can end slightly inside this).
pub const LUNGE_STOP_SHORT: f32 = 11.0;
/// The melee dash only engages a target within this range at cast time
/// (PROGRAM-RE.md "Lock-on": distance band 1); fallback window length for a
/// skill with no action row.
pub const MELEE_DASH_RANGE: f32 = 80.0;
pub const LUNGE_MAX_TICKS: u16 = 36;
/// Height above the feet that attacks aim at and fire from.
pub const CHEST: f32 = 12.0;
/// The least reach, from a guard's centre, at which a shot is judged "within the field" of an
/// orbit-changing guard (Course / Reverse / About Face). It was `BODY_RADIUS * 3` = 13.5 and stays
/// 13.5: a guess of ours (UNCONFIRMED; the program's front disc is 13 u, PROGRAM-RE.md), not tied to
/// the body radius, which is now the charaset's (`body.rs`).
const GUARD_FIELD_MIN: f32 = 13.5;

/// The built-in aura particle (it has no record in the skill table).
pub const AURA_PARTICLE: i16 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capsule {
    Attack,
    Defence,
    Erase,
    Status,
    Special,
    Environment,
}

impl Capsule {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "attack" => Capsule::Attack,
            "defence" => Capsule::Defence,
            "erase" => Capsule::Erase,
            "status" => Capsule::Status,
            "special" => Capsule::Special,
            "environment" => Capsule::Environment,
            _ => return None,
        })
    }
}

/// One skill's numbers, from the skill table.
#[derive(Clone, Debug)]
pub struct SkillDef {
    pub id: i16,
    pub name: String,
    pub capsule: Capsule,
    /// Family / motion category (table field s42): 10 short-range, 14-15 [Arc], 80 [Barrier]...
    pub family: i16,
    pub cost: i32,
    pub strength: i32,
    pub strength_rule: i16,
    pub requirement: i16,
    pub requirement_amount: i32,
    pub level_penalty: i32,
    pub health_cost: i32,
    /// DEF of a defence skill (table field s47).
    pub defence: i32,
    /// The whole table record (s0..s69); move behaviours read their fields
    /// from it (moves/mod.rs, docs/FORMATS-SKILLS.md).
    pub raw: [i16; 70],
    /// The program record's last four fields, s70-s73 (PROGRAM-RE.md "Hidden
    /// fields"): the Arc / Ricochet fan steps, the Rain cloud's drop spread.
    pub s70_73: [i16; 4],
}

/// One row of the action table: everything about a cast's timing (ticks are
/// table ticks, 1/30 s -- `moves::table_ticks` to sim ticks). Kept on
/// `Combat` (`Combat::action_row`), not on `SkillDef`, so adding it doesn't
/// touch the many places elsewhere in the crate that build a `SkillDef` by
/// hand. docs/FORMATS-SKILLS.md "Action table and hit-reaction table".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionRow {
    pub anim_name: String,
    /// `common` bank sound id played at the caster when the cast starts.
    pub cast_sound: i16,
    /// Hold-to-charge wind-up (w39): ticks of wind-up before the charge cycle starts.
    pub charge_cycle: i16,
    /// Short-range dash window, ticks after cast start (w41/w42); 0/0 = none.
    pub dash_start: i16,
    pub dash_end: i16,
    /// Ticks from cast start to the attack object being created / the effect
    /// taking hold (w43): this replaces the old flat `CAST_TICKS`.
    pub fire_tick: i16,
    /// Short-range: the hit window ends here (w44); the object/window lives
    /// `hit_window_end - fire_tick` ticks.
    pub hit_window_end: i16,
}

/// A row's combo window (w45 / w46 / w47, READ `FUN_14024fb90`): a press while the cast clip's
/// frame is in `from..=to` is buffered, and the chained cast starts when the frame equals `at`.
/// 0 / 0 / 0 = the row chains nowhere. Kept off `ActionRow` so its literals elsewhere stay put.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChainWindow {
    pub from: i16,
    pub to: i16,
    pub at: i16,
}

impl ActionRow {
    /// The row names clip 0 (`WAIT0`): the program starts no cast state for it and fires the
    /// skill at once (`0x14025a3e0` returns 0 for clip 0, `0x140258180`).
    pub fn fires_at_once(&self) -> bool {
        self.anim_name == "WAIT0"
    }
}

/// A character's right hand through one attack clip, from
/// `cache/chars/<id>.hand.json` (`pd_convert shotpoints`): the program's
/// spawn / weapon point when a skill's effect has no `SHOTnn` marker
/// (`FUN_14018a310` falls back to the owner's `RHAND` bone matrix). One sample
/// per clip frame (30 fps), in the character's frame (feet at the origin, +Z
/// forward at yaw 0): `pos` the bone's position, `fwd` its local +Z axis (the
/// direction a sword blade extends, `FUN_14026c730`). Plain data, so the sim
/// never reads the animation state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandClip {
    pub pos: Vec<Vec3>,
    pub fwd: Vec<Vec3>,
}

impl HandClip {
    /// The hand at clip frame `frame` (30 fps, fractional: interpolated
    /// between the two neighbouring samples; held at the last frame past the
    /// end). `None` for an empty clip.
    pub fn at(&self, frame: f32) -> Option<(Vec3, Vec3)> {
        let last = self.pos.len().checked_sub(1)?;
        let f = frame.clamp(0.0, last as f32);
        let (a, t) = (f.floor() as usize, f - f.floor());
        let b = (a + 1).min(last);
        let fwd = self.fwd.get(a).copied().unwrap_or(Vec3::Z).lerp(self.fwd.get(b).copied().unwrap_or(Vec3::Z), t).normalize_or(Vec3::Z);
        Some((self.pos[a].lerp(self.pos[b], t), fwd))
    }
}

/// A character-frame offset (x right, y up, z forward at yaw 0) as a world
/// position for a body at `feet` facing `yaw` -- the program places a marker
/// by multiplying it into the caster's matrix (`FUN_14018a310`).
pub fn local_to_world(feet: Vec3, yaw: f32, local: Vec3) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    feet + Vec3::new(local.x * c + local.z * s, local.y, -local.x * s + local.z * c)
}

/// A clip name as the `&'static str` the root-motion slot holds: the cast rows' names are loaded at
/// run time, and the set of them is small (a few dozen), so each is leaked once.
fn intern_clip(name: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut m = NAMES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = m.get(name) {
        return s;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    m.insert(name.to_string(), leaked);
    leaked
}

/// The same for a direction.
pub fn local_dir_to_world(yaw: f32, local: Vec3) -> Vec3 {
    local_to_world(Vec3::ZERO, yaw, local)
}

/// One row of the hit-reaction table: what a hit-reaction id (s57-s62) picks
/// -- `Combat::set_reaction_rows`, loaded once at startup like `ActionRow`.
/// docs/FORMATS-SKILLS.md "Action table and hit-reaction table"; row kinds
/// and w41..w48 meanings: PROGRAM-RE.md "Hit reactions".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactionRow {
    /// How the row plays (`reaction_kind`): a plain clip, four-way/front-back
    /// stagger, knock-down, pinned, a bind, or a fixed tick count.
    pub kind: i16,
    pub effect: i16,
    pub sound: i16,
    /// Voice group (1-6) for the victim's hurt/knocked-down voice (`sfx.rs`).
    pub voice_group: i16,
    /// Knock-down only: launch speed, `LAUNCH_SPEED_UNIT` u/s per point.
    pub launch_speed: i16,
    /// Knock-down only: launch angle in degrees (`launch_components`).
    pub launch_angle: i16,
    /// w41..w48: animation ids / tick counts, meaning depends on `kind`
    /// (`Combat::reaction_clip`).
    pub w: [i16; 8],
    /// Row word 1: the root-motion flag of the reaction clip (the 5th
    /// argument of every clip start in `0x140258690`, `[rsi + 2]`): the clip's
    /// own x/z drift moves the body (`0x140257510`). 0 on the flinch row 46
    /// and the status rows.
    pub root: bool,
}

/// Hit-reaction row kinds (`FUN_140258690`, `engine/re/state_table.py`).
pub mod reaction_kind {
    /// Play one clip (w41) for its own length.
    pub const CLIP: i16 = 0;
    /// Play a clip (w41), then lie down for 30 ticks (w42 names the pose).
    pub const CLIP_THEN_LIE: i16 = 1;
    /// Four-way stagger by hit direction: front/right/back/left = w41..w44.
    pub const FOUR_WAY: i16 = 2;
    /// Front/back stagger: front = w41, back = w42.
    pub const FRONT_BACK: i16 = 3;
    /// Knock-down launch: front pose w42, back pose w46 (`launch_speed`/`launch_angle`).
    pub const KNOCKDOWN: i16 = 5;
    /// Pinned (Pursuit/Sky Pursuit): face-up w41 + lie w42, face-down w43 + lie w44.
    pub const PINNED: i16 = 7;
    pub const BIND_A: i16 = 8;
    pub const BIND_B: i16 = 9;
    /// Play a clip (w41) for a fixed tick count (w42): row 46's 5-tick flinch.
    pub const FIXED_TICKS: i16 = 11;
}

/// Which side an attack arrived from, for a hit reaction's clip/pose
/// (PROGRAM-RE.md "Hit reactions": "the reaction faces the direction the
/// attack came from").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HitDir {
    #[default]
    Front,
    Right,
    Back,
    Left,
}

/// `pos`/`yaw`: the victim's body. `from`: where the attack arrived from
/// (world position). Reuses `barrier::in_front`'s facing-vector convention,
/// extended to four ways with a right vector -- no new trig, just the same
/// `yaw.sin()/cos()` facing already used there.
fn hit_direction(pos: Vec3, yaw: f32, from: Vec3) -> HitDir {
    let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let d = Vec3::new(from.x - pos.x, 0.0, from.z - pos.z);
    if d.length_squared() < 1e-6 {
        return HitDir::Front;
    }
    let (f, r) = (facing.dot(d), right.dot(d));
    if f.abs() >= r.abs() {
        if f >= 0.0 { HitDir::Front } else { HitDir::Back }
    } else if r < 0.0 {
        // The program's sector 1 (+90 deg in its yaw, `0x140258690` kind 2:
        // `((a + 0x1000) >> 13) & 3`) is the DMG_SHOT_RIGHT slot, and that
        // clip's root drifts toward +X at zero yaw (`side` +11.3 in
        // `pc00a.rootmotion.json`) -- the recoil away from an attacker on the
        // -X side. The exporter mirrors Z, which reverses the sense of
        // rotation: the program's +90 deg is our -90 deg. `right` below is our
        // +90 deg, so the attacker on the `-right` side plays the RIGHT clip.
        HitDir::Right
    } else {
        HitDir::Left
    }
}

/// Whether the attack arrived within 90 degrees of the body's facing: the
/// two-way reactions' split (`0x140258a60`: `iVar4 + 0x1fff < 0x3fff` on the
/// angle between the hit bearing and the facing, 0x2000 = 90 deg).
/// The Add codes whose `FUN_140272bc0` case can return 0 ("nothing changed", so no s60 reaction):
/// the erase family (0), the Health / Aura / Level changes that test the value before and after
/// (1, 2, 3, 4, 9, 10, 11), the status reset (17), the environment cancel with its test (39), the
/// Mobilize-type warp (45), Vicious Balance (47, unread: treated alike), the Meltdown / Level-per
/// -particle pair (48, 49: 0 with no particle) and the attribute reversal (51: 0 with no status).
/// Every other code (the statuses, Health +n 43, Level +n 44, Fusion 46...) ends in `return 1`.
fn add_can_return_zero(code: i16) -> bool {
    matches!(code, 0..=4 | 9..=11 | 17 | 39 | 45 | 47..=49 | 51)
}

fn hit_front(pos: Vec3, yaw: f32, from: Vec3) -> bool {
    let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let d = Vec3::new(from.x - pos.x, 0.0, from.z - pos.z);
    d.length_squared() < 1e-6 || facing.dot(d) > 0.0
}

/// The program's "front set" of clip ids (`0x1c1ce00000000000 >> id`, tested at
/// `0x140258f2d`, `0x140259990` and `0x140259b30`): BLOW0-2_FRONT 45-47,
/// BLOW_LAND_LF/MF/SF 50-52, WALLHIT_BACK / _LOOP 58 / 59 and DOWN_FRONT 60.
/// A body in one of these faces the attacker when launched and lands in the
/// front landing clips.
fn program_front_set(clip: usize) -> bool {
    matches!(clip, 45..=47 | 50..=52 | 58..=60)
}

/// The landing clip of a knocked body by fall-height class (0 < 40 u, 1 up to
/// 80 u, 2 above) and set: `0x34 - class` (BLOW_LAND_SF / MF / LF, ids 52 / 51
/// / 50) for the front set, `0x37 - class` (SB / MB / LB, 55 / 54 / 53) for
/// the back set (`FUN_140259990`).
fn landing_clip_id(front: bool, class: u8) -> usize {
    (if front { 52 } else { 55 }) - class.min(2) as usize
}

/// The DOWN_* pose queued behind the landing clip (`FUN_140259990`: the
/// charaset's six flags, `1 1 0 | 1 1 0` for every playable fighter -- read
/// from `charaset_en.dat` records 0-10 -- pick DOWN_FRONT (60) or DOWN_BACK
/// (62)): front set lies DOWN_FRONT after the small landing and DOWN_BACK
/// after the others, the back set the opposite.
fn lying_pose_id(front: bool, class: u8) -> usize {
    match (front, class == 0) {
        (true, true) | (false, false) => 60,
        _ => 62,
    }
}

/// The back-set large landing turns the body half a turn when the DOWN clip
/// starts (charaset `+0x24...` = `0 0 0 180 0 0`, applied by `0x140257510`
/// when the queued clip begins).
fn landing_flips(front: bool, class: u8) -> bool {
    !front && class >= 2
}

/// sin/cos of a knock-down launch angle. The reaction table only ever uses
/// 50 deg (ordinary knock-downs) and 90 deg (Tornado, straight up): fixed
/// constants, not runtime trig (the sim stays free of new trig calls; a
/// truly new angle falls back to a flat launch rather than guessing).
fn launch_components(deg: i16) -> (f32, f32) {
    match deg {
        50 => (0.766_044, 0.642_788),
        90 => (1.0, 0.0),
        _ => (0.0, 1.0),
    }
}

impl SkillDef {
    pub fn aura_particle() -> Self {
        // Absorbed when used: a single use.
        let mut raw = [0; 70];
        raw[pd_formats::skills::field::USES] = 1;
        SkillDef {
            id: AURA_PARTICLE,
            name: "Aura Particle".into(),
            capsule: Capsule::Special,
            family: 0,
            cost: 0,
            strength: 0,
            strength_rule: 0,
            requirement: 0,
            requirement_amount: 0,
            level_penalty: 0,
            health_cost: 0,
            defence: 0,
            raw,
            s70_73: [0; 4],
        }
    }
}

/// A defence skill that is up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveGuard {
    pub skill: i16,
    /// Its numbers; `guard.def` is the DEF it has left (each block takes the
    /// attack's STR off it, `FUN_140271760`).
    pub guard: barrier::Guard,
    pub ticks: u16,
    /// The button it was cast from: a broken guard is erased from it
    /// (`FUN_140262fc0`), a capturing guard's capture lands on it.
    pub button: u8,
    /// It has blocked something and is running out its 5-tick linger.
    pub lingering: bool,
}

impl ActiveGuard {
    pub fn new(skill: i16, guard: barrier::Guard, button: usize) -> Self {
        ActiveGuard { skill, guard, ticks: guard.ticks, button: button as u8, lingering: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Ready,
    /// Playing a skill's cast animation (rooted).
    Cast,
    /// Dashing at the target with a short-range attack.
    Lunge,
    Hit,
    /// Knocked down and getting up (untouchable meanwhile, except by pursuit skills).
    Knocked,
    Down,
}

/// One row of the original's game-rule table (gsdata `0x33040`, stride 0x10:
/// health, Level minimum, Level maximum, minutes; `FUN_14024e790` ->
/// `FUN_14024d020` give a fighter its Health and starting Level, `FUN_14024d970`
/// clamps Level to the range, `FUN_140128cc0` sets the time limit to
/// `minutes x 0x708` program ticks). Re-read from the file; `pd_formats::skills::
/// rule_table` reads the same rows and a data-dependent test pins them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleRow {
    pub health: i32,
    pub level_min: i32,
    pub level_max: i32,
    /// Time limit in minutes; 0 = none.
    pub minutes: i32,
}

const fn row(health: i32, level_min: i32, level_max: i32, minutes: i32) -> RuleRow {
    RuleRow { health, level_min, level_max, minutes }
}

/// The 13 rows (rule id = index): Standard, Ante, Quick, Puny, Heavy, Special
/// Environment, Single School, Highlander, Puny Only, Heavy Only, KO Battle,
/// Annihilate the Odd One (the others), and row 12 = the Odd One himself
/// (rule 11, player 0: `FUN_14024e790` reads `0x33100`/`0x33104`/`0x33108`).
pub const RULE_TABLE: [RuleRow; 13] = [
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(10, 0, 5, 10),
    row(40, 5, 99, 10),
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(20, 0, 99, 10),
    row(5, 0, 99, 3),
    row(10, 0, 99, 10),
    row(40, 0, 99, 0),
];

/// The original's rule in force (`DAT_1404bc5dc`): which table row a fighter
/// takes its Health and Level range from, and the battle's time limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleSet {
    /// 0 Standard .. 11 Annihilate the Odd One (`arsenal::RULE_NAMES`).
    pub rule: u8,
}

impl RuleSet {
    pub const STANDARD: RuleSet = RuleSet { rule: 0 };

    #[allow(dead_code)] // the front end's Game Rule row (lane F3) builds it
    pub fn new(rule: usize) -> Self {
        RuleSet { rule: rule.min(11) as u8 }
    }

    /// The row slot `slot`'s fighter is built from: the Odd One's own row for
    /// rule 11's player 0, the rule's row for everyone else.
    pub fn row(&self, slot: usize) -> RuleRow {
        if self.rule == 11 && slot == 0 {
            RULE_TABLE[12]
        } else {
            RULE_TABLE[self.rule as usize]
        }
    }

    /// The rule's time limit in sim ticks (60 Hz; the table's minutes x 0x708
    /// program ticks, 3600 sim ticks a minute), 0 = none. The setup screen's
    /// own choice (`Combat::time_limit`) overrides it; `Combat::use_rule_time_limit`
    /// takes this one.
    #[allow(dead_code)] // lane F3's setup takes the rule's time limit through `use_rule_time_limit`
    pub fn time_limit_ticks(&self) -> u32 {
        RULE_TABLE[self.rule as usize].minutes.max(0) as u32 * 3600
    }
}

/// Match rules the versus setup can change; `Default` = the original's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// The Level to start at (the setup option); kept inside the rule's range.
    pub start_level: i32,
    /// Skills dealt from the shuffled arsenal onto the buttons at the start.
    pub start_hand: usize,
    pub capsule_respawn_ticks: u16,
    /// The original's rule: per-fighter Health, Level range, time limit.
    pub set: RuleSet,
    /// Game modes (DESIGN-BRAWL.md, `frontend/modes.rs`): capsules exist on the field (the normal
    /// match). Off, none is ever dealt, respawned or shown, so a hand is what you keep.
    pub capsules: bool,
    /// Game modes: a skill on a button never uses up (default off: s26 counts uses as normal).
    pub infinite_uses: bool,
    /// Game modes: this hand is put on the four buttons, in order, at every round start, in place of the
    /// arsenal deal (`start_hand`), and the arsenal is left empty. None = the normal deal.
    pub fixed_hand: Option<[i16; SLOTS]>,
}

impl Rules {
    /// The normal match (`Default`), as a constant a game mode's table can build on with `..Rules::NORMAL`.
    pub const NORMAL: Rules = Rules { start_level: START_LEVEL, start_hand: START_HAND, capsule_respawn_ticks: CAPSULE_RESPAWN_TICKS, set: RuleSet::STANDARD, capsules: true, infinite_uses: false, fixed_hand: None };
}

impl Default for Rules {
    fn default() -> Self {
        Rules::NORMAL
    }
}


/// Where a cast turns the body to, captured when the button goes down
/// (`FUN_140258180`, stored at `+0x88`) and followed at the character's turn
/// rate on every tick of the cast (`FUN_14025bd40`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// The facing is left alone (erase / status / special / environment
    /// casts, mines, holes: `(s43 & 0xfffd) != 0` or motions 8 / 16 / 4-2).
    Keep,
    /// A fixed direction: the bearing of the locked target at the press, or
    /// the camera's yaw when nothing is locked (and always for a defence).
    Yaw(f32),
    /// A short-range attack's locked target, re-read every tick of its
    /// wind-up and dash (`FUN_14025be40`).
    Track(usize),
}

/// An instant effect on the opponent that has been fired and is counting down
/// (`FUN_140265ea0` stores the countdown, `FUN_140265f50` applies the effect when it is
/// spent). `due` is in sim ticks: s65 program ticks x `SIM_TICKS_PER_TABLE_TICK`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DelayedEffect {
    due: u16,
    caster: usize,
    target: usize,
    skill: i16,
    /// The button the skill was cast from (the effect modules read it).
    button: usize,
    everyone: bool,
}

/// What a cast owes when it fires: worked out at the press from the skill on
/// the button, paid at the fire tick (`FUN_1402626d0` -> `FUN_1402621f0`).
/// A hit before the fire tick drops it: the cast never pays.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bill {
    slot: usize,
    /// What the button held at the press (uses and Retain belong to it).
    button_skill: i16,
    /// What is cast (the copied skill for Recollection / Copy).
    id: i16,
    cost: i32,
    level_penalty: i32,
    health_cost: i32,
    uses: Option<u8>,
    retained: bool,
    attack: bool,
    auto_capture: bool,
    finisher: bool,
    /// Times the button had been used before this cast (a defence's DEF is
    /// s47 + s48 x this, `FUN_140272400`).
    spent_before: u8,
    /// A mine or Relearn: the use is not taken at the fire tick
    /// (`FUN_1402626d0` skips `FUN_140263410` for motion 8 and s43/s44 =
    /// 4/0x13): a mine is used up when it goes off (`FUN_14026cd70` code 8 ->
    /// `FUN_1402635b0`), Relearn's own effect takes it.
    defer_use: bool,
}

#[derive(Clone, Debug)]
pub struct Fighter {
    pub health: i32,
    /// Most Health this fighter can have (the rule's per-fighter number).
    pub max_health: i32,
    pub level: i32,
    /// The rule's Level range (`FUN_14024d970`): Level is kept inside it.
    pub level_min: i32,
    pub level_max: i32,
    /// Aura in 1/`AURA_TICKS_PER_POINT` points (`aura()` = whole points).
    pub aura_ticks: i32,
    /// Fractional aura recovery carried between refills (units).
    aura_frac: f32,
    pub slots: [Option<i16>; SLOTS],
    /// Times the skill on each button has been used since it went there
    /// (s26 uses, s24 cost per use). Reset by `set_slot`.
    pub spent: [u8; SLOTS],
    /// Ticks left of each button's "not enough Aura" flash.
    pub denied_ticks: [u8; SLOTS],
    /// Skills still to come out as capsules; the next one is last.
    pub arsenal: Vec<i16>,
    pub action: Action,
    pub action_ticks: u16,
    /// Skill of the current/last cast, and a counter that changes on every
    /// cast so the renderer can restart the animation.
    pub cast_skill: i16,
    pub cast_seq: u16,
    pub guard: Option<ActiveGuard>,
    /// "Freezes target's skill buttons": presses do nothing while > 0.
    pub frozen_ticks: u16,
    /// Ticks left of the Aura Particle use's green glow (`AURA_GLOW_TICKS` at the press,
    /// counting down every tick; a hit does not clear it). Render-only: nothing in the
    /// sim reads it, and it is left out of the state hash.
    pub absorb_glow_ticks: u16,
    /// "target's button presses cause 1 damage" while > 0.
    pub poison_ticks: u16,
    /// Attribute changes and timed conditions (moves/effects.rs).
    pub status: Status,
    /// Skills used up this round, oldest first ("consumed" skills: Recall, Relearn...).
    pub used: Vec<i16>,
    /// Capsules appear around here instead of around you (Call).
    pub capsule_anchor: Option<Vec3>,
    /// The arsenal's Aura recovery multiplier (1.3 / 1.0 / 0.7 by declared
    /// number of schools, `arsenal::aura_handicap`; `FUN_14024de20`).
    pub aura_handicap: f32,
    /// Last attack skill this player used, and the last one that hurt them
    /// (Recollection: "Duplicates last attack skill you used"; Copy: "...
    /// last attack skill that inflicted damage on you").
    pub last_attack: Option<i16>,
    pub last_hurt_by: Option<i16>,
    /// The skill button (slot) of this player's last cast ("same button" skills).
    pub last_slot: usize,
    /// A projectile skill being cast: it leaves when the cast ends (the
    /// charge-up plays first, as in the original). Lost if the cast is interrupted.
    pending_shot: Option<i16>,
    /// What the cast in progress pays when it fires (`Bill`).
    bill: Option<Bill>,
    /// Where the body turns during the cast, and whether that aim is a
    /// locked target (shots home on it; an unlocked shot flies straight).
    pub aim: Aim,
    aim_locked: bool,
    /// The fire matrix's pitch captured at the press (`moves::Launch::pitch`; 0 unless `aim_locked`).
    fire_pitch: f32,
    /// The cast began in mid-air: the body hangs there until it ends (state 11).
    pub air_cast: bool,
    /// "Hold button to charge": Aura the charge will cost at release.
    pub charge: Option<Charge>,
    /// STR fixed when the skill was used (a charge's Aura spent, Finisher's
    /// double Level before its penalty); carried by what it launches.
    cast_strength: Option<i32>,
    spawn_timer: u16,
    /// Ticks until each stage capsule spot may get a new capsule.
    spot_wait: [u16; MAX_CAPSULES_PER_PLAYER],
    prev_buttons: u8,
    /// Sim ticks left until `pending_shot` fires (from the action row's fire
    /// tick, set at `cast`); independent of `action_ticks`, which now counts
    /// down the whole cast clip instead of doubling as the fire timer.
    fire_ticks: u16,
    /// Sim ticks elapsed since the current cast/lunge began (drives the
    /// short-range dash/hit windows read from the action row in `lunge`).
    cast_elapsed: u16,
    /// Combos (READ `FUN_14024fb90`, `FUN_14025a3e0`): the chain counter (player `+0x12f30`: 0 on an
    /// ordinary cast, +1 per chained cast), the buffered button (`+0x61f4`; -1 = none), the press
    /// seen since the last program tick, and the sim ticks since this cast began (the clip frame
    /// is `cast_clock / 2`: the program's clip clock runs 1 frame per program tick).
    pub chain_count: u8,
    chain_buffer: Option<u8>,
    chain_press: Option<u8>,
    cast_clock: u16,
    /// This short-range cast may dash: the target was locked on and within
    /// `MELEE_DASH_RANGE` when the button was pressed (or released, for a
    /// charge) -- PROGRAM-RE.md "Lock-on".
    dash_eligible: bool,
    /// This short-range cast has already landed its one hit.
    struck: bool,
    /// The current cast is short-range (dash + windowed strike, `lunge`),
    /// including while its clip plays out as `Action::Cast` after the fire
    /// tick -- `tick_fighter` keeps routing it to `lunge` until the clip ends.
    melee: bool,
    /// Damage taken in the last `DAMAGE_WINDOW_TICKS` (3 s), this hit not
    /// included -- `Combat::recent_damage` picks light/medium/heavy by it
    /// (PROGRAM-RE.md "Damage").
    pub damage_accum: i32,
    last_hit_tick: Option<u32>,
    /// The hit-reaction row now playing (0 = none yet): `main.rs`'s
    /// `reaction_clip`/`lying_clip`/`getup_clip`, and the victim's voice
    /// group in `sfx.rs`.
    pub reaction_row: i16,
    /// Which side the current/last reaction arrived from.
    pub hit_dir: HitDir,
    /// Knock-down flight: current velocity while still airborne
    /// (`step_knockdown_flight`); horizontal-only and decaying once landed
    /// (the ground slide).
    knock_vel: Vec3,
    /// Still airborne from a knock-down launch (`Motion::Blown` while this
    /// holds; `Motion::Floored`/`GetUp` once it clears).
    pub flying: bool,
    /// The launch speed of the knock-down now playing (0 = none, or no
    /// reaction table loaded): picks the landed pose's size (`lying_clip`).
    last_launch_speed: i16,
    /// Highest point of the knock-down flight in progress (the fall height the
    /// landing damage reads, `0x12e7c`).
    flight_peak: Option<f32>,
    /// Blinking and untouchable, set the moment getting up starts and
    /// running past it (PROGRAM-RE.md "Knock-down physics"): sim ticks left,
    /// decremented every tick.
    pub invuln_ticks: u16,
    /// The round tick this player was knocked out on (`None` while standing):
    /// the survival score the ranking breaks ties with (`Combat::placements`;
    /// the program keeps a per-player end tick, -1 while alive, `+0x68`).
    pub ko_tick: Option<u32>,
    /// A short-range dash's step for the program tick under way (units), walked
    /// half on each of its two sim ticks (`lunge`).
    dash_step: f32,
    /// This sim tick walked the first half of a dash's program step: the program moves the whole
    /// step in the fighter's state handler and only then pushes bodies apart (`FUN_140254590` ->
    /// `FUN_140250780`, once per program tick), so `Match::step` holds this fighter's push-apart until
    /// the second half has been walked (`mid_dash_step`).
    dash_mid: bool,
    /// Last sim tick's blade (a sword is swept: this tick's and last tick's).
    last_blade: Option<(Vec3, Vec3)>,
    /// Status 9 (Decoy, add code 40, `FUN_140272bc0` case 0x28): this player
    /// can only lock onto player `.0` for `.1` more sim ticks (the status
    /// value is the decoy's player index + 1; `0x1402543e0` rejects every
    /// other target while it is set).
    pub forced_lock: Option<(usize, u16)>,
    /// The hover on this fighter came from a status (Skywalk, Demon's Wing:
    /// status 11), which a hit ends; Levitate and Lift Up are not status 11.
    pub status_hover: bool,
    /// Counts the hit reactions started on this fighter: a second hit that
    /// plays the same clip restarts it (the renderer keys the clip on this).
    pub hit_seq: u16,
    /// The last hit arrived within 90 deg of the facing: the front/back split
    /// of the two-way reactions (`hit_front`; `hit_dir` is the four-way one).
    pub hit_front: bool,
    /// How long the lying part of this knock-down lasts, sim ticks: a flyer's
    /// `KNOCK_LYING_TICKS`, a collapse's `COLLAPSE_LIE_TICKS`.
    lie_ticks: u16,
    /// The knock-down is a kind-1 collapse / KO fall: `lie_pose` is the pose
    /// it lies in and the collapse clip plays for `action_ticks - lie - getup`.
    lie_from_collapse: bool,
    /// The DOWN_* clip id a collapse lies in.
    lie_pose: u8,
    /// Fall-height class of the landing (0 < 40 u, 1 up to 80 u, 2 above).
    pub land_class: u8,
    /// The body is in the program's "front set" (`program_front_set`).
    front_set: bool,
    /// Sim ticks since the body touched down: the landing clip plays first,
    /// then the DOWN_* loop queued behind it.
    lying_elapsed: u16,
    /// Yaw the body takes at the start of the next tick: the program writes
    /// the facing in its reaction starts (`0x140258690`) and wall hits.
    pending_turn: Option<f32>,
    /// The body turns half a turn when the DOWN clip starts (`landing_flips`).
    land_flip: bool,
    /// Wall bounces left in this flight and the cool-down before the next.
    wall_hits_left: u8,
    wall_cooldown: u16,
    /// A wall hit in flight: the WALLHIT clip's id (56 front set / 58 back
    /// set) and the ticks since; its `_LOOP` follows the clip.
    pub wall_hit: Option<(u8, u16)>,
    /// Ticks the flight has hung at one height, and that height.
    stuck_ticks: u16,
    stuck_y: f32,
    /// Knocked out, but the killing blow's reaction is still playing: the body
    /// still flies, lands and lies (`Action::Down` is the KO marker everything
    /// else reads; this only keeps the presentation going).
    pub ko_play: bool,
    /// A KO by a plain stagger: ticks of that stagger left before the fighter
    /// collapses (row 25).
    ko_stagger: u16,
    /// A second reaction row (s60) chained behind a stagger (`0x14025c1a0`).
    queued_reaction: i16,
    /// The ground slide advances on every second sim tick (30 Hz).
    slide_odd: bool,
    /// Which stage capsule spots have had their first (opening) capsule this round: a later fill is a respawn (`CapsuleAppeared::respawn`).
    spot_opened: u8,
    // ---- Appended by the reactions2 lane (reaction kinds 7/8/9, struggle, root motion).
    /// Kind 9 (lift): the launch speed (u/s) the body takes on the next
    /// `fighter_step` (the reaction starts where no `Player` is at hand).
    lift_launch: Option<f32>,
    /// Kind 9: the reaction state (14) lasts until the body is back on the
    /// ground (`0x14025c1a0` only goes idle when grounded, `+0x12e74 != 0`).
    lift_hold: bool,
    /// Kind 7 (pinned): sim ticks of the stamp clip still to play inside the lying time.
    stamp_left: u16,
    /// The paralysed struggle (state 20, `DMG_PARARISE`) is what `Action::Hit` is playing.
    pub struggle: bool,
    /// The clip whose root motion moves the body now and the frame it is at
    /// (`Combat::root_step`; `None` = nothing moves it).
    root: Option<(&'static str, f32)>,
    // ---- Appended by the livefix lane.
    /// The `cast_seq` of the last cast an automatic Shelter started by itself (`auto_guard`): that
    /// cast plays no cast sound and no voice (`FUN_14026f530` -> `FUN_1402626d0` starts the clip
    /// directly, without `FUN_14025a3e0`'s `FUN_140276330` calls). Compared with `cast_seq` by sfx.rs.
    pub auto_cast_seq: u16,
}

/// A skill being charged (Gungnir, Ki Cannon, Ki Palm, Young White Dragon).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charge {
    /// The raw button held (0 = forced cast: nothing to hold).
    bit: u8,
    pub spent: i32,
    /// Sim ticks the button has been held (the wind-up counts too).
    ticks: u16,
    /// Program ticks the charge has grown (`+0x80`): one per program tick
    /// after the wind-up while the button is held and the next point is
    /// affordable; `count / 10` Aura is added to the cost at release.
    count: u16,
}

impl Fighter {
    fn new(arsenal: Vec<i16>, level: i32) -> Self {
        Fighter {
            health: START_HEALTH,
            max_health: START_HEALTH,
            level,
            level_min: 0,
            level_max: MAX_LEVEL,
            aura_ticks: level * AURA_TICKS_PER_POINT,
            aura_frac: 0.0,
            slots: [None; SLOTS],
            spent: [0; SLOTS],
            denied_ticks: [0; SLOTS],
            arsenal,
            action: Action::Ready,
            action_ticks: 0,
            cast_skill: AURA_PARTICLE,
            cast_seq: 0,
            guard: None,
            frozen_ticks: 0,
            absorb_glow_ticks: 0,
            poison_ticks: 0,
            status: Status::default(),
            used: Vec::new(),
            capsule_anchor: None,
            aura_handicap: 1.0,
            last_attack: None,
            last_hurt_by: None,
            last_slot: 0,
            pending_shot: None,
            bill: None,
            aim: Aim::Keep,
            aim_locked: false,
            fire_pitch: 0.0,
            air_cast: false,
            charge: None,
            cast_strength: None,
            spawn_timer: 0,
            spot_wait: [0; MAX_CAPSULES_PER_PLAYER],
            prev_buttons: 0,
            fire_ticks: 0,
            cast_elapsed: 0,
            chain_count: 0,
            chain_buffer: None,
            chain_press: None,
            cast_clock: 0,
            dash_eligible: false,
            struck: false,
            melee: false,
            damage_accum: 0,
            last_hit_tick: None,
            reaction_row: 0,
            hit_dir: HitDir::Front,
            knock_vel: Vec3::ZERO,
            flying: false,
            last_launch_speed: 0,
            flight_peak: None,
            invuln_ticks: 0,
            ko_tick: None,
            dash_step: 0.0,
            dash_mid: false,
            last_blade: None,
            forced_lock: None,
            status_hover: false,
            hit_seq: 0,
            hit_front: true,
            lie_ticks: KNOCK_LYING_TICKS,
            lie_from_collapse: false,
            lie_pose: 60,
            land_class: 0,
            front_set: true,
            lying_elapsed: 0,
            pending_turn: None,
            land_flip: false,
            wall_hits_left: 0,
            wall_cooldown: 0,
            wall_hit: None,
            stuck_ticks: 0,
            stuck_y: 0.0,
            ko_play: false,
            ko_stagger: 0,
            queued_reaction: 0,
            slide_odd: false,
            spot_opened: 0,
            lift_launch: None,
            lift_hold: false,
            stamp_left: 0,
            struggle: false,
            root: None,
            auto_cast_seq: 0,
        }
    }

    /// The collapse clip of a kind-1 reaction is still playing (before the
    /// lying part): `Motion::Hit` shows the row's own clip for it.
    pub fn collapsing(&self) -> bool {
        self.lie_from_collapse && (self.action_ticks > self.lie_ticks + KNOCK_GETUP_TICKS || self.stamp_left > 0)
    }

    /// A KO'd fighter still staggering from the killing blow (a plain reaction).
    pub fn ko_staggering(&self) -> bool {
        self.action == Action::Down && self.ko_play && self.ko_stagger > 0
    }

    /// Void ("Immunity to attack damage") or Invulnerability ("Immune to
    /// damage while immobile") stops this attack.
    pub fn status_blocks(&self, def: &SkillDef) -> bool {
        def.capsule == Capsule::Attack && (self.status.immune_ticks > 0 || (self.status.still_immune && self.status.still_ticks > 1))
    }

    /// Puts `skill` on button `k` (or empties it), fresh: no uses spent yet.
    pub fn set_slot(&mut self, k: usize, skill: Option<i16>) {
        self.slots[k] = skill;
        self.spent[k] = 0;
    }

    /// Whole aura points available to spend.
    pub fn aura(&self) -> i32 {
        self.aura_ticks / AURA_TICKS_PER_POINT
    }

    /// Movement input is ignored while casting, dashing, stunned, down,
    /// bound, or consuming an aura / green status capsule.
    pub fn rooted(&self) -> bool {
        self.action != Action::Ready || self.status.bound_ticks > 0
    }

    /// Sim ticks since the last Aura Particle use (`AURA_GLOW_TICKS` of them), `None`
    /// otherwise. Read-only, for the renderer's green body glow (`aura_pulse.rs`); it runs
    /// on its own counter, so it does not depend on the hold or on a hit.
    pub fn absorb_age(&self) -> Option<u16> {
        (self.absorb_glow_ticks > 0 && self.absorb_glow_ticks <= AURA_GLOW_TICKS).then(|| AURA_GLOW_TICKS - self.absorb_glow_ticks)
    }
}

/// A stage capsule spot waiting to regenerate (`Combat::pending_capsules`):
/// `elapsed` of `wait` sim ticks have passed since its capsule left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingCapsule {
    pub owner: usize,
    /// Which of the owner's three spots (the program yaws the capsule's effects by this x 90 deg).
    pub spot: usize,
    pub pos: Vec3,
    pub elapsed: u16,
    pub wait: u16,
}

#[derive(Clone, Debug)]
pub struct CapsuleItem {
    pub id: u32,
    pub owner: usize,
    pub skill: i16,
    pub pos: Vec3,
    /// Sim ticks left before it can be picked up: a new capsule appears
    /// (sound, fade-in) for `CAPSULE_APPEAR_TICKS` first (`FUN_14014ed10`
    /// state 1). 0 = pickable.
    pub appear: u16,
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub id: u32,
    pub owner: usize,
    /// The player it flies at (the owner's aim target when it was launched):
    /// homing steers to them, a defence field of theirs can turn it.
    pub target: usize,
    pub skill: i16,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    shot: Shot,
    /// Already turned by an orbit-changing guard or sent back by a Reflect.
    turned: bool,
    /// Bent aside by a [Course] guard: flies on but hurts nobody.
    pub harmless: bool,
    /// The button its caster pressed: "same button" effects (Hungry Demon,
    /// Trigger, Freeze Shot) use this one, not whatever was pressed since.
    button: usize,
    /// A stage object thrown by Psycho Kinesis / Power (index into `Combat::props`).
    pub prop: Option<usize>,
    /// Brushed aside by a [Brush] guard: hurts nobody, but "anything in the
    /// way of the brushed shell will be destroyed" (stage objects).
    pub brushed: bool,
    /// STR fixed when it was launched (a charge, Finisher, a thrown object's size).
    pub strength: Option<i32>,
}

impl Projectile {
    /// Half of the program step the shot last made. The sim moves a shot one
    /// program step every second tick and keeps `pos` at the middle of that
    /// step on the tick it moves (`move_projectiles`), so `pos` trails the
    /// program's position by this much: the renderer draws at `pos + lead()`.
    pub fn lead(&self) -> Vec3 {
        (self.shot.pos - self.shot.prev) * 0.5
    }

    /// The flight state, read-only (the CPU's threat predictor, `cpu/world.rs`).
    pub fn shot(&self) -> &Shot {
        &self.shot
    }
}

/// How a hit was delivered: the caster's button at cast time, and a thrown
/// object's own STR ("STR depends on object") in place of the skill's.
#[derive(Clone, Copy, Debug)]
struct Via {
    button: usize,
    strength: Option<i32>,
}

/// An attack set at a place that goes off later (moves/area.rs): a Hole or
/// Rain column's warning and active ticks, a Rain cloud and its drops, a
/// mine, Photon Wave. Times count program ticks (1/30 s): it steps on every
/// second sim tick from the tick it was set.
#[derive(Clone, Debug)]
pub struct Area {
    pub id: u32,
    pub owner: usize,
    /// The player it was set for (the owner's aim target when it was placed):
    /// the Hole's marker slides with them, the cloud follows them.
    pub target: usize,
    pub skill: i16,
    /// On the floor (a Rain cloud: the floor point under the cloud).
    pub pos: Vec3,
    pub spec: AreaSpec,
    /// A mine's fuse (program ticks left), a cloud's wait for its next drop.
    pub timer: u16,
    pub pulses_left: u16,
    /// Sensor/remote mines: set and waiting for their trigger.
    pub armed: bool,
    /// Program ticks of life left (Hole, Rain column and cloud; a mine never fizzles).
    life: u16,
    /// Its owner pressed a button to set off this remote mine.
    detonate: bool,
    /// Players it has already struck (bit = slot; Rain lands once per cast).
    hit: u8,
    /// Sim ticks since it was set.
    age: u16,
    /// Its first program tick has passed (`FUN_1402638d0` only arms a new object: state 1 -> 2): nothing counts
    /// down or is tested on the tick it was set (LIVE Sensor Mine: set at 43, goes off at 103 = s66 60 later).
    settled: bool,
    /// A marker's slide along the floor, per program tick (the target's run).
    slide: Vec3,
    /// The STR fixed when the cast fired (one roll for every hole of the cast), in place of the skill's own.
    strength: Option<i32>,
    /// A Rain cloud's position and drops in the air, and its drop counters
    /// (`+0x1d0`, `+0x1d4`: ticks to the first drop, ticks to the next).
    pub cloud: Vec3,
    drops: Vec<RainDrop>,
    first: i16,
    gap: i16,
}

/// One Rain drop (`FUN_14026b8d0`): a straight, fast object of 5 u radius.
#[derive(Clone, Debug)]
struct RainDrop {
    prev: Vec3,
    pos: Vec3,
    dir: Vec3,
    life: u16,
}

impl Area {
    /// Still showing where it will land (nothing has gone off yet).
    pub fn warning(&self) -> bool {
        match self.spec.kind {
            Kind::Hole | Kind::RainColumn => self.life >= self.spec.active,
            Kind::RainCloud => self.pulses_left == self.spec.pulses && self.first >= 0,
            _ => false,
        }
    }
}

/// A blast (`FUN_14026dbd0`): lives 30 program ticks, its sphere growing from
/// nothing to `radius` over them, a body test every tick, each player once.
#[derive(Clone, Debug)]
pub struct Blast {
    pub owner: usize,
    pub skill: i16,
    pub pos: Vec3,
    /// The full radius.
    pub radius: f32,
    /// Program ticks left (30 at first).
    pub life: u16,
    /// Players it can still hit (bit = slot).
    mask: u8,
    /// An area's blast (mines, Photon Wave) or a shot's (both strike as `Arrival::Blast`).
    area: bool,
    via: Via,
    age: u16,
    /// Its first program tick has passed: the object that turns into the blast (`FUN_14026dbd0`, state 3,
    /// life 30) is updated for the first time on the NEXT program tick, so nothing is tested on the
    /// tick it was made (radius 0 there anyway; the growth starts a tick later).
    armed: bool,
}

/// Something the renderer should show once (hit sparks, a barrier block).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    /// `attacker` used `skill` on `victim` (so sound, fx and stats need not
    /// guess who did it).
    Hit { attacker: usize, victim: usize, skill: i16 },
    /// `broken`: the attack was stronger than the defence and destroyed it.
    Blocked { attacker: usize, victim: usize, skill: i16, broken: bool },
    /// A shot exploded ("Explodes on impact") or an area went off here;
    /// `attacker` owns it.
    Burst { attacker: usize, pos: Vec3, skill: i16 },
    /// Player `who` pressed `slot` without enough Aura: nothing is cast
    /// (the HUD flashes the cost, a short "no" sound plays).
    Denied { who: usize, slot: usize },
    /// Player `who` captured a capsule into `slot`. The original plays common/86
    /// here (`FUN_140261a00`, `0x140261bc6`; audits/P0-SOUND.md). A capture holds nobody
    /// (it changes no state in the program).
    Capsule { who: usize, slot: usize, skill: i16 },
    // ---- Appended by the EVENTS lane (sound + effect call sites; none of these
    // changes a rule, they only say that something happened). Each cites the
    // program address the sound / effect is played from.
    /// A capsule orb of `who` was created at `pos` (the start of its 30-tick
    /// appearing phase, `FUN_14014ed10` state 0 -> 1, sound common/85 at
    /// `0x14014ed6f`). `respawn` is false for the opening fill of a round
    /// (`FUN_14014ee70` writes state 2 directly: silent).
    CapsuleAppeared { who: usize, pos: Vec3, respawn: bool },
    /// A skill erased one of `who`'s capsules (`FUN_14014fd10` mode 1: effect
    /// cap04 + common/92 at `0x14014edc7` / `0x14014ff07`), one event per capsule.
    CapsuleErased { who: usize, pos: Vec3, skill: i16 },
    /// The environment crystal of `who`'s skill was set at `pos` (`FUN_14017bcc0`,
    /// common/93 at `0x14017bdc9`; the common/87 hum loop starts with it).
    CrystalSet { who: usize, pos: Vec3 },
    /// `who` took the environment crystal at `pos` (`FUN_14017be00`, common/88
    /// at `0x14017bf70`).
    CrystalTaken { who: usize, pos: Vec3 },
    /// The standing environment crystal ended (erased, timed out; not taken):
    /// `FUN_14017ba00`, common/94 at `0x14017bb6d`.
    EnvironmentEnded { pos: Vec3 },
    /// Player `who`'s Aura rose past a whole number by regeneration
    /// (`FUN_14024de20`, common/5 at `0x14024df1d`).
    AuraTick { who: usize },
    /// A knock-down flight of `who` touched down (state 0x11 ends,
    /// `FUN_140254c80`, common/82 at `0x140254e3f`).
    KnockLanded { who: usize },
    /// `who` began to get up from a knock-down (`FUN_14025c3d0`, common/81 at
    /// `0x14025c4ee`).
    GotUp { who: usize },
    /// An attack of `attacker` erased a button of `victim` on hit
    /// (`FUN_14026f870`, common/91 at `0x14026fb8e`).
    ButtonErased { attacker: usize, victim: usize, skill: i16 },
    /// `who`'s cast of `skill` reached its fire tick and was paid for
    /// (`FUN_1402626d0`; the s19 sound is played here, `0x140262caa`).
    SkillFired { who: usize, skill: i16 },
    /// `who` was knocked out (Health 0): the KO voice.
    Knockout { who: usize },
    /// A blast started (`FUN_14026dbd0`: common/99 at `0x14026dca4`) with this
    /// full radius in world units (the impact effect is scaled by radius x 0.2,
    /// `FUN_14026cd70`); `area`: a mine / Photon Wave blast rather than a shot's.
    BlastStarted { attacker: usize, pos: Vec3, skill: i16, radius: f32, area: bool },
    /// A status-capsule effect (or the Aura Particle's Level +1) took hold on `who`:
    /// `FUN_140270b00` ends by calling `FUN_140271590(fighter, kind, amount)`, which plays
    /// common/89-90 (Level), 95-96 (stats), 97, 98 (`0x14027161f`). `kind` = s45,
    /// `amount` = s46.
    StatusEffect { who: usize, kind: i16, amount: i32 },
    /// `who` used an Aura Particle (logged with the `StatusEffect` Level +1 at the press):
    /// the footage's second sound (common/90, `sfx.rs`).
    AuraParticleUsed { who: usize },
    /// A charge skill reached the row's charge-cycle tick and is held (`FUN_140259540`:
    /// the slot-4 charge effect starts, twice, on the caster).
    ChargeHeld { who: usize, skill: i16 },
    /// The charge button was released (`FUN_140259670` ends the slot-4 loops,
    /// `FUN_140259140` plays slot 5 on the caster).
    ChargeReleased { who: usize, skill: i16 },
    /// A Rain cloud let a drop fall from `pos` (`FUN_14026b8d0`: slot 4 at the drop's start).
    RainDrop { attacker: usize, pos: Vec3, skill: i16 },
    // ---- Appended by the SOUNDEVENTS lane (sfx.rs only; none changes a rule).
    /// A skill jump (Jump, Levitate, Skywalk, Demon's Wing) left the ground at the end of its
    /// JUMP_START wind-up: `FUN_14025a0b0` calls `FUN_1402549e0`, which plays the take-off sound
    /// of the floor material (common/61 for material 2, 3, 5, `0x1402549e0`).
    JumpTakeoff { who: usize },
    /// `who`'s button `slot` was filled by `FUN_140261a00` with its sound flag set (common/86,
    /// `0x140261bc6`, only while `who` is a playable fighter): Auto-Capture (Empty Hand, Hyperpunch,
    /// Sky Pursuit), Relearn, and each skill an Annoying Gift forces onto its target. NOT the
    /// capsule pickup (that is `Capsule`, which the replay golden hashes).
    SkillCaptured { who: usize, slot: usize, skill: i16 },
    /// An "Add" effect (s51 code, s52 amount) took hold on `who`: `FUN_140272bc0` ends with
    /// `FUN_1402738d0(victim, code, amount)`, which plays common/89-90 (Level), 95-96 (stats), 97, 98
    /// for the codes it knows (`sfx::add_effect_sound`). Not the s43 = 4 kind list (`StatusEffect`).
    AddEffect { who: usize, code: i16, amount: i32 },
    // ---- Appended by the PROPSOUND lane (sfx.rs only; none changes a rule).
    /// `who` touched down from the airborne state (a jump, a fall off a ledge, the end of a
    /// float or dash) by `fall` units (highest point reached minus the landing point):
    /// `FUN_140254c80`'s state-5 branch, which plays the floor material's landing sound at its end
    /// (`FUN_140254b30`) and, for a fall over 80 units, sends a shock into the stage objects
    /// (`FUN_140163b50`, whose contacts play the surface-hit sound). A knock-down landing is
    /// `KnockLanded`, not this.
    Landed { who: usize, fall: f32 },
    // ---- Appended by the gameplay-accuracy lane (sfx.rs only; no rule reads it).
    /// Shot `id` ran out of life without meeting anything: `FUN_14026e720` ends it with impact code 0,
    /// which `FUN_14026cd70` sends to its default case (state 0, no effect, no s21 sound). The sound layer
    /// must not voice it as a wall / floor end (code 1).
    ShotExpired { id: u32, skill: i16 },
}

/// How long a button's cost flashes after a press it couldn't pay for (0.4 s).
pub const DENIED_FLASH_TICKS: u8 = 24;

/// How many recent events are kept for the renderer to catch up on.
pub const EVENT_LOG: usize = 64;

/// The window the replay goldens hash (the log held 16 before the events lane
/// appended its variants): the hash sees only the original variants, through
/// `Combat::legacy_events`, so recording a sound cue never moves a golden.
#[cfg(test)]
const LEGACY_EVENT_LOG: usize = 16;

#[derive(Clone, Debug)]
pub struct Combat {
    pub skills: Vec<SkillDef>,
    /// Each player's 30-skill arsenal (arsenal.rs rules); reshuffled every round.
    pub arsenals: [Vec<i16>; MAX_PLAYERS],
    /// Who is in the match and on whose team (slots that are not active are
    /// never stepped).
    pub roster: Roster,
    /// Whether attacks can hurt the attacker's teammates (`FriendlyFire`;
    /// UNCONFIRMED, default = the original as far as read).
    pub friendly_fire: FriendlyFire,
    /// Option `DAT_14052dfb9` of the program: teammates can be locked onto
    /// (`0x1402543e0`). Its default is unread (audit R1b); off here.
    pub friendly_lock: bool,
    /// Battle time limit in ticks (0 = none); `round_ticks` counts it after the countdown.
    pub time_limit: u32,
    /// Prop Hunt (`prophunt.rs`): a mode outside `Combat` decides when rounds end and who scores, so a knock-out
    /// never ends the round here (`check_round_end`) and neither does the time limit. Default off: every other match
    /// is untouched. Set once when the match starts; the mode's own state (in `Match::prophunt`, which
    /// `Match::checksum` hashes) carries it, so it is left out of `hash_state` and a normal match hashes as before.
    pub rounds_by_mode: bool,
    /// Opening Level, starting skills, capsule respawn (`set_rules`).
    pub rules: Rules,
    pub fighters: [Fighter; MAX_PLAYERS],
    pub capsules: Vec<CapsuleItem>,
    pub projectiles: Vec<Projectile>,
    pub areas: Vec<Area>,
    /// Blasts going on (shots that exploded, mines, Photon Wave).
    pub blasts: Vec<Blast>,
    /// The environment skill in force, if any (moves/effects.rs).
    pub crystal: Option<Crystal>,
    /// Each player's fixed capsule spawn spots from the stage layout
    /// (`stNN.props.json`: 3 per start point). Empty = spawn around the player.
    pub capsule_spots: [Vec<Vec3>; MAX_PLAYERS],
    /// The stage's objects (props.rs); back as placed every round.
    pub props: Vec<Prop>,
    /// Non-attack skills that landed this tick: (caster, target, skill); they
    /// take effect after movement (they may move players).
    pending_effects: Vec<(usize, usize, i16, usize)>,
    /// Instant effects on the opponent (s43 = 0, motion 4) counting down to their landing:
    /// `FUN_140265ea0` stores s65 program ticks in the object, `FUN_140265f50` applies the
    /// effect when the count has run out. Appended to by `take_effect`, drained in `step`.
    delayed_effects: Vec<DelayedEffect>,
    /// Casts the program fires in the press itself (the row names clip 0: no cast state,
    /// `0x140258180` calls `0x1402626d0` at once); `cast_from` has no collision at hand, so it
    /// queues them and the caller of the press (`tick_fighter` / `step`) fires them.
    instant_fires: Vec<(usize, i16)>,
    pub round: u32,
    pub wins: [u32; MAX_PLAYERS],
    /// Set when the round is decided (at most one team left standing, or time
    /// up): the winning slot (`DRAW` = nobody) and the ticks until the next
    /// round starts.
    pub round_over: Option<(usize, u16)>,
    /// The round tick the round was decided on by a knock-out; a last standing
    /// player knocked out within `SIMULTANEOUS_KO_TICKS` of it makes it a draw.
    round_decided_tick: Option<u32>,
    /// Each player's declared "Number of Schools" (`deck + 0x50`), when the
    /// arsenal has one; `None` = worked out from its contents.
    arsenal_kinds: [Option<u8>; MAX_PLAYERS],
    /// Ticks since the round started; nobody can move or cast during the
    /// first `COUNTDOWN_TICKS` (the original's "3, 2, 1, START").
    pub round_ticks: u32,
    /// The round ended on time (the result banner says TIME UP; winner may be none).
    pub time_up: bool,
    /// Which players are off the ground this tick (a hit in mid-air knocks down).
    pub airborne: [bool; MAX_PLAYERS],
    /// Test runs only: empty slots refill with these skills and aura stays
    /// full (still deterministic sim state).
    pub test_loadout: Option<([i16; SLOTS], i32)>,
    /// TEST ONLY (`PD_TEST_HEALTH`): everyone's Health is held at or below this, so a
    /// scripted capture can reach a knock-out with one hit.
    pub test_health: Option<i32>,
    /// Recent events, each with a number that only goes up (renderer keeps
    /// the last one it showed).
    pub events: Vec<(u32, CombatEvent)>,
    event_seq: u32,
    /// TEST ONLY: the original five variants with their own numbering and a
    /// 16-entry window -- what the replay hash reads (see `LEGACY_EVENT_LOG`).
    #[cfg(test)]
    legacy_events: Vec<(u32, CombatEvent)>,
    #[cfg(test)]
    legacy_seq: u32,
    next_id: u32,
    rng: u32,
    /// Each player's clip lengths (name -> sim ticks), read once at startup
    /// from their converted model (`clip_ticks_from_glb`, `set_clip_ticks`):
    /// item 2, "caster held for the whole cast clip". Empty in tests and for
    /// a character not yet converted; `clip_hold_ticks` falls back to
    /// `default_clip_table_ticks` then.
    clip_ticks: [HashMap<String, u16>; MAX_PLAYERS],
    /// Each player's clip root-motion curves (`set_root_curves`, reactions2 lane).
    root_curves: [HashMap<String, Vec<[f32; 2]>>; MAX_PLAYERS],
    /// Each skill's action-table rows (ground s12, mid-air s13), read once at
    /// startup from `skills.json` (`Combat::set_action_rows`): docs/
    /// FORMATS-SKILLS.md "Action table". Empty in tests that don't set one
    /// (`Combat::set_action_row`) for a skill: `action_row` then returns
    /// `None` and its callers fall back to the old flat timing.
    action_rows: HashMap<i16, (Option<ActionRow>, Option<ActionRow>)>,
    /// Combo rows (s14..s17, the leading non-zero ones) by skill id: hit k >= 1 of a chain plays
    /// `rows[(k - 1) % rows.len()]` (`FUN_14025a3e0`), each with its own window. `set_chain_rows`.
    chain_rows: HashMap<i16, Vec<(ActionRow, ChainWindow)>>,
    /// The ground row's (s12) combo window, by skill id (hit 0 of a chain). `set_chain_rows`.
    chain_first: HashMap<i16, ChainWindow>,
    /// The hit-reaction table, keyed by row id, read once at startup from
    /// `skills.json`'s top-level `"reactions"` (`Combat::set_reaction_rows`):
    /// docs/FORMATS-SKILLS.md "Action table and hit-reaction table". Empty in
    /// tests that don't load one: `reaction_is_knockdown`/`reaction_is_pinned`
    /// fall back to the old fixed-id guess, and clip/voice lookups fall back
    /// to a flat default.
    reaction_rows: HashMap<i16, ReactionRow>,
    /// Mines in the field whose use is taken when they go off, not at the fire
    /// tick: (area id, owner, what the cast paid). `settle_mines`.
    pending_mines: Vec<(u32, usize, Bill)>,
    /// Each skill's `SHOTnn` marker offsets for its ground and its mid-air
    /// cast effect (`Combat::set_shot_markers`, from `cache/shotpoints.json`
    /// through the action rows' effect numbers), in the caster's frame.
    /// Empty = no marker data (tests): objects start at the chest.
    shot_markers: HashMap<i16, (Vec<Vec3>, Vec<Vec3>)>,
    /// Each player's right hand through the attack clips (`set_hand_paths`).
    hands: [HashMap<String, HandClip>; MAX_PLAYERS],
}

/// Attack add-effect code (s51) "Add: Auto-Capture effect".
pub const AUTO_CAPTURE: i16 = 18;
/// s45 on a status capsule (s43 = 4): the effect kind `FUN_140270b00` runs.
/// 19 = Relearn (captures the last consumed skill onto its own button); its
/// use is not taken at the fire tick (`FUN_1402626d0`).
pub const RELEARN_KIND: i16 = 19;

/// Finisher: "STR = double your Level" (strength rule 21 keyed by id).
pub const FINISHER: i16 = 74;

/// Attack STR rules (s45 on attack skills; FORMATS-SKILLS.md), each from the
/// skills' own notes.
pub mod str_rule {
    /// "STR = (target's Health - yours) x 2. Effective when target's Health > yours" (Judge of Anubis).
    pub const TARGET_HEALTH_OVER_YOURS: i16 = 1;
    /// "STR = your Health - target's. Effective when your Health > target's" (Joke of Anubis).
    pub const YOUR_HEALTH_OVER_TARGETS: i16 = 2;
    /// "STR = number of consumed attack skills" (Scar of Battles, Memory of Battle).
    pub const CONSUMED_ATTACKS: i16 = 3;
    /// "STR = number of consumed defensive skills" (Compressor).
    pub const CONSUMED_DEFENCES: i16 = 4;
    pub const TARGET_LEVEL: i16 = 17;
    pub const TARGET_AURA: i16 = 18;
    pub const HALF_TARGET_AURA: i16 = 20;
    /// Set when used: Aura spent charging (Gungnir…), Finisher's double Level, a thrown object's size.
    /// (Not read outside tests since `SkillDef::charged` became the record's s37.)
    #[allow(dead_code)]
    pub const AT_CAST: i16 = 21;
    /// "STR = double target's unassigned skill buttons" (Heat from Void, Binding Needle, Vacuum Palm).
    pub const TARGET_EMPTY_BUTTONS: i16 = 23;
    /// "STR = double your unassigned skill buttons" (Sign of Saints).
    pub const YOUR_EMPTY_BUTTONS: i16 = 24;
    /// "STR = STR of all equipped Laser / Blaster / Piece / short-range Ki skills".
    pub const BUNDLED: i16 = 26;
    /// "STR randomly 0, 3, or 6" (Debris Bullet, Jack Pot, Excaliber): 0, STR or 2 x STR.
    pub const RANDOM: i16 = 28;
    /// "STR = total Aura Particles on battlefield" (Overheat).
    pub const AURA_PARTICLES_ON_FIELD: i16 = 30;
}

/// Skill buttons in `Input::skills`: bit k = slot k.
pub fn pressed(buttons: u8, prev: u8, slot: usize) -> bool {
    buttons & !prev & (1 << slot) != 0
}

/// Golden clip lengths (table ticks = frames at our now-30 fps playback),
/// read straight off PROGRAM-RE.md's per-handler timelines: used until a
/// character's own converted model is in the cache (`Combat::set_clip_ticks`)
/// or for a clip that model doesn't have yet. The fallback for anything else
/// keeps the old flat guess (`CAST_TICKS`) so untimed test/placeholder
/// skills don't change behaviour.
fn default_clip_table_ticks(clip: &str) -> i16 {
    match clip {
        "ATK_SPHERE" | "SPE_POWER_UP" => 40,
        "ATK_PUNCH" => 30,
        "ATK_TUKI" => 35,
        "ATK_KICK" => 35,
        "ATK_BODY" => 45,
        "ATK_DRAIN" => 45,
        "ATK_SORD" => 40,
        "ATK_WHAND_SORD" => 54,
        "ATK_LASER" => 34,
        "ATK_SHOT" => 45,
        "ATK_THROW" => 40,
        "ATK_GROUND" => 45,
        "ATK_SPIKE" => 46,
        "ATK_WHAND_SPIKE" => 53,
        "ATK_SET_BOMB" => 22,
        // The start clip of a jump (15 frames in every fighter's files).
        "JUMP_START" => 15,
        // Reaction clips (pc00a's own lengths, `clip_ticks_from_glb`): the
        // fallback for a model whose table is not loaded.
        "BLOW_LAND_SF" | "BLOW_LAND_SB" => 22,
        "BLOW_LAND_MF" | "BLOW_LAND_MB" => 40,
        "BLOW_LAND_LF" => 56,
        "BLOW_LAND_LB" => 60,
        "WALLHIT_FRONT" | "WALLHIT_BACK" => 18,
        "DMG_SHOT_FRONT" | "DMG_SHOT_BACK" | "DMG_SHOT_LEFT" | "DMG_SHOT_RIGHT" => 20,
        "DMG_SPHERE_FRONT" | "DMG_SPHERE_BACK" | "DMG_SORD_FRONT" | "DMG_SORD_BACK" | "DOWN_FRONT_STAND" | "DOWN_BACK_STAND" => 30,
        "DMG_DRAIN_HIGH" => 50,
        "DMG_SPIKE_HIGH" => 40,
        "DMG_RAISE_HIGH" => 39,
        _ => (CAST_TICKS / moves::SIM_TICKS_PER_TABLE_TICK) as i16,
    }
}

/// Reads a character's animation clip lengths (name -> sim ticks) straight
/// from its converted glTF binary: the JSON chunk already carries each
/// animation's name and its time accessor's `max[0]` (seconds), baked at our
/// pipeline's 30 fps (`pd_formats::character`). Pure byte/JSON parsing, no
/// Bevy and no `gltf` crate, so loading this stays deterministic (item 2:
/// "plain data loaded before the match").
pub fn clip_ticks_from_glb(bytes: &[u8]) -> HashMap<String, u16> {
    let mut out = HashMap::new();
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return out;
    }
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let clen = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        let ctype = &bytes[off + 4..off + 8];
        let start = off + 8;
        let end = start.saturating_add(clen).min(bytes.len());
        if ctype == b"JSON" {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes[start..end]) {
                let accessors = v["accessors"].as_array().cloned().unwrap_or_default();
                for anim in v["animations"].as_array().into_iter().flatten() {
                    let Some(name) = anim["name"].as_str() else { continue };
                    let mut end_t = 0.0f64;
                    for s in anim["samplers"].as_array().into_iter().flatten() {
                        let idx = s["input"].as_u64().unwrap_or(u64::MAX) as usize;
                        if let Some(m) = accessors.get(idx).and_then(|a| a["max"][0].as_f64()) {
                            end_t = end_t.max(m);
                        }
                    }
                    if end_t > 0.0 {
                        out.insert(name.to_string(), (end_t * 60.0).round() as u16);
                    }
                }
            }
            break;
        }
        off = end;
    }
    out
}

impl Combat {
    /// The per-tick state, for `Match::checksum`: every field that a tick
    /// can change, in a fixed order. Leaves out the static tables loaded once
    /// from the cache (`skills`, `capsule_spots`, and the `HashMap` lookups,
    /// which are only read by key -- iterating them would also hash in a
    /// different order on every run). Add any new per-tick field here.
    pub fn hash_state(&self, h: &mut impl std::fmt::Write) {
        let _ = write!(
            h,
            "{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}|{:?}|{:?}|{}|{}|{}|{:?}",
            self.arsenals,
            self.roster,
            self.friendly_fire,
            self.friendly_lock,
            self.time_limit,
            self.rules,
            self.fighters,
            self.capsules,
            self.projectiles,
            self.areas,
            self.blasts,
            self.crystal,
            self.props,
            self.pending_effects,
            self.delayed_effects,
            self.instant_fires,
            self.round,
            self.wins,
            self.round_over,
            self.round_decided_tick,
            self.arsenal_kinds,
            self.round_ticks,
            self.time_up,
            self.airborne,
            self.test_loadout,
            self.test_health,
            self.events,
            self.event_seq,
            self.next_id,
            self.rng,
            self.pending_mines,
        );
    }

    /// Both players with the same arsenal (tests, the default test set).
    #[allow(dead_code)] // the game builds every match with `with_roster` now (fourview lane)
    pub fn new(skills: Vec<SkillDef>, arsenal: Vec<i16>) -> Self {
        Self::with_arsenals(skills, [arsenal.clone(), arsenal])
    }

    #[allow(dead_code)] // as `new`
    pub fn with_arsenals(skills: Vec<SkillDef>, arsenals: [Vec<i16>; 2]) -> Self {
        let [a, b] = arsenals;
        Self::with_roster(skills, Roster::duel(), [a, b, Vec::new(), Vec::new()])
    }

    /// A match of any shape: `roster` says which slots play and their teams,
    /// `arsenals` holds each active slot's deck (inactive slots' are ignored).
    pub fn with_roster(mut skills: Vec<SkillDef>, roster: Roster, arsenals: [Vec<i16>; MAX_PLAYERS]) -> Self {
        if !skills.iter().any(|s| s.id == AURA_PARTICLE) {
            skills.push(SkillDef::aura_particle());
        }
        let mut c = Combat {
            skills,
            arsenals,
            roster,
            friendly_fire: FriendlyFire::default(),
            friendly_lock: false,
            time_limit: TIME_LIMIT_TICKS,
            rounds_by_mode: false,
            rules: Rules::default(),
            fighters: std::array::from_fn(|_| Fighter::new(Vec::new(), START_LEVEL)),
            capsules: Vec::new(),
            projectiles: Vec::new(),
            areas: Vec::new(),
            blasts: Vec::new(),
            crystal: None,
            capsule_spots: std::array::from_fn(|_| Vec::new()),
            props: Vec::new(),
            pending_effects: Vec::new(),
            delayed_effects: Vec::new(),
            instant_fires: Vec::new(),
            round: 0,
            wins: [0; MAX_PLAYERS],
            round_over: None,
            round_decided_tick: None,
            arsenal_kinds: [None; MAX_PLAYERS],
            round_ticks: 0,
            time_up: false,
            airborne: [false; MAX_PLAYERS],
            test_loadout: None,
            test_health: None,
            events: Vec::new(),
            event_seq: 0,
            #[cfg(test)]
            legacy_events: Vec::new(),
            #[cfg(test)]
            legacy_seq: 0,
            next_id: 1,
            rng: 0x9E37_79B9,
            clip_ticks: std::array::from_fn(|_| HashMap::new()),
            root_curves: std::array::from_fn(|_| HashMap::new()),
            action_rows: HashMap::new(),
            chain_rows: HashMap::new(),
            chain_first: HashMap::new(),
            reaction_rows: HashMap::new(),
            pending_mines: Vec::new(),
            shot_markers: HashMap::new(),
            hands: std::array::from_fn(|_| HashMap::new()),
        };
        c.start_round();
        c
    }

    /// Changes the rules and restarts the match's first round under them.
    pub fn set_rules(&mut self, rules: Rules) {
        self.rules = rules;
        self.round = 0;
        self.wins = [0; MAX_PLAYERS];
        self.start_round();
    }

    /// Each player's declared "Number of Schools" (1-5, `deck + 0x50`), which
    /// sets their Aura recovery multiplier (`arsenal::aura_handicap`); a slot
    /// without one gets it from its arsenal's contents. Takes effect from the
    /// next round start (`set_rules` restarts the first).
    #[allow(dead_code)] // the setup screen passes each arsenal's declared kind (lane F3 / builder)
    pub fn set_arsenal_kinds(&mut self, kinds: [Option<u8>; MAX_PLAYERS]) {
        self.arsenal_kinds = kinds;
        self.start_round();
    }

    /// Takes the time limit of the rule in force (`RuleSet::time_limit_ticks`)
    /// instead of the setup screen's.
    #[allow(dead_code)] // lane F3
    pub fn use_rule_time_limit(&mut self) {
        self.time_limit = self.rules.set.time_limit_ticks();
    }

    /// Each player's own clip lengths (main.rs at startup, read once from
    /// their converted model with `clip_ticks_from_glb`): item 2, "caster
    /// held for the whole cast clip". A missing name/character keeps using
    /// `default_clip_table_ticks`.
    pub fn set_clip_ticks(&mut self, tables: impl IntoIterator<Item = HashMap<String, u16>>) {
        for (slot, t) in self.clip_ticks.iter_mut().zip(tables) {
            *slot = t;
        }
    }

    /// Each player's fixed capsule spots from the stage layout, by slot
    /// (`stNN.props.json`: 3 per start point); slots with no entry get none.
    pub fn set_capsule_spots(&mut self, spots: impl IntoIterator<Item = Vec<Vec3>>) {
        self.capsule_spots = std::array::from_fn(|_| Vec::new());
        for (slot, s) in self.capsule_spots.iter_mut().zip(spots) {
            *slot = s;
        }
    }

    /// Sim ticks the whole cast clip holds the caster for `clip` (the action
    /// row's `anim_name`; empty when there's no row).
    pub(crate) fn clip_hold_ticks(&self, i: usize, clip: &str) -> u16 {
        self.clip_ticks[i].get(clip).copied().unwrap_or_else(|| moves::table_ticks(default_clip_table_ticks(clip)))
    }

    /// Sim ticks a cast holds the caster (state 10 / 11 / 12) for the action `row`: the row's clip
    /// length in frames plus ONE program tick. READ `FUN_14025a3e0` -> `FUN_140259d10(state, clip
    /// length)` sets the state timer; `FUN_14025bd40` counts it down once per program tick and
    /// leaves the state when it is below 1, so the state lasts length + 1 ticks. LIVE (castpose
    /// lane, `takes_full` traces, pc00a clip lengths): of 330 takes with a row-timed cast, 268 end
    /// exactly press + length + 1 (ATK_WHAND_SPIKE 53 -> 54, SPE_HEAL 40 -> 41, ATK_STAMP, ATK_SORD...;
    /// ground, air and short-range alike); the rest are cut short by a later state (a guard, the
    /// Super Shot recoil, Regenerate-type erase, a hit). A row with no clip keeps the flat guess.
    pub(crate) fn cast_hold_ticks(&self, i: usize, row: Option<&ActionRow>) -> u16 {
        match row.filter(|r| !r.anim_name.is_empty()) {
            Some(r) => self.clip_hold_ticks(i, &r.anim_name) + moves::SIM_TICKS_PER_TABLE_TICK,
            None => self.clip_hold_ticks(i, ""),
        }
    }

    /// Every skill's `SHOTnn` marker offsets: (ground effect's, mid-air
    /// effect's), main.rs at startup (`cache/shotpoints.json` + the action
    /// rows' effect numbers). Replaces whatever was set before.
    pub fn set_shot_markers(&mut self, markers: HashMap<i16, (Vec<Vec3>, Vec<Vec3>)>) {
        self.shot_markers = markers;
    }

    /// Each player's right hand through the attack clips, by slot
    /// (`cache/chars/<id>.hand.json`).
    pub fn set_hand_paths(&mut self, tables: impl IntoIterator<Item = HashMap<String, HandClip>>) {
        for (slot, t) in self.hands.iter_mut().zip(tables) {
            *slot = t;
        }
    }

    /// Whether a cast of `skill` plays its mid-air row (the choice
    /// `action_row` makes, including its fallback when one row is missing).
    fn uses_air_row(&self, skill: i16, airborne: bool) -> Option<bool> {
        let (ground, air) = self.action_rows.get(&skill)?;
        let s40 = self.skill(skill).map_or(0, |d| d.raw[40]);
        let want_air = s40 == 1 || s40 == 3 || (s40 == 2 && airborne);
        Some(if want_air { air.is_some() || ground.is_none() } else { ground.is_none() && air.is_some() })
    }

    /// Player `i`'s right hand in the world at clip frame `frame` of `clip`
    /// (position, blade direction), from the converted hand path; `None` when
    /// none is loaded for this character / clip (tests, an old cache).
    fn hand_world(&self, i: usize, clip: &str, frame: f32, players: &[Player]) -> Option<(Vec3, Vec3)> {
        let (p, d) = self.hands[i].get(clip)?.at(frame)?;
        let me = &players[i];
        Some((local_to_world(me.pos, me.yaw, p), local_dir_to_world(me.yaw, d)))
    }

    /// Where player `i`'s cast of `skill` makes its objects, in marker order
    /// (the program's fire matrix, `FUN_14018a310` / `FUN_1402626d0`): the
    /// `SHOTnn` marker offsets of the skill's effect slot 1 carried by the
    /// caster's matrix; when the effect has none, the right hand at the fire
    /// frame of the cast clip. Empty when no data is loaded: the chest.
    fn shot_marks(&self, i: usize, skill: i16, players: &[Player]) -> Vec<Vec3> {
        let air = self.uses_air_row(skill, self.airborne[i]);
        let me = &players[i];
        if let (Some(air), Some((g, a))) = (air, self.shot_markers.get(&skill)) {
            let list = if air { a } else { g };
            if !list.is_empty() {
                return list.iter().map(|m| local_to_world(me.pos, me.yaw, *m)).collect();
            }
        }
        if let Some(row) = self.cast_row(i, skill) {
            if let Some((p, _)) = self.hand_world(i, &row.anim_name, row.fire_tick as f32, players) {
                return vec![p];
            }
        }
        Vec::new()
    }

    /// Every skill's action-table rows (main.rs at startup, from
    /// `skills.json`): docs/FORMATS-SKILLS.md "Action table". Replaces
    /// whatever rows were set before.
    pub fn set_action_rows(&mut self, rows: HashMap<i16, (Option<ActionRow>, Option<ActionRow>)>) {
        self.action_rows = rows;
    }

    /// One skill's rows (tests anywhere in the crate): `Combat::action_row`
    /// then reads these instead of falling back to the old flat timing.
    #[cfg(test)]
    pub fn set_action_row(&mut self, skill: i16, ground: Option<ActionRow>, air: Option<ActionRow>) {
        self.action_rows.insert(skill, (ground, air));
    }

    /// Every skill's combo rows (main.rs at startup, from `skills.json`'s `action.chain`) and the
    /// combo window of its ground row: docs/PROGRAM-RE.md "Combos".
    pub fn set_chain_rows(&mut self, first: HashMap<i16, ChainWindow>, rows: HashMap<i16, Vec<(ActionRow, ChainWindow)>>) {
        self.chain_first = first;
        self.chain_rows = rows;
    }

    /// The row the cast player `i` is making plays (`FUN_14025a3e0`): on hit k >= 1 of a chain
    /// (`chain_count` = k) one of s14..s17, cyclically (`rows[(k - 1) % n]`, n = the leading
    /// non-zero ones); otherwise the ordinary ground / mid-air row (`action_row`).
    pub fn cast_row(&self, i: usize, skill: i16) -> Option<&ActionRow> {
        let k = self.fighters[i].chain_count as usize;
        match self.chain_rows.get(&skill).filter(|r| k > 0 && !r.is_empty()) {
            Some(rows) => Some(&rows[(k - 1) % rows.len()].0),
            None => self.action_row(skill, self.airborne[i]),
        }
    }

    /// The combo window of the row `cast_row` picks (w45 / w46 / w47); empty when it has none.
    pub fn cast_window(&self, i: usize, skill: i16) -> ChainWindow {
        let k = self.fighters[i].chain_count as usize;
        match self.chain_rows.get(&skill).filter(|r| k > 0 && !r.is_empty()) {
            Some(rows) => rows[(k - 1) % rows.len()].1,
            None => self.chain_first.get(&skill).copied().unwrap_or_default(),
        }
    }

    /// The action row for a cast of `skill`: the mid-air row (s13) when s40 =
    /// 1 or 3, or s40 = 2 and the caster is airborne (PROGRAM-RE.md
    /// "Dispatch map"); the ground row (s12) otherwise. Falls back to the
    /// other row if the one picked is missing, and to `None` (the old flat
    /// timing) for a skill with no rows set at all.
    pub fn action_row(&self, skill: i16, airborne: bool) -> Option<&ActionRow> {
        let (ground, air) = self.action_rows.get(&skill)?;
        let s40 = self.skill(skill).map_or(0, |d| d.raw[40]);
        let use_air = s40 == 1 || s40 == 3 || (s40 == 2 && airborne);
        if use_air {
            air.as_ref().or(ground.as_ref())
        } else {
            ground.as_ref().or(air.as_ref())
        }
    }

    /// The whole hit-reaction table, keyed by row id (main.rs at startup,
    /// from `skills.json`'s top-level `"reactions"`): docs/FORMATS-SKILLS.md
    /// "Action table and hit-reaction table". Replaces whatever was set before.
    pub fn set_reaction_rows(&mut self, rows: HashMap<i16, ReactionRow>) {
        self.reaction_rows = rows;
    }

    /// One row (tests anywhere in the crate).
    #[cfg(test)]
    pub fn set_reaction_row(&mut self, row: i16, r: ReactionRow) {
        self.reaction_rows.insert(row, r);
    }

    /// `i`'s current hit-reaction clip (`Motion::Hit` / `Motion::Blown` in
    /// `main.rs`'s `clip_for`): the row's pose for `hit_dir` / `hit_front`, by
    /// `kind` (docs/FORMATS-SKILLS.md "Action table and hit-reaction table"; w
    /// indices PROGRAM-RE.md "Hit reactions"). A wall hit in flight plays the
    /// WALLHIT clip and then its loop. No row loaded (tests) -> the old flat
    /// default.
    pub fn reaction_clip(&self, i: usize) -> &'static str {
        let f = &self.fighters[i];
        let anim = |idx: i16| pd_formats::anim::ANIM_NAMES.get(idx.max(0) as usize).copied().unwrap_or("DMG_SHOT_FRONT");
        // The paralysed struggle (state 20) plays DMG_PARARISE (clip 0x74).
        if f.struggle {
            return "DMG_PARARISE";
        }
        if let Some((base, ticks)) = f.wall_hit {
            let first = self.clip_hold_ticks(i, anim(base as i16));
            return anim(base as i16 + if ticks >= first { 1 } else { 0 });
        }
        let Some(r) = self.reaction_rows.get(&f.reaction_row) else { return "DMG_SHOT_FRONT" };
        let back = !f.hit_front;
        match r.kind {
            reaction_kind::FOUR_WAY => anim(match f.hit_dir {
                HitDir::Front => r.w[0],
                HitDir::Right => r.w[1],
                HitDir::Back => r.w[2],
                HitDir::Left => r.w[3],
            }),
            // The front half is +-90 deg (`0x140258a60`), not the four-way sectors.
            reaction_kind::FRONT_BACK => anim(r.w[if back { 1 } else { 0 }]),
            // Front pose w42 (index 1), back pose w46 (index 5) -- decoded
            // from row 3 (BLOW0_FRONT/BACK): FORMATS-SKILLS.md.
            reaction_kind::KNOCKDOWN => anim(r.w[if back { 5 } else { 1 }]),
            // Pinned: face-up + lie (w41/w42) arriving from the front, or
            // face-down + lie (w43/w44) from behind -- unconfirmed pairing.
            reaction_kind::PINNED => anim(r.w[if back { 2 } else { 0 }]),
            // Plain clip (0), clip-then-lie (1: its collapse clip), binds
            // (8/9): just the one pose (w41); no attack currently used as a
            // light/medium/heavy reaction plays a bind, so its lying half
            // (w42) isn't rendered yet.
            reaction_kind::CLIP | reaction_kind::CLIP_THEN_LIE | reaction_kind::BIND_A | reaction_kind::BIND_B => anim(r.w[0]),
            _ => anim(r.w[0]),
        }
    }

    fn anim_name(id: usize) -> &'static str {
        pd_formats::anim::ANIM_NAMES.get(id).copied().unwrap_or("DOWN_FRONT")
    }

    /// The pose a knock-down / collapse ends lying in (`DOWN_FRONT` /
    /// `DOWN_BACK`, the loop queued behind the landing or collapse clip) --
    /// and so which get-up follows (`0x14025c3d0`: DOWN_FRONT_STAND for 60,
    /// DOWN_BACK_STAND for 62).
    fn lying_pose(&self, i: usize) -> usize {
        let f = &self.fighters[i];
        if f.lie_from_collapse { f.lie_pose as usize } else { lying_pose_id(f.front_set, f.land_class) }
    }

    /// The clip of a body on the ground after a knock-down (`Motion::Floored`):
    /// the landing clip of its fall-height class and set (BLOW_LAND_{S,M,L}{F,B},
    /// `0x140259990`) for as long as it lasts, then the DOWN_FRONT / DOWN_BACK
    /// loop queued behind it. A collapse has no landing clip: its pose at once.
    pub fn lying_clip(&self, i: usize) -> &'static str {
        let f = &self.fighters[i];
        if !f.lie_from_collapse {
            let land = Self::anim_name(landing_clip_id(f.front_set, f.land_class));
            if f.lying_elapsed < self.clip_hold_ticks(i, land) {
                return land;
            }
        }
        Self::anim_name(self.lying_pose(i))
    }

    /// The get-up clip (`Motion::GetUp`): DOWN_FRONT_STAND after lying
    /// DOWN_FRONT, DOWN_BACK_STAND after DOWN_BACK (`0x14025c3d0`).
    pub fn getup_clip(&self, i: usize) -> &'static str {
        if self.lying_pose(i) == 60 { "DOWN_FRONT_STAND" } else { "DOWN_BACK_STAND" }
    }

    /// The victim's voice group for this reaction (`sfx.rs`): the row's own
    /// group when we have one, 0 (no voice) otherwise.
    pub fn reaction_voice_group(&self, i: usize) -> i16 {
        self.reaction_rows.get(&self.fighters[i].reaction_row).map_or(0, |r| r.voice_group)
    }

    /// The `common` sound id of player `i`'s current hit-reaction row (w3; 100 for the
    /// bind rows 20-23), 0 = none. The program plays it at the victim only together with
    /// the attacker's effect (`FUN_140259310`).
    pub fn reaction_sound(&self, i: usize) -> i16 {
        self.reaction_rows.get(&self.fighters[i].reaction_row).map_or(0, |r| r.sound)
    }

    /// The player `i` is up against when nothing else says so: their first
    /// enemy in slot order (in a duel, the other player; alone, themselves).
    /// Every other place takes its victim/target as a parameter.
    pub fn foe_of(&self, i: usize) -> usize {
        self.roster.first_enemy(i).unwrap_or(i)
    }

    /// The players an attack by `owner` can reach: everyone active but the
    /// caster (`0x14026cd70`), minus the caster's teammates when
    /// `friendly_fire` is `Off`; in slot order.
    pub fn victims_of(&self, owner: usize) -> impl Iterator<Item = usize> {
        let (roster, ff) = (self.roster, self.friendly_fire);
        roster.slots().filter(move |&j| j != owner && (ff == FriendlyFire::On || !roster.same_team(owner, j)))
    }

    /// The player `i` aims at: casts, homing, instant skills, the lock-on
    /// camera -- the lock target they hold (`Player::target`, see `lock.rs`),
    /// else their foe.
    pub fn aim_target(&self, i: usize, players: &[Player]) -> usize {
        self.aim_at(i, players.get(i).and_then(|p| p.target))
    }

    /// The melee dash's lock-on/range gate: locked on, and the target was
    /// within `MELEE_DASH_RANGE` (PROGRAM-RE.md "Lock-on": band 1) -- judged
    /// once, at the press (or a charge's release).
    fn dash_eligible(&self, i: usize, target: usize, players: &[Player]) -> bool {
        let t = players[target].target_point();
        let d = Vec3::new(t.x - players[i].pos.x, 0.0, t.z - players[i].pos.z);
        (players[i].locked || players[i].cpu_aim) && d.length() <= MELEE_DASH_RANGE
    }

    /// Player `i` is half way through a short-range dash's program step (`dash_mid`): the body
    /// push-apart waits for the step's second half, as the program pushes once per program tick,
    /// after the whole step (LIVE `188_Hyperpunch` rec 55 -> 56: a 9.4 u step lands 2 u from the
    /// target's centre and the push sends the target off sideways, not ahead of the dash).
    pub fn mid_dash_step(&self, i: usize) -> bool {
        self.fighters.get(i).is_some_and(|f| f.dash_mid && (f.action == Action::Lunge || (f.action == Action::Cast && f.melee)))
    }

    pub fn skill(&self, id: i16) -> Option<&SkillDef> {
        self.skills.iter().find(|s| s.id == id)
    }

    /// A fresh round now, under the rules in force (fighters, field and props reset as at any round start). Prop Hunt
    /// starts its own rounds with it (`prophunt.rs`).
    pub fn restart_round(&mut self) {
        self.start_round();
    }

    fn start_round(&mut self) {
        self.round += 1;
        for i in self.roster.slots() {
            let mut deck = self.arsenals[i].clone();
            // Fisher-Yates with the sim's own generator.
            for k in (1..deck.len()).rev() {
                let j = (self.next_rand() % (k as u32 + 1)) as usize;
                deck.swap(k, j);
            }
            // The arsenal's declared number of schools sets the Aura recovery
            // multiplier (`FUN_14024de20`, `arsenal::aura_handicap`).
            let kind = self.arsenal_kinds[i].unwrap_or_else(|| crate::arsenal::default_kind(&deck, &self.skills));
            let handicap = crate::arsenal::aura_handicap(kind);
            // The rule's row: Health, Level range; a fighter starts at the
            // setup's Level kept inside the range (Heavy starts at 5), Aura full.
            let row = self.rules.set.row(i);
            let f = &mut self.fighters[i];
            *f = Fighter::new(deck, self.rules.start_level.clamp(row.level_min, row.level_max));
            f.aura_handicap = handicap;
            f.health = row.health;
            f.max_health = row.health;
            f.level_min = row.level_min;
            f.level_max = row.level_max;
            for k in 0..self.rules.start_hand.min(SLOTS) {
                let skill = f.arsenal.pop();
                f.set_slot(k, skill);
            }
            if let Some(hand) = self.rules.fixed_hand {
                // A game mode's fixed hand: these four skills, in these buttons, and nothing left to draw.
                f.arsenal.clear();
                for (k, &skill) in hand.iter().enumerate() {
                    f.set_slot(k, Some(skill));
                }
            }
        }
        self.capsules.clear();
        self.projectiles.clear();
        self.areas.clear();
        self.blasts.clear();
        self.crystal = None;
        // Unconfirmed: every round starts with the arena as placed.
        for p in &mut self.props {
            p.restore();
        }
        self.pending_effects.clear();
        self.delayed_effects.clear();
        self.instant_fires.clear();
        self.round_over = None;
        self.round_decided_tick = None;
        self.round_ticks = 0;
        self.time_up = false;
    }

    /// Battle time left in ticks (None = no limit).
    pub fn time_left(&self) -> Option<u32> {
        (self.time_limit > 0).then(|| (COUNTDOWN_TICKS + self.time_limit).saturating_sub(self.round_ticks.max(COUNTDOWN_TICKS)))
    }

    /// The pre-round countdown is running (everyone frozen).
    pub fn counting_down(&self) -> bool {
        self.round_ticks < COUNTDOWN_TICKS
    }

    fn next_rand(&mut self) -> u32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// The sim's seeded RNG state, read-only: the CPU host seeds its own private stream from it (`cpu/natives.rs`) so a CPU
    /// seat never consumes draws from the sim's stream.
    pub fn rng_state(&self) -> u32 {
        self.rng
    }

    /// A deterministic pick in 0..n (for skills that choose at random).
    pub fn random(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next_rand() % n }
    }

    /// The environment skill in force.
    pub fn env_def(&self) -> Option<&SkillDef> {
        self.crystal.as_ref().and_then(|c| self.skill(c.skill))
    }

    /// The amount the standing crystal gives status `id` (0 if its status id,
    /// s44, is another): `FUN_140271660`, read by `FUN_14024d4d0` for every
    /// status query (31 Necronomicon, 32 Memory Lapse, ...).
    pub fn env_status(&self, id: i16) -> i32 {
        self.env_def().map_or(0, |e| environment::status(e, id))
    }

    /// Player `i`'s total for `stat`: their status plus the environment.
    pub fn stat(&self, i: usize, stat: Stat) -> i32 {
        (self.fighters[i].status.raw_total(stat) + self.env_def().map_or(0, |e| environment::stat(e, stat))).clamp(-stat.cap(), stat.cap())
    }

    /// Movement speed factor for player `i` (Speed Up, Burden, Reduce Gravity...).
    pub fn move_factor(&self, i: usize) -> f32 {
        (100 + self.stat(i, Stat::Speed)).max(0) as f32 / 100.0
    }

    /// Player `i`'s directional controls are inverted.
    pub fn inverted(&self, i: usize) -> bool {
        self.fighters[i].status.inverted_ticks > 0 || self.env_def().is_some_and(environment::inverted)
    }

    fn rand_unit(&mut self) -> f32 {
        (self.next_rand() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub(crate) fn log(&mut self, e: CombatEvent) {
        self.event_seq += 1;
        self.events.push((self.event_seq, e));
        if self.events.len() > EVENT_LOG {
            self.events.remove(0);
        }
        #[cfg(test)]
        if matches!(e, CombatEvent::Hit { .. } | CombatEvent::Blocked { .. } | CombatEvent::Burst { .. } | CombatEvent::Denied { .. } | CombatEvent::Capsule { .. }) {
            self.legacy_seq += 1;
            self.legacy_events.push((self.legacy_seq, e));
            if self.legacy_events.len() > LEGACY_EVENT_LOG {
                self.legacy_events.remove(0);
            }
        }
    }

    fn take_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Film comparison (`PD_FILM_FIELD`): put a takeable capsule of `skill` on `owner`'s capsule spot `spot` (the take's stage held it
    /// at the start). A spot a fighter in `bodies` stands within pickup reach of is moved 80 u away from that fighter (the original's
    /// takes were redone when a fighter touched a capsule; the film's flat-catwalk fallback puts the fighters on top of the spots, and
    /// a skill press there would capture the capsule instead of casting). Does nothing when the stage has no such spot.
    pub fn place_field_capsule(&mut self, owner: usize, spot: usize, skill: i16, bodies: &[Vec3]) {
        let Some(&spot_pos) = self.capsule_spots.get(owner).and_then(|s| s.get(spot)) else { return };
        let mut pos = spot_pos;
        for &b in bodies {
            if in_pickup_reach(pos, b) {
                let away = Vec3::new(pos.x - b.x, 0.0, pos.z - b.z);
                pos += if away.length() > 0.01 { away.normalize() * 80.0 } else { Vec3::new(80.0, 0.0, 0.0) };
            }
        }
        let id = self.take_id();
        self.capsules.push(CapsuleItem { id, owner, skill, pos, appear: 0 });
    }

    /// Runs after movement each tick. Returns true when a new round starts
    /// (the caller puts the players back on their spawns).
    pub fn step(&mut self, players: &mut [Player], buttons: impl AsRef<[u8]>, col: &Collision) -> bool {
        let buttons = buttons.as_ref();
        let roster = self.roster;
        if let Some((winner, left)) = self.round_over {
            if left <= 1 {
                self.start_round();
                return true;
            }
            self.round_over = Some((winner, left - 1));
        }
        self.round_ticks = self.round_ticks.saturating_add(1);
        if self.counting_down() {
            return false;
        }
        self.airborne = std::array::from_fn(|i| roster.is_active(i) && !players[i].grounded);
        for c in &mut self.capsules {
            c.appear = c.appear.saturating_sub(1);
        }
        // Time up: the one with more Health wins the round (a tie at the top
        // scores nobody: a draw).
        if self.time_limit > 0 && !self.rounds_by_mode && self.round_over.is_none() && self.round_ticks >= COUNTDOWN_TICKS + self.time_limit {
            let best = roster.slots().map(|i| self.fighters[i].health).max();
            let mut leaders = roster.slots().filter(|&i| Some(self.fighters[i].health) == best);
            let winner = match (leaders.next(), leaders.next()) {
                (Some(w), None) => Some(w),
                _ => None,
            };
            if let Some(w) = winner {
                self.wins[w] += 1;
            }
            self.round_over = Some((winner.unwrap_or(DRAW), ROUND_END_TICKS));
            self.time_up = true;
        }
        // Powerless: no skills left anywhere -> Health drains.
        if self.round_over.is_none() && (self.round_ticks - COUNTDOWN_TICKS) % POWERLESS_DRAIN_TICKS == 0 {
            for i in roster.slots() {
                let f = &self.fighters[i];
                let out = f.arsenal.is_empty() && f.slots.iter().all(Option::is_none) && !self.capsules.iter().any(|c| c.owner == i);
                if out && f.action != Action::Down {
                    // A silent Health change (`FUN_14024d7a0`), not a hit: no
                    // reaction from a stale row.
                    self.change_health(i, -1);
                }
            }
        }
        for i in roster.slots() {
            let still = players[i].pos.distance(players[i].prev_pos) < 0.05;
            let f = &mut self.fighters[i];
            f.status.still_ticks = if still { f.status.still_ticks.saturating_add(1) } else { 0 };
            let every = f.status.regen_every;
            if every > 0 && f.status.still_ticks > 0 && f.status.still_ticks % every == 0 && f.action != Action::Down {
                f.health = (f.health + 1).min(f.max_health);
            }
        }
        if let Some(cr) = &mut self.crystal {
            cr.age = cr.age.saturating_add(1);
        }
        // A crystal with a lifetime (s47: the one-minute bans) is gone when it
        // runs out; a new one can then be set (`FUN_14017bcc0`).
        if let Some(cr) = &self.crystal {
            if self.skill(cr.skill).and_then(environment::lifetime_ticks).is_some_and(|life| cr.age > life) {
                self.crystal = None;
            }
        }
        if let Some(h) = self.test_health {
            for i in roster.slots() {
                let f = &mut self.fighters[i];
                f.health = f.health.min(h);
            }
        }
        if let Some((slots, level)) = self.test_loadout {
            for i in roster.slots() {
                let f = &mut self.fighters[i];
                f.level = level;
                f.aura_ticks = level * AURA_TICKS_PER_POINT;
                for (s, id) in f.slots.iter_mut().zip(slots) {
                    s.get_or_insert(id);
                }
            }
        }
        for i in roster.slots() {
            self.tick_fighter(i, players, buttons.get(i).copied().unwrap_or(0), col);
        }
        self.move_projectiles(players, col);
        self.tick_areas(players, col);
        self.settle_mines();
        for (caster, target, skill, button) in std::mem::take(&mut self.pending_effects) {
            if let Some(def) = self.skill(skill).cloned() {
                // The effect modules read the caster's button from `last_slot`:
                // show them the button this skill was cast with, then put it back.
                let pressed = std::mem::replace(&mut self.fighters[caster].last_slot, button);
                let landed = moves::apply_effect(self, players, caster, Reach::Target(target), &def);
                self.fighters[caster].last_slot = pressed;
                // s60: the second reaction of a status skill whose effect landed (Pressure,
                // Lift Up, Entangle, Pin Down, ...; `combat/reactions.rs`).
                if landed {
                    self.status_skill_reaction(caster, target, &def, players);
                }
            }
        }
        self.land_delayed_effects(players);
        // A forced cast (Trigger) of an instant skill made during the effects above.
        self.fire_instants(players, col);
        for i in roster.slots() {
            self.spawn_capsule(i, &players[i], col);
        }
        // Everyone knocked out on the same tick: a draw. (The first knock-out
        // processed this tick had scored it as that survivor's win.)
        if let Some((w, left)) = self.round_over {
            if left == ROUND_END_TICKS && w != DRAW && roster.slots().all(|i| self.fighters[i].action == Action::Down) {
                self.wins[w] -= 1;
                self.round_over = Some((DRAW, ROUND_END_TICKS));
            }
        }
        false
    }

    /// A fighter in the air from a knock-down launch (state 17 before the
    /// landing): one tick of the program's flight -- gravity `KNOCKDOWN_GRAVITY`,
    /// wall contact (`0x140255020` flags it, `0x140259b30` bounces the flyer at
    /// half speed up to three times, 20 program ticks apart, playing WALLHIT),
    /// the ground, and the rule that a flyer hung at one height for 90 program
    /// ticks is put down (`0x14025c260`). On touch-down the fall height picks
    /// the landing class and its Health cost (0 / 1 / 2, `FUN_140254c80`; it
    /// can be the killing blow) and the landing clip (`lying_clip`).
    fn knock_flight(&mut self, i: usize, players: &mut [Player], col: &Collision) {
        let first = self.fighters[i].wall_hit.map_or(0, |(base, _)| self.clip_hold_ticks(i, Self::anim_name(base as usize)));
        let f = &mut self.fighters[i];
        f.wall_cooldown = f.wall_cooldown.saturating_sub(1);
        // While the first WALLHIT clip plays the body cannot land (`0x140254590`
        // tests the clip id): it slides on along the floor.
        let can_land = f.wall_hit.is_none_or(|(_, ticks)| ticks >= first);
        if let Some((_, ticks)) = &mut f.wall_hit {
            *ticks = ticks.saturating_add(1);
        }
        let step = step_knockdown_flight(f, &mut players[i], col, can_land);
        if let Some(normal) = step.wall {
            if f.wall_hits_left > 0 && f.wall_cooldown == 0 {
                f.wall_hits_left -= 1;
                f.wall_cooldown = WALL_COOLDOWN_TICKS;
                let speed = Vec3::new(f.knock_vel.x, 0.0, f.knock_vel.z).length() * WALL_SPEED_KEEP;
                f.knock_vel = Vec3::new(normal.x * speed, f.knock_vel.y, normal.z * speed);
                // The program picks the clip by the set of the clip playing
                // now: the front set plays WALLHIT_FRONT (56), anything else
                // WALLHIT_BACK (58), facing the way it now flies / the other way.
                let now = f.wall_hit.map_or(None, |(base, _)| Some(base as usize + 1));
                let front = match now {
                    Some(id) => program_front_set(id),
                    None => f.front_set,
                };
                f.wall_hit = Some((if front { 56 } else { 58 }, 0));
                let bearing = yaw_of(normal);
                f.pending_turn = Some(if front { bearing } else { bearing + std::f32::consts::PI });
            }
        }
        // Hung in the air (stuck on geometry): put down after 90 program ticks.
        let y = players[i].pos.y;
        let f = &mut self.fighters[i];
        if (y - f.stuck_y).abs() <= 0.25 {
            f.stuck_ticks = f.stuck_ticks.saturating_add(1);
        } else {
            f.stuck_ticks = 0;
            f.stuck_y = y;
        }
        let mut landed = step.landed;
        if landed.is_none() && f.flying && f.stuck_ticks > STUCK_FLIGHT_TICKS {
            f.flying = false;
            f.knock_vel = Vec3::new(f.knock_vel.x, 0.0, f.knock_vel.z);
            landed = Some(0.0);
        }
        let Some(fall) = landed else { return };
        self.log(CombatEvent::KnockLanded { who: i });
        let f = &mut self.fighters[i];
        let class: u8 = match fall {
            h if h < LAND_FALL_MIN => 0,
            h if h <= LAND_FALL_HEAVY => 1,
            _ => 2,
        };
        // The set is that of the clip playing now: the flight clip, or the
        // WALLHIT clip's loop.
        let id = match f.wall_hit {
            Some((base, _)) => base as usize + 1,
            None => {
                let name = self.reaction_clip(i);
                pd_formats::anim::ANIM_NAMES.iter().position(|n| *n == name).unwrap_or(0)
            }
        };
        let f = &mut self.fighters[i];
        let front = if f.wall_hit.is_some() { program_front_set(id) } else { f.front_set };
        f.front_set = front;
        f.land_class = class;
        f.lying_elapsed = 0;
        f.wall_hit = None;
        f.land_flip = landing_flips(front, class);
        // The landing clip (BLOW_LAND_*) starts with root motion (`0x140259990`, root flag 1).
        let landing = Self::anim_name(landing_clip_id(front, class));
        self.root_clip(i, landing);
        if class > 0 {
            self.change_health(i, -(class as i32));
        }
    }

    /// The landed flyer's slide: it loses 15% of its speed per program tick and
    /// stops under 0.02 u per tick (`0x14025c260`); the program steps at 30 Hz,
    /// so it moves on every second sim tick. The half turn of a back-set big
    /// landing happens when the DOWN clip starts behind the landing clip.
    fn knock_slide(&mut self, i: usize, players: &mut [Player]) {
        let land_len = self.clip_hold_ticks(i, Self::anim_name(landing_clip_id(self.fighters[i].front_set, self.fighters[i].land_class)));
        let f = &mut self.fighters[i];
        if f.land_flip && !f.lie_from_collapse && f.lying_elapsed >= land_len {
            f.land_flip = false;
            f.pending_turn = Some(players[i].yaw + std::f32::consts::PI);
        }
        if f.flying || f.knock_vel == Vec3::ZERO {
            return;
        }
        f.slide_odd = !f.slide_odd;
        if !f.slide_odd {
            return;
        }
        let speed = f.knock_vel.length();
        if speed <= SLIDE_STOP_SPEED {
            f.knock_vel = Vec3::ZERO;
            return;
        }
        f.knock_vel *= SLIDE_KEEP;
        players[i].pos += f.knock_vel * (2.0 * DT);
    }

    /// One tick of a fighter who is out but whose last reaction still plays:
    /// the flight, the slide, the landing clip, then lying for the rest of the
    /// round (never the get-up). A KO by a plain stagger collapses (row 25)
    /// once its clip has played.
    fn ko_step(&mut self, i: usize, players: &mut [Player], col: &Collision) {
        if self.fighters[i].flying {
            self.knock_flight(i, players, col);
            return;
        }
        self.knock_slide(i, players);
        let f = &mut self.fighters[i];
        if f.ko_stagger > 0 {
            f.ko_stagger -= 1;
            if f.ko_stagger == 0 {
                self.start_ko_collapse(i);
            }
            return;
        }
        if !f.lie_from_collapse {
            f.lying_elapsed = f.lying_elapsed.saturating_add(1);
        }
        // Lying for good: the timeline stops where the get-up would begin.
        if f.action_ticks > KNOCK_GETUP_TICKS + 1 {
            f.action_ticks -= 1;
        }
    }

    /// The second reaction (s60) chained behind a stagger starts as the
    /// first one ends: `FUN_140258690(row, -1, facing)` is called with the
    /// victim's own facing as the hit bearing, so it always plays the front
    /// variant (`0x14025c1a0`).
    fn start_queued_reaction(&mut self, i: usize, players: &[Player]) {
        let row = std::mem::take(&mut self.fighters[i].queued_reaction);
        let yaw = players[i].yaw;
        let away = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let f = &mut self.fighters[i];
        f.reaction_row = row;
        f.hit_dir = HitDir::Front;
        f.hit_front = true;
        f.hit_seq = f.hit_seq.wrapping_add(1);
        self.play_row(i, away);
    }

    /// One tick of one fighter: the state handler and the button handling,
    /// then the timers (the program's order in `0x14024ec80`: the dispatcher,
    /// the physics, `0x1402517c0` the buttons, `0x14024d2f0` the timers, which
    /// is where the Aura refill runs).
    fn tick_fighter(&mut self, i: usize, players: &mut [Player], buttons: u8, col: &Collision) {
        if std::mem::take(&mut players[i].took_off) {
            self.log(CombatEvent::JumpTakeoff { who: i });
        }
        if let Some(fall) = players[i].landed.take() {
            self.log(CombatEvent::Landed { who: i, fall });
        }
        self.fighter_step(i, players, buttons, col);
        self.refill_aura(i, players);
    }

    /// Aura recovery (`FUN_14024de20`, allowed by `FUN_1402523a0`): per
    /// program tick (1/30 s -- every second sim tick, with the whole tick's
    /// amount) `+ (100 + Aura-rate status) x (handicap x max(0.25, 1.25 -
    /// Aura / 10) / 30) / 100` points, never above Level. Nothing is gained
    /// with no Health, nor in the ground dodge / Dash (state 3), the jump
    /// wind-up (4) or any moment of a cast (10-13: the whole clip, including
    /// a held charge). Under Fatigue only a still player recovers.
    fn refill_aura(&mut self, i: usize, players: &[Player]) {
        if self.round_ticks % 2 != 0 {
            return;
        }
        // The round's knock-out freezes it 60 program ticks (120 sim ticks) later: the same 0x3c the program waits after the
        // first death before it closes the round (`FUN_14012c510`: `earliest - 0x3c`, then `FUN_14012c790` and the round-end
        // state). LIVE `257_Judge_of_Anubis`: P1 knocked out at tick 64, P0's Aura climbs 4.00 -> 5.31 and stops dead at tick 124
        // (= KO + 60), no cast or dodge involved. The state / call that switches the timer off is UNCONFIRMED (the 60 is READ + LIVE).
        if self.round_decided_tick.is_some_and(|t| self.round_ticks.saturating_sub(t) > SIMULTANEOUS_KO_TICKS) {
            return;
        }
        let f = &self.fighters[i];
        if f.health <= 0 || f.action == Action::Down || matches!(f.action, Action::Cast | Action::Lunge) || players[i].dodging_or_winding_up() {
            return;
        }
        if f.status.still_ticks == 0 && self.env_def().is_some_and(environment::aura_stops_while_moving) {
            return;
        }
        let aura = f.aura_ticks as f32 / AURA_TICKS_PER_POINT as f32;
        let curve = (AURA_CURVE_TOP - aura / AURA_CURVE_SLOPE).max(AURA_CURVE_FLOOR);
        let rate = (100 + self.stat(i, Stat::AuraRate)) as f32;
        let gain = (rate * ((f.aura_handicap * curve) / PROGRAM_TICK_DIV) / 100.0) * AURA_TICKS_PER_POINT as f32;
        let f = &mut self.fighters[i];
        f.aura_frac += gain;
        let whole = f.aura_frac.floor();
        f.aura_frac -= whole;
        let cap = f.level * AURA_TICKS_PER_POINT;
        let before = f.aura_ticks / AURA_TICKS_PER_POINT;
        f.aura_ticks = (f.aura_ticks + whole as i32).min(cap);
        if f.aura_ticks >= cap {
            f.aura_frac = 0.0;
        }
        // The whole-number part of Aura rose: the program's regen blip
        // (`FUN_14024de20`, common/5 at `0x14024df1d`).
        if f.aura_ticks / AURA_TICKS_PER_POINT > before {
            self.log(CombatEvent::AuraTick { who: i });
        }
    }

    fn fighter_step(&mut self, i: usize, players: &mut [Player], buttons: u8, col: &Collision) {
        let prev = std::mem::replace(&mut self.fighters[i].prev_buttons, buttons);
        let f = &mut self.fighters[i];
        f.status.tick();
        if let Some(g) = &mut f.guard {
            g.ticks = g.ticks.saturating_sub(1);
            if g.ticks == 0 {
                f.guard = None;
            }
        }
        f.frozen_ticks = f.frozen_ticks.saturating_sub(1);
        f.absorb_glow_ticks = f.absorb_glow_ticks.saturating_sub(1);
        f.denied_ticks = f.denied_ticks.map(|t| t.saturating_sub(1));
        f.poison_ticks = f.poison_ticks.saturating_sub(1);
        f.invuln_ticks = f.invuln_ticks.saturating_sub(1);
        if let Some((_, t)) = &mut f.forced_lock {
            *t = t.saturating_sub(1);
            if *t == 0 {
                f.forced_lock = None;
            }
        }
        // A reaction that writes the facing (a knock-down's launch, a wall
        // hit, the half turn of a big landing) decided it on the tick it
        // started; the body takes it now, before anything moves, and the
        // renderer's blend between ticks is moved with it (an instant turn).
        if let Some(yaw) = f.pending_turn.take() {
            let p = &mut players[i];
            let (turned, _) = turn_toward(p.yaw, yaw, std::f32::consts::TAU);
            p.prev_yaw += turned - p.yaw;
            p.yaw = turned;
        }
        // Kind 9 (lift): the launch the reaction start left for the body (`0x140258d7f`).
        if let Some(v) = f.lift_launch.take() {
            players[i].lift(v);
        }
        // The reaction clip's root motion moves the body this tick (`0x140257510`). A short-range
        // cast takes it inside `lunge`, after the dash: the program runs the state handler (the
        // dash measures its distance and steps) before the clip update adds the root motion
        // (`FUN_14024ec80`: `FUN_140256e80` then `FUN_140257510`).
        let lunging = f.charge.is_none() && (f.action == Action::Lunge || (f.action == Action::Cast && f.melee));
        if !lunging {
            self.root_step(i, players);
        }
        let f = &mut self.fighters[i];
        if f.stamp_left > 0 {
            f.stamp_left -= 1;
        }
        if f.action == Action::Down {
            if f.ko_play {
                self.ko_step(i, players, col);
            }
            return;
        }

        // Knock-down flight: real physics (`KNOCKDOWN_GRAVITY`, wall and
        // ground collision) instead of a fixed tick count -- `action_ticks`
        // (the ground timeline: lying then getting up) only starts once
        // landed. Landing from state 17 costs 0 / 1 / 2 Health by the fall
        // height (< 40 / 40-80 / > 80 u, `FUN_140254c80`; it can be the
        // killing blow).
        if f.action == Action::Knocked && f.flying {
            self.knock_flight(i, players, col);
            return;
        }
        // Landed: the horizontal knock-back keeps sliding a little (below);
        // the landing clip's own clock runs.
        if f.action == Action::Knocked && !f.lie_from_collapse {
            f.lying_elapsed = f.lying_elapsed.saturating_add(1);
        }
        if f.action == Action::Knocked {
            self.knock_slide(i, players);
        }
        let f = &mut self.fighters[i];

        // Short-range: `lunge` owns the whole cast (dash, hit window,
        // recovery), not just the initial dash -- it keeps running after the
        // fire tick switches `action` to `Cast` (the strike/recovery pose)
        // until the row's clip ends. A charge still holds as `Cast` first.
        if f.charge.is_none() && (f.action == Action::Lunge || (f.action == Action::Cast && f.melee)) {
            self.lunge(i, players, col);
            self.chain_step(i, players, buttons, prev, col);
            return;
        }
        if f.action == Action::Cast && f.charge.is_some() {
            self.charge_step(i, players, buttons);
            return;
        }
        if f.action != Action::Ready {
            let casting = f.action == Action::Cast;
            let was_knocked = f.action == Action::Knocked;
            f.action_ticks = f.action_ticks.saturating_sub(1);
            // Getting up starts here (the lying timer just crossed into the
            // get-up clip's own length): blinking and untouchable for
            // `POST_GETUP_INVULN_TICKS` from *this* tick -- PROGRAM-RE.md
            // "Invulnerable ... until (get-up clip + 45 ticks) after starting
            // to get up", so the window covers the get-up clip itself, not
            // just the time after it. No skill's s62 has bit 8 (the only
            // thing that could still reach a player in this state), so it
            // reads as full invulnerability once it starts.
            let got_up = was_knocked && f.action_ticks == KNOCK_GETUP_TICKS;
            if got_up {
                f.invuln_ticks = POST_GETUP_INVULN_TICKS;
            }
            // The get-up clip starts: common/81 (`FUN_14025c3d0`, `0x14025c4ee`).
            let f = if got_up {
                self.log(CombatEvent::GotUp { who: i });
                // The get-up clip's root motion (`0x14025c4c0`: root flag 1).
                let clip = self.getup_clip(i);
                self.root_clip(i, clip);
                &mut self.fighters[i]
            } else {
                f
            };
            if !casting {
                f.pending_shot = None; // hit or knocked down mid-cast, before it fired
                f.bill = None;
                f.charge = None;
            } else if f.fire_ticks > 0 {
                f.fire_ticks -= 1;
            }
            // Fires exactly once, when the row's fire tick is reached (item
            // 1/3): a hit that ends the cast before then already cleared
            // `pending_shot` above, so nothing fires for it.
            let fire = if casting && f.fire_ticks == 0 { f.pending_shot.take() } else { None };
            // Kind 9: state 14 only goes idle once the body is back on the ground
            // (`0x14025c1a0`: `+0x12e74` must be set).
            if f.lift_hold {
                let top = f.flight_peak.get_or_insert(players[i].pos.y);
                *top = top.max(players[i].pos.y);
            }
            if f.action_ticks == 0 && f.lift_hold && !players[i].grounded {
                f.action_ticks = 1;
            }
            // The lifted body comes down in state 17 and its landing costs Health by the fall from the top of the lift
            // like any knock-down's (`FUN_140254c80`: < 40 u nothing, 40-80 u 1, > 80 u 2): LIVE `303_Antigravity_Trap`, P0 lifted
            // 48.3 u (270.48 -> 318.76, state 14), state 17 from the apex at tick 184, down at 270.31 on tick 212 with Health -1.
            let lift_fall = if f.action_ticks == 0 && f.lift_hold { f.flight_peak.take().map(|top| top - players[i].pos.y) } else { None };
            if f.action_ticks == 0 {
                f.action = Action::Ready;
                f.lift_hold = false;
                f.struggle = false;
                f.root = None;
                if let Some(fall) = lift_fall {
                    let cost = if fall < LAND_FALL_MIN { 0 } else if fall <= LAND_FALL_HEAVY { 1 } else { 2 };
                    if cost > 0 {
                        self.change_health(i, -cost);
                    }
                }
                let f = &mut self.fighters[i];
                // A second reaction chained behind a stagger (s60) starts
                // as the first one ends (`0x14025c1a0`).
                if !casting && f.queued_reaction != 0 {
                    self.start_queued_reaction(i, players);
                    return;
                }
            }
            let f = &mut self.fighters[i];
            let aim = f.aim;
            // The program checks the fire countdown, then runs the state's
            // handler (`0x14025c720` before `0x14025bd40`): the shot leaves
            // along the facing the previous tick left, and the turn toward the
            // aim carries on after it, for every tick the state lasts.
            if let Some(skill) = fire {
                self.fire(i, skill, players, col);
                // READ `FUN_140270b00` (the Add-effect switch on s45 of a Status skill): kind 11 (Regenerate)
                // starts player state 21, kind 15 (Invulnerability) state 22, kind 20 (Mephisto's Pact) state 24,
                // each through `FUN_140259d10`, which REPLACES the cast state (10) at the fire tick. LIVE
                // takes_full 238 / 298 / 300: state 10 lasts exactly the fire tick (20), then 21 / 22 / 24.
                // Level Boost (kind 10) and every other kind leave the cast state alone (clip + 1, above).
                if self.fighters[i].action == Action::Cast && self.skill(skill).is_some_and(|d| d.raw[43] == 4 && matches!(d.raw[45], 11 | 15 | 20)) {
                    let f = &mut self.fighters[i];
                    f.action = Action::Ready;
                    f.action_ticks = 0;
                    f.root = None;
                }
            }
            if casting && self.fighters[i].action == Action::Cast {
                cast_turn(aim, players, i);
            }
            if casting {
                self.chain_step(i, players, buttons, prev, col);
            }
            return;
        }
        if self.round_over.is_some() {
            return;
        }
        if let Some(button) = (0..SLOTS).find(|&k| pressed(buttons, prev, k)) {
            // Other buttons pressed on this same tick are handled next tick
            // (unmark them as seen) instead of being lost.
            let others = buttons & !prev & !(1u8 << button);
            self.fighters[i].prev_buttons &= !others;
            // Dazzle: the buttons are scrambled.
            let st = &self.fighters[i].status;
            let slot = if st.scrambled_ticks > 0 { st.scramble[button] as usize % SLOTS } else { button };
            // Frostbite: this button is locked.
            if st.locked_ticks > 0 && st.locked_slot as usize == slot {
                // The program's refusal sound (common/2: the button's own lock flags,
                // `0x140251b32`, P0-SOUND.md D4).
                self.log(CombatEvent::Denied { who: i, slot });
                return;
            }
            // A remote mine of yours is out: this press sets it off instead.
            if let Some(a) = self.areas.iter_mut().find(|a| a.owner == i && a.armed && a.spec.trigger == Trigger::Remote && !a.detonate) {
                a.detonate = true;
                return;
            }
            let f = &mut self.fighters[i];
            if f.frozen_ticks > 0 {
                // Frozen buttons: not usable, the same refusal (`FUN_140261e90`, `0x140261fa0`).
                self.log(CombatEvent::Denied { who: i, slot });
                return;
            }
            // As in the original: standing at one of your capsules, a skill
            // button captures it into that slot (replacing what was there);
            // anywhere else it uses the skill in that slot. Capturing works
            // only in the ground states 0 / 1 / 8 (`0x1402517c0`); in the air,
            // mid-dodge or in a landing the press goes to the skill instead.
            let reach = if players[i].body_state() == BodyState::Ground { self.capsule_in_reach(i, &players[i]) } else { None };
            // Poison: "each time the opponent tries to use a skill or capture a
            // new skill they will incur damage of 1" -- a capture pays it at the
            // press, a skill at its fire tick (`FUN_1402626d0`). It can kill.
            if self.fighters[i].poison_ticks > 0 && reach.is_some() {
                self.change_health(i, -1);
                if self.fighters[i].action == Action::Down {
                    return;
                }
            }
            match reach {
                // "Can't be erased or overwritten": that button can't take a capsule.
                Some(_) if self.fighters[i].slots[slot].and_then(|id| self.skill(id)).is_some_and(SkillDef::protected) => {}
                Some(k) => {
                    let cap = self.capsules.remove(k);
                    let dh = self.env_def().map_or(0, |e| environment::health_on_capture(e, cap.skill == AURA_PARTICLE));
                    self.change_health(i, dh);
                    let f = &mut self.fighters[i];
                    f.set_slot(slot, Some(cap.skill));
                    // A capture changes no state in the program (`0x1402517c0` ->
                    // `0x140261a00`, `0x140262670`): the fighter is not held.
                    self.log(CombatEvent::Capsule { who: i, slot, skill: cap.skill });
                    if self.fighters[i].action == Action::Down {
                        return;
                    }
                }
                None => {
                    // No capsule in reach: an environment crystal that has
                    // outlived its capture lock goes to whoever presses a skill
                    // button next to it, ground states only (mode 3,
                    // `FUN_140251550` -> `FUN_14017be00`).
                    if players[i].body_state() == BodyState::Ground && self.take_crystal(i, slot, players[i].pos) {
                        return;
                    }
                    self.cast(i, slot, players);
                    self.fire_instants(players, col);
                    // A charge holds while this (raw, unscrambled) button stays down.
                    if let Some(ch) = &mut self.fighters[i].charge {
                        ch.bit = 1 << button;
                    }
                }
            }
        }
    }

    /// Captures the standing crystal for player `i` if it is within 15 u
    /// (3-D) and past its capture lock (s45: `now > created + s45`,
    /// `FUN_14017c5d0`). The capturer gets an Aura Particle on `slot` (record
    /// 0, `FUN_140261a00(slot, 0)`; a button holding a "can't be erased or
    /// overwritten" skill keeps it) and the crystal ends: its effect stops and a
    /// new one can be set. No Health change and no consume lock.
    fn take_crystal(&mut self, i: usize, slot: usize, at: Vec3) -> bool {
        let Some(cr) = &self.crystal else { return false };
        let lock = self.skill(cr.skill).filter(|d| d.raw[43] == 5).map_or(CRYSTAL_CAPTURE_AFTER, environment::capture_lock_ticks);
        if cr.age <= lock || cr.pos.distance(at) >= PICKUP_REACH {
            return false;
        }
        let cr_pos = cr.pos;
        self.crystal = None;
        self.log(CombatEvent::CrystalTaken { who: i, pos: cr_pos });
        let keeps = self.fighters[i].slots[slot].and_then(|id| self.skill(id)).is_some_and(SkillDef::protected);
        if !keeps {
            self.fighters[i].set_slot(slot, Some(AURA_PARTICLE));
        }
        true
    }

    /// Sim ticks a stage capsule spot stays empty after its capsule is taken or
    /// erased, before the appearing phase: the program's 420 hidden ticks plus the
    /// environment's respawn status (`FUN_14014fd10`), two sim ticks each.
    fn spot_hidden_ticks(&self) -> u16 {
        let delta = self.env_def().map_or(0, environment::capsule_wait_delta);
        (self.rules.capsule_respawn_ticks.saturating_sub(CAPSULE_APPEAR_TICKS) as i32 + delta * moves::SIM_TICKS_PER_TABLE_TICK as i32).clamp(0, u16::MAX as i32) as u16
    }

    /// The stage capsule spots that are empty and waiting to regenerate
    /// (the program's capsule slot state 0, `FUN_14014ed10`): read-only, for
    /// the renderer's regeneration motes (`capsule_birth.rs`). A spot is listed
    /// from the tick after its capsule left until the capsule appears (then it
    /// is in `capsules`, `appear` counting down). The opening deal is never
    /// listed (it starts present, `FUN_14014ee70`). Spots whose next capsule
    /// will not come because the arsenal is spent are left out: the soonest
    /// `arsenal.len()` spots per player count.
    pub fn pending_capsules(&self) -> Vec<PendingCapsule> {
        if !self.rules.capsules {
            return Vec::new();
        }
        let wait = self.spot_hidden_ticks();
        let mut out = Vec::new();
        for i in 0..MAX_PLAYERS {
            if self.fighters[i].capsule_anchor.is_some() {
                continue;
            }
            let f = &self.fighters[i];
            let mut mine: Vec<PendingCapsule> = (0..self.capsule_spots[i].len().min(MAX_CAPSULES_PER_PLAYER))
                .filter(|&k| f.spot_opened & (1 << k) != 0 && f.spot_wait[k] > 0)
                .filter(|&k| !self.capsules.iter().any(|c| c.owner == i && c.pos.distance(self.capsule_spots[i][k]) < 1.0))
                .map(|k| PendingCapsule { owner: i, spot: k, pos: self.capsule_spots[i][k], elapsed: wait.saturating_sub(f.spot_wait[k]), wait })
                .collect();
            mine.sort_by_key(|p| std::cmp::Reverse(p.elapsed));
            mine.truncate(f.arsenal.len());
            out.extend(mine);
        }
        out
    }

    /// Index (in `capsules`) of one of player `i`'s capsules close enough to capture.
    pub fn capsule_in_reach(&self, i: usize, player: &Player) -> Option<usize> {
        self.capsules.iter().position(|c| c.owner == i && c.appear == 0 && in_pickup_reach(c.pos, player.pos))
    }

    fn requirement_met(&self, i: usize, def: &SkillDef) -> bool {
        let f = &self.fighters[i];
        let n = def.requirement_amount;
        match def.requirement {
            1 => f.health <= n,
            2 => f.health >= n,
            // "N or fewer skills left": `FUN_140262470` case 5 refuses when (30 - dealt) +
            // `DAT_140585c20[player]` > N. The second term counts the player's capsule objects not
            // retired (lying, appearing, waiting to regenerate: `FUN_14014f320`'s tail loop, every slot
            // whose state is not 3); the first is the undealt deck.
            5 => {
                let lying = self.capsules.iter().filter(|c| c.owner == i).count();
                let waiting = self.pending_capsules().iter().filter(|p| p.owner == i).count();
                (f.arsenal.len() + lying + waiting) as i32 <= n
            }
            7 => f.aura() >= f.level,
            8 => f.level <= n,
            9 => f.level >= n,
            _ => true,
        }
    }

    /// Player `i` presses the button `slot` (the program's `0x1402517c0` ->
    /// `0x140258180`). What the press checks, in the program's order:
    ///
    /// 1. the skill is usable at all (Aura covers the cost, Level covers the
    ///    Level penalty, the requirement holds, the environment allows it):
    ///    a failure says no (`Denied`);
    /// 2. the body is in a state that may cast (`0x14024fce0`, `cast_allowed`):
    ///    a refusal is silent and starts nothing -- a cast never cancels a
    ///    dodge, a landing or a wind-up.
    ///
    /// Nothing is paid here: the Aura, Level, Health and use are paid at the
    /// fire tick (`settle`), so a hit before it costs nothing.
    pub fn cast(&mut self, i: usize, slot: usize, players: &mut [Player]) {
        // An ordinary cast ends any chain (`FUN_14024fb90` sets the counter to 0 on its non-combo
        // branch, which is also the only way a press outside a cast is accepted).
        self.fighters[i].chain_count = 0;
        self.cast_from(i, slot, players, false);
    }

    /// Trigger ("Forces target to use equipped skill"): the same cast with no
    /// body-state check (`0x140261e90` with its "forced" argument skips it).
    pub fn cast_forced(&mut self, i: usize, slot: usize, players: &mut [Player]) {
        self.cast_from(i, slot, players, true);
    }

    /// `0x14024fce0`: whether the body may start a cast of a skill whose air
    /// flag (s40) is `s40`: 0 only on the ground (states 0, 1, 8), 1 only in
    /// the air (5, 7), 2 and 3 on the ground or in the air (mask `0x1a3`);
    /// anything else (3 dodge, 4 jump wind-up, 6 landing...) never. Every
    /// skill of the table is s40 0 or 2.
    pub fn cast_allowed(&self, s40: i16, body: BodyState) -> bool {
        match (s40, body) {
            (_, BodyState::Busy) => false,
            (0, b) => b == BodyState::Ground,
            (1, b) => b == BodyState::Air,
            _ => true,
        }
    }

    /// Where a cast turns the body to (`0x140258180`). With a locked target
    /// an attack aims at it (a short-range one only within the dash band, and
    /// it keeps re-reading the target); with none an attack and every defence
    /// aim at the camera's yaw; erase / status / special / environment casts,
    /// mines and holes leave the facing alone. The flag says the aim is a
    /// locked target (its shots home on it).
    fn capture_aim(&self, i: usize, def: &SkillDef, victim: usize, players: &[Player]) -> (Aim, bool) {
        let (s43, motion) = (def.raw[43], def.motion());
        let short = s43 == 0 && matches!(motion, 3 | 12 | 13 | 14);
        let no_aim = s43 == 0 && (motion == moves::motion::MINE || motion == moves::motion::HOLE_B || (motion == 4 && def.param(0) == 2));
        let p = &players[i];
        let tracked = s43 == 0 && !no_aim && (p.locked || p.cpu_aim) && (!short || self.dash_eligible(i, victim, players));
        if tracked {
            let bearing = yaw_of(toward(p.pos, players[victim].target_point()));
            (if short { Aim::Track(victim) } else { Aim::Yaw(bearing) }, true)
        } else if no_aim || !(s43 == 0 || s43 == 2) {
            (Aim::Keep, false)
        } else {
            (Aim::Yaw(p.cam_yaw), false)
        }
    }

    fn cast_from(&mut self, i: usize, slot: usize, players: &mut [Player], forced: bool) {
        let Some(id) = self.fighters[i].slots[slot] else { return };
        let Some(def) = self.skill(id).cloned() else { return };
        // Passive skills (automatic Shelters, Orb, Amulet) work from the slot; pressing does nothing.
        if def.passive() {
            return;
        }
        // Uses and per-use cost belong to the skill on the button (for
        // Recollection / Copy: theirs, not the copied skill's).
        let (uses, retained, spent) = (def.uses(), def.retained(), self.fighters[i].spent[slot]);
        let base_cost = def.cost + def.cost_per_use() * spent as i32;
        // Recollection / Copy (s43 = 6; s44 0 = your last attack, 1 = the last
        // attack that hurt you): cast as that skill, paid at their own cost
        // and judged by the environment as the copied skill.
        let def = if def.capsule == Capsule::Attack && def.raw[43] == 6 {
            let f = &self.fighters[i];
            let copied = if def.raw[44] == 1 { f.last_hurt_by } else { f.last_attack };
            match copied.and_then(|c| self.skill(c)).cloned() {
                Some(c) => SkillDef { cost: base_cost, level_penalty: def.level_penalty, health_cost: def.health_cost, ..c },
                // Nothing remembered: `FUN_140261e90` hands `FUN_140258180` the index -1, which plays
                // `FUN_140276330(0, 2, 2, ...)` = common/2 (our `Denied`) and starts nothing.
                None => {
                    self.log(CombatEvent::Denied { who: i, slot });
                    return;
                }
            }
        } else {
            SkillDef { cost: base_cost, ..def }
        };
        let env = self.env_def().cloned();
        let cost = env.as_ref().map_or(def.cost, |e| environment::cost(e, &def, def.cost)).max(0);
        if env.as_ref().is_some_and(|e| !environment::allowed(e, &def)) {
            return;
        }
        if self.fighters[i].aura() < cost {
            self.fighters[i].denied_ticks[slot] = DENIED_FLASH_TICKS;
            self.log(CombatEvent::Denied { who: i, slot });
            return;
        }
        // Level must cover the Level penalty (`FUN_1402621f0`: Level >= s23; -1
        // is the "Level drops to the minimum" kind and asks nothing).
        if def.level_penalty > 0 && self.fighters[i].level < def.level_penalty {
            self.log(CombatEvent::Denied { who: i, slot });
            return;
        }
        if !self.requirement_met(i, &def) {
            return;
        }
        // [Object] ("Requires: Available object") is NOT a press gate: the original casts Psycho Kinesis
        // and Power with no object in reach exactly like any cast (LIVE takes 018 / 017: state 10 for the
        // whole clip, the fire event at press + 10, Psycho Kinesis's 2 Aura paid there) and the creator
        // `FUN_140268ee0` just finds nothing to throw; `fire` below makes no shot in that case.
        // The body must be in a state that may cast (silent, like the
        // program's "not now" flag).
        if !forced && !self.cast_allowed(def.raw[40], players[i].body_state()) {
            return;
        }
        self.fighters[i].last_slot = slot;
        {
            let f = &mut self.fighters[i];
            f.cast_clock = 0;
            f.chain_buffer = None;
            f.chain_press = None;
        }
        let id = def.id;
        let behaviour = moves::behaviour_of(&def);
        let victim = self.aim_target(i, players);
        // The action row's own timing (docs/FORMATS-SKILLS.md "Action table"):
        // which row (ground/mid-air), its fire tick and its clip's length.
        // Read before any mutable borrow of `self`/`players` below.
        let airborne = self.airborne[i];
        let row = self.cast_row(i, id).cloned();
        let clip_ticks = self.cast_hold_ticks(i, row.as_ref());
        let fire_ticks = match (&row, &behaviour) {
            // READ `FUN_14025a3e0` sets the countdown (`+0x64`) to w43 at the press; `FUN_14025c720` runs
            // in the NEXT program tick's state update and fires when it is below 1 (after counting down
            // while the clip lasts). So a row that names a real clip and has w43 = 0 (Dash, row 294)
            // fires one program tick after the press, not in it: LIVE takes_full_trace `240_Dash`:
            // press tick 39, fire tick 40, the first step of the dash on tick 40. Rows with w43 = 1 do
            // the same (one update). Clip-0 rows never get here (`instant`).
            (Some(r), _) => moves::table_ticks(r.fire_tick.max(1)),
            (None, Behaviour::Guard(_)) => 0,
            (None, _) => CAST_TICKS,
        };
        let dash_ok = self.dash_eligible(i, victim, players);
        // Aim captured now, followed at the turn rate for the whole cast
        // (`0x140258180`, `0x14025bd40`): no snap to the opponent.
        let (aim, aim_locked) = self.capture_aim(i, &def, victim, players);
        // READ `FUN_140258180` (the same branch that stores the bearing): with a locked target the press also
        // stores `+0x8c` = atan2(caster y + centre height - target point y, horizontal distance to the target
        // point), the pitch `FUN_1402514c0` tips the fire matrix by (`moves::Launch::pitch`).
        let fire_pitch = if aim_locked {
            let (me, t) = (players[i].pos, players[victim].target_point());
            let ty = t.y + players[victim].body.point_height;
            (me.y + moves::HIT_BODY_CENTRE - ty).atan2(Vec3::new(t.x - me.x, 0.0, t.z - me.z).length())
        } else {
            0.0
        };
        // A row that names clip 0 (`WAIT0`) enters no cast state: `0x14025a3e0` returns 0 and
        // `0x140258180` fires the skill in the press itself (`0x1402626d0`), whatever the
        // row's fire tick says (`FUN_14025c720`'s countdown only runs inside state 10 / 11).
        // Optical Camo, Jump, Retreat, Skywalk, Return, Levitate, Surprise Attack, Void, Demon's
        // Wing, Charge. A hold-to-charge or short-range cast always has a real clip.
        let instant = row.as_ref().is_some_and(ActionRow::fires_at_once) && !def.charged() && !matches!(behaviour, Behaviour::Melee(_) | Behaviour::AuraParticle);
        let air_cast = airborne && matches!(def.raw[40], 1..=3) && !instant;
        {
            let me = &mut players[i];
            // A cast from a locked strafe drops the pivot; a cast that leaves
            // the facing alone takes the heading first (`0x140258585`).
            if !matches!(behaviour, Behaviour::AuraParticle) && !instant {
                me.end_pivot(aim == Aim::Keep);
            }
            if air_cast {
                // State 11: the body is held where it is.
                me.vel = Vec3::ZERO;
            }
        }
        let bill = Bill {
            slot,
            button_skill: self.fighters[i].slots[slot].unwrap_or(id),
            id,
            cost,
            level_penalty: def.level_penalty,
            health_cost: def.health_cost,
            uses,
            retained,
            attack: def.capsule == Capsule::Attack,
            auto_capture: def.capsule == Capsule::Attack && def.raw[51] == AUTO_CAPTURE,
            finisher: def.id == FINISHER,
            spent_before: spent,
            defer_use: (def.raw[43] == 0 && def.motion() == moves::motion::MINE) || (def.raw[43] == 4 && def.raw[45] == RELEARN_KIND),
        };
        // The Aura Particle (program record 0) starts no cast and holds nobody: its row
        // names clip 0 (`WAIT0`), so `0x14025a3e0` returns 0 and `0x140258180` fires it at
        // once (`0x1402626d0`) without leaving the idle / walk states. Confirmed live on the
        // PC original (moverec, four runs, stick held forward through the use: 1.85 u per
        // program tick before, at and after the press, state stayed "run", Level 3 -> 4,
        // sound 89). The cost and use are paid now (fire tick 0); the fighter keeps moving.
        if matches!(behaviour, Behaviour::AuraParticle) {
            self.fighters[i].bill = Some(bill);
            self.settle(i);
            let f = &mut self.fighters[i];
            f.level = (f.level + 1).min(f.level_max);
            f.absorb_glow_ticks = AURA_GLOW_TICKS;
            // Its record is status kind 1 (Level), amount +1: the effect's own sound
            // (`FUN_140270b00` -> `FUN_140271590`, common/89).
            self.log(CombatEvent::StatusEffect { who: i, kind: 1, amount: 1 });
            // Marks the use for the green body glow (aura_pulse.rs); silent in sfx.rs (live trace: only 89).
            self.log(CombatEvent::AuraParticleUsed { who: i });
            return;
        }
        // An instant cast: no state, no hold, no turn toward the aim; the cast sound and effect
        // still start (`cast_skill` / `cast_seq`) and the skill fires in this very press. Level
        // and Aura are paid by `fire` -> `settle`, as for every cast (`FUN_1402621f0`).
        if instant {
            let f = &mut self.fighters[i];
            f.bill = Some(bill);
            f.aim = aim;
            f.aim_locked = aim_locked;
            f.fire_pitch = fire_pitch;
            f.air_cast = false;
            f.pending_shot = None;
            f.cast_skill = id;
            f.cast_seq = f.cast_seq.wrapping_add(1);
            f.struck = false;
            self.instant_fires.push((i, id));
            return;
        }
        let f = &mut self.fighters[i];
        f.bill = Some(bill);
        f.aim = aim;
        f.aim_locked = aim_locked;
        f.fire_pitch = fire_pitch;
        f.air_cast = air_cast;
        f.pending_shot = matches!(behaviour, Behaviour::Shoot(_) | Behaviour::Place(_) | Behaviour::Effect(_) | Behaviour::Guard(_) | Behaviour::NotBuilt).then_some(def.id);
        f.cast_skill = id;
        f.cast_seq = f.cast_seq.wrapping_add(1);
        f.action = Action::Cast;
        // The whole cast clip holds the caster (item 2); the row's own fire
        // tick replaces the old flat `CAST_TICKS` (item 1). Never let the
        // clip end before the fire tick even if the clip-length table is
        // missing/wrong for this clip.
        f.action_ticks = clip_ticks.max(fire_ticks);
        f.fire_ticks = fire_ticks;
        f.cast_elapsed = 0;
        f.struck = false;
        f.dash_step = 0.0;
        f.last_blade = None;
        f.melee = false;
        // The cast clip's root motion moves the body from the press (hittiming lane): every action
        // row has w1 = 1 (`FUN_14025a3e0` passes it as the clip start's root flag, as for the
        // reaction clips), so the body drifts along the clip's root track -- ATK_WHAND_SORD pulls
        // back 1 u then strides 26 u forward, ATK_SHOT steps back 11 u. LIVE: the original's
        // takes_full traces show exactly this curve under every cast (Psycho Blade: -0.26 u at
        // press + 1, the dash then 7.2 u a tick = 6.15 dash + ~1 u of stride; Blaster: P0 pushed
        // 8 u back after the fire tick).
        f.root = row.as_ref().filter(|r| !r.anim_name.is_empty()).map(|r| (intern_clip(&r.anim_name), 0.0));
        match behaviour {
            Behaviour::AuraParticle => unreachable!("returned above: the Aura Particle starts no cast"),
            // A defence holds its caster for the cast clip only, like any
            // skill; the guard object then stands on its own (and follows the
            // holder) for s46 - 1 program ticks (`0x140264020`/`0x140264340`).
            Behaviour::Guard(_) => {}
            Behaviour::Melee(_) if def.charged() => {
                f.melee = true;
            }
            // Short-range: dash at the target (only when locked on within
            // `MELEE_DASH_RANGE`, PROGRAM-RE.md "Lock-on"), steer toward it
            // and strike inside the row's hit window whether or not the dash
            // arrived (`lunge`).
            Behaviour::Melee(_) => {
                f.action = Action::Lunge;
                f.melee = true;
                f.dash_eligible = dash_ok;
            }
            Behaviour::Shoot(_) | Behaviour::Place(_) | Behaviour::Effect(_) | Behaviour::NotBuilt => {}
        }
        // "Hold button to charge": the cast holds while the button is down
        // (`charge_step` grows the charge), then goes off on release.
        let f = &mut self.fighters[i];
        f.charge = def.charged().then_some(Charge { bit: 0, spent: 0, ticks: 0, count: 0 });
        if f.charge.is_some() {
            f.action = Action::Cast;
            f.action_ticks = u16::MAX;
        }
    }

    /// The combo gate, once per tick of a cast (READ `FUN_14024fb90`, called through
    /// `FUN_140261e90` from the button loop `FUN_1402517c0`, which runs LAST in the player update
    /// `FUN_14024ec80`, after the state handler and the clip advance): the clip frame is the program
    /// ticks since the cast began (`cast_clock / 2`; the gate sees it AFTER this tick's advance).
    ///
    /// - A press (an edge) of a button whose skill's program index is in the casting skill's
    ///   s27..s36 is BUFFERED (`+0x61f4` = the button) while `0 < frame` and the frame is in the
    ///   current row's w45..w46. One button is held in the buffer; a press of another button
    ///   while one is buffered is ignored. The buffer is not a timer: it stays until the chain
    ///   starts or the cast ends.
    /// - When the frame equals w47 and a button is buffered (or pressed), the chained cast starts
    ///   NOW: counter + 1, the next of s14..s17 as its row (`cast_row`), a fresh cast of the same
    ///   button's skill (`FUN_140258180`: pays at its fire like any cast, so a chain pays Aura,
    ///   Level and a use again for every hit; Aura / Level short = the usual Denied and no chain).
    ///   The body-state check is the non-combo branch's, so it is skipped.
    /// - The program runs this once per program tick (30 Hz); a press that lands on the other
    ///   sim tick waits for the next program tick (`chain_press`).
    fn chain_step(&mut self, i: usize, players: &mut [Player], buttons: u8, prev: u8, col: &Collision) {
        let f = &mut self.fighters[i];
        if !matches!(f.action, Action::Cast | Action::Lunge) || f.charge.is_some() {
            f.chain_buffer = None;
            f.chain_press = None;
            return;
        }
        f.cast_clock = f.cast_clock.saturating_add(1);
        if f.chain_press.is_none() {
            if let Some(button) = (0..SLOTS).find(|&k| pressed(buttons, prev, k)) {
                let st = &f.status;
                let slot = if st.scrambled_ticks > 0 { st.scramble[button] as usize % SLOTS } else { button };
                let refused = (st.locked_ticks > 0 && st.locked_slot as usize == slot) || f.frozen_ticks > 0;
                if refused {
                    self.log(CombatEvent::Denied { who: i, slot });
                } else {
                    self.fighters[i].chain_press = Some(slot as u8);
                }
            }
        }
        let f = &self.fighters[i];
        if f.cast_clock % 2 != 0 {
            return;
        }
        let frame = (f.cast_clock / 2) as i16;
        let press = f.chain_press;
        let skill = f.cast_skill;
        let win = self.cast_window(i, skill);
        let Some(list) = self.skill(skill).map(|d| <[i16; 10]>::try_from(&d.raw[27..37]).unwrap_or([0; 10])) else { return };
        self.fighters[i].chain_press = None;
        let in_window = frame > 0 && win.from <= frame && frame <= win.to;
        let mut start = None;
        for slot in 0..SLOTS {
            let f = &self.fighters[i];
            if press != Some(slot as u8) && f.chain_buffer != Some(slot as u8) {
                continue;
            }
            let Some(pidx) = f.slots[slot].and_then(|id| self.skill(id)).filter(|d| !d.passive()).map(|d| d.raw[2]) else { continue };
            if pidx == 0 || !list.contains(&pidx) {
                continue;
            }
            if in_window && self.fighters[i].chain_buffer.is_none() {
                self.fighters[i].chain_buffer = Some(slot as u8);
            }
            if self.fighters[i].chain_buffer.is_none() || frame != win.at {
                continue;
            }
            start = Some(slot);
            break;
        }
        let Some(slot) = start else { return };
        let f = &mut self.fighters[i];
        f.chain_buffer = None;
        let (before, seq) = (f.chain_count, f.cast_seq);
        f.chain_count = before.saturating_add(1);
        self.cast_from(i, slot, players, true);
        self.fire_instants(players, col);
        // Refused (Aura / Level / requirement): no new cast, no chain step.
        if self.fighters[i].cast_seq == seq {
            self.fighters[i].chain_count = before;
        }
    }

    /// Pays for the cast at its fire tick (`FUN_1402621f0`, called from
    /// `FUN_1402626d0` before anything is created): Aura >= cost and Level >=
    /// Level penalty, else nothing leaves and nothing is lost. Then the
    /// poison and Backdraft Health loss (each can kill), the Aura, the Health
    /// cost and the Level penalty, the use, and Auto-Capture. A "Level reduced
    /// to the minimum" skill (s23 = -1: Finisher, Rebirth) pays nothing else.
    fn settle(&mut self, i: usize) -> bool {
        let Some(b) = self.fighters[i].bill.take() else { return true };
        if !self.can_pay(i, &b) {
            return false;
        }
        // Poison: "each time the opponent tries to use a skill ... damage of 1";
        // Backdraft: "Causes everyone to lose 1 Health for each skill use".
        if self.fighters[i].poison_ticks > 0 {
            self.change_health(i, -1);
        }
        let lose = self.env_def().map_or(0, environment::health_on_use);
        if lose > 0 {
            self.change_health(i, -lose);
        }
        if self.fighters[i].action == Action::Down {
            return true;
        }
        let f = &mut self.fighters[i];
        // Finisher: "STR = double your Level" -- the Level before its penalty.
        if b.finisher {
            f.cast_strength = Some(2 * f.level);
        }
        self.pay(i, &b);
        // One roll per cast (`FUN_1402626d0` calls `FUN_140272400` ONCE and hands the number to every object the cast makes;
        // rules 27-29 are `FUN_140274a10`: s46 x (rand x (rule - 25) >> 15), so rule 28 is 0, s46 or 2 x s46): both holes of
        // Jack Pot take the same 3 (LIVE `319_Jack_Pot`: P0 -3 and P1 -3), not a draw each.
        if self.skill(b.id).is_some_and(|d| d.capsule == Capsule::Attack && d.strength_rule == str_rule::RANDOM) {
            let s = self.skill(b.id).map_or(0, |d| d.strength);
            let roll = self.random(3) as i32;
            self.fighters[i].cast_strength = Some(s * roll);
        }
        let f = &mut self.fighters[i];
        if b.attack {
            f.last_attack = Some(b.id);
        }
        if !b.defer_use {
            self.consume_use(i, &b);
        } else if self.skill(b.id).is_some_and(|d| d.raw[43] == 0 && d.motion() == moves::motion::MINE) {
            // A mine is used up when it goes off, not now.
            let area = self.next_id + 1;
            self.pending_mines.push((area, i, b));
        }
        true
    }

    /// Takes one use off the button a skill was cast from (`FUN_140263410`,
    /// then the exhaustion test `FUN_140263460`): the skill stays on its
    /// button until its uses (s26) run out; under Memory Lapse (status 32) any
    /// use empties it unless it "can't be erased" (s41 = 2). A last use is
    /// kept by Retain (s41 = 1): if Aura still covers it the cast is paid again
    /// right now and the skill stays with one use left; Necronomicon (status
    /// 31) switches that off for attack skills. Used-up skills are "consumed"
    /// (Recall, Relearn); Auto-Capture (s51 = 18) fills the button from the
    /// arsenal.
    fn consume_use(&mut self, i: usize, b: &Bill) {
        let lapse = self.env_status(32) != 0;
        let necro = self.env_status(31) != 0;
        let protected = self.skill(b.button_skill).is_some_and(SkillDef::protected);
        let f = &mut self.fighters[i];
        if f.slots[b.slot] != Some(b.button_skill) || self.rules.infinite_uses {
            return;
        }
        f.spent[b.slot] = f.spent[b.slot].saturating_add(1);
        let exhausted = b.uses.is_some_and(|n| f.spent[b.slot] >= n) || (lapse && !protected);
        if exhausted {
            let again = b.retained && !(b.attack && necro) && self.can_pay(i, b);
            if again {
                // "Retain if you have enough Aura to use again": paid again now.
                self.pay(i, b);
                let f = &mut self.fighters[i];
                f.spent[b.slot] = b.uses.map_or(0, |n| n.saturating_sub(1));
            } else {
                let f = &mut self.fighters[i];
                f.set_slot(b.slot, None);
                f.used.push(b.id);
            }
        }
        let f = &mut self.fighters[i];
        // Auto-Capture (s51 = 18: Empty Hand, Hyperpunch, Sky Pursuit): "you
        // can capture the next skill even if a capsule has not spawned" --
        // "regardless of the success or failure of the skill hitting the
        // target, when the skill is used" (guide). The next skill of your
        // arsenal goes straight onto the button.
        if b.auto_capture && !necro && f.slots[b.slot].is_none() {
            let next = f.arsenal.pop();
            f.set_slot(b.slot, next);
            if let Some(skill) = next {
                // LIVE (Empty Hand, Hyperpunch, Sky Pursuit takes): `capture:` + common/86 at the fire tick.
                self.log(CombatEvent::SkillCaptured { who: i, slot: b.slot, skill });
            }
        }
    }

    /// Can the fighter pay what `b` costs right now (`FUN_1402620e0` /
    /// `FUN_1402621f0`'s gate): Aura and Level.
    fn can_pay(&self, i: usize, b: &Bill) -> bool {
        let f = &self.fighters[i];
        b.level_penalty < 0 || (f.aura_ticks >= b.cost * AURA_TICKS_PER_POINT && f.level >= b.level_penalty)
    }

    /// The payment itself (`FUN_1402621f0`): Aura, Health, Level. A "Level
    /// reduced to the minimum" skill (s23 = -1: Finisher, Rebirth) pays nothing else.
    fn pay(&mut self, i: usize, b: &Bill) {
        let min_level = self.fighters[i].level_min;
        if b.level_penalty < 0 {
            let f = &mut self.fighters[i];
            f.level = min_level.min(f.level);
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
        } else {
            self.fighters[i].aura_ticks -= b.cost * AURA_TICKS_PER_POINT;
            self.change_health(i, -b.health_cost);
            let f = &mut self.fighters[i];
            f.level = (f.level - b.level_penalty).max(f.level_min);
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
        }
    }

    /// A mine that has gone off (or is otherwise gone) takes its use now
    /// (`FUN_14026cd70` code 8 -> `FUN_1402635b0`).
    fn settle_mines(&mut self) {
        if self.pending_mines.is_empty() {
            return;
        }
        let gone: Vec<(u32, usize, Bill)> = self.pending_mines.iter().filter(|(id, ..)| !self.areas.iter().any(|a| a.id == *id)).copied().collect();
        self.pending_mines.retain(|(id, ..)| self.areas.iter().any(|a| a.id == *id));
        for (_, i, b) in gone {
            self.consume_use(i, &b);
        }
    }

    /// One tick of a held charge (Gungnir, Ki Cannon, Ki Palm, Young White
    /// Dragon; `0x14025c720`): after the row's wind-up (`charge_cycle`) the
    /// charge grows one step per program tick while the button is held and
    /// `count / 10` more Aura than the cost is still in hand; nothing is paid
    /// until the release (`settle`). The body keeps turning toward its aim.
    fn charge_step(&mut self, i: usize, players: &mut [Player], buttons: u8) {
        let skill = self.fighters[i].cast_skill;
        let wind = self.cast_row(i, skill).map_or(0, |r| moves::table_ticks(r.charge_cycle));
        let base = self.fighters[i].bill.map_or(0, |b| b.cost);
        let f = &mut self.fighters[i];
        let aim = f.aim;
        let Some(ch) = &mut f.charge else { return };
        let pressed = buttons & ch.bit != 0;
        // A release inside the wind-up is not acted on until the row's charge cycle has run: LIVE
        // (takes_full `073_Gungnir`, `204_Ki_Cannon`, `195_Ki_Palm`: the button is down 3 ticks, yet
        // the shot / palm fires at press + 25 / 25 / 20 table ticks = charge_cycle (13 / 13 / 7)
        // + the row's remaining fire_tick - charge_cycle, and the charge effect still starts on
        // the charge-cycle tick, 13 ticks after the press).
        if pressed || ch.ticks < wind {
            ch.ticks = ch.ticks.saturating_add(1);
            // The row's charge-cycle tick: the slot-4 charge effect starts (`FUN_140259540`).
            let held = ch.ticks == wind.max(1);
            if pressed && ch.ticks > wind && (ch.ticks - wind) % moves::SIM_TICKS_PER_TABLE_TICK == 0 {
                ch.count += 1;
                if f.aura_ticks < (base + (ch.count / 10) as i32) * AURA_TICKS_PER_POINT {
                    ch.count -= 1;
                }
            }
            ch.spent = (ch.count / 10) as i32;
            cast_turn(aim, players, i);
            if held {
                self.log(CombatEvent::ChargeHeld { who: i, skill });
            }
            return;
        }
        self.release_charge(i, players);
        cast_turn(aim, players, i);
    }

    /// A charge ends (button released): STR = the Aura it will cost beyond the
    /// skill's own; a short-range charge dashes in now (its own dash/hit
    /// windows from the row, judged fresh at release), a shot leaves
    /// `fire_tick - charge_cycle` ticks after release (phase 3: 12 ticks for
    /// Gungnir/Ki Cannon/...). The Aura is paid when it fires.
    fn release_charge(&mut self, i: usize, players: &[Player]) {
        let skill = self.fighters[i].cast_skill;
        let Some(ch) = self.fighters[i].charge.take() else { return };
        self.log(CombatEvent::ChargeReleased { who: i, skill });
        let row = self.cast_row(i, skill).cloned();
        let clip_ticks = self.clip_hold_ticks(i, row.as_ref().map_or("", |r| r.anim_name.as_str()));
        let remaining = row.as_ref().map_or(0, |r| (r.fire_tick - r.charge_cycle).max(0));
        let wind_ticks = row.as_ref().map_or(0, |r| moves::table_ticks(r.charge_cycle));
        let fire_ticks = if row.is_some() { moves::table_ticks(remaining) } else { CAST_TICKS };
        let melee = matches!(self.skill(skill).map(moves::behaviour_of), Some(Behaviour::Melee(_)));
        let victim = self.aim_target(i, players);
        let dash_ok = melee.then(|| self.dash_eligible(i, victim, players)).unwrap_or(false);
        let f = &mut self.fighters[i];
        f.cast_strength = Some(ch.spent);
        if let Some(b) = &mut f.bill {
            b.cost += ch.spent;
        }
        // A shot's cast goes on from the clip's charge-cycle frame: the whole cast is still the row's clip + 1 program tick from the
        // press, however soon the button came up (LIVE `073_Gungnir`, `204_Ki_Cannon`: press 39, up at 42, fire 64, state 10 until 86 =
        // press + 46 + 1; before, ours restarted the full clip at the release and held to press + 60). A short-range charge keeps its
        // own run (`lunge`, Ki Palm already matches: ends press + 41).
        f.action_ticks = if melee { clip_ticks.max(fire_ticks) } else { (clip_ticks + moves::SIM_TICKS_PER_TABLE_TICK).saturating_sub(wind_ticks).max(fire_ticks) };
        f.fire_ticks = fire_ticks;
        // The cast resumes at the clip's charge-cycle point: a short-range charge's dash and hit
        // windows are the row's own, counted from the press (LIVE `195_Ki_Palm`: fires at press + 20
        // table ticks = the row's fire tick, with a 7-tick charge cycle and a 3-tick press).
        f.cast_elapsed = if melee { row.as_ref().map_or(0, |r| moves::table_ticks(r.charge_cycle)) } else { 0 };
        f.struck = false;
        f.dash_step = 0.0;
        f.last_blade = None;
        if melee {
            f.action = Action::Lunge;
            f.dash_eligible = dash_ok;
        }
    }

    /// The end of a cast: launch the skill's projectiles, or set its area.
    fn fire(&mut self, i: usize, skill: i16, players: &mut [Player], col: &Collision) {
        let Some(def) = self.skill(skill).cloned() else { return };
        // Pay first: a cast that can't pay at its fire tick leaves nothing
        // (`FUN_1402626d0` returns before creating anything).
        let uses_before = self.fighters[i].bill.map_or(0, |b| b.spent_before);
        if !self.settle(i) {
            return;
        }
        self.log(CombatEvent::SkillFired { who: i, skill });
        let victim = self.aim_target(i, players);
        let caster = players[i].pos;
        // The program's fire matrix (`FUN_1402626d0`): the shot is made at the
        // caster's chest facing where the body faces; the locked target's body
        // centre is what it aims and homes at (`FUN_14026e410`), and with
        // nothing locked there is no target -- the aimed families head for a
        // point 190 u (s44 = 0) / 300 u ahead, the shells fly straight.
        let marks = self.shot_marks(i, skill, players);
        let ctx = self.launch_ctx(i, victim, players, col, &marks);
        match moves::behaviour_of(&def) {
            Behaviour::Effect(d) => self.take_effect(i, victim, d, &def, players, col),
            Behaviour::Guard(guard) => {
                let button = self.fighters[i].last_slot;
                self.fighters[i].guard = Some(ActiveGuard::new(skill, guard.with_uses(&def, uses_before), button));
            }
            Behaviour::Shoot(kind) => {
                // Homing accuracy (Acquisition, Magnetic Field, Dense Fog...).
                let homing = if ctx.target.is_some() { (100 + self.stat(i, Stat::Homing)).max(0) as f32 / 100.0 } else { 0.0 };
                let strength = self.fighters[i].cast_strength.take();
                let button = self.fighters[i].last_slot;
                // [Object]: the nearest object in reach flies from where it stood
                // (none left by now, e.g. it was broken during the cast: nothing).
                if def.motion() == moves::motion::OBJECT {
                    let Some(k) = props::nearest_throwable(&self.props, caster) else { return };
                    self.props[k].state = PropState::Thrown;
                    let str = self.props[k].throw_strength();
                    // Lifted clear of its spot (s68 x 0.1 u over s69 ticks, `FUN_140268ee0`), held, then thrown.
                    let prop = &self.props[k];
                    let (lift, hang) = (def.param(4).max(0) as f32 * moves::TABLE_DIST, def.param(5).max(0) as u16 * moves::SIM_TICKS_PER_TABLE_TICK);
                    let (lift, hang) = if hang > 0 { (lift, hang) } else { (props::THROW_LIFT, props::THROW_HANG_TICKS) };
                    let from = Vec3::new(prop.centre().x, prop.max.y + lift, prop.centre().z);
                    let (aim, _) = ctx.aim(&def, false);
                    let thrown = Launch { spawn: from, facing: (aim - from).normalize_or(ctx.facing), pitch: 0.0, marks: &[], ..ctx };
                    for mut shot in moves::launch_from(kind, &def, &thrown) {
                        shot.homing *= homing;
                        shot.delay = shot.delay.max(hang);
                        let id = self.take_id();
                        self.projectiles.push(Projectile { id, owner: i, target: victim, skill, pos: shot.pos, prev_pos: shot.pos, shot, turned: false, harmless: false, button, prop: Some(k), brushed: false, strength: Some(str) });
                    }
                    return;
                }
                for mut shot in moves::launch_from(kind, &def, &ctx) {
                    shot.homing *= homing;
                    let id = self.take_id();
                    self.projectiles.push(Projectile { id, owner: i, target: victim, skill, pos: shot.pos, prev_pos: shot.pos, shot, turned: false, harmless: false, button, prop: None, brushed: false, strength });
                }
            }
            Behaviour::Place(spec) => self.place_area(i, victim, skill, &def, spec, &ctx, players, col),
            _ => {}
        }
    }

    /// The launch context of player `i`'s cast (`moves::Launch`). `marks` =
    /// `shot_marks`: where the objects start (the first marker / the hand, the
    /// chest when there is no data). Families that start on the floor use the
    /// marker's place on the ground.
    fn launch_ctx<'a>(&self, i: usize, victim: usize, players: &[Player], col: &'a Collision, marks: &'a [Vec3]) -> Launch<'a> {
        let f = &self.fighters[i];
        let yaw = players[i].yaw;
        let homes = f.aim == Aim::Keep || f.aim_locked;
        let feet = players[i].pos;
        let (spawn, feet) = match marks.first() {
            Some(m) => (*m, Vec3::new(m.x, feet.y, m.z)),
            None => (feet + Vec3::Y * CHEST, feet),
        };
        Launch {
            spawn,
            feet,
            facing: Vec3::new(yaw.sin(), 0.0, yaw.cos()),
            pitch: if f.aim_locked { f.fire_pitch } else { 0.0 },
            target: homes.then(|| body_centre(players[victim].target_point())),
            index: 0,
            col: Some(col),
            marks,
        }
    }

    /// Sets a Hole / Rain / mine / Photon Wave where it goes (`moves/area.rs`).
    #[allow(clippy::too_many_arguments)]
    fn place_area(&mut self, i: usize, victim: usize, skill: i16, def: &SkillDef, spec: AreaSpec, ctx: &Launch, players: &[Player], col: &Collision) {
        let caster = players[i].pos;
        let strength = self.fighters[i].cast_strength.take();
        match spec.kind {
            // "Explodes around self": a blast at the spawn point at once, the
            // caster excluded (`FUN_14026dbd0(obj, owner)`).
            Kind::Wave => {
                let mask = self.victims_of(i).fold(0u8, |m, j| m | 1 << j);
                self.add_blast(i, skill, caster + Vec3::Y * HIT_BODY_CENTRE, spec.radius, mask, true);
            }
            // Set on the floor under the fire matrix's point -- the effect's marker or the hand at the fire
            // frame -- found within 10 u above and below it; with no floor there it stays at that point
            // (`FUN_140267b90` probes from y + 10 to y - 10 at the matrix's x / z). LIVE Sensor Mine: its own
            // blast reaches the owner 4 ticks in and the foe 50 u away 28 ticks in, so it lies ~9 u to the side.
            Kind::Mine => {
                let (px, py, pz) = if ctx.marks.is_empty() { (caster.x, caster.y, caster.z) } else { (ctx.spawn.x, ctx.spawn.y, ctx.spawn.z) };
                let at = match col.ground_below_object(px, pz, py + 10.0).filter(|g| *g >= py - 10.0) {
                    Some(y) => Vec3::new(px, y, pz),
                    None if ctx.marks.is_empty() => Vec3::new(px, caster.y, pz),
                    None => Vec3::new(px, py, pz),
                };
                let id = self.take_id();
                self.areas.push(Area { id, owner: i, target: victim, skill, pos: at, spec, timer: spec.warn, pulses_left: 1, armed: false, life: u16::MAX, detonate: false, hit: 0, age: 0, settled: false, slide: Vec3::ZERO, strength: None, cloud: Vec3::ZERO, drops: Vec::new(), first: 0, gap: 0 });
            }
            // Holes, columns and clouds are aimed at the floor under the target;
            // the unlimited Hole (motion 16) "makes one hole at every player's
            // feet" (`FUN_1402626d0` over all four players).
            Kind::Hole | Kind::RainColumn | Kind::RainCloud => {
                let everyone = def.motion() == moves::motion::HOLE_B;
                // LIVE (Tremor, Earthquake, Massive Quake, Jack Pot, Calamity: both fighters hurt / floored by the
                // caster's own hole) and read (`FUN_1402626d0` loops every living player with no owner test): the
                // unlimited Hole is made under the caster as well.
                let victims: Vec<usize> = if everyone { self.roster.slots().filter(|&j| j == i || self.victims_of(i).any(|v| v == j)).collect() } else { vec![victim] };
                for v in victims {
                    let c = Launch { target: if everyone { Some(body_centre(players[v].target_point())) } else { ctx.target }, ..*ctx };
                    let (aim, _) = c.aim(def, true);
                    // No floor under the aim point: it fails silently.
                    let Some(y) = col.ground_below_object(aim.x, aim.z, aim.y + 1.0) else { continue };
                    // The marker slides the way the target was running (their
                    // movement over a program tick = two sim ticks).
                    let run = if v == i { Vec3::ZERO } else { (players[v].pos - players[v].prev_pos) * 2.0 };
                    let at = Vec3::new(aim.x, y, aim.z);
                    let (life, cloud) = (spec.warn + spec.active, at + Vec3::Y * spec.cloud_height);
                    let id = self.take_id();
                    self.areas.push(Area { id, owner: i, target: v, skill, pos: at, spec, timer: 0, pulses_left: spec.pulses, armed: false, life, detonate: false, hit: 0, age: 0, settled: false, slide: Vec3::new(run.x, 0.0, run.z), strength, cloud, drops: Vec::new(), first: spec.warn as i16, gap: 0 });
                }
            }
        }
    }

    /// A blast starts at `pos` (it steps from the next program tick on).
    fn add_blast(&mut self, owner: usize, skill: i16, pos: Vec3, radius: f32, mask: u8, area: bool) {
        let via = Via { button: self.fighters[owner].last_slot, strength: None };
        self.add_blast_via(owner, skill, pos, radius, mask, area, via);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_blast_via(&mut self, owner: usize, skill: i16, pos: Vec3, radius: f32, mask: u8, area: bool, via: Via) {
        self.log(CombatEvent::BlastStarted { attacker: owner, pos, skill, radius, area });
        self.log(CombatEvent::Burst { attacker: owner, pos, skill });
        self.blasts.push(Blast { owner, skill, pos, radius, life: area::BLAST_TICKS, mask, area, via, age: 0, armed: false });
    }

    /// Fires the casts `cast_from` queued because their row names no clip.
    fn fire_instants(&mut self, players: &mut [Player], col: &Collision) {
        for (i, skill) in std::mem::take(&mut self.instant_fires) {
            self.fire(i, skill, players, col);
        }
    }

    /// One sim tick of every counting-down instant effect (`FUN_140265f50`: applies when the
    /// count is below 1, else counts down). The count is s65 program ticks = `2 x s65` sim
    /// ticks after the object's arming tick (its first program tick only arms it, like a shot's,
    /// `move_projectiles`), so an effect fired on sim tick F lands on F + 2 x (s65 + 1).
    fn land_delayed_effects(&mut self, players: &mut [Player]) {
        if self.delayed_effects.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.delayed_effects.retain_mut(|e| {
            if e.due == 0 {
                due.push(*e);
                false
            } else {
                e.due -= 1;
                true
            }
        });
        for e in due {
            let Some(def) = self.skill(e.skill).cloned() else { continue };
            let reach = if e.everyone { Reach::Everyone } else { Reach::Target(e.target) };
            let pressed = std::mem::replace(&mut self.fighters[e.caster].last_slot, e.button);
            let victims: Vec<usize> = if e.everyone { self.roster.slots().collect() } else { vec![e.target] };
            let before: Vec<_> = victims.iter().map(|&v| self.effect_signature(v, &players[v])).collect();
            let landed = self.land_effect(e.caster, reach, &def, players);
            self.fighters[e.caster].last_slot = pressed;
            // The hit reaction of a delayed instant (`FUN_140265f50` -> `FUN_14026f870` ->
            // `FUN_14024fd80`): every player it reaches, the caster too when it is "everyone's".
            // The s60 row follows only for a victim the effect changed (`FUN_140272bc0` returns 0 when
            // nothing changed -- Armaros with no status to reverse, Purify / Return to Nature with
            // nothing to reset, Vicious Balance at equal Levels, Judge of Ares with no defence
            // equipped -- and the callback then plays no s60 row; LIVE: those takes stay idle).
            if landed {
                for (k, &v) in victims.iter().enumerate() {
                    if !add_can_return_zero(def.raw[pd_formats::skills::field::ADD_EFFECT]) || before[k] != self.effect_signature(v, &players[v]) {
                        self.status_skill_reaction(e.caster, v, &def, players);
                    }
                }
            }
        }
    }

    /// Everything an Add / erase / status / special effect can change on a fighter: the stand-in for
    /// the program's "did the effect take hold" return value (`FUN_140272bc0`).
    fn effect_signature(&self, i: usize, player: &Player) -> impl PartialEq {
        let f = &self.fighters[i];
        (f.level, f.aura_ticks, f.health, f.slots, f.frozen_ticks, f.poison_ticks, f.status.clone(), f.forced_lock, f.status_hover, player.pos.to_array().map(f32::to_bits))
    }

    /// The effect itself: the hit event for a targeted one (unless a status blocks it), then the
    /// effect modules.
    fn land_effect(&mut self, i: usize, reach: Reach, def: &SkillDef, players: &mut [Player]) -> bool {
        if let Reach::Target(t) = reach {
            if !self.fighters[t].status_blocks(def) {
                self.log(CombatEvent::Hit { attacker: i, victim: t, skill: def.id });
            }
        }
        moves::apply_effect(self, players, i, reach, def)
    }

    /// A non-travelling erase / status / special / environment skill takes effect.
    fn take_effect(&mut self, i: usize, victim: usize, d: Delivery, def: &SkillDef, players: &mut [Player], col: &Collision) {
        let reach = match d {
            Delivery::Caster => Reach::Caster,
            Delivery::Everyone => Reach::Everyone,
            Delivery::Field => Reach::Field,
            Delivery::TargetNow | Delivery::Travels => Reach::Target(victim),
            Delivery::Environment => {
                // "Only one crystal at a time is effective ... if you try to use
                // another Environmental skill its effects will not take place."
                if self.crystal.is_none() {
                    let id = self.take_id();
                    self.crystal = Some(Crystal { id, owner: i, skill: def.id, pos: players[i].pos, age: 0 });
                    self.log(CombatEvent::CrystalSet { who: i, pos: players[i].pos });
                }
                return;
            }
        };
        // An instant skill that acts on the opponent (target kind 0, motion 4) does not land in the
        // fire tick: `FUN_140265ea0` stores s65 as the object's countdown and `FUN_140265f50`
        // applies the effect once it is spent (every player for s64 = 2). s65 is 25 program ticks
        // for most, 5 for Frighten, 0 for the crystal erasers (which land at once).
        if matches!(d, Delivery::TargetNow | Delivery::Everyone) && def.raw[43] == 0 && def.motion() == 4 && def.raw[65] > 0 {
            // The object's first program tick only arms it (`FUN_1402638d0`, state 1 -> 2), then the count runs:
            // LIVE Meltdown / Quantum Decay / Quantum Amp (s65 = 25): fire tick 69, the Level changes at 95 = fire + 26.
            let due = moves::table_ticks(def.raw[65]) + moves::SIM_TICKS_PER_TABLE_TICK;
            let button = self.fighters[i].last_slot;
            self.delayed_effects.push(DelayedEffect { due, caster: i, target: victim, skill: def.id, button, everyone: matches!(d, Delivery::Everyone) });
            return;
        }
        let before = players[i].pos;
        let _ = self.land_effect(i, reach, def, players);
        // Dash moves you along the ground, so a wall stops it (special.rs has no collision).
        if def.capsule == Capsule::Special && def.raw[43] == 3 && def.raw[44] == moves::special::move_kind::DASH {
            let to = players[i].pos - before;
            let (dir, len) = (to.normalize_or(Vec3::Z), to.length());
            let radius = players[i].body.wall_radius();
            if let Some(hit) = col.raycast(before + Vec3::Y * CHEST, dir, len + radius) {
                players[i].pos = before + dir * (hit - radius).max(0.0);
            }
        }
    }

    /// Runs a short-range cast for one tick: the dash window and the hit
    /// window both come straight from the action row (`dash_start..dash_end`,
    /// `fire_tick..hit_window_end`, table ticks converted to sim ticks). The
    /// dash only runs while `dash_eligible` (locked on, target within
    /// `MELEE_DASH_RANGE` at the press); the hit test runs every tick of its
    /// window regardless of whether the dash arrived, and lands at most once.
    /// `action` shows `Lunge` before the fire tick and `Cast` from it to the
    /// clip's end, when the caster is freed; both play the same cast-row clip
    /// and the same cast effect (slot 0) from the cast start, so the swing's
    /// authored frames land on the fire tick without any extra lining-up here
    /// (main.rs::cast_clip_now, fx.rs::skill_effects).
    fn lunge(&mut self, i: usize, players: &mut [Player], col: &Collision) {
        let id = self.fighters[i].cast_skill;
        let def = self.skill(id).cloned();
        let melee = match def.as_ref().map(moves::behaviour_of) {
            Some(Behaviour::Melee(m)) => m,
            _ => moves::melee::Melee { reach: moves::melee::DEFAULT_REACH, shape: moves::melee::Shape::Sphere, dash: 8, turn: 30 },
        };
        let row = self.cast_row(i, id).cloned();
        let fire_tick = row.as_ref().map_or(LUNGE_MAX_TICKS, |r| moves::table_ticks(r.fire_tick));
        let (dash_start, dash_end) = row.as_ref().map_or((0, LUNGE_MAX_TICKS), |r| (moves::table_ticks(r.dash_start), moves::table_ticks(r.dash_end)));
        let hit_end = row.as_ref().map_or(fire_tick.saturating_add(1), |r| moves::table_ticks(r.hit_window_end).max(fire_tick.saturating_add(1)));
        let clip_end = self.fighters[i].action_ticks.max(hit_end + moves::SIM_TICKS_PER_TABLE_TICK);

        let dash_eligible = self.fighters[i].dash_eligible;
        let already_struck = self.fighters[i].struck;
        self.fighters[i].cast_elapsed = self.fighters[i].cast_elapsed.saturating_add(1);
        let elapsed = self.fighters[i].cast_elapsed;

        let victim = self.aim_target(i, players);
        let target = players[victim].target_point();
        let aim = self.fighters[i].aim;
        // The turn (`FUN_14025be40`): through the wind-up the body comes round
        // to the target's bearing, re-read every tick, at the character's turn
        // rate; in the dash it steers toward the target by at most s66 / 10
        // degrees a program tick and moves along its own facing; once the dash
        // is over a dashing cast no longer turns. A cast with no dash (no lock,
        // or too far) just keeps turning toward its aim.
        let dash_len = (dash_end.saturating_sub(dash_start) / moves::SIM_TICKS_PER_TABLE_TICK).max(1);
        let dashing = dash_eligible && elapsed > dash_start && elapsed <= dash_end;
        if dashing {
            let want = yaw_of(toward(players[i].pos, target));
            players[i].yaw = turn_toward(players[i].yaw, want, melee.dash_turn() / moves::SIM_TICKS_PER_TABLE_TICK as f32).0;
        } else if !dash_eligible || elapsed <= dash_start {
            cast_turn(aim, players, i);
        }
        // The dash: (10 / window) x s65 u a program tick, never over 10
        // (`FUN_14018f050`), and the last step is cut by the quirk of
        // `FUN_14025be40` (`step - (distance - 11)` when closer than a step).
        // The program step is chosen on the first sim tick of each program
        // tick and walked half on each of its two sim ticks.
        self.fighters[i].dash_mid = false;
        if dashing {
            let me = &players[i];
            let to = Vec3::new(target.x - me.pos.x, 0.0, target.z - me.pos.z);
            let dist = to.length();
            if (elapsed - dash_start) % 2 == 1 {
                let mut step = melee.dash_step(dash_len);
                if dist > LUNGE_STOP_SHORT && dist - LUNGE_STOP_SHORT < step {
                    step = step - dist + LUNGE_STOP_SHORT;
                }
                self.fighters[i].dash_step = if dist > LUNGE_STOP_SHORT { step } else { 0.0 };
            }
            let me = &mut players[i];
            let half = self.fighters[i].dash_step * 0.5;
            self.fighters[i].dash_mid = half != 0.0 && (elapsed - dash_start) % 2 == 1;
            if half != 0.0 {
                let dir = Vec3::new(me.yaw.sin(), 0.0, me.yaw.cos());
                me.pos = col.push_out_of_walls(me.pos + dir * half, me.body.wall_radius(), STEP_HEIGHT + 1.0, BODY_HEIGHT);
            }
        }
        // The cast clip's root motion, after the dash (see `fighter_step`).
        self.root_step(i, players);
        // The blow is created at the fire tick, and that is when it is paid
        // for; a cast that can't pay lands nothing (the dash still happened).
        let mut already_struck = already_struck;
        if elapsed >= fire_tick && self.fighters[i].bill.is_some() && !self.settle(i) {
            self.fighters[i].struck = true;
            already_struck = true;
        }
        let me = &mut players[i];
        let from = me.pos;
        // The blow passes through players (object flag 0x20, PROGRAM-RE.md "Who can
        // be hit"): everyone it can reach takes it, once, in slot order. A punch is
        // a sphere of s64 x 0.1 u + the body radius on the weapon marker (the
        // caster's centre here), a sword a blade segment of that length tested at
        // 12 u around the target's centre for this tick's and last tick's blade
        // (`FUN_14026c340`, `FUN_14026c730`); Pursuit tests once, at its fire
        // tick, against its locked target only (`FUN_140265e00`).
        let mut struck: Vec<usize> = Vec::new();
        // The blow is an object like any other: made at the fire tick in state 1, its first update only
        // arms it (`FUN_1402638d0`), so the first test is one program tick later and the window
        // (`+0x18` = w44 - w43 further updates, `FUN_14026a020`) is shifted with it. LIVE: every punch /
        // sword / kick take's fire -> health change is 1 program tick (59 -> 60, 53 -> 58 ...).
        let arm = moves::SIM_TICKS_PER_TABLE_TICK;
        let window = !already_struck && elapsed >= fire_tick + arm && elapsed <= hit_end + arm && (melee.shape != moves::melee::Shape::Stamp || elapsed == fire_tick + arm);
        if window {
            let mut facing = Vec3::new(me.yaw.sin(), 0.0, me.yaw.cos());
            let mut at = body_centre(from);
            // The weapon point: the attack object's matrix is the cast effect's
            // `SHOT01` marker or, with none (every melee skill), the right-hand
            // bone's matrix, re-read every tick of the window (`FUN_14026c340`
            // / `FUN_14026c730` via `FUN_14018a310`): a punch is a sphere on the
            // hand, a sword a blade from the hand along its +Z axis. The clip
            // is at frame elapsed / 2 (30 fps clips, 60 Hz ticks). No hand data
            // (tests, an old cache): the caster's centre as before.
            if melee.shape != moves::melee::Shape::Stamp {
                if let Some(h) = row.as_ref().and_then(|r| self.hands[i].get(r.anim_name.as_str())).and_then(|c| c.at(elapsed as f32 / 2.0)) {
                    let yaw = me.yaw;
                    at = local_to_world(from, yaw, h.0);
                    facing = local_dir_to_world(yaw, h.1);
                }
            }
            let last = self.fighters[i].last_blade;
            let candidates: Vec<usize> = if melee.shape == moves::melee::Shape::Stamp { vec![victim] } else { self.victims_of(i).collect() };
            // Line of fire (`FUN_1401641b0`, called by both melee tests after the reach test): the ray
            // from the caster's own centre (the object's `+0x1a4`, written from the owner's position +
            // centre height by `FUN_14026a0d0` / `FUN_140269db0` / `FUN_140265e00` every update) to the
            // target's point; any stage or object face but class 0x10 stops the blow. LIVE
            // `242_Dhampir_Fang` / `317_True_Muramasa`: a box face or a wall between the two, and the
            // original's blow never lands.
            let eye = body_centre(from);
            for v in candidates {
                let c = body_centre(players[v].target_point());
                if melee.reaches(at, facing, last, c) {
                    let (ray, d) = (c - eye, c.distance(eye));
                    if d < 1e-3 || col.raycast_los(eye, ray / d, d, None).is_none() {
                        struck.push(v);
                    }
                }
            }
            self.fighters[i].last_blade = Some((at, at + facing * melee.reach));
        }
        let action_before_strike = self.fighters[i].action;

        if !struck.is_empty() {
            self.fighters[i].struck = true;
            let bodies = bodies_of(players);
            let via = Via { button: self.fighters[i].last_slot, strength: self.fighters[i].cast_strength.take() };
            for v in struck {
                self.strike(i, v, id, Some(from), bodies, Arrival::Melee, via);
            }
        }

        // Don't clobber a state the strike itself just set (e.g. `stun`
        // brushing the attacker into `Action::Hit`): only move the dash/cast
        // pose along if the strike left us in this cast's own run.
        if self.fighters[i].action == action_before_strike {
            let f = &mut self.fighters[i];
            f.action = if elapsed >= fire_tick { Action::Cast } else { Action::Lunge };
            if elapsed >= clip_end {
                f.action = Action::Ready;
                f.melee = false;
            }
        }
    }

    /// Damage from `attacker`'s skill `id` against the player they face
    /// (`foe_of`): for tests and places with no victim of their own.
    pub fn damage(&mut self, attacker: usize, id: i16) -> i32 {
        let victim = self.foe_of(attacker);
        self.damage_to(attacker, victim, id)
    }

    /// Damage from `attacker`'s skill `id` against `victim` (the STR rules
    /// that read "target's Level / Aura / Health / buttons" read theirs).
    pub fn damage_to(&mut self, attacker: usize, victim: usize, id: i16) -> i32 {
        let Some(def) = self.skill(id).cloned() else { return 0 };
        let (me, t) = (&self.fighters[attacker], &self.fighters[victim]);
        let s = def.strength;
        let empty = |f: &Fighter| f.slots.iter().filter(|x| x.is_none()).count() as i32;
        let consumed = |f: &Fighter, cap: Capsule| f.used.iter().filter(|&&u| self.skill(u).is_some_and(|d| d.capsule == cap)).count() as i32;
        use str_rule::*;
        let base = match def.strength_rule {
            TARGET_LEVEL => t.level,
            TARGET_AURA => t.aura(),
            HALF_TARGET_AURA => t.aura() / 2,
            // The rest are attack-only readings (the same numbers mean other things on other colours).
            _ if def.capsule != Capsule::Attack => s,
            TARGET_HEALTH_OVER_YOURS => (t.health - me.health).max(0) * s,
            YOUR_HEALTH_OVER_TARGETS => (me.health - t.health).max(0) * s,
            CONSUMED_ATTACKS => consumed(me, Capsule::Attack) * s,
            CONSUMED_DEFENCES => consumed(me, Capsule::Defence) * s,
            TARGET_EMPTY_BUTTONS => empty(t) * s,
            YOUR_EMPTY_BUTTONS => empty(me) * s,
            BUNDLED => self.bundled_strength(attacker, &def),
            RANDOM => s * self.random(3) as i32,
            AURA_PARTICLES_ON_FIELD => self.capsules.iter().filter(|c| c.skill == AURA_PARTICLE).count() as i32 * s,
            _ => s,
        };
        // Heat, Doze, Lunar Force...: "+1 damage" to this player's attacks.
        if def.capsule == Capsule::Attack { (base + self.stat(attacker, Stat::Attack)).max(0) } else { base }
    }

    /// "STR = STR of all equipped …" (rule 26, `0x140274990`): over the attacker's four buttons,
    /// every occupied one whose skill has the same kind field (s10) as this skill's s46 adds its
    /// own s46 (`FUN_1401272e0` gives the kind: s46 of an attack-kind skill; a slot skill that
    /// is not an attack-kind one adds -1). Bundled Laser's kind is 2 (the Lasers), Bundled
    /// Blaster's 3, Crystallization's 4 ("Piece of ..."), Dance of Death's 1 (the short-range
    /// Ki attacks). The kind is the record's own field, not a name or a school; the skill itself
    /// never matches (its s10 is 0).
    fn bundled_strength(&self, attacker: usize, def: &SkillDef) -> i32 {
        let kind = if def.raw[43] == 0 { def.raw[46] } else { -1 };
        self.fighters[attacker]
            .slots
            .iter()
            .flatten()
            .filter_map(|&id| self.skill(id))
            .filter(|d| d.raw[10] == kind)
            .map(|d| if d.raw[43] == 0 { d.raw[46] as i32 } else { -1 })
            .sum()
    }

    /// Applies `attacker`'s skill, arriving from `from`, to `victim`
    /// (`bodies` = every player's feet and facing). `from` None = from above or
    /// below (Rain, Hole): only an all-round defence covers it. False if a
    /// defence stopped all of it.
    fn strike(&mut self, attacker: usize, victim: usize, id: i16, from: Option<Vec3>, bodies: Bodies, arrival: Arrival, via: Via) -> bool {
        let mut dmg = match via.strength {
            Some(s) => (s + self.stat(attacker, Stat::Attack)).max(0),
            None => self.damage_to(attacker, victim, id),
        };
        if self.fighters[victim].action == Action::Down || self.fighters[victim].invuln_ticks > 0 {
            return false;
        }
        let Some(mut def) = self.skill(id).cloned() else { return false };
        // Necronomicon (status 31) cancels every Pierce / "penetrates defensive
        // skills" flag (`FUN_1402723b0` reads s54 as 0).
        if def.raw[43] == 0 && self.env_status(31) != 0 {
            def.raw[pd_formats::skills::field::PIERCE] = 0;
        }
        // Berserk: "All attack skills gain Pierce effect".
        if def.capsule == Capsule::Attack && def.raw[pd_formats::skills::field::PIERCE] == 0 && self.env_def().is_some_and(environment::all_pierce) {
            def.raw[pd_formats::skills::field::PIERCE] = 1;
        }
        let knocked = self.fighters[victim].action == Action::Knocked;
        if (knocked && !self.hits_downed(&def)) || (!knocked && self.only_hits_downed(&def)) {
            return false;
        }
        // A standing guard takes the attack; otherwise an automatic Shelter is used for THIS attack
        // alone: the guard object it makes is never stored (it ends at its first update).
        let mut guard = self.fighters[victim].guard;
        if guard.is_none() {
            guard = self.auto_guard(victim, &def);
            if self.fighters[victim].action == Action::Down {
                return false;
            }
        }
        if let Some(g) = guard {
            let front = from.is_some_and(|f| barrier::in_front(bodies[victim].0, bodies[victim].1, f));
            let necro = self.env_status(31) != 0;
            match barrier::resolve(&g.guard, dmg, &def, arrival, front, necro) {
                barrier::Outcome::Through => {}
                // About Face / Reverse: no DEF is spent, the shot goes home.
                barrier::Outcome::Turned { back: true } => {
                    self.log(CombatEvent::Blocked { attacker, victim, skill: id, broken: false });
                    self.send_back(victim, attacker, id, bodies, via, REVERSE_WAIT_TICKS);
                    return false;
                }
                // [Course] stops nothing: it only sets the steer-away flag on shots that
                // pass through its field (`move_projectiles`; `FUN_140271760` returns 0).
                barrier::Outcome::Turned { back: false } => {}
                barrier::Outcome::Blocked { broken, damage, effect } => {
                    self.log(CombatEvent::Blocked { attacker, victim, skill: id, broken });
                    self.guard_blocked(victim, attacker, &g, &def, dmg, arrival, broken, via.button);
                    match effect {
                        // A Reflect sends the shell or arc back (only while the wall holds).
                        barrier::BlockEffect::Reflect if !broken => self.send_back(victim, attacker, id, bodies, via, 0),
                        barrier::BlockEffect::Brush if arrival == Arrival::Shot && !broken => self.brush_away(victim, attacker, id, bodies, from),
                        _ => {}
                    }
                    if damage == 0 {
                        return false;
                    }
                    dmg = damage;
                }
            }
        }
        // Whatever reaches the body ends Optical Camo and a status float
        // (Skywalk, Demon's Wing): statuses 8 and 11 are cleared at the end of
        // `FUN_14026f870` whether or not damage was dealt.
        {
            let t = &mut self.fighters[victim];
            t.status.invisible_ticks = 0;
            if t.status_hover {
                t.status.hover_ticks = 0;
                t.status_hover = false;
            }
        }
        // Erase / status / special skills: no damage; they take effect once
        // the sim has finished moving things this tick.
        if def.capsule != Capsule::Attack {
            self.pending_effects.push((attacker, victim, id, via.button));
            self.log(CombatEvent::Hit { attacker, victim, skill: id });
            return true;
        }
        // Void / Invulnerability: attack damage does nothing.
        if self.fighters[victim].status_blocks(&def) {
            return false;
        }
        let (row, total) = self.pick_reaction(victim, &def, dmg);
        let knock = self.airborne[victim] || self.reaction_is_knockdown(row);
        // "The reaction faces the direction the attack came from" -- from an
        // area hit (Rain, Hole: no `from`) there's no horizontal direction,
        // so it defaults to the attacker's own position.
        let origin = from.unwrap_or(bodies[attacker].0);
        let dir = hit_direction(bodies[victim].0, bodies[victim].1, origin);
        let front = hit_front(bodies[victim].0, bodies[victim].1, origin);
        {
            let t = &mut self.fighters[victim];
            t.last_hurt_by = Some(id);
            t.reaction_row = row;
            t.hit_dir = dir;
            t.hit_front = front;
            t.damage_accum = total;
            t.last_hit_tick = Some(self.round_ticks);
        }
        let knockback = toward(origin, bodies[victim].0);
        self.hurt(victim, dmg, knock, knockback);
        self.log(CombatEvent::Hit { attacker, victim, skill: id });
        self.attack_extra(attacker, victim, &def, dmg);
        true
    }

    /// Damage `victim` took in the last `DAMAGE_WINDOW_TICKS` sim ticks, this
    /// hit not yet included (PROGRAM-RE.md "Damage").
    fn recent_damage(&self, victim: usize) -> i32 {
        let f = &self.fighters[victim];
        match f.last_hit_tick {
            Some(t) if self.round_ticks.saturating_sub(t) < DAMAGE_WINDOW_TICKS => f.damage_accum,
            _ => 0,
        }
    }

    /// The hit-reaction row this hit picks, and the damage total (this hit
    /// included) that picked it -- PROGRAM-RE.md "Hit reactions": airborne
    /// always knocks down (row 27, s61 is 0 on every skill); on the ground,
    /// by the damage taken in the last 3 s: < 3 light (s57), 3-4 medium
    /// (s58), >= 5 heavy (s59).
    fn pick_reaction(&self, victim: usize, def: &SkillDef, dmg: i32) -> (i16, i32) {
        let total = self.recent_damage(victim) + dmg;
        if self.airborne[victim] {
            return (AIR_KNOCKDOWN_ROW, total);
        }
        use pd_formats::skills::field::*;
        let row = def.raw[if total < 3 {
            REACTION_LIGHT
        } else if total < 5 {
            REACTION_MEDIUM
        } else {
            REACTION_HEAVY
        }];
        (row, total)
    }

    /// Whether reaction row `row` is a knock-down: the loaded table's own
    /// `kind` when we have one, else the old fixed-id guess (tests that
    /// don't call `set_reaction_rows`).
    fn reaction_is_knockdown(&self, row: i16) -> bool {
        match self.reaction_rows.get(&row) {
            Some(r) => r.kind == reaction_kind::KNOCKDOWN,
            None => FALLBACK_KNOCKDOWN_ROWS.contains(&row),
        }
    }

    /// Whether reaction row `row` is the pinned kind (Pursuit/Sky Pursuit
    /// "check the down state themselves" through this, PROGRAM-RE.md "Who
    /// can be hit").
    fn reaction_is_pinned(&self, row: i16) -> bool {
        match self.reaction_rows.get(&row) {
            Some(r) => r.kind == reaction_kind::PINNED,
            None => row == FALLBACK_PINNED_ROW,
        }
    }

    /// Only lands on a knocked-down target: the pinned-reaction pursuit
    /// skills, or s62 = downed bits without the normal-state bit (Backdraft
    /// Bullet: "Effective only against knocked down targets").
    fn only_hits_downed(&self, def: &SkillDef) -> bool {
        if def.capsule != Capsule::Attack {
            return false;
        }
        let s62 = def.raw[pd_formats::skills::field::TARGET_STATES];
        self.reaction_is_pinned(def.raw[pd_formats::skills::field::REACTION_LIGHT]) || (s62 != 0 && s62 & 1 == 0 && s62 & 0b110 != 0)
    }

    /// s62 bits 2 (knocked down, still airborne) / 4 (lying on the ground):
    /// can this attack touch a downed target at all (Gravity Press, Geyser
    /// Impact, Backdraft Bullet, and the pinned pursuit skills)?
    fn hits_downed(&self, def: &SkillDef) -> bool {
        if def.capsule != Capsule::Attack {
            return false;
        }
        self.only_hits_downed(def) || def.raw[pd_formats::skills::field::TARGET_STATES] & 0b110 != 0
    }

    /// Takes `dmg` off player `victim` and starts the reaction the picked row
    /// asks for -- a knock-down (flight, landing, lying, get-up), a kind-1
    /// collapse (clip, lie, get-up) or a stagger of the clip's own length. At
    /// 0 Health the same reaction plays and the fighter stays down (a round
    /// point to the other player, `kill`). `knockback`: horizontal unit vector
    /// away from the hit, for a knock-down's launch and the body turn.
    fn hurt(&mut self, victim: usize, dmg: i32, knock: bool, knockback: Vec3) {
        let health = (self.fighters[victim].health - dmg).max(0);
        self.fighters[victim].health = health;
        // (A hit does not drop a standing guard: the guard object runs on its own
        // countdown and only a break, the linger or its time ends it, `0x140264340`.)
        self.fighters[victim].pending_shot = None;
        self.fighters[victim].bill = None;
        let action = self.fighters[victim].action;
        if action != Action::Down {
            let row = self.fighters[victim].reaction_row;
            if knock {
                self.fighters[victim].hit_seq = self.fighters[victim].hit_seq.wrapping_add(1);
                self.start_knockdown(victim, knockback);
            } else if action == Action::Knocked && self.reaction_kind_of(row) == Some(reaction_kind::PINNED) {
                // Kind 7: a Pursuit on a body lying down stamps it (`combat/reactions.rs`).
                self.start_pinned(victim);
            } else if action != Action::Knocked {
                self.fighters[victim].hit_seq = self.fighters[victim].hit_seq.wrapping_add(1);
                self.play_row(victim, knockback);
            }
        }
        if health == 0 {
            self.kill(victim);
        }
    }

    /// Adds `delta` to player `i`'s Health the way every Health change of the
    /// program does (`FUN_14024d7a0`): only while they have Health; below 1 is
    /// a knock-out (a Health cost, poison, Backdraft, a drop landing or a
    /// capture penalty can all be the killing blow); never above their maximum.
    pub fn change_health(&mut self, i: usize, delta: i32) {
        let f = &mut self.fighters[i];
        if f.health <= 0 || f.action == Action::Down {
            return;
        }
        let health = f.health + delta;
        if health < 1 {
            f.health = 0;
            self.kill(i);
        } else {
            f.health = health.min(f.max_health);
        }
    }

    /// Player `i` is out of Health: down for good this round, and a round
    /// point may fall to the survivor. The body does not drop dead on the
    /// spot: the killing blow's reaction plays out (a launch flies, lands and
    /// lies; a stagger plays its clip first) and a fighter who was standing,
    /// casting or falling collapses with row 25 (DMG_DRAIN_HIGH, then
    /// DOWN_BACK); none of them ever gets up (`0x14025c3d0` dead branch ->
    /// state 27 for 90 ticks, `0x14024ee75`). `Action::Down` is the KO marker.
    fn kill(&mut self, i: usize) {
        let f = &mut self.fighters[i];
        let was = f.action;
        if was != Action::Down {
            // The KO voice, the moment the fighter falls (also mid-round, with 3-4 players).
            self.log(CombatEvent::Knockout { who: i });
        }
        let f = &mut self.fighters[i];
        f.health = 0;
        f.guard = None;
        f.pending_shot = None;
        f.bill = None;
        f.charge = None;
        f.action = Action::Down;
        f.ko_tick.get_or_insert(self.round_ticks);
        match was {
            Action::Down => {}
            Action::Knocked => {
                f.ko_play = true;
                // Already getting up when it hit zero: lie down again.
                if !f.flying && !f.collapsing() && f.action_ticks <= KNOCK_GETUP_TICKS {
                    f.action_ticks = KNOCK_GETUP_TICKS + 1;
                }
            }
            Action::Hit => {
                f.ko_play = true;
                f.ko_stagger = f.action_ticks.max(1);
                f.action_ticks = 0;
            }
            _ => self.start_ko_collapse(i),
        }
        self.check_round_end();
    }

    /// A collapse for a fighter who died on their feet: row 25's clip then
    /// the lying pose, kept for the rest of the round. Without the reaction
    /// table (tests) the old static pose stays.
    fn start_ko_collapse(&mut self, i: usize) {
        if self.reaction_kind_of(KO_COLLAPSE_ROW) != Some(reaction_kind::CLIP_THEN_LIE) {
            return;
        }
        let f = &mut self.fighters[i];
        f.reaction_row = KO_COLLAPSE_ROW;
        f.hit_seq = f.hit_seq.wrapping_add(1);
        f.ko_play = true;
        f.ko_stagger = 0;
        f.flying = false;
        f.knock_vel = Vec3::ZERO;
        self.begin_collapse(i);
    }

    /// Ends the round when at most one team still has a player standing
    /// (the original counts one alive flag per team each tick and the battle
    /// is over at one or none: `0x14012c510` through `0x14012c440`, docs/audits/
    /// P0-FOURPLAYER.md "1b. F"). The survivor (the lowest standing slot of
    /// the last team) takes the round. Nobody standing is a draw; a
    /// same-tick wipe is settled at the end of `step`. Not while the round
    /// is already decided.
    fn check_round_end(&mut self) {
        // Prop Hunt ends its rounds itself (`prophunt.rs`).
        if self.rounds_by_mode {
            return;
        }
        let (teams, survivor) = self.roster.alive_teams(|i| self.fighters[i].action != Action::Down);
        if let Some((w, left)) = self.round_over {
            // The last one standing was knocked out within the window of the
            // first knock-out: they went down together, nobody wins.
            let together = self.round_decided_tick.is_some_and(|t| self.round_ticks.saturating_sub(t) <= SIMULTANEOUS_KO_TICKS);
            if w != DRAW && teams == 0 && together && !self.time_up {
                self.wins[w] = self.wins[w].saturating_sub(1);
                self.round_over = Some((DRAW, left));
            }
            return;
        }
        if teams <= 1 {
            self.round_decided_tick = Some(self.round_ticks);
            match survivor {
                Some(w) => {
                    self.wins[w] += 1;
                    self.round_over = Some((w, ROUND_END_TICKS));
                }
                None => self.round_over = Some((DRAW, ROUND_END_TICKS)),
            }
        }
    }

    /// How the decided round ended, if it has: the surviving player (and
    /// their team) or a draw. `Combat::round_over` holds the same as a slot.
    #[allow(dead_code)] // read by the result screen, another lane's
    pub fn round_outcome(&self) -> Option<Outcome> {
        let (w, _) = self.round_over?;
        Some(if w == DRAW { Outcome::Draw } else { Outcome::Winner { slot: w, team: self.roster.team[w] } })
    }

    /// Rounds won by `team`'s players (a round is credited to one player, the
    /// survivor, so a team's score is the sum over its members).
    #[allow(dead_code)] // read by the match-end check and result screen, another lane's
    pub fn team_wins(&self, team: u8) -> u32 {
        self.roster.slots().filter(|&i| self.roster.team[i] == team).map(|i| self.wins[i]).sum()
    }

    /// Finishing order, 1 = best, by player (`0` for slots not playing):
    /// standing players share first place, then the later a player was
    /// knocked out the better (the program ranks by alive flag, then by
    /// survival score for every rule set but Tag Team, which is decided by
    /// team: `0x14012c8c0`). Equal scores share a place.
    #[allow(dead_code)] // read by the result screen, another lane's
    pub fn placements(&self) -> [u8; MAX_PLAYERS] {
        let score = |i: usize| self.fighters[i].ko_tick.map_or(u32::MAX, |t| t);
        std::array::from_fn(|i| if self.roster.is_active(i) { 1 + self.roster.slots().filter(|&j| score(j) > score(i)).count() as u8 } else { 0 })
    }

    /// The row kind of reaction row `row`, if the table is loaded.
    fn reaction_kind_of(&self, row: i16) -> Option<i16> {
        self.reaction_rows.get(&row).map(|r| r.kind)
    }

    /// The ordinary (non-knock-down) hit-stun length: the picked clip's own
    /// length (state 14's timer is the clip length, `FUN_140258690` kinds
    /// 0/2/3/10 -> `FUN_140259d10(0xe, len)`), a fixed tick count for kind 11
    /// rows (row 46's 5-tick DMG_SHORT flinch), the flat guess when this
    /// character's clip table is not loaded or the row is a kind we do not
    /// play yet.
    fn stagger_ticks(&self, victim: usize) -> u16 {
        let row = self.fighters[victim].reaction_row;
        match self.reaction_rows.get(&row) {
            Some(r) if r.kind == reaction_kind::FIXED_TICKS => moves::table_ticks(r.w[1]).max(1),
            // Kinds 8 / 9 (reactions2 lane, `combat/reactions.rs`): the bind's timer + the rest of
            // its clip, the lift's w42 + w43.
            Some(r) if r.kind == reaction_kind::BIND_A => self.bind_ticks(victim, r),
            Some(r) if r.kind == reaction_kind::BIND_B => Self::lift_ticks(r),
            Some(r) if matches!(r.kind, reaction_kind::CLIP | reaction_kind::FOUR_WAY | reaction_kind::FRONT_BACK | 10) => {
                self.clip_ticks[victim].get(self.reaction_clip(victim)).map_or(HIT_STUN_TICKS, |&t| t.max(2))
            }
            _ => HIT_STUN_TICKS,
        }
    }

    /// Begins kind 1's timeline on `victim` (state 15, `0x140258690` case 1):
    /// the row's collapse clip (w41, blend -1) for its length + 1 program
    /// ticks, then lying `COLLAPSE_LIE_TICKS` in the pose w42 names, then the
    /// get-up. All one countdown: `collapsing()` / the get-up window are read
    /// off `action_ticks`.
    fn begin_collapse(&mut self, victim: usize) {
        let row = self.fighters[victim].reaction_row;
        let (clip_id, pose) = self.reaction_rows.get(&row).map_or((117, 60), |r| (r.w[0], r.w[1]));
        let clip = Self::anim_name(clip_id.max(0) as usize);
        let collapse = self.clip_ticks[victim].get(clip).copied().unwrap_or_else(|| moves::table_ticks(default_clip_table_ticks(clip))) + 2;
        let f = &mut self.fighters[victim];
        f.lie_from_collapse = true;
        f.stamp_left = 0;
        f.struggle = false;
        f.lift_hold = false;
        f.lift_launch = None;
        f.lie_pose = if matches!(pose, 60 | 62) { pose as u8 } else { 62 };
        f.lie_ticks = COLLAPSE_LIE_TICKS;
        f.lying_elapsed = 0;
        f.land_class = 0;
        f.wall_hit = None;
        f.queued_reaction = 0;
        f.action_ticks = collapse + COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS;
    }

    /// A hit with a kind-1 row: the victim collapses and lies helpless, only
    /// attacks that reach a downed body (`hits_downed`) can touch them.
    fn start_collapse(&mut self, victim: usize) {
        let f = &mut self.fighters[victim];
        f.action = Action::Knocked;
        f.flying = false;
        f.knock_vel = Vec3::ZERO;
        f.flight_peak = None;
        self.begin_collapse(victim);
    }

    /// Starts a knock-down: launch velocity from the picked reaction row's
    /// `launch_speed`/`launch_angle` along `knockback` (PROGRAM-RE.md
    /// "Knock-down physics"), the body turned the way the program turns it
    /// (`0x140258dfd`: facing := the bearing to the attacker when the pose is
    /// in the front set, that + 180 deg otherwise; the slide goes away from
    /// the attacker either way); no reaction table loaded, or the row has no
    /// launch speed (tests) -- no flight, straight to lying (the old flat
    /// timing, `KNOCKDOWN_TICKS`).
    fn start_knockdown(&mut self, victim: usize, knockback: Vec3) {
        let row = self.fighters[victim].reaction_row;
        let launch = self.reaction_rows.get(&row).filter(|r| r.launch_speed > 0).map(|r| (r.launch_speed, r.launch_angle));
        let clip = self.reaction_clip(victim);
        let clip_id = pd_formats::anim::ANIM_NAMES.iter().position(|n| *n == clip).unwrap_or(0);
        let front_set = program_front_set(clip_id) || self.reaction_kind_of(row) == Some(6);
        let to_attacker = yaw_of(-knockback);
        let f = &mut self.fighters[victim];
        f.action = Action::Knocked;
        f.action_ticks = KNOCKDOWN_TICKS;
        f.lie_ticks = KNOCK_LYING_TICKS;
        f.lie_from_collapse = false;
        f.flight_peak = None;
        f.front_set = front_set;
        f.land_class = 0;
        f.lying_elapsed = 0;
        f.land_flip = false;
        f.wall_hits_left = WALL_HITS;
        f.wall_cooldown = WALL_COOLDOWN_TICKS;
        f.wall_hit = None;
        f.stuck_ticks = 0;
        f.queued_reaction = 0;
        f.pending_turn = Some(if front_set { to_attacker } else { to_attacker + std::f32::consts::PI });
        f.stamp_left = 0;
        f.struggle = false;
        f.lift_hold = false;
        f.lift_launch = None;
        f.root = None;
        match launch {
            Some((speed, angle)) => {
                let (sin_a, cos_a) = launch_components(angle);
                let total = speed as f32 * LAUNCH_SPEED_UNIT;
                let horiz = Vec3::new(knockback.x, 0.0, knockback.z).normalize_or(Vec3::Z);
                f.knock_vel = horiz * (total * cos_a) + Vec3::Y * (total * sin_a);
                f.last_launch_speed = speed;
                f.flying = true;
            }
            None => {
                f.knock_vel = Vec3::ZERO;
                f.last_launch_speed = 0;
                f.flying = false;
            }
        }
    }

    /// Player `i` fell out of the arena and was put back on their spawn
    /// (`FUN_140251d80`): no reaction -- the body stands on its start point,
    /// facing the start yaw (`Match::step` puts it there), idle, with 3
    /// Health gone silently (it can be the killing blow, then the body lies
    /// DOWN_FRONT) and 90 program ticks of protection.
    pub fn fall_out(&mut self, i: usize) {
        if self.fighters[i].action == Action::Down || self.round_over.is_some() {
            return;
        }
        let f = &mut self.fighters[i];
        f.action = Action::Ready;
        f.action_ticks = 0;
        f.pending_shot = None;
        f.bill = None;
        f.charge = None;
        f.flying = false;
        f.knock_vel = Vec3::ZERO;
        f.wall_hit = None;
        f.pending_turn = None;
        f.queued_reaction = 0;
        f.stamp_left = 0;
        f.struggle = false;
        f.lift_hold = false;
        f.lift_launch = None;
        f.root = None;
        f.air_cast = false;
        f.melee = false;
        f.invuln_ticks = FALL_OUT_PROTECT_TICKS;
        self.change_health(i, -FALL_OUT_DAMAGE);
        let f = &mut self.fighters[i];
        if f.action == Action::Down && !f.ko_play {
            // Dead from the fall: DOWN_FRONT, lying for good.
            f.ko_play = true;
            f.lie_from_collapse = true;
            f.lie_pose = 60;
            f.lie_ticks = COLLAPSE_LIE_TICKS;
            f.action_ticks = COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS + 1;
        }
    }

    /// An automatic Shelter in `i`'s slots puts itself up when they are
    /// attacked, if they can pay for it ("If there isn't enough Aura ... will
    /// not take effect"). `FUN_14026f530`: not for an attack that penetrates
    /// defences (s54 = 2, unless Necronomicon is on) nor for a motion-4
    /// instant; the first affordable one fires through the ordinary fire path.
    ///
    /// One block per use (`FUN_14026f530` -> `FUN_1402626d0` -> `FUN_140271760`): the use pays
    /// (`FUN_1402621f0`), consumes the skill's use and makes a fresh guard object that the call
    /// resolves THIS attack against. The object is never in contact with anything on its own
    /// (`FUN_14026dcb0`: kind 7 falls through to "no contact") and `FUN_140264340` ends a kind 7
    /// object at its first update, so it is returned here and never stored in `Fighter::guard`:
    /// the next attack, a tick or a second later, triggers (and pays for) another use.
    fn auto_guard(&mut self, i: usize, attack: &SkillDef) -> Option<ActiveGuard> {
        let necro = self.env_status(31) != 0;
        if attack.raw[43] == 0 && ((attack.ignores_defence() && !necro) || attack.motion() == 4) {
            return None;
        }
        let env = self.env_def().cloned();
        let found = self.fighters[i].slots.iter().enumerate().find_map(|(k, s)| {
            let def = self.skill((*s)?)?;
            let g = barrier::Guard::from_def(def).filter(|g| g.automatic())?;
            // Same rules as casting it: environment cost and bans, then the aura check.
            let base = def.cost + def.cost_per_use() * self.fighters[i].spent[k] as i32;
            let cost = env.as_ref().map_or(base, |e| environment::cost(e, def, base)).max(0);
            let allowed = env.as_ref().is_none_or(|e| environment::allowed(e, def));
            (allowed && self.fighters[i].aura() >= cost && self.fighters[i].level >= def.level_penalty).then_some((k, def.clone(), cost, g))
        });
        let found = found?;
        let (slot, def, cost, guard) = found;
        let skill = def.id;
        let lose = env.as_ref().map_or(0, environment::health_on_use);
        let uses_before = self.fighters[i].spent[slot];
        // Its penalties apply too ("Guardian Angel ... Penalty: 2 Health"); a
        // Health penalty can be the killing blow.
        self.change_health(i, -(def.health_cost + lose));
        let bill = Bill {
            slot,
            button_skill: skill,
            id: skill,
            cost,
            level_penalty: def.level_penalty,
            health_cost: 0,
            uses: def.uses(),
            retained: def.retained(),
            attack: false,
            auto_capture: false,
            finisher: false,
            spent_before: uses_before,
            defer_use: false,
        };
        let f = &mut self.fighters[i];
        f.aura_ticks -= cost * AURA_TICKS_PER_POINT;
        f.level = if def.level_penalty < 0 { f.level_min.min(f.level) } else { (f.level - def.level_penalty).max(f.level_min) };
        f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
        self.consume_use(i, &bill);
        let f = &mut self.fighters[i];
        let object = ActiveGuard::new(skill, guard.with_uses(&def, uses_before), slot);
        if f.action == Action::Ready {
            f.cast_skill = skill;
            f.cast_seq = f.cast_seq.wrapping_add(1);
            // LIVE (takes_full_trace, Mind's Eye / Guard of Water / Auto Shell / Guardian Angel): no
            // cast sound, no voice. READ `FUN_1402626d0` (kind 7 branch): the clip is started with
            // `FUN_140257310` directly; the row's cast sound (w3) and voice (w4) are played only by
            // `FUN_14025a3e0` (press path: `FUN_140276330` for row +6, then the voice from row +8).
            f.auto_cast_seq = f.cast_seq;
            f.action = Action::Cast;
            f.action_ticks = guard.ticks;
            // It puts itself up: nothing to aim, nothing pending or owed.
            f.aim = Aim::Keep;
            f.air_cast = false;
            f.bill = None;
            f.pending_shot = None;
            f.charge = None;
        }
        Some(object)
    }

    /// A guard has stopped `attack` (STR `strength`): its lifecycle
    /// (`FUN_140271760`). Each block takes the attack's STR off the guard's
    /// DEF; below zero the guard breaks at once (impact 10), its Add effect
    /// runs and the defence skill is erased from its button
    /// (`FUN_140262fc0`: Retain re-pays, "can't be erased" stays). A block
    /// that leaves it standing runs the Add effect and then ends the guard 5
    /// program ticks later (impact 9; only the first block starts the
    /// linger -- the guard keeps blocking, and draining, until then).
    #[allow(clippy::too_many_arguments)]
    fn guard_blocked(&mut self, holder: usize, attacker: usize, g: &ActiveGuard, attack: &SkillDef, strength: i32, arrival: Arrival, broken: bool, button: usize) {
        self.guard_block_recoil(holder, g);
        if broken {
            // State 0 first (so a capture Add finds the guard gone), then the Add
            // effect, then the skill leaves its button.
            self.fighters[holder].guard = None;
            self.guard_extra(holder, attacker, g, attack, strength, arrival, true, button);
            moves::erase::erase_slot(self, holder, g.button as usize);
        } else {
            self.guard_extra(holder, attacker, g, attack, strength, arrival, false, button);
            if let Some(ag) = &mut self.fighters[holder].guard {
                ag.guard.def -= strength;
                if !ag.lingering {
                    ag.lingering = true;
                    ag.ticks = barrier::LINGER_TICKS;
                }
            }
        }
    }

    /// A guard that stops something cuts its holder's cast short (`FUN_140271760`: both the break
    /// branch and the final `LAB_140271cc9` path call the holder's controller vtable `+0x88` =
    /// `FUN_14024ff20`; the Course / Reverse kinds return before it): the cast clip is restarted at
    /// frame w44 of the row (`FUN_1402573e0`, 4th argument `(float) row[+0x58]`, 30 on every guard
    /// row) and the state becomes 10 with the timer `clip length - w44` (`FUN_140259d10(state 10,
    /// length - w44)`, length = `controller[+0x13014 + clip * 0x10]`), i.e. length - w44 + 1 program
    /// ticks. LIVE (takes_full, Blaster STR 4 on every DEF_SHIELD / DEF_SHELTER guard that stood in
    /// its way, broken or not: Eraser, Power Shell, Laser Barrier, Reflect Barrier, Fortress of
    /// Iron ... end press + 38): the block lands at t66 and 66 + (40 - 30) + 1 = 77. Built for a
    /// holder still in the guard's own cast and not for the automatic Shelter (s44 = 7: its cast
    /// starts from the block itself); an idle holder of a standing guard would start a fresh state
    /// 10 in the program (not built: no take shows it, UNCONFIRMED).
    fn guard_block_recoil(&mut self, holder: usize, g: &ActiveGuard) {
        let f = &self.fighters[holder];
        if f.action != Action::Cast || f.cast_skill != g.skill {
            return;
        }
        if self.skill(g.skill).is_none_or(|d| d.raw[44] == barrier::kind::AUTO_SHELTER) {
            return;
        }
        let Some(row) = self.cast_row(holder, g.skill).cloned().filter(|r| !r.anim_name.is_empty()) else { return };
        let left = self.clip_hold_ticks(holder, &row.anim_name).saturating_sub(moves::table_ticks(row.hit_window_end.max(0)));
        self.fighters[holder].action_ticks = left + moves::SIM_TICKS_PER_TABLE_TICK;
    }

    /// A defence's "Add:" effect when it blocks or breaks (`FUN_140271ed0`),
    /// and a Brush's stun on a close-range attacker. Not for the program's
    /// motion-18 objects; a guard with s50 = 1 applies it only to short-range
    /// attacks. `button`: the attacker's button the attack was cast from.
    #[allow(clippy::too_many_arguments)]
    fn guard_extra(&mut self, holder: usize, attacker: usize, g: &ActiveGuard, attack: &SkillDef, strength: i32, arrival: Arrival, broken: bool, button: usize) {
        // `FUN_14026cd70` case 7 stuns the attacker (reaction 26) only for attack-object motions 3, 12
        // and 13; a Pursuit (14) and anything else is brushed aside with no stun.
        if g.guard.kind == barrier::kind::BRUSH && attack.raw[43] == 0 && matches!(attack.motion(), 3 | 12 | 13) {
            stun(&mut self.fighters[attacker], barrier::BRUSH_STUN_TICKS);
        }
        let (guard_button, g) = (g.button as usize, &g.guard);
        if attack.raw[43] == 0 && attack.motion() == 18 {
            return;
        }
        if g.short_range_only && !(arrival == Arrival::Melee || barrier::is_short_range(attack)) {
            return;
        }
        let ticks = moves::table_ticks(g.add_amount);
        match g.add {
            barrier::add::HEALTH_FROM_DAMAGE => self.change_health(holder, strength),
            // Absorb Energy: `FUN_140271ed0` case 2 = `FUN_1402618e0(holder, (float)[attack + 0x60])`: the
            // attack's STR, the field `FUN_140271760` takes off the guard's DEF (not its Aura cost).
            barrier::add::AURA_FROM_COST => {
                let f = &mut self.fighters[holder];
                f.aura_ticks = (f.aura_ticks + strength * AURA_TICKS_PER_POINT).min(f.level * AURA_TICKS_PER_POINT);
            }
            // The attacker's skill leaves the button it was cast from (attack
            // skills only: `s9 = 1`).
            barrier::add::ERASE_ATTACK => {
                if attack.capsule == Capsule::Attack && self.fighters[attacker].slots[button].is_some() {
                    moves::erase::erase_slot(self, attacker, button);
                }
            }
            // "Target can't move for 3 sec": bound, not stunned (skills still usable).
            barrier::add::PARALYSE => {
                let st = &mut self.fighters[attacker].status;
                st.bound_ticks = st.bound_ticks.max(ticks);
            }
            barrier::add::FREEZE => self.fighters[attacker].frozen_ticks = ticks,
            barrier::add::POISON => self.fighters[attacker].poison_ticks = ticks,
            // The blocked skill lands on the guard's own button (a guard that
            // is still standing).
            barrier::add::CAPTURE if !broken => self.fighters[holder].set_slot(guard_button, Some(attack.id)),
            _ => {}
        }
    }

    /// An attack's "Add:" effect when it lands for `dmg` (moves::attack_add).
    fn attack_extra(&mut self, attacker: usize, victim: usize, def: &SkillDef, dmg: i32) {
        use moves::attack_add::*;
        // Necronomicon (status 31): an attack skill's Add effect, its erase-on-hit
        // and the reaction that follows them are skipped (`FUN_14026f870`:
        // `s9 != 1 || status 31 == 0` gates `FUN_140272510` and `FUN_140272bc0`).
        if self.env_status(31) != 0 {
            return;
        }
        let (kind, ticks) = def.add_effect();
        // The Add effect's own sound on the victim (`FUN_140272bc0` -> `FUN_1402738d0`;
        // LIVE: Song of Succubus / Succubus Tempt common/90).
        if kind != 0 {
            self.log(CombatEvent::AddEffect { who: victim, code: kind, amount: def.raw[pd_formats::skills::field::ADD_AMOUNT] as i32 });
        }
        match kind {
            AURA_TO_ZERO => self.fighters[victim].aura_ticks = 0,
            FREEZE => self.fighters[victim].frozen_ticks = ticks,
            POISON => self.fighters[victim].poison_ticks = ticks,
            HEALTH_FROM_DAMAGE => {
                let f = &mut self.fighters[attacker];
                f.health = (f.health + dmg).min(f.max_health);
            }
            LEVEL_DRAIN => {
                let t = &mut self.fighters[victim];
                t.level = (t.level - dmg).max(0);
                t.aura_ticks = t.aura_ticks.min(t.level * AURA_TICKS_PER_POINT);
                let f = &mut self.fighters[attacker];
                f.level = (f.level + dmg).min(f.level_max);
            }
            _ => {}
        }
        // An attack that emptied one of the victim's buttons plays common/91 (`0x14026fb8e`).
        if moves::erase::on_hit(self, attacker, victim, def) {
            self.log(CombatEvent::ButtonErased { attacker, victim, skill: def.id });
        }
        // s60: a second reaction plays once the Add effect above actually
        // took hold (PROGRAM-RE.md "Hit reactions"; non-attack skills use
        // only this -- `moves::status`/`moves::erase` land through
        // `pending_effects`, not `attack_extra`, so this is the attack side:
        // aura-to-zero, freeze, poison, health/level drain).
        let after = def.raw[pd_formats::skills::field::REACTION_AFTER_ADD];
        if after != 0 && self.fighters[victim].action == Action::Hit {
            // Chained behind the first reaction, not in place of it
            // (`0x14025c1a0` plays it when the stagger's timer runs out).
            self.fighters[victim].queued_reaction = after;
        }
    }

    /// A [Brush] guard brushes a shell aside: it flies off to the holder's
    /// side, harmless to players, destroying stage objects in its way.
    fn brush_away(&mut self, holder: usize, attacker: usize, skill: i16, bodies: Bodies, from: Option<Vec3>) {
        let Some(def) = self.skill(skill).cloned() else { return };
        let Behaviour::Shoot(kind @ moves::ShotKind::Shell(_)) = moves::behaviour_of(&def) else { return };
        let me = bodies[holder].0;
        let incoming = from.map_or(Vec3::Z, |f| toward(f, me));
        let aside = me + Vec3::new(-incoming.z, 0.0, incoming.x) * 300.0 + incoming * 150.0;
        for mut shot in moves::launch(kind, &def, me, aside) {
            shot.homing = 0.0;
            shot.delay = 0;
            shot.armed = true; // the same object, brushed in place (code 7: state stays 2)
            let id = self.take_id();
            self.projectiles.push(Projectile { id, owner: holder, target: attacker, skill, pos: shot.pos, prev_pos: shot.pos, shot, turned: true, harmless: true, button: 0, prop: None, brushed: true, strength: None });
        }
    }

    /// A Reflect / Reverse sends `skill` back: player `from` fires it at `to`
    /// (whoever sent it). The object waits `wait` program ticks before it
    /// moves again: 0 for a Reflect (code 5), 20 for a Reverse (code 6, `+0x20 = 0x14`).
    fn send_back(&mut self, from: usize, to: usize, skill: i16, bodies: Bodies, via: Via, wait: u16) {
        let Some(def) = self.skill(skill).cloned() else { return };
        let Behaviour::Shoot(kind) = moves::behaviour_of(&def) else { return };
        for mut shot in moves::launch(kind, &def, bodies[from].0, bodies[to].0) {
            shot.armed = true; // the same object, re-aimed in place (code 5 / 6: state 5 -> 2, no new arming tick)
            shot.delay = wait * moves::SIM_TICKS_PER_TABLE_TICK;
            let id = self.take_id();
            self.projectiles.push(Projectile { id, owner: from, target: to, skill, pos: shot.pos, prev_pos: shot.pos, shot, turned: true, harmless: false, button: via.button, prop: None, brushed: false, strength: via.strength });
        }
    }

    /// Steps every flying object one program tick on every second sim tick
    /// from its creation (docs/MOVES.md): the family's own update
    /// (`moves::Shot::step`), then the program's shared tail (`FUN_14026e720`):
    /// the swept body tests (`FUN_14026c1c0`: segment against the body sphere,
    /// 5 u + 5 u, with a clear line from where the step began), the wall test
    /// (`FUN_14026ca50`), the life count. The drawn position (`Projectile::pos`)
    /// moves half a step each sim tick.
    fn move_projectiles(&mut self, players: &[Player], col: &Collision) {
        let bodies = bodies_of(players);
        // Orbit-changing guards turn shots before they arrive.
        let fields: [Option<barrier::Guard>; MAX_PLAYERS] = std::array::from_fn(|i| self.fighters[i].guard.map(|g| g.guard).filter(|g| g.turns_shots()));
        // Whom each player's shots can reach (everyone but the caster, teammates per the friendly-fire policy).
        let reach: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|o| self.victims_of(o).collect());
        // Whom a blast on a wall or the floor reaches: those, and the thrower ("a Bomb that hits a wall hurts its thrower").
        let everyone: [u8; MAX_PLAYERS] = std::array::from_fn(|o| reach[o].iter().fold(1u8 << o, |m, &j| m | 1 << j));
        // s54 = 3 attacks are not turned by Course / About Face (`FUN_1402723b0` -> `FUN_140271760`).
        let turn_proof: Vec<i16> = self.skills.iter().filter(|d| d.raw[pd_formats::skills::field::PIERCE] == 3).map(|d| d.id).collect();
        let mut turned = Vec::new();
        // (owner, victim, skill, where it came from, how it arrived, delivery)
        let mut hits: Vec<(usize, usize, i16, Option<Vec3>, Arrival, Via)> = Vec::new();
        // (owner, skill, blast centre, radius, players it can reach, delivery)
        let mut explosions: Vec<(usize, i16, Vec3, f32, u8, Via)> = Vec::new();
        // (object hit, attacking player, skill, thrown object's STR)
        let mut prop_hits: Vec<(u32, usize, i16, Option<i32>)> = Vec::new();
        // A thrown object passes through its own hull (switched off only after this tick).
        let own_hull: Vec<Option<u32>> = self.projectiles.iter().map(|p| p.prop.and_then(|k| self.props.get(k)).map(|o| o.index)).collect();
        let mut expired: Vec<(u32, i16)> = Vec::new();
        let mut k = 0;
        self.projectiles.retain_mut(|p| {
            let (strength, own) = (p.strength, own_hull[k]);
            k += 1;
            let via = Via { button: p.button, strength };
            p.prev_pos = p.pos;
            if p.shot.delay > 0 {
                p.shot.delay -= 1;
                return true;
            }
            let stepped = p.shot.tick % 2 == 0;
            p.shot.tick = p.shot.tick.wrapping_add(1);
            if !stepped {
                p.pos = p.shot.pos;
                return true;
            }
            // The object's first program tick only arms it (state 1 -> 2, `FUN_1402638d0`): it is made in the
            // caster's update and run later in the same frame, so it first moves a program tick after the fire
            // tick (LIVE: every takes' fire -> hit is 1 tick longer than ours, punches included).
            if !p.shot.armed {
                p.shot.armed = true;
                p.pos = p.shot.pos;
                return true;
            }
            // An orbit-changing guard of any player the shot can reach turns it as it comes within the field:
            // a [Course] field makes it steer away from its target (it can still land), a Reverse /
            // About Face field sends it back at its owner (`FUN_14026cd70` codes 5 and 6).
            let holder = reach[p.owner].iter().copied().find(|&v| {
                let c = body_centre(players[v].target_point());
                fields[v].is_some_and(|g| p.shot.pos.distance(c) <= g.field.max(GUARD_FIELD_MIN))
            });
            if let Some((v, g)) = holder.and_then(|v| fields[v].map(|g| (v, g))).filter(|_| !p.turned && !turn_proof.contains(&p.skill)) {
                turned.push((p.owner, v, p.skill));
                p.turned = true;
                if g.kind == barrier::kind::REVERSE {
                    // Back at whoever sent it.
                    (p.owner, p.target) = (v, p.owner);
                    p.shot.send_back(body_centre(players[p.target].target_point()));
                    // Code 6 sets the object's own wait to 20 program ticks (`+0x20 = 0x14`, `FUN_14026cd70`;
                    // `FUN_1402638d0` counts it down before the next move): the reversed shell hangs 20 ticks
                    // where it turned (LIVE `034_Reverse`: P1's Blaster, fired at 65, hits P1 at 88). A Reflect
                    // (code 5) waits 0.
                    p.shot.delay = REVERSE_WAIT_TICKS * moves::SIM_TICKS_PER_TABLE_TICK;
                } else {
                    p.shot.away = true;
                }
            }
            let target = body_centre(players[p.target].target_point());
            if !p.shot.step(Some(target), col) {
                return false;
            }
            let (a, b) = p.shot.swept();
            let clear = |from: Vec3, to: Vec3| {
                let v = to - from;
                let d = v.length();
                d < 1e-3 || col.raycast_los(from, v / d, d, own).is_none()
            };
            // The first body the swept step touches, in slot order (the program tests its
            // colliders in order); a column or a blow that passes through takes all of them.
            let mut touched: Vec<(usize, Vec3)> = Vec::new();
            if !p.harmless {
                let mut cands: Vec<usize> = reach[p.owner].clone();
                if matches!(p.shot.flight, moves::Flight::Tornado(f) if f.wait == 0) {
                    cands.push(p.owner);
                    cands.sort_unstable();
                }
                let radius = p.shot.radius + HIT_BODY_RADIUS;
                for j in cands {
                    if p.shot.pass && p.shot.hit & (1 << j) != 0 {
                        continue;
                    }
                    let c = body_centre(players[j].target_point());
                    let q = moves::closest_on_segment(a, b, c);
                    if q.distance(c) > radius || (!p.shot.through_walls && !clear(a, c)) {
                        continue;
                    }
                    touched.push((j, q));
                    if !p.shot.pass {
                        break;
                    }
                }
            }
            if p.shot.pass {
                // The Tornado's column: everyone in it, once; a Barrier or Reflect does not stop it.
                for (j, _) in touched {
                    p.shot.hit |= 1 << j;
                    hits.push((p.owner, j, p.skill, None, Arrival::Area, via));
                }
            } else if let Some(&(j, _)) = touched.first() {
                hits.push((p.owner, j, p.skill, Some(p.shot.prev), Arrival::Shot, via));
                if p.shot.blast > 0.0 {
                    // "Blast skips the player already hit": on a body or a guard it skips the shot's target.
                    let mask = reach[p.owner].iter().fold(0u8, |m, &v| m | 1 << v) & !(1u8 << p.target);
                    explosions.push((p.owner, p.skill, p.shot.prev, p.shot.blast, mask, via));
                }
                return false;
            }
            // A wall or the floor in the way of the step (a crawler looks along the floor from 3 u up).
            if !p.shot.through_walls && !matches!(p.shot.flight, moves::Flight::Tornado(_)) {
                let low = if matches!(p.shot.flight, moves::Flight::Crawl(_)) { Vec3::Y * 3.0 } else { Vec3::ZERO };
                let (from, to) = (a + low, b + low);
                let v = to - from;
                let len = v.length();
                if let Some((t, obj, face, stored)) = (len > 1e-4).then(|| col.raycast_object_face(from, v / len, len, own)).flatten() {
                    let at = from + v / len * t;
                    // The side the step came from (blasts start there).
                    let n = if face.dot(v) > 0.0 { -face } else { face };
                    // A Ricochet laser turns and flies on (`FUN_14026ca50`); everything else ends. The program hands
                    // it the face's STORED normal (`FUN_140163e80`, contact `+0x38`) and a contact it moves along
                    // (heading . normal >= 0, the back of a face) is ignored: no turn, no bounce counted. Our stage
                    // triangles are wound to that normal; object hulls keep their raw winding, so there the face is
                    // still taken from the side the laser met it (UNCONFIRMED for hulls).
                    if !p.shot.bounce(at, if stored { face } else { n }, Some(target)) {
                        if obj != NO_OBJECT && (!p.harmless || p.brushed) {
                            prop_hits.push((obj, p.owner, p.skill, if p.brushed { Some(props::BRUSHED_WEAR) } else { strength }));
                        }
                        if p.shot.blast > 0.0 && !p.harmless {
                            explosions.push((p.owner, p.skill, at + n, p.shot.blast, everyone[p.owner], via));
                        }
                        return false;
                    }
                }
            }
            if p.shot.life == 0 {
                expired.push((p.id, p.skill));
                return false;
            }
            p.shot.life -= 1;
            p.pos = p.shot.prev.lerp(p.shot.pos, 0.5);
            true
        });
        for (id, skill) in expired {
            self.log(CombatEvent::ShotExpired { id, skill });
        }
        for (attacker, victim, skill) in turned {
            self.log(CombatEvent::Blocked { attacker, victim, skill, broken: false });
        }
        // A thrown object shatters when its flight ends, however it ends.
        let flying: Vec<usize> = self.projectiles.iter().filter_map(|p| p.prop).collect();
        for (k, prop) in self.props.iter_mut().enumerate() {
            if prop.state == PropState::Thrown && !flying.contains(&k) {
                prop.state = PropState::Gone;
            }
        }
        for (obj, owner, skill, strength) in prop_hits {
            let str = strength.unwrap_or_else(|| self.damage(owner, skill)); // a stage object's wear: the victim's stats don't matter
            if let Some(prop) = self.props.iter_mut().find(|p| p.index == obj) {
                prop.wear_down(str);
            }
        }
        for (owner, skill, at, radius, mask, via) in explosions {
            let str = via.strength.unwrap_or_else(|| self.damage(owner, skill));
            self.wear_props_near(at, radius, str);
            self.add_blast_via(owner, skill, at, radius, mask, false, via);
        }
        for (owner, victim, skill, from, arrival, via) in hits {
            self.strike(owner, victim, skill, from, bodies, arrival, via);
        }
    }

    /// A blast or an area going off wears down every standing object it reaches.
    fn wear_props_near(&mut self, at: Vec3, radius: f32, str: i32) {
        for prop in &mut self.props {
            if prop.distance_to(at) <= radius {
                prop.wear_down(str);
            }
        }
    }

    /// Switches the collision of objects that no longer stand (broken,
    /// thrown, gone) off and the rest on. The match calls it every tick, so
    /// the collision always follows the sim state (a new round restores it).
    pub fn sync_collision(&self, col: &mut Collision) {
        for p in &self.props {
            if col.object_gone(p.index) == p.standing() {
                col.set_object_gone(p.index, !p.standing());
            }
        }
    }

    /// Steps the areas and blasts (moves/area.rs): everything counts program
    /// ticks, one per two sim ticks since it was set, with the program's
    /// timelines (docs/MOVES.md): Hole and Rain column warn, slide with the
    /// target's run, then are active; the Rain cloud follows its target and
    /// drops drops; mines count their fuse and go off into a blast that hurts
    /// everyone, their owner too; blasts grow over 30 ticks.
    fn tick_areas(&mut self, players: &[Player], col: &Collision) {
        let bodies = bodies_of(players);
        let reach: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|o| self.victims_of(o).collect());
        // A mine's blast reaches those, and its owner.
        let everyone: [u8; MAX_PLAYERS] = std::array::from_fn(|o| reach[o].iter().fold(1u8 << o, |m, &j| m | 1 << j));
        let alive: Vec<usize> = self.roster.slots().collect();
        let through: Vec<(i16, bool)> = self.areas.iter().map(|a| (a.skill, self.skill(a.skill).is_some_and(|d| d.ignores_walls()))).collect();
        // The unlimited Hole (motion 16) hurts whoever stands in it, its caster included (LIVE, see `place_area`).
        let hits_owner: Vec<(i16, bool)> = self.areas.iter().map(|a| (a.skill, a.spec.kind == Kind::Hole && self.skill(a.skill).is_some_and(|d| d.motion() == moves::motion::HOLE_B))).collect();
        let vias: [Via; MAX_PLAYERS] = std::array::from_fn(|o| Via { button: self.fighters[o].last_slot, strength: None });
        let mut rng = self.rng;
        // (area id, owner, victim, skill)
        let mut strikes: Vec<(u32, usize, usize, i16, Option<i32>)> = Vec::new();
        // (owner, skill, centre, radius, players it reaches)
        let mut explosions: Vec<(usize, i16, Vec3, f32, u8)> = Vec::new();
        // (owner, position, skill): effects for the renderer
        let mut bursts: Vec<(usize, Vec3, i16)> = Vec::new();
        // (owner, where the drop starts, skill): `RainDrop` events (slot 4 of the skill's effect)
        let mut drop_starts: Vec<(usize, Vec3, i16)> = Vec::new();
        self.areas.retain_mut(|a| {
            let stepped = a.age % 2 == 0;
            a.age = a.age.wrapping_add(1);
            if !stepped {
                return true;
            }
            if !a.settled {
                a.settled = true;
                return true;
            }
            let walls_ok = through.iter().find(|t| t.0 == a.skill).is_some_and(|t| t.1);
            let clear = |from: Vec3, to: Vec3| {
                let v = to - from;
                let d = v.length();
                d < 1e-3 || col.raycast_los(from, v / d, d, None).is_none()
            };
            match a.spec.kind {
                // Warning (life >= s65), then active (life < s65): `FUN_14026a7f0`, `FUN_140267410`.
                Kind::Hole | Kind::RainColumn => {
                    let active = a.life < a.spec.active;
                    if !active {
                        // The marker slides along the target's run, sticking to the floor.
                        if a.slide.length_squared() > 0.0 {
                            let to = a.pos + a.slide;
                            match col.ground_below_object(to.x, to.z, a.pos.y + moves::tornado::FLOOR_REACH).filter(|g| *g >= a.pos.y - moves::tornado::FLOOR_REACH) {
                                Some(g) => a.pos = Vec3::new(to.x, g, to.z),
                                None => a.slide = Vec3::ZERO,
                            }
                        }
                    } else {
                        let base = a.pos + Vec3::Y;
                        let first = a.life + 1 == a.spec.active;
                        if first {
                            bursts.push((a.owner, a.pos, a.skill));
                            // A roof over the column's first active tick ends it (unless it penetrates obstacles).
                            if a.spec.kind == Kind::RainColumn && !walls_ok && col.raycast_object(base, Vec3::Y, a.spec.height).is_some() {
                                return false;
                            }
                        }
                        let top = base + Vec3::Y * a.spec.height;
                        let owner_too = hits_owner.iter().find(|t| t.0 == a.skill).is_some_and(|t| t.1);
                        let targets: Vec<usize> = if owner_too { alive.iter().copied().filter(|&v| v == a.owner || reach[a.owner].contains(&v)).collect() } else { reach[a.owner].clone() };
                        for &v in &targets {
                            if a.hit & (1 << v) != 0 {
                                continue;
                            }
                            let c = body_centre(players[v].target_point());
                            let near = if a.spec.kind == Kind::Hole {
                                players[v].grounded && c.distance(base) < a.spec.radius + HIT_BODY_RADIUS && clear(base, c)
                            } else {
                                moves::closest_on_segment(base, top, c).distance(c) <= a.spec.radius + HIT_BODY_RADIUS && (walls_ok || clear(top, c))
                            };
                            if near {
                                strikes.push((a.id, a.owner, v, a.skill, a.strength));
                            }
                        }
                    }
                    if a.life == 0 {
                        return false;
                    }
                    a.life -= 1;
                    true
                }
                // The cloud follows its target and drops drops (`FUN_14026bbd0`, `FUN_14026b8d0`).
                Kind::RainCloud => {
                    let mut taken = a.hit;
                    // The drops already falling (a drop made this tick first moves on the next).
                    a.drops.retain_mut(|d| {
                        d.prev = d.pos;
                        d.pos += d.dir * a.spec.drop_speed;
                        let mut end = false;
                        for &v in &reach[a.owner] {
                            if taken & (1 << v) != 0 {
                                continue;
                            }
                            let c = body_centre(players[v].target_point());
                            let q = moves::closest_on_segment(d.prev, d.pos, c);
                            if q.distance(c) <= area::DROP_RADIUS + HIT_BODY_RADIUS && (walls_ok || clear(d.prev, c)) {
                                taken |= 1 << v;
                                strikes.push((a.id, a.owner, v, a.skill, a.strength));
                                bursts.push((a.owner, q, a.skill));
                                end = true;
                                break;
                            }
                        }
                        if !end && !walls_ok {
                            let v = d.pos - d.prev;
                            let len = v.length();
                            if len > 1e-4 && col.raycast(d.prev, v / len, len).is_some() {
                                bursts.push((a.owner, d.pos, a.skill));
                                end = true;
                            }
                        }
                        if end {
                            return false;
                        }
                        if d.life == 0 {
                            return false;
                        }
                        d.life -= 1;
                        true
                    });
                    if !a.armed {
                        // The cloud holds s69 x 0.1 u over its target and chases at s71 / 10.8 a tick.
                        let t = body_centre(players[a.target].target_point());
                        let dest = Vec3::new(t.x, t.y + a.spec.cloud_height, t.z);
                        let v = dest - a.cloud;
                        let d = v.length();
                        a.cloud = if a.spec.follow <= d { a.cloud + v / d * a.spec.follow } else { dest };
                        a.pos = Vec3::new(a.cloud.x, a.pos.y, a.cloud.z);
                        a.first -= 1;
                        if a.first < 0 && a.pulses_left > 0 {
                            a.gap -= 1;
                            if a.gap < 0 {
                                a.pulses_left -= 1;
                                a.gap = a.spec.every as i16 - 1;
                                // A random point within the spread of the cloud, pointing nearly
                                // straight down (80-100 deg about the horizontal).
                                let (r1, r2, r3) = (xorshift(&mut rng) >> 8, xorshift(&mut rng) >> 8, xorshift(&mut rng) >> 8);
                                let (yaw, pitch, reach_u) = ((r1 & 0x7fff) as f32 / 32768.0 * std::f32::consts::TAU, (80.0 + (r2 & 0x7fff) as f32 / 32768.0 * 20.0).to_radians(), (r3 & 0x7fff) as f32 / 32768.0 * a.spec.radius);
                                let (sy, cy) = yaw.sin_cos();
                                let (sp, cp) = pitch.sin_cos();
                                let up = Vec3::new(sp * sy, cp, sp * cy);
                                let dir = Vec3::new(cp * sy, -sp, cp * cy);
                                let life = ((a.spec.cloud_height + area::DROP_FALL_EXTRA) / a.spec.drop_speed.max(1e-3)) as u16;
                                a.drops.push(RainDrop { prev: a.cloud + up * reach_u, pos: a.cloud + up * reach_u, dir, life });
                                drop_starts.push((a.owner, a.cloud + up * reach_u, a.skill));
                            }
                        }
                        if a.life == 0 {
                            a.armed = true;
                        } else {
                            a.life -= 1;
                        }
                    }
                    !(a.armed && a.drops.is_empty())
                }
                // The mine counts its fuse; Timer goes off at 0, Sensor is armed at 0 and goes off
                // when anyone is near, Remote when its owner presses (`FUN_140267e90`).
                Kind::Mine => {
                    if a.timer > 0 {
                        a.timer -= 1;
                    }
                    let go = match a.spec.trigger {
                        Trigger::Timer => a.timer == 0,
                        Trigger::Sensor { radius } => {
                            if a.timer == 0 {
                                a.armed = true;
                                alive.iter().any(|&j| {
                                    let c = body_centre(players[j].target_point());
                                    c.distance(a.pos) < radius && clear(a.pos + Vec3::Y, c)
                                })
                            } else {
                                false
                            }
                        }
                        Trigger::Remote => {
                            if a.timer == 0 {
                                a.armed = true;
                            }
                            a.armed && a.detonate
                        }
                    };
                    if go {
                        explosions.push((a.owner, a.skill, a.pos, a.spec.radius, everyone[a.owner]));
                        return false;
                    }
                    true
                }
                Kind::Wave => false,
            }
        });
        self.rng = rng;
        for (attacker, pos, skill) in drop_starts {
            self.log(CombatEvent::RainDrop { attacker, pos, skill });
        }
        for (owner, pos, skill) in bursts {
            self.log(CombatEvent::Burst { attacker: owner, pos, skill });
        }
        for (owner, skill, at, radius, mask) in explosions {
            let str = self.damage(owner, skill);
            self.wear_props_near(at, radius, str);
            self.add_blast_via(owner, skill, at, radius, mask, true, vias[owner]);
        }
        for (id, owner, victim, skill, strength) in strikes {
            // Rain, Hole: from above or below, so only an all-round guard covers them.
            if self.strike(owner, victim, skill, None, bodies, Arrival::Area, Via { strength, ..vias[owner] }) {
                if let Some(a) = self.areas.iter_mut().find(|a| a.id == id) {
                    a.hit |= 1 << victim;
                }
            }
        }
        self.tick_blasts(players, col);
    }

    /// Every blast one program tick: its sphere has grown by a thirtieth, each
    /// player it can reach and whose body it touches (with a clear line from
    /// its centre) takes it once (`FUN_14026ae10`, `FUN_14026c5b0`).
    fn tick_blasts(&mut self, players: &[Player], col: &Collision) {
        let bodies = bodies_of(players);
        let mut strikes: Vec<(usize, usize, i16, Vec3, Via)> = Vec::new();
        self.blasts.retain_mut(|b| {
            let stepped = b.age % 2 == 0;
            b.age = b.age.wrapping_add(1);
            if !stepped {
                return true;
            }
            if !b.armed {
                b.armed = true;
                return true;
            }
            let r = area::blast_radius(b.radius, b.life);
            for v in 0..MAX_PLAYERS {
                if b.mask & (1 << v) == 0 {
                    continue;
                }
                let c = body_centre(players[v].target_point());
                let d = c - b.pos;
                let len = d.length();
                // A mine's blast sits ON the floor: a line that starts exactly on a floor triangle can catch it
                // (or its neighbour) at t ~ 0, so it starts 1 u up, as the Sensor Mine's own trigger does.
                let from = if b.area { b.pos + Vec3::Y } else { b.pos };
                let to = c - from;
                if len < r + HIT_BODY_RADIUS && (len < 1e-3 || col.raycast_los(from, to.normalize_or(Vec3::Y), to.length(), None).is_none()) {
                    b.mask &= !(1u8 << v);
                    strikes.push((b.owner, v, b.skill, b.pos, b.via));
                }
            }
            if b.life == 0 {
                return false;
            }
            b.life -= 1;
            true
        });
        // A blast of a shot, a bomb on a wall or a mine alike: the program's object state 3
        // (`FUN_14026dbd0`), which `FUN_140271760` lets through Absorb, Brush and About Face and
        // never sends back (audit lane BLAST3).
        for (owner, victim, skill, at, via) in strikes {
            self.strike(owner, victim, skill, Some(at), bodies, Arrival::Blast, via);
        }
    }

    fn spawn_capsule(&mut self, i: usize, player: &Player, col: &Collision) {
        if !self.rules.capsules {
            return;
        }
        // Accelerate / Quiet Days: capsules come faster or slower.
        let rate = self.env_def().map_or(100, environment::capsule_rate_pct).max(1) as u32;
        let scaled = |ticks: u16| (ticks as u32 * 100 / rate).min(u16::MAX as u32) as u16;

        // The stage's own capsule spots (Call's anchor overrides them): each
        // is full at the start and refills after it was emptied, "a new
        // capsule at the same point": hidden for 420 program ticks (plus the
        // environment's respawn status, `FUN_14014fd10`: 420 + status 34), then
        // appearing for 15 (`CAPSULE_APPEAR_TICKS`) before it can be taken.
        if self.fighters[i].capsule_anchor.is_none() && !self.capsule_spots[i].is_empty() {
            let wait = self.spot_hidden_ticks();
            for k in 0..self.capsule_spots[i].len().min(MAX_CAPSULES_PER_PLAYER) {
                let pos = self.capsule_spots[i][k];
                let taken = self.capsules.iter().any(|c| c.owner == i && c.pos.distance(pos) < 1.0);
                let f = &mut self.fighters[i];
                if taken {
                    f.spot_wait[k] = wait;
                } else if f.spot_wait[k] > 0 {
                    f.spot_wait[k] -= 1;
                } else if let Some(skill) = f.arsenal.pop() {
                    f.spot_wait[k] = wait;
                    // The first fill of a spot is the round's opening deal (silent in the
                    // program, `FUN_14014ee70`); every later one is a respawn that sounds.
                    let respawn = f.spot_opened & (1 << k) != 0;
                    f.spot_opened |= 1 << k;
                    let id = self.take_id();
                    // The opening deal is takeable at once (`FUN_14014ee70` places it
                    // already appeared); only a respawn has the appearing phase.
                    let appear = if respawn { CAPSULE_APPEAR_TICKS } else { 0 };
                    self.capsules.push(CapsuleItem { id, owner: i, skill, pos, appear });
                    self.log(CombatEvent::CapsuleAppeared { who: i, pos, respawn });
                }
            }
            return;
        }

        let every = scaled(CAPSULE_EVERY_TICKS);
        let f = &mut self.fighters[i];
        let around = f.capsule_anchor.unwrap_or(player.pos);
        f.spawn_timer = f.spawn_timer.saturating_add(1);
        if f.spawn_timer < every || f.arsenal.is_empty() || self.capsules.iter().filter(|c| c.owner == i).count() >= MAX_CAPSULES_PER_PLAYER {
            return;
        }
        // A few tries for a clear spot on (about) the player's floor level.
        for _ in 0..6 {
            let a = self.rand_unit() * std::f32::consts::TAU;
            let d = CAPSULE_MIN_DIST + self.rand_unit() * (CAPSULE_MAX_DIST - CAPSULE_MIN_DIST);
            let (x, z) = (around.x + a.sin() * d, around.z + a.cos() * d);
            let Some(y) = col.ground_below(x, z, around.y + 20.0) else { continue };
            let pos = Vec3::new(x, y, z);
            // Same floor as the player (not on top of crates they'd have to climb).
            if (y - around.y).abs() > STEP_HEIGHT ||col.push_out_of_walls(pos, 4.0, STEP_HEIGHT + 1.0, 10.0).distance(pos) > 0.1 {
                continue;
            }
            self.fighters[i].spawn_timer = 0;
            let skill = self.fighters[i].arsenal.pop().unwrap_or(AURA_PARTICLE);
            let id = self.take_id();
            self.capsules.push(CapsuleItem { id, owner: i, skill, pos, appear: 0 });
            self.log(CombatEvent::CapsuleAppeared { who: i, pos, respawn: true });
            return;
        }
    }

}

/// A shot that met a wall at `at` flying with `vel` bounces off it. The
/// collision data gives no surface normals, so try mirroring each axis and
/// keep the first that leads into open space (stages are mostly boxes).
/// xorshift32: the sim's own random stream (`Combat::next_rand`) as a function.
fn xorshift(x: &mut u32) -> u32 {
    *x ^= *x << 13;
    *x ^= *x >> 17;
    *x ^= *x << 5;
    *x
}

/// Stops `f` in their tracks for `ticks` (paralysed / brushed off).
fn stun(f: &mut Fighter, ticks: u16) {
    if f.action != Action::Down {
        f.action = Action::Hit;
        f.action_ticks = f.action_ticks.max(ticks);
        f.pending_shot = None;
        f.bill = None;
    }
}

/// What one tick of a knock-down flight did.
struct FlightStep {
    /// The fall height (highest point of the flight minus the landing point) on
    /// the tick the body touched down.
    landed: Option<f32>,
    /// The body was pushed out of a wall: its horizontal normal.
    wall: Option<Vec3>,
}

/// One tick of a knock-down still in the air: `KNOCKDOWN_GRAVITY` (not the
/// sim's jump gravity), full momentum kept until real ground collision lands
/// it (PROGRAM-RE.md "Knock-down physics" -- the same fall/land pattern as an
/// ordinary jump, just with the program's own, much lower, gravity), pushed
/// out of walls like any body (the contact is reported for the bounce).
/// `can_land` false: on the floor but not allowed to land yet (a WALLHIT clip
/// is playing), so it stays on the floor and the flight goes on.
fn step_knockdown_flight(f: &mut Fighter, p: &mut Player, col: &Collision, can_land: bool) -> FlightStep {
    let peak = f.flight_peak.get_or_insert(p.pos.y);
    *peak = peak.max(p.pos.y);
    f.knock_vel.y -= KNOCKDOWN_GRAVITY * DT;
    let vel = f.knock_vel;
    let mut next = p.pos + vel * DT;
    let pushed = col.push_out_of_walls(next, p.body.wall_radius(), STEP_HEIGHT + 1.0, BODY_HEIGHT);
    let push = Vec3::new(pushed.x - next.x, 0.0, pushed.z - next.z);
    let wall = (push.length() > 1e-3).then(|| push.normalize());
    next.x = pushed.x;
    next.z = pushed.z;
    let ground = col.ground_below(next.x, next.z, p.pos.y.max(next.y) + STEP_HEIGHT);
    let mut landed = None;
    match ground {
        Some(g) if next.y <= g && can_land => {
            p.pos = Vec3::new(next.x, g, next.z);
            f.flying = false;
            // The ground slide picks up from here, horizontal-only.
            f.knock_vel = Vec3::new(vel.x, 0.0, vel.z);
            landed = f.flight_peak.take().map(|top| top.max(next.y) - g);
        }
        Some(g) if next.y <= g => {
            // Down on the floor with a wall clip still playing.
            p.pos = Vec3::new(next.x, g, next.z);
            f.knock_vel.y = 0.0;
        }
        _ => p.pos = next,
    }
    FlightStep { landed, wall }
}

/// One tick of a cast's turn toward its aim (`FUN_14025bd40`): at most the
/// character's turn rate, 20 degrees per program tick = 10 per sim tick, on
/// every tick of the state, before and after the shot leaves.
fn cast_turn(aim: Aim, players: &mut [Player], i: usize) {
    let want = match aim {
        Aim::Keep => return,
        Aim::Yaw(y) => y,
        Aim::Track(v) => yaw_of(toward(players[i].pos, players[v].target_point())),
    };
    players[i].yaw = turn_toward(players[i].yaw, want, TURN_RATE_DEG_PER_S.to_radians() * DT).0;
}

/// Whether a capsule lying at `capsule` (its ground point) can be captured
/// from `player` (their feet): 3-D distance <= `PICKUP_REACH`
/// (`FUN_14014f630`; it measures from the orb 9 u above the ground point, less
/// those 9 u, to the player's position).
pub fn in_pickup_reach(capsule: Vec3, player: Vec3) -> bool {
    capsule.distance(player) <= PICKUP_REACH
}

/// Helpers for rule tests anywhere in the crate (the capsule-colour modules
/// in moves/ use these for their own tests).
#[cfg(test)]
pub mod testkit {
    pub use super::*;
    pub use pd_formats::skills::field as f;
    pub use crate::moves::effects::{Reach, Stat};
    pub use crate::sim::Player;

    /// First motion parameter (s64); `P + k` = s(64 + k).
    pub const P: usize = f::MOTION_PARAMS;

    pub fn floor() -> Collision {
        let s = 1000.0;
        Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
        ])
    }

    /// The rules the skill tests were written against: Level 1, empty
    /// buttons (each test deals its own skills).
    pub const TEST_RULES: Rules = Rules { start_level: 1, start_hand: 0, capsule_respawn_ticks: CAPSULE_RESPAWN_TICKS, set: RuleSet::STANDARD, capsules: true, infinite_uses: false, fixed_hand: None };

    /// A skill with its table numbers: `fields` are (raw index, value) pairs,
    /// copied from the skill's `raw` array in cache/skills.json.
    pub fn def(id: i16, capsule: Capsule, cost: i32, fields: &[(usize, i16)]) -> SkillDef {
        let mut raw = [0i16; 70];
        for &(k, v) in fields {
            raw[k] = v;
        }
        SkillDef {
            id,
            name: String::new(),
            capsule,
            family: raw[f::FAMILY],
            cost,
            strength: raw[f::STRENGTH] as i32,
            strength_rule: raw[f::STRENGTH_RULE],
            requirement: raw[f::REQUIREMENT],
            requirement_amount: raw[f::REQUIREMENT_AMOUNT] as i32,
            level_penalty: raw[f::LEVEL_PENALTY] as i32,
            health_cost: raw[f::HEALTH_COST] as i32,
            defence: raw[f::DEFENCE] as i32,
            raw,
            s70_73: [0; 4],
        }
    }

    /// Two players `dist` apart on a flat floor (player 0 at the origin facing
    /// +Z, player 1 at z = dist facing back), countdown over, no capsules yet.
    pub fn setup(skills: Vec<SkillDef>, dist: f32) -> (Combat, [Player; 2], Collision) {
        let mut c = Combat::new(skills, vec![AURA_PARTICLE; 30]);
        c.set_rules(TEST_RULES);
        c.round_ticks = COUNTDOWN_TICKS;
        let players = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, dist), std::f32::consts::PI)];
        (c, players, floor())
    }

    pub fn run(c: &mut Combat, p: &mut [Player], col: &Collision, ticks: usize) {
        for _ in 0..ticks {
            c.step(p, [0; 2], col);
        }
    }

    /// Player `who` presses (and releases) skill button `slot`.
    pub fn press(c: &mut Combat, p: &mut [Player], col: &Collision, who: usize, slot: usize) {
        let mut b = [0u8; 2];
        b[who] = 1 << slot;
        c.step(p, b, col);
        c.step(p, [0; 2], col);
    }

    /// Player `who` gets Level and Aura 8, `skill` in slot 0, uses it, and the
    /// cast animation plays out (so the skill has left / taken effect).
    pub fn use_skill(c: &mut Combat, p: &mut [Player], col: &Collision, who: usize, skill: i16) {
        c.fighters[who].level = 8;
        c.fighters[who].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[who].slots[0] = Some(skill);
        press(c, p, col, who, 0);
        run(c, p, col, CAST_TICKS as usize + 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testkit::TEST_RULES;
    use crate::sim::{Input, Match};

    use pd_formats::skills::field as f;

    const WAVE: i16 = 1;
    const LASER: i16 = 65;
    const WALL: i16 = 26;
    const BLASTER: i16 = 70;
    const LANCE: i16 = 71;
    const BOMB: i16 = 130;
    const FIRE_WALL: i16 = 140;
    const SHELL: i16 = 27;
    const TREMOR: i16 = 147;
    const RAIN: i16 = 20;
    const TIMER_MINE: i16 = 23;
    const SENSOR_MINE: i16 = 24;
    const REMOTE_MINE: i16 = 25;
    const PHOTON_WAVE: i16 = 63;
    const TWIN_LASERS: i16 = 69;
    const ROOT: i16 = 138;
    const METEOR: i16 = 19;
    const BLOCK: i16 = 300;
    const DIGESTION: i16 = 301;
    const ABSORB_ENERGY: i16 = 302;
    const REFLECT: i16 = 303;
    const GUST: i16 = 304;
    const ABOUT_FACE: i16 = 305;
    const MINDS_EYE: i16 = 306;
    const PARALYZE_BARRIER: i16 = 307;
    const ICE_LANCE: i16 = 308;
    const DHAMPIR: i16 = 309;
    const KI_BULLET: i16 = 191;
    const RAIL: i16 = 14;
    const PURSUIT: i16 = 197;
    const POKE: i16 = 311;
    const BACKDRAFT: i16 = 312;
    const GRAVITY_PRESS: i16 = 313;
    const CURE: i16 = 314;

    fn floor() -> Collision {
        let s = 1000.0;
        Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
        ])
    }

    /// A skill with its real table numbers (`fields`: (field, value) pairs from skills.json).
    fn def(id: i16, capsule: Capsule, family: i16, cost: i32, strength: i32, strength_rule: i16, fields: &[(usize, i16)]) -> SkillDef {
        let mut raw = [0i16; 70];
        raw[f::FAMILY] = family;
        raw[f::STRENGTH] = strength as i16;
        for &(k, v) in fields {
            raw[k] = v;
        }
        let defence = raw[f::DEFENCE] as i32;
        SkillDef { id, name: String::new(), capsule, family, cost, strength, strength_rule, requirement: 0, requirement_amount: 0, level_penalty: 0, health_cost: 0, defence, raw, s70_73: [0; 4] }
    }

    const P: usize = f::MOTION_PARAMS;

    fn skills() -> Vec<SkillDef> {
        vec![
            // Psycho Wave: short-range punch, reach 130.
            def(WAVE, Capsule::Attack, 10, 4, 1, 17, &[(f::MOTION, 13), (P, 130), (P + 1, 8), (P + 2, 30)]),
            // Laser: [Arc], speed 140.
            def(LASER, Capsule::Attack, 15, 1, 1, 0, &[(f::MOTION, 6), (P, -80), (P + 1, 140), (P + 2, 8), (P + 3, 20), (P + 4, 20)]),
            // Crystal Wall: Barrier, time 23, DEF 3, Anti-Pierce.
            def(WALL, Capsule::Defence, 80, 1, 23, 0, &[(43, 2), (f::DEFENCE_KIND, 0), (f::DEFENCE, 3), (f::ANTI_PIERCE, 1)]),
            // Blaster: shell, speed 140, homing 15, STR 4.
            def(BLASTER, Capsule::Attack, 12, 3, 4, 0, &[(P, -30), (P + 1, 140), (P + 4, 15)]),
            // Ki Lance: shell with Pierce, STR 5.
            def(LANCE, Capsule::Attack, 12, 3, 5, 0, &[(P, -30), (P + 1, 100), (P + 4, 20), (f::PIERCE, 1)]),
            // Bomb: [Parabola] 90 / 4, explodes (radius 300), STR 3.
            def(BOMB, Capsule::Attack, 16, 3, 3, 0, &[(f::MOTION, 1), (P, 90), (P + 2, 4), (P + 3, 10), (f::ADD_EFFECT, 5), (f::ADD_AMOUNT, 300)]),
            // Wall of Fire: Barrier, DEF 6, no Anti-Pierce (DEF lowered to 3 for the pierce test).
            def(FIRE_WALL, Capsule::Defence, 80, 2, 23, 0, &[(43, 2), (f::DEFENCE_KIND, 0), (f::DEFENCE, 3)]),
            // Psycho Shell: [Shelter], time 28, DEF 6.
            def(SHELL, Capsule::Defence, 82, 3, 28, 0, &[(43, 2), (f::DEFENCE_KIND, 1), (f::DEFENCE, 6), (f::ANTI_PIERCE, 1)]),
            // Tremor: [Hole] 30 10 300, STR 3.
            def(TREMOR, Capsule::Attack, 75, 3, 3, 0, &[(f::MOTION, 16), (P, 30), (P + 1, 10), (P + 2, 300)]),
            // Rain of Stones: [Rain] 30 30 6 5 0 1000, STR 3.
            def(RAIN, Capsule::Attack, 21, 3, 3, 0, &[(f::MOTION, 20), (P, 30), (P + 1, 30), (P + 2, 6), (P + 3, 5), (P + 5, 1000)]),
            // Mines: timer 0 0 150 500, sensor 1 300 60 500, remote 2 0 0 300; STR 3.
            def(TIMER_MINE, Capsule::Attack, 50, 1, 3, 0, &[(f::MOTION, 8), (P + 2, 150), (P + 3, 500)]),
            def(SENSOR_MINE, Capsule::Attack, 50, 2, 3, 0, &[(f::MOTION, 8), (P, 1), (P + 1, 300), (P + 2, 60), (P + 3, 500)]),
            def(REMOTE_MINE, Capsule::Attack, 100, 1, 3, 0, &[(f::MOTION, 8), (P, 2), (P + 3, 300)]),
            // Photon Wave: explodes around self, 400, STR 4.
            def(PHOTON_WAVE, Capsule::Attack, 11, 4, 4, 0, &[(f::MOTION, 9), (P, 400)]),
            // Double Lasers: [Arc] -80 160 8 -25 -15 2.
            def(TWIN_LASERS, Capsule::Attack, 15, 2, 1, 0, &[(f::MOTION, 6), (P, -80), (P + 1, 160), (P + 2, 8), (P + 3, -25), (P + 4, -15), (P + 5, 2)]),
            // Root of Tree: [Crawl] -80 60 0 0 500 100, STR 6.
            def(ROOT, Capsule::Attack, 18, 2, 6, 0, &[(f::MOTION, 5), (P, -80), (P + 1, 60), (P + 4, 500), (P + 5, 100)]),
            // Defences, with their kinds (s44), times (s46), DEF (s47), field (s45) and add-effects (s51/s52).
            def(BLOCK, Capsule::Defence, 84, 2, 18, 0, &[(43, 2), (f::DEFENCE_KIND, 3), (f::DEFENCE, 2), (f::STRENGTH_RULE, 130)]),
            def(DIGESTION, Capsule::Defence, 83, 3, 23, 0, &[(43, 2), (f::DEFENCE_KIND, 5), (f::DEFENCE, 2), (f::ADD_EFFECT, 1)]),
            def(ABSORB_ENERGY, Capsule::Defence, 83, 2, 23, 0, &[(43, 2), (f::DEFENCE_KIND, 5), (f::DEFENCE, 4), (f::ADD_EFFECT, 2)]),
            def(REFLECT, Capsule::Defence, 81, 1, 28, 0, &[(43, 2), (f::DEFENCE_KIND, 4), (f::DEFENCE, 4)]),
            def(GUST, Capsule::Defence, 85, 2, 28, 0, &[(43, 2), (f::DEFENCE_KIND, 2), (f::STRENGTH_RULE, 800)]),
            def(ABOUT_FACE, Capsule::Defence, 86, 2, 28, 0, &[(43, 2), (f::DEFENCE_KIND, 6), (f::STRENGTH_RULE, 130)]),
            def(MINDS_EYE, Capsule::Defence, 89, 5, 10, 0, &[(43, 2), (f::DEFENCE_KIND, 7), (f::DEFENCE, 5), (f::USES, 1)]),
            def(PARALYZE_BARRIER, Capsule::Defence, 80, 4, 28, 0, &[(43, 2), (f::DEFENCE_KIND, 0), (f::DEFENCE, 2), (f::ADD_EFFECT, 4), (f::ADD_AMOUNT, 90)]),
            // Ice Lance: shell, STR 2, freezes buttons 300. Dhampir Claw: punch, STR 1, health += damage.
            def(ICE_LANCE, Capsule::Attack, 12, 1, 2, 0, &[(P, -30), (P + 1, 120), (P + 4, 10), (f::ADD_EFFECT, 7), (f::ADD_AMOUNT, 300)]),
            def(DHAMPIR, Capsule::Attack, 10, 1, 2, 0, &[(f::MOTION, 13), (P, 130), (P + 1, 8), (P + 2, 30), (f::ADD_EFFECT, 9)]),
            // Ki Bullet: shell, reactions 3 3 3 ("Knocks down target"), STR 1.
            def(KI_BULLET, Capsule::Attack, 12, 1, 1, 0, &[(P, -30), (P + 1, 110), (P + 4, 16), (57, 3), (58, 3), (59, 3)]),
            // Rail Gun-like shell with ordinary reactions 46 2 3 (knocks down only in the air), STR 2.
            def(RAIL, Capsule::Attack, 12, 1, 2, 0, &[(P, -80), (P + 1, 300), (P + 4, 3), (57, 46), (58, 2), (59, 3)]),
            // Pursuit: reaction 24 ("Requires: Knocked down target"), STR 3.
            def(PURSUIT, Capsule::Attack, 100, 1, 3, 0, &[(P, -30), (P + 1, 200), (P + 4, 30), (57, 24), (58, 24), (59, 24)]),
            // Meteor: [Fall] 6 18 80 15, STR 6.
            def(METEOR, Capsule::Attack, 20, 3, 6, 0, &[(f::MOTION, 2), (P, 6), (P + 1, 18), (P + 2, 80), (P + 3, 15)]),
            // A shell whose reactions are its own rows (light/medium/heavy
            // selection tests), STR 2.
            def(POKE, Capsule::Attack, 12, 1, 2, 0, &[(P, -30), (P + 1, 300), (P + 4, 3), (57, 201), (58, 202), (59, 203)]),
            // Backdraft Bullet: s62 = 6 ("Effective only against knocked down targets"), STR 2.
            def(BACKDRAFT, Capsule::Attack, 12, 1, 2, 0, &[(P, -30), (P + 1, 300), (P + 4, 3), (57, 46), (58, 46), (59, 46), (f::TARGET_STATES, 6)]),
            // Gravity Press: s62 = 7 ("Effective against downed opponents", also normal), STR 3.
            // A plain shell standing in for Gravity Press's s62 = 7 (real
            // Gravity Press is a [Hole], but the [Hole] warn-then-land timing
            // isn't what this test is about -- only the s62 gate is).
            def(GRAVITY_PRESS, Capsule::Attack, 12, 1, 3, 0, &[(P, -30), (P + 1, 300), (P + 4, 3), (57, 46), (58, 46), (59, 46), (f::TARGET_STATES, 7)]),
            // Cure: a green Status capsule stand-in. Never cast in these
            // tests -- only captured, so its family/motion don't matter.
            def(CURE, Capsule::Status, 0, 1, 0, 0, &[]),
        ]
    }

    fn setup(dist: f32) -> (Combat, [Player; 2], Collision) {
        let skills = skills();
        let mut c = Combat::new(skills, vec![AURA_PARTICLE; 30]);
        c.set_rules(TEST_RULES);
        c.round_ticks = COUNTDOWN_TICKS; // skip the countdown
        let players = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, dist), std::f32::consts::PI)];
        (c, players, floor())
    }

    fn run(c: &mut Combat, p: &mut [Player], col: &Collision, buttons: [u8; 2], ticks: usize) {
        for _ in 0..ticks {
            c.step(p, buttons, col);
        }
    }

    fn press(c: &mut Combat, p: &mut [Player], col: &Collision, who: usize, slot: usize) {
        let mut b = [0u8; 2];
        b[who] = 1 << slot;
        c.step(p, b, col);
        c.step(p, [0; 2], col);
    }

    #[test]
    fn capturing_a_capsule_logs_the_event_the_pickup_sound_plays_on() {
        // Standing on your own capsule and pressing a button captures it into
        // that slot instead of casting (combat.rs, the `Some(k)` arm). That arm
        // logs `CombatEvent::Capsule`, which is what `sfx::cues_for_event` maps
        // to the original's pickup sound (`common/34`) - the sound Laurence
        // noticed missing. Without the log the capture is silent.
        let (mut c, mut p, col) = setup(100.0);
        let skill = 5;
        c.capsules.push(CapsuleItem { id: 9999, owner: 0, skill, pos: p[0].pos, appear: 0 });
        assert_eq!(c.capsule_in_reach(0, &p[0]), Some(0), "capsule should be in reach");

        c.events.clear();
        let before = c.fighters[0].slots[1];
        press(&mut c, &mut p, &col, 0, 1);

        assert!(c.capsules.is_empty(), "the capsule was taken off the field");
        assert_eq!(c.fighters[0].slots[1], Some(skill), "and went into slot 1");
        assert_ne!(c.fighters[0].slots[1], before);
        assert_eq!(
            c.events.iter().map(|(_, e)| *e).find(|e| matches!(e, CombatEvent::Capsule { .. })),
            Some(CombatEvent::Capsule { who: 0, slot: 1, skill }),
            "capturing a capsule must log exactly one Capsule event for the sound"
        );
    }

    #[test]
    fn capturing_someone_elses_capsule_is_not_a_pickup() {
        // Only your own capsules are captured (`capsule_in_reach` filters on
        // owner), so walking over the opponent's orb must log nothing.
        let (mut c, mut p, col) = setup(100.0);
        c.capsules.push(CapsuleItem { id: 9998, owner: 1, skill: 5, pos: p[0].pos, appear: 0 });
        assert_eq!(c.capsule_in_reach(0, &p[0]), None);
        c.events.clear();
        press(&mut c, &mut p, &col, 0, 1);
        assert!(!c.events.iter().any(|(_, e)| matches!(e, CombatEvent::Capsule { .. })));
        assert_eq!(c.capsules.len(), 1, "their capsule is still on the field");
    }

    /// A two-player match on the floor, P1 holding full stick forward at the
    /// press; returns the match after the press tick and P1's position then.
    fn held_stick_cast(skills: Vec<SkillDef>, rows: &[(i16, &str, i16)], slot_skill: i16) -> (Match, Vec3) {
        let mut col = floor();
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 600.0)], Combat::new(skills, vec![AURA_PARTICLE; 30]));
        m.combat.set_rules(TEST_RULES);
        m.combat.round_ticks = COUNTDOWN_TICKS; // skip the countdown
        for &(id, clip, fire) in rows {
            m.combat.set_action_row(id, Some(ActionRow { anim_name: clip.into(), cast_sound: 0, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: fire, hit_window_end: 0 }), None);
        }
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(slot_skill);
        let run = Input { stick: [0, 127], ..Default::default() };
        m.step([Input { skills: 1, ..run }, Input::default()], &mut col);
        let at = m.players[0].pos;
        (m, at)
    }

    /// Ticks after the press tick during which P1 (stick held forward the
    /// whole time) did not move at all.
    fn still_ticks(m: &mut Match, at: Vec3, limit: usize) -> usize {
        let mut col = floor();
        let run = Input { stick: [0, 127], ..Default::default() };
        let mut n = 0;
        while n < limit {
            m.step([run, Input::default()], &mut col);
            if (m.players[0].pos - at).length() > 1e-3 {
                break;
            }
            n += 1;
        }
        n
    }

    #[test]
    fn a_cast_with_a_real_clip_holds_the_fighter_for_the_whole_clip() {
        // The program's rule (caststill lane): `0x140258180` -> `0x14025a3e0`
        // enters the cast state (10 ground / 11 air) for every skill whose
        // action row names a clip; the state's handler `0x14025bd40` only
        // turns the body, it never reads the stick, and the timer is the clip's
        // length (`0x140259d10` with len - combo offset), frozen only under a
        // held charge. A green (Status) skill on SPE_POWER_UP is 40 frames =
        // 80 sim ticks; the press tick itself already moved (movement runs
        // before the button arm), so 80 ticks stand still after it (and the entry tick).
        const GREEN: i16 = 9101;
        let mut skills = skills();
        skills.push(def(GREEN, Capsule::Status, 0, 1, 0, 0, &[(43, 4), (45, 1)]));
        let (mut m, at) = held_stick_cast(skills, &[(GREEN, "SPE_POWER_UP", 20)], GREEN);
        assert_eq!(m.combat.fighters[0].action, Action::Cast, "the press started the cast state");
        let held = still_ticks(&mut m, at, 200);
        // 82 locked ticks (clip + 1 program tick), then the movement-entry tick that moves nothing (`sim.rs` "ENTRY").
        assert_eq!(held, 82 + 1, "stick held forward: no displacement for the clip's 40 frames + 1 program tick = 82 ticks (LIVE castpose: state 10 lasts clip + 1)");
        assert_eq!(m.combat.fighters[0].action, Action::Ready, "free again once the clip is over");
    }

    #[test]
    fn a_state_starting_effect_ends_the_cast_hold_at_the_fire_tick() {
        // READ `FUN_140270b00` kinds 11 / 15 / 20 start player state 21 / 22 / 24 at the fire tick, replacing
        // the cast state (LIVE takes_full 238 / 298 / 300: state 10 for exactly the fire tick, 20). Kind 10
        // (Level Boost) does not: it holds the whole clip.
        const REGEN: i16 = 9111;
        const BOOST: i16 = 9112;
        let mut skills = skills();
        skills.push(def(REGEN, Capsule::Status, 0, 1, 0, 0, &[(43, 4), (45, 11)]));
        skills.push(def(BOOST, Capsule::Status, 0, 1, 0, 0, &[(43, 4), (45, 10)]));
        let (mut m, _) = held_stick_cast(skills.clone(), &[(REGEN, "SPE_HEAL", 20)], REGEN);
        let mut col = floor();
        for _ in 0..moves::table_ticks(20) + 4 {
            m.step([Input::default(), Input::default()], &mut col);
        }
        assert_eq!(m.combat.fighters[0].action, Action::Ready, "free right after the fire tick");
        let (mut m, _) = held_stick_cast(skills, &[(BOOST, "SPE_HEAL", 20)], BOOST);
        m.combat.fighters[0].action_ticks = 200; // a long clip
        for _ in 0..moves::table_ticks(20) + 4 {
            m.step([Input::default(), Input::default()], &mut col);
        }
        assert_eq!(m.combat.fighters[0].action, Action::Cast, "Level Boost keeps the cast hold");
    }

    #[test]
    fn the_cast_hold_is_the_clip_plus_one_program_tick() {
        // LIVE castpose: takes_full state 10 lasts press + clip length + 1 (Meteor ATK_WHAND_SPIKE 53 -> 54,
        // Level Boost SPE_HEAL 40 -> 41); READ 0x14025bd40 counts the timer down to below 1.
        let c = Combat::new(skills(), vec![]);
        let row = ActionRow { anim_name: "SPE_HEAL".into(), ..Default::default() };
        assert_eq!(c.cast_hold_ticks(0, Some(&row)), c.clip_hold_ticks(0, "SPE_HEAL") + 2);
    }

    #[test]
    fn using_an_aura_particle_holds_nobody_and_fires_at_once() {
        // The program: the Aura Particle (record 0, action row 343) names clip 0 (`WAIT0`),
        // so `0x14025a3e0` returns 0 and `0x140258180` calls `0x1402626d0` straight away,
        // never leaving the idle / walk states. LIVE PROOF on the PC original (moverec, four
        // runs, `~/.pdtools/moverec/takes/000_Aura_Particle_held_stick_trace.csv` and
        // `pilot_run2.log`): stick held forward through the use, speed 1.85 u per program
        // tick before, at and after the press, state "run", Level 3 -> 4, sound 89.
        let (mut m, at) = held_stick_cast(skills(), &[], AURA_PARTICLE);
        assert_eq!(m.combat.fighters[0].level, 9, "Level +1 is paid at the press");
        assert_eq!(m.combat.fighters[0].action, Action::Ready, "no cast state");
        assert!(!m.combat.fighters[0].rooted());
        assert!(m.combat.fighters[0].absorb_glow_ticks > 0, "the green glow still shows (render-only)");
        let mut col = floor();
        let run = Input { stick: [0, 127], ..Default::default() };
        let mut last = at;
        for t in 0..30 {
            m.step([run, Input::default()], &mut col);
            let step = (m.players[0].pos - last).length();
            assert!(step > 0.9, "tick {t}: still running through the use ({step} u)");
            last = m.players[0].pos;
        }
    }

    #[test]
    fn capturing_a_capsule_holds_nobody() {
        // A capture changes no fighter state in the program (`0x1402517c0` ->
        // `0x140261a00`, `0x140262670` set the slot only), whatever the capsule's colour.
        // (The moverec traces logged no capture, `capsule_touched: false` in every take.)
        for skill in [AURA_PARTICLE, CURE, LASER] {
            let mut col = floor();
            let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], Combat::new(skills(), vec![AURA_PARTICLE; 30]));
            m.combat.set_rules(TEST_RULES);
            m.combat.round_ticks = COUNTDOWN_TICKS;
            m.combat.capsules.push(CapsuleItem { id: 9999, owner: 0, skill, pos: Vec3::ZERO, appear: 0 });
            let run = Input { stick: [0, 127], ..Default::default() };
            m.step([Input { skills: 1, ..run }, Input::default()], &mut col);
            assert!(m.combat.capsules.is_empty(), "captured {skill}");
            assert_eq!(m.combat.fighters[0].slots[0], Some(skill));
            assert!(!m.combat.fighters[0].rooted(), "no hold after capturing {skill}");
            let at = m.players[0].pos;
            m.step([run, Input::default()], &mut col);
            m.step([run, Input::default()], &mut col);
            assert!((m.players[0].pos - at).length() > 0.5, "keeps running after capturing {skill}");
        }
    }

    #[test]
    fn absorb_age_counts_an_aura_particle_use_and_nothing_else() {
        // Read-only accessor for the renderer's glow (`aura_pulse.rs`).
        let (mut m, _) = held_stick_cast(skills(), &[], AURA_PARTICLE);
        let first = m.combat.fighters[0].absorb_age().expect("the use is running");
        assert!(first <= 1, "starts at the press (age {first})");
        assert_eq!(AURA_GLOW_TICKS, 53, "frame 160 -> 186.5 at 30 fps");
        let mut col = floor();
        let mut last = first;
        for _ in 0..(AURA_GLOW_TICKS as usize + 4) {
            m.step([Input::default(), Input::default()], &mut col);
            match m.combat.fighters[0].absorb_age() {
                Some(a) => {
                    assert!(a >= last && a < AURA_GLOW_TICKS, "age {a} runs forward inside the 53-tick glow");
                    last = a;
                }
                None => break,
            }
        }
        assert!(last >= AURA_GLOW_TICKS - 2, "ran to the end of the glow (last {last})");
        assert_eq!(m.combat.fighters[0].absorb_age(), None, "off once the glow is over");
        assert_eq!(m.combat.fighters[1].absorb_age(), None, "the other fighter never used one");
    }

    #[test]
    fn an_aura_particle_use_does_not_pause_the_aura_refill() {
        // The program has no cast state for record 0, so nothing pauses the
        // Aura refill (`FUN_14024de20`, `FUN_1402523a0`); a green cast does
        // pause it (states 10-13).
        const GREEN: i16 = 9101;
        let gained = |slot_skill: i16| {
            let mut skills = skills();
            skills.push(def(GREEN, Capsule::Status, 0, 0, 0, 0, &[(43, 4), (45, 1)]));
            let (mut m, _) = held_stick_cast(skills, &[(GREEN, "SPE_POWER_UP", 20)], slot_skill);
            m.combat.fighters[0].aura_ticks = 2 * AURA_TICKS_PER_POINT;
            m.combat.fighters[0].aura_frac = 0.0;
            let mut col = floor();
            for _ in 0..40 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            assert_eq!(m.combat.fighters[0].action == Action::Cast, slot_skill == GREEN, "only the real cast is in a cast state");
            m.combat.fighters[0].aura_ticks - 2 * AURA_TICKS_PER_POINT
        };
        assert!(gained(AURA_PARTICLE) > 0, "the refill keeps running through an Aura Particle use");
        assert_eq!(gained(GREEN), 0, "a real cast pauses it (`0x1402523a0`)");
    }

    /// Events logged after sequence number `seen` (the probe's way of watching a match).
    fn events_since(m: &Match, seen: u32) -> Vec<CombatEvent> {
        m.combat.events.iter().filter(|e| e.0 > seen).map(|e| e.1).collect()
    }

    #[test]
    fn a_row_naming_clip_zero_fires_in_the_press_and_holds_nobody() {
        // The ten instant roster casts (Jump, Void, Levitate, Surprise Attack, Demon's Wing,
        // Return, Optical Camo, Skywalk, Retreat, Charge): their action row names clip 0 (WAIT0),
        // so `0x14025a3e0` returns 0 and `0x140258180` calls `0x1402626d0` at once -- no cast
        // state 10 / 11, no clip hold, no countdown to the row's fire tick (Optical Camo, Void and
        // Demon's Wing carry fire tick 20 on their WAIT0 row and it is not used). Supersedes the
        // 2026-10-01 caststill decision that kept an 80-tick hold for them (DECISIONS.md).
        const INSTANT: i16 = 9103;
        let mut skills = skills();
        // A green self skill (Optical Camo shape) with fire tick 20 on a WAIT0 row.
        skills.push(def(INSTANT, Capsule::Status, 0, 1, 0, 0, &[(43, 4), (45, 1)]));
        let (mut m, at) = held_stick_cast(skills.clone(), &[(INSTANT, "WAIT0", 20)], INSTANT);
        let f = &m.combat.fighters[0];
        assert_eq!(f.action, Action::Ready, "no cast state");
        assert!(!f.rooted());
        assert_eq!(f.cast_seq, 1, "the cast still starts (its sound and effect key off the sequence)");
        assert!(m.combat.events.iter().any(|e| matches!(e.1, CombatEvent::SkillFired { who: 0, skill: INSTANT })), "fired in the press, not 40 ticks later");
        assert_eq!(f.aura(), 7, "and paid in the press (8 - 1)");
        assert_eq!(still_ticks(&mut m, at, 200), 0, "stick held forward: the body never stops");
        // The same skill on a real clip is held for it (control).
        let (mut held, at) = held_stick_cast(skills, &[(INSTANT, "SPE_POWER_UP", 20)], INSTANT);
        assert_eq!(held.combat.fighters[0].action, Action::Cast);
        assert_eq!(still_ticks(&mut held, at, 200), 82 + 1);
    }

    #[test]
    fn an_instant_cast_in_the_air_does_not_freeze_the_body() {
        // State 11 (the air cast) zeroes `vy` and skips gravity, but an instant cast never enters it.
        const INSTANT: i16 = 9103;
        let mut skills = skills();
        skills.push(def(INSTANT, Capsule::Status, 0, 1, 0, 0, &[(43, 4), (45, 1), (40, 2)]));
        let mut col = floor();
        let mut m = Match::new([Vec3::new(0.0, 80.0, 0.0), Vec3::new(0.0, 0.0, 600.0)], Combat::new(skills, vec![AURA_PARTICLE; 30]));
        m.combat.set_rules(TEST_RULES);
        m.combat.round_ticks = COUNTDOWN_TICKS;
        m.combat.set_action_row(INSTANT, Some(ActionRow { anim_name: "WAIT0".into(), ..Default::default() }), None);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(INSTANT);
        m.players[0].grounded = false;
        m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
        assert!(m.combat.airborne[0], "the press happened in the air");
        assert_eq!(m.combat.fighters[0].action, Action::Ready, "no state 11");
        assert!(m.combat.fighters[0].cast_seq > 0, "it was cast");
        assert!(!m.combat.fighters[0].air_cast);
    }

    #[test]
    fn recollection_and_copy_cast_the_copied_skills_row_not_their_own() {
        // Their own row names clip 0, but the press resolves them to the copied skill
        // (PROGRAM-RE.md: "none (resolved when the button is pressed)"): the cast is the copied
        // skill's, with its clip and fire tick. So they are not "instant" here (the 13 table
        // records whose row names clip 0 include 205 / 206 only as records).
        const RECOLLECTION: i16 = 206;
        const COPIED: i16 = 9104;
        let mut skills = skills();
        skills.push(def(RECOLLECTION, Capsule::Attack, 2, 1, 0, 0, &[(42, 100), (43, 6), (44, 0)]));
        skills.push(def(COPIED, Capsule::Attack, 12, 1, 1, 0, &[(P, -30), (P + 1, 100)]));
        let mut col = floor();
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 600.0)], Combat::new(skills, vec![AURA_PARTICLE; 30]));
        m.combat.set_rules(TEST_RULES);
        m.combat.round_ticks = COUNTDOWN_TICKS;
        m.combat.set_action_row(RECOLLECTION, Some(ActionRow { anim_name: "WAIT0".into(), ..Default::default() }), None);
        m.combat.set_action_row(COPIED, Some(ActionRow { anim_name: "ATK_SHOT".into(), fire_tick: 20, ..Default::default() }), None);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(RECOLLECTION);
        m.combat.fighters[0].last_attack = Some(COPIED);
        m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
        let f = &m.combat.fighters[0];
        assert_eq!((f.action, f.cast_skill, f.fire_ticks), (Action::Cast, COPIED, 40), "the copied skill's cast: its wind-up, held");
        assert!(m.combat.projectiles.is_empty(), "nothing leaves in the press");
    }

    #[test]
    fn an_instant_effect_on_the_opponent_lands_s65_program_ticks_after_its_fire_tick() {
        // `FUN_140265ea0` stores s65 as the object's countdown (target kind 0, motion 4) and
        // `FUN_140265f50` applies the effect when it is spent: 25 program ticks for Purify, Fairy
        // Ring, Quantum Decay, Armaros, Chill Breeze, Blessed Rain..., 5 for Frighten, 0 for the
        // crystal erasers. A program tick is 2 sim ticks (`SIM_TICKS_PER_TABLE_TICK`: the program
        // runs at 30 Hz, the sim at 60 Hz), and the object's first update only arms it (state 1 -> 2,
        // `FUN_1402638d0`, as a shot's first update does), so the effect lands on fire tick +
        // 2 x (s65 + 1) (LIVE Meltdown / Quantum Decay / Quantum Amp: fire 69, Level changes at 95).
        // Fusion's "average both Healths" is the observable (Add code 46).
        const PURIFY_LIKE: i16 = 9102;
        for (s65, fire_tick) in [(25, 20), (5, 20), (0, 20), (25, 0)] {
            let mut skills = skills();
            skills.push(def(PURIFY_LIKE, Capsule::Special, 40, 1, 0, 0, &[(43, 0), (f::MOTION, 4), (P + 1, s65), (f::ADD_EFFECT, 46)]));
            let (mut m, _) = held_stick_cast(skills, &[(PURIFY_LIKE, "SPE_POWER_UP", fire_tick)], PURIFY_LIKE);
            m.combat.fighters[0].health = 10;
            m.combat.fighters[1].health = 20;
            let seen = m.combat.events.last().map_or(0, |e| e.0);
            let mut col = floor();
            let (mut fired, mut landed) = (None, None);
            // Step 0 was the press (inside `held_stick_cast`); count from there.
            for k in 1..=400u32 {
                m.step([Input::default(), Input::default()], &mut col);
                if fired.is_none() && events_since(&m, seen).iter().any(|e| matches!(e, CombatEvent::SkillFired { who: 0, skill: PURIFY_LIKE })) {
                    fired = Some(k);
                }
                if landed.is_none() && m.combat.fighters[1].health != 20 {
                    landed = Some(k);
                }
            }
            let (fired, landed) = (fired.expect("fired"), landed.expect("landed"));
            // LIVE (takes_full_trace/240_Dash_trace.csv: press tick 39, fire tick 40): a real clip with w43 = 0 fires
            // one program tick (2 sim ticks) after the press, like w43 = 1 (`FUN_14025c720`'s first update).
            assert_eq!(fired, 2 * fire_tick.max(1) as u32, "fire tick {fire_tick}: press + the row's own countdown (at least one program tick)");
            let arming = if s65 > 0 { 2 } else { 0 };
            assert_eq!(landed, fired + 2 * s65 as u32 + arming, "s65 = {s65}: the opponent changes 2 x (s65 + 1) sim ticks after the fire tick (at once when s65 = 0)");
            assert_eq!(m.combat.fighters[1].health, 15, "Healths averaged once");
        }
    }

    /// The program's rule, derived from the data it reads (`0x14025a3e0`: a row
    /// whose clip is 0 = `WAIT0`, or no row, returns 0 and the skill fires at
    /// once with no cast state; every other row enters state 10/11 for the
    /// clip's length). Skipped when the converted cache is not present.
    #[test]
    fn the_instant_skills_are_the_ones_whose_row_names_no_clip() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut instant = Vec::new();
        let mut rooted_by_capsule = std::collections::BTreeMap::<String, usize>::new();
        // The FIRST record of an id: ids 257 and 294 occur twice and the second
        // record is a character's dodge move (records 749 / 750), not the skill.
        for s in crate::first_skill_records(&json) {
            let id = s["id"].as_i64().unwrap();
            if id > 340 {
                continue; // enemy-only ids 341+ are outside the versus roster
            }
            let clip = s["action"]["ground"]["anim_id"].as_i64();
            if clip.is_none() || clip == Some(0) {
                instant.push(id);
            } else {
                *rooted_by_capsule.entry(s["capsule"].as_str().unwrap().to_string()).or_default() += 1;
            }
        }
        instant.sort();
        // Skywalk 51, Levitate 56, Return 57, Surprise Attack 58, Retreat 60, Optical Camo 115,
        // Copy 205, Recollection 206, Void 235, Jump 239, Demon's Wing 292, Charge 304
        // (+ the Aura Particle, record 0, outside the table). Id 257 is NOT one: Judge of
        // Anubis casts ATK_FAITH_SHOT; the WAIT0 record is the second 257, a dodge move.
        assert_eq!(instant, vec![51, 56, 57, 58, 60, 115, 205, 206, 235, 239, 292, 304], "skills the program fires at once");
        // Every other skill of the 340 is rooted for its clip: 29 green (Status), 32 Special, ...
        assert_eq!(rooted_by_capsule.values().sum::<usize>(), 340 - 12, "{rooted_by_capsule:?}");
        assert_eq!(rooted_by_capsule["status"], 29, "{rooted_by_capsule:?}");
    }

    #[test]
    fn laser_costs_aura_stays_on_its_button_and_damages() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].slots[0] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 0);
        assert_eq!(c.fighters[0].aura(), 1, "nothing is paid at the press");
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[0].slots[0], Some(LASER), "Laser is unlimited-use (s26 = 0)");
        assert!(c.fighters[0].used.is_empty(), "not consumed");
        assert_eq!(c.fighters[0].aura(), 0, "Laser costs 1 of the starting 1, paid when it leaves");
        assert_eq!(c.fighters[1].health, START_HEALTH - 1);
        assert_eq!(c.fighters[1].action, Action::Hit);
        assert_eq!(c.events.last().map(|e| e.1), Some(CombatEvent::Hit { attacker: 0, victim: 1, skill: LASER }));
    }

    /// S1a: every combat event names who did it, so sound, fx and stats stop
    /// guessing "the other player".
    #[test]
    fn hit_blocked_and_burst_events_name_the_attacker() {
        // Player 1 is the attacker this time round.
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 1, LASER);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert!(c.events.iter().any(|e| e.1 == CombatEvent::Hit { attacker: 1, victim: 0, skill: LASER }), "{:?}", c.events);
        // A blocked shot: attacker 1, victim (and guard holder) 0.
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].guard = Some(ActiveGuard { ticks: 100, ..ActiveGuard::new(WALL, barrier::Guard::from_def(c.skill(WALL).unwrap()).unwrap(), 0) });
        cast_skill(&mut c, &mut p, &col, 1, LASER);
        run(&mut c, &mut p, &col, [0; 2], 90);
        assert!(c.events.iter().any(|e| matches!(e.1, CombatEvent::Blocked { attacker: 1, victim: 0, skill: LASER, .. })), "{:?}", c.events);
        // A bomb's blast is owned by whoever threw it.
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 1, BOMB);
        run(&mut c, &mut p, &col, [0; 2], 200);
        assert!(c.events.iter().any(|e| matches!(e.1, CombatEvent::Burst { attacker: 1, skill: BOMB, .. })), "{:?}", c.events);
    }

    /// S1a: `strike` and `damage_to` take the victim as a parameter; the
    /// `1 - attacker` convention is only `foe_of`'s business now.
    #[test]
    fn strike_and_damage_take_their_victim_explicitly() {
        let (mut c, p, _col) = setup(100.0);
        c.fighters[0].level = 2;
        c.fighters[1].level = 5;
        let laser = c.skills.iter().position(|d| d.id == LASER).unwrap();
        c.skills[laser].strength_rule = str_rule::TARGET_LEVEL;
        assert_eq!((c.foe_of(0), c.foe_of(1)), (1, 0));
        // "STR = target's Level": the victim's, whoever they are.
        assert_eq!(c.damage_to(0, 1, LASER), 5);
        assert_eq!(c.damage_to(1, 0, LASER), 2);
        assert_eq!(c.damage(0, LASER), c.damage_to(0, c.foe_of(0), LASER), "the two-argument form is the foe shorthand");
        // `strike` lands on the victim it is given and scores the knock-out for the attacker.
        let bodies = bodies_of(&p);
        let via = Via { button: 0, strength: Some(START_HEALTH) };
        assert!(c.strike(1, 0, LASER, Some(p[1].pos), bodies, Arrival::Shot, via));
        assert_eq!((c.fighters[0].health, c.fighters[1].health), (0, START_HEALTH), "only the named victim is hurt");
        assert_eq!(c.round_over, Some((1, ROUND_END_TICKS)), "the attacker takes the round");
        assert_eq!(c.wins[..2], [0, 1]);
    }

    #[test]
    fn not_enough_aura_does_nothing() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].slots[0] = Some(WAVE); // costs 4, have 1
        press(&mut c, &mut p, &col, 0, 0);
        assert_eq!(c.fighters[0].slots[0], Some(WAVE));
        assert_eq!(c.fighters[0].action, Action::Ready);
    }

    #[test]
    fn aura_particle_raises_level_and_aura_refills() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].slots = [Some(AURA_PARTICLE); 4];
        for k in 0..4 {
            press(&mut c, &mut p, &col, 0, k);
            run(&mut c, &mut p, &col, [0; 2], 80);
        }
        assert_eq!(c.fighters[0].level, 5);
        // From 1 point up to 5 at the program's curve (about 1.15 points/s falling
        // to 0.85) takes a little over 4 s.
        run(&mut c, &mut p, &col, [0; 2], 10 * 60);
        assert_eq!(c.fighters[0].aura(), 5);
    }

    #[test]
    fn psycho_wave_dashes_and_hits_for_target_level() {
        // 60 u: within the melee dash's 80 u lock-on band. Psycho Wave's real
        // action row (FORMATS-SKILLS.md "golden values"): dash 10..20, fire
        // tick 20, hit window 20..22 (table ticks).
        let (mut c, mut p, col) = setup(60.0);
        c.set_action_row(WAVE, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 201, charge_cycle: 0, dash_start: 10, dash_end: 20, fire_tick: 20, hit_window_end: 22 }), None);
        c.fighters[0].level = 4;
        c.fighters[0].aura_ticks = 4 * AURA_TICKS_PER_POINT;
        c.fighters[1].level = 3;
        c.fighters[0].slots[1] = Some(WAVE);
        press(&mut c, &mut p, &col, 0, 1);
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert!(p[0].pos.z > 40.0, "dashed toward the target: {}", p[0].pos.z);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3, "STR = target's Level");
    }

    /// A flat hand path: the right hand held at `at` (character frame) through
    /// every frame of the clip, pointing along +Z.
    fn still_hand(at: Vec3) -> HandClip {
        HandClip { pos: vec![at; 60], fwd: vec![Vec3::Z; 60] }
    }

    /// Lane SHOTPOINTS (`FUN_14026c340` via `FUN_14018a310`): a punch is a sphere
    /// on the attached matrix -- the right-hand bone's when the effect has no
    /// `SHOT01` marker -- not on the caster's centre. With the hand held 30 u
    /// out to the side no body 11 u in front is in reach (13 + 5 u); with it
    /// 9 u ahead the same blow lands.
    #[test]
    fn a_punch_lands_where_the_hand_is_not_where_the_body_is() {
        for (hand, expect) in [(Vec3::new(30.0, 9.0, 0.0), START_HEALTH), (Vec3::new(0.0, 9.0, 9.0), START_HEALTH - 3)] {
            let (mut c, mut p, col) = setup(60.0);
            c.set_action_row(WAVE, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 201, charge_cycle: 0, dash_start: 10, dash_end: 20, fire_tick: 20, hit_window_end: 22 }), None);
            c.set_hand_paths([HashMap::from([("ATK_SPHERE".to_string(), still_hand(hand))])]);
            c.fighters[0].level = 4;
            c.fighters[0].aura_ticks = 4 * AURA_TICKS_PER_POINT;
            c.fighters[1].level = 3;
            c.fighters[0].slots[1] = Some(WAVE);
            press(&mut c, &mut p, &col, 0, 1);
            run(&mut c, &mut p, &col, [0; 2], 52);
            assert!(p[0].pos.z > 40.0, "dashed toward the target: {}", p[0].pos.z);
            assert_eq!(c.fighters[1].health, expect, "hand at {hand}");
        }
    }

    /// `FUN_14026c340` / `FUN_14026c730` test the line of fire (`FUN_1401641b0`) from the caster's centre
    /// (object `+0x1a4`) to the target's point after the reach test: a wall between the two stops a blow
    /// whose hand reaches past it (LIVE `242_Dhampir_Fang`, `317_True_Muramasa`). Same blow, same hand as
    /// the test above, which lands on open ground.
    #[test]
    fn a_wall_between_caster_and_target_stops_a_melee_blow() {
        let s = 1000.0;
        let (w, z) = (50.0, 56.0);
        let col = Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
            [Vec3::new(-w, 0.0, z), Vec3::new(w, 0.0, z), Vec3::new(w, 40.0, z)],
            [Vec3::new(-w, 0.0, z), Vec3::new(w, 40.0, z), Vec3::new(-w, 40.0, z)],
        ]);
        let (mut c, mut p, _) = setup(60.0);
        c.set_action_row(WAVE, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 201, charge_cycle: 0, dash_start: 10, dash_end: 20, fire_tick: 20, hit_window_end: 22 }), None);
        c.set_hand_paths([HashMap::from([("ATK_SPHERE".to_string(), still_hand(Vec3::new(0.0, 9.0, 9.0)))])]);
        c.fighters[0].level = 4;
        c.fighters[0].aura_ticks = 4 * AURA_TICKS_PER_POINT;
        c.fighters[1].level = 3;
        c.fighters[0].slots[1] = Some(WAVE);
        press(&mut c, &mut p, &col, 0, 1);
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert!(p[0].pos.z < z && p[0].pos.z + 9.0 > z, "stopped at the wall, hand past it: {}", p[0].pos.z);
        assert_eq!(c.fighters[1].health, START_HEALTH, "the wall is in the line of fire");
    }

    /// The hand is read at the clip frame of the tick (30 fps clip, 60 Hz
    /// ticks), in the caster's frame turned to its facing, and runs between
    /// samples.
    #[test]
    fn the_hand_is_sampled_at_the_clip_frame_and_turned_with_the_body() {
        let mut clip = still_hand(Vec3::ZERO);
        clip.pos[4] = Vec3::new(2.0, 10.0, 6.0);
        clip.pos[5] = Vec3::new(4.0, 10.0, 8.0);
        let (c, mut p, _) = setup(100.0);
        let mut c = c;
        c.set_hand_paths([HashMap::from([("ATK_SPHERE".to_string(), clip)])]);
        let (at, dir) = c.hand_world(0, "ATK_SPHERE", 4.5, &p).unwrap();
        assert!((at - Vec3::new(3.0, 10.0, 7.0)).length() < 1e-4, "{at}");
        assert!((dir - Vec3::Z).length() < 1e-4);
        // Turned a quarter round (yaw 90 deg faces +X): its right (+X in the body frame) is now -Z.
        p[0].yaw = std::f32::consts::FRAC_PI_2;
        p[0].pos = Vec3::new(10.0, 0.0, 10.0);
        let (at, dir) = c.hand_world(0, "ATK_SPHERE", 5.0, &p).unwrap();
        assert!((at - Vec3::new(10.0 + 8.0, 10.0, 10.0 - 4.0)).length() < 1e-4, "{at}");
        assert!((dir - Vec3::X).length() < 1e-4, "{dir}");
        assert!(c.hand_world(0, "ATK_NOPE", 0.0, &p).is_none());
    }

    /// `FUN_14018a310`: with no `SHOTnn` marker in the effect the object is
    /// made at the right-hand bone, at the cast clip's fire frame (the action
    /// row's fire tick, 30 fps frames).
    #[test]
    fn a_shot_with_no_marker_starts_at_the_hand_on_the_fire_frame() {
        let (mut c, mut p, _) = setup(300.0);
        c.set_action_row(BLASTER, Some(blaster_row()), None);
        let mut clip = still_hand(Vec3::ZERO);
        clip.pos[20] = Vec3::new(1.0, 13.0, 7.0);
        c.set_hand_paths([HashMap::from([("ATK_SHOT".to_string(), clip)])]);
        let marks = c.shot_marks(0, BLASTER, &p);
        assert_eq!(marks.len(), 1);
        assert!((marks[0] - Vec3::new(1.0, 13.0, 7.0)).length() < 1e-4, "{:?}", marks);
        // Facing +X it is carried round with the body.
        p[0].yaw = std::f32::consts::FRAC_PI_2;
        let marks = c.shot_marks(0, BLASTER, &p);
        assert!((marks[0] - Vec3::new(7.0, 13.0, -1.0)).length() < 1e-4, "{:?}", marks);
        // No hand data and no marker data: nothing (the chest default).
        let (c, p, _) = setup(300.0);
        assert!(c.shot_marks(0, BLASTER, &p).is_empty());
    }

    /// The effect's `SHOTnn` markers (eft0045 Double Laser: (13, 12, 0) and
    /// (-13, 12, 0), `pd_convert shotpoints`) win over the hand, one per shot,
    /// carried by the caster's matrix; a mid-air cast reads the air effect's.
    #[test]
    fn markers_are_the_spawn_points_in_marker_order_in_the_casters_frame() {
        let (mut c, mut p, _) = setup(200.0);
        c.set_action_row(TWIN_LASERS, Some(laser_row()), Some(laser_air_row()));
        c.set_shot_markers(HashMap::from([(TWIN_LASERS, (vec![Vec3::new(13.0, 12.0, 0.0), Vec3::new(-13.0, 12.0, 0.0)], vec![Vec3::new(0.0, 20.0, 5.0)]))]));
        c.set_hand_paths([HashMap::from([("ATK_LASER".to_string(), still_hand(Vec3::new(5.0, 5.0, 5.0)))])]);
        let m = c.shot_marks(0, TWIN_LASERS, &p);
        assert_eq!(m, vec![Vec3::new(13.0, 12.0, 0.0), Vec3::new(-13.0, 12.0, 0.0)]);
        // Yaw 90 deg: the body's right is world -Z.
        p[0].yaw = std::f32::consts::FRAC_PI_2;
        let m = c.shot_marks(0, TWIN_LASERS, &p);
        assert!((m[0] - Vec3::new(0.0, 12.0, -13.0)).length() < 1e-4 && (m[1] - Vec3::new(0.0, 12.0, 13.0)).length() < 1e-4, "{m:?}");
        // Mid-air: the air effect's marker.
        p[0].yaw = 0.0;
        c.airborne[0] = true;
        c.skills.iter_mut().find(|d| d.id == TWIN_LASERS).unwrap().raw[40] = 1;
        assert_eq!(c.shot_marks(0, TWIN_LASERS, &p), vec![Vec3::new(0.0, 20.0, 5.0)]);
    }

    /// Both lasers of a Double Laser leave from their own marker, 26 u apart,
    /// and the same cast without marker data leaves from the one chest point.
    #[test]
    fn a_double_laser_starts_its_two_shots_at_the_two_markers() {
        let start_gap = |markers: bool| {
            let (mut c, mut p, col) = setup(200.0);
            c.set_action_row(TWIN_LASERS, Some(laser_row()), None);
            if markers {
                c.set_shot_markers(HashMap::from([(TWIN_LASERS, (vec![Vec3::new(13.0, 12.0, 0.0), Vec3::new(-13.0, 12.0, 0.0)], Vec::new()))]));
            }
            cast_skill(&mut c, &mut p, &col, 0, TWIN_LASERS);
            for _ in 0..200 {
                c.step(&mut p, [0; 2], &col);
                if c.projectiles.len() == 2 {
                    break;
                }
            }
            assert_eq!(c.projectiles.len(), 2);
            (c.projectiles[0].prev_pos - c.projectiles[1].prev_pos).length()
        };
        let (with, without) = (start_gap(true), start_gap(false));
        assert!(with > without + 15.0, "markers {with} u apart, chest {without} u");
    }

    #[test]
    fn short_range_skills_dont_dash_beyond_80u_and_whiff() {
        // PROGRAM-RE.md "Lock-on": the melee dash only gets a target within
        // 80 u (distance band 1); further than that, no lunge at all.
        let (mut c, mut p, col) = setup(500.0);
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (4, 4 * AURA_TICKS_PER_POINT);
        c.fighters[0].slots[1] = Some(WAVE);
        press(&mut c, &mut p, &col, 0, 1);
        assert_eq!(c.fighters[0].action, Action::Lunge);
        run(&mut c, &mut p, &col, [0; 2], LUNGE_MAX_TICKS as usize + 2);
        assert!(p[0].pos.z < 5.0, "too far for the 80 u lock-on band: stayed put: {}", p[0].pos.z);
        assert_eq!(c.fighters[1].health, START_HEALTH, "too far: the swing whiffs");
    }

    #[test]
    fn short_range_skill_whiffs_without_lock_on_even_when_close() {
        // Round-2 footage: "no lunge without lock-on" -- a clean whiff in
        // place even though the target is well within range.
        let (mut c, mut p, col) = setup(60.0);
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (4, 4 * AURA_TICKS_PER_POINT);
        c.fighters[0].slots[1] = Some(WAVE);
        p[0].locked = false;
        press(&mut c, &mut p, &col, 0, 1);
        run(&mut c, &mut p, &col, [0; 2], LUNGE_MAX_TICKS as usize + 2);
        assert!(p[0].pos.z < 5.0, "not locked on: stayed put: {}", p[0].pos.z);
        assert_eq!(c.fighters[1].health, START_HEALTH, "not locked on: whiffs");
    }

    /// Laser's real ground/mid-air action rows (FORMATS-SKILLS.md "golden
    /// values" / PROGRAM-RE.md pilot): row 42, fire tick 13; mid-air row 305,
    /// `ATK_SKYSHOT_LOW`, fire tick 20.
    fn laser_row() -> ActionRow {
        ActionRow { anim_name: "ATK_LASER".into(), cast_sound: 242, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 13, hit_window_end: 0 }
    }
    fn laser_air_row() -> ActionRow {
        ActionRow { anim_name: "ATK_SKYSHOT_LOW".into(), cast_sound: 528, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 20, hit_window_end: 0 }
    }
    /// Blaster's real row (id 80, row 53): fire tick 20, `ATK_SHOT`.
    fn blaster_row() -> ActionRow {
        ActionRow { anim_name: "ATK_SHOT".into(), cast_sound: 253, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 20, hit_window_end: 0 }
    }
    /// Psycho Knife's real row (id 3, row 3): dash 12..22, fire tick 18, hit
    /// window 18..22 (table ticks) -- sim ticks 24..44 / 36..44.
    fn psycho_knife_row() -> ActionRow {
        ActionRow { anim_name: "ATK_SORD".into(), cast_sound: 203, charge_cycle: 0, dash_start: 12, dash_end: 22, fire_tick: 18, hit_window_end: 22 }
    }

    #[test]
    fn laser_fires_at_sim_tick_26() {
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(LASER, Some(laser_row()), Some(laser_air_row()));
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        // The press's own release step already counts as elapsed tick 1.
        run(&mut c, &mut p, &col, [0; 2], 24);
        assert!(c.projectiles.is_empty(), "not fired yet at tick 25");
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert!(!c.projectiles.is_empty(), "fires at sim tick 26 (13 table ticks)");
    }

    #[test]
    fn a_cast_clips_root_track_moves_the_caster_from_the_press() {
        // hittiming lane: every action row passes root flag 1 to the cast clip's start
        // (`FUN_14025a3e0`, as for the reaction clips: `0x140257510` adds the root track's x / z
        // change each tick, rotated by the facing). LIVE (takes_full `080_Blaster`): ATK_SHOT
        // pushes the caster 8 u back after the fire tick; `004_Psycho_Blade`: ATK_WHAND_SORD's
        // track is -0.26 u at press + 1 and +26 u over the clip. Here a curve of 3 frames: -2 u
        // then +6 u forward, half a frame a sim tick, caster at yaw 0 (forward = +Z).
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(BLASTER, Some(blaster_row()), None);
        let curve: Vec<[f32; 2]> = vec![[0.0, 0.0], [0.0, -2.0], [0.0, 4.0]];
        let t: HashMap<String, Vec<[f32; 2]>> = [("ATK_SHOT".to_string(), curve)].into_iter().collect();
        c.set_root_curves([t, HashMap::new()]);
        let z0 = p[0].pos.z;
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert!(p[0].pos.z < z0, "the clip's first frames step back: {} -> {}", z0, p[0].pos.z);
        run(&mut c, &mut p, &col, [0; 2], 10);
        assert!((p[0].pos.z - (z0 + 4.0)).abs() < 1e-3, "the track ends 4 u forward of the start and stops there: {}", p[0].pos.z - z0);
        // The opponent has no table: its own casts do not move it.
        let z1 = p[1].pos.z;
        c.fighters[1].slots[0] = Some(BLASTER);
        cast_skill(&mut c, &mut p, &col, 1, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], 12);
        assert_eq!(p[1].pos.z, z1);
    }

    #[test]
    fn blaster_fires_at_sim_tick_40() {
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(BLASTER, Some(blaster_row()), None);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], 38);
        assert!(c.projectiles.is_empty(), "not fired yet at tick 39");
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert!(!c.projectiles.is_empty(), "fires at sim tick 40 (20 table ticks)");
    }

    #[test]
    fn psycho_knife_dashes_24_to_44_and_hits_36_to_44_when_locked_on_within_80u() {
        use super::testkit as k;
        const PSYCHO_KNIFE: i16 = 3;
        let mut sk = skills();
        sk.push(k::def(PSYCHO_KNIFE, Capsule::Attack, 2, &[(f::FAMILY, 10), (f::STRENGTH, 2), (f::MOTION, 12), (P, 130), (P + 1, 8), (P + 2, 10)]));
        let (mut c, mut p, col) = k::setup(sk, 60.0);
        c.set_action_row(PSYCHO_KNIFE, Some(psycho_knife_row()), None);
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (4, 4 * AURA_TICKS_PER_POINT);
        c.fighters[0].slots[0] = Some(PSYCHO_KNIFE);
        press(&mut c, &mut p, &col, 0, 0);
        // The wind-up lasts 12 table ticks = 24 sim ticks (`FUN_14025be40` counts it
        // down first); the dash steps are the 20 sim ticks after it (a program tick
        // chooses its step on the first sim tick of the pair and walks half on each).
        run(&mut c, &mut p, &col, [0; 2], 22); // (the press tick is elapsed 1)
        let before_dash = p[0].pos.z;
        run(&mut c, &mut p, &col, [0; 2], 1); // elapsed 24: still winding up
        assert_eq!(p[0].pos.z, before_dash, "the dash starts after sim tick 24");
        run(&mut c, &mut p, &col, [0; 2], 1); // elapsed 25: first step
        assert!(p[0].pos.z > before_dash, "dashing from sim tick 25");
        assert_eq!(c.fighters[1].health, START_HEALTH, "not hit yet: the hit window opens at tick 36");
        run(&mut c, &mut p, &col, [0; 2], 20); // elapsed 44
        assert!(c.fighters[1].health < START_HEALTH, "hits by the window's end (sim tick 44)");
    }

    #[test]
    fn psycho_knife_no_dash_or_hit_without_lock_on() {
        use super::testkit as k;
        const PSYCHO_KNIFE: i16 = 3;
        let mut sk = skills();
        sk.push(k::def(PSYCHO_KNIFE, Capsule::Attack, 2, &[(f::FAMILY, 10), (f::STRENGTH, 2), (f::MOTION, 12), (P, 130), (P + 1, 8), (P + 2, 10)]));
        let (mut c, mut p, col) = k::setup(sk, 60.0);
        c.set_action_row(PSYCHO_KNIFE, Some(psycho_knife_row()), None);
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (4, 4 * AURA_TICKS_PER_POINT);
        c.fighters[0].slots[0] = Some(PSYCHO_KNIFE);
        p[0].locked = false;
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], 46);
        assert!(p[0].pos.z < 5.0, "not locked on: no dash: {}", p[0].pos.z);
        assert_eq!(c.fighters[1].health, START_HEALTH, "not locked on: whiffs");
    }

    #[test]
    fn a_hit_before_the_fire_tick_cancels_the_cast() {
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(BLASTER, Some(blaster_row()), None);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        // Well before the fire tick (40): a hit lands and cancels the cast.
        run(&mut c, &mut p, &col, [0; 2], 5);
        c.hurt(0, 1, false, Vec3::Z);
        assert_eq!(c.fighters[0].action, Action::Hit, "the hit interrupts the cast");
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert!(c.projectiles.is_empty(), "cancelled before it fired: no shot ever leaves");
    }

    #[test]
    fn caster_cant_act_until_the_clip_ends() {
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(LASER, Some(laser_row()), Some(laser_air_row()));
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        // ATK_LASER is 34 table ticks (68 sim ticks) long -- well past the
        // fire tick (26): the caster is still held long after firing.
        run(&mut c, &mut p, &col, [0; 2], 50);
        assert_ne!(c.fighters[0].action, Action::Ready, "still held well before the clip ends");
        assert!(!c.projectiles.is_empty(), "already fired (tick 26), but still can't act");
        // A press while held does nothing (`press` itself costs 2 ticks: the
        // held-then-released button, both while the cast still owns the slot).
        press(&mut c, &mut p, &col, 0, 0);
        assert_ne!(c.fighters[0].action, Action::Ready, "the press while held did nothing");
        run(&mut c, &mut p, &col, [0; 2], 20);
        assert_eq!(c.fighters[0].action, Action::Ready, "free once the clip ends");
    }

    #[test]
    fn pressing_without_enough_aura_flashes_and_says_no() {
        let (mut c, mut p, col) = setup(100.0);
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (1, AURA_TICKS_PER_POINT);
        c.fighters[0].slots[2] = Some(WAVE); // costs 4
        press(&mut c, &mut p, &col, 0, 2);
        assert_eq!(c.fighters[0].slots[2], Some(WAVE), "not cast");
        assert!(c.fighters[0].denied_ticks[2] > 0);
        assert!(c.events.iter().any(|(_, e)| *e == CombatEvent::Denied { who: 0, slot: 2 }));
    }

    #[test]
    fn barrier_blocks() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].slots[0] = Some(LASER);
        c.fighters[1].slots[0] = Some(WALL);
        press(&mut c, &mut p, &col, 1, 0);
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert_eq!(c.events.last().map(|e| e.1), Some(CombatEvent::Blocked { attacker: 0, victim: 1, skill: LASER, broken: false }));
    }

    #[test]
    fn knock_out_scores_and_restarts_round() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[1].health = 1;
        c.fighters[0].slots[0] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].action, Action::Down);
        assert_eq!(c.wins[..2], [1, 0]);
        let mut restarted = false;
        for _ in 0..ROUND_END_TICKS {
            restarted |= c.step(&mut p, [0; 2], &col);
        }
        assert!(restarted);
        assert_eq!((c.round, c.fighters[1].health, c.fighters[1].action), (2, START_HEALTH, Action::Ready));
    }

    #[test]
    fn capsules_spawn_and_are_captured_with_a_button() {
        let (mut c, mut p, col) = setup(100.0);
        run(&mut c, &mut p, &col, [0; 2], CAPSULE_EVERY_TICKS as usize);
        assert_eq!(c.capsules.len(), 2, "one each");
        let cap = c.capsules.iter().find(|k| k.owner == 0).unwrap().clone();
        p[0].pos = cap.pos;
        c.step(&mut p, [0; 2], &col);
        assert_eq!(c.fighters[0].slots, [None; SLOTS], "walking over it doesn't capture");
        c.fighters[0].slots[2] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 2);
        assert_eq!(c.fighters[0].slots[2], Some(AURA_PARTICLE), "captured into the pressed slot, replacing it");
        assert!(c.capsules.iter().all(|k| k.id != cap.id));
        assert_eq!(c.fighters[0].level, TEST_RULES.start_level, "capturing is not casting");
    }

    /// Presses `slot` with a full Aura of 8 and lets the cast finish; returns the Aura it cost.
    fn use_from_full(c: &mut Combat, p: &mut [Player; 2], col: &Collision, slot: usize) -> i32 {
        c.fighters[0].arsenal.clear(); // no capsules turning up to capture instead
        (c.fighters[0].level, c.fighters[0].aura_ticks) = (8, 8 * AURA_TICKS_PER_POINT);
        press(c, p, col, 0, slot);
        run(c, p, col, [0; 2], CAST_TICKS as usize + 2);
        // Paid at the fire tick (`0x1402621f0` from `0x1402626d0`), not at the press.
        8 - c.fighters[0].aura()
    }

    #[test]
    fn single_use_skills_are_used_up_and_consumed() {
        let (mut c, mut p, col) = setup(300.0);
        // Ki Lance: s26 = 1.
        c.skills.iter_mut().find(|d| d.id == LANCE).unwrap().raw[f::USES] = 1;
        c.fighters[0].slots[0] = Some(LANCE);
        use_from_full(&mut c, &mut p, &col, 0);
        assert_eq!(c.fighters[0].slots[0], None);
        assert_eq!(c.fighters[0].used, vec![LANCE], "consumed (Recall, Relearn, Scar of Battles count it)");
    }

    #[test]
    fn knife_grind_gets_cheaper_per_use_and_runs_out_after_three() {
        const KNIFE_GRIND: i16 = 322;
        let (mut c, mut p, col) = setup(300.0);
        // Cost 2, "Aura cost -1 per use" (s24 = -1), 3 uses (s26).
        c.skills.push(def(KNIFE_GRIND, Capsule::Attack, 12, 2, 2, 0, &[(P, -30), (P + 1, 140), (f::COST_PER_USE, -1), (f::USES, 3)]));
        c.fighters[0].slots[1] = Some(KNIFE_GRIND);
        let costs: Vec<i32> = (0..3).map(|_| use_from_full(&mut c, &mut p, &col, 1)).collect();
        assert_eq!(costs, vec![2, 1, 0]);
        assert_eq!(c.fighters[0].slots[1], None, "3 uses, then gone");
    }

    #[test]
    fn aura_cost_rises_per_use_until_a_fresh_capture() {
        const ICE_SWORD: i16 = 121;
        let (mut c, mut p, col) = setup(300.0);
        // Cost 2, "Penalty: Aura cost +1 per use" (s24 = 1), unlimited (s26 = 0).
        c.skills.push(def(ICE_SWORD, Capsule::Attack, 12, 2, 2, 0, &[(P, -30), (P + 1, 140), (f::COST_PER_USE, 1)]));
        c.fighters[0].slots[2] = Some(ICE_SWORD);
        let costs: Vec<i32> = (0..3).map(|_| use_from_full(&mut c, &mut p, &col, 2)).collect();
        assert_eq!(costs, vec![2, 3, 4]);
        assert_eq!(c.fighters[0].slots[2], Some(ICE_SWORD), "unlimited: still there");
        // Capturing it again onto the button starts over.
        c.capsules.push(CapsuleItem { id: 99, owner: 0, skill: ICE_SWORD, pos: p[0].pos, appear: 0 });
        press(&mut c, &mut p, &col, 0, 2);
        assert_eq!(c.fighters[0].spent[2], 0);
        assert_eq!(use_from_full(&mut c, &mut p, &col, 2), 2);
    }

    #[test]
    fn the_original_rules_start_at_level_0_with_four_skills_dealt() {
        let c = Combat::new(skills(), (1..=30).collect());
        for f in &c.fighters[..2] {
            assert_eq!((f.level, f.aura()), (0, 0));
            assert!(f.slots.iter().all(Option::is_some), "4 skills on the buttons");
            assert_eq!(f.arsenal.len(), 26);
        }
    }

    #[test]
    fn capsule_spots_start_full_and_refill_where_emptied() {
        let (mut c, mut p, col) = setup(300.0);
        let spots = vec![Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 150.0)];
        c.set_capsule_spots([spots.clone(), vec![]]);
        c.fighters[0].arsenal = vec![LASER; 10];
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert_eq!(c.capsules.iter().filter(|k| k.owner == 0).count(), 3, "all three spots filled at once");
        c.capsules.retain(|k| k.pos != spots[1]); // captured
        // 420 program ticks hidden = 840 sim ticks (`FUN_14014fd10`: timer 0x1a4 = 420).
        run(&mut c, &mut p, &col, [0; 2], 2 * 420 - 4);
        assert_eq!(c.capsules.iter().filter(|k| k.owner == 0).count(), 2, "still hidden");
        run(&mut c, &mut p, &col, [0; 2], 8);
        let back = c.capsules.iter().find(|k| k.owner == 0 && k.pos == spots[1]).expect("back at the same spot");
        assert!(back.appear > 0, "appearing: there, but not yet pickable (`FUN_14014ed10` state 1, 15 program ticks)");
        // 420 + 15 program ticks = 870 sim ticks in all (14.5 s).
        assert_eq!(CAPSULE_RESPAWN_TICKS, 2 * (420 + 15));
        run(&mut c, &mut p, &col, [0; 2], CAPSULE_APPEAR_TICKS as usize);
        assert_eq!(c.capsules.iter().find(|k| k.pos == spots[1]).unwrap().appear, 0, "pickable");
    }

    /// The read-only view of spots waiting to regenerate (program state 0,
    /// `FUN_14014ed10`): none for the opening deal, listed from the tick after the
    /// capsule left with a rising elapsed count over the 840-tick wait, gone once
    /// the capsule is back (appearing). No rule changes: it only reads.
    #[test]
    fn pending_capsules_follow_the_respawn_wait() {
        let (mut c, mut p, col) = setup(300.0);
        let spots = vec![Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 150.0)];
        c.set_capsule_spots([spots.clone(), vec![]]);
        c.fighters[0].arsenal = vec![LASER; 10];
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert!(c.pending_capsules().is_empty(), "the opening deal is present, not regenerating");
        c.capsules.retain(|k| k.pos != spots[1]); // captured
        run(&mut c, &mut p, &col, [0; 2], 10);
        let pend = c.pending_capsules();
        assert_eq!(pend.len(), 1);
        assert_eq!((pend[0].owner, pend[0].spot, pend[0].pos, pend[0].wait), (0, 1, spots[1], 2 * 420));
        assert!((9..=11).contains(&pend[0].elapsed), "{}", pend[0].elapsed);
        run(&mut c, &mut p, &col, [0; 2], 2 * 420 - 10 - 4);
        assert_eq!(c.pending_capsules().len(), 1, "still waiting just before the end");
        assert!(c.pending_capsules()[0].elapsed >= 2 * 420 - 6);
        run(&mut c, &mut p, &col, [0; 2], 8);
        assert!(c.pending_capsules().is_empty(), "back (appearing): the flare takes over");
        // No capsule left to come: nothing is regenerating.
        c.fighters[0].arsenal.clear();
        c.capsules.retain(|k| k.pos != spots[1]);
        run(&mut c, &mut p, &col, [0; 2], 10);
        assert!(c.pending_capsules().is_empty());
    }

    /// `FUN_14014f630`: a capsule is captured from <= 15 u away in 3-D (from its
    /// ground point), not from a flat 12.5 u with a height gate; and not while it
    /// is still appearing (`FUN_14014ed10`).
    #[test]
    fn pickup_reach_is_fifteen_units_in_three_dimensions() {
        let (mut c, p, _col) = setup(300.0);
        let at = Vec3::new(0.0, 0.0, 100.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: LASER, pos: at, appear: 0 });
        let mut me = p[0].clone();
        for (off, yoff, reach) in [(14.9, 0.0, true), (15.1, 0.0, false), (13.0, 6.0, true), (13.0, 9.0, false), (0.0, 14.9, true)] {
            me.pos = at + Vec3::new(off, yoff, 0.0);
            assert_eq!(c.capsule_in_reach(0, &me).is_some(), reach, "{off} u across, {yoff} u up");
        }
        me.pos = at;
        c.capsules[0].appear = 5;
        assert_eq!(c.capsule_in_reach(0, &me), None, "appearing");
        c.capsules[0].appear = 0;
        assert_eq!(c.capsule_in_reach(0, &me), Some(0));
        assert_eq!(c.capsule_in_reach(1, &me), None, "someone else's");
    }

    #[test]
    fn countdown_blocks_casting() {
        let (mut c, mut p, col) = setup(100.0);
        c.round_ticks = 0;
        c.fighters[0].slots[0] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 0);
        assert_eq!(c.fighters[0].slots[0], Some(LASER));
        run(&mut c, &mut p, &col, [0; 2], COUNTDOWN_TICKS as usize);
        assert!(!c.counting_down());
        let seq = c.fighters[0].cast_seq;
        press(&mut c, &mut p, &col, 0, 0);
        assert_ne!(c.fighters[0].cast_seq, seq, "cast");
    }

    /// Player `who` gets level/aura 8 and `skill` in slot 0, and presses it.
    fn cast_skill(c: &mut Combat, p: &mut [Player; 2], col: &Collision, who: usize, skill: i16) {
        c.fighters[who].level = 8;
        c.fighters[who].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[who].slots[0] = Some(skill);
        press(c, p, col, who, 0);
    }

    /// The program's object wall sweep ignores hull classes 0x10 / 0x12 / 0x14 / 0x1F (`FUN_140163e80`): a
    /// shot flies through such a face and hits the fighter behind it; a face of any other class stops it.
    /// The line-of-fire ray (`FUN_1401641b0`) is drawn from the shot's own position, so once it is past the
    /// face nothing is in the way.
    #[test]
    fn a_shot_flies_through_rail_classes_and_a_solid_class_stops_it() {
        let s = 1000.0;
        let tris = vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
            [Vec3::new(-s, 0.0, 50.0), Vec3::new(s, 0.0, 50.0), Vec3::new(s, 60.0, 50.0)],
            [Vec3::new(-s, 0.0, 50.0), Vec3::new(s, 60.0, 50.0), Vec3::new(-s, 60.0, 50.0)],
        ];
        for (class, passes) in [(0x02u8, false), (0x11, false), (0x12, true), (0x14, true), (0x10, true), (0x1F, true), (pd_formats::collision::CLASS_NONE, false)] {
            let (mut c, mut p, _) = setup(100.0);
            let col = Collision::new_classed(tris.clone(), &[NO_OBJECT; 4], false, &[0, 0, class, class]);
            cast_skill(&mut c, &mut p, &col, 0, BLASTER);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 80);
            assert_eq!(c.fighters[1].health < START_HEALTH, passes, "class {class:#x}: the shot {} the face", if passes { "passes" } else { "stops at" });
        }
    }

    #[test]
    fn shell_flies_at_its_table_speed_and_hits() {
        let (mut c, mut p, col) = setup(300.0);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        let shot = c.projectiles.first().expect("leaves at the end of the cast");
        assert_eq!(shot.shot.pos, shot.shot.prev, "its first program tick only arms it (`FUN_1402638d0`): nothing moved yet");
        run(&mut c, &mut p, &col, [0; 2], 4);
        let shot = c.projectiles.first().expect("still flying");
        assert!(((shot.shot.pos - shot.shot.prev).length() - 140.0 * moves::SPEED_UNIT).abs() < 0.01, "140 / 10.8 = 12.96 u a program tick");
        run(&mut c, &mut p, &col, [0; 2], 56);
        assert_eq!(c.fighters[1].health, START_HEALTH - 4);
    }

    /// The renderer's shot position, `pos + lead()`, is the program's position:
    /// the sim keeps `pos` mid-step on the tick a shot moves (and at the
    /// program position on the tick between), so `pos` itself trails by half a
    /// step on every other tick. Drawn at `pos + lead`, a straight shell sits
    /// exactly on its program position on the tick it steps and half a step
    /// beyond on the next, advancing the same half step every sim tick.
    #[test]
    fn a_drawn_shot_is_on_its_program_position_not_half_a_step_behind() {
        let (mut c, mut p, col) = setup(600.0);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        assert!(!c.projectiles.is_empty());
        let step = 140.0 * moves::SPEED_UNIT;
        let mut last_drawn: Option<Vec3> = None;
        let (mut stepped_ticks, mut mid_ticks) = (0, 0);
        for _ in 0..12 {
            let before = c.projectiles[0].shot.pos;
            c.step(&mut p, [0; 2], &col);
            let s = &c.projectiles[0];
            let drawn = s.pos + s.lead();
            if (s.shot.pos - before).length() > 1e-3 {
                stepped_ticks += 1;
                assert!((drawn - s.shot.pos).length() < 1e-3, "on the tick it steps the drawn shot is on its program position");
                assert!((s.pos - s.shot.pos).length() > step * 0.4, "while `pos` itself is half a step behind");
            } else {
                mid_ticks += 1;
                assert!(((drawn - s.shot.pos).length() - step * 0.5).abs() < 1e-3, "between steps it is half a step on");
            }
            if let Some(l) = last_drawn {
                assert!(((drawn - l).length() - step * 0.5).abs() < 1e-3, "the drawn shot advances half a step every sim tick");
            }
            last_drawn = Some(drawn);
        }
        assert!(stepped_ticks >= 5 && mid_ticks >= 5, "{stepped_ticks} {mid_ticks}");
    }

    #[test]
    fn shell_homes_on_a_target_that_moved() {
        let (mut c, mut p, col) = setup(300.0);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        p[1].pos.x = 30.0; // side-step as it leaves
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH - 4, "homing 15 turns it onto the target");
    }

    #[test]
    fn parabola_lobs_over_and_explodes_on_the_target() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, BOMB);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        let mut top = 0.0f32;
        for _ in 0..120 {
            if let Some(s) = c.projectiles.first() {
                top = top.max(s.pos.y);
            }
            c.step(&mut p, [0; 2], &col);
        }
        assert!(top > CHEST + 15.0, "arcs well above a straight line: {top}");
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
        assert!(c.events.iter().any(|e| matches!(e.1, CombatEvent::Burst { skill: BOMB, .. })));
    }

    #[test]
    fn bomb_blast_catches_a_near_miss() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, BOMB);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        p[1].pos.x = 22.0; // inside the 30-unit blast, but the bomb misses the body
        run(&mut c, &mut p, &col, [0; 2], 120);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
    }

    #[test]
    fn barrier_only_covers_the_front() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[1].slots[0] = Some(WALL);
        // A defence goes up where the camera looks (`0x140258180`): camera and body
        // both turned away from the shooter.
        p[1].yaw += std::f32::consts::PI;
        p[1].cam_yaw += std::f32::consts::PI;
        press(&mut c, &mut p, &col, 1, 0);
        c.fighters[0].slots[0] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 1, "hit in the back");
    }

    #[test]
    fn shelter_covers_all_round() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 1, SHELL);
        p[1].yaw += std::f32::consts::PI;
        c.fighters[0].slots[0] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH);
    }

    #[test]
    fn stronger_attack_destroys_the_barrier_without_damage() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[1].slots[0] = Some(WALL); // DEF 3
        press(&mut c, &mut p, &col, 1, 0);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER); // STR 4
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH, "STR > DEF: attack has no effect");
        assert!(c.fighters[1].guard.is_none(), "defence skill is destroyed");
        assert_eq!(c.events.last().map(|e| e.1), Some(CombatEvent::Blocked { attacker: 0, victim: 1, skill: BLASTER, broken: true }));
    }

    #[test]
    fn pierce_gets_the_difference_through_unless_anti_pierce() {
        for (guard, expect) in [(FIRE_WALL, START_HEALTH - 2), (WALL, START_HEALTH)] {
            let (mut c, mut p, col) = setup(100.0);
            // The lance is pressed first: a shot's first program tick only arms it (`FUN_1402638d0`), which
            // makes it arrive at the very end of a guard pressed on the same tick.
            cast_skill(&mut c, &mut p, &col, 0, LANCE); // STR 5, Pierce
            run(&mut c, &mut p, &col, [0; 2], 4);
            cast_skill(&mut c, &mut p, &col, 1, guard); // DEF 3
            run(&mut c, &mut p, &col, [0; 2], 60);
            assert_eq!(c.fighters[1].health, expect, "guard {guard}: {:?}", c.events);
        }
    }

    #[test]
    fn barrier_lasts_its_table_time() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[1].slots[0] = Some(WALL);
        press(&mut c, &mut p, &col, 1, 0);
        // s46 = 23 table ticks: the guard's countdown starts at s46 - 2 and it
        // stands for s46 - 1 = 22 program ticks (`0x140264020`, `0x140264340`).
        assert_eq!(c.fighters[1].guard.map(|g| g.guard.ticks), Some(44), "22 table ticks of 1/30 s");
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert!(c.fighters[1].guard.is_none());
    }

    #[test]
    fn retain_skills_stay_on_their_button_while_aura_covers_them() {
        // "Add: Retain if you have enough Aura to use again" (s41 = 1) on a
        // single-use skill, as Arrow of Artemis and Reincarnation (Photon
        // Barrier has the note too, but is unlimited-use anyway).
        const PHOTON_BARRIER: i16 = 90;
        let skills = vec![def(PHOTON_BARRIER, Capsule::Defence, 80, 2, 28, 0, &[(f::DEFENCE_KIND, 0), (f::DEFENCE, 1), (f::PROTECTED, 1), (f::USES, 1)])];
        let (mut c, mut p, col) = setup(100.0);
        c.skills.extend(skills);
        let f = &mut c.fighters[1];
        (f.level, f.aura_ticks, f.slots[0]) = (8, 4 * AURA_TICKS_PER_POINT, Some(PHOTON_BARRIER));
        press(&mut c, &mut p, &col, 1, 0);
        assert!(c.fighters[1].guard.is_some());
        assert_eq!(c.fighters[1].slots[0], Some(PHOTON_BARRIER), "2 Aura left: still enough to use again");
        c.fighters[1].guard = None;
        (c.fighters[1].action, c.fighters[1].aura_ticks) = (Action::Ready, 3 * AURA_TICKS_PER_POINT);
        press(&mut c, &mut p, &col, 1, 0);
        assert!(c.fighters[1].guard.is_some());
        assert_eq!(c.fighters[1].slots[0], None, "1 Aura left: not enough, so it's used up");
    }

    /// Casts `skill` for player 0 and runs until the cast animation has ended.
    fn cast_and_release(c: &mut Combat, p: &mut [Player; 2], col: &Collision, skill: i16) {
        cast_skill(c, p, col, 0, skill);
        run(c, p, col, [0; 2], CAST_TICKS as usize);
    }

    #[test]
    fn hole_warns_then_hits_where_the_target_stood() {
        let (mut c, mut p, col) = setup(100.0);
        cast_and_release(&mut c, &mut p, &col, TREMOR);
        let a = c.areas.iter().find(|a| a.target == 1).expect("set at the target's feet");
        assert!(a.warning() && a.pos.distance(p[1].pos) < 1.0);
        run(&mut c, &mut p, &col, [0; 2], 30);
        assert_eq!(c.fighters[1].health, START_HEALTH, "still warning (1 s)");
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
    }

    #[test]
    fn hole_is_dodged_by_moving_off_the_mark() {
        let (mut c, mut p, col) = setup(100.0);
        cast_and_release(&mut c, &mut p, &col, TREMOR);
        p[1].pos.x += 50.0;
        run(&mut c, &mut p, &col, [0; 2], 90);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert!(c.areas.is_empty());
    }

    /// Plays one Rain of Stones cast against player 1 (standing 100 u away, still) with a
    /// seed for the drops; returns the Health they lost.
    fn rain_damage(seed: u32, guard: Option<i16>, roof: bool) -> i32 {
        rain_run(seed, guard, roof).0
    }

    /// `rain_damage`, and how many drops a guard stopped.
    fn rain_run(seed: u32, guard: Option<i16>, roof: bool) -> (i32, usize) {
        let s = 1000.0;
        let mut tris = vec![[Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)], [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)]];
        if roof {
            let (lo, hi) = (Vec3::new(-60.0, 40.0, 40.0), Vec3::new(60.0, 40.0, 160.0));
            tris.push([lo, Vec3::new(hi.x, 40.0, lo.z), hi]);
            tris.push([lo, hi, Vec3::new(lo.x, 40.0, hi.z)]);
        }
        let col = Collision::new(tris);
        let (mut c, mut p, _) = setup(100.0);
        c.rng = seed | 1;
        if let Some(g) = guard {
            // Up the whole time (the test is about what stops a drop, not how long a guard lasts).
            let def = c.skill(g).cloned().unwrap();
            let guard = barrier::Guard::from_def(&def).unwrap();
            c.fighters[1].guard = Some(ActiveGuard { ticks: 10_000, ..ActiveGuard::new(g, guard, 0) });
        }
        cast_and_release(&mut c, &mut p, &col, RAIN);
        run(&mut c, &mut p, &col, [0; 2], 260);
        (START_HEALTH - c.fighters[1].health, c.events.iter().filter(|(_, e)| matches!(e, CombatEvent::Blocked { .. })).count())
    }

    /// Barrier and Reflect never stop Rain (motions 7/20, `FUN_140271760`); a Shelter stops the
    /// first drop that reaches it, but a block ends the guard 5 program ticks later (the drops come
    /// 6 ticks apart), so a later drop of the same cast can still land. (Drops are random: seeds
    /// that land at least once in the open are used.)
    #[test]
    fn rain_hits_once_a_barrier_ignores_it_and_a_shelter_holds_only_through_its_linger() {
        let seeds = 0x1357_9bdfu32;
        let lands: Vec<u32> = (0..40u32).map(|k| seeds.wrapping_mul(k + 1)).filter(|&sd| rain_damage(sd, None, false) > 0).collect();
        assert!(lands.len() > 3);
        let mut stopped_a_drop = 0;
        for sd in lands {
            let (barrier_dmg, barrier_blocks) = rain_run(sd, Some(WALL), false);
            assert_eq!((barrier_dmg, barrier_blocks), (3, 0), "the Barrier is never in the Rain's way; once per cast");
            let (shelter_dmg, shelter_blocks) = rain_run(sd, Some(SHELL), false);
            assert!(shelter_dmg <= 3 && shelter_blocks >= 1, "the Shelter stops the first drop that reaches it");
            if shelter_dmg == 0 {
                stopped_a_drop += 1;
            }
        }
        assert!(stopped_a_drop > 0, "sometimes the first block is all that was coming");
    }

    /// Rain (motion 20): six drops from a cloud 100 u up, each from a random point within 30 u of
    /// it and leaning up to 10 deg; a drop lands only if it falls within the 5 u + 5 u of the body, so
    /// a still target is hit by some casts and missed by others, and the drops of one cast share a hit
    /// mask: **once** per cast (3 Health, STR 3). Only a Shelter (all round) stops it; a Barrier does not.
    #[test]
    fn rain_lands_at_random_once_per_cast_and_only_a_shelter_stops_it() {
        let seeds = 0x1357_9bdfu32;
        let (mut hit, mut missed) = (0, 0);
        for k in 0..40u32 {
            let d = rain_damage(seeds.wrapping_mul(k + 1), None, false);
            assert!(d == 0 || d == 3, "once per cast: {d}");
            if d == 0 { missed += 1 } else { hit += 1 }
        }
        assert!(hit > 3 && missed > 3, "random drops: {hit} casts hit, {missed} missed");
        for k in 0..40u32 {
            let sd = seeds.wrapping_mul(k + 1);
            assert!(rain_damage(sd, Some(SHELL), false) <= rain_damage(sd, None, false), "a Shelter never makes it worse");
        }
        let through_barrier = (0..40u32).filter(|&k| rain_damage(seeds.wrapping_mul(k + 1), Some(WALL), false) > 0).count();
        assert!(through_barrier > 3, "a Barrier does not stop rain: {through_barrier}");
    }

    /// "If objects are above the target, these skills typically won't affect the target": a roof
    /// over the target stops every drop (the same casts that hit in the open).
    #[test]
    fn rain_misses_under_a_roof() {
        let seeds = 0x1357_9bdfu32;
        let open: Vec<u32> = (0..40u32).filter(|&k| rain_damage(seeds.wrapping_mul(k + 1), None, false) > 0).collect();
        assert!(open.len() > 3);
        for k in open {
            assert_eq!(rain_damage(seeds.wrapping_mul(k + 1), None, true), 0, "seed {k}: a roof overhead");
        }
    }

    #[test]
    fn timer_mine_goes_off_after_five_seconds() {
        let (mut c, mut p, col) = setup(40.0);
        cast_and_release(&mut c, &mut p, &col, TIMER_MINE);
        run(&mut c, &mut p, &col, [0; 2], 280);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        // Off at 150 program ticks (`FUN_140267e90`), then the 50 u blast grows to the
        // target 40 u away (centre to centre, + the body) over 22 more program ticks.
        run(&mut c, &mut p, &col, [0; 2], 90);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3, "\"Explodes automatically 5 sec later\", 50 u blast");
    }

    #[test]
    fn sensor_mine_waits_for_the_target_to_come_close() {
        let (mut c, mut p, col) = setup(200.0);
        cast_and_release(&mut c, &mut p, &col, SENSOR_MINE);
        // The owner walks off: a Sensor Mine is set off by *anyone* (its owner too).
        p[0].pos.z = -100.0;
        run(&mut c, &mut p, &col, [0; 2], 300);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert!(c.areas[0].armed);
        p[1].pos.z = 25.0;
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
    }

    #[test]
    fn remote_mine_goes_off_on_the_next_button_press() {
        let (mut c, mut p, col) = setup(20.0);
        cast_and_release(&mut c, &mut p, &col, REMOTE_MINE);
        run(&mut c, &mut p, &col, [0; 2], 120);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        c.fighters[0].slots[1] = Some(LASER);
        press(&mut c, &mut p, &col, 0, 1);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
        assert_eq!(c.fighters[0].slots[1], Some(LASER), "the detonating press doesn't use a skill");
    }

    #[test]
    fn photon_wave_reaches_only_around_the_caster() {
        for (dist, expect) in [(30.0, START_HEALTH - 4), (90.0, START_HEALTH)] {
            let (mut c, mut p, col) = setup(dist);
            cast_and_release(&mut c, &mut p, &col, PHOTON_WAVE);
            // The blast grows from nothing to 40 u over 30 program ticks (60 sim ticks): it
            // reaches a body 30 u away (centre to centre, + the 5 u body) after 19 of them.
            run(&mut c, &mut p, &col, [0; 2], 36);
            assert_eq!(c.fighters[1].health, START_HEALTH, "not yet at {dist}: the sphere is still small");
            run(&mut c, &mut p, &col, [0; 2], 30);
            assert_eq!(c.fighters[1].health, expect, "target {dist} away");
        }
    }

    #[test]
    fn laser_arcs_out_and_back_onto_a_still_target() {
        let (mut c, mut p, col) = setup(200.0);
        cast_and_release(&mut c, &mut p, &col, LASER);
        let mut widest = 0.0f32;
        for _ in 0..60 {
            if let Some(s) = c.projectiles.first() {
                widest = widest.max(s.pos.x.abs());
            }
            c.step(&mut p, [0; 2], &col);
        }
        assert!(widest > 10.0, "bends away from the straight line: {widest}");
        assert_eq!(c.fighters[1].health, START_HEALTH - 1);
    }

    #[test]
    fn double_lasers_fire_two_that_both_arrive() {
        let (mut c, mut p, col) = setup(200.0);
        cast_skill(&mut c, &mut p, &col, 0, TWIN_LASERS);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        assert_eq!(c.projectiles.len(), 2);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.events.iter().filter(|e| matches!(e.1, CombatEvent::Hit { skill: TWIN_LASERS, .. })).count(), 2);
    }

    #[test]
    fn crawler_hits_on_the_ground_but_not_in_the_air() {
        for (up, expect) in [(0.0, START_HEALTH - 6), (30.0, START_HEALTH)] {
            let (mut c, mut p, col) = setup(100.0);
            cast_and_release(&mut c, &mut p, &col, ROOT);
            assert!(c.projectiles[0].pos.y < 2.0, "starts on the floor");
            p[1].pos.y = up; // (held in the air for the test)
            run(&mut c, &mut p, &col, [0; 2], 90);
            assert_eq!(c.fighters[1].health, expect, "target {up} up");
        }
    }

    /// Meteor 6 / 18 / 80 / 15: it slides sideways after the target by at most 15 / 10.8 u a tick
    /// while it comes in (about 25 ticks from 190 u at 7.4 u a tick), so a step of 15 u is followed
    /// and a step of 80 u dodges it (`FUN_1402657f0`).
    #[test]
    fn meteor_drops_where_the_target_stood() {
        for (step_aside, expect) in [(0.0, START_HEALTH - 6), (15.0, START_HEALTH - 6), (80.0, START_HEALTH)] {
            let (mut c, mut p, col) = setup(100.0);
            cast_and_release(&mut c, &mut p, &col, METEOR);
            assert!(c.projectiles[0].pos.y > 50.0, "starts high above (s64 x 10 = 60 u over the aim point)");
            p[1].pos.x += step_aside;
            run(&mut c, &mut p, &col, [0; 2], 90);
            assert_eq!(c.fighters[1].health, expect, "stepped {step_aside}");
        }
    }

    /// Player 1 puts up `guard`, then player 0 uses `attack`; runs `ticks`.
    /// 60 u apart: within the melee dash's 80 u lock-on band. `WAVE` gets a
    /// quick synthetic action row (fire tick 8 table ticks) so its hit lands
    /// well inside a defence's short lifetime -- these tests are about the
    /// guard mechanics, not `WAVE`'s own real timing (covered elsewhere).
    fn guard_vs(guard: i16, attack: i16, ticks: usize) -> (Combat, [Player; 2]) {
        let (mut c, mut p, col) = setup(60.0);
        if attack == WAVE {
            c.set_action_row(WAVE, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 201, charge_cycle: 0, dash_start: 0, dash_end: 8, fire_tick: 8, hit_window_end: 10 }), None);
        }
        cast_skill(&mut c, &mut p, &col, 1, guard);
        cast_skill(&mut c, &mut p, &col, 0, attack);
        run(&mut c, &mut p, &col, [0; 2], ticks);
        (c, p)
    }

    #[test]
    fn brush_blocks_close_range_and_stuns_the_attacker() {
        let (c, _) = guard_vs(BLOCK, WAVE, 22);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert_eq!(c.fighters[0].action, Action::Hit, "paralysed for an instant");
    }

    #[test]
    fn absorb_is_no_use_against_close_range() {
        let (c, _) = guard_vs(DIGESTION, WAVE, 30);
        assert!(c.fighters[1].health < START_HEALTH);
    }

    #[test]
    fn digestion_turns_blocked_damage_into_health() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[1].health = 10;
        cast_skill(&mut c, &mut p, &col, 1, DIGESTION);
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, 11, "Laser STR 1 blocked -> +1 health");
    }

    #[test]
    fn absorb_energy_turns_the_attack_cost_into_aura() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 1, ABSORB_ENERGY);
        c.fighters[1].aura_ticks = 0;
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert!(c.fighters[1].aura() >= 1, "Laser cost 1 absorbed");
    }

    #[test]
    fn reflect_sends_a_weaker_shot_back() {
        let (c, _) = guard_vs(REFLECT, LASER, 70);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert_eq!(c.fighters[0].health, START_HEALTH - 1, "own Laser came back");
    }

    #[test]
    fn reflect_breaks_on_a_stronger_shot() {
        let (c, _) = guard_vs(REFLECT, LANCE, 70);
        assert_eq!(c.fighters[0].health, START_HEALTH, "nothing came back");
        assert_eq!(c.fighters[1].health, START_HEALTH - 1, "STR 5 Pierce vs DEF 4");
    }

    /// [Course] stops nothing (`FUN_140271760` kind 2 only ORs the 0x100 flag and returns 0): a fast
    /// shell with a weak turn still lands (Blaster, 1.5 deg a tick), a slow shell that homes hard
    /// (8 deg a tick) is turned away from its target by the flag and misses.
    #[test]
    fn course_steers_away_and_stops_nothing() {
        let (c, _) = guard_vs(GUST, BLASTER, 70);
        assert_eq!(c.fighters[1].health, START_HEALTH - 4, "a Blaster is too fast and too stiff to be turned off in 60 u");
        assert!(c.events.iter().any(|e| matches!(e.1, CombatEvent::Blocked { victim: 1, skill: BLASTER, .. })), "the field is logged as it turns the shot");
        use super::testkit as k;
        let faith = k::def(309, Capsule::Attack, 1, &[(f::FAMILY, 12), (f::STRENGTH, 3), (k::P, -80), (k::P + 1, 30), (k::P + 4, 80)]);
        let gust = k::def(GUST, Capsule::Defence, 2, &[(f::FAMILY, 85), (43, 2), (f::STRENGTH, 28), (f::DEFENCE_KIND, 2), (f::STRENGTH_RULE, 800)]);
        let (mut c, mut p, col) = k::setup(vec![faith, gust], 100.0);
        // The shell leaves first; the Course field goes up while it is still on its way.
        k::use_skill(&mut c, &mut p, &col, 0, 309);
        k::use_skill(&mut c, &mut p, &col, 1, GUST);
        k::run(&mut c, &mut p, &col, 200);
        assert_eq!(c.fighters[1].health, START_HEALTH, "turned away");
    }

    #[test]
    fn course_is_no_use_against_rain() {
        let (mut c, mut p, col) = setup(100.0);
        cast_and_release(&mut c, &mut p, &col, RAIN);
        run(&mut c, &mut p, &col, [0; 2], 56);
        cast_skill(&mut c, &mut p, &col, 1, GUST);
        run(&mut c, &mut p, &col, [0; 2], 90);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3);
    }

    #[test]
    fn about_face_sends_shots_back() {
        let (c, _) = guard_vs(ABOUT_FACE, BLASTER, 90);
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert_eq!(c.fighters[0].health, START_HEALTH - 4, "own Blaster came back");
    }

    #[test]
    fn automatic_shelter_goes_up_by_itself_if_you_can_pay() {
        for (aura, expect) in [(8, START_HEALTH), (2, START_HEALTH - 4)] {
            let (mut c, mut p, col) = setup(100.0);
            c.fighters[1].slots[0] = Some(MINDS_EYE);
            c.fighters[1].level = 8;
            cast_skill(&mut c, &mut p, &col, 0, BLASTER);
            c.fighters[1].aura_ticks = aura * AURA_TICKS_PER_POINT;
            run(&mut c, &mut p, &col, [0; 2], 40);
            assert_eq!(c.fighters[1].health, expect, "aura {aura}");
            assert_eq!(c.fighters[1].slots[0].is_none(), aura >= 5, "used up only when it went up");
        }
    }

    #[test]
    fn automatic_shelter_cannot_be_cast_by_hand() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 1, MINDS_EYE);
        assert_eq!(c.fighters[1].slots[0], Some(MINDS_EYE));
        assert!(c.fighters[1].guard.is_none());
    }

    #[test]
    fn paralyze_barrier_holds_the_attacker_for_three_seconds() {
        let (c, _) = guard_vs(PARALYZE_BARRIER, WAVE, 22);
        // "Target can't move for 3 sec": bound (can't move), not stunned.
        assert!(c.fighters[0].status.bound_ticks > 150, "90 table ticks = 3 s: {}", c.fighters[0].status.bound_ticks);
        assert!(c.fighters[0].rooted());
    }

    #[test]
    fn anti_pierce_walls_stop_pierce_and_short_range_erasers_ignore_shots() {
        use super::testkit as k;
        // Crystal Wall's real fields: s44 0, s46 23, s47 3, s49 1 (Anti-Pierce).
        let mut sk = skills();
        sk.push(k::def(126, Capsule::Defence, 1, &[(f::DEFENCE_KIND, 0), (f::STRENGTH, 23), (f::DEFENCE, 3), (49, 1)]));
        // Wall of Current: erases short-range attacks only (s51 3, s50 1).
        sk.push(k::def(150, Capsule::Defence, 2, &[(f::DEFENCE_KIND, 0), (f::STRENGTH, 23), (f::DEFENCE, 6), (f::ADD_EFFECT, 3), (50, 1)]));
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        cast_skill(&mut c, &mut p, &col, 0, LANCE); // STR 5 Pierce vs DEF 3 (pressed first: the shot's arming tick)
        run(&mut c, &mut p, &col, [0; 2], 4);
        cast_skill(&mut c, &mut p, &col, 1, 126);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH, "Anti-Pierce: no damage regardless of STR");
        let (mut c, mut p, col) = k::setup({ let mut s = skills(); s.push(k::def(150, Capsule::Defence, 2, &[(f::DEFENCE_KIND, 0), (f::STRENGTH, 23), (f::DEFENCE, 6), (f::ADD_EFFECT, 3), (50, 1)])); s }, 100.0);
        cast_skill(&mut c, &mut p, &col, 1, 150);
        c.fighters[0].slots[1] = Some(LASER);
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[0].slots[1], Some(LASER), "a blocked shot isn't erased by a short-range-only eraser");
    }

    #[test]
    fn protected_and_passive_skills() {
        use super::testkit as k;
        let mut sk = skills();
        sk.push(k::def(264, Capsule::Attack, 2, &[(41, 2)])); // Sign of Saints: can't be overwritten
        sk.push(k::def(180, Capsule::Special, 2, &[(41, 2), (43, 4)])); // Orb: passive
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        // A capsule can't be captured onto a protected button.
        k::run(&mut c, &mut p, &col, CAPSULE_EVERY_TICKS as usize);
        let cap = c.capsules.iter().find(|x| x.owner == 0).unwrap().clone();
        p[0].pos = cap.pos;
        c.fighters[0].slots[2] = Some(264);
        k::press(&mut c, &mut p, &col, 0, 2);
        assert_eq!(c.fighters[0].slots[2], Some(264));
        // Orb can't be pressed: it stays, no aura spent.
        let (mut c, mut p, col) = k::setup({ let mut s = skills(); s.push(k::def(180, Capsule::Special, 2, &[(41, 2), (43, 4)])); s }, 100.0);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(180);
        k::press(&mut c, &mut p, &col, 0, 0);
        assert_eq!((c.fighters[0].slots[0], c.fighters[0].aura()), (Some(180), 8));
    }

    #[test]
    fn level_penalty_minus_one_means_level_zero() {
        use super::testkit as k;
        let mut sk = skills();
        sk.push(k::def(74, Capsule::Attack, 1, &[(f::LEVEL_PENALTY, -1), (k::P + 1, 150), (k::P + 4, 30)])); // Finisher
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 74);
        assert_eq!(c.fighters[0].level, 0, "\"Penalty: Level reduced to 0\"");
    }

    #[test]
    fn ice_lance_freezes_the_targets_buttons() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, ICE_LANCE);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH - 2);
        assert!(c.fighters[1].frozen_ticks > 500, "10 s");
        run(&mut c, &mut p, &col, [0; 2], HIT_STUN_TICKS as usize);
        cast_skill(&mut c, &mut p, &col, 1, LASER);
        assert_eq!(c.fighters[1].slots[0], Some(LASER), "frozen: the press did nothing");
    }

    #[test]
    fn dhampir_claw_heals_by_the_damage_dealt() {
        // 60 u: within the melee dash's 80 u lock-on band (no action row for
        // this test skill -> the fallback dash/fire window, `LUNGE_MAX_TICKS`).
        let (mut c, mut p, col) = setup(60.0);
        c.fighters[0].health = 10;
        cast_skill(&mut c, &mut p, &col, 0, DHAMPIR);
        // (52, was 40: the blow is armed one program tick = 2 sim ticks after the fire tick, `FUN_1402638d0`.)
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert_eq!((c.fighters[1].health, c.fighters[0].health), (START_HEALTH - 2, 12));
    }

    #[test]
    fn knock_down_floors_the_target_and_they_cant_be_hit_while_down() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, KI_BULLET);
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert_eq!(c.fighters[1].action, Action::Knocked);
        assert_eq!(c.fighters[1].health, START_HEALTH - 1);
        cast_skill(&mut c, &mut p, &col, 0, RAIL);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 1, "untouchable while getting up");
        run(&mut c, &mut p, &col, [0; 2], KNOCKDOWN_TICKS as usize);
        assert_eq!(c.fighters[1].action, Action::Ready, "back up after the clips");
    }

    #[test]
    fn an_ordinary_shot_knocks_down_only_in_the_air() {
        for (air, expect) in [(false, Action::Hit), (true, Action::Knocked)] {
            let (mut c, mut p, col) = setup(150.0);
            p[1].grounded = !air;
            cast_skill(&mut c, &mut p, &col, 0, RAIL);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 14);
            assert_eq!(c.fighters[1].action, expect, "in the air: {air}");
        }
    }

    #[test]
    fn pursuit_only_works_on_a_downed_target() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, PURSUIT);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH, "standing: no effect");
        cast_skill(&mut c, &mut p, &col, 0, KI_BULLET);
        run(&mut c, &mut p, &col, [0; 2], 40);
        cast_skill(&mut c, &mut p, &col, 0, PURSUIT);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 1 - 3);
    }

    #[test]
    fn falling_out_of_the_arena_costs_three() {
        let (mut c, _, _) = setup(100.0);
        c.fall_out(1);
        assert_eq!(c.fighters[1].health, START_HEALTH - FALL_OUT_DAMAGE);
    }

    #[test]
    fn light_medium_heavy_pick_by_damage_in_the_last_3_seconds() {
        // PROGRAM-RE.md "Hit reactions": < 3 light (s57), 3-4 medium (s58),
        // >= 5 heavy (s59), by the damage accumulator including this hit.
        let (mut c, mut p, col) = setup(150.0);
        c.set_reaction_row(201, ReactionRow { kind: reaction_kind::CLIP, ..Default::default() });
        c.set_reaction_row(202, ReactionRow { kind: reaction_kind::CLIP, ..Default::default() });
        c.set_reaction_row(203, ReactionRow { kind: reaction_kind::CLIP, ..Default::default() });
        cast_skill(&mut c, &mut p, &col, 0, POKE);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!((c.fighters[1].health, c.fighters[1].reaction_row), (START_HEALTH - 2, 201), "2 damage: light");
        cast_skill(&mut c, &mut p, &col, 0, POKE);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!((c.fighters[1].health, c.fighters[1].reaction_row), (START_HEALTH - 4, 202), "4 in the last 3 s: medium");
        cast_skill(&mut c, &mut p, &col, 0, POKE);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!((c.fighters[1].health, c.fighters[1].reaction_row), (START_HEALTH - 6, 203), "6 in the last 3 s: heavy");
    }

    #[test]
    fn knock_down_timeline_flight_lying_getup_then_invulnerable() {
        // Golden values from row 3 (FORMATS-SKILLS.md): launch 20 at 50 deg.
        let (mut c, mut p, col) = setup(150.0);
        c.set_reaction_row(AIR_KNOCKDOWN_ROW, ReactionRow { kind: reaction_kind::KNOCKDOWN, launch_speed: 20, launch_angle: 50, voice_group: 5, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
        p[1].grounded = false; // airborne: any hit knocks down (row 27)
        cast_skill(&mut c, &mut p, &col, 0, RAIL);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 14);
        assert_eq!(c.fighters[1].action, Action::Knocked);
        assert!(c.fighters[1].flying, "launched into the air");
        let health_after_launch = c.fighters[1].health;
        // ~0.8 s of flight (20 @ 50 deg with the program's own knock-down
        // gravity, PROGRAM-RE.md "Knock-down physics"): well landed by 70 ticks.
        run(&mut c, &mut p, &col, [0; 2], 70);
        assert!(!c.fighters[1].flying, "landed by now");
        assert_eq!(c.fighters[1].action, Action::Knocked);
        assert!(c.fighters[1].action_ticks > KNOCK_GETUP_TICKS, "still lying");
        // Through the rest of lying, then getting up.
        run(&mut c, &mut p, &col, [0; 2], KNOCKDOWN_TICKS as usize);
        assert_eq!(c.fighters[1].action, Action::Ready, "back up after the clips");
        assert!(c.fighters[1].invuln_ticks > 0 && c.fighters[1].invuln_ticks <= POST_GETUP_INVULN_TICKS, "blinking, untouchable: {}", c.fighters[1].invuln_ticks);
        // No skill's s62 sets bit 8, so nothing lands while this is up.
        cast_skill(&mut c, &mut p, &col, 0, RAIL);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, health_after_launch, "untouchable while blinking");
        run(&mut c, &mut p, &col, [0; 2], POST_GETUP_INVULN_TICKS as usize);
        assert_eq!(c.fighters[1].invuln_ticks, 0);
    }

    #[test]
    fn s62_downed_bits_gate_which_attacks_reach_a_downed_target() {
        let (mut c, mut p, col) = setup(100.0);
        // Backdraft Bullet-like: s62 = 6, no normal-state bit.
        cast_skill(&mut c, &mut p, &col, 0, BACKDRAFT);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, START_HEALTH, "standing: s62 has no normal-state bit");
        cast_skill(&mut c, &mut p, &col, 0, KI_BULLET);
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert_eq!(c.fighters[1].action, Action::Knocked);
        let after_knockdown = c.fighters[1].health;
        // An ordinary attack still whiffs while they're down (existing rule).
        cast_skill(&mut c, &mut p, &col, 0, RAIL);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, after_knockdown, "ordinary attacks can't touch a downed target");
        // Backdraft Bullet now lands.
        cast_skill(&mut c, &mut p, &col, 0, BACKDRAFT);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert_eq!(c.fighters[1].health, after_knockdown - 2, "s62 bits 2|4: effective on a downed target");

        // Gravity Press (s62 = 7) is effective on a standing target too.
        let (mut c2, mut p2, col2) = setup(100.0);
        cast_skill(&mut c2, &mut p2, &col2, 0, GRAVITY_PRESS);
        run(&mut c2, &mut p2, &col2, [0; 2], 40);
        assert_eq!(c2.fighters[1].health, START_HEALTH - 3, "s62 = 7: also effective while standing");
    }

    #[test]
    fn stagger_direction_matches_where_the_attack_came_from() {
        let cases = [
            (std::f32::consts::PI, HitDir::Front),
            (0.0, HitDir::Back),
            // The program's sector 1 is the side DMG_SHOT_RIGHT recoils away from (see
            // `the_programs_right_sector_is_the_side_its_clip_recoils_away_from`).
            (std::f32::consts::FRAC_PI_2, HitDir::Left),
            (-std::f32::consts::FRAC_PI_2, HitDir::Right),
        ];
        for (yaw, expect) in cases {
            let (mut c, mut p, col) = setup(100.0);
            p[1].yaw = yaw;
            cast_skill(&mut c, &mut p, &col, 0, RAIL);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 10);
            assert_eq!(c.fighters[1].hit_dir, expect, "yaw {yaw}");
        }
    }

    #[test]
    fn reaction_clip_picks_the_rows_own_pose_by_kind_and_direction() {
        // Golden values from the reaction table (FORMATS-SKILLS.md): row 2
        // (four-way stagger) w = [117, 120, 118, 119] = DMG_SHOT_FRONT/RIGHT/BACK/LEFT;
        // row 3 (knock-down) w42/w46 = 45/48 = BLOW0_FRONT/BACK.
        let (mut c, _p, _col) = setup(100.0);
        c.set_reaction_row(2, ReactionRow { kind: reaction_kind::FOUR_WAY, w: [117, 120, 118, 119, 0, 0, 0, 0], ..Default::default() });
        c.fighters[1].reaction_row = 2;
        for (dir, want) in [(HitDir::Front, "DMG_SHOT_FRONT"), (HitDir::Right, "DMG_SHOT_RIGHT"), (HitDir::Back, "DMG_SHOT_BACK"), (HitDir::Left, "DMG_SHOT_LEFT")] {
            c.fighters[1].hit_dir = dir;
            assert_eq!(c.reaction_clip(1), want);
        }
        c.set_reaction_row(3, ReactionRow { kind: reaction_kind::KNOCKDOWN, launch_speed: 20, launch_angle: 50, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
        c.fighters[1].reaction_row = 3;
        c.fighters[1].hit_dir = HitDir::Front;
        c.fighters[1].hit_front = true;
        assert_eq!(c.reaction_clip(1), "BLOW0_FRONT");
        c.fighters[1].hit_dir = HitDir::Back;
        c.fighters[1].hit_front = false;
        assert_eq!(c.reaction_clip(1), "BLOW0_BACK");
    }

    // ---- groundwork for the non-attack capsules (moves/effects.rs) ----

    #[test]
    fn environment_skill_sets_one_crystal_that_can_be_taken_after_15_s() {
        use super::testkit as k;
        // (s44 = status id 0, s45 = 450 capture lock, s46 = amount: Holy Ray's numbers)
        let env_a = k::def(400, Capsule::Environment, 1, &[(43, 5), (44, 0), (45, 450), (46, -1)]);
        let env_b = k::def(401, Capsule::Environment, 1, &[(43, 5), (44, 0), (45, 450), (46, 1)]);
        let (mut c, mut p, col) = k::setup(vec![env_a, env_b], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 400);
        assert_eq!(c.crystal.as_ref().map(|x| x.skill), Some(400));
        k::use_skill(&mut c, &mut p, &col, 1, 401);
        assert_eq!(c.crystal.as_ref().map(|x| x.skill), Some(400), "only one at a time");
        p[1].pos = c.crystal.as_ref().unwrap().pos;
        k::press(&mut c, &mut p, &col, 1, 1);
        assert!(c.crystal.is_some(), "not before 15 s");
        c.crystal.as_mut().unwrap().age = CRYSTAL_CAPTURE_AFTER + 1;
        k::press(&mut c, &mut p, &col, 1, 1);
        assert!(c.crystal.is_none(), "captured: the effect ends");
        assert_eq!(c.fighters[1].slots[1], Some(AURA_PARTICLE), "and the capturer gets an Aura Particle on the button they pressed");
    }

    #[test]
    fn a_non_attack_shot_lands_without_damage_or_stun() {
        use super::testkit as k;
        // Tiger Slayer-like erase shell: s64..s68 = -80 120 0 0 100.
        let slayer = k::def(216, Capsule::Erase, 2, &[(k::P, -80), (k::P + 1, 120), (k::P + 4, 100)]);
        let (mut c, mut p, col) = k::setup(vec![slayer], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 216);
        assert_eq!(c.projectiles.len(), 1, "travels like a shell");
        k::run(&mut c, &mut p, &col, 30);
        assert!(c.events.iter().any(|e| e.1 == CombatEvent::Hit { attacker: 0, victim: 1, skill: 216 }));
        assert_eq!((c.fighters[1].health, c.fighters[1].action), (START_HEALTH, Action::Ready));
    }

    #[test]
    fn status_hooks_reach_the_sim() {
        use super::testkit as k;
        let (mut c, mut p, col) = k::setup(skills(), 100.0);
        c.fighters[0].status.add(Stat::Speed, 20, None);
        assert!((c.move_factor(0) - 1.2).abs() < 1e-6);
        c.fighters[0].status.add(Stat::Attack, 2, Some(10));
        assert_eq!(c.damage(0, BLASTER), 4 + 2, "attack bonus");
        k::run(&mut c, &mut p, &col, 10);
        assert_eq!(c.damage(0, BLASTER), 4, "timed bonus ran out");
        c.fighters[1].status.immune_ticks = 600;
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], 60);
        assert_eq!(c.fighters[1].health, START_HEALTH, "Void: immune");
        c.fighters[0].status.bound_ticks = 10;
        assert!(c.fighters[0].rooted());
        c.fighters[0].status.reset();
        assert_eq!(c.fighters[0].status.total(Stat::Speed), 0);
    }

    /// Aura gained over `ticks` sim ticks from `aura` points, in points.
    fn refill_gain(aura: f32, ticks: usize, setup_fighter: impl Fn(&mut Fighter)) -> f32 {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].level = 30;
        c.fighters[0].aura_ticks = (aura * AURA_TICKS_PER_POINT as f32) as i32;
        setup_fighter(&mut c.fighters[0]);
        let before = c.fighters[0].aura_ticks;
        run(&mut c, &mut p, &col, [0; 2], ticks);
        (c.fighters[0].aura_ticks - before) as f32 / AURA_TICKS_PER_POINT as f32
    }

    /// `FUN_14024de20`: per program tick `handicap x max(0.25, 1.25 - Aura/10) / 30`
    /// points (constants `0x1402f7ddc` 1.25, `0x1402f7f7c` 10, `0x1402f7d0c` 0.25,
    /// `0x1402f7fd8` 30). 1.25 points/s at Aura 0, 0.75 at 5, 0.25 from 10 up.
    #[test]
    fn aura_refill_follows_the_programs_curve() {
        // From 10 up it is flat 0.25/s: 4 s = 1.0 point (the program ticks at 30 Hz, the sim at 60).
        assert!((refill_gain(10.0, 240, |_| {}) - 1.0).abs() < 0.002, "{}", refill_gain(10.0, 240, |_| {}));
        assert!((refill_gain(20.0, 120, |_| {}) - 0.5).abs() < 0.002);
        // At 5 it starts at 0.75/s, at 0 at 1.25/s (a short stretch, so the curve barely moves).
        assert!((refill_gain(5.0, 12, |_| {}) - 0.75 * 0.2).abs() < 0.005);
        assert!((refill_gain(0.0, 12, |_| {}) - 1.25 * 0.2).abs() < 0.005);
        // The long way round matches the program's own arithmetic tick by tick.
        let mut a = 0.0f32;
        for _ in 0..60 {
            a += (1.25f32 - a / 10.0).max(0.25) / 30.0;
        }
        let got = refill_gain(0.0, 120, |_| {});
        assert!((got - a).abs() < 0.01, "2 s from empty: {got} vs {a}");
    }

    /// The arsenal's handicap multiplies the rate (1.3 / 1.0 / 0.7 for 1 / 2 / 3
    /// schools, gsdata `0x33034`) and so does the status: `(100 + status3) / 100`.
    #[test]
    fn aura_refill_multiplies_handicap_and_status() {
        let base = refill_gain(10.0, 240, |_| {});
        assert!((refill_gain(10.0, 240, |f| f.aura_handicap = 1.3) - base * 1.3).abs() < 0.003);
        assert!((refill_gain(10.0, 240, |f| f.aura_handicap = 0.7) - base * 0.7).abs() < 0.003);
        assert!((refill_gain(10.0, 240, |f| f.status.add(Stat::AuraRate, 100, None)) - base * 1.9).abs() < 0.003, "capped at +90");
        assert!((refill_gain(10.0, 240, |f| f.status.add(Stat::AuraRate, -30, None)) - base * 0.7).abs() < 0.003);
        assert!((refill_gain(10.0, 240, |f| {
            f.aura_handicap = 1.3;
            f.status.add(Stat::AuraRate, 50, None);
        }) - base * 1.3 * 1.5)
            .abs()
            < 0.004);
    }

    /// `FUN_1402523a0`: no recovery with no Health, nor during any cast (states
    /// 10-13: the whole clip); it is capped at Level.
    #[test]
    fn aura_refill_pauses_during_a_cast_and_stops_at_level() {
        let (mut c, mut p, col) = setup(300.0);
        c.fighters[0].level = 4;
        c.fighters[0].aura_ticks = 0;
        // A real cast (state 10): the Aura Particle is no cast state (it holds nobody).
        c.fighters[0].action = Action::Cast;
        c.fighters[0].action_ticks = 40;
        let during = c.fighters[0].aura_ticks;
        assert_eq!(c.fighters[0].action, Action::Cast);
        c.step(&mut p, [0; 2], &col);
        assert_eq!(c.fighters[0].aura_ticks, during, "no recovery while casting");
        run(&mut c, &mut p, &col, [0; 2], 3000);
        assert_eq!(c.fighters[0].aura_ticks, c.fighters[0].level * AURA_TICKS_PER_POINT, "full, and never above Level");
        c.fighters[1].level = 10;
        c.fighters[1].aura_ticks = 0;
        c.fighters[1].action = Action::Down;
        c.fighters[1].health = 0;
        run(&mut c, &mut p, &col, [0; 2], 200);
        assert_eq!(c.fighters[1].aura_ticks, 0, "none with no Health");
    }

    #[test]
    fn scrambled_buttons_use_another_slot() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].slots = [Some(LASER), None, None, Some(AURA_PARTICLE)];
        c.fighters[0].status.scrambled_ticks = 100;
        c.fighters[0].status.scramble = [3, 2, 1, 0];
        press(&mut c, &mut p, &col, 0, 0);
        assert_eq!(c.fighters[0].slots, [Some(LASER), None, None, None], "button 0 used slot 3");
    }

    // ---- leftover attacks ----

    #[test]
    fn ricochet_laser_fires_four_that_bounce_off_a_wall() {
        use super::testkit as k;
        let mut ric = k::def(77, Capsule::Attack, 2, &[(f::MOTION, 17), (k::P, 90), (k::P + 1, 80), (k::P + 2, 80), (k::P + 4, -45), (k::P + 5, -45), (f::STRENGTH, 2)]);
        ric.s70_73 = [4, 0, 90, 3];
        // A wall across z = 60 between the players.
        let s = 1000.0;
        let col = Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
            [Vec3::new(-s, 0.0, 60.0), Vec3::new(s, 0.0, 60.0), Vec3::new(s, 50.0, 60.0)],
            [Vec3::new(-s, 0.0, 60.0), Vec3::new(s, 50.0, 60.0), Vec3::new(-s, 50.0, 60.0)],
        ]);
        let (mut c, mut p, _) = k::setup(vec![ric], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 77);
        assert_eq!(c.projectiles.len(), 4);
        k::run(&mut c, &mut p, &col, 40);
        assert!(c.projectiles.iter().any(|x| x.shot.heading().z < 0.0), "came back off the wall");
    }

    /// `FUN_14026ca50` takes the face's STORED normal (`FUN_140163e80`) and ignores a contact the laser moves
    /// along: the two lasers that climb from the chest pass up through a deck above them (its stored normal
    /// points up) without turning or counting a bounce, and the two that dive turn on the floor once. A laser
    /// whose life runs out ends silently (impact code 0: `CombatEvent::ShotExpired`). LIVE
    /// `077_Ricochet_Laser`: the original never hits; ours spent its three bounces on faces it was leaving
    /// and turned on the target.
    #[test]
    fn ricochet_passes_the_back_of_a_face_and_expires_silently() {
        use super::testkit as k;
        let mut ric = k::def(77, Capsule::Attack, 2, &[(f::MOTION, 17), (k::P, 90), (k::P + 1, 80), (k::P + 2, 80), (k::P + 4, -45), (k::P + 5, -45), (f::STRENGTH, 2)]);
        ric.s70_73 = [4, 0, 90, 3];
        let s = 1000.0;
        // Wound so the cross product points up, as the program's stored normals of a floor and a deck do.
        let up = |y: f32| [[Vec3::new(-s, y, -s), Vec3::new(s, y, s), Vec3::new(s, y, -s)], [Vec3::new(-s, y, -s), Vec3::new(-s, y, s), Vec3::new(s, y, s)]];
        let tris: Vec<[Vec3; 3]> = up(0.0).into_iter().chain(up(20.0)).collect();
        let col = Collision::new_signed(tris, &[pd_formats::collision::NO_OBJECT; 4]);
        let (mut c, mut p, _) = k::setup(vec![ric], 300.0);
        k::use_skill(&mut c, &mut p, &col, 0, 77);
        assert_eq!(c.projectiles.len(), 4);
        k::run(&mut c, &mut p, &col, 24);
        let flying: Vec<(f32, i16)> = c.projectiles.iter().map(|x| (x.pos.y, match x.shot.flight { moves::Flight::Ricochet(f) => f.bounces, _ => -9 })).collect();
        assert_eq!(flying.iter().filter(|&&(y, b)| y > 20.0 && b == 3).count(), 2, "two climbed through the deck untouched: {flying:?}");
        assert_eq!(flying.iter().filter(|&&(_, b)| b == 2).count(), 2, "two turned on the floor once: {flying:?}");
        k::run(&mut c, &mut p, &col, 200);
        assert!(c.projectiles.is_empty());
        let expired = c.events.iter().filter(|(_, e)| matches!(e, CombatEvent::ShotExpired { skill: 77, .. })).count();
        assert_eq!(expired, 4, "each laser ran out of life: {:?}", c.events);
    }

    #[test]
    fn recollection_repeats_your_last_attack_and_copy_the_last_that_hurt_you() {
        use super::testkit as k;
        let mut sk = skills();
        sk.push(k::def(206, Capsule::Attack, 2, &[(42, 100), (43, 6), (44, 0)]));
        sk.push(k::def(205, Capsule::Attack, 2, &[(42, 100), (43, 6), (44, 1)]));
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 206);
        assert!(c.projectiles.is_empty(), "nothing to repeat yet");
        k::use_skill(&mut c, &mut p, &col, 0, BLASTER);
        k::run(&mut c, &mut p, &col, 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 4);
        k::use_skill(&mut c, &mut p, &col, 0, 206);
        k::run(&mut c, &mut p, &col, 40);
        assert_eq!(c.fighters[1].health, START_HEALTH - 8, "Recollection = Blaster again");
        k::run(&mut c, &mut p, &col, 40);
        k::use_skill(&mut c, &mut p, &col, 1, 205);
        k::run(&mut c, &mut p, &col, 40);
        assert_eq!(c.fighters[0].health, START_HEALTH - 4, "Copy = the Blaster that hurt you");
    }

    #[test]
    fn pursuit_dashes_at_a_downed_target() {
        use super::testkit as k;
        let mut sk = skills();
        sk.push(k::def(197, Capsule::Attack, 1, &[(f::MOTION, 14), (k::P, 250), (k::P + 1, 8), (k::P + 2, 10), (57, 24), (58, 24), (59, 24), (f::STRENGTH, 3)]));
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        cast_skill(&mut c, &mut p, &col, 0, KI_BULLET);
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert_eq!(c.fighters[1].action, Action::Knocked);
        k::use_skill(&mut c, &mut p, &col, 0, 197);
        k::run(&mut c, &mut p, &col, 30);
        assert_eq!(c.fighters[1].health, START_HEALTH - 1 - 3);
    }

    /// The Dash skill is a scripted move of the body now (`Player::begin_dash`): the sim's wall push
    /// stops it like any other move, so the wall at z = 40 holds it short of the wall.
    #[test]
    fn dash_stops_at_a_wall() {
        use super::testkit as k;
        let dash = k::def(240, Capsule::Special, 2, &[(43, 3), (44, moves::special::move_kind::DASH), (45, 10)]);
        let s = 1000.0;
        let mut col = Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
            [Vec3::new(-s, 0.0, 40.0), Vec3::new(s, 0.0, 40.0), Vec3::new(s, 50.0, 40.0)],
            [Vec3::new(-s, 0.0, 40.0), Vec3::new(s, 50.0, 40.0), Vec3::new(-s, 50.0, 40.0)],
        ]);
        let mut combat = Combat::new(vec![dash], vec![AURA_PARTICLE; 30]);
        combat.set_rules(k::TEST_RULES);
        combat.round_ticks = COUNTDOWN_TICKS;
        combat.set_action_row(240, Some(ActionRow { anim_name: "SPE_DASH".into(), cast_sound: 0, charge_cycle: 0, dash_start: 0, dash_end: 15, fire_tick: 0, hit_window_end: 0 }), None);
        let mut m = crate::sim::Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 400.0)], combat);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(240);
        for t in 0..80 {
            let skills = if t == 0 { 1 } else { 0 };
            m.step([crate::sim::Input { skills, ..Default::default() }, crate::sim::Input::default()], &mut col);
        }
        let z = m.players[0].pos.z;
        assert!(z < 40.0 && z > 20.0, "stopped short of the wall at z 40: {z}");
    }

    #[test]
    fn time_up_gives_the_round_to_the_healthier_player() {
        let (mut c, mut p, col) = setup(100.0);
        c.time_limit = 60;
        c.fighters[0].health = 15;
        run(&mut c, &mut p, &col, [0; 2], 61);
        assert!(c.time_up);
        assert_eq!(c.wins[..2], [0, 1]);
    }

    #[test]
    fn powerless_players_lose_health() {
        let (mut c, mut p, col) = setup(100.0);
        c.fighters[0].arsenal.clear();
        c.capsules.retain(|k| k.owner != 0);
        run(&mut c, &mut p, &col, [0; 2], 2 * POWERLESS_DRAIN_TICKS as usize);
        assert!(c.fighters[0].health < START_HEALTH, "no skills left anywhere");
        assert_eq!(c.fighters[1].health, START_HEALTH);
    }

    #[test]
    fn each_player_brings_their_own_arsenal_and_school_speed() {
        use super::testkit as k;
        let mut sk = skills();
        sk.push(k::def(900, Capsule::Attack, 1, &[(f::SCHOOL, 1)]));
        sk.push(k::def(901, Capsule::Attack, 1, &[(f::SCHOOL, 2)]));
        sk.push(k::def(902, Capsule::Attack, 1, &[(f::SCHOOL, 3)]));
        let a = vec![900; 30];
        let b: Vec<i16> = [900, 901, 902].repeat(10);
        let c = Combat::with_arsenals(sk, [a, b]);
        assert!(c.fighters[0].arsenal.iter().all(|&x| x == 900));
        assert!(c.fighters[1].arsenal.contains(&902));
        // The multiplier by the arsenal's number of schools (gsdata 0x33034:
        // 1 school 1.3, 2 schools 1.0, 3 schools 0.7; `FUN_14024de20`).
        assert_eq!((c.fighters[0].aura_handicap, c.fighters[1].aura_handicap), (1.3, 0.7), "one school faster, three slower");
    }

    // ---- audit fixes ----

    #[test]
    fn a_double_knock_out_on_the_same_tick_is_a_draw() {
        let (mut c, mut p, col) = setup(30.0);
        // Photon-Wave-like blasts from both at once: each reaches the other.
        c.fighters[0].health = 1;
        c.fighters[1].health = 1;
        c.fighters[0].slots[0] = Some(LASER);
        c.fighters[1].slots[0] = Some(LASER);
        c.step(&mut p, [1, 1], &col);
        c.step(&mut p, [0, 0], &col);
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert!(c.fighters[..2].iter().all(|f| f.action == Action::Down), "both down: {:?}", c.fighters.iter().map(|f| f.health).collect::<Vec<_>>());
        assert_eq!(c.wins[..2], [0, 0]);
        assert_eq!(c.round_over.map(|r| r.0), Some(DRAW), "draw");
    }

    #[test]
    fn two_buttons_on_the_same_tick_both_happen() {
        let (mut c, mut p, col) = setup(100.0);
        // Button 0's slot is empty (nothing happens); button 1 holds an aura
        // particle. Pressed together and held, button 1 must still be used.
        c.fighters[0].slots = [None, Some(AURA_PARTICLE), None, None];
        run(&mut c, &mut p, &col, [0b11, 0], 3);
        assert_eq!(c.fighters[0].level, TEST_RULES.start_level + 1, "the second press was not lost");
    }

    #[test]
    fn capsules_appear_at_the_stages_own_spots() {
        let (mut c, mut p, col) = setup(100.0);
        let spots = vec![Vec3::new(-50.0, 0.0, -20.0), Vec3::new(0.0, 0.0, -20.0), Vec3::new(50.0, 0.0, -20.0)];
        c.capsule_spots[0] = spots.clone();
        run(&mut c, &mut p, &col, [0; 2], 4 * CAPSULE_EVERY_TICKS as usize);
        let mine: Vec<Vec3> = c.capsules.iter().filter(|k| k.owner == 0).map(|k| k.pos).collect();
        assert_eq!(mine.len(), 3, "one per spot");
        assert!(mine.iter().all(|m| spots.contains(m)));
    }

    #[test]
    fn same_inputs_same_result() {
        let script = |c: &mut Combat, p: &mut [Player; 2], col: &Collision| {
            for t in 0..900u32 {
                let b = [((t / 7) % 16) as u8, ((t / 11) % 16) as u8];
                c.step(p, b, col);
            }
            (c.fighters[0].health, c.fighters[1].health, c.fighters[0].level, c.capsules.len(), c.rng)
        };
        let (mut a, mut pa, col) = setup(100.0);
        let (mut b, mut pb, _) = setup(100.0);
        assert_eq!(script(&mut a, &mut pa, &col), script(&mut b, &mut pb, &col));
    }

    // --- Stage objects (props.rs) -------------------------------------------

    const PSYCHO_KINESIS: i16 = 18;

    /// Flat stage floor plus a 60-wide, 40-tall wall slab across z = `z`
    /// that belongs to object 0.
    fn floor_with_wall(z: f32) -> Collision {
        let s = 1000.0;
        let (a, b, c, d) = (Vec3::new(-30.0, 0.0, z), Vec3::new(30.0, 0.0, z), Vec3::new(30.0, 40.0, z), Vec3::new(-30.0, 40.0, z));
        let tris = vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
            [a, b, c],
            [a, c, d],
        ];
        Collision::new_tagged(tris, &[NO_OBJECT, NO_OBJECT, 0, 0])
    }

    fn wall_prop(z: f32) -> Prop {
        Prop::new(0, 2, 3, Vec3::new(-30.0, 0.0, z - 1.0), Vec3::new(30.0, 40.0, z + 1.0), true, true)
    }

    #[test]
    fn shots_wear_an_object_down_until_it_breaks_and_stops_blocking() {
        let (mut c, mut p, _) = setup(100.0);
        let mut col = floor_with_wall(50.0);
        c.props = vec![wall_prop(50.0)];
        assert_eq!(c.props[0].toughness, 5);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 60);
        assert_eq!(c.fighters[1].health, START_HEALTH, "the wall took the shot");
        assert_eq!((c.props[0].state, c.props[0].wear), (PropState::Damaged, 4), "STR 4 of toughness 5: damaged");
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 60);
        assert_eq!(c.props[0].state, PropState::Broken);
        c.sync_collision(&mut col);
        assert!(col.object_gone(0), "a broken object's collision goes");
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 60);
        assert_eq!(c.fighters[1].health, START_HEALTH - 4, "nothing in the way now");
    }

    #[test]
    fn psycho_kinesis_needs_an_object_in_reach_and_throws_it() {
        let (mut c, mut p, col) = setup(150.0);
        // [Object], table speed 80, homing 30 (Psycho Kinesis's numbers).
        c.skills.push(def(PSYCHO_KINESIS, Capsule::Attack, 19, 2, 1, 0, &[(f::MOTION, 11), (P, -30), (P + 1, 80), (P + 4, 30)]));
        cast_skill(&mut c, &mut p, &col, 0, PSYCHO_KINESIS);
        // LIVE takes 018 / 017: with no object the cast still happens (the fighter is held, the skill fires),
        // only nothing is thrown.
        assert_eq!(c.fighters[0].action, Action::Cast, "no object: the press still casts");
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 40);
        assert!(c.projectiles.is_empty(), "nothing to throw");
        assert_eq!(c.fighters[1].health, START_HEALTH);
        // A chair-sized object just behind player 0 (STR by size: 20 -> 2).
        c.props = vec![Prop::new(3, 2, 3, Vec3::new(-4.0, 0.0, -24.0), Vec3::new(4.0, 20.0, -16.0), true, true)];
        cast_skill(&mut c, &mut p, &col, 0, PSYCHO_KINESIS);
        assert_eq!(c.fighters[0].slots[0], Some(PSYCHO_KINESIS), "cast (unlimited use: it stays)");
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
        assert_eq!(c.props[0].state, PropState::Thrown);
        assert_eq!(c.projectiles.iter().filter(|s| s.prop == Some(0)).count(), 1);
        assert!(c.projectiles[0].pos.z < -10.0, "flies from where the object stood");
        run(&mut c, &mut p, &col, [0; 2], 120);
        assert_eq!(c.fighters[1].health, START_HEALTH - 2, "STR depends on object");
        assert_eq!(c.props[0].state, PropState::Gone, "shattered on impact");
    }

    #[test]
    fn a_new_round_puts_every_object_back() {
        let (mut c, _, _) = setup(100.0);
        c.props = vec![wall_prop(50.0)];
        c.props[0].wear_down(99);
        assert_eq!(c.props[0].state, PropState::Broken);
        c.start_round();
        assert_eq!((c.props[0].state, c.props[0].wear), (PropState::Intact, 0));
    }

    #[test]
    fn a_brushed_shell_flies_aside_and_destroys_objects_in_its_way() {
        // "when they successfully 'brush' a Shell-type attack, anything in the
        // way of the brushed shell will be destroyed" (guide).
        const STRONG_BRUSH: i16 = 391;
        let (mut c, mut p, _) = setup(100.0);
        c.skills.push(def(STRONG_BRUSH, Capsule::Defence, 84, 2, 60, 0, &[(f::DEFENCE_KIND, 3), (f::DEFENCE, 9)]));
        // A big wall object off to player 1's side (the shell comes along +z,
        // so "aside" is -x): the plane x = -60, z 60..260.
        let s = 1000.0;
        let (a, b, cc, d) = (Vec3::new(-60.0, 0.0, 60.0), Vec3::new(-60.0, 0.0, 260.0), Vec3::new(-60.0, 60.0, 260.0), Vec3::new(-60.0, 60.0, 60.0));
        let col = Collision::new_tagged(
            vec![[Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)], [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)], [a, b, cc], [a, cc, d]],
            &[NO_OBJECT, NO_OBJECT, 0, 0],
        );
        c.props = vec![Prop::new(0, 2, 3, Vec3::new(-61.0, 0.0, 60.0), Vec3::new(-59.0, 60.0, 260.0), true, true)];
        assert!(c.props[0].toughness > 4, "far tougher than the shell's STR");
        cast_skill(&mut c, &mut p, &col, 1, STRONG_BRUSH);
        cast_skill(&mut c, &mut p, &col, 0, BLASTER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 90);
        assert_eq!((c.fighters[0].health, c.fighters[1].health), (START_HEALTH, START_HEALTH), "brushed: hurts nobody");
        assert_eq!(c.props[0].state, PropState::Broken, "destroyed what it met");
    }

    #[test]
    fn auto_capture_puts_the_next_skill_on_the_button_at_once() {
        const EMPTY_HAND: i16 = 181;
        let (mut c, mut p, col) = setup(300.0);
        c.skills.push(def(EMPTY_HAND, Capsule::Attack, 10, 1, 1, 0, &[(f::MOTION, 13), (P, 130), (P + 1, 8), (P + 2, 30), (51, AUTO_CAPTURE), (f::USES, 1)]));
        c.fighters[0].arsenal = vec![WALL, LASER];
        cast_skill(&mut c, &mut p, &col, 0, EMPTY_HAND);
        run(&mut c, &mut p, &col, [0; 2], LUNGE_MAX_TICKS as usize + 4);
        assert_eq!(c.fighters[0].slots[0], Some(LASER), "the next skill of the arsenal, hit or miss");
        assert_eq!(c.fighters[0].arsenal, vec![WALL]);
        // `FUN_140261a00` with its sound flag (LIVE: Empty Hand / Hyperpunch / Sky Pursuit take `capture:` + common/86).
        assert!(c.events.iter().any(|(_, e)| matches!(e, CombatEvent::SkillCaptured { who: 0, slot: 0, skill: LASER })), "the capture is announced");
    }

    #[test]
    fn tiger_kick_erases_an_equipped_attack_skill_when_it_lands() {
        // "Add: Erases 1 equipped attack skill from target" (s48 1, s49 2, s50 1).
        const TIGER_KICK: i16 = 184;
        let (mut c, mut p, col) = setup(20.0);
        c.skills.push(def(TIGER_KICK, Capsule::Attack, 10, 1, 1, 0, &[(f::MOTION, 13), (P, 130), (P + 1, 8), (P + 2, 30), (48, 1), (49, 2), (50, 1)]));
        c.fighters[1].slots = [Some(LASER), Some(WALL), None, None];
        cast_skill(&mut c, &mut p, &col, 0, TIGER_KICK);
        // (52, was 40: the blow is armed one program tick = 2 sim ticks after the fire tick, `FUN_1402638d0`.)
        run(&mut c, &mut p, &col, [0; 2], 52);
        assert!(c.fighters[1].health < START_HEALTH, "it landed");
        assert_eq!(c.fighters[1].slots, [None, Some(WALL), None, None], "the attack skill went, the defence stayed");
    }

    // --- Attack STR rules (str_rule) ---------------------------------------

    fn named(mut d: SkillDef, name: &str) -> SkillDef {
        d.name = name.into();
        d
    }

    /// The record's kind field (s10): what rule 26 groups skills by (`0x140274990`).
    fn kind_of(mut d: SkillDef, kind: i16) -> SkillDef {
        d.raw[10] = kind;
        d
    }

    #[test]
    fn strength_rules_follow_the_notes() {
        use str_rule::*;
        let (mut c, _, _) = setup(100.0);
        let shell = |id, rule, str| def(id, Capsule::Attack, 12, 3, str, rule, &[(P, -30), (P + 1, 100)]);
        c.skills.extend([
            shell(257, TARGET_HEALTH_OVER_YOURS, 2),
            shell(320, YOUR_HEALTH_OVER_TARGETS, 1),
            shell(132, CONSUMED_ATTACKS, 1),
            shell(83, CONSUMED_DEFENCES, 1),
            shell(201, TARGET_EMPTY_BUTTONS, 2),
            shell(264, YOUR_EMPTY_BUTTONS, 2),
            shell(86, AURA_PARTICLES_ON_FIELD, 1),
            named(shell(78, BUNDLED, 2), "Bundled Laser"),
            kind_of(shell(900, 0, 3), 2),
            kind_of(shell(901, 0, 4), 3),
        ]);
        (c.fighters[0].health, c.fighters[1].health) = (10, 20);
        assert_eq!(c.damage(0, 257), 20, "Judge of Anubis: (20 - 10) x 2");
        assert_eq!(c.damage(0, 320), 0, "Joke of Anubis: only when your Health > target's");
        assert_eq!(c.damage(1, 320), 10);
        c.fighters[0].used = vec![BLASTER, WALL, LASER, 132];
        assert_eq!((c.damage(0, 132), c.damage(0, 83)), (3, 1), "consumed attacks / defences");
        c.fighters[1].slots = [Some(LASER), None, None, None];
        assert_eq!(c.damage(0, 201), 6, "double the target's 3 empty buttons");
        c.fighters[0].slots = [Some(900), Some(901), Some(264), None];
        assert_eq!(c.damage(0, 264), 2, "double your 1 empty button");
        assert_eq!(c.damage(0, 78), 3, "Bundled Laser: the equipped Lasers' STR only");
        c.capsules.clear();
        for k in 0..4 {
            c.capsules.push(CapsuleItem { id: 100 + k, owner: (k % 2) as usize, skill: if k < 3 { AURA_PARTICLE } else { LASER }, pos: Vec3::ZERO, appear: 0 });
        }
        assert_eq!(c.damage(0, 86), 3, "Overheat: every Aura Particle on the battlefield");
    }

    #[test]
    fn bundled_strength_sums_the_equipped_skills_of_the_same_kind_field() {
        // `0x140274990` (rule 26): over the four buttons, every occupied one whose s10 equals
        // this skill's s46 adds its own s46; `FUN_1401272e0` is "s46 if s43 = 0, else -1" for
        // both the kind and the addend. Not names, not schools: Octolaser (kind 2, but a lower-case
        // "laser" the old name test missed) and Laser Blade / Ki Palm (other kinds) differ here.
        use str_rule::*;
        let (mut c, _, _) = setup(100.0);
        let shell = |id, rule, str| def(id, Capsule::Attack, 12, 3, str, rule, &[(P, -30), (P + 1, 100)]);
        c.skills.extend([
            shell(78, BUNDLED, 2),
            // Kind 2 (the Lasers): Octolaser has no capital-L "Laser" in its name.
            kind_of(named(shell(910, 0, 1), "Octolaser"), 2),
            kind_of(named(shell(911, 0, 3), "Mist Laser"), 2),
            // Named "Laser" but another kind: the old test added it.
            kind_of(named(shell(912, 0, 6), "Laser Blade"), 0),
            // Dance of Death (kind 1): Ki Palm and Aura Leak are other kinds.
            shell(192, BUNDLED, 1),
            kind_of(shell(913, 0, 2), 1),
            kind_of(shell(914, 0, 4), 1),
            kind_of(named(shell(915, 0, 9), "Ki Palm"), 5),
            // A matching kind on a skill that is not an attack kind adds -1 (`FUN_1401272e0`).
            kind_of(def(916, Capsule::Status, 12, 3, 5, 0, &[(43, 4)]), 1),
        ]);
        c.fighters[0].slots = [Some(910), Some(911), Some(912), Some(78)];
        assert_eq!(c.bundled_strength(0, c.skill(78).unwrap()), 1 + 3, "Octolaser + Mist Laser; Laser Blade and the Bundled Laser itself are not kind 2");
        assert_eq!(c.damage(0, 78), 4);
        c.fighters[0].slots = [Some(913), Some(914), Some(915), Some(192)];
        assert_eq!(c.bundled_strength(0, c.skill(192).unwrap()), 2 + 4, "the two kind-1 attacks only");
        c.fighters[0].slots = [Some(913), Some(916), None, Some(192)];
        assert_eq!(c.bundled_strength(0, c.skill(192).unwrap()), 2 - 1, "a matching non-attack adds -1");
        c.fighters[0].slots = [None; 4];
        assert_eq!(c.bundled_strength(0, c.skill(192).unwrap()), 0, "nothing equipped, nothing added");
    }

    #[test]
    fn random_strength_is_zero_once_or_twice_and_replays() {
        let rolls = |seed_round: u32| {
            let (mut c, _, _) = setup(100.0);
            c.skills.push(def(321, Capsule::Attack, 10, 4, 3, str_rule::RANDOM, &[]));
            c.round = seed_round;
            (0..40).map(|_| c.damage(0, 321)).collect::<Vec<_>>()
        };
        let a = rolls(0);
        assert!(a.iter().all(|v| [0, 3, 6].contains(v)), "Excaliber: 0, 3 or 6");
        assert!([0, 3, 6].iter().all(|v| a.contains(v)), "all three come up");
        assert_eq!(a, rolls(0), "the sim's own generator: same every run");
    }

    #[test]
    fn holding_a_charge_skill_spends_aura_into_its_strength() {
        // Ki Cannon: "STR = Aura spent on skill. Hold button to charge." (cost 0, rule 21).
        const KI_CANNON: i16 = 204;
        let (mut c, mut p, col) = setup(100.0);
        c.skills.push(def(KI_CANNON, Capsule::Attack, 12, 0, 1, str_rule::AT_CAST, &[(37, 2), (P, -30), (P + 1, 100), (P + 4, 30)]));
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(KI_CANNON);
        // `FUN_14025c720`: no wind-up row here, so the count grows one per program
        // tick (2 sim ticks) while held; `count / 10` Aura joins the cost.
        const CHARGE_TICKS_PER_POINT: usize = 20; // 10 program ticks
        run(&mut c, &mut p, &col, [1, 0], 3 * CHARGE_TICKS_PER_POINT + 1);
        assert_eq!(c.fighters[0].charge.map(|ch| ch.spent), Some(3), "three points in");
        assert_eq!(c.fighters[0].aura(), 8, "nothing is paid while holding (`FUN_1402626d0` pays at the release)");
        assert!(c.projectiles.is_empty(), "still holding");
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 60);
        assert_eq!(c.fighters[1].health, START_HEALTH - 3, "STR = Aura spent");
        assert_eq!(c.fighters[0].aura(), 5, "3 points paid when it left");
    }

    /// `FUN_14025c720`: the charge count only starts after the row's wind-up
    /// (`charge_cycle`, w39), and stops growing where the Aura runs out.
    #[test]
    fn a_charge_counts_only_after_its_wind_up_and_only_as_far_as_aura_reaches() {
        const GUNGNIR: i16 = 73;
        let (mut c, mut p, col) = setup(100.0);
        c.skills.push(def(GUNGNIR, Capsule::Attack, 12, 0, 1, str_rule::AT_CAST, &[(37, 2), (P, -30), (P + 1, 100), (P + 4, 30)]));
        c.set_action_row(GUNGNIR, Some(ActionRow { anim_name: "ATK_SPIKE".into(), cast_sound: 0, charge_cycle: 13, dash_start: 0, dash_end: 0, fire_tick: 25, hit_window_end: 0 }), None);
        c.fighters[0].level = 3;
        c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(GUNGNIR);
        c.step(&mut p, [1, 0], &col);
        // 13 program ticks of wind-up = 26 sim ticks: nothing counts yet.
        for _ in 0..24 {
            c.step(&mut p, [1, 0], &col);
        }
        assert_eq!(c.fighters[0].charge.map(|ch| ch.count), Some(0), "still winding up");
        for _ in 0..200 {
            c.step(&mut p, [1, 0], &col);
        }
        assert_eq!(c.fighters[0].charge.map(|ch| (ch.spent, ch.count / 10)), Some((3, 3)), "capped at the 3 Aura in hand");
        assert!(c.fighters[0].charge.unwrap().count < 40, "the count stops where the next point can't be paid: {:?}", c.fighters[0].charge);
    }

    #[test]
    fn finisher_hits_for_double_the_level_it_had() {
        let (mut c, mut p, col) = setup(100.0);
        let mut finisher = def(FINISHER, Capsule::Attack, 12, 0, 2, str_rule::AT_CAST, &[(P, -30), (P + 1, 100), (P + 4, 30)]);
        finisher.level_penalty = -1;
        c.skills.push(finisher);
        cast_skill(&mut c, &mut p, &col, 0, FINISHER); // level 8
        assert_eq!(c.fighters[0].level, 8, "nothing is paid at the press");
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 60);
        assert_eq!(c.fighters[0].level, 0, "Penalty: Level reduced to 0 (at the fire tick)");
        assert_eq!(c.fighters[1].health, START_HEALTH - 16, "STR 16 from Level 8");
    }

    // ---- lane CASTRULES: cast state and the rules core ---------------------
    //
    // Tests from reads of the program (addresses in each), not from the old
    // behaviour: the cast gate, the aim captured at the press and the turn at
    // the character's rate, the air-cast hover, payment at the fire tick,
    // the Level gate, costs that kill, the drop's landing damage and the
    // status slots.
    mod cast_state {
        use super::*;
        use crate::sim::{BodyState, Dodge};
        use std::f32::consts::{FRAC_PI_2, PI};

        /// Laser as the table has it: s40 = 2 (castable on the ground or in the air).
        fn air_laser() -> SkillDef {
            def(LASER, Capsule::Attack, 15, 1, 1, 0, &[(f::MOTION, 6), (P, -80), (P + 1, 140), (P + 2, 8), (P + 3, 20), (P + 4, 20), (40, 2)])
        }

        fn fresh(dist: f32, extra: Vec<SkillDef>) -> (Combat, [Player; 2], Collision) {
            let mut sk = skills();
            sk.extend(extra);
            let mut c = Combat::new(sk, vec![AURA_PARTICLE; 30]);
            c.set_rules(TEST_RULES);
            c.round_ticks = COUNTDOWN_TICKS;
            let p = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, dist), PI)];
            (c, p, floor())
        }

        fn arm(c: &mut Combat, who: usize, slot: usize, skill: i16) {
            c.fighters[who].level = 8;
            c.fighters[who].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            c.fighters[who].slots[slot] = Some(skill);
        }

        /// `0x14024fce0`: s40 0 = states 0 / 1 / 8, 1 = 5 / 7, 2 and 3 = `0x1a3` (0, 1, 5, 7, 8);
        /// the ground dodge (3), jump wind-up (4) and landing (6) never.
        #[test]
        fn the_cast_gate_is_the_programs_truth_table() {
            let (c, _p, _col) = setup(100.0);
            for (s40, ground, air) in [(0, true, false), (1, false, true), (2, true, true), (3, true, true)] {
                assert_eq!((c.cast_allowed(s40, BodyState::Ground), c.cast_allowed(s40, BodyState::Air)), (ground, air), "s40 {s40}");
                assert!(!c.cast_allowed(s40, BodyState::Busy), "s40 {s40}: never mid-dodge, wind-up or landing");
            }
        }

        #[test]
        fn body_states_follow_the_movement_state() {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            assert_eq!(p.body_state(), BodyState::Ground);
            p.grounded = false;
            assert_eq!(p.body_state(), BodyState::Air);
            for busy in [
                |p: &mut Player| p.dodge_ticks = 5,
                |p: &mut Player| p.recover_ticks = 5,
                |p: &mut Player| p.land_timer = 0.2,
                |p: &mut Player| p.begin_jump(10, 100.0),
            ] {
                let mut p = Player::new(Vec3::ZERO, 0.0);
                busy(&mut p);
                assert_eq!(p.body_state(), BodyState::Busy);
            }
        }

        #[test]
        fn a_ground_skill_is_refused_in_the_air_silently_and_a_laser_is_not() {
            let (mut c, mut p, col) = fresh(300.0, vec![]);
            c.skills.retain(|d| d.id != LASER);
            c.skills.push(air_laser());
            c.set_action_row(LASER, Some(laser_row()), Some(laser_air_row()));
            p[0].grounded = false;
            p[0].pos.y = 30.0;
            arm(&mut c, 0, 0, BLASTER);
            arm(&mut c, 0, 1, LASER);
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].action, Action::Ready, "s40 = 0: ground states only");
            assert!(c.events.iter().all(|e| !matches!(e.1, CombatEvent::Denied { .. })), "the state gate is silent (`FUN_140262050` flag 4)");
            assert_eq!(c.fighters[0].aura(), 8);
            press(&mut c, &mut p, &col, 0, 1);
            assert_eq!(c.fighters[0].action, Action::Cast, "s40 = 2: castable in the air");
            assert!(c.fighters[0].air_cast);
        }

        /// A press in the ground dodge is refused and the dodge goes on (it used to
        /// start the cast and zero the dodge).
        #[test]
        fn a_press_during_a_dodge_is_refused_and_the_dodge_goes_on() {
            let mut col = floor();
            let mut combat = Combat::new(skills(), vec![AURA_PARTICLE; 30]);
            combat.set_rules(TEST_RULES);
            combat.round_ticks = COUNTDOWN_TICKS;
            let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 300.0)], combat);
            m.players[0].dodge = Dodge::Tumble;
            arm(&mut m.combat, 0, 0, BLASTER);
            m.step([Input { jump: true, ..Default::default() }, Input::default()], &mut col);
            let left = m.players[0].dodge_ticks;
            assert!(left > 0, "rolling");
            m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
            assert_eq!(m.combat.fighters[0].action, Action::Ready, "no cast");
            assert_eq!(m.players[0].dodge_ticks, left - 1, "the roll went on");
            assert_eq!(m.combat.fighters[0].aura(), 8);
        }

        #[test]
        fn the_aim_is_captured_at_the_press_from_the_lock_the_camera_or_nothing() {
            // `0x140258180`: a locked attack aims at the target's bearing (+X here = 90 deg), an
            // unlocked one and every defence at the camera's yaw, erase / status / special /
            // environment casts and mines leave the facing alone.
            let status = def(900, Capsule::Status, 13, 1, 0, 0, &[(43, 4)]);
            let (mut c, mut p, col) = fresh(100.0, vec![status]);
            p[1].pos = Vec3::new(100.0, 0.0, 0.0);
            p[0].cam_yaw = -1.0;
            for (skill, locked, want) in [
                (BLASTER, true, Aim::Yaw(FRAC_PI_2)),
                (BLASTER, false, Aim::Yaw(-1.0)),
                (WALL, true, Aim::Yaw(-1.0)),
                (900, true, Aim::Keep),
                (TIMER_MINE, true, Aim::Keep),
            ] {
                p[0].locked = locked;
                p[0].yaw = 0.0;
                c.fighters[0].action = Action::Ready;
                arm(&mut c, 0, 0, skill);
                press(&mut c, &mut p, &col, 0, 0);
                let aim = match c.fighters[0].aim {
                    Aim::Yaw(y) => Aim::Yaw(if (y - FRAC_PI_2).abs() < 1e-4 { FRAC_PI_2 } else { y }),
                    other => other,
                };
                assert_eq!(aim, want, "skill {skill}, locked {locked}");
            }
            // A short-range attack tracks a locked target inside the 80 u dash band, and
            // beyond it aims at the camera (`FUN_140258180`: target only in band 1).
            p[0].locked = true;
            p[1].pos = Vec3::new(60.0, 0.0, 0.0);
            c.fighters[0].action = Action::Ready;
            arm(&mut c, 0, 0, WAVE);
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].aim, Aim::Track(1));
            p[1].pos = Vec3::new(150.0, 0.0, 0.0);
            c.fighters[0].action = Action::Ready;
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].aim, Aim::Yaw(-1.0), "too far for the dash: the camera");
        }

        /// `0x14025bd40`: the facing moves toward the aim by at most 20 degrees per
        /// program tick = 10 per sim tick, on every tick of the cast: 180 deg takes 18.
        #[test]
        fn the_body_turns_ten_degrees_a_tick_toward_the_aim_and_no_snap() {
            let (mut c, mut p, col) = fresh(100.0, vec![]);
            p[0].locked = false;
            p[0].cam_yaw = PI; // the camera looks behind the caster
            arm(&mut c, 0, 0, BLASTER);
            press(&mut c, &mut p, &col, 0, 0); // press tick (nothing turns yet) + the first cast tick
            assert!((p[0].yaw - 10f32.to_radians()).abs() < 1e-4, "one tick = 10 deg, not a snap: {}", p[0].yaw.to_degrees());
            run(&mut c, &mut p, &col, [0; 2], 8);
            assert!((p[0].yaw - FRAC_PI_2).abs() < 1e-3, "9 ticks = 90 deg: {}", p[0].yaw.to_degrees());
            run(&mut c, &mut p, &col, [0; 2], 9);
            assert!((p[0].yaw - PI).abs() < 1e-3, "18 ticks = 180 deg (9 program ticks, 0.3 s): {}", p[0].yaw.to_degrees());
            run(&mut c, &mut p, &col, [0; 2], 3);
            assert!((p[0].yaw - PI).abs() < 1e-3, "no overshoot");
        }

        /// Unlocked, the shot goes where the camera looks (the aim point 190 u ahead,
        /// `FUN_14026e410`) and nothing homes on the foe; locked, it hits as before.
        #[test]
        fn an_unlocked_shot_leaves_along_the_camera_and_does_not_home() {
            let (mut c, mut p, col) = fresh(100.0, vec![]);
            p[0].locked = false;
            p[0].cam_yaw = FRAC_PI_2; // looking at +X, the foe is at +Z
            arm(&mut c, 0, 0, BLASTER);
            press(&mut c, &mut p, &col, 0, 0);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize);
            let shot = c.projectiles.first().expect("fired").shot;
            assert!(shot.heading().x > 10.0 * shot.heading().z.abs(), "along +X: {:?}", shot.heading());
            assert_eq!(shot.homing, 0.0, "no target, no homing");
            run(&mut c, &mut p, &col, [0; 2], 150);
            assert_eq!(c.fighters[1].health, START_HEALTH, "it went where the camera looked");
            // The same cast locked on: aimed at the foe, homing kept, it lands.
            let (mut c, mut p, col) = fresh(100.0, vec![]);
            arm(&mut c, 0, 0, BLASTER);
            press(&mut c, &mut p, &col, 0, 0);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
            assert_eq!(c.fighters[1].health, START_HEALTH - 4);
        }

        /// State 11 (`0x14025a3e0`: vertical speed := 0, `0x140254590`: no vertical
        /// step while the state lasts): an air cast hangs the body until it ends.
        #[test]
        fn a_cast_in_mid_air_hangs_the_body_until_it_ends_then_it_falls() {
            let mut col = floor();
            let mut sk = skills();
            sk.retain(|d| d.id != LASER);
            sk.push(air_laser());
            let mut combat = Combat::new(sk, vec![AURA_PARTICLE; 30]);
            combat.set_rules(TEST_RULES);
            combat.round_ticks = COUNTDOWN_TICKS;
            combat.set_action_row(LASER, Some(laser_row()), Some(laser_air_row()));
            let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 300.0)], combat);
            arm(&mut m.combat, 0, 0, LASER);
            m.players[0].pos.y = 60.0;
            m.players[0].grounded = false;
            m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
            assert_eq!(m.combat.fighters[0].action, Action::Cast);
            assert!(m.combat.fighters[0].air_cast);
            let y0 = m.players[0].pos.y;
            let mut held = 0;
            while m.combat.fighters[0].action == Action::Cast {
                m.step([Input::default(), Input::default()], &mut col);
                if m.combat.fighters[0].action == Action::Cast {
                    assert!((m.players[0].pos.y - y0).abs() < 1e-4, "hanging at tick {held}: {} vs {y0}", m.players[0].pos.y);
                    held += 1;
                }
                assert!(held < 300);
            }
            assert!(held > 30, "the whole clip: {held} ticks");
            for _ in 0..20 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            assert!(m.players[0].pos.y < y0 - 5.0, "falls once it ends: {}", m.players[0].pos.y);
        }

        /// `FUN_14025be40`: in the dash the body steers toward the target by at most
        /// s66 / 10 degrees a program tick (here 30 -> 3 deg = 1.5 deg a sim tick) and moves
        /// along its own facing.
        #[test]
        fn a_dashing_attack_steers_at_most_its_s66_tenth_degrees_a_program_tick() {
            let id = 960;
            let dash = def(id, Capsule::Attack, 10, 1, 1, 0, &[(f::MOTION, 13), (P, 130), (P + 1, 8), (P + 2, 30)]);
            let (mut c, mut p, col) = fresh(60.0, vec![dash]);
            c.set_action_row(id, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 0, charge_cycle: 0, dash_start: 10, dash_end: 20, fire_tick: 20, hit_window_end: 22 }), None);
            arm(&mut c, 0, 0, id);
            press(&mut c, &mut p, &col, 0, 0);
            // Wind-up (20 sim ticks): the body comes round to the target at the turn rate.
            run(&mut c, &mut p, &col, [0; 2], 19);
            // Mid-dash the target jumps to the other side: the body can only follow slowly.
            p[1].pos = Vec3::new(-50.0, 0.0, 0.0);
            let mut last = p[0].yaw;
            let mut biggest = 0.0f32;
            for _ in 0..16 {
                run(&mut c, &mut p, &col, [0; 2], 1);
                let step = (p[0].yaw - last).abs();
                biggest = biggest.max(step);
                last = p[0].yaw;
            }
            let cap = 1.5f32.to_radians();
            assert!(biggest <= cap + 1e-4, "steered {} deg in one tick, cap 1.5", biggest.to_degrees());
            assert!(biggest > cap * 0.5, "and it does steer: {}", biggest.to_degrees());
        }

        // ---- payment at the fire tick ---------------------------------------

        fn costly() -> SkillDef {
            // A straight shell: cost 1, Level penalty 1, Health cost 1, two uses.
            let mut d = def(950, Capsule::Attack, 12, 1, 2, 0, &[(P, -30), (P + 1, 140), (P + 4, 0), (f::USES, 2)]);
            d.level_penalty = 1;
            d.health_cost = 1;
            d
        }

        /// `FUN_1402626d0` pays (`FUN_1402621f0`) when the object is created: a hit
        /// before the fire tick cancels the cast and it costs nothing; left alone it pays
        /// Aura, Level, Health and the use together, at the fire tick.
        #[test]
        fn nothing_is_paid_at_the_press_and_an_interrupted_cast_costs_nothing() {
            let (mut c, mut p, col) = fresh(300.0, vec![costly()]);
            c.fighters[0].level = 3;
            c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
            c.fighters[0].slots[0] = Some(950);
            press(&mut c, &mut p, &col, 0, 0);
            let state = |c: &Combat| (c.fighters[0].aura(), c.fighters[0].level, c.fighters[0].health, c.fighters[0].spent[0]);
            assert_eq!(state(&c), (3, 3, 20, 0), "the press pays nothing");
            c.hurt(0, 1, false, Vec3::Z); // hit before the fire tick
            run(&mut c, &mut p, &col, [0; 2], 60);
            assert_eq!(state(&c), (3, 3, 19, 0), "only the hit's damage: no Aura, Level, Health or use spent");
            assert_eq!(c.fighters[0].slots[0], Some(950));
            assert!(c.projectiles.is_empty());
            // Uninterrupted: everything is paid together when it leaves.
            press(&mut c, &mut p, &col, 0, 0);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize - 4);
            assert_eq!(state(&c), (3, 3, 19, 0), "still winding up");
            run(&mut c, &mut p, &col, [0; 2], 6);
            assert_eq!(state(&c), (2, 2, 18, 1), "Aura 1, Level 1, Health 1 and one use, at the fire tick");
            assert_eq!(c.projectiles.len(), 1);
        }

        /// `FUN_1402621f0`: Level >= the Level penalty (s23) to cast at all.
        #[test]
        fn the_level_penalty_needs_that_much_level() {
            let mut d = def(951, Capsule::Attack, 12, 0, 2, 0, &[(P, -30), (P + 1, 140), (P + 4, 0)]);
            d.level_penalty = 2;
            let (mut c, mut p, col) = fresh(300.0, vec![d]);
            c.fighters[0].level = 1;
            c.fighters[0].aura_ticks = AURA_TICKS_PER_POINT;
            c.fighters[0].slots[0] = Some(951);
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].action, Action::Ready, "Level 1 < 2: no");
            assert!(c.events.iter().any(|e| e.1 == CombatEvent::Denied { who: 0, slot: 0 }), "it says no");
            assert_eq!(c.fighters[0].denied_ticks[0], 0, "no cost flash: it is the Level, not the Aura");
            c.fighters[0].level = 2;
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].action, Action::Cast);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 2);
            assert_eq!(c.fighters[0].level, 0, "the penalty is taken at the fire tick");
        }

        /// `FUN_14024d7a0` kills at Health < 1 whatever took it: a Health cost (s25) is
        /// the killing blow and the round goes to the other player.
        #[test]
        fn a_health_cost_can_be_the_killing_blow() {
            let mut d = def(952, Capsule::Attack, 12, 0, 2, 0, &[(P, -30), (P + 1, 140), (P + 4, 0)]);
            d.health_cost = 3;
            let (mut c, mut p, col) = fresh(300.0, vec![d]);
            c.fighters[0].health = 2;
            c.fighters[0].slots[0] = Some(952);
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].health, 2, "not yet");
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 2);
            assert_eq!((c.fighters[0].health, c.fighters[0].action), (0, Action::Down));
            assert_eq!(c.round_over.map(|r| r.0), Some(1), "player 1 takes the round");
        }

        #[test]
        fn poison_hurts_a_capture_at_the_press_and_a_cast_at_its_fire_tick() {
            let (mut c, mut p, col) = fresh(300.0, vec![]);
            c.fighters[0].poison_ticks = 600;
            c.capsules.push(CapsuleItem { id: 77, owner: 0, skill: BLASTER, pos: p[0].pos, appear: 0 });
            press(&mut c, &mut p, &col, 0, 2);
            assert_eq!((c.fighters[0].slots[2], c.fighters[0].health), (Some(BLASTER), START_HEALTH - 1), "a capture pays it at the press");
            arm(&mut c, 0, 0, BLASTER);
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].health, START_HEALTH - 1, "a cast has not paid it yet");
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 2);
            assert_eq!(c.fighters[0].health, START_HEALTH - 2, "`FUN_1402626d0` pays it at the fire tick");
        }

        #[test]
        fn level_stops_at_99() {
            let (mut c, mut p, col) = fresh(300.0, vec![]);
            c.fighters[0].level = 98;
            c.fighters[0].slots = [Some(AURA_PARTICLE); 4];
            for k in 0..2 {
                press(&mut c, &mut p, &col, 0, k);
                run(&mut c, &mut p, &col, [0; 2], 80);
            }
            assert_eq!(c.fighters[0].level, 99, "98 + 1 + 1 stops at the cap of 99 (`FUN_14024d970`)");
        }

        /// A capture needs a ground state (`0x1402517c0`: states 0 / 1 / 8): in the air the
        /// same press casts the skill, mid-dodge it does neither.
        #[test]
        fn capturing_needs_the_ground_and_a_dodge_blocks_it() {
            let mut sk_air = air_laser();
            sk_air.id = 965;
            let (mut c, mut p, col) = fresh(300.0, vec![sk_air]);
            c.set_action_row(965, Some(laser_row()), Some(laser_air_row()));
            c.capsules.push(CapsuleItem { id: 78, owner: 0, skill: WALL, pos: p[0].pos, appear: 0 });
            arm(&mut c, 0, 0, 965);
            p[0].grounded = false;
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].slots[0], Some(965), "not captured in the air");
            assert_eq!(c.fighters[0].action, Action::Cast, "the press cast the skill instead");
            let (mut c, mut p, col) = fresh(300.0, vec![]);
            c.capsules.push(CapsuleItem { id: 79, owner: 0, skill: WALL, pos: p[0].pos, appear: 0 });
            p[0].dodge_ticks = 10;
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.capsules.len(), 1, "a dodge takes no capture");
            p[0].dodge_ticks = 0;
            press(&mut c, &mut p, &col, 0, 0);
            assert!(c.capsules.is_empty(), "on the ground it does");
        }

        // ---- the drop's landing damage (`FUN_140254c80`, state 17) ---------------

        fn knocked_from(y0: f32, health: i32) -> Combat {
            let (mut c, mut p, col) = setup(150.0);
            c.set_reaction_row(77, ReactionRow { kind: reaction_kind::KNOCKDOWN, launch_speed: 1, launch_angle: 50, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
            p[1].pos.y = y0;
            p[1].grounded = false;
            c.fighters[1].health = health;
            c.fighters[1].reaction_row = 77;
            c.hurt(1, 0, true, Vec3::Z);
            assert!(c.fighters[1].flying);
            for _ in 0..400 {
                if !c.fighters[1].flying {
                    break;
                }
                run(&mut c, &mut p, &col, [0; 2], 1);
            }
            assert!(!c.fighters[1].flying, "landed");
            c
        }

        #[test]
        fn landing_from_a_knock_down_costs_zero_one_or_two_health_by_the_fall() {
            // < 40 u: nothing; 40-80 u: 1; > 80 u: 2 (constants 0x1402f7ff0 = 40, 0x1402f8040 = 80).
            for (y0, cost) in [(20.0, 0), (39.0, 0), (50.0, 1), (75.0, 1), (100.0, 2), (130.0, 2)] {
                let c = knocked_from(y0, START_HEALTH);
                assert_eq!(START_HEALTH - c.fighters[1].health, cost, "fell from {y0}");
            }
            let c = knocked_from(100.0, 2);
            assert_eq!((c.fighters[1].health, c.fighters[1].action), (0, Action::Down), "a long drop can be the killing blow");
        }

        // ---- status slots (`FUN_14024d5c0`, caps `0x1402dcf30`) ------------------

        #[test]
        fn status_slots_hold_one_permanent_value_and_one_timed_amount_within_the_caps() {
            use crate::moves::effects::{Stat, Status};
            let mut st = Status::default();
            st.add(Stat::Attack, 2, Some(100));
            st.add(Stat::Attack, 2, Some(200));
            assert_eq!(st.total(Stat::Attack), 2, "a timed amount is replaced, not stacked: Tiger's Strength twice is +2");
            assert_eq!(st.mods.len(), 1);
            assert_eq!(st.mods[0].ticks, Some(200), "and its timer restarts");
            st.add(Stat::Attack, 5, None);
            st.add(Stat::Attack, 7, None);
            assert_eq!(st.total(Stat::Attack), 9, "permanent 5 + 7 stops at the cap of 9 (timed 2 on top is still within it)");
            st.add(Stat::Speed, -70, None);
            st.add(Stat::Speed, -70, None);
            assert_eq!(st.total(Stat::Speed), -90, "speed caps at 90 either way");
            st.add(Stat::Speed, 500, Some(10));
            assert_eq!(st.total(Stat::Speed), 90, "-90 permanent + 500 timed is capped at +90 (the program clamps the sum it reads)");
            for _ in 0..10 {
                st.tick();
            }
            assert_eq!(st.total(Stat::Speed), -90, "the timed part expires");
        }

        #[test]
        fn the_environment_joins_the_status_under_the_same_cap() {
            let (mut c, _p, _col) = fresh(100.0, vec![]);
            c.fighters[0].status.add(Stat::AuraRate, 80, None);
            c.crystal = None;
            assert_eq!(c.stat(0, Stat::AuraRate), 80);
            c.fighters[0].status.add(Stat::AuraRate, 500, None);
            assert_eq!(c.stat(0, Stat::AuraRate), 90);
        }
    }

    // ---- lane CASTRULES phase 2: the original's rule table (F2) -------------
    /// Lane REACTIONS: the program's hit-reaction state machine
    /// (`FUN_140258690` and the state handlers `0x14025c1a0` / `0x14025c260` /
    /// `0x14025c3d0`, landing `0x140254c80` / `0x140259990`, wall hit
    /// `0x140259b30`, fall-out `0x140251d80`, KO `0x14024ec80`).
    mod reactions {
        use super::*;
        use std::f32::consts::{FRAC_PI_2, PI};

        fn id(name: &str) -> i16 {
            pd_formats::anim::ANIM_NAMES.iter().position(|x| *x == name).unwrap() as i16
        }

        fn knock_row(c: &mut Combat, row: i16, speed: i16, front: &str, back: &str) {
            c.set_reaction_row(row, ReactionRow { kind: reaction_kind::KNOCKDOWN, launch_speed: speed, launch_angle: 50, w: [0, id(front), 0, 0, 0, id(back), 0, 0], ..Default::default() });
        }

        fn collapse_row(c: &mut Combat, row: i16, clip: &str, pose: &str) {
            c.set_reaction_row(row, ReactionRow { kind: reaction_kind::CLIP_THEN_LIE, w: [id(clip), id(pose), 0, 0, 0, 0, 0, 0], ..Default::default() });
        }

        fn clips(c: &mut Combat, table: &[(&str, u16)]) {
            let t: HashMap<String, u16> = table.iter().map(|(n, v)| (n.to_string(), *v)).collect();
            c.set_clip_ticks([t.clone(), t]);
        }

        #[test]
        fn the_programs_clip_sets_and_landing_tables() {
            // `0x1c1ce00000000000 >> id` (`0x140259990`): BLOW0-2_FRONT 45-47, BLOW_LAND_LF/MF/SF
            // 50-52, WALLHIT_BACK / _LOOP 58 / 59, DOWN_FRONT 60.
            for n in [45, 46, 47, 50, 51, 52, 58, 59, 60] {
                assert!(program_front_set(n), "{n}");
            }
            for n in [48, 49, 53, 54, 55, 56, 57, 61, 62, 63] {
                assert!(!program_front_set(n), "{n}");
            }
            // `0x34 - class` front, `0x37 - class` back.
            let name = |n: usize| pd_formats::anim::ANIM_NAMES[n];
            assert_eq!([0, 1, 2].map(|c| name(landing_clip_id(true, c))), ["BLOW_LAND_SF", "BLOW_LAND_MF", "BLOW_LAND_LF"]);
            assert_eq!([0, 1, 2].map(|c| name(landing_clip_id(false, c))), ["BLOW_LAND_SB", "BLOW_LAND_MB", "BLOW_LAND_LB"]);
            // The queued DOWN pose from the charaset flags 1 1 0 | 1 1 0 (records 0-10).
            assert_eq!([0, 1, 2].map(|c| name(lying_pose_id(true, c))), ["DOWN_FRONT", "DOWN_BACK", "DOWN_BACK"]);
            assert_eq!([0, 1, 2].map(|c| name(lying_pose_id(false, c))), ["DOWN_BACK", "DOWN_FRONT", "DOWN_FRONT"]);
            // Only the back-set large landing turns half way (charaset 0 0 0 180 0 0).
            assert_eq!([(true, 2), (false, 0), (false, 1), (false, 2)].map(|(f, c)| landing_flips(f, c)), [false, false, false, true]);
        }

        #[test]
        fn two_way_reactions_split_at_ninety_degrees_not_at_the_four_way_sectors() {
            // `0x140258a60`: front when the hit bearing is within 90 deg of the facing.
            let at = |deg: f32| Vec3::new(deg.to_radians().sin(), 0.0, deg.to_radians().cos()) * 10.0;
            for (deg, front) in [(0.0, true), (60.0, true), (-80.0, true), (100.0, false), (-120.0, false), (180.0, false)] {
                assert_eq!(hit_front(Vec3::ZERO, 0.0, at(deg)), front, "{deg}");
            }
            // 60 deg off is a side sector in the four-way split but still the front half.
            assert_ne!(hit_direction(Vec3::ZERO, 0.0, at(60.0)), HitDir::Front);
        }

        #[test]
        fn the_programs_right_sector_is_the_side_its_clip_recoils_away_from() {
            // `DMG_SHOT_RIGHT` is the clip in four-way slot 1 and its root drifts toward +X
            // (`side` +11.3, pc00a.rootmotion.json): the attacker is on the -X side at yaw 0.
            assert_eq!(hit_direction(Vec3::ZERO, 0.0, Vec3::new(-10.0, 0.0, 1.0)), HitDir::Right);
            assert_eq!(hit_direction(Vec3::ZERO, 0.0, Vec3::new(10.0, 0.0, 1.0)), HitDir::Left);
        }

        #[test]
        fn a_stagger_lasts_the_clips_length_and_every_hit_restarts_it() {
            // State 14's timer is the clip length (`FUN_140259d10(0xe, len)`).
            let (mut c, _p, _col) = setup(100.0);
            c.set_reaction_row(2, ReactionRow { kind: reaction_kind::FOUR_WAY, w: [id("DMG_SHOT_FRONT"), id("DMG_SHOT_RIGHT"), id("DMG_SHOT_BACK"), id("DMG_SHOT_LEFT"), 0, 0, 0, 0], ..Default::default() });
            c.fighters[1].reaction_row = 2;
            c.hurt(1, 1, false, Vec3::Z);
            assert_eq!((c.fighters[1].action, c.fighters[1].action_ticks), (Action::Hit, HIT_STUN_TICKS), "no clip table: the flat guess");
            clips(&mut c, &[("DMG_SHOT_FRONT", 40)]);
            let seq = c.fighters[1].hit_seq;
            c.hurt(1, 1, false, Vec3::Z);
            assert_eq!(c.fighters[1].action_ticks, 40, "20 frames = 40 sim ticks");
            assert_eq!(c.fighters[1].hit_seq, seq + 1, "a second hit with the same clip counts as a new reaction");
        }

        #[test]
        fn a_second_reaction_chains_behind_the_first() {
            // `0x14025c1a0`: when the stagger's timer runs out `FUN_140258690(queued row, -1, facing)`.
            let (mut c, mut p, col) = setup(100.0);
            c.set_reaction_row(2, ReactionRow { kind: reaction_kind::CLIP, w: [id("DMG_SHOT_FRONT"), 0, 0, 0, 0, 0, 0, 0], ..Default::default() });
            c.set_reaction_row(5, ReactionRow { kind: reaction_kind::CLIP, w: [id("DMG_STATUS_UP"), 0, 0, 0, 0, 0, 0, 0], ..Default::default() });
            c.fighters[1].reaction_row = 2;
            c.hurt(1, 1, false, Vec3::Z);
            c.fighters[1].queued_reaction = 5;
            let seq = c.fighters[1].hit_seq;
            run(&mut c, &mut p, &col, [0; 2], HIT_STUN_TICKS as usize - 1);
            assert_eq!((c.fighters[1].reaction_row, c.fighters[1].action), (2, Action::Hit), "the first is still playing");
            run(&mut c, &mut p, &col, [0; 2], 1);
            assert_eq!((c.fighters[1].reaction_row, c.fighters[1].action, c.fighters[1].hit_seq), (5, Action::Hit, seq + 1), "then the second");
            assert_eq!(c.reaction_clip(1), "DMG_STATUS_UP");
        }

        #[test]
        fn a_collapse_is_clip_then_lie_then_get_up_about_three_point_seven_seconds() {
            // Kind 1 (rows 9 / 11 / 13 / 25): state 15 for the 50-frame DMG_DRAIN_HIGH (+1 tick), state 18
            // with timer 0x1e (31 ticks), then the 30-frame get-up.
            let (mut c, mut p, col) = setup(100.0);
            collapse_row(&mut c, 9, "DMG_DRAIN_HIGH", "DOWN_BACK");
            clips(&mut c, &[("DMG_DRAIN_HIGH", 100)]);
            c.fighters[1].reaction_row = 9;
            c.hurt(1, 1, false, Vec3::Z);
            let total = 102 + COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS;
            assert_eq!((c.fighters[1].action, c.fighters[1].action_ticks), (Action::Knocked, total));
            assert!(total >= 220 && total <= 230, "~3.7 s: {total}");
            assert!(c.fighters[1].collapsing());
            assert_eq!(c.reaction_clip(1), "DMG_DRAIN_HIGH");
            // Not touchable by an ordinary attack the whole time.
            let health = c.fighters[1].health;
            cast_skill(&mut c, &mut p, &col, 0, RAIL);
            run(&mut c, &mut p, &col, [0; 2], 60);
            assert_eq!(c.fighters[1].health, health, "helpless but untouchable");
            // Collapse over -> lying in the pose w42 names, then its get-up.
            while c.fighters[1].collapsing() {
                run(&mut c, &mut p, &col, [0; 2], 1);
            }
            assert_eq!(c.fighters[1].action_ticks, COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS);
            assert_eq!((c.lying_clip(1), c.getup_clip(1)), ("DOWN_BACK", "DOWN_BACK_STAND"));
            run(&mut c, &mut p, &col, [0; 2], COLLAPSE_LIE_TICKS as usize);
            assert_eq!(c.fighters[1].invuln_ticks, POST_GETUP_INVULN_TICKS, "protected from the first get-up tick");
            run(&mut c, &mut p, &col, [0; 2], KNOCK_GETUP_TICKS as usize);
            assert_eq!(c.fighters[1].action, Action::Ready);
        }

        #[test]
        fn a_collapse_lies_in_the_pose_its_row_names() {
            let (mut c, _p, _col) = setup(100.0);
            collapse_row(&mut c, 13, "DMG_RAISE_HIGH", "DOWN_FRONT");
            c.fighters[1].reaction_row = 13;
            c.hurt(1, 1, false, Vec3::Z);
            c.fighters[1].action_ticks = COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS;
            assert_eq!((c.lying_clip(1), c.getup_clip(1)), ("DOWN_FRONT", "DOWN_FRONT_STAND"));
        }

        /// Launches player 1 from (50, 0, 0) away from an attacker at -X and returns the match.
        fn launch(front_hit: bool, y0: f32) -> Match {
            let mut col = testkit::floor();
            let combat = Combat::new(vec![], vec![AURA_PARTICLE; 30]);
            let mut m = Match::new([Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0)], combat);
            knock_row(&mut m.combat, 77, 20, "BLOW0_FRONT", "BLOW0_BACK");
            for _ in 0..200 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            m.players[1].pos.y = y0;
            m.players[1].grounded = false;
            m.combat.fighters[1].reaction_row = 77;
            m.combat.fighters[1].hit_front = front_hit;
            m.combat.hurt(1, 1, true, Vec3::X);
            m.step([Input::default(), Input::default()], &mut col);
            m
        }

        #[test]
        fn a_knock_down_turns_the_body_to_face_the_attacker_or_away_from_it() {
            // `0x140258dfd`: the front set faces the bearing to the attacker, the back set that + 180 deg.
            // The attacker is at -X of player 1: bearing = -pi/2.
            let front = launch(true, 0.0);
            assert!((front.players[1].yaw - -FRAC_PI_2).abs() < 1e-4, "hit from the front: faces the attacker, {}", front.players[1].yaw);
            let back = launch(false, 0.0);
            assert!((back.players[1].yaw - FRAC_PI_2).abs() < 1e-4, "hit from behind: back to the attacker, {}", back.players[1].yaw);
            assert_eq!(back.combat.reaction_clip(1), "BLOW0_BACK");
            // The slide goes away from the attacker either way.
            assert!(front.combat.fighters[1].knock_vel.x > 0.0 && back.combat.fighters[1].knock_vel.x > 0.0);
        }

        #[test]
        fn the_flight_is_a_real_arc_and_the_landing_clip_follows_the_fall_height() {
            // An ordinary hit rises ~8 u (launch 20 at 50 deg, gravity 0.12 u/tick^2): fall height < 40 -> the
            // small landing, not the medium one. (Before, the floor-stick step glued the body to y ~ 0.7.)
            let mut m = launch(true, 0.0);
            let mut col = testkit::floor();
            let mut peak: f32 = 0.0;
            for _ in 0..400 {
                peak = peak.max(m.players[1].pos.y);
                if !m.combat.fighters[1].flying {
                    break;
                }
                m.step([Input::default(), Input::default()], &mut col);
            }
            assert!(peak > 6.0 && peak < 10.0, "apex {peak}");
            m.step([Input::default(), Input::default()], &mut col);
            let f = &m.combat.fighters[1];
            assert_eq!(f.land_class, 0);
            assert_eq!(m.combat.lying_clip(1), "BLOW_LAND_SF");
            assert_eq!(m.players[1].motion, crate::sim::Motion::Floored);
            assert!(m.players[1].pos.y.abs() < 1e-3 && m.players[1].grounded, "down on the floor, no landing lock stacked on top");
            assert_eq!(m.players[1].land_timer, 0.0);
        }

        #[test]
        fn landing_class_by_fall_height_picks_the_clip_then_the_down_loop_and_its_get_up() {
            for (y0, class, land, lying, getup, hit_front) in [
                (20.0, 0u8, "BLOW_LAND_SF", "DOWN_FRONT", "DOWN_FRONT_STAND", true),
                (60.0, 1, "BLOW_LAND_MF", "DOWN_BACK", "DOWN_BACK_STAND", true),
                (110.0, 2, "BLOW_LAND_LF", "DOWN_BACK", "DOWN_BACK_STAND", true),
                (20.0, 0, "BLOW_LAND_SB", "DOWN_BACK", "DOWN_BACK_STAND", false),
                (60.0, 1, "BLOW_LAND_MB", "DOWN_FRONT", "DOWN_FRONT_STAND", false),
            ] {
                let (mut c, mut p, col) = setup(150.0);
                knock_row(&mut c, 77, 1, "BLOW0_FRONT", "BLOW0_BACK");
                clips(&mut c, &[("BLOW_LAND_SF", 44), ("BLOW_LAND_MF", 80), ("BLOW_LAND_LF", 112), ("BLOW_LAND_SB", 44), ("BLOW_LAND_MB", 70)]);
                p[1].pos.y = y0;
                p[1].grounded = false;
                c.fighters[1].health = 20;
                c.fighters[1].reaction_row = 77;
                c.fighters[1].hit_front = hit_front;
                c.hurt(1, 0, true, Vec3::Z);
                while c.fighters[1].flying {
                    run(&mut c, &mut p, &col, [0; 2], 1);
                }
                assert_eq!(c.fighters[1].land_class, class, "from {y0}");
                assert_eq!(c.lying_clip(1), land, "from {y0}");
                let len = c.clip_hold_ticks(1, land);
                run(&mut c, &mut p, &col, [0; 2], len as usize);
                assert_eq!((c.lying_clip(1), c.getup_clip(1)), (lying, getup), "from {y0}: the DOWN loop queued behind the landing clip");
            }
        }

        #[test]
        fn the_big_back_landing_turns_the_body_when_the_down_clip_starts() {
            let (mut c, mut p, col) = setup(150.0);
            knock_row(&mut c, 77, 1, "BLOW0_FRONT", "BLOW0_BACK");
            clips(&mut c, &[("BLOW_LAND_LB", 60)]);
            p[1].pos.y = 110.0;
            p[1].grounded = false;
            c.fighters[1].reaction_row = 77;
            c.fighters[1].hit_front = false;
            c.hurt(1, 0, true, Vec3::Z);
            while c.fighters[1].flying {
                run(&mut c, &mut p, &col, [0; 2], 1);
            }
            assert_eq!(c.lying_clip(1), "BLOW_LAND_LB");
            let yaw = p[1].yaw;
            run(&mut c, &mut p, &col, [0; 2], 58);
            assert_eq!(p[1].yaw, yaw, "not while the landing clip plays");
            run(&mut c, &mut p, &col, [0; 2], 4);
            assert!((wrap(p[1].yaw - yaw).abs() - PI).abs() < 1e-3, "half a turn: {} -> {}", yaw, p[1].yaw);
            fn wrap(a: f32) -> f32 {
                (a + PI).rem_euclid(2.0 * PI) - PI
            }
        }

        #[test]
        fn the_ground_slide_steps_at_thirty_hertz_losing_fifteen_percent() {
            // `0x14025c260`: speed x 0.85 per program tick (every second sim tick).
            let (mut c, mut p, _col) = setup(100.0);
            c.fighters[1].action = Action::Knocked;
            c.fighters[1].action_ticks = 100;
            c.fighters[1].knock_vel = Vec3::new(30.0, 0.0, 0.0);
            let x0 = p[1].pos.x;
            c.knock_slide(1, &mut p);
            assert!((p[1].pos.x - x0 - 30.0 * 0.85 / 30.0).abs() < 1e-4);
            c.knock_slide(1, &mut p);
            assert!((p[1].pos.x - x0 - 30.0 * 0.85 / 30.0).abs() < 1e-4, "nothing on the odd tick");
            c.knock_slide(1, &mut p);
            assert!((p[1].pos.x - x0 - (30.0 * 0.85 + 30.0 * 0.85 * 0.85) / 30.0).abs() < 1e-4);
            for _ in 0..200 {
                c.knock_slide(1, &mut p);
            }
            assert_eq!(c.fighters[1].knock_vel, Vec3::ZERO, "stops under 0.02 u per tick");
        }

        /// A floor and a tall wall across x = `x`.
        fn walled(x: f32) -> Collision {
            let s = 1000.0;
            Collision::new(vec![
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
                [Vec3::new(x, 0.0, -200.0), Vec3::new(x, 400.0, -200.0), Vec3::new(x, 400.0, 200.0)],
                [Vec3::new(x, 0.0, -200.0), Vec3::new(x, 400.0, 200.0), Vec3::new(x, 0.0, 200.0)],
            ])
        }

        #[test]
        fn a_flyer_bounces_off_a_wall_at_half_speed_playing_wallhit() {
            // `0x140259b30`: speed x 0.5 along the wall's normal, WALLHIT_FRONT then its loop, at most three
            // times and not within 20 program ticks of the start or the last one.
            let (mut c, mut p, _col) = setup(150.0);
            let col = walled(100.0);
            knock_row(&mut c, 77, 60, "BLOW0_FRONT", "BLOW0_BACK");
            clips(&mut c, &[("WALLHIT_FRONT", 36)]);
            p[1].pos = Vec3::new(0.0, 0.0, 0.0);
            c.fighters[1].reaction_row = 77;
            c.hurt(1, 1, true, Vec3::X);
            let horizontal = c.fighters[1].knock_vel.x;
            let mut ticks = 0;
            while c.fighters[1].wall_hit.is_none() {
                run(&mut c, &mut p, &col, [0; 2], 1);
                ticks += 1;
                assert!(ticks < 300, "never hit the wall");
            }
            assert!(ticks > WALL_COOLDOWN_TICKS, "not before the 20-program-tick cool-down: {ticks}");
            let f = &c.fighters[1];
            assert_eq!(f.wall_hits_left, WALL_HITS - 1);
            assert!((f.knock_vel.x - -horizontal * WALL_SPEED_KEEP).abs() < 1.0, "half the speed, back the way it came: {} vs {}", f.knock_vel.x, horizontal);
            assert_eq!(c.reaction_clip(1), "WALLHIT_FRONT");
            run(&mut c, &mut p, &col, [0; 2], 37);
            assert_eq!(c.reaction_clip(1), "WALLHIT_FRONT_LOOP");
            assert!(p[1].pos.x < 100.0, "stayed on this side of the wall");
        }

        #[test]
        fn a_flyer_cannot_land_while_the_wallhit_clip_plays_and_a_stuck_one_is_put_down() {
            // `0x140254590` tests the clip id; `0x14025c260` puts down a flyer that hung for 90 program ticks.
            let (mut c, mut p, col) = setup(150.0);
            clips(&mut c, &[("WALLHIT_FRONT", 2000)]);
            c.fighters[1].action = Action::Knocked;
            c.fighters[1].action_ticks = KNOCKDOWN_TICKS;
            c.fighters[1].flying = true;
            c.fighters[1].wall_hit = Some((56, 0));
            run(&mut c, &mut p, &col, [0; 2], 100);
            assert!(c.fighters[1].flying, "on the floor but still in flight");
            run(&mut c, &mut p, &col, [0; 2], STUCK_FLIGHT_TICKS as usize);
            assert!(!c.fighters[1].flying, "put down after 90 program ticks hung at one height");
        }

        #[test]
        fn falling_out_leaves_no_reaction_faces_the_start_yaw_and_protects_for_ninety_program_ticks() {
            // `0x140251d80`: facing := start yaw, idle, Health -3 silently, protect timer 0x5a.
            let mut col = testkit::floor();
            let combat = Combat::new(vec![], vec![AURA_PARTICLE; 30]);
            let mut m = Match::new([Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0)], combat);
            for _ in 0..200 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            m.combat.fighters[1].reaction_row = 59; // a stale heavy row must not matter
            m.players[1].yaw = 2.0;
            m.players[1].pos.y = m.players[1].spawn.y - crate::sim::FALL_RESPAWN_BELOW - 1.0;
            m.step([Input::default(), Input::default()], &mut col);
            let (p, f) = (&m.players[1], &m.combat.fighters[1]);
            assert_eq!(p.pos, p.spawn);
            assert!((p.yaw - p.spawn_yaw).abs() < 1e-5 && p.spawn_yaw != 2.0, "facing the start yaw: {} vs {}", p.yaw, p.spawn_yaw);
            assert_eq!((f.action, f.health), (Action::Ready, START_HEALTH - FALL_OUT_DAMAGE), "no stagger");
            assert!(f.invuln_ticks >= FALL_OUT_PROTECT_TICKS - 2, "protected: {}", f.invuln_ticks);
            // And a fall that kills lies DOWN_FRONT for good.
            m.combat.fighters[1].health = 2;
            m.combat.fighters[1].invuln_ticks = 0;
            m.players[1].pos.y = m.players[1].spawn.y - crate::sim::FALL_RESPAWN_BELOW - 1.0;
            m.step([Input::default(), Input::default()], &mut col);
            assert_eq!(m.combat.fighters[1].action, Action::Down);
            assert_eq!(m.combat.lying_clip(1), "DOWN_FRONT");
            assert_eq!(m.players[1].motion, crate::sim::Motion::Floored);
        }

        #[test]
        fn a_knock_out_plays_the_killing_blows_reaction_and_never_gets_up() {
            // `0x14025c3d0` dead branch -> state 27: the reaction (here a launch) flies, lands and lies for good.
            let mut col = testkit::floor();
            let combat = Combat::new(vec![], vec![AURA_PARTICLE; 30]);
            let mut m = Match::new([Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0)], combat);
            knock_row(&mut m.combat, 77, 20, "BLOW0_FRONT", "BLOW0_BACK");
            for _ in 0..200 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            m.combat.fighters[1].health = 1;
            m.combat.fighters[1].reaction_row = 77;
            m.combat.hurt(1, 1, true, Vec3::X);
            assert_eq!(m.combat.fighters[1].action, Action::Down, "out at once: the round ends");
            assert!(m.combat.round_over.is_some());
            m.step([Input::default(), Input::default()], &mut col);
            assert_eq!(m.players[1].motion, crate::sim::Motion::Blown, "but the body still flies");
            assert!(m.players[1].pos.y > 0.5);
            for _ in 0..170 {
                m.step([Input::default(), Input::default()], &mut col);
            }
            assert_eq!(m.players[1].motion, crate::sim::Motion::Floored);
            assert!(m.combat.fighters[1].land_class == 0 && m.players[1].pos.y.abs() < 1e-3);
            assert_eq!(m.combat.lying_clip(1), "DOWN_FRONT", "the small front landing lies DOWN_FRONT");
        }

        #[test]
        fn a_knock_out_by_a_stagger_plays_it_then_collapses_with_row_25() {
            let (mut c, mut p, col) = setup(100.0);
            c.set_reaction_row(2, ReactionRow { kind: reaction_kind::CLIP, w: [id("DMG_SHOT_FRONT"), 0, 0, 0, 0, 0, 0, 0], ..Default::default() });
            collapse_row(&mut c, KO_COLLAPSE_ROW, "DMG_DRAIN_HIGH", "DOWN_BACK");
            clips(&mut c, &[("DMG_SHOT_FRONT", 40), ("DMG_DRAIN_HIGH", 100)]);
            c.fighters[1].health = 1;
            c.fighters[1].reaction_row = 2;
            c.hurt(1, 1, false, Vec3::Z);
            assert!(c.fighters[1].action == Action::Down && c.fighters[1].ko_staggering());
            assert_eq!(c.reaction_clip(1), "DMG_SHOT_FRONT");
            run(&mut c, &mut p, &col, [0; 2], 41);
            assert_eq!(c.fighters[1].reaction_row, KO_COLLAPSE_ROW);
            assert!(c.fighters[1].collapsing() && !c.fighters[1].ko_staggering());
            run(&mut c, &mut p, &col, [0; 2], 120);
            assert_eq!((c.fighters[1].action, c.lying_clip(1)), (Action::Down, "DOWN_BACK"), "never gets up");
        }

        #[test]
        fn dying_on_your_feet_collapses_with_row_25() {
            // `0x14024ee75`: idle and dead -> `FUN_140258690(0x19)`.
            let (mut c, mut p, col) = setup(100.0);
            collapse_row(&mut c, KO_COLLAPSE_ROW, "DMG_DRAIN_HIGH", "DOWN_BACK");
            c.change_health(1, -99);
            assert_eq!((c.fighters[1].action, c.fighters[1].reaction_row), (Action::Down, KO_COLLAPSE_ROW));
            assert!(c.fighters[1].collapsing());
            run(&mut c, &mut p, &col, [0; 2], 160);
            assert_eq!(c.fighters[1].action, Action::Down);
            assert_eq!(c.lying_clip(1), "DOWN_BACK");
        }
    }

    mod rule_set {
        use super::*;

        fn with_rule(rule: usize, start_level: i32, roster: Roster) -> Combat {
            let mut c = Combat::with_roster(skills(), roster, std::array::from_fn(|_| vec![AURA_PARTICLE; 30]));
            c.set_rules(Rules { start_level, start_hand: 0, set: RuleSet::new(rule), ..Rules::default() });
            c
        }

        /// gsdata 0x33040 (`FUN_14024e790` -> `FUN_14024d020`): every fighter
        /// starts with the row's Health, at the setup Level kept inside the row's
        /// Level range, Aura full.
        #[test]
        fn every_rule_row_gives_its_health_and_level_range() {
            for (rule, health, (lo, hi)) in [(0, 20, (0, 99)), (1, 20, (0, 99)), (3, 10, (0, 5)), (4, 40, (5, 99)), (6, 20, (0, 99)), (10, 5, (0, 99))] {
                let c = with_rule(rule, 0, Roster::duel());
                for f in &c.fighters[..2] {
                    assert_eq!((f.health, f.max_health, f.level_min, f.level_max), (health, health, lo, hi), "rule {rule}");
                    assert_eq!((f.level, f.aura()), (lo, lo), "rule {rule}: starts at the range's low end, Aura full");
                }
            }
            let c = with_rule(3, 99, Roster::duel());
            assert_eq!(c.fighters[0].level, 5, "Puny: a higher setup Level is cut to 5");
        }

        /// Rule 11 (Annihilate the Odd One): player 0 is built from row 12.
        #[test]
        fn the_odd_one_is_built_from_the_host_row() {
            let c = with_rule(11, 0, Roster::free_for_all(3));
            assert_eq!((c.fighters[0].health, c.fighters[1].health, c.fighters[2].health), (40, 10, 10));
        }

        #[test]
        fn the_time_limit_is_the_rows_minutes_in_sim_ticks() {
            for (rule, minutes) in [(0, 10), (4, 10), (10, 3), (11, 10)] {
                assert_eq!(RuleSet::new(rule).time_limit_ticks(), minutes * 3600, "rule {rule}");
            }
            let mut c = with_rule(10, 0, Roster::duel());
            c.use_rule_time_limit();
            assert_eq!(c.time_limit, 3 * 3600, "KO Battle: 3 minutes");
            assert_eq!(TIME_LIMIT_TICKS, 10 * 3600, "the default is the original's 10 minutes");
        }

        #[test]
        fn level_stays_inside_the_rules_range_and_heals_stop_at_its_health() {
            let mut c = with_rule(3, 0, Roster::duel());
            c.fighters[0].level = 5;
            c.fighters[0].slots[0] = Some(AURA_PARTICLE);
            let (mut p, col) = ([Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, 300.0), std::f32::consts::PI)], floor());
            c.round_ticks = COUNTDOWN_TICKS;
            press(&mut c, &mut p, &col, 0, 0);
            assert_eq!(c.fighters[0].level, 5, "Puny: Level 5 is the most");
            c.fighters[1].health = 4;
            c.change_health(1, 50);
            assert_eq!(c.fighters[1].health, 10, "Puny: Health 10 is the most");
            let mut c = with_rule(4, 0, Roster::duel());
            c.change_health(0, -30);
            assert_eq!(c.fighters[0].health, 10);
            c.change_health(0, 99);
            assert_eq!(c.fighters[0].health, 40, "Heavy: up to 40");
        }

        /// The row table agrees with the real file when it is there.
        #[test]
        fn the_built_in_table_is_the_games_own() {
            let path = std::path::Path::new("../../../game_files/Assets/Data/gstorage/gsdata_en.dat");
            let Ok(d) = std::fs::read(path) else {
                eprintln!("skipping the_built_in_table_is_the_games_own: no gsdata");
                return;
            };
            for (k, r) in pd_formats::skills::rule_table(&d).unwrap().iter().enumerate() {
                assert_eq!((RULE_TABLE[k].health, RULE_TABLE[k].level_min, RULE_TABLE[k].level_max, RULE_TABLE[k].minutes), (r.health, r.level_min, r.level_max, r.minutes), "row {k}");
            }
            let h = pd_formats::skills::aura_handicaps(&d).unwrap();
            assert_eq!([1, 2, 3].map(|k| crate::arsenal::aura_handicap(k)), h);
        }

        /// A declared "Number of Schools" sets the multiplier whatever the contents are.
        #[test]
        fn the_declared_number_of_schools_sets_the_aura_multiplier() {
            let mut c = with_rule(0, 0, Roster::duel());
            c.set_arsenal_kinds([Some(1), Some(3), None, None]);
            assert_eq!((c.fighters[0].aura_handicap, c.fighters[1].aura_handicap), (1.3, 0.7));
            c.set_arsenal_kinds([Some(2), Some(5), None, None]);
            assert_eq!((c.fighters[0].aura_handicap, c.fighters[1].aura_handicap), (1.0, 1.0), "kind 4 and 5 are x1.0");
        }

        /// `FUN_14012c510` / `FUN_14012c440`: knock-outs within 60 program ticks (120
        /// sim ticks) of the first leave nobody standing together: a draw.
        #[test]
        fn a_second_knock_out_inside_the_window_makes_a_draw() {
            for (gap, draw) in [(60, true), (SIMULTANEOUS_KO_TICKS as usize, true), (SIMULTANEOUS_KO_TICKS as usize + 20, false)] {
                let (mut c, mut p, col) = setup(300.0);
                c.hurt(1, 99, false, Vec3::Z);
                assert_eq!(c.round_over.map(|r| r.0), Some(0));
                assert_eq!(c.wins, [1, 0, 0, 0]);
                run(&mut c, &mut p, &col, [0; 2], gap);
                c.hurt(0, 99, false, Vec3::Z);
                if draw {
                    assert_eq!((c.round_over.map(|r| r.0), c.wins), (Some(DRAW), [0; 4]), "{gap} ticks apart");
                } else {
                    assert_eq!((c.round_over.map(|r| r.0), c.wins), (Some(0), [1, 0, 0, 0]), "{gap} ticks apart: the first still wins");
                }
            }
        }

        /// A tie on Health at time-up scores nobody.
        #[test]
        fn a_tie_at_time_up_is_a_draw() {
            let (mut c, mut p, col) = setup(300.0);
            c.time_limit = 600;
            run(&mut c, &mut p, &col, [0; 2], 620);
            assert_eq!(c.round_over.map(|r| r.0), Some(DRAW));
            assert!(c.time_up);
            assert_eq!(c.wins, [0; 4]);
        }
    }

    // ---- four players (lane S1b/S1d) ------------------------------------
    /// The projectile and area families to the program's numbers (lane
    /// PROJECTILES; docs/MOVES.md "Verdicts"). Each test names the program
    /// address its rule was read from. One program tick = two sim ticks.
    mod projectiles {
        use super::testkit as k;
        use super::*;

        fn health(c: &Combat, i: usize) -> i32 {
            c.fighters[i].health
        }

        /// Blaster -30 / 140 (`FUN_1402636f0`): a range of 300 u, 23 program ticks. A target
        /// 400 u away is never hit (the shell used to fly 900 u); one at 250 u is.
        #[test]
        fn a_shell_ends_at_its_range() {
            let (mut c, mut p, col) = setup(400.0);
            cast_skill(&mut c, &mut p, &col, 0, BLASTER);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
            assert_eq!(health(&c, 1), START_HEALTH, "out of range");
            assert!(c.projectiles.is_empty(), "the shell has ended");
            let (mut c, mut p, col) = setup(250.0);
            cast_skill(&mut c, &mut p, &col, 0, BLASTER);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
            assert_eq!(health(&c, 1), START_HEALTH - 4, "in range");
        }

        fn laser44(id: i16) -> SkillDef {
            k::def(id, Capsule::Attack, 1, &[(f::FAMILY, 15), (f::STRENGTH, 1), (f::MOTION, 6), (k::P, -80), (k::P + 1, 140), (k::P + 2, 8), (k::P + 3, 20), (k::P + 4, 20), (f::RANGE_BAND, 1)])
        }

        /// Laser, s44 = 1 (`FUN_140273d40`, `FUN_14026e410`): the aim point is pulled out to 300 u
        /// and the target dropped, so a target 150 u away is passed by the arc; at 400 u (inside the
        /// 300-600 band) the circle closes on it and the shot re-aims every tick.
        #[test]
        fn a_laser_cannot_hit_a_target_closer_than_300_u() {
            let (mut c, mut p, col) = k::setup(vec![laser44(65)], 150.0);
            k::use_skill(&mut c, &mut p, &col, 0, 65);
            k::run(&mut c, &mut p, &col, 260);
            assert_eq!(health(&c, 1), START_HEALTH, "150 u: the arc closes at 300 u and goes by");
            let (mut c, mut p, col) = k::setup(vec![laser44(65)], 400.0);
            k::use_skill(&mut c, &mut p, &col, 0, 65);
            k::run(&mut c, &mut p, &col, 260);
            assert_eq!(health(&c, 1), START_HEALTH - 1, "400 u: it arrives");
        }

        /// A bomb that hits a wall or the floor blasts there, and the blast hurts
        /// *everyone* in its sphere including its thrower (`FUN_14026dbd0` with no target sets all
        /// four hit bits; DECISIONS 2026-10-01 "self-damage from mines/bombs faithful").
        #[test]
        fn a_bomb_on_a_wall_hurts_its_thrower() {
            let s = 1000.0;
            let col = Collision::new(vec![
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
                [Vec3::new(-s, 0.0, 14.0), Vec3::new(s, 0.0, 14.0), Vec3::new(s, 80.0, 14.0)],
                [Vec3::new(-s, 0.0, 14.0), Vec3::new(s, 80.0, 14.0), Vec3::new(-s, 80.0, 14.0)],
            ]);
            let (mut c, mut p, _) = setup(200.0);
            cast_skill(&mut c, &mut p, &col, 0, BOMB);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
            assert!(health(&c, 0) < START_HEALTH, "the thrower is in the blast: {}", health(&c, 0));
            assert_eq!(health(&c, 1), START_HEALTH, "the target is far away");
        }

        /// A bomb that lands on its target hurts it once: the direct hit, and the blast skips the
        /// target (`FUN_14026dbd0(obj, target)` clears the target's bit); the thrower is not in it.
        #[test]
        fn a_bomb_on_a_body_skips_that_body_in_the_blast() {
            let (mut c, mut p, col) = setup(100.0);
            cast_skill(&mut c, &mut p, &col, 0, BOMB);
            run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
            assert_eq!((health(&c, 0), health(&c, 1)), (START_HEALTH, START_HEALTH - 3));
        }

        /// A mine's blast hurts the person who set it (`FUN_14026cd70` code 8 -> `FUN_14026dbd0(obj, 0)`).
        #[test]
        fn a_mine_hurts_its_own_owner() {
            let (mut c, mut p, col) = setup(300.0);
            cast_and_release(&mut c, &mut p, &col, TIMER_MINE);
            run(&mut c, &mut p, &col, [0; 2], 2 * 150 + 80);
            assert_eq!(health(&c, 0), START_HEALTH - 3, "the owner stood on it");
            assert_eq!(health(&c, 1), START_HEALTH);
        }

        /// A Sensor Mine is set off by anyone, its owner included, once it is armed (60 ticks):
        /// `FUN_140267e90` tests every collider with the mine's own hit bits all set.
        #[test]
        fn a_sensor_mine_is_set_off_by_its_owner() {
            let (mut c, mut p, col) = setup(300.0);
            cast_and_release(&mut c, &mut p, &col, SENSOR_MINE);
            run(&mut c, &mut p, &col, [0; 2], 2 * 55);
            assert_eq!(health(&c, 0), START_HEALTH, "not armed yet");
            run(&mut c, &mut p, &col, [0; 2], 80);
            assert!(health(&c, 0) < START_HEALTH, "armed, and its owner is standing on it");
        }

        fn tremor(motion: i16) -> SkillDef {
            k::def(147, Capsule::Attack, 3, &[(f::FAMILY, 75), (f::STRENGTH, 3), (f::MOTION, motion), (k::P, 30), (k::P + 1, 10), (k::P + 2, 300)])
        }

        /// Hole (`FUN_14026a7f0`): 30 ticks of warning, then 10 active; the sphere only takes
        /// players standing on the ground, so a jumper is untouched.
        #[test]
        fn a_hole_only_takes_those_on_the_ground() {
            for (grounded, expect) in [(true, START_HEALTH - 3), (false, START_HEALTH)] {
                let (mut c, mut p, col) = k::setup(vec![tremor(15)], 100.0);
                k::use_skill(&mut c, &mut p, &col, 0, 147);
                let a = c.areas.first().expect("set under the target");
                assert!(a.warning());
                // 30 warning ticks (60 sim ticks) from the cast, then hold the target off the floor.
                k::run(&mut c, &mut p, &col, 50);
                p[1].grounded = grounded;
                k::run(&mut c, &mut p, &col, 60);
                assert_eq!(health(&c, 1), expect, "grounded {grounded}");
            }
        }

        /// Hole motion 16 makes one under every player, the caster included: `FUN_1402626d0`
        /// loops over every living player with no owner test (`0x1402627xx`, the `uVar11 < 4`
        /// loop calling `FUN_14026ede0`), and the LIVE takes agree -- Tremor (147), Earthquake
        /// (312), Jack Pot (319), Overheat (86) hurt P0 and P1 by the same amount on the same
        /// tick, Massive Quake (148) 6 each, Calamity (283) floors both. (PROGRAM-RE.md's
        /// "the caster's own hole can't hurt the caster" was wrong.)
        #[test]
        fn the_unlimited_hole_is_placed_under_every_player_the_caster_included() {
            let (mut c, mut p, col) = k::setup(vec![tremor(16)], 100.0);
            k::use_skill(&mut c, &mut p, &col, 0, 147);
            assert_eq!(c.areas.len(), 2);
            assert!(c.areas.iter().any(|a| a.pos.distance(p[1].pos) < 1.0), "under the foe");
            assert!(c.areas.iter().any(|a| a.pos.distance(p[0].pos) < 1.0), "and under the caster");
            k::run(&mut c, &mut p, &col, 120);
            assert_eq!(health(&c, 1), START_HEALTH - 3);
            assert_eq!(health(&c, 0), START_HEALTH - 3, "LIVE take 147: P0 20 -> 17 with P1 40 -> 37");
        }

        /// The ordinary Hole (motion 15) is a single hole at the target: it does not reach the caster.
        #[test]
        fn the_aimed_hole_only_takes_the_one_it_sits_under() {
            let (mut c, mut p, col) = k::setup(vec![tremor(15)], 100.0);
            k::use_skill(&mut c, &mut p, &col, 0, 147);
            assert_eq!(c.areas.len(), 1);
            k::run(&mut c, &mut p, &col, 120);
            assert_eq!(health(&c, 0), START_HEALTH);
        }

        /// Rain column (Lightning Bolt 30 / 10 / 6 / 0 / 300; `FUN_140267410`): warning, then a
        /// vertical column of 30 u radius for 10 ticks; a roof over its first active tick ends it.
        #[test]
        fn a_rain_column_strikes_in_its_active_window_and_a_roof_ends_it() {
            let bolt = || k::def(142, Capsule::Attack, 3, &[(f::FAMILY, 21), (f::STRENGTH, 3), (f::MOTION, 7), (k::P, 30), (k::P + 1, 10), (k::P + 2, 6), (k::P + 4, 300)]);
            let (mut c, mut p, col) = k::setup(vec![bolt()], 100.0);
            k::use_skill(&mut c, &mut p, &col, 0, 142);
            k::run(&mut c, &mut p, &col, 40);
            assert_eq!(health(&c, 1), START_HEALTH, "still warning");
            k::run(&mut c, &mut p, &col, 60);
            assert_eq!(health(&c, 1), START_HEALTH - 3);
            // A roof 30 u over the target.
            let s = 1000.0;
            let roofed = Collision::new(vec![
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
                [Vec3::new(-80.0, 30.0, 40.0), Vec3::new(80.0, 30.0, 40.0), Vec3::new(80.0, 30.0, 160.0)],
                [Vec3::new(-80.0, 30.0, 40.0), Vec3::new(80.0, 30.0, 160.0), Vec3::new(-80.0, 30.0, 160.0)],
            ]);
            let (mut c, mut p, _) = k::setup(vec![bolt()], 100.0);
            k::use_skill(&mut c, &mut p, &roofed, 0, 142);
            k::run(&mut c, &mut p, &roofed, 100);
            assert_eq!(health(&c, 1), START_HEALTH, "the roof ends the column");
        }

        /// The Tornado (`FUN_140268940`): it can hit its own caster after its first 30 ticks. A
        /// stationary one (speed 0) sits on the person who made it.
        #[test]
        fn a_tornado_hits_its_caster_after_thirty_ticks() {
            let twister = k::def(140, Capsule::Attack, 3, &[(f::FAMILY, 18), (f::STRENGTH, 2), (f::MOTION, 10), (k::P, 90), (k::P + 1, 100), (k::P + 2, 10)]);
            let (mut c, mut p, col) = k::setup(vec![twister], 300.0);
            k::use_skill(&mut c, &mut p, &col, 0, 140);
            assert_eq!(health(&c, 0), START_HEALTH, "safe for its first 30 ticks");
            k::run(&mut c, &mut p, &col, 2 * 30);
            assert!(health(&c, 0) < START_HEALTH, "then it takes its own caster: {}", health(&c, 0));
        }

        /// The Tornado never homes: it goes down the caster's facing even with the target off to the side.
        #[test]
        fn a_tornado_goes_straight_and_misses_a_target_to_the_side() {
            let twister = k::def(140, Capsule::Attack, 3, &[(f::FAMILY, 18), (f::STRENGTH, 2), (f::MOTION, 10), (k::P, 90), (k::P + 1, 100), (k::P + 2, 10), (k::P + 3, 100), (k::P + 4, 0), (k::P + 5, 60)]);
            let (mut c, mut p, col) = k::setup(vec![twister], 100.0);
            k::use_skill(&mut c, &mut p, &col, 0, 140);
            p[1].pos.x = 60.0; // side-steps as it leaves: it does not follow
            k::run(&mut c, &mut p, &col, 200);
            assert_eq!(health(&c, 1), START_HEALTH);
            let (mut c, mut p, col) = k::setup(vec![k::def(140, Capsule::Attack, 3, &[(f::FAMILY, 18), (f::STRENGTH, 2), (f::MOTION, 10), (k::P, 90), (k::P + 1, 100), (k::P + 2, 10), (k::P + 3, 100), (k::P + 4, 0), (k::P + 5, 60)])], 100.0);
            k::use_skill(&mut c, &mut p, &col, 0, 140);
            k::run(&mut c, &mut p, &col, 200);
            assert_eq!(health(&c, 1), START_HEALTH - 2, "straight down the line it does hit");
        }

        /// A Ricochet laser never ends on a wall (it reflects); Diffused Laser, the other motion-17
        /// record, ramps its speed s65 -> s66 over s67 ticks and is built the same way now.
        #[test]
        fn motion_17_with_a_ramp_is_a_ricochet_too() {
            let mut diffused = k::def(720, Capsule::Attack, 1, &[(f::MOTION, 17), (k::P, -120), (k::P + 1, 20), (k::P + 2, 100), (k::P + 3, 120), (f::STRENGTH, 1)]);
            diffused.s70_73 = [1, 0, 0, 3];
            assert!(matches!(moves::behaviour_of(&diffused), Behaviour::Shoot(moves::ShotKind::Ricochet(_))));
            let r = moves::ricochet::Ricochet::from_def(&diffused);
            assert_eq!((r.ramp, r.count(), r.bounces), (120, 1, 3));
            assert_eq!(r.life, moves::life_ticks(-120, 20, 100, 120), "-120 = 1200 u along the ramp");
        }

        /// The dash is (10 / window) x s65 u a program tick, so a longer window is a slower dash
        /// to the same place; it stops 11 u from the target's centre (`FUN_14025be40`).
        #[test]
        fn a_dash_stops_eleven_units_from_the_target() {
            let row = |len: i16| ActionRow { anim_name: "ATK_KICK".into(), cast_sound: 0, charge_cycle: 0, dash_start: 10, dash_end: 10 + len, fire_tick: 30, hit_window_end: 32 };
            for len in [8, 12] {
                let (mut c, mut p, col) = setup(70.0);
                c.set_action_row(WAVE, Some(row(len)), None);
                (c.fighters[0].level, c.fighters[0].aura_ticks) = (4, 4 * AURA_TICKS_PER_POINT);
                c.fighters[0].slots[0] = Some(WAVE);
                press(&mut c, &mut p, &col, 0, 0);
                run(&mut c, &mut p, &col, [0; 2], 120);
                let gap = p[1].pos.z - p[0].pos.z;
                assert!((5.0..=13.0).contains(&gap), "window {len}: ended {gap} u from the target");
            }
        }
    }

    mod four_player {
        use super::*;

        fn spread() -> [Vec3; 4] {
            [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 300.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(400.0, 0.0, 400.0)]
        }

        /// `roster` on a flat floor, countdown over, every active slot given
        /// an all-Aura-Particle arsenal (no powerless drain) and the spots in `at`.
        fn setup4(roster: Roster, at: [Vec3; 4]) -> (Combat, [Player; 4], Collision) {
            let mut c = Combat::with_roster(skills(), roster, std::array::from_fn(|_| vec![AURA_PARTICLE; 30]));
            c.set_rules(TEST_RULES);
            c.round_ticks = COUNTDOWN_TICKS;
            let players = std::array::from_fn(|i| {
                let toward_origin = if i == 0 { 0.0 } else { std::f32::consts::PI };
                Player::new(at[i], toward_origin)
            });
            (c, players, floor())
        }

        fn run4(c: &mut Combat, p: &mut [Player; 4], col: &Collision, ticks: usize) {
            for _ in 0..ticks {
                c.step(p, [0u8; 4], col);
            }
        }

        /// Player `who` gets Level/Aura 8 and `skill` on button 0, presses and releases it.
        fn cast4(c: &mut Combat, p: &mut [Player; 4], col: &Collision, who: usize, skill: i16) {
            c.fighters[who].level = 8;
            c.fighters[who].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            c.fighters[who].slots[0] = Some(skill);
            let mut b = [0u8; 4];
            b[who] = 1;
            c.step(p, b, col);
            c.step(p, [0u8; 4], col);
        }

        #[test]
        fn four_player_a_duel_leaves_slots_two_and_three_inert() {
            let mut col = floor();
            let mut m = Match::new([Vec3::new(-150.0, 0.0, 0.0), Vec3::new(150.0, 0.0, 0.0)], Combat::new(skills(), vec![AURA_PARTICLE; 30]));
            let pristine = m.players[2].clone();
            let rng_before = m.combat.rng;
            for t in 0..700 {
                let run = Input { stick: [0, 127], skills: if t % 7 == 0 { 1 } else { 0 }, ..Default::default() };
                m.step([run, run], &mut col);
            }
            for i in 2..4 {
                assert_eq!(m.players[i].pos, Vec3::ZERO, "slot {i} never moves");
                assert_eq!(m.players[i].pos, pristine.pos);
                assert_eq!(m.combat.fighters[i].health, START_HEALTH);
                assert_eq!(m.combat.fighters[i].action, Action::Ready);
                assert!(m.combat.fighters[i].arsenal.is_empty(), "an inactive slot is never dealt a hand");
            }
            assert_ne!(m.combat.rng, rng_before, "the active slots did draw");
            assert!(m.combat.events.iter().all(|(_, e)| match e {
                CombatEvent::Hit { attacker, victim, .. } | CombatEvent::Blocked { attacker, victim, .. } => *attacker < 2 && *victim < 2,
                CombatEvent::Burst { attacker, .. } => *attacker < 2,
                CombatEvent::Denied { who, .. } | CombatEvent::Capsule { who, .. } => *who < 2,
                CombatEvent::CapsuleAppeared { who, .. }
                | CombatEvent::CapsuleErased { who, .. }
                | CombatEvent::CrystalSet { who, .. }
                | CombatEvent::CrystalTaken { who, .. }
                | CombatEvent::AuraTick { who }
                | CombatEvent::KnockLanded { who }
                | CombatEvent::GotUp { who }
                | CombatEvent::SkillFired { who, .. }
                | CombatEvent::Knockout { who } => *who < 2,
                CombatEvent::BlastStarted { attacker, .. } | CombatEvent::ButtonErased { attacker, .. } | CombatEvent::RainDrop { attacker, .. } => *attacker < 2,
                CombatEvent::StatusEffect { who, .. } | CombatEvent::AuraParticleUsed { who } | CombatEvent::ChargeHeld { who, .. } | CombatEvent::ChargeReleased { who, .. } => *who < 2,
                CombatEvent::JumpTakeoff { who } | CombatEvent::SkillCaptured { who, .. } | CombatEvent::AddEffect { who, .. } | CombatEvent::Landed { who, .. } => *who < 2,
                CombatEvent::EnvironmentEnded { .. } | CombatEvent::ShotExpired { .. } => true,
            }));
        }

        #[test]
        fn four_player_all_four_slots_tick_spawn_and_get_a_hand_each() {
            let mut col = floor();
            let spawns = [Vec3::new(-100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, 100.0), Vec3::new(-100.0, 0.0, 100.0)];
            let mut c = Combat::with_roster(skills(), Roster::free_for_all(4), std::array::from_fn(|i| vec![[WAVE, LASER, BLASTER, LANCE][i]; 30]));
            c.set_rules(Rules { start_hand: 4, ..TEST_RULES });
            let mut m = Match::with_spawns(&spawns, c);
            for i in 0..4 {
                assert_eq!(m.players[i].spawn, spawns[i], "slot {i} starts at its own point");
                assert!(m.combat.fighters[i].slots.iter().all(Option::is_some), "slot {i} was dealt its hand");
            }
            // Facing: everyone turns toward their first enemy (slots 1, 0, 0, 0).
            for (i, foe) in [1usize, 0, 0, 0].into_iter().enumerate() {
                let want = yaw_of(toward(spawns[i], spawns[foe]));
                assert!((m.players[i].yaw - want).abs() < 1e-4, "slot {i} faces slot {foe}");
            }
            let walk = Input { stick: [0, 127], ..Default::default() };
            let before: Vec<Vec3> = m.players.iter().map(|p| p.pos).collect();
            for _ in 0..(COUNTDOWN_TICKS + 60) {
                m.step([walk; 4], &mut col);
            }
            for i in 0..4 {
                assert!(m.players[i].pos.distance(before[i]) > 10.0, "slot {i} walked");
            }
        }

        #[test]
        fn four_player_overlapping_bodies_are_pushed_apart_pairwise() {
            let mut col = floor();
            let here = Vec3::new(0.0, 0.0, 0.0);
            let spawns = [here, here + Vec3::new(1.0, 0.0, 0.0), here + Vec3::new(0.0, 0.0, 1.0), here + Vec3::new(1.0, 0.0, 1.0)];
            let mut m = Match::with_spawns(&spawns, Combat::with_roster(skills(), Roster::free_for_all(4), std::array::from_fn(|_| vec![AURA_PARTICLE; 30])));
            for _ in 0..40 {
                m.step([Input::default(); 4], &mut col);
            }
            for a in 0..4 {
                for b in a + 1..4 {
                    let d = m.players[a].pos.distance(m.players[b].pos);
                    assert!(d > crate::body::Body::DEFAULT.radius() * 1.6, "slots {a} and {b} still overlap: {d}");
                }
            }
        }

        #[test]
        fn four_player_a_shot_hits_the_first_body_in_its_way_not_just_its_target() {
            // Player 0 aims at player 1 (their first enemy) 300 away; player 2 stands in the line 100 away.
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), spread());
            cast4(&mut c, &mut p, &col, 0, BLASTER);
            run4(&mut c, &mut p, &col, 120);
            assert_eq!(c.fighters[2].health, START_HEALTH - 4, "the body in the way takes the hit");
            assert_eq!(c.fighters[1].health, START_HEALTH, "the aimed-at player is shielded by it");
            assert!(c.events.iter().any(|e| e.1 == CombatEvent::Hit { attacker: 0, victim: 2, skill: BLASTER }), "{:?}", c.events);
        }

        #[test]
        fn four_player_friendly_fire_policy_decides_whether_a_partner_takes_the_hit() {
            // 2v2, slots 0+1 against 2+3: player 1 (partner) stands in player 0's line to player 2.
            let at = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 300.0), Vec3::new(400.0, 0.0, 400.0)];
            for (policy, partner_hp, enemy_hp) in [(FriendlyFire::On, START_HEALTH - 4, START_HEALTH), (FriendlyFire::Off, START_HEALTH, START_HEALTH - 4)] {
                let (mut c, mut p, col) = setup4(Roster::with_teams(&[0, 0, 1, 1]), at);
                c.friendly_fire = policy;
                assert_eq!(c.foe_of(0), 2, "the aim is an enemy, never the partner");
                cast4(&mut c, &mut p, &col, 0, BLASTER);
                run4(&mut c, &mut p, &col, 160);
                assert_eq!((c.fighters[1].health, c.fighters[2].health), (partner_hp, enemy_hp), "{policy:?}");
            }
        }

        #[test]
        fn four_player_a_blast_area_reaches_every_body_in_radius() {
            // Photon Wave goes off around the caster (radius reaches ~30 u).
            let at = [Vec3::ZERO, Vec3::new(20.0, 0.0, 0.0), Vec3::new(-20.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 400.0)];
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), at);
            cast4(&mut c, &mut p, &col, 0, PHOTON_WAVE);
            run4(&mut c, &mut p, &col, CAST_TICKS as usize + 40);
            assert_eq!(c.fighters[1].health, START_HEALTH - 4);
            assert_eq!(c.fighters[2].health, START_HEALTH - 4);
            assert_eq!(c.fighters[3].health, START_HEALTH, "out of range");
            assert_eq!(c.fighters[0].health, START_HEALTH, "never the caster");
            let hits: Vec<(usize, usize)> = c.events.iter().filter_map(|e| if let CombatEvent::Hit { attacker, victim, .. } = e.1 { Some((attacker, victim)) } else { None }).collect();
            assert_eq!(hits, vec![(0, 1), (0, 2)], "one hit per victim, slot order");
        }

        #[test]
        fn four_player_the_round_ends_when_one_team_is_left() {
            // Free for all, three players: the round goes on until one is standing.
            let (mut c, mut p, col) = setup4(Roster::free_for_all(3), spread());
            c.hurt(1, 99, false, Vec3::Z);
            assert!(c.round_over.is_none(), "two still stand");
            c.hurt(2, 99, false, Vec3::Z);
            assert_eq!(c.round_over, Some((0, ROUND_END_TICKS)));
            assert_eq!(c.wins, [1, 0, 0, 0]);
            // The next round starts fresh for all three.
            for _ in 0..ROUND_END_TICKS {
                c.step(&mut p, [0u8; 4], &col);
            }
            assert!(c.round_over.is_none() && c.round == 2);
            assert!((0..3).all(|i| c.fighters[i].action == Action::Ready && c.fighters[i].health == START_HEALTH));
            // 2v2: a team is out when both of its players are down.
            let (mut c, _p, _col) = setup4(Roster::with_teams(&[0, 0, 1, 1]), spread());
            c.hurt(2, 99, false, Vec3::Z);
            assert!(c.round_over.is_none(), "one of team 1 still stands");
            c.hurt(0, 99, false, Vec3::Z);
            assert!(c.round_over.is_none(), "and one of team 0");
            c.hurt(3, 99, false, Vec3::Z);
            assert_eq!(c.round_over, Some((1, ROUND_END_TICKS)), "team 1 is wiped; team 0's last player takes it");
        }

        #[test]
        fn four_player_time_up_goes_to_the_healthiest_and_a_tie_at_the_top_is_a_draw() {
            let (mut c, mut p, col) = setup4(Roster::free_for_all(3), spread());
            c.time_limit = 10;
            c.fighters[2].health = START_HEALTH - 5;
            c.fighters[0].health = START_HEALTH - 1;
            for _ in 0..12 {
                c.step(&mut p, [0u8; 4], &col);
            }
            assert_eq!(c.round_over.map(|r| r.0), Some(1), "slot 1 has the most Health");
            let (mut c, mut p, col) = setup4(Roster::free_for_all(3), spread());
            c.time_limit = 10;
            c.fighters[2].health = 4;
            for _ in 0..12 {
                c.step(&mut p, [0u8; 4], &col);
            }
            assert_eq!(c.round_over.map(|r| r.0), Some(DRAW), "slots 0 and 1 tie at the top");
            assert!(c.time_up);
        }

        #[test]
        fn four_player_everyone_down_together_is_a_draw() {
            // The powerless drain hits everyone in one tick: the first knock-outs leave
            // someone standing (who is scored the winner), the last takes them too.
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), spread());
            for i in 0..4 {
                c.fighters[i].health = 1;
                c.fighters[i].arsenal.clear();
            }
            c.round_ticks = COUNTDOWN_TICKS + POWERLESS_DRAIN_TICKS - 1;
            c.step(&mut p, [0u8; 4], &col);
            assert!(c.fighters.iter().all(|f| f.action == Action::Down));
            assert_eq!(c.round_over, Some((DRAW, ROUND_END_TICKS)));
            assert_eq!(c.wins, [0; 4], "the provisional win is taken back");
        }

        #[test]
        fn four_player_a_melee_blow_passes_through_and_reaches_everyone_in_range() {
            // Psycho Wave (STR = the victim's Level): players 1 and 2 stand inside its reach, 3 far away.
            let at = [Vec3::ZERO, Vec3::new(0.0, 0.0, 40.0), Vec3::new(10.0, 0.0, 35.0), Vec3::new(400.0, 0.0, 400.0)];
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), at);
            c.set_action_row(WAVE, Some(ActionRow { anim_name: "ATK_SPHERE".into(), cast_sound: 201, charge_cycle: 0, dash_start: 10, dash_end: 20, fire_tick: 20, hit_window_end: 22 }), None);
            (c.fighters[1].level, c.fighters[2].level) = (3, 2);
            cast4(&mut c, &mut p, &col, 0, WAVE);
            run4(&mut c, &mut p, &col, 50);
            assert_eq!(
                [c.fighters[0].health, c.fighters[1].health, c.fighters[2].health, c.fighters[3].health],
                [START_HEALTH, START_HEALTH - 3, START_HEALTH - 2, START_HEALTH],
                "each victim takes their own Level; the caster and the far player nothing"
            );
            let hits: Vec<usize> = c.events.iter().filter_map(|e| if let CombatEvent::Hit { attacker: 0, victim, .. } = e.1 { Some(victim) } else { None }).collect();
            assert_eq!(hits, vec![1, 2], "one hit each, in slot order");
        }

        #[test]
        fn four_player_the_unlimited_hole_opens_under_every_enemy() {
            // Tremor is motion 16: "one hole at every player's feet" (`FUN_1402626d0`), the caster's too.
            let at = [Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0), Vec3::new(120.0, 0.0, 60.0), Vec3::new(-150.0, 0.0, 200.0)];
            for (name, roster, ff, holes, hurt) in [
                // The caster's own hole is made too and hurts the caster (LIVE takes 147 / 312 / 148 / 319).
                ("free for all", Roster::free_for_all(4), FriendlyFire::On, vec![0, 1, 2, 3], [true, true, true, true]),
                ("2v2, friendly fire on", Roster::with_teams(&[0, 0, 1, 1]), FriendlyFire::On, vec![0, 1, 2, 3], [true, true, true, true]),
                ("2v2, friendly fire off", Roster::with_teams(&[0, 0, 1, 1]), FriendlyFire::Off, vec![0, 2, 3], [true, false, true, true]),
            ] {
                let (mut c, mut p, col) = setup4(roster, at);
                c.friendly_fire = ff;
                cast4(&mut c, &mut p, &col, 0, TREMOR);
                run4(&mut c, &mut p, &col, CAST_TICKS as usize);
                let mut targets: Vec<usize> = c.areas.iter().map(|a| a.target).collect();
                targets.sort();
                assert_eq!(targets, holes, "{name}: a hole per victim");
                for a in &c.areas {
                    assert!(a.pos.distance(p[a.target].pos) < 1.0, "{name}: under {}'s feet", a.target);
                }
                run4(&mut c, &mut p, &col, 80);
                for (i, expect_hurt) in hurt.into_iter().enumerate() {
                    assert_eq!(c.fighters[i].health < START_HEALTH, expect_hurt, "{name}: player {i}");
                }
            }
        }

        #[test]
        fn four_player_ranking_is_standing_players_first_then_the_latest_knock_out() {
            let (mut c, _p, _col) = setup4(Roster::free_for_all(4), spread());
            assert_eq!(c.placements(), [1, 1, 1, 1], "everyone standing shares first");
            c.round_ticks = 200;
            c.hurt(3, 99, false, Vec3::Z);
            c.round_ticks = 250;
            c.hurt(1, 99, false, Vec3::Z);
            assert_eq!(c.placements(), [1, 3, 1, 4], "{:?}", c.placements());
            c.round_ticks = 300;
            c.hurt(2, 99, false, Vec3::Z);
            assert_eq!(c.round_outcome(), Some(Outcome::Winner { slot: 0, team: 0 }));
            assert_eq!(c.placements(), [1, 3, 2, 4], "later knock-out = better place");
            assert_eq!(c.team_wins(0), 1);
            assert_eq!(c.team_wins(1), 0);
            // A team's score is the sum over its members.
            let (mut c, _p, _col) = setup4(Roster::with_teams(&[0, 0, 1, 1]), spread());
            c.wins = [1, 2, 0, 4];
            assert_eq!((c.team_wins(0), c.team_wins(1)), (3, 4));
            assert_eq!(c.round_outcome(), None);
        }

        // ---- S1e: skills that name "the target / the opponent" ----

        /// A special/status/erase skill with raw fields, cast from button 0 by `who`, with `who`
        /// locked on `lock` (their `Player::target`).
        fn use_locked(c: &mut Combat, p: &mut [Player; 4], col: &Collision, who: usize, lock: usize, skill: SkillDef) {
            let id = skill.id;
            c.skills.push(skill);
            p[who].target = Some(lock);
            cast4(c, p, col, who, id);
            run4(c, p, col, CAST_TICKS as usize + 2);
        }

        #[test]
        fn four_player_instant_effects_land_on_the_lock_target_only() {
            // Fusion ("your Health and target's are averaged"), Teleportation ("teleports target to your start").
            let fusion = def(294, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 46)]);
            let tele = def(95, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 45)]);
            for lock in [1, 2, 3] {
                let (mut c, mut p, col) = setup4(Roster::free_for_all(4), spread());
                c.fighters[0].health = 10;
                use_locked(&mut c, &mut p, &col, 0, lock, fusion.clone());
                for i in 0..4 {
                    // LIVE take 294: the target goes to the average, the caster stays as it was.
                    let want = if i == 0 { 10 } else if i == lock { 15 } else { START_HEALTH };
                    assert_eq!(c.fighters[i].health, want, "Fusion on {lock}: player {i}");
                }
                assert!(c.events.iter().any(|e| e.1 == CombatEvent::Hit { attacker: 0, victim: lock, skill: 294 }));
            }
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), spread());
            let spawn0 = p[0].spawn;
            use_locked(&mut c, &mut p, &col, 0, 3, tele);
            assert_eq!(p[3].pos, spawn0, "the locked player is sent to the caster's start point");
            assert_eq!(p[1].pos, spread()[1], "nobody else moves");
            assert_eq!(p[2].pos, spread()[2]);
            // Doppelganger ("copies the opponent's attributes") copies the lock target's, not the first enemy's.
            let doppel = def(291, Capsule::Status, 76, 2, 0, 18, &[(43, 4)]);
            let (mut c, mut p, col) = setup4(Roster::free_for_all(4), spread());
            c.fighters[1].status.add(Stat::Attack, 1, None);
            c.fighters[3].status.add(Stat::Speed, -20, None);
            use_locked(&mut c, &mut p, &col, 0, 3, doppel);
            assert_eq!(c.fighters[0].status.mods, c.fighters[3].status.mods);
            assert!(c.fighters[0].status.mods.iter().all(|m| m.stat == Stat::Speed), "{:?}", c.fighters[0].status.mods);
        }

        #[test]
        fn four_player_surprise_attack_picks_a_random_enemy_never_a_partner_and_a_duel_draws_nothing() {
            let surprise = def(77, Capsule::Special, 30, 2, 0, 0, &[(43, 3), (44, crate::moves::special::move_kind::TO_TARGET_SPAWN)]);
            // 2v2: enemies are 2 and 3; over many casts both are visited, never 1 (partner) or 0.
            let mut seen = [0; 4];
            let (mut c, mut p, _col) = setup4(Roster::with_teams(&[0, 0, 1, 1]), spread());
            c.skills.push(surprise.clone());
            for k in 0..40 {
                p[0].pos = Vec3::new(1.0 + k as f32, 0.0, 1.0);
                crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Caster, &surprise);
                for (i, n) in seen.iter_mut().enumerate() {
                    *n += (p[0].pos == p[i].spawn) as u32;
                }
            }
            assert!(seen[2] > 5 && seen[3] > 5, "both enemies are picked: {seen:?}");
            assert_eq!((seen[0], seen[1]), (0, 0), "never yourself or your partner");
            // One enemy: no random number is drawn (a duel replays as it always did).
            let (mut c, mut p, _col) = setup4(Roster::duel(), spread());
            let rng = c.rng;
            crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Caster, &surprise);
            assert_eq!(c.rng, rng);
            assert_eq!(p[0].pos, p[1].spawn);
        }

        #[test]
        fn four_player_everyone_effects_reach_exactly_the_active_players() {
            // Fairy Ring (everyone's Aura to 0) and Vicious Balance (everyone's Level to the lowest).
            let fairy = def(400, Capsule::Erase, 40, 2, 0, 0, &[(f::MOTION, 4), (P, 2), (f::ADD_EFFECT, 1), (f::ADD_AMOUNT, 99)]);
            let balance = def(401, Capsule::Erase, 40, 2, 0, 0, &[(f::MOTION, 4), (P, 2), (f::ADD_EFFECT, 47), (f::ADD_AMOUNT, 1)]);
            let (mut c, mut p, _col) = setup4(Roster::free_for_all(3), spread());
            for (i, lv) in [6, 3, 5, 0].into_iter().enumerate() {
                c.fighters[i].level = lv;
                c.fighters[i].aura_ticks = lv * AURA_TICKS_PER_POINT;
            }
            crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Everyone, &balance);
            assert_eq!([0, 1, 2].map(|i| c.fighters[i].level), [3, 3, 3], "the lowest ACTIVE level (slot 3 is not playing and does not count)");
            assert_eq!(c.fighters[3].level, 0, "an inactive slot is untouched");
            crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Everyone, &fairy);
            assert_eq!([0, 1, 2].map(|i| c.fighters[i].aura_ticks), [0, 0, 0]);
            // Status skills that say "everyone".
            let chill = def(330, Capsule::Status, 13, 2, 0, 0, &[(f::MOTION, 4), (P, 2), (f::ADD_EFFECT, 7), (f::ADD_AMOUNT, 90)]);
            crate::moves::apply_effect(&mut c, &mut p, 1, Reach::Everyone, &chill);
            assert!([0, 1, 2].iter().all(|&i| c.fighters[i].frozen_ticks > 0), "all three are frozen");
            assert_eq!(c.fighters[3].frozen_ticks, 0);
        }

        fn ffa4() -> (Match, Collision) {
            let spawns = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 300.0), Vec3::new(300.0, 0.0, 0.0), Vec3::new(-300.0, 0.0, -300.0)];
            let mut c = Combat::with_roster(skills(), Roster::free_for_all(4), std::array::from_fn(|_| vec![AURA_PARTICLE; 30]));
            c.set_rules(TEST_RULES);
            (Match::with_spawns(&spawns, c), floor())
        }

        const PRESS: Input = Input { stick: [0, 0], jump: false, lock: false, skills: 0, cam: 0, cam_fast: false, recenter: false, switch_target: true };

        /// `0x1402531d0`: while locked, the target button's rising edge moves the lock to the
        /// next valid slot; holding it does not repeat; unlocked, it does nothing.
        #[test]
        fn four_player_the_target_button_cycles_on_its_edge_while_locked() {
            let (mut m, mut col) = ffa4();
            let idle = [Input::default(); 4];
            m.step(idle, &mut col);
            assert_eq!(m.players[0].target, Some(1), "the first valid slot");
            assert_eq!(m.players[2].target, Some(0), "everyone holds a target of their own");
            let mut seen = Vec::new();
            for _ in 0..4 {
                m.step([PRESS, Input::default(), Input::default(), Input::default()], &mut col);
                seen.push(m.players[0].target);
                m.step([PRESS, Input::default(), Input::default(), Input::default()], &mut col); // still held: no second step
                assert_eq!(m.players[0].target, *seen.last().unwrap(), "holding the button does not keep cycling");
                m.step(idle, &mut col);
            }
            assert_eq!(seen, vec![Some(2), Some(3), Some(1), Some(2)], "slot order with wrap, never itself");
            assert_eq!(m.players[1].target, Some(0), "the others did not move");
            // Not locked: the button is ignored.
            m.players[0].locked = false;
            let before = m.players[0].target;
            m.step([PRESS, Input::default(), Input::default(), Input::default()], &mut col);
            assert_eq!(m.players[0].target, before);
        }

        #[test]
        fn four_player_the_lock_camera_and_casts_follow_the_lock_target() {
            let (mut m, mut col) = ffa4();
            m.step([Input::default(); 4], &mut col);
            m.step([PRESS, Input::default(), Input::default(), Input::default()], &mut col); // 1 -> 2
            assert_eq!(m.players[0].target, Some(2));
            for _ in 0..40 {
                m.step([Input::default(); 4], &mut col);
            }
            assert!((m.players[0].cam_yaw - std::f32::consts::FRAC_PI_2).abs() < 0.05, "the camera swings east to slot 2: {}", m.players[0].cam_yaw);
            // A cast flies at the lock target, not the first enemy.
            m.combat.round_ticks = COUNTDOWN_TICKS;
            m.combat.fighters[0].level = 8;
            m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            m.combat.fighters[0].slots[0] = Some(BLASTER);
            m.step([Input { skills: 1, ..Default::default() }, Input::default(), Input::default(), Input::default()], &mut col);
            for _ in 0..CAST_TICKS + 1 {
                m.step([Input::default(); 4], &mut col);
            }
            let shot = m.combat.projectiles.first().expect("the shell left");
            assert_eq!((shot.owner, shot.target), (0, 2));
            assert!(shot.shot.heading().x > 0.0 && shot.shot.heading().z.abs() < shot.shot.heading().x, "flying east: {:?}", shot.shot.heading());
            assert!((m.players[0].yaw - std::f32::consts::FRAC_PI_2).abs() < 0.1, "the caster faces the target: {}", m.players[0].yaw);
        }

        /// D19 (audit): Optical Camo only invalidates that one target; the lock moves on and is
        /// only released when nobody is left.
        #[test]
        fn four_player_camouflage_moves_the_lock_on_and_only_drops_it_when_nobody_is_left() {
            let (mut m, mut col) = ffa4();
            m.step([Input::default(); 4], &mut col);
            assert_eq!((m.players[0].locked, m.players[0].target), (true, Some(1)));
            m.combat.fighters[1].status.invisible_ticks = 600;
            m.step([Input::default(); 4], &mut col);
            assert_eq!((m.players[0].locked, m.players[0].target), (true, Some(2)), "handed over to the next valid player, lock kept");
            m.combat.fighters[2].status.invisible_ticks = 600;
            m.combat.fighters[3].status.invisible_ticks = 600;
            m.step([Input::default(); 4], &mut col);
            assert_eq!((m.players[0].locked, m.players[0].target), (false, None), "no one can lock onto you: released");
            // Toggling the lock on again does nothing while everyone is hidden.
            m.step([Input { lock: true, ..Default::default() }, Input::default(), Input::default(), Input::default()], &mut col);
            assert!(!m.players[0].locked);
            // Coming out of hiding, the lock button works again.
            for f in 1..4 {
                m.combat.fighters[f].status.invisible_ticks = 0;
            }
            m.step([Input::default(); 4], &mut col);
            m.step([Input { lock: true, ..Default::default() }, Input::default(), Input::default(), Input::default()], &mut col);
            assert!(m.players[0].locked && m.players[0].target.is_some());
        }

        #[test]
        fn four_player_a_knocked_out_target_hands_over_and_the_target_survives_a_new_round() {
            let (mut m, mut col) = ffa4();
            m.step([Input::default(); 4], &mut col);
            m.players[0].target = Some(1);
            m.combat.hurt(1, 99, false, Vec3::Z);
            m.step([Input::default(); 4], &mut col);
            assert_eq!(m.players[0].target, Some(2), "player 1 is out: on to the next");
            assert!(m.combat.round_over.is_none(), "three are still standing");
            // The target is sim state like the lock: a new round keeps it.
            m.combat.hurt(3, 99, false, Vec3::Z);
            m.combat.hurt(2, 99, false, Vec3::Z);
            assert!(m.combat.round_over.is_some());
            for _ in 0..ROUND_END_TICKS + 2 {
                m.step([Input::default(); 4], &mut col);
            }
            assert_eq!(m.combat.round, 2);
            assert_eq!(m.players[0].pos, m.players[0].spawn, "back on the spawn");
            assert!(m.players[0].target.is_some());
        }

        /// Seeded four-player scripts (casts of every family, lock cycling, knock-outs,
        /// round changes, a time-up), hashed every tick over >= 3000 ticks: two runs
        /// of the same script agree on every tick, for teams and for free-for-all.
        /// Returns (final hash, hit events, knock-out rounds, time-up rounds, round, wins).
        fn play4(roster: Roster, seed: u32, ticks: u64) -> (u64, usize, usize, usize, u32, [u32; 4]) {
            let mut col = replay::arena();
            let spawns = [Vec3::new(-150.0, 0.0, -150.0), Vec3::new(150.0, 0.0, -150.0), Vec3::new(150.0, 0.0, 150.0), Vec3::new(-150.0, 0.0, 150.0)];
            let mk = |a: &[i16]| a.iter().copied().cycle().take(30).collect::<Vec<i16>>();
            let mut combat = Combat::with_roster(replay::extra_skills(), roster, [mk(replay::WEAPONS), mk(replay::TRAPS), mk(replay::TRICKS), mk(replay::FX)]);
            combat.test_loadout = Some(([BLASTER, WAVE, SHELL, TIMER_MINE], 8));
            combat.time_limit = 1500;
            let mut m = Match::with_spawns(&spawns, combat);
            let mut script = replay::Script::new(seed, 4).with_switch_presses(seed ^ 0x7a7a_7a7a);
            let mut h = replay::H::new();
            let (mut hits, mut seen, mut ko_rounds, mut time_rounds) = (0, 0, 0, 0);
            for t in 0..ticks {
                m.step(script.tick(t), &mut col);
                replay::hash_state(&mut h, &m);
                for (seq, e) in &m.combat.events {
                    if *seq > seen {
                        seen = *seq;
                        hits += matches!(e, CombatEvent::Hit { .. }) as usize;
                    }
                }
                if m.combat.round_over.is_some_and(|(_, left)| left == ROUND_END_TICKS) {
                    if m.combat.time_up {
                        time_rounds += 1;
                    } else {
                        ko_rounds += 1;
                    }
                }
            }
            (h.0, hits, ko_rounds, time_rounds, m.combat.round, m.combat.wins)
        }

        #[test]
        fn four_player_replay_is_deterministic() {
            for (name, roster, seed) in [("2v2", Roster::with_teams(&[0, 0, 1, 1]), 0x01f3_877a), ("free for all", Roster::free_for_all(4), 0x01f4_0336)] {
                let (a, b) = (play4(roster, seed, 3600), play4(roster, seed, 3600));
                assert_eq!(a, b, "{name}: same script, same state on every tick");
                assert!(a.1 > 20, "{name}: the script fights: {a:?}");
                assert!(a.2 + a.3 >= 2 && a.4 >= 2, "{name}: rounds turn over: {a:?}");
                eprintln!("four-player replay {name}: hash {:#018x} hits {} ko rounds {} time-up rounds {} rounds {} wins {:?}", a.0, a.1, a.2, a.3, a.4, a.5);
            }
            // The knock-out rules are exercised by at least one of the two scripts.
            let ko: usize = [(Roster::with_teams(&[0, 0, 1, 1]), 0x01f3_877a), (Roster::free_for_all(4), 0x01f4_0336)].iter().map(|(r, sd)| play4(*r, *sd, 3600).2).sum();
            assert!(ko >= 1, "a round ended by a knock-out in one of the scripts");
            // (Seeds re-picked by lane CASTRULES: with the aim following the camera when
            // unlocked, payment at the fire tick and the cast gate, the old two seeds
            // happened to play out with no knock-out; these two fight to one in each script.)
            // (Re-picked again by lane REACTIONS2: a paralysed fighter who pushes the stick now
            // struggles (state 20, `0x14025b5e0`) instead of standing, which moved every script
            // that binds anyone; the old two seeds then ended no round by a knock-out.)
        }
    }

    // ---- deterministic 1v1 replay hash (lane S1, four-player foundation) ----
    //
    // The four-player refactor (docs/audits/P0-FOURPLAYER.md, steps S1a-S1d)
    // must leave a two-player match bit-identical. These scripted matches run
    // through `Match::step` (the real tick: movement, combat, pushout) with
    // seeded pseudo-random inputs and hash every tick of state. The golden
    // values below were recorded on the tree BEFORE the refactor and must
    // never change while 1v1 behaviour does not. Only fields that existed
    // before the refactor are hashed, and only slots 0 and 1, so later
    // additions (targets, teams, slots 2-3) do not disturb them.
    mod replay {
        use super::*;

        pub(super) struct H(pub(super) u64);
        impl H {
            pub(super) fn new() -> Self {
                H(0xcbf2_9ce4_8422_2325)
            }
            fn u(&mut self, v: u64) {
                for b in v.to_le_bytes() {
                    self.0 ^= b as u64;
                    self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            fn i(&mut self, v: i64) {
                self.u(v as u64)
            }
            fn f(&mut self, v: f32) {
                self.u(v.to_bits() as u64)
            }
            fn v3(&mut self, v: Vec3) {
                self.f(v.x);
                self.f(v.y);
                self.f(v.z);
            }
            fn s(&mut self, s: &str) {
                self.u(s.len() as u64);
                for b in s.bytes() {
                    self.u(b as u64);
                }
            }
        }

        fn hash_event(h: &mut H, e: &CombatEvent) {
            match e {
                CombatEvent::Hit { victim, skill, .. } => {
                    h.u(1);
                    h.u(*victim as u64);
                    h.i(*skill as i64);
                }
                CombatEvent::Blocked { victim, skill, broken, .. } => {
                    h.u(2);
                    h.u(*victim as u64);
                    h.i(*skill as i64);
                    h.u(*broken as u64);
                }
                CombatEvent::Burst { pos, skill, .. } => {
                    h.u(3);
                    h.v3(*pos);
                    h.i(*skill as i64);
                }
                CombatEvent::Denied { who, slot } => {
                    h.u(4);
                    h.u(*who as u64);
                    h.u(*slot as u64);
                }
                CombatEvent::Capsule { who, slot, skill } => {
                    h.u(5);
                    h.u(*who as u64);
                    h.u(*slot as u64);
                    h.i(*skill as i64);
                }
                // The events lane's variants never reach the hash (`legacy_events`).
                _ => unreachable!("a sound/effect event in the golden hash"),
            }
        }

        pub(super) fn hash_state(h: &mut H, m: &Match) {
            h.u(m.tick);
            for i in m.combat.roster.slots() {
                let p = &m.players[i];
                h.v3(p.pos);
                h.v3(p.vel);
                h.v3(p.prev_pos);
                h.f(p.yaw);
                h.f(p.prev_yaw);
                h.f(p.cam_yaw);
                h.f(p.land_timer);
                h.f(p.rise_seconds);
                h.f(p.heading());
                h.u(p.grounded as u64);
                h.u(p.heavy_land as u64);
                h.u(p.locked as u64);
                h.u(p.lock_on_events as u64);
                h.u(p.dodge_ticks as u64);
                h.u(p.recover_ticks as u64);
                h.s(&format!("{:?}|{:?}|{:?}", p.motion, p.gait, p.dodge));
                match p.pivot() {
                    Some(pv) => {
                        h.u(1);
                        h.s(&format!("{:?}", pv.dir));
                        h.f(pv.time);
                    }
                    None => h.u(0),
                }
                let f = &m.combat.fighters[i];
                for v in [f.health, f.level, f.aura_ticks, f.aura_frac.to_bits() as i32, f.charge.map_or(-1, |c| c.spent)] {
                    h.i(v as i64);
                }
                for s in f.slots {
                    h.i(s.map_or(-1, |x| x as i64));
                }
                for v in f.spent.iter().chain(f.denied_ticks.iter()) {
                    h.u(*v as u64);
                }
                h.u(f.arsenal.len() as u64);
                for a in &f.arsenal {
                    h.i(*a as i64);
                }
                for a in &f.used {
                    h.i(*a as i64);
                }
                h.s(&format!("{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", f.action, f.guard, f.status, f.capsule_anchor, f.hit_dir, f.charge));
                // (`0` stands where the removed consume lock counter was hashed, so the stream is unchanged.)
                for v in [f.action_ticks, f.cast_seq, f.frozen_ticks, 0, f.poison_ticks, f.invuln_ticks, f.fire_ticks, f.cast_elapsed, f.spawn_timer] {
                    h.u(v as u64);
                }
                for v in f.spot_wait {
                    h.u(v as u64);
                }
                for v in [f.cast_skill, AURA_PARTICLE, f.last_slot as i16, f.reaction_row, f.last_launch_speed] {
                    h.i(v as i64);
                }
                for v in [f.last_attack, f.last_hurt_by, f.pending_shot] {
                    h.i(v.map_or(-1, |x| x as i64));
                }
                h.i(f.cast_strength.map_or(-1, |x| x as i64));
                h.i(f.last_hit_tick.map_or(-1, |x| x as i64));
                h.v3(f.knock_vel);
                for v in [f.prev_buttons as u64, f.dash_eligible as u64, f.struck as u64, f.melee as u64, f.flying as u64, f.damage_accum as u64] {
                    h.u(v);
                }
            }
            let c = &m.combat;
            for v in [c.round as u64, c.wins[0] as u64, c.wins[1] as u64, c.round_ticks as u64, c.time_up as u64, c.airborne[0] as u64, c.airborne[1] as u64, c.rng as u64, c.legacy_seq as u64, c.next_id as u64] {
                h.u(v);
            }
            // A draw was the magic winner 2; keep that spelling for the hash.
            h.i(c.round_over.map_or(-1, |(w, left)| (if w > 1 { 2 } else { w as i64 }) * 10_000 + left as i64));
            for (seq, e) in &c.legacy_events {
                h.u(*seq as u64);
                hash_event(h, e);
            }
            for p in &c.projectiles {
                h.u(p.id as u64);
                h.u(p.owner as u64);
                h.i(p.skill as i64);
                h.v3(p.pos);
                h.v3(p.prev_pos);
                h.s(&format!("{:?}", p.shot));
                h.u(p.turned as u64);
                h.u(p.harmless as u64);
                h.u(p.brushed as u64);
                h.u(p.button as u64);
                h.i(p.prop.map_or(-1, |x| x as i64));
                h.i(p.strength.map_or(-1, |x| x as i64));
            }
            for k in &c.capsules {
                h.u(k.id as u64);
                h.u(k.owner as u64);
                h.i(k.skill as i64);
                h.v3(k.pos);
            }
            for a in &c.areas {
                h.u(a.id as u64);
                h.u(a.owner as u64);
                h.i(a.skill as i64);
                h.v3(a.pos);
                h.s(&format!("{:?}", a.spec));
                for v in [a.timer, a.pulses_left, a.life] {
                    h.u(v as u64);
                }
                h.u(a.armed as u64);
                h.u(a.detonate as u64);
                h.u((a.hit != 0) as u64);
            }
            h.s(&format!("{:?}", c.crystal));
            for (a, b, c2, d) in &c.pending_effects {
                for v in [*a as u64, *b as u64, *c2 as u64, *d as u64] {
                    h.u(v);
                }
            }
            // Instant effects still counting down (nothing is hashed when none are, so scenarios
            // without an opponent-targeting instant keep their hashes).
            for e in &c.delayed_effects {
                for v in [e.due as u64, e.caster as u64, e.target as u64, e.skill as u64, e.button as u64, e.everyone as u64] {
                    h.u(v);
                }
            }
            // Slots beyond the duel (nothing is hashed here in a two-player match,
            // so the golden values do not move), and the new target/aim fields.
            for i in c.roster.slots().filter(|&i| i >= 2) {
                h.u(c.wins[i] as u64);
                h.u(c.airborne[i] as u64);
            }
            if c.roster.count() > 2 {
                for p in &c.projectiles {
                    h.u(p.target as u64);
                }
                for a in &c.areas {
                    h.u(a.target as u64);
                    h.u(a.hit as u64);
                }
                for i in c.roster.slots() {
                    h.i(m.players[i].target.map_or(-1, |t| t as i64));
                }
            }
        }

        /// A stage to fight on: a finite floor (so runners really fall off and
        /// respawn) and a wall stub between the start points.
        pub(super) fn arena() -> Collision {
            let (s, w) = (320.0, 70.0);
            Collision::new(vec![
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
                [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
                [Vec3::new(0.0, 0.0, -w), Vec3::new(0.0, 60.0, -w), Vec3::new(0.0, 60.0, w)],
                [Vec3::new(0.0, 0.0, -w), Vec3::new(0.0, 60.0, w), Vec3::new(0.0, 0.0, w)],
            ])
        }

        /// A seeded stream of inputs for both players.
        pub(super) struct Script {
            rng: u32,
            players: usize,
            switch: Option<u32>,
            stick: [[i8; 2]; MAX_PLAYERS],
            cam: [i8; MAX_PLAYERS],
            held: [u8; MAX_PLAYERS],
        }
        impl Script {
            pub(super) fn new(seed: u32, players: usize) -> Self {
                Script { rng: seed | 1, players, switch: None, stick: [[0; 2]; MAX_PLAYERS], cam: [0; MAX_PLAYERS], held: [0; MAX_PLAYERS] }
            }
            /// Also press the switch-target button now and then (its own random stream).
            pub(super) fn with_switch_presses(mut self, seed: u32) -> Self {
                self.switch = Some(seed | 1);
                self
            }
            fn next(&mut self) -> u32 {
                let mut x = self.rng;
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                self.rng = x;
                x
            }
            pub(super) fn tick(&mut self, t: u64) -> [Input; MAX_PLAYERS] {
                let mut out = [Input::default(); MAX_PLAYERS];
                for i in 0..self.players {
                    if t % 24 == (i as u64 * 7) % 24 {
                        let r = self.next();
                        self.stick[i] = match r % 6 {
                            0 => [0, 0],
                            1 => [127, 0],
                            2 => [-127, 0],
                            3 => [0, 127],
                            4 => [0, -127],
                            _ => [((r >> 8) % 255) as i8, ((r >> 16) % 255) as i8],
                        };
                        self.cam[i] = if r % 5 == 0 { ((r >> 24) % 255) as i8 } else { 0 };
                    }
                    let r = self.next();
                    // Buttons are held a few ticks, so presses have edges and charges can hold.
                    if self.held[i] != 0 && r % 4 != 0 {
                        // keep holding
                    } else {
                        self.held[i] = if r % 9 < 3 { 1 << ((r >> 8) % 4) } else { 0 };
                    }
                    out[i] = Input {
                        stick: self.stick[i],
                        jump: r % 41 == 0,
                        lock: r % 53 == 0,
                        skills: self.held[i],
                        cam: self.cam[i],
                        recenter: r % 97 == 0,
                        ..Default::default()
                    };
                }
                // The target button, from a stream of its own so the script above is untouched.
                if let Some(sw) = &mut self.switch {
                    for o in out.iter_mut().take(self.players) {
                        *sw ^= *sw << 13;
                        *sw ^= *sw >> 17;
                        *sw ^= *sw << 5;
                        o.switch_target = *sw % 4 == 0;
                    }
                }
                out
            }
        }

        pub(super) fn extra_skills() -> Vec<SkillDef> {
            let mut sk = skills();
            // Status: Optical Camo, Doppelganger, Paralyze (targeted, at once).
            sk.push(def(115, Capsule::Status, 71, 2, 0, 12, &[(53, 450)]));
            sk.push(def(291, Capsule::Status, 76, 2, 0, 18, &[]));
            sk.push(def(112, Capsule::Status, 13, 2, 0, 0, &[(f::MOTION, 4), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]));
            // Specials: Surprise Attack (teleport to the target's start), Fusion, Dazzle, Trigger.
            sk.push(def(77, Capsule::Special, 30, 2, 0, 0, &[(43, 3), (44, 7)]));
            sk.push(def(294, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 46)]));
            sk.push(def(54, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 41), (52, 300)]));
            sk.push(def(236, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 42)]));
            // Annoying Gift, Teleportation (target to your start), Angel's Breath (reverse target's attributes).
            sk.push(def(93, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 50)]));
            sk.push(def(95, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 45)]));
            sk.push(def(96, Capsule::Special, 30, 2, 0, 0, &[(f::MOTION, 4), (51, 51)]));
            // Erase: Fairy Ring (everyone's Aura to 0), Vicious Balance (everyone's Level to the lowest).
            sk.push(def(400, Capsule::Erase, 40, 2, 0, 0, &[(f::MOTION, 4), (P, 2), (f::ADD_EFFECT, 1), (f::ADD_AMOUNT, 99)]));
            sk.push(def(401, Capsule::Erase, 40, 2, 0, 0, &[(f::MOTION, 4), (P, 2), (f::ADD_EFFECT, 47), (f::ADD_AMOUNT, 1)]));
            sk
        }

        /// One scripted match; returns (final hash, per-100-tick checkpoints, coverage numbers).
        fn play(seed: u32, ticks: u64, arsenals: [&[i16]; 2], loadout: Option<[i16; 4]>, time_limit: Option<u32>, spots: bool) -> (u64, Vec<u64>, [u64; 9]) {
            play_with(seed, ticks, arsenals, loadout, time_limit, spots, false)
        }

        fn play_with(seed: u32, ticks: u64, arsenals: [&[i16]; 2], loadout: Option<[i16; 4]>, time_limit: Option<u32>, spots: bool, switch_presses: bool) -> (u64, Vec<u64>, [u64; 9]) {
            let mut col = arena();
            let mk = |a: &[i16]| a.iter().copied().cycle().take(30).collect::<Vec<i16>>();
            let mut combat = Combat::with_arsenals(extra_skills(), [mk(arsenals[0]), mk(arsenals[1])]);
            if let Some(l) = loadout {
                combat.test_loadout = Some((l, 8));
            }
            if let Some(t) = time_limit {
                combat.time_limit = t;
            }
            if spots {
                combat.set_capsule_spots([
                    vec![Vec3::new(-150.0, 0.0, 0.0), Vec3::new(-120.0, 0.0, 30.0), Vec3::new(-120.0, 0.0, -30.0)],
                    vec![Vec3::new(150.0, 0.0, 0.0), Vec3::new(120.0, 0.0, 30.0), Vec3::new(120.0, 0.0, -30.0)],
                ]);
            }
            let mut m = Match::new([Vec3::new(-150.0, 0.0, 0.0), Vec3::new(150.0, 0.0, 0.0)], combat);
            m.jump_zones = vec![(Vec3::new(-20.0, 0.0, 100.0), Vec3::new(20.0, 0.0, 140.0), 150.0)];
            let mut script = Script::new(seed, 2);
            if switch_presses {
                script = script.with_switch_presses(0xfeed_5eed);
            }
            let mut h = H::new();
            let mut checkpoints = Vec::new();
            let (mut hits, mut blocks, mut bursts, mut ko_rounds, mut captures) = (0u64, 0u64, 0u64, 0u64, 0u64);
            let mut seen_seq = 0;
            let mut peak_shots = 0u64;
            for t in 0..ticks {
                let inputs = script.tick(t);
                m.step(inputs, &mut col);
                hash_state(&mut h, &m);
                for (seq, e) in &m.combat.events {
                    if *seq > seen_seq {
                        seen_seq = *seq;
                        match e {
                            CombatEvent::Hit { .. } => hits += 1,
                            CombatEvent::Blocked { .. } => blocks += 1,
                            CombatEvent::Burst { .. } => bursts += 1,
                            CombatEvent::Capsule { .. } => captures += 1,
                            _ => {}
                        }
                    }
                }
                peak_shots = peak_shots.max((m.combat.projectiles.len() + m.combat.areas.len()) as u64);
                if m.combat.round_over.is_some_and(|(_, left)| left == ROUND_END_TICKS) {
                    ko_rounds += 1;
                }
                if t % 100 == 99 {
                    checkpoints.push(h.0);
                }
            }
            let cov = [hits, blocks, bursts, ko_rounds, m.combat.round as u64, peak_shots, m.players[0].lock_on_events as u64 + m.players[1].lock_on_events as u64, m.combat.wins[0] as u64 + m.combat.wins[1] as u64, captures];
            (h.0, checkpoints, cov)
        }

        pub(super) const WEAPONS: &[i16] = &[BLASTER, WAVE, SHELL, TIMER_MINE, LASER, LANCE, WALL, BOMB];
        pub(super) const TRAPS: &[i16] = &[TREMOR, RAIN, SENSOR_MINE, REMOTE_MINE, PHOTON_WAVE, TWIN_LASERS, FIRE_WALL, ROOT];
        pub(super) const TRICKS: &[i16] = &[KI_BULLET, ICE_LANCE, DHAMPIR, REFLECT, GUST, ABOUT_FACE, PARALYZE_BARRIER, METEOR];
        const MIXED: &[i16] = &[AURA_PARTICLE, AURA_PARTICLE, BLASTER, AURA_PARTICLE, WAVE, AURA_PARTICLE, LASER, AURA_PARTICLE, SHELL, AURA_PARTICLE, AURA_PARTICLE, BOMB];
        pub(super) const FX: &[i16] = &[115, 291, 112, 77, 294, 54, 236, 93, 95, 96, 400, 401, BLASTER, WAVE, PHOTON_WAVE];

        fn run(name: &str) -> (u64, Vec<u64>, [u64; 9]) {
            match name {
                "weapons" => play(0x1234_5678, 3000, [WEAPONS, TRAPS], Some([BLASTER, WAVE, SHELL, TIMER_MINE]), None, false),
                "traps" => play(0x0bad_cafe, 3000, [TRAPS, TRICKS], Some([TREMOR, RAIN, SENSOR_MINE, PHOTON_WAVE]), None, false),
                "tricks" => play(0xdead_beef, 3000, [TRICKS, WEAPONS], Some([KI_BULLET, REFLECT, ICE_LANCE, PARALYZE_BARRIER]), None, true),
                "effects" => play(0x5eed_0001, 3600, [FX, FX], Some([115, 291, 294, 400]), None, false),
                "effects2" => play(0x5eed_0002, 3600, [FX, WEAPONS], Some([77, 54, 236, 401]), Some(900), true),
                "effects3" => play(0x5eed_0003, 3600, [FX, TRAPS], Some([93, 95, 96, 54]), Some(1200), true),
                "capsules" => play(0x0c0f_fee0, 5400, [MIXED, TRICKS], None, None, true),
                _ => unreachable!(),
            }
        }

        /// Re-recorded by lane CASTRULES (base 0952fcc held the lane S1 values
        /// ca0c671e149aafc5, 66c026198288ffbd, 58b43fc2b203094b, 9b19bebb15c71c72,
        /// ddb4e99837dcb5eb, dd31b635dcc36655, 1072a4afab1198ba). Every scenario
        /// presses skill buttons, so every hash moves, for these reasons (the
        /// movement-only golden below did NOT move, which pins that none of it
        /// touched the body):
        ///  * all: Aura is a 60,000-unit fixed point refilled on the program's
        ///    curve with the multiplicative handicap and paused during casts and
        ///    dodges (`0x14024de20`); payment at the fire tick, not the press
        ///    (`0x1402626d0`); casts refused in a dodge, landing or (for ground
        ///    skills) in the air (`0x14024fce0`); the body turns to its aim at 10 deg
        ///    a tick and an unlocked cast flies where the camera looks, with no
        ///    homing (`0x14025bd40`, `0x140258180`); Level cap 99;
        ///  * weapons / traps / tricks / capsules: the same, plus capsules back after
        ///    870 sim ticks and captured only from the ground within 15 u in 3-D
        ///    (`0x14014fd10`, `0x14014f630`);
        ///  * effects / effects2 / effects3: the same, plus Health costs and poison
        ///    paid at the fire tick and able to kill, and status slots that replace a
        ///    timed amount and cap (`0x14024d7a0`, `0x14024d5c0`).
        /// Re-recorded again by lane REACTIONS (base e6ab78b held weapons f7d0c57bde400ac9,
        /// traps 85109e837e4846ad, tricks 67ef14ae3a7f00e1, effects2 e83e8a16cd19ea9a, effects3
        /// 8738 4d7bc6b8ff36, capsules 325d81f33fcd9073; `effects` did not move). Reasons: a hit
        /// reaction is now the clip's own length, chains an s60 second reaction, and a body in a
        /// reaction no longer turns back toward the camera (`0x14025c1a0`, `0x14025b5e0` is only
        /// the idle states' handler); a knock-out plays out its reaction and lies down for good
        /// instead of freezing in place; a launched body is owned by the combat side (no gravity or
        /// floor-stick from the ordinary jump step); falling out of the arena is now detected (it
        /// never was: the respawn rebuilt `prev_pos`), costs 3 Health silently with no stagger and
        /// 180 ticks of protection (`0x140251d80`); the powerless drain is a silent Health change.
        /// Re-recorded by lane GUARDS (merged base held weapons 5fed0d341180ced9, traps
        /// ffffca582ec002d2, tricks 2f55d73ad6e7070c, effects 1a39d9caf22eaaf3, effects2
        /// fa84abd1a0977067, effects3 fb101a31410dce3c; `capsules` did not move). Reasons --
        /// weapons / traps / tricks: guards now drain their DEF per block, end 10 sim ticks after
        /// the first block, break and are erased from their button (`0x140271760`, `0x14026cd70`,
        /// `0x140262fc0`), skip Rain / mines / around-self etc. per kind, and a hit no longer
        /// drops a standing guard; effects: Optical Camo ends when a hit reaches the body
        /// (`0x14026f870` tail) and Surprise Attack (s44 = 7) is no longer read as an automatic
        /// Shelter; effects2 / effects3: both. The movement-only golden did not move.
        /// Re-recorded by lane PROJECTILES (merged base held weapons d170bf8d86a231f5, traps
        /// d5b2afdf572eef2b, tricks 7fdce9c083e41ba4, effects 1dfbe4b6e46f52ce, effects2
        /// 9085fa29907031e9, effects3 f3566f662944c9ad, capsules 3a73aae76920483a). Every scenario
        /// casts Blaster / Bomb / Laser / Meteor / mines / Photon Wave / Rain, so every hash moves:
        /// objects step once per program tick with the read numbers (2.78 u/s and 3 deg/s per
        /// point, ramps, range = -s64 x 10 u, the 60 deg homing cone, lobs by s66 / s67, Fall
        /// diagonal, Arc tilted with re-aim, range bands), hits are swept segments against the 5 u
        /// body sphere 9 u over the feet, blasts grow over 30 ticks and a wall / floor / mine blast
        /// hurts its owner, Hole / Rain have their warning-active timelines (Rain drops are random,
        /// from the sim's own generator, so the rng stream moves too), Sensor Mines are set off by
        /// anyone, the dash is (10 / window) x s65 a tick and sword / punch hits are a blade / a
        /// sphere (`0x140264710`, `0x140273d40`, `0x140264b00`, `0x1402657f0`, `0x140266290`,
        /// `0x140266bf0`, `0x14026ae10`, `0x14026a7f0`, `0x14026b8d0`, `0x140267e90`, `0x14018f050`).
        /// The movement-only golden did not move.
        /// Re-recorded by lane CASTSTILL (final: effects2 5563e1d94aba0722 -> bb7a9035e50ba29e,
        /// effects3 9564e879e09cedba -> 2661b8773d9b81bc, capsules ce77a32b87f7c031 ->
        /// ee5f80f31991479a; weapons, traps, tricks, effects and the movement-only golden did
        /// not move): the Aura Particle use no longer holds the fighter and a capture of an
        /// Aura / green capsule no longer locks it, so the scripted walks in the three
        /// scenarios that use or capture them go on moving. Live trace of the PC original:
        /// 1.85 u per program tick through the use; programs `0x14025a3e0`, `0x1402517c0`.
        /// Re-recorded 2026-10-01 (orchestrator): the round's opening capsules are takeable at
        /// once -- `FUN_14014ee70` writes the spot state 2 (ready) directly, only a respawn
        /// goes through the 15-tick appearing state 1 (`FUN_14014ed10`). tricks, effects2,
        /// effects3 and capsules capture an opening capsule earlier; the others don't.
        /// Re-recorded by lane MOVESKILLS (effects2 bb7a9035e50ba29e -> b313131c7d5902ae; every other
        /// scenario and the movement-only golden did not move, checked by toggling the change back):
        /// Annoying Gift (the scenario's stand-in id 93, s51 = 50) no longer overwrites the target's
        /// button it was pressed from -- the target gets only the caster's OTHER buttons
        /// (`FUN_140272bc0` case 0x32 skips `attack + 0xc`).
        /// Re-recorded by lane REACTIONS2 (effects 52c23fe8dd7a4ff9 -> d726f0bb9ca63bd7, effects2
        /// b313131c7d5902ae -> 63315e43c6949ca8, effects3 2661b8773d9b81bc -> c1902b1208a619ab;
        /// weapons, traps, tricks, capsules and the movement-only golden did not move, and all
        /// three effects hashes return when the change below is switched off): a paralysed
        /// fighter (status 4: Pressure, Entangle ... Paralyze Barrier) who pushes the stick now
        /// struggles -- the body turns to the stick and plays DMG_PARARISE for the clip's length
        /// as a state of its own (state 20, `0x14025b5e0` / `0x14025ad50`) -- instead of
        /// standing; the scripted walks in those scenarios push the stick while bound.
        /// Re-recorded by lane SHOTCLASS (all seven moved; the movement-only golden did not, and it
        /// is the proof that nothing but the cast / hit timeline changed): a new attack object's
        /// first program tick only arms it (`FUN_1402638d0`, state 1 -> 2) -- shots, areas, mines,
        /// the delayed instants (LIVE fire + s65 + 1) and the blast that grows from an explosion all
        /// start one program tick later than before, and a shell turned back by a Reverse field waits
        /// its 20 ticks (`FUN_14026cd70` code 6), so every scripted duel's hit ticks move by one
        /// program tick or more and the hash with them.
        /// Re-recorded by lane BODYRADIUS (effects 06da13d5c3971bcf -> 86509b64a487e55a, effects2
        /// a12273fcefb1e4cb -> 95159edde211aa7d, effects3 ecb1f6e9799a08c2 -> e67641aa1c13b8dd; weapons,
        /// traps, tricks, capsules and the movement-only golden did not move): fighters now stop 10.0 u
        /// apart, not 9.0 (`FUN_140250780`: `size_a / 2 + size_b / 2`, charaset `+0x0E` = 10;
        /// `body.rs`), so the three scripted duels that close to melee range play out differently.
        /// Switching the contact distance back to 9.0 returns all three hashes exactly (checked).
        /// Re-recorded by lane RESIDUE (weapons 030e20cc416b7089 -> 9a4f6e6a70b0d793, traps
        /// efcf1cae23669938 -> 4560fe5e478b9861, tricks baa97ea66b11a69f -> c843dd2d6e104886, effects2
        /// 95159edde211aa7d -> 9612e07fdc034d33; effects, effects3, capsules and the movement-only golden did
        /// not move; the scripted duels' skills have no action rows, so their melee blows use the fallback
        /// window, whose end now also waits for the armed blow; and Psycho Kinesis / Power with no object in
        /// reach now cast, LIVE takes 018 / 017): a melee blow
        /// (punch / kick / sword object) is armed one program tick after its fire tick like every attack
        /// object (`FUN_1402638d0`, LIVE: fire 59 -> health change 60 in every punch / sword take), so the
        /// one scripted duel with a melee hit in it plays its hit two sim ticks later. The target-point
        /// offset (`Body::lock`) is not in the goldens: their fighters use the default body.
        /// Re-recorded by lane GAMEPLAY-ACCURACY (effects2 9612e07fdc034d33 -> 21b621a6447cb169; the six
        /// other scenarios and the movement-only golden did not move): a short-range dash's program step
        /// is pushed apart from the other body only after its second half (`FUN_140254590` pushes once per
        /// program tick, after the state handler), and the cast clip's root motion is added after the
        /// dash has measured its distance (`FUN_14024ec80`: `FUN_140256e80` then `FUN_140257510`), so the
        /// one scripted duel with a melee dash in it closes differently. LIVE `188_Hyperpunch`.
        /// Re-recorded by lane GAMEPLAY-ACCURACY (weapons 9a4f6e6a70b0d793 -> 5fb3120985f2e51b, tricks
        /// c843dd2d6e104886 -> 361cc528f1b7e5d9, effects2 21b621a6447cb169 -> e489fe0d1df909c0, capsules
        /// ac31dee7320c59ad -> 83fd5d1979b44928; traps, effects, effects3 and the movement-only golden did not
        /// move): the arc and multi-laser shots now bend and tilt to the program's side (`FUN_140290180` /
        /// `FUN_140290050` turn toward the object's physical right, which is minus our frame's `right`),
        /// so every scripted laser flies the mirror of its old arc.
        /// Re-recorded by lane GAMEPLAY-ACCURACY round 2 (tricks 361cc528f1b7e5d9 -> 039c125a7d43f344, capsules
        /// 83fd5d1979b44928 -> 33c7dbbd1818b62f; the five other scenarios and the movement-only golden did not
        /// move, and with the pitch forced to 0 both old values come back): an arc / multi-laser now leaves
        /// along the fire matrix tipped by the press's pitch toward the target point (`FUN_140258180` stores
        /// it at `+0x12f8c`, `FUN_1402514c0` applies it), so a Laser cast while one fighter is higher than the
        /// other flies a tipped arc.
        const GOLDEN: [(&str, u64); 7] = [
            ("weapons", 0x5fb3_1209_85f2_e51b),
            ("traps", 0x4560_fe5e_478b_9861),
            ("tricks", 0x039c_125a_7d43_f344),
            ("effects", 0x8650_9b64_a487_e55a),
            ("effects2", 0xe489_fe0d_1df9_09c0),
            ("effects3", 0xe676_41aa_1c13_b8dd),
            ("capsules", 0x33c7_dbbd_1818_b62f),
        ];

        #[test]
        fn one_v_one_replay_hash_is_unchanged() {
            let mut bad = Vec::new();
            for (name, golden) in GOLDEN {
                let (hash, _, cov) = run(name);
                eprintln!("replay {name}: {hash:#018x} coverage {cov:?} (hits, blocks, bursts, ko rounds, rounds, peak shots+areas, lock toggles, wins, captures)");
                if hash != golden {
                    bad.push(format!("`{name}`: {hash:#018x} (golden {golden:#018x})"));
                }
            }
            assert!(bad.is_empty(), "1v1 replay hash changed: {}", bad.join(", "));
        }

        /// With one enemy the target button has nothing to cycle to: pressing it
        /// throughout every scripted duel changes not one bit of the match
        /// (`0x1402531d0`: `pick(current + 1)` over a single valid slot).
        #[test]
        fn one_v_one_the_target_button_changes_nothing() {
            for (name, golden) in GOLDEN {
                let (hash, _, _) = match name {
                    "weapons" => play_with(0x1234_5678, 3000, [WEAPONS, TRAPS], Some([BLASTER, WAVE, SHELL, TIMER_MINE]), None, false, true),
                    "effects" => play_with(0x5eed_0001, 3600, [FX, FX], Some([115, 291, 294, 400]), None, false, true),
                    "effects2" => play_with(0x5eed_0002, 3600, [FX, WEAPONS], Some([77, 54, 236, 401]), Some(900), true, true),
                    "effects3" => play_with(0x5eed_0003, 3600, [FX, TRAPS], Some([93, 95, 96, 54]), Some(1200), true, true),
                    _ => continue,
                };
                assert_eq!(hash, golden, "`{name}` with switch-target presses");
            }
        }

        /// Movement only: nobody ever presses a skill button, and only what the
        /// movement lane approved is hashed (body, gait, pivot, dodge, landing,
        /// camera, lock; Health and action only for the fall-out stagger). A rules
        /// change (casting, payment, Aura, capsules) must not move a bit of it: this
        /// is the one golden that stays put for pure refactors, never re-recorded.
        /// Recorded on base 0952fcc, before lane CASTRULES touched anything.
        fn walk_only(seed: u32, ticks: u64) -> u64 {
            let mut col = arena();
            let mk = |a: &[i16]| a.iter().copied().cycle().take(30).collect::<Vec<i16>>();
            let combat = Combat::with_arsenals(extra_skills(), [mk(WEAPONS), mk(TRAPS)]);
            let mut m = Match::new([Vec3::new(-150.0, 0.0, 0.0), Vec3::new(150.0, 0.0, 0.0)], combat);
            m.jump_zones = vec![(Vec3::new(-20.0, 0.0, 100.0), Vec3::new(20.0, 0.0, 140.0), 150.0)];
            let mut script = Script::new(seed, 2);
            let mut h = H::new();
            for t in 0..ticks {
                let mut inputs = script.tick(t);
                for i in &mut inputs {
                    i.skills = 0;
                }
                m.step(inputs, &mut col);
                h.u(m.tick);
                for i in 0..2 {
                    let p = &m.players[i];
                    h.v3(p.pos);
                    h.v3(p.vel);
                    h.f(p.yaw);
                    h.f(p.cam_yaw);
                    h.f(p.land_timer);
                    h.f(p.heading());
                    h.u(p.grounded as u64);
                    h.u(p.locked as u64);
                    h.u(p.dodge_ticks as u64);
                    h.u(p.recover_ticks as u64);
                    h.s(&format!("{:?}|{:?}|{:?}", p.motion, p.gait, p.dodge));
                    if let Some(pv) = p.pivot() {
                        h.s(&format!("{:?}", pv.dir));
                        h.f(pv.time);
                    }
                    h.i(m.combat.fighters[i].health as i64);
                    h.s(&format!("{:?}", m.combat.fighters[i].action));
                }
            }
            h.0
        }

        #[test]
        fn one_v_one_movement_only_hash_is_unchanged() {
            // Lane REACTIONS moved seed 0xc0ffee ONLY (0x0dec0de1 is bit-identical): that script walks
            // off the arena, and fall-outs were never detected before (the respawn rebuilt `prev_pos`,
            // `Match::step` tested it) -- now it costs 3 Health, no stagger, 180 protected ticks and the
            // start yaw (`0x140251d80`). Verified by running the old fall-out rules with everything
            // else of the lane in: the old hash came back exactly.
            // Lane BODYRADIUS moved seed 0x0dec0de1 ONLY (7786f5d504194995 -> df1df45fa88ba861; 0xc0ffee is
            // bit-identical): a fighter now keeps 5.0 u from a wall, not 4.5 (`FUN_140255020` -> `FUN_1401959c0`:
            // the body radius is `size / 2` = 5, `body.rs`), and that script walks into the arena's centre wall.
            // Speeds, turning and the walk are untouched; the fighters never come within 10 u of each other here
            // (no push-apart fired), and returning the wall radius to 4.5 returns the old hash exactly (checked).
            const WALK_GOLDEN: [(u32, u64, u64); 2] = [(0x00c0_ffee, 3600, 0x5206_23f1_0353_79f7), (0x0dec_0de1, 3600, 0xdf1d_f45f_a88b_a861)];
            let mut bad = Vec::new();
            for (seed, ticks, golden) in WALK_GOLDEN {
                let hash = walk_only(seed, ticks);
                eprintln!("walk-only {seed:#x}: {hash:#018x}");
                if hash != golden {
                    bad.push(format!("seed {seed:#x}: {hash:#018x} (golden {golden:#018x})"));
                }
            }
            assert!(bad.is_empty(), "movement-only 1v1 hash changed: {}", bad.join(", "));
        }

        /// Two runs of the same script agree tick for tick (no hidden clocks), and the
        /// script actually fights -- or the hash would prove little.
        #[test]
        fn one_v_one_replay_is_reproducible_and_covers_the_rules() {
            let (h1, c1, cov) = run("weapons");
            let (h2, c2, _) = run("weapons");
            assert_eq!((h1, &c1), (h2, &c2));
            // (>= 4, was > 5: casts are now refused in a dodge, a landing or the air for a ground skill,
            // and an unlocked cast flies where the camera looks, so fewer land than before.)
            assert!(cov[0] >= 4 && cov[2] > 0, "weapons script lands hits and bursts: {cov:?}");
            assert!(cov[6] > 0, "and toggles lock-on: {cov:?}");
            // The knock-out and the next round are covered by the traps script now (the weapons
            // script no longer happens to end a round under the new aim and payment rules).
            let (_, _, traps) = run("traps");
            assert!(traps[4] >= 2 && traps[7] > 0, "traps script knocks someone out and starts another round: {traps:?}");
        }
    }

    // ---- The events lane: what each sound / effect event is logged for. ----
    // Each cites the program address its sound or effect is played from.

    fn logged(c: &Combat) -> Vec<CombatEvent> {
        c.events.iter().map(|e| e.1).collect()
    }

    /// `FUN_14014ee70` (the opening fill) is silent, `FUN_14014ed10` (state 0 -> 1,
    /// `0x14014ed6f`) is the only place the appear sound plays: the first capsule of
    /// each spot is `respawn: false`, the one that comes back after a pickup `true`.
    #[test]
    fn the_opening_fill_is_not_a_respawn_and_a_refill_is() {
        let (mut c, mut p, col) = setup(300.0);
        let spots = vec![Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 150.0)];
        c.set_capsule_spots([spots.clone(), vec![]]);
        c.fighters[0].arsenal = vec![LASER; 10];
        run(&mut c, &mut p, &col, [0; 2], 1);
        let appeared = |c: &Combat| logged(c).into_iter().filter_map(|e| if let CombatEvent::CapsuleAppeared { who: 0, pos, respawn } = e { Some((pos, respawn)) } else { None }).collect::<Vec<_>>();
        assert_eq!(appeared(&c), spots.iter().map(|&s| (s, false)).collect::<Vec<_>>(), "the opening deal: three silent appearances");
        c.capsules.retain(|k| k.pos != spots[1]); // captured
        run(&mut c, &mut p, &col, [0; 2], 2 * 420 + 4);
        let all = appeared(&c);
        assert_eq!(all.iter().filter(|a| a.1).count(), 1, "exactly one respawn");
        assert_eq!(all.last(), Some(&(spots[1], true)), "at the spot that was emptied");
    }

    /// `FUN_14024de20` (`0x14024df1d`): a tick whenever the whole-number part of Aura
    /// rises through regeneration; none when Aura is full or does not change.
    #[test]
    fn aura_regeneration_logs_one_tick_per_whole_point() {
        let (mut c, mut p, col) = setup(300.0);
        c.fighters[0].level = 5;
        c.fighters[0].aura_ticks = 0;
        c.fighters[1].level = 0;
        run(&mut c, &mut p, &col, [0; 2], 1200);
        let ticks = logged(&c).iter().filter(|e| matches!(e, CombatEvent::AuraTick { who: 0 })).count() as i32;
        assert_eq!(ticks, c.fighters[0].aura_ticks / AURA_TICKS_PER_POINT, "one per point gained");
        assert!(ticks >= 2, "regen is slow but 20 s gives several points: {ticks}");
        assert!(!logged(&c).iter().any(|e| matches!(e, CombatEvent::AuraTick { who: 1 })), "a player at their Level cap gains nothing");
        c.events.clear();
        c.fighters[0].aura_ticks = 5 * AURA_TICKS_PER_POINT;
        run(&mut c, &mut p, &col, [0; 2], 300);
        assert!(logged(&c).is_empty(), "full Aura: no tick");
    }

    /// `FUN_140254c80` (`0x140254e3f`, common/82) at the touch-down of a knock-down
    /// flight, `FUN_14025c3d0` (`0x14025c4ee`, common/81) as the get-up starts: one
    /// event each, landing first.
    #[test]
    fn a_knock_down_logs_its_landing_and_its_get_up_once() {
        let (mut c, mut p, col) = setup(150.0);
        c.set_reaction_row(AIR_KNOCKDOWN_ROW, ReactionRow { kind: reaction_kind::KNOCKDOWN, launch_speed: 20, launch_angle: 50, voice_group: 5, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
        p[1].grounded = false;
        cast_skill(&mut c, &mut p, &col, 0, RAIL);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 14);
        assert!(c.fighters[1].flying);
        run(&mut c, &mut p, &col, [0; 2], 70 + KNOCKDOWN_TICKS as usize);
        let seq: Vec<&str> = logged(&c).iter().filter_map(|e| match e {
            CombatEvent::KnockLanded { who: 1 } => Some("landed"),
            CombatEvent::GotUp { who: 1 } => Some("got up"),
            _ => None,
        }).collect();
        assert_eq!(seq, vec!["landed", "got up"]);
        assert!(!logged(&c).iter().any(|e| matches!(e, CombatEvent::KnockLanded { who: 0 } | CombatEvent::GotUp { who: 0 })));
    }

    /// The KO voice plays the moment a fighter falls, once, also mid-round.
    #[test]
    fn a_knock_out_logs_once_when_the_fighter_falls() {
        let (mut c, mut p, col) = setup(300.0);
        c.change_health(1, -999);
        assert_eq!(logged(&c).iter().filter(|e| matches!(e, CombatEvent::Knockout { who: 1 })).count(), 1);
        c.change_health(1, -999);
        run(&mut c, &mut p, &col, [0; 2], 30);
        assert_eq!(logged(&c).iter().filter(|e| matches!(e, CombatEvent::Knockout { .. })).count(), 1, "already down: no second voice");
    }

    /// `FUN_14017bcc0` (`0x14017bdc9`, common/93) when the crystal is set -- only if it
    /// takes effect (one crystal at a time) -- and `FUN_14017be00` (`0x14017bf70`,
    /// common/88) when it is taken.
    #[test]
    fn the_environment_crystal_logs_set_and_taken() {
        use super::testkit as k;
        let env_a = k::def(400, Capsule::Environment, 1, &[(43, 5), (44, 0), (45, 450), (46, -1)]);
        let env_b = k::def(401, Capsule::Environment, 1, &[(43, 5), (44, 0), (45, 450), (46, 1)]);
        let (mut c, mut p, col) = k::setup(vec![env_a, env_b], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 400);
        k::use_skill(&mut c, &mut p, &col, 1, 401);
        let sets = logged(&c).into_iter().filter(|e| matches!(e, CombatEvent::CrystalSet { .. })).collect::<Vec<_>>();
        assert_eq!(sets.len(), 1, "the refused second crystal makes no sound");
        assert!(matches!(sets[0], CombatEvent::CrystalSet { who: 0, .. }));
        p[1].pos = c.crystal.as_ref().unwrap().pos;
        c.crystal.as_mut().unwrap().age = CRYSTAL_CAPTURE_AFTER + 1;
        k::press(&mut c, &mut p, &col, 1, 1);
        assert!(logged(&c).iter().any(|e| matches!(e, CombatEvent::CrystalTaken { who: 1, .. })));
    }

    /// `FUN_14026dbd0` (`0x14026dca4`, common/99): a blast announces itself right
    /// before its `Burst`, with the radius the impact effect is scaled by.
    #[test]
    fn a_blast_logs_its_start_and_radius_before_its_burst() {
        let (mut c, mut p, col) = setup(100.0);
        cast_skill(&mut c, &mut p, &col, 0, BOMB);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 120);
        let ev = logged(&c);
        let at = ev.iter().position(|e| matches!(e, CombatEvent::BlastStarted { skill: BOMB, .. })).expect("blast logged");
        let CombatEvent::BlastStarted { radius, area, attacker, .. } = ev[at] else { unreachable!() };
        assert!(radius > 0.0 && !area && attacker == 0);
        assert!(matches!(ev[at + 1], CombatEvent::Burst { skill: BOMB, .. }));
    }

    /// `FUN_140259540` (the slot-4 charge cue starts at the row's charge-cycle tick) and
    /// `FUN_140259670` / `FUN_140259140` (it ends, slot 5 plays, on the release): one
    /// `ChargeHeld` after the 26-sim-tick wind-up (13 program ticks), one
    /// `ChargeReleased` when the button comes up.
    #[test]
    fn a_charge_logs_when_it_is_held_and_when_it_is_released() {
        const GUNGNIR: i16 = 73;
        let (mut c, mut p, col) = setup(100.0);
        c.skills.push(def(GUNGNIR, Capsule::Attack, 12, 0, 1, str_rule::AT_CAST, &[(37, 2), (P, -30), (P + 1, 100), (P + 4, 30)]));
        c.set_action_row(GUNGNIR, Some(ActionRow { anim_name: "ATK_SPIKE".into(), cast_sound: 0, charge_cycle: 13, dash_start: 0, dash_end: 0, fire_tick: 25, hit_window_end: 0 }), None);
        c.fighters[0].level = 3;
        c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(GUNGNIR);
        let count = |c: &Combat, held: bool| logged(c).iter().filter(|e| if held { matches!(e, CombatEvent::ChargeHeld { who: 0, skill: GUNGNIR }) } else { matches!(e, CombatEvent::ChargeReleased { who: 0, skill: GUNGNIR }) }).count();
        run(&mut c, &mut p, &col, [1, 0], 20);
        assert_eq!(count(&c, true), 0, "still winding up");
        run(&mut c, &mut p, &col, [1, 0], 20);
        assert_eq!((count(&c, true), count(&c, false)), (1, 0), "held once the wind-up is over");
        run(&mut c, &mut p, &col, [1, 0], 30);
        assert_eq!(count(&c, true), 1, "not again while it is held");
        run(&mut c, &mut p, &col, [0; 2], 4);
        assert_eq!(count(&c, false), 1, "released once");
    }

    /// LIVE (takes_full `073_Gungnir`, `204_Ki_Cannon`): the button is down 3 ticks (press tick 39,
    /// up at 42) and the shot still leaves at tick 64 = press + 25 table ticks = charge_cycle 13 +
    /// the row's fire_tick - charge_cycle (12). A release inside the wind-up waits the wind-up out.
    #[test]
    fn a_short_press_on_a_charge_skill_fires_at_the_rows_fire_tick_from_the_press() {
        const GUNGNIR: i16 = 73;
        let (mut c, mut p, col) = setup(100.0);
        c.skills.push(def(GUNGNIR, Capsule::Attack, 12, 0, 1, str_rule::AT_CAST, &[(37, 2), (P, -30), (P + 1, 100), (P + 4, 30)]));
        c.set_action_row(GUNGNIR, Some(ActionRow { anim_name: "ATK_SPIKE".into(), cast_sound: 0, charge_cycle: 13, dash_start: 0, dash_end: 0, fire_tick: 25, hit_window_end: 0 }), None);
        c.fighters[0].level = 3;
        c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(GUNGNIR);
        run(&mut c, &mut p, &col, [1, 0], 6); // 3 program ticks held
        run(&mut c, &mut p, &col, [0; 2], 40);
        assert!(c.projectiles.is_empty(), "not fired at press + 46 sim ticks");
        run(&mut c, &mut p, &col, [0; 2], 8);
        assert!(!c.projectiles.is_empty(), "fired by press + 54 sim ticks (25 program ticks = 50, +-2)");
        let held = logged(&c).iter().filter(|e| matches!(e, CombatEvent::ChargeHeld { who: 0, skill: GUNGNIR })).count();
        assert_eq!(held, 1, "the charge cue still starts on the charge-cycle tick");
    }

    /// Rain: every drop that starts to fall is a `RainDrop` event (slot 4 of the skill's
    /// effect, `FUN_14026b8d0`), so the effect has something to hang on.
    #[test]
    fn rain_drops_log_where_they_start() {
        let (mut c, mut p, col) = setup(150.0);
        cast_and_release(&mut c, &mut p, &col, RAIN);
        run(&mut c, &mut p, &col, [0; 2], 400);
        let drops = logged(&c).into_iter().filter(|e| matches!(e, CombatEvent::RainDrop { attacker: 0, skill: RAIN, .. })).count();
        assert!(drops > 0, "the cloud dropped something");
    }

    /// `FUN_140270b00` ends a status effect with `FUN_140271590(fighter, kind, amount)`:
    /// an Aura Particle used is kind 1 (Level), amount +1; a status capsule's own kind is
    /// its s45 and amount its s46; an attack or an erase skill logs none.
    #[test]
    fn using_an_aura_particle_or_a_status_capsule_logs_its_effect() {
        let se = |c: &Combat| logged(c).into_iter().filter_map(|e| if let CombatEvent::StatusEffect { who, kind, amount } = e { Some((who, kind, amount)) } else { None }).collect::<Vec<_>>();
        let (mut c, mut p, col) = setup(300.0);
        c.fighters[0].level = 3;
        c.fighters[0].aura_ticks = 0;
        c.fighters[0].slots[0] = Some(AURA_PARTICLE);
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 4);
        assert_eq!(se(&c), vec![(0, 1, 1)]);
        // A status capsule: Speed (kind 5) +20 -> the stat sound's "up" id; its kind and amount ride the event.
        let (mut c, mut p, col) = setup(300.0);
        c.skills.push(def(777, Capsule::Status, 70, 5, 0, 5, &[(43, 4), (45, 5), (49, 20)]));
        cast_skill(&mut c, &mut p, &col, 0, 777);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 40);
        assert_eq!(se(&c), vec![(0, 5, 20)]);
        let (mut c, mut p, col) = setup(300.0);
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 40);
        assert!(se(&c).is_empty(), "an attack has no status effect");
    }

    /// The self-status record (s43 = 4) is not only for the green capsule: Level Amp and Healing
    /// Water are specials, and every effect slot (kind s45+k, amount s49+k) is logged with the
    /// slot's own amount (the sound map then picks 89 / 90 / 95 / 96 ... from it; LIVE: Level Amp 89).
    #[test]
    fn a_special_with_a_self_status_record_logs_each_effect_slot() {
        let se = |c: &Combat| logged(c).into_iter().filter_map(|e| if let CombatEvent::StatusEffect { who, kind, amount } = e { Some((who, kind, amount)) } else { None }).collect::<Vec<_>>();
        let (mut c, mut p, col) = setup(300.0);
        c.skills.push(def(179, Capsule::Special, 70, 5, 0, 5, &[(43, 4), (45, 3), (49, 5)]));
        cast_skill(&mut c, &mut p, &col, 0, 179);
        run(&mut c, &mut p, &col, [0; 2], CAST_TICKS as usize + 40);
        assert_eq!(se(&c), vec![(0, 3, 5)]);
    }

    /// A press on a frozen button is refused with the same sound as a press that cannot
    /// pay (`FUN_140261e90`, common/2).
    #[test]
    fn a_press_on_a_frozen_button_is_denied() {
        let (mut c, mut p, col) = setup(300.0);
        c.fighters[0].slots[0] = Some(LASER);
        c.fighters[0].frozen_ticks = 600;
        press(&mut c, &mut p, &col, 0, 0);
        assert!(logged(&c).contains(&CombatEvent::Denied { who: 0, slot: 0 }));
        assert_eq!(c.fighters[0].action, Action::Ready, "and nothing is cast");
    }

    /// `FUN_14026cd70` cases 1-3 / 8: the impact effect is scaled by radius x 0.2 -- Bomb
    /// (s52 300 = 30 u) x6, Timer Mine (50 u) x10, Photon Wave (s64 400 = 40 u) x8 -- and
    /// a mine's or wave's blast is announced as an area blast (no s21 of its own).
    #[test]
    fn blasts_announce_the_radius_the_impact_effect_scales_by() {
        for (skill, dist, ticks, radius, scale, area) in [(BOMB, 100.0, CAST_TICKS as usize + 120, 30.0, 6.0, false), (TIMER_MINE, 40.0, 400, 50.0, 10.0, true), (PHOTON_WAVE, 30.0, CAST_TICKS as usize + 10, 40.0, 8.0, true)] {
            let (mut c, mut p, col) = setup(dist);
            if skill == BOMB {
                cast_skill(&mut c, &mut p, &col, 0, skill);
            } else {
                cast_and_release(&mut c, &mut p, &col, skill);
            }
            run(&mut c, &mut p, &col, [0; 2], ticks);
            let b = logged(&c).into_iter().find_map(|e| if let CombatEvent::BlastStarted { skill: s, radius, area, .. } = e { (s == skill).then_some((radius, area)) } else { None });
            let (r, a) = b.unwrap_or_else(|| panic!("skill {skill}: no blast logged"));
            assert!((r - radius).abs() < 0.01 && a == area, "skill {skill}: radius {r} area {a}");
            assert!((crate::fx::impact_scale(r) - scale).abs() < 0.01, "skill {skill}: scale");
        }
    }

    /// `FUN_1402626d0` (`0x140262caa`): `SkillFired` at the fire tick -- not at the
    /// cast -- so s19 can play there. Laser fires at sim tick 26.
    #[test]
    fn a_skill_logs_its_fire_tick() {
        let (mut c, mut p, col) = setup(300.0);
        c.set_action_row(LASER, Some(laser_row()), Some(laser_air_row()));
        cast_skill(&mut c, &mut p, &col, 0, LASER);
        run(&mut c, &mut p, &col, [0; 2], 24);
        assert!(!logged(&c).iter().any(|e| matches!(e, CombatEvent::SkillFired { .. })), "not at the cast");
        run(&mut c, &mut p, &col, [0; 2], 1);
        assert_eq!(logged(&c).iter().filter(|e| matches!(e, CombatEvent::SkillFired { who: 0, skill: LASER })).count(), 1);
        assert!(!c.projectiles.is_empty());
    }
}

/// Lane GUARDS: the guard lifecycle and the shared rules around it, each pinned
/// to the program address it was read from (`PDUWP.exe`, decompile in
/// `~/.pdtools/re-out/decomp_all.c`).
#[cfg(test)]
mod guard_rules {
    use super::barrier::{self, kind};
    use super::testkit::{self as k, f};
    use super::*;

    const WALL: i16 = 500;
    const SHELTER: i16 = 501;
    const BRUSH: i16 = 502;
    const REFLECT: i16 = 503;
    const ABSORB: i16 = 504;
    const COURSE: i16 = 505;
    const REVERSE: i16 = 506;
    const AUTO: i16 = 507;
    const ZAP: i16 = 600; // STR 1 shell
    const SLAM: i16 = 601; // STR 2 shell
    const MINE: i16 = 602; // motion 8
    const AROUND: i16 = 603; // motion 9
    const LOB: i16 = 604; // motion 1
    const RICO: i16 = 605; // motion 17
    const PUNCH: i16 = 606; // motion 13
    const SHEARING: i16 = 607; // motion 4 instant
    const PIERCER: i16 = 608; // s54 = 2
    const SPEAR: i16 = 609; // s54 = 1
    const RAINDROP: i16 = 610; // motion 20
    const TORNADO: i16 = 611; // motion 10

    fn guard(id: i16, kind: i16, def: i16, extra: &[(usize, i16)]) -> SkillDef {
        let mut fields = vec![(43, 2), (f::DEFENCE_KIND, kind), (f::STRENGTH, 28), (f::DEFENCE, def)];
        fields.extend_from_slice(extra);
        k::def(id, Capsule::Defence, 1, &fields)
    }

    fn attack(id: i16, motion: i16, extra: &[(usize, i16)]) -> SkillDef {
        let mut fields = vec![(f::MOTION, motion), (f::STRENGTH, 1)];
        fields.extend_from_slice(extra);
        k::def(id, Capsule::Attack, 1, &fields)
    }

    fn skills() -> Vec<SkillDef> {
        vec![
            guard(WALL, kind::BARRIER, 3, &[]),
            guard(SHELTER, kind::SHELTER, 3, &[]),
            guard(BRUSH, kind::BRUSH, 3, &[]),
            guard(REFLECT, kind::REFLECT, 3, &[]),
            guard(ABSORB, kind::ABSORB, 3, &[]),
            guard(COURSE, kind::COURSE, 0, &[(f::STRENGTH_RULE, 800)]),
            guard(REVERSE, kind::REVERSE, 0, &[]),
            guard(AUTO, kind::AUTO_SHELTER, 5, &[(f::STRENGTH, 10)]),
            attack(ZAP, 0, &[]),
            attack(SLAM, 0, &[(f::STRENGTH, 2)]),
            attack(MINE, 8, &[]),
            attack(AROUND, 9, &[]),
            attack(LOB, 1, &[]),
            attack(RICO, 17, &[]),
            attack(PUNCH, 13, &[]),
            attack(SHEARING, 4, &[]),
            attack(PIERCER, 0, &[(f::PIERCE, 2), (f::STRENGTH, 9)]),
            attack(SPEAR, 0, &[(f::PIERCE, 1), (f::STRENGTH, 5)]),
            attack(RAINDROP, 20, &[]),
            attack(TORNADO, 10, &[]),
        ]
    }

    /// Player 1 holds `guard_id` on button `button`; returns the setup.
    fn holder(guard_id: i16, button: usize) -> (Combat, [Player; 2]) {
        let (mut c, p, _) = k::setup(skills(), 100.0);
        let g = barrier::Guard::from_def(c.skill(guard_id).unwrap()).unwrap();
        c.fighters[1].slots[button] = Some(guard_id);
        c.fighters[1].guard = Some(ActiveGuard::new(guard_id, g, button));
        (c, p)
    }

    /// Attacker 0 hits player 1 with `id` from in front, as a shot.
    fn hit(c: &mut Combat, p: &[Player; 2], id: i16, arrival: Arrival) -> bool {
        let str = c.skill(id).unwrap().strength;
        let bodies = bodies_of(p);
        c.strike(0, 1, id, Some(p[0].pos), bodies, arrival, Via { button: 0, strength: Some(str) })
    }

    fn blocks(c: &Combat) -> usize {
        c.events.iter().filter(|e| matches!(e.1, CombatEvent::Blocked { .. })).count()
    }

    #[test]
    fn a_block_restarts_the_holders_cast_clip_at_w44_and_ends_it_length_minus_w44_plus_one_ticks_later() {
        // `FUN_140271760` -> holder vtable `+0x88` = `FUN_14024ff20`: state 10 with the timer
        // `clip length - w44` (LIVE: DEF_SHIELD / DEF_SHELTER guards stopping a Blaster end press + 38
        // instead of press + 41). Here clip 40 program ticks, w44 = 30: 10 + 1 program ticks left.
        let (mut c, p) = holder(WALL, 1);
        c.set_clip_ticks([HashMap::new(), HashMap::from([("DEF_SHIELD".to_string(), 80u16)])]);
        let row = ActionRow { anim_name: "DEF_SHIELD".into(), cast_sound: 0, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 2, hit_window_end: 30 };
        c.action_rows.insert(WALL, (Some(row.clone()), Some(row)));
        let f = &mut c.fighters[1];
        f.action = Action::Cast;
        f.cast_skill = WALL;
        f.action_ticks = 60;
        assert!(!hit(&mut c, &p, ZAP, Arrival::Shot));
        assert_eq!(c.fighters[1].action_ticks, (80 - 60) + 2, "clip 40 - w44 30 = 10 program ticks, +1");
    }

    #[test]
    fn each_block_takes_the_attacks_str_off_the_guard_and_the_fourth_one_breaks_it() {
        // `FUN_140271760`: `guard.STR -= attack.STR`; below zero the guard breaks.
        // PROGRAM-RE: "Octolaser's STR-1 shots break a DEF-3 wall on the 4th".
        let (mut c, p) = holder(WALL, 1);
        for n in 1..=3 {
            assert!(!hit(&mut c, &p, ZAP, Arrival::Shot), "block {n}: no damage");
            assert_eq!(c.fighters[1].guard.map(|g| g.guard.def), Some(3 - n), "DEF left after block {n}");
        }
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert!(!hit(&mut c, &p, ZAP, Arrival::Shot), "STR 1 > DEF 0: the wall breaks and the shot is stopped too");
        assert!(c.fighters[1].guard.is_none(), "destroyed at once");
        assert_eq!(c.fighters[1].health, START_HEALTH);
        assert!(hit(&mut c, &p, ZAP, Arrival::Shot), "no guard left: it lands");
        assert_eq!(blocks(&c), 4);
    }

    #[test]
    fn a_block_ends_the_guard_five_program_ticks_later_and_only_the_first_block_starts_it() {
        // `FUN_14026cd70(guard, 9)`: state 6, life 5, only if not already in state 6.
        let (mut c, mut p) = holder(WALL, 1);
        c.fighters[1].guard.as_mut().unwrap().ticks = 500;
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!(c.fighters[1].guard.map(|g| (g.lingering, g.ticks)), Some((true, barrier::LINGER_TICKS)), "5 program ticks = 10 sim ticks");
        let col = k::floor();
        k::run(&mut c, &mut p, &col, 4);
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!(c.fighters[1].guard.map(|g| g.ticks), Some(barrier::LINGER_TICKS - 4), "a second block does not restart the 5 ticks");
        k::run(&mut c, &mut p, &col, barrier::LINGER_TICKS as usize);
        assert!(c.fighters[1].guard.is_none(), "gone after the linger");
    }

    #[test]
    fn a_broken_defence_is_erased_from_its_button() {
        // `FUN_140271760` -> `FUN_140262fc0`: the guard's own skill leaves its button
        // (and is "consumed", `FUN_140263570`).
        let (mut c, p) = holder(WALL, 2);
        hit(&mut c, &p, SLAM, Arrival::Shot); // STR 2 vs DEF 3: blocked
        assert_eq!(c.fighters[1].slots[2], Some(WALL));
        hit(&mut c, &p, SLAM, Arrival::Shot); // DEF 1 < 2: breaks
        assert!(c.fighters[1].guard.is_none());
        assert_eq!(c.fighters[1].slots[2], None, "gone from its button");
        assert_eq!(c.fighters[1].used, vec![WALL], "consumed");
    }

    #[test]
    fn a_broken_retain_defence_is_paid_for_again_and_stays_while_aura_covers_it() {
        // `FUN_140262fc0`: s41 = 1 and `FUN_1402620e0` true -> `FUN_1402621f0` (pay), keep.
        let (mut c, p) = holder(WALL, 0);
        c.skills.iter_mut().find(|d| d.id == WALL).unwrap().raw[41] = 1;
        c.skills.iter_mut().find(|d| d.id == WALL).unwrap().cost = 2;
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 5 * AURA_TICKS_PER_POINT;
        hit(&mut c, &p, SPEAR, Arrival::Shot); // STR 5 > DEF 3: breaks
        assert_eq!(c.fighters[1].slots[0], Some(WALL), "kept");
        assert_eq!(c.fighters[1].aura(), 3, "5 - its cost 2, paid again now");
        c.fighters[1].guard = Some(ActiveGuard::new(WALL, barrier::Guard::from_def(c.skill(WALL).unwrap()).unwrap(), 0));
        c.fighters[1].aura_ticks = AURA_TICKS_PER_POINT; // cannot cover it
        hit(&mut c, &p, SPEAR, Arrival::Shot);
        assert_eq!(c.fighters[1].slots[0], None, "no Aura to pay again: erased");
    }

    #[test]
    fn a_locked_skill_cannot_be_erased_by_a_break() {
        // `FUN_140262fc0`: s41 = 2 -> nothing happens to the button.
        let (mut c, p) = holder(WALL, 1);
        c.skills.iter_mut().find(|d| d.id == WALL).unwrap().raw[41] = 2;
        hit(&mut c, &p, SPEAR, Arrival::Shot);
        assert!(c.fighters[1].guard.is_none());
        assert_eq!(c.fighters[1].slots[1], Some(WALL));
    }

    #[test]
    fn a_hit_does_not_drop_a_standing_guard() {
        // `0x140264340` updates the guard by its own countdown only. Rain goes through a Barrier.
        let (mut c, p) = holder(WALL, 1);
        assert!(hit(&mut c, &p, RAINDROP, Arrival::Area), "a Barrier never stops Rain");
        assert!(c.fighters[1].health < START_HEALTH);
        assert!(c.fighters[1].guard.is_some(), "the guard stands on");
    }

    #[test]
    fn the_guards_own_numbers_set_its_time_and_its_def() {
        // Countdown starts at s46 - 2 and ends at 0: s46 - 1 program ticks (`0x140264020`).
        let d = guard(WALL, kind::BARRIER, 3, &[]);
        assert_eq!(barrier::Guard::from_def(&d).unwrap().ticks, 2 * 27);
        // DEF = s47 + s48 x uses (`FUN_140272400`): Failing Wall 7 - uses, Dark Protection 3 + uses.
        let failing = guard(WALL, kind::BARRIER, 7, &[(48, -1)]);
        let g = barrier::Guard::from_def(&failing).unwrap();
        assert_eq!((g.with_uses(&failing, 0).def, g.with_uses(&failing, 2).def), (7, 5));
        let dark = guard(WALL, kind::BARRIER, 3, &[(48, 1)]);
        assert_eq!(barrier::Guard::from_def(&dark).unwrap().with_uses(&dark, 3).def, 6);
    }

    fn outcome(guard_id: i16, attack_id: i16, arrival: Arrival, front: bool) -> barrier::Outcome {
        let c = k::setup(skills(), 100.0).0;
        let g = barrier::Guard::from_def(c.skill(guard_id).unwrap()).unwrap();
        barrier::resolve(&g, c.skill(attack_id).unwrap().strength, c.skill(attack_id).unwrap(), arrival, front, false)
    }

    #[test]
    fn what_each_guard_kind_never_stops() {
        use barrier::Outcome::Through;
        // Barrier and Reflect: Rain (7, 20) and Tornado (10) only.
        for g in [WALL, REFLECT] {
            assert_eq!(outcome(g, RAINDROP, Arrival::Area, true), Through, "guard {g}: Rain");
            assert_eq!(outcome(g, TORNADO, Arrival::Shot, true), Through, "guard {g}: Tornado");
            assert!(matches!(outcome(g, MINE, Arrival::Area, true), barrier::Outcome::Blocked { .. }), "guard {g} does stop a mine");
            assert!(matches!(outcome(g, PUNCH, Arrival::Melee, true), barrier::Outcome::Blocked { .. }), "guard {g} does stop a punch");
        }
        // Absorb: close range, Rain, Tornado, mines and around-self too.
        for a in [PUNCH, RAINDROP, TORNADO, MINE, AROUND] {
            assert_eq!(outcome(ABSORB, a, if a == PUNCH { Arrival::Melee } else { Arrival::Area }, true), Through, "Absorb vs {a}");
        }
        assert!(matches!(outcome(ABSORB, ZAP, Arrival::Shot, true), barrier::Outcome::Blocked { .. }));
        // Brush: Rain, Tornado, mines, around-self -- but it does take close-range attacks.
        for a in [RAINDROP, TORNADO, MINE, AROUND] {
            assert_eq!(outcome(BRUSH, a, Arrival::Area, true), Through, "Brush vs {a}");
        }
        assert!(matches!(outcome(BRUSH, PUNCH, Arrival::Melee, true), barrier::Outcome::Blocked { effect: barrier::BlockEffect::Brush, .. }));
        // Shelter stops all of them.
        for a in [RAINDROP, TORNADO, MINE, AROUND, ZAP] {
            assert!(matches!(outcome(SHELTER, a, Arrival::Area, false), barrier::Outcome::Blocked { .. }), "Shelter vs {a}");
        }
    }

    #[test]
    fn front_guards_cover_the_front_only_but_absorb_is_a_sphere() {
        use barrier::Outcome::Through;
        assert_eq!(outcome(WALL, ZAP, Arrival::Shot, false), Through, "behind a Barrier");
        assert_eq!(outcome(BRUSH, ZAP, Arrival::Shot, false), Through);
        assert!(matches!(outcome(ABSORB, ZAP, Arrival::Shot, false), barrier::Outcome::Blocked { .. }), "`FUN_14026dcb0` tests Absorb as a sphere");
        assert!(matches!(outcome(SHELTER, ZAP, Arrival::Shot, false), barrier::Outcome::Blocked { .. }));
    }

    #[test]
    fn s54_2_passes_barriers_and_shelters_but_not_course_or_about_face() {
        use barrier::Outcome::*;
        for g in [WALL, SHELTER, BRUSH, REFLECT, ABSORB] {
            assert_eq!(outcome(g, PIERCER, Arrival::Shot, true), Through, "guard {g}");
        }
        assert_eq!(outcome(REVERSE, PIERCER, Arrival::Shot, true), Turned { back: true });
        assert_eq!(outcome(COURSE, PIERCER, Arrival::Shot, true), Turned { back: false });
        // Necronomicon (status 31) cancels it.
        let c = k::setup(skills(), 100.0).0;
        let g = barrier::Guard::from_def(c.skill(WALL).unwrap()).unwrap();
        assert!(matches!(barrier::resolve(&g, 9, c.skill(PIERCER).unwrap(), Arrival::Shot, true, true), Blocked { broken: true, .. }));
    }

    #[test]
    fn about_face_only_turns_shells_lobs_falls_crawlers_objects_and_arcs() {
        use barrier::Outcome::*;
        for m in [0, 1, 2, 5, 6, 11] {
            let mut c = k::setup(skills(), 100.0).0;
            c.skills.push(attack(700, m, &[]));
            let g = barrier::Guard::from_def(c.skill(REVERSE).unwrap()).unwrap();
            assert_eq!(barrier::resolve(&g, 1, c.skill(700).unwrap(), Arrival::Shot, true, false), Turned { back: true }, "motion {m}");
        }
        assert_eq!(outcome(REVERSE, RICO, Arrival::Shot, true), Through, "a Ricochet Laser (17) is not reversed");
        assert_eq!(outcome(REVERSE, PUNCH, Arrival::Melee, true), Through);
    }

    #[test]
    fn about_face_spends_no_def_and_reflect_does() {
        let (mut c, p) = holder(REVERSE, 1);
        let def_before = c.fighters[1].guard.unwrap().guard.def;
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!(c.fighters[1].guard.unwrap().guard.def, def_before);
        assert!(!c.fighters[1].guard.unwrap().lingering, "no linger either");
        let (mut c, p) = holder(REFLECT, 1);
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!(c.fighters[1].guard.unwrap().guard.def, 2, "Reflect is a block: DEF - STR");
        assert!(c.fighters[1].guard.unwrap().lingering);
        assert_eq!(c.projectiles.len(), 1, "and the shot goes home");
    }

    #[test]
    fn reflect_sends_back_only_shells_and_arcs() {
        use barrier::{BlockEffect, Outcome::Blocked};
        assert!(matches!(outcome(REFLECT, ZAP, Arrival::Shot, true), Blocked { effect: BlockEffect::Reflect, .. }));
        assert!(matches!(outcome(REFLECT, LOB, Arrival::Shot, true), Blocked { effect: BlockEffect::Stop, .. }), "a lob is just stopped");
        assert!(matches!(outcome(REFLECT, ZAP, Arrival::Shot, true), Blocked { broken: false, .. }));
        let c = k::setup(skills(), 100.0).0;
        let g = barrier::Guard::from_def(c.skill(REFLECT).unwrap()).unwrap();
        assert!(matches!(barrier::resolve(&g, 9, c.skill(ZAP).unwrap(), Arrival::Shot, true, false), Blocked { broken: true, effect: BlockEffect::Stop, .. }), "a broken Reflect reflects nothing");
    }

    #[test]
    fn a_brush_stuns_a_close_range_attacker_for_a_second_even_when_it_breaks() {
        // code 7 on a melee attacker = reaction 26, 30 frames (`FUN_140271760`, `FUN_14026cd70`).
        assert_eq!(barrier::BRUSH_STUN_TICKS, 60);
        let (mut c, p) = holder(BRUSH, 1);
        c.skills.iter_mut().find(|d| d.id == PUNCH).unwrap().strength = 9;
        let bodies = bodies_of(&p);
        c.strike(0, 1, PUNCH, Some(p[0].pos), bodies, Arrival::Melee, Via { button: 0, strength: Some(9) });
        assert!(c.fighters[1].guard.is_none(), "broken");
        assert_eq!(c.fighters[0].action, Action::Hit, "the puncher is stunned all the same");
    }

    #[test]
    fn guard_add_effects_follow_the_program() {
        // Add 3 erases the attack skill from the button it was cast from, only for
        // attack-colour skills, and a guard with s50 = 1 only for short-range attacks.
        let mut sk = skills();
        sk.push(guard(510, kind::BARRIER, 9, &[(f::ADD_EFFECT, 3)]));
        sk.push(guard(511, kind::BARRIER, 9, &[(f::ADD_EFFECT, 3), (50, 1)]));
        sk.push(guard(512, kind::BARRIER, 9, &[(f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]));
        sk.push(guard(513, kind::BARRIER, 9, &[(f::ADD_EFFECT, 7)]));
        let setup = |g: i16| {
            let (mut c, p, _) = k::setup(sk.clone(), 100.0);
            let gd = barrier::Guard::from_def(c.skill(g).unwrap()).unwrap();
            c.fighters[1].slots[3] = Some(g);
            c.fighters[1].guard = Some(ActiveGuard::new(g, gd, 3));
            c.fighters[0].slots = [None, Some(ZAP), Some(PUNCH), None];
            (c, p)
        };
        let bodies = |p: &[Player; 2]| bodies_of(p);
        let (mut c, p) = setup(510);
        let b = bodies(&p);
        c.strike(0, 1, ZAP, Some(p[0].pos), b, Arrival::Shot, Via { button: 1, strength: Some(1) });
        assert_eq!(c.fighters[0].slots[1], None, "add 3: the shot's own button is erased (not another)");
        assert_eq!(c.fighters[0].slots[2], Some(PUNCH));
        // s50 = 1: short-range only.
        let (mut c, p) = setup(511);
        let b = bodies(&p);
        c.strike(0, 1, ZAP, Some(p[0].pos), b, Arrival::Shot, Via { button: 1, strength: Some(1) });
        assert_eq!(c.fighters[0].slots[1], Some(ZAP), "a shot does not trigger it");
        c.strike(0, 1, PUNCH, Some(p[0].pos), b, Arrival::Melee, Via { button: 2, strength: Some(1) });
        assert_eq!(c.fighters[0].slots[2], None, "a punch does");
        // Add 6 poisons the attacker for s52 ticks.
        let (mut c, p) = setup(512);
        let b = bodies(&p);
        c.strike(0, 1, ZAP, Some(p[0].pos), b, Arrival::Shot, Via { button: 1, strength: Some(1) });
        assert_eq!(c.fighters[0].poison_ticks, 180, "90 program ticks");
        // Add 7 captures the blocked skill onto the guard's own button.
        let (mut c, p) = setup(513);
        let b = bodies(&p);
        c.strike(0, 1, ZAP, Some(p[0].pos), b, Arrival::Shot, Via { button: 1, strength: Some(1) });
        assert_eq!(c.fighters[1].slots[3], Some(ZAP), "on the guard's button, replacing it");
    }

    #[test]
    fn a_guard_with_s50_1_applies_its_add_effect_to_short_range_attacks_only_even_when_it_breaks() {
        // The program checks s50 for every Add code (`FUN_140271ed0`'s first test), and the Add also runs on a break.
        let mut sk = skills();
        sk.push(guard(511, kind::BARRIER, 0, &[(f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90), (50, 1)]));
        let (mut c, p, _) = k::setup(sk, 100.0);
        let gd = barrier::Guard::from_def(c.skill(511).unwrap()).unwrap();
        c.fighters[1].guard = Some(ActiveGuard::new(511, gd, 0));
        let b = bodies_of(&p);
        c.strike(0, 1, ZAP, Some(p[0].pos), b, Arrival::Shot, Via { button: 0, strength: Some(1) });
        assert!(c.fighters[1].guard.is_none(), "STR 1 > DEF 0: broken");
        assert_eq!(c.fighters[0].poison_ticks, 0, "a shot: not short-range");
    }

    #[test]
    fn automatic_shelter_ignores_instants_and_penetrating_attacks() {
        // `FUN_14026f530`: not for an attack with s54 = 2 (unless status 31) nor for a motion-4 instant.
        for (attack_id, goes_up) in [(ZAP, true), (SHEARING, false), (PIERCER, false)] {
            let (mut c, p, _) = k::setup(skills(), 100.0);
            c.fighters[1].slots[0] = Some(AUTO);
            c.fighters[1].level = 8;
            c.fighters[1].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            let _ = hit(&mut c, &p, attack_id, Arrival::Shot);
            // It went up if it was paid for (cost 1) and used: nothing stands afterwards (kind 7 ends at its first update).
            assert_eq!(c.fighters[1].aura() < 8, goes_up, "attack {attack_id}");
        }
    }

    #[test]
    fn a_special_with_s44_7_is_not_an_automatic_shelter() {
        // Surprise Attack's s44 = 7 is its own kind; only a defence record carries a guard kind.
        let sneaky = k::def(77, Capsule::Special, 2, &[(43, 3), (44, 7)]);
        assert!(barrier::Guard::from_def(&sneaky).is_none());
    }

    #[test]
    fn a_defence_holds_its_caster_for_the_clip_only() {
        // The guard follows the holder on its own countdown (`0x140264340`); the caster is
        // released when the cast clip ends, not when the guard does.
        let (mut c, mut p, col) = k::setup(skills(), 100.0);
        c.set_action_row(WALL, Some(ActionRow { anim_name: "DEF".into(), cast_sound: 0, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 2, hit_window_end: 0 }), None);
        k::use_skill(&mut c, &mut p, &col, 0, WALL);
        assert!(c.fighters[0].guard.is_some(), "the barrier is up");
        k::run(&mut c, &mut p, &col, 10);
        assert_eq!(c.fighters[0].action, Action::Ready, "free to move while it stands");
        assert!(c.fighters[0].guard.is_some());
    }

    #[test]
    fn a_defence_goes_up_at_the_rows_fire_tick() {
        let (mut c, mut p, col) = k::setup(skills(), 100.0);
        c.set_action_row(WALL, Some(ActionRow { anim_name: "DEF".into(), cast_sound: 0, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 2, hit_window_end: 0 }), None);
        c.fighters[0].slots[0] = Some(WALL);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.step(&mut p, [1, 0], &col);
        assert!(c.fighters[0].guard.is_none(), "not yet");
        k::run(&mut c, &mut p, &col, 5);
        assert!(c.fighters[0].guard.is_some(), "fire tick 2 program ticks = 4 sim ticks");
    }

    #[test]
    fn a_defences_def_uses_the_slots_earlier_uses() {
        let (mut c, mut p, col) = k::setup({ let mut s = skills(); s.push(guard(520, kind::BARRIER, 3, &[(48, 1)])); s }, 100.0);
        c.fighters[0].level = 8;
        for expected in [3, 4] {
            k::run(&mut c, &mut p, &col, 60); // the last cast has played out
            c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            c.fighters[0].slots[0] = Some(520);
            c.fighters[0].guard = None;
            k::press(&mut c, &mut p, &col, 0, 0);
            k::run(&mut c, &mut p, &col, 3);
            assert_eq!(c.fighters[0].guard.map(|g| g.guard.def), Some(expected), "DEF = s47 + s48 x uses");
        }
    }

    // ---- shared rules around the guard lifecycle ------------------------------

    #[test]
    fn hitting_the_body_ends_optical_camo_and_a_status_float() {
        // `FUN_14026f870` tail: statuses 8 and 11 are cleared by whatever reaches the body.
        let (mut c, p, _) = k::setup(skills(), 100.0);
        c.fighters[1].status.invisible_ticks = 500;
        c.fighters[1].status.hover_ticks = 500;
        c.fighters[1].status_hover = true;
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!((c.fighters[1].status.invisible_ticks, c.fighters[1].status.hover_ticks), (0, 0));
        // Levitate / Lift Up are not status 11: a hit leaves that hover alone.
        let (mut c, p, _) = k::setup(skills(), 100.0);
        c.fighters[1].status.hover_ticks = 500;
        hit(&mut c, &p, ZAP, Arrival::Shot);
        assert_eq!(c.fighters[1].status.hover_ticks, 500);
    }

    #[test]
    fn necronomicon_switches_off_attack_add_effects_and_pierce() {
        let nec = k::def(287, Capsule::Environment, 1, &[(43, 5), (44, 31), (45, 450), (46, 1)]);
        let mut sk = skills();
        sk.push(nec);
        sk.push(attack(620, 0, &[(f::ADD_EFFECT, 7), (f::ADD_AMOUNT, 300)])); // freezes buttons
        let (mut c, p, _) = k::setup(sk, 100.0);
        hit(&mut c, &p, 620, Arrival::Shot);
        assert_eq!(c.fighters[1].frozen_ticks, 600, "normally the Add effect lands");
        c.fighters[1].frozen_ticks = 0;
        c.crystal = Some(Crystal { id: 1, owner: 0, skill: 287, pos: Vec3::ZERO, age: 0 });
        c.fighters[1].health = START_HEALTH;
        hit(&mut c, &p, 620, Arrival::Shot);
        assert_eq!(c.fighters[1].frozen_ticks, 0, "under Necronomicon it does not");
        assert!(c.fighters[1].health < START_HEALTH, "the damage still does");
    }

    #[test]
    fn retain_pays_again_when_its_last_use_is_taken() {
        // `FUN_140263460`: uses exhausted + s41 = 1 + Aura covers it -> pay again, keep.
        let mut sk = skills();
        sk.push(k::def(630, Capsule::Attack, 2, &[(f::MOTION, 0), (f::STRENGTH, 1), (f::USES, 1), (41, 1)]));
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        c.fighters[0].arsenal.clear();
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(630);
        k::press(&mut c, &mut p, &col, 0, 0);
        k::run(&mut c, &mut p, &col, 40);
        assert_eq!(c.fighters[0].slots[0], Some(630), "kept");
        assert!(c.fighters[0].aura() <= 8 - 4 + 1, "its cost was paid twice: {}", c.fighters[0].aura());
        // Necronomicon switches Retain off for attack skills.
        let nec = k::def(287, Capsule::Environment, 1, &[(43, 5), (44, 31), (45, 450), (46, 1)]);
        let mut sk2 = skills();
        sk2.push(k::def(630, Capsule::Attack, 2, &[(f::MOTION, 0), (f::STRENGTH, 1), (f::USES, 1), (41, 1)]));
        sk2.push(nec);
        let (mut c, mut p, col) = k::setup(sk2, 100.0);
        c.fighters[0].arsenal.clear();
        c.crystal = Some(Crystal { id: 1, owner: 1, skill: 287, pos: Vec3::new(500.0, 0.0, 500.0), age: 0 });
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(630);
        k::press(&mut c, &mut p, &col, 0, 0);
        k::run(&mut c, &mut p, &col, 40);
        assert_eq!(c.fighters[0].slots[0], None, "status 31: no Retain");
    }

    #[test]
    fn memory_lapse_empties_the_button_after_any_use_unless_it_cannot_be_erased() {
        // `FUN_140263410`: status 32 -> uses = 0 unless s41 = 2.
        let lapse = k::def(226, Capsule::Environment, 1, &[(43, 5), (44, 32), (45, 450), (46, 1)]);
        let mut sk = skills();
        sk.push(lapse);
        sk.push(k::def(631, Capsule::Attack, 1, &[(f::MOTION, 0), (f::STRENGTH, 1)])); // unlimited
        sk.push(k::def(632, Capsule::Attack, 1, &[(f::MOTION, 0), (f::STRENGTH, 1), (41, 2)])); // locked
        let (mut c, mut p, col) = k::setup(sk, 100.0);
        c.fighters[0].arsenal.clear();
        c.crystal = Some(Crystal { id: 1, owner: 1, skill: 226, pos: Vec3::new(500.0, 0.0, 500.0), age: 0 });
        for (skill, left) in [(631, None), (632, Some(632))] {
            c.fighters[0].level = 8;
            c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
            c.fighters[0].slots[0] = Some(skill);
            k::press(&mut c, &mut p, &col, 0, 0);
            k::run(&mut c, &mut p, &col, 40);
            assert_eq!(c.fighters[0].slots[0], left, "skill {skill}");
        }
    }

    #[test]
    fn a_mine_takes_its_use_when_it_goes_off_not_when_it_is_set() {
        // `FUN_1402626d0` skips the use for motion 8; `FUN_14026cd70` code 8 -> `FUN_1402635b0` takes it.
        let (mut c, mut p, col) = k::setup({ let mut s = skills(); s.push(k::def(640, Capsule::Attack, 1, &[(f::MOTION, 8), (f::STRENGTH, 1), (f::USES, 1), (66, 60), (67, 500)])); s }, 100.0);
        c.fighters[0].arsenal.clear();
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(640);
        k::press(&mut c, &mut p, &col, 0, 0);
        k::run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        assert!(!c.areas.is_empty(), "the mine is set");
        assert_eq!(c.fighters[0].slots[0], Some(640), "still on its button while it waits");
        k::run(&mut c, &mut p, &col, 400);
        assert!(c.areas.is_empty(), "it has gone off");
        assert_eq!(c.fighters[0].slots[0], None, "used up now");
    }

    #[test]
    fn crystal_capture_through_the_whole_match_loop() {
        // The same rule driven by `Match::step` with real inputs: set a crystal with button 0,
        // wait out the 15 s lock standing on it, press button 1: that button becomes an Aura Particle.
        use crate::sim::{Input, Match};
        let env = k::def(163, Capsule::Environment, 2, &[(43, 5), (44, 3), (45, 450), (46, -30)]);
        let mut combat = Combat::new(vec![env, attack(ZAP, 0, &[])], vec![AURA_PARTICLE; 30]);
        combat.set_rules(k::TEST_RULES);
        combat.fighters[0].slots = [Some(163), Some(ZAP), None, None];
        combat.fighters[0].level = 8;
        combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        combat.fighters[0].arsenal.clear();
        combat.fighters[1].arsenal.clear();
        let mut m = Match::new([Vec3::new(-150.0, 0.0, 0.0), Vec3::new(150.0, 0.0, 0.0)], combat);
        let mut col = k::floor();
        let press = |slot: u8| [Input { skills: 1 << slot, ..Default::default() }, Input::default()];
        for _ in 0..COUNTDOWN_TICKS + 2 {
            m.step([Input::default(); 2], &mut col);
        }
        for _ in 0..4 {
            m.step(press(0), &mut col);
        }
        for _ in 0..60 {
            m.step([Input::default(); 2], &mut col);
        }
        assert!(m.combat.crystal.is_some(), "set by button 0");
        for _ in 0..800 {
            m.step([Input::default(); 2], &mut col);
        }
        for _ in 0..4 {
            m.step(press(1), &mut col);
        }
        assert!(m.combat.crystal.is_some(), "still inside its 15 s lock");
        for _ in 0..200 {
            m.step([Input::default(); 2], &mut col);
        }
        for _ in 0..4 {
            m.step(press(1), &mut col);
        }
        assert!(m.combat.crystal.is_none(), "captured after the lock");
        assert_eq!(m.combat.fighters[0].slots[1], Some(AURA_PARTICLE));
    }
}

/// Lane GUARDFIX (audit lanes BLAST3, GUARDADD, AUTOSHELTER, REQ5, COPYNO): each rule pinned to the
/// program address it was read from (`PDUWP.exe`, decompile in `~/.pdtools/re-out/decomp_all.c`).
#[cfg(test)]
mod guardfix {
    use super::barrier::{self, kind, BlockEffect, Outcome};
    use super::testkit::{self as k, f};
    use super::*;

    const SHELTER: i16 = 501;
    const BRUSH: i16 = 502;
    const REFLECT: i16 = 503;
    const ABSORB: i16 = 504;
    const REVERSE: i16 = 506;
    const AUTO: i16 = 507; // cost 2, unlimited
    const AUTO_ONCE: i16 = 508; // cost 2, one use
    const ABSORB_ENERGY: i16 = 509; // Add 2
    const ZAP: i16 = 600; // STR 1 shell, cost 7
    const STRONG: i16 = 601; // STR 4 shell, cost 7
    const BIG: i16 = 602; // STR 6 shell
    const PUNCH: i16 = 606; // motion 13
    const PURSUIT: i16 = 607; // motion 14
    const NEEDS_FEW: i16 = 620; // requirement 5, N = 3
    const COPYCAT: i16 = 621; // s43 = 6

    fn guard(id: i16, kind: i16, def: i16, cost: i32, extra: &[(usize, i16)]) -> SkillDef {
        let mut fields = vec![(43, 2), (f::DEFENCE_KIND, kind), (f::STRENGTH, 28), (f::DEFENCE, def)];
        fields.extend_from_slice(extra);
        k::def(id, Capsule::Defence, cost, &fields)
    }

    fn attack(id: i16, motion: i16, strength: i16, cost: i32) -> SkillDef {
        k::def(id, Capsule::Attack, cost, &[(f::MOTION, motion), (f::STRENGTH, strength)])
    }

    fn skills() -> Vec<SkillDef> {
        vec![
            guard(SHELTER, kind::SHELTER, 5, 1, &[]),
            guard(BRUSH, kind::BRUSH, 5, 1, &[]),
            guard(REFLECT, kind::REFLECT, 5, 1, &[]),
            guard(ABSORB, kind::ABSORB, 5, 1, &[]),
            guard(REVERSE, kind::REVERSE, 0, 1, &[]),
            guard(AUTO, kind::AUTO_SHELTER, 5, 2, &[(f::STRENGTH, 10)]),
            guard(AUTO_ONCE, kind::AUTO_SHELTER, 5, 2, &[(f::STRENGTH, 10), (f::USES, 1)]),
            guard(ABSORB_ENERGY, kind::ABSORB, 9, 1, &[(f::ADD_EFFECT, 2)]),
            attack(ZAP, 0, 1, 7),
            attack(STRONG, 0, 4, 7),
            attack(BIG, 0, 6, 7),
            attack(PUNCH, 13, 1, 1),
            attack(PURSUIT, 14, 1, 1),
            k::def(NEEDS_FEW, Capsule::Attack, 1, &[(f::MOTION, 0), (f::STRENGTH, 1), (f::REQUIREMENT, 5), (f::REQUIREMENT_AMOUNT, 3)]),
            k::def(COPYCAT, Capsule::Attack, 1, &[(43, 6), (44, 0)]),
        ]
    }

    /// Player 1 holds `guard_id` standing on button 0; Level 20, Aura 0.
    fn holder(guard_id: i16) -> (Combat, [Player; 2], Collision) {
        let (mut c, p, col) = k::setup(skills(), 100.0);
        let g = barrier::Guard::from_def(c.skill(guard_id).unwrap()).unwrap();
        c.fighters[1].slots[0] = Some(guard_id);
        c.fighters[1].guard = Some(ActiveGuard::new(guard_id, g, 0));
        c.fighters[1].level = 20;
        c.fighters[1].aura_ticks = 0;
        (c, p, col)
    }

    fn strike(c: &mut Combat, p: &[Player; 2], id: i16, arrival: Arrival) -> bool {
        let str = c.skill(id).unwrap().strength;
        let bodies = bodies_of(p);
        c.strike(0, 1, id, Some(p[0].pos), bodies, arrival, Via { button: 0, strength: Some(str) })
    }

    fn resolve(guard_id: i16, attack_id: i16, arrival: Arrival) -> Outcome {
        let c = k::setup(skills(), 100.0).0;
        let g = barrier::Guard::from_def(c.skill(guard_id).unwrap()).unwrap();
        barrier::resolve(&g, c.skill(attack_id).unwrap().strength, c.skill(attack_id).unwrap(), arrival, true, false)
    }

    fn blocks(c: &Combat) -> usize {
        c.events.iter().filter(|e| matches!(e.1, CombatEvent::Blocked { .. })).count()
    }

    // ---- BLAST3 ----

    #[test]
    fn a_blast_is_object_state_3_to_the_guard_resolution() {
        // `FUN_140271760`: state 3 returns 0 (the attack carries on) for Absorb (`0x54 == 5`), Brush
        // (`== 3`) and About Face / Reverse (`== 6`), and the Reflect branch (`0x54 == 4`) needs
        // state != 3: a blast is stopped like a plain attack and never sent back.
        for g in [ABSORB, BRUSH, REVERSE] {
            assert_eq!(resolve(g, ZAP, Arrival::Blast), Outcome::Through, "guard {g}");
        }
        assert!(matches!(resolve(REFLECT, ZAP, Arrival::Blast), Outcome::Blocked { broken: false, effect: BlockEffect::Stop, .. }), "stopped, not reflected");
        assert!(matches!(resolve(REFLECT, ZAP, Arrival::Shot), Outcome::Blocked { effect: BlockEffect::Reflect, .. }), "the shot itself is");
        assert!(matches!(resolve(SHELTER, ZAP, Arrival::Blast), Outcome::Blocked { .. }), "a Shelter still stops it");
    }

    #[test]
    fn the_blast_of_a_shot_reaches_an_absorb_holder_through_tick_blasts() {
        let (mut c, p, col) = holder(ABSORB);
        c.fighters[1].health = START_HEALTH;
        c.add_blast(0, ZAP, p[1].pos, 30.0, 1 << 1, false);
        for _ in 0..40 {
            c.tick_blasts(&p, &col);
        }
        assert_eq!(c.fighters[1].health, START_HEALTH - 1, "the blast landed on the body");
        assert_eq!(blocks(&c), 0, "no block: the program lets state 3 through Absorb");
        // Reflect: stopped, nothing flies back.
        let (mut c, p, col) = holder(REFLECT);
        c.add_blast(0, ZAP, p[1].pos, 30.0, 1 << 1, false);
        for _ in 0..40 {
            c.tick_blasts(&p, &col);
        }
        assert_eq!(blocks(&c), 1);
        assert!(c.projectiles.is_empty(), "a blast is not sent back");
    }

    // ---- GUARDADD ----

    #[test]
    fn absorb_energy_gains_the_attacks_str_not_its_cost() {
        // `FUN_140271ed0` case 2 = `FUN_1402618e0(holder, (float)[attack + 0x60])`.
        let (mut c, p, _) = holder(ABSORB_ENERGY);
        assert!(!strike(&mut c, &p, STRONG, Arrival::Shot));
        assert_eq!(c.fighters[1].aura(), 4, "STR 4, not the Aura cost 7");
        let (mut c, p, _) = holder(ABSORB_ENERGY);
        assert!(!strike(&mut c, &p, ZAP, Arrival::Shot));
        assert_eq!(c.fighters[1].aura(), 1, "STR 1");
    }

    #[test]
    fn a_brush_stuns_only_motions_3_12_and_13() {
        // `FUN_14026cd70` case 7: s43 == 0 and motion 3 / 12 / 13 -> reaction 26; anything else
        // (a Pursuit, motion 14) is brushed aside.
        let (mut c, p, _) = holder(BRUSH);
        c.strike(0, 1, PUNCH, Some(p[0].pos), bodies_of(&p), Arrival::Melee, Via { button: 0, strength: Some(1) });
        assert_eq!(c.fighters[0].action, Action::Hit, "motion 13");
        let (mut c, p, _) = holder(BRUSH);
        c.strike(0, 1, PURSUIT, Some(p[0].pos), bodies_of(&p), Arrival::Melee, Via { button: 0, strength: Some(1) });
        assert_ne!(c.fighters[0].action, Action::Hit, "motion 14 takes the no-stun branch");
        assert_eq!(blocks(&c), 1, "it is still blocked");
    }

    // ---- AUTOSHELTER ----

    fn auto_holder(id: i16) -> (Combat, [Player; 2]) {
        let (mut c, p, _) = k::setup(skills(), 100.0);
        c.fighters[1].slots[0] = Some(id);
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        (c, p)
    }

    #[test]
    fn an_automatic_shelter_blocks_one_attack_per_use_and_pays_each_time() {
        // `FUN_14026f530` -> `FUN_1402626d0` (pay, consume) -> `FUN_140271760` on THIS attack;
        // `FUN_140264340` ends a kind 7 object at its first update: no standing guard.
        let (mut c, p) = auto_holder(AUTO);
        assert!(!strike(&mut c, &p, ZAP, Arrival::Shot), "first blocked");
        assert!(c.fighters[1].guard.is_none(), "nothing stands afterwards");
        assert_eq!(c.fighters[1].aura(), 6, "paid 2");
        assert!(!strike(&mut c, &p, ZAP, Arrival::Shot), "second blocked by a fresh use, same tick");
        assert_eq!(c.fighters[1].aura(), 4, "paid again");
        assert_eq!(blocks(&c), 2);
        assert_eq!(c.fighters[1].slots[0], Some(AUTO), "unlimited: kept");
        assert_eq!(c.fighters[1].health, START_HEALTH);
    }

    #[test]
    fn a_used_up_automatic_shelter_lets_the_second_attack_land() {
        let (mut c, p) = auto_holder(AUTO_ONCE);
        assert!(!strike(&mut c, &p, ZAP, Arrival::Shot));
        assert!(c.fighters[1].slots[0].is_none(), "its one use is gone");
        assert!(strike(&mut c, &p, ZAP, Arrival::Shot), "no Shelter left: lands");
        assert_eq!(c.fighters[1].health, START_HEALTH - 1);
    }

    #[test]
    fn an_automatic_shelter_with_too_little_aura_does_not_trigger() {
        let (mut c, p) = auto_holder(AUTO);
        c.fighters[1].aura_ticks = AURA_TICKS_PER_POINT; // 1 < cost 2
        assert!(strike(&mut c, &p, ZAP, Arrival::Shot));
        assert_eq!(blocks(&c), 0);
    }

    #[test]
    fn a_strong_hit_breaks_the_automatic_shelter_and_it_leaves_its_button() {
        let (mut c, p) = auto_holder(AUTO);
        assert!(!strike(&mut c, &p, BIG, Arrival::Shot), "STR 6 > DEF 5: stopped too");
        assert!(c.events.iter().any(|e| matches!(e.1, CombatEvent::Blocked { broken: true, .. })));
        assert!(c.fighters[1].slots[0].is_none(), "a broken guard leaves its button");
    }

    // ---- REQ5 ----

    #[test]
    fn requirement_5_counts_the_players_capsules_and_the_undealt_deck() {
        // `FUN_140262470` case 5: refused when (30 - dealt) + `DAT_140585c20` > N; the second term
        // counts the player's capsule objects not retired (lying, appearing, waiting).
        let (mut c, _, _) = k::setup(skills(), 300.0);
        let def = c.skill(NEEDS_FEW).unwrap().clone();
        c.fighters[0].arsenal = vec![ZAP; 3];
        assert!(c.requirement_met(0, &def), "deck 3, no capsule: 3 <= 3");
        c.capsules.push(CapsuleItem { id: 900, owner: 0, skill: ZAP, pos: Vec3::new(300.0, 0.0, 300.0), appear: 0 });
        assert!(!c.requirement_met(0, &def), "deck 3 + 1 lying: 4 > 3");
        c.fighters[0].arsenal = vec![ZAP; 2];
        assert!(c.requirement_met(0, &def), "deck 2 + 1 lying: 3");
        c.capsules.push(CapsuleItem { id: 901, owner: 0, skill: ZAP, pos: Vec3::new(320.0, 0.0, 300.0), appear: 5 });
        assert!(!c.requirement_met(0, &def), "an appearing one counts too: 4");
        c.fighters[1].arsenal = vec![];
        assert!(c.requirement_met(1, &def), "the other player's capsules are not yours");
    }

    #[test]
    fn requirement_5_counts_a_capsule_waiting_to_regenerate() {
        let (mut c, mut p, col) = k::setup(skills(), 300.0);
        let def = c.skill(NEEDS_FEW).unwrap().clone();
        let spots = vec![Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 150.0)];
        c.set_capsule_spots([spots.clone(), vec![]]);
        c.fighters[0].arsenal = vec![ZAP; 10];
        k::run(&mut c, &mut p, &col, 1);
        c.capsules.retain(|x| x.pos != spots[1]); // captured: waits to regenerate
        k::run(&mut c, &mut p, &col, 10);
        assert_eq!(c.pending_capsules().len(), 1);
        c.fighters[0].arsenal = vec![ZAP; 1];
        assert!(!c.requirement_met(0, &def), "deck 1 + 2 lying + 1 waiting = 4 > 3");
        c.capsules.retain(|x| x.owner != 0 || x.pos == spots[0]);
        assert!(c.requirement_met(0, &def), "deck 1 + 1 lying + 1 waiting = 3");
    }

    // ---- COPYNO ----

    #[test]
    fn copy_with_nothing_remembered_says_no_and_starts_nothing() {
        // `FUN_140258180`: index -1 -> `FUN_140276330(0, 2, 2, ...)` = common/2 (our `Denied`).
        let (mut c, mut p, col) = k::setup(skills(), 100.0);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[0] = Some(COPYCAT);
        let seq = c.fighters[0].cast_seq;
        k::press(&mut c, &mut p, &col, 0, 0);
        assert!(c.events.iter().any(|e| e.1 == CombatEvent::Denied { who: 0, slot: 0 }), "the refusal sound");
        assert_eq!(c.fighters[0].cast_seq, seq, "nothing started");
        assert_eq!(c.fighters[0].slots[0], Some(COPYCAT), "nothing paid");
        let cues = crate::sfx::cues_for_event(&CombatEvent::Denied { who: 0, slot: 0 }, |_| Default::default(), |_| (0, 0));
        assert!(matches!(cues.as_slice(), [crate::sfx::Cue::Sfx(2, _)]), "common/2");
    }
}
