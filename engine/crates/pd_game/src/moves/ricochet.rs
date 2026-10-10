//! Ricochet Laser and the other multi-lasers (motion 17): "Shoots 4 lasers
//! that ricochet off objects."
//!
//! Read from the program (`FUN_14026af40` init, `FUN_14026b240` update,
//! `FUN_14026ca50` the wall):
//! - straight flight, speed ramping s65 -> s66 over s67 ticks (Ricochet 80:
//!   222 u/s; Impact Burst 40 -> 200 over 80 ticks), 1 point = 1/10.8 u a
//!   tick; the life is s64 ticks (Ricochet 90 = 3 s) or a range when s64 < 0
//!   (Diffused Laser -120 = 1200 u);
//! - each shot k (index mod s70) leaves rolled s69 + k x s72 degrees about the
//!   line of fire and then turned s68 + k x s71 degrees: Ricochet's four are
//!   45 deg off the line in an X (up-left, up-right, down-right, down-left);
//!   the number of shots is the `SHOTnn` marker count of the cast effect
//!   (`FUN_14018a310`), s70 here;
//! - a wall turns it by reflecting the heading off the wall's normal; the
//!   bounce counter s73 (Ricochet 3, Impact Burst 1): when it reaches 0 and
//!   the target is on the open side of the wall the laser turns straight at
//!   the target instead; after that it goes on reflecting. It never ends on a
//!   wall; only its life ends it.

use super::{life_ticks, ramp, Flight, Frame, Launch, Shot, DEG, SPEED_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ricochet {
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    pub life: u16,
    /// Shot 0's yaw and roll, radians (s68, s69) and the per-shot steps (s71, s72).
    pub yaw: f32,
    pub roll: f32,
    pub yaw_step: f32,
    pub roll_step: f32,
    /// Shots in the pattern (s70).
    pub pattern: i16,
    /// Bounces before it turns on the target (s73).
    pub bounces: i16,
}

impl Ricochet {
    pub fn from_def(def: &SkillDef) -> Self {
        let (s64, s65, s66, s67) = (def.param(0), def.param(1), def.param(2), def.param(3));
        Ricochet {
            speed: s65.max(0) as f32 * SPEED_UNIT,
            end_speed: s66.max(0) as f32 * SPEED_UNIT,
            ramp: s67,
            life: life_ticks(s64, s65, s66, s67),
            yaw: def.param(4) as f32 * DEG,
            roll: def.param(5) as f32 * DEG,
            yaw_step: def.s70_73[1] as f32 * DEG,
            roll_step: def.s70_73[2] as f32 * DEG,
            pattern: def.s70_73[0],
            bounces: def.s73(),
        }
    }

    /// Shots a cast makes: the marker count in the program, s70 here.
    pub fn count(&self) -> u32 {
        self.pattern.max(1) as u32
    }

    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        (0..self.count())
            .map(|m| {
                let k = if self.pattern > 1 { ((l.index + m) % self.pattern as u32) as f32 } else { 0.0 };
                let frame = Frame::fire(l.facing, l.pitch).roll_program(self.roll + self.roll_step * k).yaw_program(self.yaw + self.yaw_step * k);
                let f = Flying { frame, speed: self.speed, end_speed: self.end_speed, ramp: self.ramp, bounces: self.bounces };
                let mut s = Shot::new(def, l.spawn_of(m), Flight::Ricochet(f), self.life);
                s.through_walls = false;
                s
            })
            .collect()
    }
}

/// A ricochet laser in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    pub frame: Frame,
    pub speed: f32,
    pub end_speed: f32,
    pub ramp: i16,
    pub bounces: i16,
}

impl Flying {
    pub fn step(&mut self, pos: &mut Vec3, age: u16) {
        *pos += self.frame.fwd * ramp(self.speed, self.end_speed, self.ramp, age);
    }

