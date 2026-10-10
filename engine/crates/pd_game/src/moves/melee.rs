//! Short-range attacks (s42 = 10; motion 12 sword, 13 punch / kick / palm /
//! wave, 14 Pursuit): dash at the target, strike inside a hit window whether
//! or not the dash arrived. The dash and hit windows (ticks after cast start)
//! and the lock-on / range gate come from the skill's action-table row and
//! `combat::Combat::lunge` (docs/FORMATS-SKILLS.md "Action table").
//!
//! Read from the program (`FUN_14025a3e0` -> `FUN_14018f050` / `FUN_14018f0f0`
//! the dash numbers, `FUN_14025be40` the dash, `FUN_14026c340` / `FUN_14026c730`
//! the hits):
//! - **dash speed** = (10 / dash length in ticks) x s65 u per program tick,
//!   never more than 10 (s65 = 8 over a 10-tick dash: 8 u a tick = 240 u/s;
//!   over a kick's 12 ticks 6.67 u a tick -- the same 80 u in all: the dash
//!   is "s65 x 10 u in the window", not a flat 240 u/s); **turn cap** s66 / 10
//!   degrees a tick (swords 1, punches 3, Pursuit Stopper 30);
//! - the dash needs a locked target; it stops 11 u from the target's centre
//!   (the last step is shortened by `speed - (distance - 11)`, which can end
//!   slightly inside);
//! - **punch / kick / wave** (13): a sphere of s64 x 0.1 u (13 u for all of
//!   them) plus the target's body radius on the weapon marker (`SHOT01` of the
//!   cast effect, else a caster matrix) -- here the caster's centre, the
//!   marker positions are not read out of the effect files yet;
//! - **sword** (12): a blade, the segment from the attached point along its
//!   forward axis, s64 x 0.1 u long (13; Laser Blade 15; Ice / Blazing / Frostbite
//!   25; Excalibur / Mist Blade 30), tested against the target's centre with a
//!   fixed 12 u radius, for last tick's blade and this tick's;
//! - **Pursuit / Sky Pursuit** (14): dash ticks 3-21 and **one** test, at tick
//!   19 (the fire tick), of a sphere of s64 x 0.1 = 25 u plus the body radius
//!   around the caster's centre, against the locked target only.

use super::{closest_on_segment, motion, HIT_BODY_RADIUS, SPEED_UNIT, TABLE_DIST};
use crate::combat::SkillDef;
use glam::Vec3;

/// Used when a skill has no reach in the table.
pub const DEFAULT_REACH: f32 = 14.0;
/// A sword's blade is tested against the target's centre with this radius
/// (`0x1402f7f9c`).
pub const BLADE_RADIUS: f32 = 12.0;
/// The dash never covers more than this a program tick (`0x1402f7f7c`).
pub const MAX_DASH_STEP: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Motion 12: a blade segment.
    Sword,
    /// Motion 13: a sphere on the weapon marker.
    Sphere,
    /// Motion 14: one sphere test at the fire tick, against the locked target.
    Stamp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melee {
    /// Sphere radius / blade length / stamp radius (s64 x 0.1).
    pub reach: f32,
    pub shape: Shape,
    /// The table's dash numbers (s65, s66).
    pub dash: i16,
    pub turn: i16,
}

/// Motion 12 sword, 13 punch/kick, 14 pursuit (Pursuit / Sky Pursuit: s64
/// 250 reach, "Requires: Knocked down target" -- they dash at a downed foe).
pub fn is_melee(def: &SkillDef) -> bool {
    matches!(def.motion(), 12 | 13 | motion::PURSUIT)
}

impl Melee {
    pub fn from_def(def: &SkillDef) -> Self {
        let reach = def.param(0) as f32 * TABLE_DIST;
        Melee {
            reach: if reach > 0.0 { reach } else { DEFAULT_REACH },
            shape: match def.motion() {
                12 => Shape::Sword,
                motion::PURSUIT => Shape::Stamp,
                _ => Shape::Sphere,
            },
            dash: def.param(1),
            turn: def.param(2),
        }
    }

