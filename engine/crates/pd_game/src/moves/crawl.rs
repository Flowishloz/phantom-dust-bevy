//! Attacks that travel along the ground: [Crawl] (motion 5: Root of Tree,
//! Crawlers, Crawling Bullet, Ground Laser, Young White Dragon).
//!
//! Guide: "cause the shell to crawl on the ground to its target. If an object
//! is in its path or if no ground is beneath it, this skill type becomes
//! ineffective--for example, if the case of the target being in the air."
//!
//! Read from the program (`FUN_140266060` init, `FUN_140266290` update):
//! - it starts at the spawn point heading along the caster's facing flattened
//!   to the ground;
//! - speed ramps s65 -> s66 over s67 ticks (Crawlers 30 -> 83 u/s, Root of
//!   Tree 60 -> 167, Crawling Bullet 110 -> 306, Ground Laser 0 -> 80 over 90
//!   ticks); range s64 (-80 = 800 u, Crawling Bullet -30 = 300 u);
//! - it steers only while it touches the floor, and the turn *accelerates*:
//!   each tick the turn rate grows by s68 x 0.01 deg x homing % (500 = 5 deg
//!   a tick squared) up to s69 x 0.1 deg a tick (100 = 10 deg, 300 deg/s), or
//!   snaps onto the target when the bearing error is under one step; past a
//!   Course field it turns away at 5 deg a tick;
//! - off an edge it falls (0.12 u per tick squared) and goes on when it lands;
//!   it ends at a wall or when its range runs out.

use super::{life_ticks, ramp, Flight, Launch, Shot, DEG, GRAVITY_TICK, SPEED_UNIT};
use crate::combat::SkillDef;
use crate::sim::STEP_HEIGHT;
use glam::Vec3;
use pd_formats::collision::Collision;

/// A [Course] field makes a crawler turn away at this much a tick (`0x1402f7cd4`).
pub const AWAY_TURN: f32 = 5.0 * DEG;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crawl {
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    /// Turn acceleration, radians per program tick squared at 100 % homing (s68 x 0.01 deg).
    pub accel: f32,
    /// Top turn rate, radians per program tick (s69 x 0.1 deg).
    pub cap: f32,
    pub life: u16,
}

impl Crawl {
    pub fn from_def(def: &SkillDef) -> Self {
        let (s64, s65, s66, s67) = (def.param(0), def.param(1), def.param(2), def.param(3));
        Crawl {
            speed: s65.max(0) as f32 * SPEED_UNIT,
            end_speed: s66.max(0) as f32 * SPEED_UNIT,
            ramp: s67,
            accel: def.param(4).max(0) as f32 * 0.01 * DEG,
            cap: def.param(5).max(0) as f32 * 0.1 * DEG,
            life: life_ticks(s64, s65, s66, s67),
        }
    }

    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let f = Flying { fwd: l.facing, speed: self.speed, end_speed: self.end_speed, ramp: self.ramp, accel: self.accel, cap: self.cap, rate: 0.0, vy: 0.0 };
        vec![Shot::new(def, l.feet + Vec3::Y, Flight::Crawl(f), self.life)]
    }
}

/// A crawler on the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    /// Horizontal heading, unit.
    pub fwd: Vec3,
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    pub accel: f32,
    pub cap: f32,
    /// The turn rate it has built up (radians per tick, signed).
    pub rate: f32,
    pub vy: f32,
}

fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a - tau
    } else if a < -std::f32::consts::PI {
        a + tau
    } else {
        a
    }
}

impl Flying {
    /// `FUN_140266290`: move at the ramped speed (and fall by `vy`), find the
    /// floor; on it, steer with an accelerating turn and stop falling.
    pub fn step(&mut self, pos: &mut Vec3, age: u16, target: Option<Vec3>, homing: f32, away: bool, col: &Collision) -> bool {
        *pos += Vec3::Y * self.vy + self.fwd * ramp(self.speed, self.end_speed, self.ramp, age);
        let floor = col.ground_below_object(pos.x, pos.z, pos.y + STEP_HEIGHT).filter(|g| pos.y - g <= STEP_HEIGHT);
        match floor {
            None => self.vy -= GRAVITY_TICK,
            Some(g) => {
                pos.y = g + 1.0;
                if let Some(t) = target {
                    let (mut to, mut accel) = (Vec3::new(t.x - pos.x, 0.0, t.z - pos.z), self.accel * homing);
                    if away {
                        (to, accel) = (-to, AWAY_TURN);
                    }
                    let diff = wrap(to.x.atan2(to.z) - self.fwd.x.atan2(self.fwd.z));
                    self.rate = if diff.abs() >= accel {
                        if diff > 0.0 { (self.rate + accel).min(self.cap) } else { (self.rate - accel).max(-self.cap) }
                    } else {
                        diff
                    };
                    let (s, c) = self.rate.sin_cos();
                    self.fwd = Vec3::new(self.fwd.x * c + self.fwd.z * s, 0.0, self.fwd.z * c - self.fwd.x * s).normalize_or(self.fwd);
                }
                self.vy = 0.0;
            }
        }
        pos.y > -2000.0
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

    fn root() -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (138, Capsule::Attack);
        d.raw[field::MOTION] = 5;
        for (k, v) in [-80, 60, 0, 0, 500, 100].into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d
    }

    fn launch(d: &SkillDef) -> Shot {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target: None, index: 0, col: None, marks: &[] };
        Crawl::from_def(d).launch(d, &l).remove(0)
    }

    /// Root of Tree -80 / 60 / 0 / 0 / 500 / 100 (`FUN_140266060`): 60 / 10.8 =
    /// 5.56 u a tick (167 u/s), 800 u range, turn acceleration 5 deg a tick
    /// squared up to 10 deg a tick.
    #[test]
    fn root_of_tree_numbers() {
        let c = Crawl::from_def(&root());
        assert!((c.speed - 5.555).abs() < 0.01);
        assert_eq!(c.life, (800.0 / (60.0 / 10.8)) as u16);
        assert!((c.accel - 5.0 * DEG).abs() < 1e-5 && (c.cap - 10.0 * DEG).abs() < 1e-5);
    }

    /// The turn builds up 5, 10, 10 ... degrees a tick toward a target off to
    /// the side, and only while it is on the floor.
    #[test]
    fn the_turn_accelerates_and_needs_the_floor() {
        let col = floor();
        let mut s = launch(&root());
        let target = Vec3::new(400.0, 9.0, 0.0);
        let mut bearings = Vec::new();
        for _ in 0..4 {
            s.step(Some(target), &col);
            bearings.push(s.heading().x.atan2(s.heading().z).to_degrees());
        }
        assert!((bearings[0] - 5.0).abs() < 0.1, "{bearings:?}");
        assert!((bearings[1] - 15.0).abs() < 0.1, "{bearings:?}");
        assert!((bearings[2] - 25.0).abs() < 0.1, "{bearings:?}");
        // Over a gap there is no steering, only the fall.
        let hole = Collision::new(vec![]);
        let mut s = launch(&root());
        s.step(Some(target), &hole);
        assert!(s.heading().x.abs() < 1e-6, "no floor: it flies straight");
        let Flight::Crawl(f) = s.flight else { panic!() };
        assert!(f.vy < 0.0, "and falls");
    }

    /// Past a Course field it turns away at 5 deg a tick.
    #[test]
    fn a_course_field_turns_it_away() {
        let col = floor();
        let mut s = launch(&root());
        s.away = true;
        s.step(Some(Vec3::new(300.0, 9.0, 300.0)), &col);
        assert!(s.heading().x < 0.0, "turned from the target: {:?}", s.heading());
    }
}
