//! Tornado attacks (motion 10, `[Move]`: Twister, Whirlwind, Tornado).
//!
//! Guide: Tornado types "have the same effect as the Crawl types: moving along
//! the ground to the target" -- the program's code is a different handler from
//! the crawler's (`FUN_140268590` init, `FUN_140268940` update):
//! - it is placed on the floor at the spawn point and **moves straight along
//!   the caster's facing, with no homing at all**;
//! - speed ramps s67 -> s68 over s69 ticks: Twister 100 -> 0 over 60 (starts
//!   at 278 u/s, stops after 2 s), Whirlwind 40 -> 0 over 90, Tornado 0 -> 100
//!   over 90 (starts still, speeds up); it lives s64 ticks (90 / 150 / 90);
//! - its column has radius s65 x 0.1 and height s66 x 10 - 2 (98 / 118 / 98
//!   u), or up to the ceiling; it hits everything standing in it (swept) and
//!   passes through players (each is hit once) -- **including its own caster
//!   after its first 30 ticks**;
//! - it follows the floor within +-7.5 u; no floor under it ends it; a face
//!   crossing its column at the new spot (a low roof, an overhang) keeps it
//!   where it was, and it stays alive. There is no sideways wall ray: a wall
//!   stops it only through the column or the floor test (READ 2026-10-09);
//! - the hit reaction is row 45: a straight-up launch (~89 u, ~2.6 s).
//!
//! (The first reading folded it into the crawler with a homing turn of 1.5
//! rad/s and used s64 as a speed.)

use super::{ramp, Flight, Launch, Shot, SPEED_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;
use pd_formats::collision::Collision;

/// Ticks before its own caster can be hit (`+0x1cc` = 0x1e).
pub const CASTER_SAFE_TICKS: u16 = 30;
/// How far above/below it looks for the floor each tick (`0x1402f7f58`).
pub const FLOOR_REACH: f32 = 7.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tornado {
    pub life: u16,
    pub radius: f32,
    /// Column height when nothing is overhead (s66 x 10 - 2).
    pub height: f32,
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
}

impl Tornado {
    pub fn from_def(def: &SkillDef) -> Self {
        Tornado {
            life: def.param(0).max(0) as u16,
            radius: def.param(1).max(0) as f32 * 0.1,
            height: def.param(2) as f32 * 10.0 - 2.0,
            speed: def.param(3).max(0) as f32 * SPEED_UNIT,
            end_speed: def.param(4).max(0) as f32 * SPEED_UNIT,
            ramp: def.param(5),
        }
    }

    /// Placed on the floor under the caster (nothing under it: it fails
    /// silently), heading along the facing.
    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let floor = match l.col {
            Some(c) => match c.ground_below_object(l.feet.x, l.feet.z, l.feet.y + 20.0).filter(|g| *g >= l.feet.y - 20.0) {
                Some(g) => g,
                None => return Vec::new(),
            },
            None => l.feet.y,
        };
        let base = Vec3::new(l.feet.x, floor + 1.0, l.feet.z);
        // A roof within reach shortens the column.
        let room = l.col.and_then(|c| c.raycast_object(base, Vec3::Y, self.height)).map_or(self.height, |t| (t - 1.0).max(0.0));
        let f = Flying { fwd: l.facing, speed: self.speed, end_speed: self.end_speed, ramp: self.ramp, height: room, wait: CASTER_SAFE_TICKS };
        let mut s = Shot::new(def, base, Flight::Tornado(f), self.life);
        s.radius = self.radius;
        s.pass = true;
        vec![s]
    }
}

/// A tornado on the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    pub fwd: Vec3,
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    pub height: f32,
    /// Ticks until it can hit its own caster.
    pub wait: u16,
}

