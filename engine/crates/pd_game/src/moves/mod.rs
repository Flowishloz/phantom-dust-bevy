//! How each skill behaves once cast, read from the skill table.
//!
//! One module per family; this file is the registry (`behaviour_of`,
//! `launch`). A family lane adds its module, a `Behaviour`/`ShotKind`
//! variant and one line in each match below — nothing else. Same contract as
//! the sim: pure functions, no Bevy types, no clocks. Field meanings and how
//! sure we are of them: docs/FORMATS-SKILLS.md; per-family rules: docs/MOVES.md.

pub mod arc;
pub mod area;
pub mod barrier;
pub mod crawl;
pub mod effects;
pub mod environment;
pub mod erase;
pub mod fall;
pub mod melee;
pub mod parabola;
pub mod ricochet;
pub mod shell;
pub mod special;
pub mod status;
pub mod tornado;

use crate::combat::{Capsule, SkillDef, AURA_PARTICLE, CHEST};
use glam::{Quat, Vec3};
use pd_formats::collision::Collision;
use pd_formats::skills::field;

// ---- Table units -----------------------------------------------------------
//
// The program runs its logic at 30 Hz ("program tick" = 1/30 s, `FUN_1400b1bc0`)
// and the sim at 60 Hz. Every flying object, area and blast steps once per
// program tick = every second sim tick, from its creation, with the program's
// own per-tick numbers; the position it is drawn at moves half a step each sim
// tick (`Shot::pos` is the program position, `Projectile::pos` the drawn one),
// so a shot's path, timing and hits are the program's and its motion is smooth.
// (The Aura refill does the same: whole program ticks, every second sim tick.)

/// The table counts time in program ticks of 1/30 s ("Freezes target's skill
/// buttons for 10 sec" = 300, "can't move for 3 sec" = 90). The sim runs at
/// 60 Hz, so one table tick is two sim ticks.
pub const SIM_TICKS_PER_TABLE_TICK: u16 = 2;
/// Table distances are tenths of a world unit (blast radius 300 -> 30 u,
/// short-range reach 130 -> 13 u). Read: PROGRAM-RE.md "Units, settled".
pub const TABLE_DIST: f32 = 0.1;
/// Table speeds are 1/10.8 of a unit per program tick per point (float at
/// `0x1402f7cd8`): Laser 140 -> 12.96 u/tick = 389 u/s (2.78 u/s per point,
/// not the 3 the first reading used).
pub const SPEED_UNIT: f32 = 1.0 / 10.8;
/// One degree.
pub const DEG: f32 = std::f32::consts::PI / 180.0;
/// Turn rates are 0.1 degree per program tick per point (`0x1402f7cdc` x
/// `0x1402f7c84`), times the caster's Homing status: 3 deg/s = 0.052 rad/s
/// per point, not the 0.1 rad/s the first reading used.
pub const TURN_UNIT: f32 = 0.1 * DEG;
/// The program's body: a sphere of 5 u (character table +0x10 / 10) whose
/// centre is 9 u above the feet (+0x12 / 10) -- what projectiles, blasts and
/// areas are tested against (`FUN_14026c1c0`, `FUN_14026c5b0`).
pub const HIT_BODY_RADIUS: f32 = 5.0;
pub const HIT_BODY_CENTRE: f32 = 9.0;
/// Every object's own hit radius at creation (`FUN_140263a00`: 5.0 at +0x190).
pub const SHOT_RADIUS: f32 = 5.0;
/// Fall acceleration of crawlers and the like: 0.12 u per program tick squared
/// (`0x1402f7ce4`).
pub const GRAVITY_TICK: f32 = 0.12;

pub fn table_ticks(n: i16) -> u16 {
    (n.max(0) as u16).saturating_mul(SIM_TICKS_PER_TABLE_TICK)
}

/// A body's centre from its feet.
pub fn body_centre(feet: Vec3) -> Vec3 {
    feet + Vec3::Y * HIT_BODY_CENTRE
}

/// Where a point between `a` and `b` is closest to `c` (a segment-sphere
/// test, `FUN_140196580`).
pub fn closest_on_segment(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 <= 1e-12 {
        return a;
    }
    a + ab * ((c - a).dot(ab) / len2).clamp(0.0, 1.0)
}

/// Speed ramp shared by shells, crawlers, tornadoes and lasers: `v0` for
/// `ticks == 0`, else a straight line from `v0` to `v1` over `ticks` program
/// ticks and `v1` after (`FUN_140264710`).
pub fn ramp(v0: f32, v1: f32, ticks: i16, age: u16) -> f32 {
    if ticks == 0 {
        v0
    } else if (age as i32) < ticks as i32 {
        (v1 - v0) / ticks as f32 * age as f32 + v0
    } else {
        v1
    }
}

