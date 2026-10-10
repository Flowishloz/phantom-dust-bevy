//! [Parabola] attacks (motion 1: Bomb, Throwing Stone, Rock Shot, Trench
//! Mortar, Gungnir). Guide: "The trajectory of these skills follows a
//! parabola, and their attacks are especially effective when a wall or height
//! differential exists between the attacker and the target. They may appear to
//! lack in homing capability, but most are quite effective."
//!
//! Read from the program (`FUN_140264b00` init, `FUN_140265080` update,
//! `FUN_14026bf50` the steering predictor):
//! - the lob is aimed at the target's body centre through the range band
//!   (80-300 u; the mortars s44 = 1: 300-600 u);
//! - horizontal speed = s64 / 10.8 per program tick (s65 when s64 = 0); the
//!   flight takes T = distance / speed ticks;
//! - s66 is the arc's height: s66 x 10 u above the straight line at mid-flight
//!   (gravity g = 8h / T^2, launch rise = g T / 2 + height difference / T);
//! - s67 is the steering: each tick the lob predicts where it will come down
//!   at the target's height and slides sideways toward the target by at most
//!   s67 / 10.8 x homing % u; once it is falling past the target's height it
//!   keeps its last correction;
//! - it lives T + 30 ticks, ends on the floor or a wall; Bombs blast (s51 = 5).
//!
//! (Our first reading had s66 as the steering and s67 unknown, a flight time
//! clamped to 0.5-1.6 s and a fixed gravity of 150.)

use super::{Flight, Launch, Shot, SPEED_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;

/// A [Course] field slides a lob this far away from its target each tick
/// (`0x1402f7f2c` = 5.5556, negated).
pub const AWAY_STEP: f32 = 5.555_555;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parabola {
    /// Horizontal units per program tick (s64, or s65 when s64 = 0).
    pub speed: f32,
    /// Arc height in units (s66 x 10).
    pub height: f32,
    /// Sideways slide per program tick at 100 % homing (s67 / 10.8).
    pub steer: f32,
}

impl Parabola {
    pub fn from_def(def: &SkillDef) -> Self {
        let table = if def.param(0) == 0 { def.param(1) } else { def.param(0) };
        Parabola { speed: table.max(1) as f32 * SPEED_UNIT, height: def.param(2) as f32 * 10.0, steer: def.param(3).max(0) as f32 * SPEED_UNIT }
    }

    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let (aim, kept) = l.aim(def, false);
        let mut f = Flying { dir: l.facing, speed: self.speed, vy: 0.0, g: 0.0, steer: self.steer, last: Vec3::ZERO, height: self.height };
        let t = f.aim_at(l.spawn, aim);
        let mut shot = Shot::new(def, l.spawn, Flight::Lob(f), t.floor() as u16 + 30);
        shot.homing = if kept { 1.0 } else { 0.0 };
        vec![shot]
    }
}

/// A lob in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    /// Horizontal heading (the caster's facing), unit.
    pub dir: Vec3,
    pub speed: f32,
    /// Vertical speed this tick (up positive); falls by `g` each tick.
    pub vy: f32,
    pub g: f32,
    pub steer: f32,
    /// The last sideways correction, kept once the lob falls past the target.
    pub last: Vec3,
    pub height: f32,
}

impl Flying {
    /// Sets the launch rise and gravity for a lob from `from` onto `aim`
    /// (`FUN_140264b00`); returns the flight time in program ticks.
    pub fn aim_at(&mut self, from: Vec3, aim: Vec3) -> f32 {
        let flat = Vec3::new(aim.x - from.x, 0.0, aim.z - from.z).length();
        let t = (flat / self.speed).max(1.0);
        self.g = 8.0 * self.height / (t * t);
        self.vy = self.g * 0.5 * t + (aim.y - from.y) / t;
        t
    }

    /// Re-aim at `to` from `here` (a Reverse field sent it back).
    pub fn reaim(&mut self, here: Vec3, to: Vec3) {
        self.dir = Vec3::new(to.x - here.x, 0.0, to.z - here.z).normalize_or(self.dir);
        self.aim_at(here, to);
        self.last = Vec3::ZERO;
    }

    /// `FUN_14026bf50`: the time (program ticks) until the lob is back down
    /// at height `target_y` and the sideways gap that leaves to the target:
    /// `(0, last correction)` when it is already past the target's height.
    fn predict(&self, pos: Vec3, target: Vec3) -> (f32, Vec3) {
        if self.g <= 1e-6 {
            return (0.0, self.last);
        }
        let t_apex = self.vy / self.g;
        let peak = (self.g * t_apex * t_apex * 0.5).abs() + pos.y;
        let fall = peak - target.y;
        let rem = t_apex + ((fall + fall) / self.g).sqrt();
        if peak < target.y || (pos.y < target.y && self.vy < 0.0) || rem.is_nan() || rem <= 0.0 {
            return (0.0, self.last);
        }
        let come_down = pos + self.dir * (rem * self.speed);
        (rem, Vec3::new(target.x - come_down.x, 0.0, target.z - come_down.z))
    }