impl Flying {
    /// `FUN_140268940`: move along the heading at the ramped speed; look for
    /// the floor within 7.5 u (none: it ends); then test only the column at
    /// the new spot (`FUN_140163680`, floor + 1 up by the column height): a
    /// face crossing it puts x / z back where it stood. READ: there is no
    /// sideways wall ray; a wall stops it only through the column or the floor.
    pub fn step(&mut self, pos: &mut Vec3, age: u16, col: &Collision) -> bool {
        self.wait = self.wait.saturating_sub(1);
        let before = *pos;
        let step = self.fwd * ramp(self.speed, self.end_speed, self.ramp, age);
        let to = before + step;
        let Some(g) = col.ground_below_object(to.x, to.z, to.y + FLOOR_REACH).filter(|g| *g >= to.y - FLOOR_REACH) else {
            *pos = to;
            return false;
        };
        let base = Vec3::new(to.x, g + 1.0, to.z);
        if col.raycast_object(base, Vec3::Y, self.height).is_some() {
            return true;
        }
        *pos = base;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::skills::field;

    fn floor() -> Collision {
        let s = 1000.0;
        Collision::new(vec![[Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)], [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)]])
    }

    fn twister() -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (140, Capsule::Attack);
        d.raw[field::MOTION] = 10;
        for (k, v) in [90, 100, 10, 100, 0, 60].into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d
    }

    fn launch(d: &SkillDef, col: &Collision) -> Vec<Shot> {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target: Some(Vec3::new(80.0, 9.0, 50.0)), index: 0, col: Some(col), marks: &[] };
        Tornado::from_def(d).launch(d, &l)
    }

    /// Twister 90 / 100 / 10 / 100 / 0 / 60 (`FUN_140268590`): lives 90 ticks,
    /// 10 u radius, 98 u column; starts at 100 / 10.8 = 9.26 u a tick (278 u/s)
    /// and slows to 0 over 60 ticks.
    #[test]
    fn twister_numbers_and_ramp() {
        let col = floor();
        let d = twister();
        let s = launch(&d, &col).remove(0);
        assert_eq!(s.life, 90);
        assert!((s.radius - 10.0).abs() < 1e-5 && s.pass);
        let Flight::Tornado(f) = s.flight else { panic!() };
        assert!((f.height - 98.0).abs() < 1e-3);
        assert!((ramp(f.speed, f.end_speed, f.ramp, 0) - 100.0 / 10.8).abs() < 1e-4);
        assert_eq!(ramp(f.speed, f.end_speed, f.ramp, 60), 0.0);
    }

    /// No homing at all: it goes straight down the caster's facing even with
    /// the target off to the side; it can hit its own caster after 30 ticks.
    #[test]
    fn it_does_not_home_and_its_caster_is_safe_for_thirty_ticks() {
        let col = floor();
        let mut s = launch(&twister(), &col).remove(0);
        for _ in 0..5 {
            assert!(s.step(Some(Vec3::new(80.0, 9.0, 50.0)), &col));
        }
        assert!(s.pos.x.abs() < 1e-4 && s.pos.z > 40.0, "{:?}", s.pos);
        let Flight::Tornado(f) = s.flight else { panic!() };
        assert_eq!(f.wait, 25);
    }

    /// READ `FUN_140268940`: only the column at the new spot is tested (floor + 1 up by its height), never a
    /// sideways ray, so an upright wall it crosses does not stop it (LIVE `311_Tornado`: the original's column
    /// reaches its target at t91); a roof over the next spot does, and it stays alive where it stood.
    #[test]
    fn a_wall_does_not_stop_it_only_its_column_does() {
        let s = 1000.0;
        let mut tris = vec![[Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)], [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)]];
        // An upright wall across its path at z = 20.
        tris.push([Vec3::new(-50.0, 0.0, 20.0), Vec3::new(50.0, 0.0, 20.0), Vec3::new(50.0, 40.0, 20.0)]);
        tris.push([Vec3::new(-50.0, 0.0, 20.0), Vec3::new(50.0, 40.0, 20.0), Vec3::new(-50.0, 40.0, 20.0)]);
        let walled = Collision::new(tris);
        let mut t = launch(&twister(), &walled).remove(0);
        for _ in 0..5 {
            assert!(t.step(None, &walled));
        }
        assert!(t.pos.z > 40.0, "it went through the wall: {:?}", t.pos);
        // A roof 50 u up from z = 25 on: the column meets it there and it stops short, still alive.
        let mut tris = vec![[Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)], [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)]];
        tris.push([Vec3::new(-s, 50.0, 25.0), Vec3::new(s, 50.0, 25.0), Vec3::new(s, 50.0, s)]);
        tris.push([Vec3::new(-s, 50.0, 25.0), Vec3::new(s, 50.0, s), Vec3::new(-s, 50.0, s)]);
        let roofed = Collision::new(tris);
        let mut t = launch(&twister(), &roofed).remove(0);
        for _ in 0..5 {
            assert!(t.step(None, &roofed));
        }
        assert!(t.pos.z < 25.0 && t.pos.z > 15.0, "{:?}", t.pos);
    }

    /// Nothing under it: it fails at once (no launch) / ends.
    #[test]
    fn without_a_floor_it_ends() {
        let none = Collision::new(vec![]);
        assert!(launch(&twister(), &none).is_empty());
    }
}