/// How many program ticks an object lives (`FUN_1402636f0`): `s64 >= 0` is
/// the number of ticks, `s64 < 0` is a range of `-s64 x 10` u, walked at the
/// speed ramp `s65 -> s66` over `s67` ticks (Blaster -30 -> 300 u in 23
/// ticks).
pub fn life_ticks(s64: i16, s65: i16, s66: i16, s67: i16) -> u16 {
    if s64 >= 0 {
        return s64 as u16;
    }
    let range = -(s64 as f32) * 10.0;
    let (v0, v1) = (s65 as f32 * SPEED_UNIT, s66 as f32 * SPEED_UNIT);
    let ticks = if s67 == 0 {
        range / v0.max(1e-3)
    } else {
        let ramp_len = (v0 + v1) * s67 as f32 * 0.5;
        if range <= ramp_len {
            s67 as f32
        } else {
            (range - ramp_len) / v1.max(1e-3) + s67 as f32
        }
    };
    ticks.clamp(0.0, 60_000.0) as u16
}

/// The range band of the aimed families (`FUN_140273d40`): the aim point is
/// pulled to 80-300 u from the spawn point (s44 = 0), 300-600 u (s44 = 1, the
/// Optical lasers and the mortars), or left alone (s44 = 2, every motion-16
/// Hole); a pulled aim point drops the target (no homing, no steering).
/// Returns the aim point and whether the target is kept.
pub fn band(def: &SkillDef, spawn: Vec3, aim: Vec3) -> (Vec3, bool) {
    let (s43, s44) = (def.raw[43], def.raw[field::RANGE_BAND]);
    if s43 == 0 && (s44 == 2 || def.motion() == motion::HOLE_B) {
        return (aim, true);
    }
    let v = aim - spawn;
    let d2 = v.length_squared();
    if d2 <= 0.0 {
        return (aim, false);
    }
    let (min, max) = if s43 == 0 && s44 == 0 { (80.0, 300.0) } else { (300.0, 600.0) };
    if d2 < min * min {
        (spawn + v / d2.sqrt() * min, false)
    } else if d2 <= max * max {
        (aim, true)
    } else {
        (spawn + v / d2.sqrt() * max, false)
    }
}

/// An orthonormal frame: the program's object matrix (`+0xd0` right, `+0xe0`
/// up, `+0xf0` forward).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
}

impl Frame {
    /// The frame that looks along `fwd` with the world's up (`FUN_14012e6f0`).
    pub fn look(fwd: Vec3) -> Frame {
        let fwd = fwd.normalize_or(Vec3::Z);
        let right = Vec3::Y.cross(fwd).try_normalize().unwrap_or(Vec3::X);
        Frame { right, up: fwd.cross(right), fwd }
    }

    /// Roll about the forward axis (`FUN_140290180`): the arc's plane tilts.
    pub fn roll(self, a: f32) -> Frame {
        let (s, c) = a.sin_cos();
        Frame { right: self.right * c + self.up * s, up: self.up * c - self.right * s, fwd: self.fwd }
    }

    /// Turn about the up axis (`FUN_140290050`): positive = toward `right`
    /// (the bearing `atan2(x, z)` grows).
    pub fn yaw(self, a: f32) -> Frame {
        let (s, c) = a.sin_cos();
        Frame { right: self.right * c - self.fwd * s, up: self.up, fwd: self.right * s + self.fwd * c }
    }

    /// `FUN_140290180` with the program's own angle: the program's row 0 is the object's physical right
    /// (left-handed, x right / y up / z forward), and our export mirrors z, so `right` here (`Y x fwd`)
    /// is the program's LEFT -- the same rotation is `roll` by the negated angle (MEASURED the same way
    /// by the target-point offset, `body.rs`: the program's right axis is minus ours).
    pub fn roll_program(self, a: f32) -> Frame {
        self.roll(-a)
    }

    /// `FUN_140290050` with the program's own angle: a positive angle turns the heading toward the
    /// object's physical right (the program's row 0), which is `yaw` by the negated angle here.
    pub fn yaw_program(self, a: f32) -> Frame {
        self.yaw(-a)
    }

    /// `FUN_1402905c0` with the program's own angle: rows 1 / 2 mix (forward -> forward cos a - up sin a), so a
    /// positive angle tips the heading down. Up and forward are the same rows in both spaces (the z mirror
    /// only flips their z components), so no sign changes here.
    pub fn pitch_program(self, a: f32) -> Frame {
        let (s, c) = a.sin_cos();
        Frame { right: self.right, up: self.up * c + self.fwd * s, fwd: self.fwd * c - self.up * s }
    }

    /// The program's fire matrix (`FUN_1402514c0`, rebuilt every player update and handed to the fire
    /// `FUN_1402626d0` as `+0x12dc0`): the facing, then tipped by the pitch the press captured
    /// (`FUN_140258180` -> `+0x12f8c`, see `Launch::pitch`).
    pub fn fire(facing: Vec3, pitch: f32) -> Frame {
        Frame::look(facing).pitch_program(pitch)
    }