    /// `FUN_140265080`: slide toward (or, past a Course field, away from) the
    /// target by at most `steer` x `homing`, then fly on.
    pub fn step(&mut self, pos: &mut Vec3, target: Option<Vec3>, homing: f32, away: bool) {
        if let Some(t) = target {
            let (_, out) = self.predict(*pos, t);
            let len = out.length();
            let c = if away {
                if len <= 0.0 { Vec3::new(AWAY_STEP, 0.0, 0.0) } else { out / len * -AWAY_STEP }
            } else if self.steer * homing < len {
                out / len * self.steer * homing
            } else {
                out
            };
            *pos += c;
            self.last = c;
        }
        *pos += self.dir * self.speed + Vec3::Y * self.vy;
        self.vy -= self.g;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::collision::Collision;
    use pd_formats::skills::field;

    fn lob(p: [i16; 4], band: i16) -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (130, Capsule::Attack);
        d.raw[field::MOTION] = 1;
        d.raw[field::RANGE_BAND] = band;
        for (k, v) in p.into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d
    }

    fn launch(d: &SkillDef, target: Option<Vec3>) -> Shot {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target, index: 0, col: None, marks: &[] };
        Parabola::from_def(d).launch(d, &l).remove(0)
    }

    /// Bomb 90 / 0 / 4 / 10 (`FUN_140264b00`): 250 u/s (8.33 u a tick) over
    /// 160 u is T = 19.2 ticks, the arc peaks 40 u (s66 x 10) above the
    /// straight line at mid-flight, and the shot lives T + 30 ticks.
    #[test]
    fn bomb_arcs_40_u_and_comes_down_on_the_target() {
        let d = lob([90, 0, 4, 10], 0);
        let aim = Vec3::new(0.0, 9.0, 160.0);
        let mut s = launch(&d, Some(aim));
        assert_eq!(s.life, 19 + 30, "(int) T + 30");
        let col = Collision::new(vec![]);
        let straight = |z: f32| 12.0 + (9.0 - 12.0) * z / 160.0;
        let mut peak = f32::MIN;
        let mut landed = None;
        for _ in 0..40 {
            s.step(Some(aim), &col);
            peak = peak.max(s.pos.y - straight(s.pos.z));
            if landed.is_none() && s.pos.z >= 159.0 {
                landed = Some(s.pos);
            }
        }
        assert!((peak - 40.0).abs() < 6.0, "peak {peak} u over the straight line (a tick is a coarse sample)");
        let at = landed.expect("reached the target's range");
        assert!((at.y - 9.0).abs() < 12.0, "at the target's height when it gets there: {at:?}");
    }

    /// Throwing Stone peaks 10 u over the line, Delay Bomb 100 u: s66 is the
    /// height (the 150 u/s^2 we used peaked every lob at the same ~40 u).
    #[test]
    fn height_is_s66_x_10() {
        for (s66, want) in [(1, 10.0), (10, 100.0)] {
            let d = lob([140, 0, s66, 0], 0);
            let aim = Vec3::new(0.0, 12.0, 200.0);
            let mut s = launch(&d, Some(aim));
            let col = Collision::new(vec![]);
            let mut peak = 0.0f32;
            for _ in 0..40 {
                s.step(None, &col);
                peak = peak.max(s.pos.y - 12.0);
            }
            assert!((peak - want).abs() < want * 0.2 + 2.0, "s66 {s66}: peak {peak}, want about {want}");
        }
    }

    /// A target that is not where the lob was aimed (a step to the side while
    /// it flies) is followed by sliding sideways at s67 / 10.8 a tick at most.
    #[test]
    fn steering_slides_sideways_and_is_capped() {
        let d = lob([90, 0, 4, 10], 0);
        let mut s = launch(&d, Some(Vec3::new(0.0, 9.0, 160.0)));
        let col = Collision::new(vec![]);
        let moved = Vec3::new(40.0, 9.0, 160.0);
        let before = s.pos.x;
        s.step(Some(moved), &col);
        let slid = s.pos.x - before;
        assert!(slid > 0.0 && slid <= 10.0 / 10.8 + 1e-4, "slid {slid}, cap {}", 10.0 / 10.8);
        // Past a Course field: away, 5.56 u a tick.
        let mut s = launch(&d, Some(Vec3::new(0.0, 9.0, 160.0)));
        s.away = true;
        let before = s.pos.x;
        s.step(Some(Vec3::new(10.0, 9.0, 160.0)), &col);
        assert!(s.pos.x - before < -5.0, "slid away {}", s.pos.x - before);
    }

    /// The mortars (s44 = 1) cannot be aimed closer than 300 u: a target 150 u
    /// away gets a lob that lands 300 u out, with no steering (the target is dropped).
    #[test]
    fn mortars_are_pulled_out_to_300_u_and_drop_the_target() {
        let d = lob([140, 0, 1, 0], 1);
        let s = launch(&d, Some(Vec3::new(0.0, 9.0, 150.0)));
        assert_eq!(s.homing, 0.0, "a pulled aim point drops the target");
        let Flight::Lob(f) = s.flight else { panic!() };
        let t = (300.0f32 / (140.0f32 / 10.8)).ceil();
        assert!((s.life as f32 - 30.0 - t).abs() <= 1.5, "flight of about {t} ticks, life {}", s.life);
        assert!(f.vy > 0.0);
    }
}