    /// `FUN_14026ca50` (motion 17): a wall with outward normal `n` at `at`.
    /// A laser moving away from the face ignores it; otherwise the bounce
    /// counter drops and the heading is reflected -- or, when the counter
    /// reaches 0 with the target on the open side, aimed at the target.
    /// True if it turned.
    pub fn bounce(&mut self, n: Vec3, at: Vec3, target: Option<Vec3>) -> bool {
        let d = self.frame.fwd;
        if d.dot(n) >= 0.0 {
            return false;
        }
        self.bounces -= 1;
        let mut dir = d - n * (2.0 * d.dot(n));
        if self.bounces == 0 {
            if let Some(v) = target.map(|t| t - at).filter(|v| v.dot(n) > 0.0) {
                dir = v;
            }
        }
        self.frame = Frame::look(dir);
        true
    }
}

impl SkillDef {
    /// s73: the Ricochet's bounce counter.
    pub fn s73(&self) -> i16 {
        self.s70_73[3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::skills::field;

    fn ricochet() -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (77, Capsule::Attack);
        d.raw[field::MOTION] = 17;
        for (k, v) in [90, 80, 80, 0, -45, -45].into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d.s70_73 = [4, 0, 90, 3];
        d
    }

    /// Ricochet Laser -> 90 / 80 / 80 / 0 / -45 / -45 + (4, 0, 90, 3): four
    /// lasers, each 45 deg off the line in an X by roll -45 + 90 k and yaw -45;
    /// 222 u/s; 3 s.
    #[test]
    fn four_lasers_leave_in_an_x() {
        let d = ricochet();
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target: None, index: 0, col: None, marks: &[] };
        let shots = Ricochet::from_def(&d).launch(&d, &l);
        assert_eq!(shots.len(), 4);
        assert_eq!(shots[0].life, 90);
        let dirs: Vec<Vec3> = shots.iter().map(|s| s.heading()).collect();
        for h in &dirs {
            assert!((h.angle_between(Vec3::Z) - 45.0f32.to_radians()).abs() < 0.01, "45 deg off the line: {h:?}");
        }
        // The X: two above the line of sight, two below, two each side.
        assert_eq!(dirs.iter().filter(|h| h.y > 0.1).count(), 2, "{dirs:?}");
        assert_eq!(dirs.iter().filter(|h| h.y < -0.1).count(), 2);
        assert_eq!(dirs.iter().filter(|h| h.x > 0.1).count(), 2);
        let mut p = shots[0].pos;
        let mut f = match shots[0].flight { Flight::Ricochet(f) => f, _ => panic!() };
        f.step(&mut p, 0);
        assert!(((p - shots[0].pos).length() - 80.0 / 10.8).abs() < 1e-3);
    }

    /// Off a wall it reflects about the normal; the third bounce (counter 0)
    /// turns it straight at a target on the open side instead.
    #[test]
    fn bounces_reflect_and_the_last_aims_at_the_target() {
        let mut f = Flying { frame: Frame::look(Vec3::new(1.0, 0.0, 1.0)), speed: 7.0, end_speed: 7.0, ramp: 0, bounces: 3 };
        let n = Vec3::new(-1.0, 0.0, 0.0);
        assert!(f.bounce(n, Vec3::ZERO, None));
        let h = f.frame.fwd;
        assert!(h.x < -0.6 && h.z > 0.6, "mirrored in x: {h:?}");
        assert!(!f.bounce(n, Vec3::ZERO, None), "moving away from the face: ignored");
        let mut f = Flying { bounces: 1, frame: Frame::look(Vec3::new(1.0, 0.0, 0.2)), ..f };
        assert!(f.bounce(n, Vec3::ZERO, Some(Vec3::new(-50.0, 0.0, 50.0))));
        assert!(f.frame.fwd.distance(Vec3::new(-1.0, 0.0, 1.0).normalize()) < 1e-3, "straight at the target: {:?}", f.frame.fwd);
    }
}