    /// Turn toward `want` by at most `max` radians, the whole frame about the
    /// axis `fwd x want` (`FUN_1401950c0`).
    pub fn turn_toward(self, want: Vec3, max: f32) -> Frame {
        let Some(want) = want.try_normalize() else { return self };
        let dot = self.fwd.dot(want);
        if dot >= 1.0 {
            return self;
        }
        let angle = if dot > -1.0 { (1.0 - dot * dot).max(0.0).sqrt().atan2(dot) } else { std::f32::consts::PI };
        let Some(axis) = self.fwd.cross(want).try_normalize() else { return self };
        let q = Quat::from_axis_angle(axis, angle.min(max));
        Frame { right: q * self.right, up: q * self.up, fwd: q * self.fwd }
    }
}

/// `v` turned toward `want` by at most `max` radians (the same rotation on a
/// bare direction).
pub fn rotate_toward(v: Vec3, want: Vec3, max: f32) -> Vec3 {
    Frame::look(v).turn_toward(want, max).fwd
}

/// Where an object is created and what it can aim at (`FUN_1402626d0`).
#[derive(Clone, Copy)]
pub struct Launch<'a> {
    /// The creation point of the first object: the cast effect's `SHOT01`
    /// marker, or the caster's right hand when the effect has none
    /// (`FUN_14018a310`, `Combat::shot_marks`); the chest when no marker data
    /// is loaded (tests, an unconverted cache).
    pub spawn: Vec3,
    /// The caster's feet (crawlers, tornadoes, mines start on the floor).
    pub feet: Vec3,
    /// The caster's facing, a horizontal unit vector: the fire matrix.
    pub facing: Vec3,
    /// The fire matrix's pitch, program sense (positive = down; `Frame::fire`): READ, `FUN_140258180` stores
    /// `atan2(caster y + centre height - target point y, horizontal distance)` at the press of an aimed attack
    /// with a locked target (0 otherwise) and `FUN_1402514c0` tips the fire matrix by it. Used by the families
    /// whose init takes the fire matrix rows as the heading (arc `FUN_1402666b0`, multi-laser `FUN_14026af40`).
    pub pitch: f32,
    /// The locked target's body centre; `None` = nothing locked.
    pub target: Option<Vec3>,
    /// This object's index among the cast's shots (marker order).
    pub index: u32,
    /// Stage collision, for the families that look for the floor.
    pub col: Option<&'a Collision>,
    /// Every marker's world position in marker order (`SHOT01`, `SHOT02`, ...),
    /// for the casts that make several objects: shot `m` of a cast starts at
    /// marker `(index + m) mod count` (`FUN_1402626d0`: `local_c8[(counter + i)
    /// % count]`). Empty = every object starts at `spawn`.
    pub marks: &'a [Vec3],
}

impl Launch<'_> {
    /// Where object `m` of this cast starts (`marks`, else `spawn`).
    pub fn spawn_of(&self, m: u32) -> Vec3 {
        if self.marks.is_empty() {
            self.spawn
        } else {
            self.marks[((self.index + m) as usize) % self.marks.len()]
        }
    }

    /// The aim point of the families that aim through the range band
    /// (`FUN_14026e410`): the target's centre (`floor`: the floor under it,
    /// for Hole and Rain), pulled into the band; with nothing locked a
    /// point 190 u (s44 = 0) / 300 u (s44 = 1) straight ahead. Returns the
    /// point and whether the target is kept (a pulled aim point drops it).
    pub fn aim(&self, def: &SkillDef, floor: bool) -> (Vec3, bool) {
        let Some(t) = self.target else {
            let far = def.raw[43] == 0 && def.raw[field::RANGE_BAND] == 1;
            return (self.spawn + self.facing * if far { 300.0 } else { 190.0 }, false);
        };
        let (point, kept) = if floor {
            match self.col.and_then(|c| c.ground_below_object(t.x, t.z, t.y)) {
                Some(y) => (Vec3::new(t.x, y, t.z), true),
                None => (t, false),
            }
        } else {
            (t, true)
        };
        let (point, in_band) = band(def, self.spawn, point);
        (point, kept && in_band)
    }
}

/// Motion type (s63).
pub mod motion {
    pub const SHELL: i16 = 0;
    pub const PARABOLA: i16 = 1;
    pub const FALL: i16 = 2;
    pub const CRAWL: i16 = 5;
    pub const ARC: i16 = 6;
    pub const RAIN_A: i16 = 7;
    pub const MINE: i16 = 8;
    pub const AROUND_SELF: i16 = 9;
    pub const TORNADO: i16 = 10;
    pub const OBJECT: i16 = 11;
    pub const PURSUIT: i16 = 14;
    pub const RICOCHET: i16 = 17;
    pub const HOLE_A: i16 = 15;
    pub const HOLE_B: i16 = 16;
    pub const RAIN_B: i16 = 20;
}

/// Extra effect codes on attacks (s51).
pub const ADD_EXPLODES: i16 = 5;

