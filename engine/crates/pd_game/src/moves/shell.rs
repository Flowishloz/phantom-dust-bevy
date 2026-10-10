//! Shell-type attacks (motion 0: Blaster, Bullet of Fire, Rail Gun, Faith
//! fires...). Guide: "the most basic of Attack skills. They are medium- or
//! long-range attacks ... a straight trajectory and is easy to follow."
//!
//! Read from the program (`FUN_140264570` init, `FUN_140264710` update,
//! `FUN_1402636f0` life; docs/MOVES.md "Verdicts"):
//! - speed ramps from s65 to s66 over s67 program ticks (s67 = 0: s65 all the
//!   way), 1 point = 1/10.8 u per tick; Psycho Sniper and Dragon Bullet start
//!   at 0 and speed up -- they do not wait;
//! - range: s64 < 0 is a range of -s64 x 10 u (Blaster 300 u = 23 ticks),
//!   s64 >= 0 a life in ticks;
//! - homing s68: at most s68 x 0.1 deg per tick x the caster's Homing status,
//!   only while the target is within 60 deg of its heading (dot > 0.5);
//!   past a [Course] field the same turn goes *away* from the target.

use super::{life_ticks, ramp, rotate_toward, Launch, Shot, SPEED_UNIT, TURN_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shell {
    /// Units per program tick at the start (s65).
    pub speed: f32,
    /// ... and after the ramp (s66).
    pub end_speed: f32,
    /// Program ticks the ramp takes (s67; 0 = no ramp).
    pub ramp: i16,
    /// Radians per program tick at 100 % homing (s68).
    pub turn: f32,
    /// Program ticks it lives.
    pub life: u16,
}

/// A shell with flight numbers in the table (not e.g. Recollection, which only copies a skill).
pub fn has_flight(def: &SkillDef) -> bool {
    def.param(1) > 0 || def.param(2) > 0
}

impl Shell {
    /// For families that aren't built yet (the M1 generic homing shot; the
    /// enemy-only motions 18 / 19 have no handler in the program).
    pub const PLACEHOLDER: Shell = Shell { speed: 170.0 / 30.0, end_speed: 170.0 / 30.0, ramp: 0, turn: 1.5 / 30.0, life: 160 };

    pub fn from_def(def: &SkillDef) -> Self {
        let (s64, s65, s66, s67) = (def.param(0), def.param(1), def.param(2), def.param(3));
        Shell {
            speed: s65.max(0) as f32 * SPEED_UNIT,
            end_speed: s66.max(0) as f32 * SPEED_UNIT,
            ramp: s67,
            // [Object] (Power, Psycho Kinesis) turns by s71 (100 = 10 deg a tick) -- its s68 is the lift.
            turn: if def.motion() == super::motion::OBJECT { def.s70_73[1].max(0) as f32 * TURN_UNIT } else { def.param(4).max(0) as f32 * TURN_UNIT },
            life: life_ticks(s64, s65, s66, s67),
        }
    }

    /// The shot flies along the caster's facing (the fire matrix) and homes
    /// onto the target from its first tick (when `l.target` is set; the sim
    /// zeroes `Shot::homing` for an unlocked cast).
    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let f = Flying { fwd: l.facing, speed: self.speed, end_speed: self.end_speed, ramp: self.ramp, turn: self.turn };
        vec![Shot::new(def, l.spawn, super::Flight::Shell(f), self.life)]
    }
}

/// A shell in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    pub fwd: Vec3,
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    pub turn: f32,
}

impl Flying {
    /// `FUN_140264710`: turn toward the target if it is within 60 deg of the
    /// heading, then move along the heading at the ramped speed.
    pub fn step(&mut self, pos: &mut Vec3, age: u16, target: Option<Vec3>, homing: f32, away: bool) {
        if let Some(dir) = target.and_then(|t| (t - *pos).try_normalize()) {
            if dir.dot(self.fwd) > 0.5 {
                self.fwd = rotate_toward(self.fwd, if away { -dir } else { dir }, self.turn * homing);
            }
        }
        *pos += self.fwd * ramp(self.speed, self.end_speed, self.ramp, age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::collision::Collision;
    use pd_formats::skills::field;

    fn shell(s: [i16; 5]) -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (70, Capsule::Attack);
        for (k, v) in s.into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d
    }

    fn launch(d: &SkillDef, target: Option<Vec3>) -> Shot {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target, index: 0, col: None, marks: &[] };
        Shell::from_def(d).launch(d, &l).remove(0)
    }

    /// Blaster -30 / 140 / 0 / 0 / 15 (`FUN_140264570`, `FUN_1402636f0`): 1 point
    /// is 1/10.8 u a program tick, so 140 is 12.96 u/tick = 389 u/s, and the
    /// -30 is a range of 300 u = 23 ticks (0.77 s), not the 900 u it flew.
    #[test]
    fn blaster_speed_and_range_are_the_programs() {
        let d = shell([-30, 140, 0, 0, 15]);
        let s = Shell::from_def(&d);
        assert!((s.speed - 12.963).abs() < 0.01, "{}", s.speed);
        assert_eq!(s.life, 23);
        assert!((s.turn - 15.0 * 0.1f32.to_radians()).abs() < 1e-6, "1.5 deg a tick");
        assert_eq!(life_ticks(300, 0, 0, 0), 300, "s64 >= 0 is a life in ticks");
    }

    /// Psycho Sniper -80 / 0 / 380 / 60 / 4: it starts at rest and speeds up to
    /// 380 over 60 ticks; it does not wait (`FUN_140264710`). Its 800 u range
    /// is spent inside the ramp, so it lives the ramp's 60 ticks.
    #[test]
    fn sniper_speeds_up_from_rest() {
        let d = shell([-80, 0, 380, 60, 4]);
        let s = Shell::from_def(&d);
        assert_eq!((s.speed, s.ramp, s.life), (0.0, 60, 60));
        let mid = ramp(s.speed, s.end_speed, s.ramp, 30);
        assert!(mid > 0.0 && mid < s.end_speed, "{mid}");
        assert!((ramp(s.speed, s.end_speed, s.ramp, 60) - 380.0 * SPEED_UNIT).abs() < 1e-4);
    }

    /// A homing shell only turns while the target is within 60 deg of its
    /// heading, at most s68 x 0.1 deg a tick.
    #[test]
    fn homing_is_limited_to_a_cone_and_a_turn_rate() {
        let d = shell([-30, 140, 0, 0, 15]);
        let col = Collision::new(vec![]);
        let mut sh = launch(&d, Some(Vec3::new(50.0, 9.0, 50.0)));
        sh.step(Some(Vec3::new(50.0, 9.0, 50.0)), &col);
        let turned = sh.heading().angle_between(Vec3::Z);
        assert!((turned - 1.5f32.to_radians()).abs() < 1e-3, "1.5 deg in one tick, got {}", turned.to_degrees());
        // 90 deg off: straight on.
        let mut sh = launch(&d, Some(Vec3::new(80.0, 12.0, 0.0)));
        sh.step(Some(Vec3::new(80.0, 12.0, 0.0)), &col);
        assert!(sh.heading().angle_between(Vec3::Z) < 1e-4, "outside the cone it flies straight");
        // Past a Course field it turns away.
        let mut sh = launch(&d, Some(Vec3::new(10.0, 12.0, 100.0)));
        sh.away = true;
        sh.step(Some(Vec3::new(10.0, 12.0, 100.0)), &col);
        assert!(sh.heading().x < 0.0, "turned the other way");
    }
}