    /// Units per program tick of a dash window `len` ticks long
    /// (`FUN_14018f050`): (10 / len) x s65, capped at 10; a negative s65 is a
    /// plain 1/10.8 per point. 0 for a window of no length.
    pub fn dash_step(&self, len: u16) -> f32 {
        if len == 0 {
            return 0.0;
        }
        let s = self.dash as f32;
        let step = if s >= 0.0 { 10.0 / len as f32 * s } else { SPEED_UNIT * s };
        step.min(MAX_DASH_STEP)
    }

    /// Radians the dash can turn toward the target per program tick
    /// (`FUN_14018f0f0`: s66 / 10 degrees).
    pub fn dash_turn(&self) -> f32 {
        (self.turn.max(0) as f32 / 10.0).to_radians()
    }

    /// Whether the blow reaches a target whose body centre is `target`, from a
    /// weapon at `from` facing `facing` (sword blades are tested for this
    /// tick's blade and `last` tick's).
    pub fn reaches(&self, from: Vec3, facing: Vec3, last: Option<(Vec3, Vec3)>, target: Vec3) -> bool {
        match self.shape {
            Shape::Sphere | Shape::Stamp => from.distance(target) < self.reach + HIT_BODY_RADIUS,
            Shape::Sword => {
                let now = (from, from + facing * self.reach);
                let hit = |(a, b): (Vec3, Vec3)| closest_on_segment(a, b, target).distance(target) <= BLADE_RADIUS;
                hit(now) || last.is_some_and(hit)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::skills::field;

    fn melee(motion: i16, p: [i16; 3]) -> Melee {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (3, Capsule::Attack);
        d.raw[field::MOTION] = motion;
        for (k, v) in p.into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        Melee::from_def(&d)
    }

    /// Psycho Knife 130 / 8 / 10 over a 10-tick dash: 8 u a tick; a kick's
    /// 12-tick dash: 6.67 u a tick -- 80 u either way; and never over 10 a
    /// tick (`FUN_14018f050`, the cap in `FUN_14025a3e0`).
    #[test]
    fn dash_speed_is_ten_over_the_window_times_s65() {
        let knife = melee(12, [130, 8, 10]);
        assert!((knife.dash_step(10) - 8.0).abs() < 1e-5);
        let kick = melee(13, [130, 8, 30]);
        assert!((kick.dash_step(12) - 80.0 / 12.0).abs() < 1e-5);
        assert!((kick.dash_step(12) * 12.0 - 80.0).abs() < 1e-4, "the same 80 u");
        assert_eq!(kick.dash_step(4), 10.0, "capped");
        assert!((knife.dash_turn() - 1.0f32.to_radians()).abs() < 1e-6, "swords turn 1 deg a tick");
        assert!((kick.dash_turn() - 3.0f32.to_radians()).abs() < 1e-6, "punches 3");
    }

    /// A punch is a 13 u sphere + the 5 u body; a sword a segment of its
    /// length tested at 12 u from the target's centre.
    #[test]
    fn punch_is_a_sphere_and_a_sword_a_blade() {
        let punch = melee(13, [130, 8, 30]);
        let me = Vec3::new(0.0, 9.0, 0.0);
        assert!(punch.reaches(me, Vec3::Z, None, Vec3::new(0.0, 9.0, 17.9)));
        assert!(!punch.reaches(me, Vec3::Z, None, Vec3::new(0.0, 9.0, 18.1)));
        let sword = melee(12, [130, 8, 10]);
        assert!(sword.reaches(me, Vec3::Z, None, Vec3::new(0.0, 9.0, 24.0)), "13 u blade + 12 u");
        assert!(!sword.reaches(me, Vec3::Z, None, Vec3::new(20.0, 9.0, 6.0)), "off to the side of the blade");
        assert!(sword.reaches(me, Vec3::X, Some((me, me + Vec3::Z * 13.0)), Vec3::new(0.0, 9.0, 24.0)), "last tick's blade counts too");
    }
}