/// "Add:" effects of attacks when they land (s51, amount/time in s52). Codes
/// checked against the notes (FORMATS-SKILLS.md).
pub mod attack_add {
    /// "Reduce target's Aura to 0" (Greedy Spirit).
    pub const AURA_TO_ZERO: i16 = 1;
    /// "Freezes target's skill buttons for 10 sec" (300).
    pub const FREEZE: i16 = 7;
    /// "For 10 sec, target's button presses cause 1 damage" (300).
    pub const POISON: i16 = 8;
    /// "Health increase = damage dealt" (Dhampir Claw).
    pub const HEALTH_FROM_DAMAGE: i16 = 9;
    /// "Level increase = damage dealt. Target's Level decreases by same" (Succubus).
    pub const LEVEL_DRAIN: i16 = 11;
}

impl SkillDef {
    pub fn motion(&self) -> i16 {
        self.raw[field::MOTION]
    }

    /// Motion parameter k (0..6 = table fields s64..s69).
    pub fn param(&self, k: usize) -> i16 {
        self.raw[field::MOTION_PARAMS + k]
    }

    /// "Explodes on impact": blast radius in world units, 0 if it doesn't.
    pub fn blast_radius(&self) -> f32 {
        if self.capsule == Capsule::Attack && self.raw[field::ADD_EFFECT] == ADD_EXPLODES {
            self.raw[field::ADD_AMOUNT].max(0) as f32 * TABLE_DIST
        } else {
            0.0
        }
    }

    /// An attack's "Add:" effect code and its time in sim ticks.
    pub fn add_effect(&self) -> (i16, u16) {
        if self.capsule != Capsule::Attack {
            return (0, 0);
        }
        (self.raw[field::ADD_EFFECT], table_ticks(self.raw[field::ADD_AMOUNT]))
    }

    /// "Can't be erased or overwritten" (s41 = 2).
    pub fn protected(&self) -> bool {
        self.raw[field::PROTECTED] == 2
    }

    /// "Hold button to charge" (Gungnir, Ki Cannon, Ki Palm, Young White
    /// Dragon). The program decides it from the skill record, not the action
    /// row: at the cast start (`FUN_140258180`, `0x14025823e`) the record's
    /// field at `+0x46` (= our s37) picks the cast's mode `+0x68`: s37 = 2 ->
    /// 1 (hold-to-charge: `FUN_14025c720` freezes the fire countdown while the
    /// button is held once the w39 wind-up has run), s37 = 0 -> 2 (an ordinary
    /// cast), anything else -> 0. The same flag is the arsenal rules' "charge
    /// skill" (`FUN_14015b830`, `arsenal::is_charge_skill`). Dragon Bullet's
    /// row carries w39 = 13 like the charge rows but its s37 is 0, so it is an
    /// ordinary cast there: the w39 countdown only matters in mode 1.
    pub fn charged(&self) -> bool {
        self.raw[37] == 2
    }

    /// How many times it can be used from its button before it's used up;
    /// `None` = unlimited (s26 = 0). Probable reading (FORMATS-SKILLS.md s26).
    pub fn uses(&self) -> Option<u8> {
        u8::try_from(self.raw[field::USES]).ok().filter(|&n| n > 0)
    }

    /// Aura cost change per earlier use from the same button: "Aura cost +1
    /// per use" (s24 = 1), Knife Grind's "-1" (s24 = -1).
    pub fn cost_per_use(&self) -> i32 {
        self.raw[field::COST_PER_USE] as i32
    }

    /// "Retain if you have enough Aura to use again" (s41 = 1): stays on its
    /// button after use while the caster can still pay for it.
    pub fn retained(&self) -> bool {
        self.raw[field::PROTECTED] == 1
    }

    /// Works on its own from a slot and can't be pressed: automatic Shelters,
    /// Orb ("Automatically negates any Erase skill used") and Amulet
    /// ("Automatically reverses effects of last Erase skill that hit you").
    pub fn passive(&self) -> bool {
        (self.capsule == Capsule::Defence && self.raw[field::DEFENCE_KIND] == barrier::kind::AUTO_SHELTER) || (self.capsule == Capsule::Special && matches!(self.id, 180 | 299))
    }

    /// "Add: Pierce effect": damage through a defence = STR - DEF.
    pub fn pierces(&self) -> bool {
        self.raw[field::PIERCE] == 1
    }

    /// "Penetrates defensive skills".
    pub fn ignores_defence(&self) -> bool {
        self.raw[field::PIERCE] == 2
    }

    /// "Penetrates obstacles" (s55 = 3) and s55 = 2: neither is stopped by a
    /// wall nor needs a clear line to its target (`FUN_14026ca50`, `FUN_1401641b0`).
    pub fn ignores_walls(&self) -> bool {
        self.raw[field::OBSTACLES] >= 2
    }
}

// ---- Registry ---------------------------------------------------------------

