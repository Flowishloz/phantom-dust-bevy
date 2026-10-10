//! [Fall] attacks (motion 2: Meteor, Mini-Meteor, Thor's Hammer, Arrow of
//! Artemis). Guide: "drop from overhead onto the target ... most effective
//! when the target is at a higher elevation and is not within sight."
//!
//! Read from the program (`FUN_140265470` init, `FUN_1402657f0` update):
//! - it is **not** dropped straight down: the shot appears s64 x 10 u above
//!   and s65 x 10 u back toward the caster from the aim point (Meteor 60 up,
//!   180 back; Thor's Hammer 120 / 180; Arrow of Artemis 40 / 190) and flies
//!   a straight line at the aim point, the target's body centre pulled into
//!   the range band (s44 = 0: 80-300 u);
//! - speed s66 / 10.8 per program tick (Meteor 222 u/s, Demon's Venom 111);
//! - while the target is still ahead of it the shot slides sideways (across
//!   its flight direction) toward the target by at most s67 / 10.8 x homing %
//!   per tick, and keeps the last correction after that;
//! - it lives twice the flight distance / speed (it carries on into the
//!   ground); a roof stops it unless "Penetrates obstacles" (Thor's Hammer).

use super::{Flight, Frame, Launch, Shot, SPEED_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;

/// A [Course] field slides it this far away each tick (`0x1402f7f2c`).
pub const AWAY_STEP: f32 = 5.555_555;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fall {
    /// Units above the aim point it appears (s64 x 10).
    pub up: f32,
    /// Units back toward the caster (s65 x 10).
    pub back: f32,
    /// Units per program tick (s66 / 10.8).
    pub speed: f32,
    /// Sideways slide per tick at 100 % homing (s67 / 10.8).
    pub steer: f32,
}

impl Fall {
    pub fn from_def(def: &SkillDef) -> Self {
        Fall { up: def.param(0).max(0) as f32 * 10.0, back: def.param(1).max(0) as f32 * 10.0, speed: def.param(2).max(1) as f32 * SPEED_UNIT, steer: def.param(3).max(0) as f32 * SPEED_UNIT }
    }

    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let (aim, kept) = l.aim(def, false);
        // `FUN_14026e410` turns the matrix to the aim point's bearing from the
        // caster; the shot starts above it and back along that bearing.
        let bearing = Vec3::new(aim.x - l.spawn.x, 0.0, aim.z - l.spawn.z).normalize_or(l.facing);
        let start = aim + Vec3::Y * self.up - bearing * self.back;
        let dist = (aim - start).length();
        let f = Flying { frame: Frame::look(aim - start), speed: self.speed, steer: self.steer, last: (0.0, 0.0) };
        let mut shot = Shot::new(def, start, Flight::Fall(f), (2.0 * dist / self.speed) as u16);
        shot.homing = if kept { 1.0 } else { 0.0 };
        vec![shot]
    }
}

/// A fall in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    pub frame: Frame,
    pub speed: f32,
    pub steer: f32,
    /// The last correction in the shot's own frame: (right, up).
    pub last: (f32, f32),
}

impl Flying {
    /// `FUN_1402657f0`: with the target still ahead, work out the sideways gap
    /// in the shot's frame and close it by at most `steer` x `homing` (or
    /// open it by 5.56 past a Course field); then slide by the last
    /// correction and fly along the heading.
    pub fn step(&mut self, pos: &mut Vec3, target: Option<Vec3>, homing: f32, away: bool) {
        if let Some(t) = target {
            let d = t - *pos;
            if d.dot(self.frame.fwd) > 0.0 {
                let (x, y) = (d.dot(self.frame.right), d.dot(self.frame.up));
                let len = (x * x + y * y).sqrt();
                self.last = if away {
                    if len <= 0.0 { (AWAY_STEP, 0.0) } else { (x / len * -AWAY_STEP, y / len * -AWAY_STEP) }
                } else if self.steer * homing < len {
                    let k = self.steer * homing / len;
                    (x * k, y * k)
                } else {
                    (x, y)
                };
            }
            *pos += self.frame.right * self.last.0 + self.frame.up * self.last.1;
        }
        *pos += self.frame.fwd * self.speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::collision::Collision;
    use pd_formats::skills::field;

    fn meteor() -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (19, Capsule::Attack);
        d.raw[field::MOTION] = 2;
        for (k, v) in [6, 18, 80, 15].into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d
    }

    fn launch(d: &SkillDef, target: Vec3) -> Shot {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target: Some(target), index: 0, col: None, marks: &[] };
        Fall::from_def(d).launch(d, &l).remove(0)
    }

    /// Meteor 6 / 18 / 80 / 15 (`FUN_140265470`): it appears 60 u over and
    /// 180 u back from the aim point on the line from the caster, flies at
    /// 80 / 10.8 = 7.4 u a tick (222 u/s) and lives twice the way there.
    #[test]
    fn meteor_comes_in_diagonally_from_behind_and_above() {
        let d = meteor();
        let aim = Vec3::new(0.0, 9.0, 200.0);
        let s = launch(&d, aim);
        assert!((s.pos - Vec3::new(0.0, 69.0, 20.0)).length() < 1e-3, "starts at {:?}", s.pos);
        let way = (aim - s.pos).length();
        assert_eq!(s.life, (2.0 * way / (80.0 / 10.8)) as u16);
        let h = s.heading();
        assert!(h.z > 0.9 && h.y < -0.2, "forward and down: {h:?}");
    }

    /// It reaches the aim point on a still target, and slides sideways (at
    /// most 15 / 10.8 a tick) after one that stepped aside.
    #[test]
    fn meteor_lands_on_a_still_target_and_follows_a_moving_one() {
        let d = meteor();
        let col = Collision::new(vec![]);
        let aim = Vec3::new(0.0, 9.0, 200.0);
        let mut s = launch(&d, aim);
        let mut closest = f32::MAX;
        for _ in 0..60 {
            s.step(Some(aim), &col);
            closest = closest.min(s.pos.distance(aim));
        }
        assert!(closest < 4.0, "passes within {closest} u of the target");
        let mut s = launch(&d, aim);
        let moved = Vec3::new(30.0, 9.0, 200.0);
        let before = s.pos;
        s.step(Some(moved), &col);
        let slid = (s.pos - before - s.heading() * (80.0 / 10.8)).length();
        assert!(slid > 0.5 && slid <= 15.0 / 10.8 + 1e-3, "slid {slid}");
    }
}