/// What a skill does once cast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Behaviour {
    AuraParticle,
    /// Short-range: dash at the target, strike on arrival.
    Melee(melee::Melee),
    /// Leaves as projectile(s) at the end of the cast animation.
    Shoot(ShotKind),
    /// Set at a place at the end of the cast; goes off later (Hole, Rain, mines, Photon Wave).
    Place(area::AreaSpec),
    /// A defence held in front of (or all round) the caster.
    Guard(barrier::Guard),
    /// An erase / status / special / environment skill that takes effect at
    /// the end of the cast without travelling (on you, everyone, the field...).
    Effect(effects::Delivery),
    /// Paid for and animated, but does nothing yet.
    NotBuilt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotKind {
    Shell(shell::Shell),
    Parabola(parabola::Parabola),
    Arc(arc::Arc),
    Crawl(crawl::Crawl),
    Fall(fall::Fall),
    Ricochet(ricochet::Ricochet),
    Tornado(tornado::Tornado),
}

pub fn behaviour_of(def: &SkillDef) -> Behaviour {
    if def.id == AURA_PARTICLE {
        return Behaviour::AuraParticle;
    }
    match def.capsule {
        Capsule::Attack => match def.motion() {
            _ if melee::is_melee(def) => Behaviour::Melee(melee::Melee::from_def(def)),
            motion::SHELL if shell::has_flight(def) => Behaviour::Shoot(ShotKind::Shell(shell::Shell::from_def(def))),
            motion::PARABOLA => Behaviour::Shoot(ShotKind::Parabola(parabola::Parabola::from_def(def))),
            motion::ARC => Behaviour::Shoot(ShotKind::Arc(arc::Arc::from_def(def))),
            motion::CRAWL => Behaviour::Shoot(ShotKind::Crawl(crawl::Crawl::from_def(def))),
            motion::TORNADO => Behaviour::Shoot(ShotKind::Tornado(tornado::Tornado::from_def(def))),
            motion::FALL => Behaviour::Shoot(ShotKind::Fall(fall::Fall::from_def(def))),
            // [Object] (Psycho Kinesis, Power): "Throws nearby object". This arm
            // only decides the *motion*; `combat.rs::fire` intercepts OBJECT
            // before launch and throws the nearest prop itself (lift, hang,
            // per-prop strength), tagging the projectile with `prop: Some(k)`.
            motion::OBJECT => Behaviour::Shoot(ShotKind::Shell(shell::Shell::from_def(def))),
            motion::RICOCHET => Behaviour::Shoot(ShotKind::Ricochet(ricochet::Ricochet::from_def(def))),
            _ if area::from_def(def).is_some() => Behaviour::Place(area::from_def(def).unwrap()),
            // Families not built yet keep a generic homing shot so any arsenal is playable.
            _ => Behaviour::Shoot(ShotKind::Shell(shell::Shell::PLACEHOLDER)),
        },
        Capsule::Defence => barrier::Guard::from_def(def).map_or(Behaviour::NotBuilt, Behaviour::Guard),
        // Erase, status, special, environment: delivered like an attack when
        // they travel (effects.rs), then do their thing where they land.
        _ => match effects::delivery(def) {
            effects::Delivery::Travels => match def.motion() {
                _ if melee::is_melee(def) => Behaviour::Melee(melee::Melee::from_def(def)),
                motion::CRAWL => Behaviour::Shoot(ShotKind::Crawl(crawl::Crawl::from_def(def))),
                motion::TORNADO => Behaviour::Shoot(ShotKind::Tornado(tornado::Tornado::from_def(def))),
                _ if area::from_def(def).is_some() => Behaviour::Place(area::from_def(def).unwrap()),
                _ if shell::has_flight(def) => Behaviour::Shoot(ShotKind::Shell(shell::Shell::from_def(def))),
                _ => Behaviour::Effect(effects::Delivery::TargetNow),
            },
            d => Behaviour::Effect(d),
        },
    }
}

/// What a flying object does each program tick: one state per family, all
/// read from the program's init/update pairs (docs/MOVES.md has the table).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flight {
    Shell(shell::Flying),
    Lob(parabola::Flying),
    Fall(fall::Flying),
    Crawl(crawl::Flying),
    Arc(arc::Flying),
    Ricochet(ricochet::Flying),
    Tornado(tornado::Flying),
}

/// One projectile leaving the caster (the sim adds id/owner/skill). `pos` is
/// the program's position, stepped once per program tick (`step`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: Vec3,
    /// Where it was before the last program step (`+0x140`): the hit tests
    /// sweep `prev -> pos`.
    pub prev: Vec3,
    /// Multiplier on the family's turn / steer limits: the caster's Homing
    /// status, 0 = no target (an unlocked cast, a brushed shell).
    pub homing: f32,
    /// Sim ticks it hangs where it was made before the program runs it (a
    /// thrown object lifting off).
    pub delay: u16,
    /// Program ticks left (`FUN_14026e720`): it moves, is tested, and goes when
    /// this is 0.
    pub life: u16,
    /// Explodes on impact with this radius (0 = no blast; s51 = 5, s52).
    pub blast: f32,
    /// "Penetrates obstacles" (s55 = 2 / 3): no wall test, no line-of-sight test.
    pub through_walls: bool,
    /// A [Course] field has turned it: it steers away from its target (`0x100`).
    pub away: bool,
    /// Its own hit radius (5 u).
    pub radius: f32,
    /// Sim ticks since it was made; it steps on the even ones.
    pub tick: u16,
    /// Program ticks it has run.
    pub age: u16,
    /// Its first program tick has passed: `FUN_1402638d0` only arms a new
    /// motion object (state 1 -> 2) and returns, so nothing moves or is tested
    /// on the tick it was made.
    pub armed: bool,
    /// Passes through bodies and keeps going (the Tornado's column).
    pub pass: bool,
    /// Players a pass-through object has already hit (bit = slot).
    pub hit: u8,
    pub flight: Flight,
}

impl Shot {
    pub fn new(def: &SkillDef, pos: Vec3, flight: Flight, life: u16) -> Shot {
        Shot {
            pos,
            prev: pos,
            homing: 1.0,
            delay: 0,
            life,
            blast: def.blast_radius(),
            through_walls: def.ignores_walls(),
            away: false,
            radius: SHOT_RADIUS,
            tick: 0,
            age: 0,
            armed: false,
            pass: false,
            hit: 0,
            flight,
        }
    }

    /// One program tick of flight (the family's update, `+0x1e0` handler):
    /// steers at `target` (the target's body centre) if there is one, moves.
    /// False when the flight itself is over (a Tornado with no floor under it).
    pub fn step(&mut self, target: Option<Vec3>, col: &Collision) -> bool {
        self.prev = self.pos;
        let target = target.filter(|_| self.homing > 0.0);
        // The updater counts the age up BEFORE the family's handler runs (`+0x1c`, `FUN_1402638d0`), so
        // the handlers' speed ramps see 1 on the first real step.
        let (h, away, age) = (self.homing, self.away, self.age + 1);
        let alive = match &mut self.flight {
            Flight::Shell(f) => {
                f.step(&mut self.pos, age, target, h, away);
                true
            }
            Flight::Lob(f) => {
                f.step(&mut self.pos, target, h, away);
                true
            }
            Flight::Fall(f) => {
                f.step(&mut self.pos, target, h, away);
                true
            }
            Flight::Crawl(f) => f.step(&mut self.pos, age, target, h, away, col),
            Flight::Arc(f) => {
                f.step(&mut self.pos, target, h, away);
                true
            }
            Flight::Ricochet(f) => {
                f.step(&mut self.pos, age);
                true
            }
            Flight::Tornado(f) => f.step(&mut self.pos, age, col),
        };
        self.age = self.age.saturating_add(1);
        alive
    }

    /// The segment the hit tests sweep this tick: last position -> position,
    /// except the Tornado, a column standing at its position.
    pub fn swept(&self) -> (Vec3, Vec3) {
        match self.flight {
            Flight::Tornado(f) => (self.pos + Vec3::Y * f.height, self.pos),
            _ => (self.prev, self.pos),
        }
    }

    /// Its heading (unit).
    #[allow(dead_code)]
    pub fn heading(&self) -> Vec3 {
        match self.flight {
            Flight::Shell(f) => f.fwd,
            Flight::Lob(f) => f.dir,
            Flight::Fall(f) => f.frame.fwd,
            Flight::Crawl(f) => f.fwd,
            Flight::Arc(f) => f.frame.fwd,
            Flight::Ricochet(f) => f.frame.fwd,
            Flight::Tornado(f) => f.fwd,
        }
    }

    /// A wall stopped the swept step at `at` with surface normal `n`. A
    /// Ricochet laser never ends on a wall: it turns (or ignores a face it is
    /// moving away from) and flies on (true); everything else ends (false).
    pub fn bounce(&mut self, at: Vec3, n: Vec3, target: Option<Vec3>) -> bool {
        let target = target.filter(|_| self.homing > 0.0);
        match &mut self.flight {
            Flight::Ricochet(f) => {
                if f.bounce(n, at, target) {
                    self.pos = at;
                }
                true
            }
            _ => false,
        }
    }

    /// A Reverse / About Face field turns it back on `to` (the former owner's
    /// body centre): the program re-runs the family's init with the new target
    /// (`FUN_14026cd70` codes 5/6); the ones that did not read their kind-5
    /// branch fly straight at it.
    pub fn send_back(&mut self, to: Vec3) {
        let here = self.pos;
        let toward = (to - here).normalize_or(Vec3::Z);
        self.away = false;
        match &mut self.flight {
            Flight::Shell(f) => f.fwd = -f.fwd,
            Flight::Lob(f) => f.reaim(here, to),
            Flight::Fall(f) => *f = fall::Flying { frame: Frame::look(toward), last: (0.0, 0.0), ..*f },
            Flight::Crawl(f) => f.fwd = Vec3::new(toward.x, 0.0, toward.z).normalize_or(f.fwd),
            Flight::Arc(f) => {
                f.frame = Frame::look(toward);
                f.curve = 0;
            }
            Flight::Ricochet(f) => f.frame = Frame::look(-f.frame.fwd),
            Flight::Tornado(_) => {}
        }
    }
}

/// Projectiles for a `Shoot` behaviour from a full launch context.
pub fn launch_from(kind: ShotKind, def: &SkillDef, l: &Launch) -> Vec<Shot> {
    match kind {
        ShotKind::Shell(s) => s.launch(def, l),
        ShotKind::Parabola(p) => p.launch(def, l),
        ShotKind::Arc(a) => a.launch(def, l),
        ShotKind::Crawl(c) => c.launch(def, l),
        ShotKind::Fall(f) => f.launch(def, l),
        ShotKind::Ricochet(r) => r.launch(def, l),
        ShotKind::Tornado(t) => t.launch(def, l),
    }
}

/// Runs an erase / status / special skill's effect (environment skills work
/// through the crystal hooks instead). False if that skill isn't built yet.
pub fn apply_effect(c: &mut crate::combat::Combat, players: &mut [crate::sim::Player], caster: usize, reach: effects::Reach, def: &SkillDef) -> bool {
    // Levels before the effect, for the Add sound's "something to reduce" gate below.
    let levels_before: Vec<(usize, i32)> = c.roster.slots().map(|i| (i, c.fighters[i].level)).collect();
    // What each fighter had before the effect, for the other "took hold" gates of the Add sound.
    let before: Vec<(usize, AddGate)> = c.roster.slots().map(|i| (i, AddGate::of(c, i))).collect();
    let lowest_level = c.roster.slots().filter(|&i| c.fighters[i].health > 0).map(|i| c.fighters[i].level).min();
    let done = match def.capsule {
        Capsule::Erase => erase::apply(c, players, caster, reach, def),
        Capsule::Status => status::apply(c, players, caster, reach, def),
        Capsule::Special => special::apply(c, players, caster, reach, def),
        _ => false,
    };
    // `FUN_140270a00` runs up to four effects per s43 = 4 skill (kind s45+k, amount s49+k) through
    // `FUN_140270b00`, which ends each with `FUN_140271590(fighter, kind, amount)`: the effect's own
    // sound at the affected fighter. Any capsule kind can carry them (Level Amp / Level Boost are
    // specials, Angel Tears is an erase). LIVE (341 takes): the sound matches in every s43 = 4 take
    // except kinds 8 and 0x12 (Refresh, Angel Tears' slot 0, Doppelganger), which logged nothing, so
    // those stay silent (`FUN_140270b00` returns early on a condition we have not read for kind 8).
    // (The Aura Particle's Level +1 is logged where the cast takes it, `Combat::cast_from`.)
    if done && def.raw[43] == 4 {
        for k in 0..4 {
            let (kind, amount) = (def.raw[45 + k], def.raw[49 + k] as i32);
            if kind == 0 || matches!(kind, 8 | 0x12 | 0x17) {
                continue;
            }
            let who: Vec<usize> = match reach {
                effects::Reach::Caster => vec![caster],
                effects::Reach::Target(t) => vec![t],
                effects::Reach::Everyone => c.roster.slots().collect(),
                effects::Reach::Field => Vec::new(),
            };
            for w in who {
                c.log(crate::combat::CombatEvent::StatusEffect { who: w, kind, amount });
            }
        }
    }
    // The Add effect of an s43 = 0 skill (code s51, amount s52: `FUN_140272bc0`) ends with
    // `FUN_1402738d0(victim, code, amount)`, its own sound (`sfx::add_effect_sound`). The Level
    // codes 2 / 4 return before it when the victim's Level is 0 (READ, `0x140272c83` cases 2-4).
    // A [Hole] erase reaches everyone, as `erase::apply` does.
    if done && def.raw[43] == 0 && def.raw[51] != 0 && def.capsule != Capsule::Attack {
        let who: Vec<usize> = match reach {
            effects::Reach::Caster => vec![caster],
            effects::Reach::Target(_) if def.capsule == Capsule::Erase && matches!(def.motion(), motion::HOLE_A | motion::HOLE_B) => c.roster.slots().collect(),
            effects::Reach::Target(t) => vec![t],
            effects::Reach::Everyone => c.roster.slots().collect(),
            effects::Reach::Field => Vec::new(),
        };
        for w in who {
            let had_level = levels_before.iter().any(|&(i, l)| i == w && l > 0);
            if matches!(def.raw[51], 2 | 4) && !had_level {
                continue;
            }
            let gate = before.iter().find(|&&(i, _)| i == w).map(|&(_, g)| g).unwrap_or_default();
            if !gate.took_hold(def.raw[51], lowest_level) {
                continue;
            }
            c.log(crate::combat::CombatEvent::AddEffect { who: w, code: def.raw[51], amount: def.raw[52] as i32 });
        }
    }
    done
}

/// What a victim had when an Add effect reached it: the program's `FUN_140272bc0` returns BEFORE its
/// sound (`FUN_1402738d0`) when the effect finds nothing to change, so only an effect that took hold
/// is heard. READ (`140272bc0`): code 0x11 (Neutralization / Purify) and 0x27 (Return to Nature, after
/// cancelling the crystal) return 0 when `FUN_14024d690` finds all 47 status slots empty; 0x2f (Vicious
/// Balance) returns 0 when the victim already has the lowest Level of the players present and alive;
/// 0x33 (Angel's Breath / Armaros) returns 0 unless one of the four stat statuses (slots 0-3: attack /
/// speed / homing / Aura rate, `FUN_14024d4d0`) is non-zero; 0x31 (Meltdown) when the victim has no
/// Aura Particle ready on its capsule spots. LIVE: takes 160 / 162 / 280 / 293 / 296 (the opponent has
/// no status, equal Levels, no particle) log no common/97, 98 or 90.
#[derive(Clone, Copy, Default)]
struct AddGate {
    level: i32,
    any_status: bool,
    any_stat: bool,
    particles: usize,
}

impl AddGate {
    fn of(c: &crate::combat::Combat, i: usize) -> Self {
        let f = &c.fighters[i];
        let s = &f.status;
        let any_stat = [effects::Stat::Attack, effects::Stat::Speed, effects::Stat::Homing, effects::Stat::AuraRate].into_iter().any(|k| c.stat(i, k) != 0);
        let any_status = any_stat
            || !s.mods.is_empty()
            || [s.bound_ticks, s.invisible_ticks, s.inverted_ticks, s.scrambled_ticks, s.immune_ticks, s.locked_ticks, s.hover_ticks, s.regen_every].iter().any(|&t| t > 0)
            || s.still_immune
            || f.frozen_ticks > 0
            || f.poison_ticks > 0
            || f.forced_lock.is_some()
            || f.status_hover;
        let particles = c.capsules.iter().filter(|item| item.owner == i && item.skill == crate::combat::AURA_PARTICLE && item.appear == 0).count();
        AddGate { level: f.level, any_status, any_stat, particles }
    }

    /// Whether the effect with this Add `code` made a change on the victim.
    fn took_hold(self, code: i16, lowest_level: Option<i32>) -> bool {
        match code {
            0x11 | 0x27 => self.any_status,
            0x2f => lowest_level.is_some_and(|low| self.level > low),
            0x33 => self.any_stat,
            0x31 => self.particles > 0,
            _ => true,
        }
    }
}

/// Projectiles for a `Shoot` behaviour; `caster` and `target` are feet
/// positions: a launch from the caster's chest at the target, facing it, with
/// a lock (Reflect / Brush, tests).
pub fn launch(kind: ShotKind, def: &SkillDef, caster: Vec3, target: Vec3) -> Vec<Shot> {
    let facing = Vec3::new(target.x - caster.x, 0.0, target.z - caster.z).normalize_or(Vec3::Z);
    let l = Launch { spawn: caster + Vec3::Y * CHEST, feet: caster, facing, pitch: 0.0, target: Some(body_centre(target)), index: 0, col: None, marks: &[] };
    launch_from(kind, def, &l)
}

#[cfg(test)]
mod add_gate_tests {
    use super::AddGate;

    /// `FUN_140272bc0` returns before the Add sound when the effect finds nothing to change (LIVE: takes
    /// 160 / 162 / 280 / 293 / 296, an opponent with no status, an equal Level, no Aura Particle, play none).
    #[test]
    fn an_add_effect_that_changes_nothing_is_silent() {
        let plain = AddGate { level: 12, any_status: false, any_stat: false, particles: 0 };
        for code in [0x11, 0x27, 0x2f, 0x33, 0x31] {
            assert!(!plain.took_hold(code, Some(12)), "code {code:#x} on a victim with nothing to change");
        }
        // The same effects with something to change are heard.
        assert!(AddGate { any_status: true, ..plain }.took_hold(0x11, Some(12)));
        assert!(AddGate { any_status: true, ..plain }.took_hold(0x27, Some(12)));
        assert!(AddGate { any_stat: true, ..plain }.took_hold(0x33, Some(12)));
        assert!(AddGate { particles: 1, ..plain }.took_hold(0x31, Some(12)));
        // Vicious Balance: only a victim above the lowest Level of the players present.
        assert!(plain.took_hold(0x2f, Some(8)));
        assert!(!plain.took_hold(0x2f, Some(12)));
        assert!(!plain.took_hold(0x2f, None));
        // Every other code keeps playing (the Level and stat codes are gated where they are applied).
        assert!(plain.took_hold(0xb, Some(12)) && plain.took_hold(0xd, Some(12)) && plain.took_hold(0x2c, Some(12)));
    }
}
