//! [Arc] attacks (motion 6: Laser, Slide Laser, Twist Laser, Sonic Boom,
//! Double / Triple / Octolaser). Guide: "an arclike trajectory. Because the
//! attack doesn't have a straight trajectory and bends, it can be used ...
//! where the target is not in a straight line with you ... if you don't know
//! the trajectory to your opponent, it is most difficult to use."
//!
//! Read from the program (`FUN_1402666b0` init, `FUN_140266bf0` update,
//! `FUN_1401950c0` the turn):
//! - the shot leaves along the caster's facing, rolled `tilt` about its own
//!   forward axis and then turned `bend` sideways *in that tilted frame*
//!   (Laser 20 / 20: the arc's plane is tilted, so it also rises or dips);
//! - the circle: R = distance to the aim point / (2 sin bend); it turns back
//!   by speed / R a tick for 2 x bend / that ticks, which brings it onto the
//!   aim point (the target's body centre pulled into the range band: the
//!   Optical lasers, s44 = 1, cannot be aimed closer than 300 u); then it
//!   flies straight for 30 more ticks (lifetime = curve ticks + 30);
//! - while it curves it re-aims every tick at the target's *current*
//!   position: the heading is turned toward the tangent of the circle through
//!   the target by at most s66 x 0.1 deg x homing % (24 deg/s for Laser);
//!   past a Course field it turns away at 5 deg a tick;
//! - fans (Double / Triple / Octolaser): shot k of the cast has tilt
//!   s68 + k x s71 and bend s67 + k x s70 (k = shot index mod s69). The number
//!   of shots is the count of `SHOTnn` markers in the cast effect
//!   (`FUN_14018a310`), not a table field: until the effect files give them,
//!   s69 stands in (Double 2, Triple 3, Octolaser 8 agree with the notes).
//!
//! (The first reading flew a flat circle in the ground plane with no homing, a
//! 300 u straight tail and a symmetric yaw fan.)

use super::{Flight, Frame, Launch, Shot, DEG, SPEED_UNIT, TURN_UNIT};
use crate::combat::SkillDef;
use glam::Vec3;

/// The straight flight after the curve, program ticks.
pub const TAIL_TICKS: u16 = 30;
/// A [Course] field turns an arc away at this much a tick.
pub const AWAY_TURN: f32 = 5.0 * DEG;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    /// Units per program tick (s65 / 10.8).
    pub speed: f32,
    /// Bend and tilt of shot 0, radians (s67, s68).
    pub bend: f32,
    pub tilt: f32,
    /// Shots in the pattern (s69; 0 / 1 = a single shot).
    pub pattern: i16,
    /// Per-shot steps of bend and tilt, radians (s70, s71).
    pub bend_step: f32,
    pub tilt_step: f32,
    /// Re-aim limit per program tick at 100 % homing (s66 x 0.1 deg).
    pub turn: f32,
}

impl Arc {
    pub fn from_def(def: &SkillDef) -> Self {
        Arc {
            speed: def.param(1).max(1) as f32 * SPEED_UNIT,
            bend: def.param(3) as f32 * DEG,
            tilt: def.param(4) as f32 * DEG,
            pattern: def.param(5),
            bend_step: def.s70_73[0] as f32 * DEG,
            tilt_step: def.s70_73[1] as f32 * DEG,
            turn: def.param(2).max(0) as f32 * TURN_UNIT,
        }
    }

    /// Shots a cast makes: the `SHOTnn` marker count in the program, s69 here.
    pub fn count(&self) -> u32 {
        self.pattern.max(1) as u32
    }

    /// `(bend, tilt)` of shot `index` (`FUN_1402666b0`).
    pub fn angles(&self, index: u32) -> (f32, f32) {
        let k = if self.pattern > 1 { (index % self.pattern as u32) as f32 } else { 0.0 };
        (self.bend + self.bend_step * k, self.tilt + self.tilt_step * k)
    }

    pub fn launch(&self, def: &SkillDef, l: &Launch) -> Vec<Shot> {
        let (aim, kept) = l.aim(def, false);
        (0..self.count())
            .map(|m| {
                let (bend, tilt) = self.angles(l.index + m);
                let spawn = l.spawn_of(m);
                let dist = (aim - spawn).length();
                let sin = bend.sin();
                let (r, rate, curve) = if sin.abs() < 1e-4 {
                    (f32::INFINITY, 0.0, 0)
                } else {
                    let r = dist / (sin * 2.0);
                    let rate = -(self.speed / r);
                    (r, rate, ((bend / rate).abs() as u16) * 2)
                };
                let frame = Frame::fire(l.facing, l.pitch).roll_program(tilt).yaw_program(bend);
                let f = Flying { frame, speed: self.speed, rate, r, curve, tilt, bend, turn: self.turn };
                let mut s = Shot::new(def, spawn, Flight::Arc(f), curve + TAIL_TICKS);
                s.homing = if kept { 1.0 } else { 0.0 };
                s
            })
            .collect()
    }
}

/// An arc in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flying {
    pub frame: Frame,
    pub speed: f32,
    /// Radians a tick it turns back by (negative of speed / R; signed by the bend).
    pub rate: f32,
    /// The circle's radius (signed with the bend).
    pub r: f32,
    /// Program ticks of curve left.
    pub curve: u16,
    pub tilt: f32,
    pub bend: f32,
    pub turn: f32,
}

impl Flying {
    /// `FUN_140266bf0`: while it curves, turn by `rate` in its own frame, then
    /// re-aim at the target's current position by at most `turn` x `homing`;
    /// always fly along the heading.
    pub fn step(&mut self, pos: &mut Vec3, target: Option<Vec3>, homing: f32, away: bool) {
        if self.curve > 0 {
            self.curve -= 1;
            self.frame = self.frame.yaw_program(self.rate);
            if let Some(t) = target {
                let v = t - *pos;
                let d = v.length();
                let x = self.r * self.r - d * d * 0.25;
                if d > 0.0 && x > 0.0 {
                    let mut a = (d * 0.5).atan2(x.sqrt());
                    if self.bend <= 0.0 {
                        a = -a;
                    }
                    let want = Frame::look(v / d).roll_program(self.tilt).yaw_program(a).fwd;
                    let (want, max) = if away { (-want, AWAY_TURN) } else { (want, self.turn * homing) };
                    self.frame = self.frame.turn_toward(want, max);
                }
            }
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

    /// `p` = s64..s69, `h` = s70..s71.
    fn arc(p: [i16; 6], h: [i16; 2], band: i16) -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (65, Capsule::Attack);
        d.raw[field::MOTION] = 6;
        d.raw[field::RANGE_BAND] = band;
        for (k, v) in p.into_iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = v;
        }
        d.s70_73 = [h[0], h[1], 0, 0];
        d
    }

    const LASER: [i16; 6] = [-80, 140, 8, 20, 20, 0];

    fn launch(d: &SkillDef, target: Option<Vec3>) -> Vec<Shot> {
        let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target, index: 0, col: None, marks: &[] };
        Arc::from_def(d).launch(d, &l)
    }

    /// Laser -80 / 140 / 8 / 20 / 20 / 0 (`FUN_1402666b0`): 389 u/s, leaves
    /// 20 deg off, tilted 20 deg about its own axis, and lives its curve plus
    /// 30 ticks.
    #[test]
    fn laser_leaves_tilted_and_bent_and_lives_curve_plus_thirty() {
        let d = arc(LASER, [0, 0], 1);
        let target = Vec3::new(0.0, 9.0, 400.0);
        let s = launch(&d, Some(target)).remove(0);
        let Flight::Arc(f) = s.flight else { panic!() };
        let h = f.frame.fwd;
        assert!((h.angle_between(Vec3::Z) - 20.0f32.to_radians()).abs() < 0.01, "20 deg off the line");
        assert!(h.y > 0.05, "the tilted plane lifts it: {h:?}");
        let dist = (target - Vec3::new(0.0, 12.0, 0.0)).length();
        let r = dist / (2.0 * 20.0f32.to_radians().sin());
        assert!((f.r - r).abs() < 0.5);
        let turn = (140.0 / 10.8) / r;
        assert_eq!(f.curve, ((20.0f32.to_radians() / turn) as u16) * 2);
        assert_eq!(s.life, f.curve + 30);
    }

    /// `FUN_140290050` turns a positive bend toward the object's physical right (the program's row 0 in its
    /// left-handed space). Facing +z in our right-handed, z-mirrored space that is -x: a Laser (bend +20)
    /// leaves to the caster's right and a Double Laser's first shot (bend -25) to its left. LIVE
    /// `309_Piece_of_Wind` (bend +15): the original's shot leaves to the right of the target on screen.
    #[test]
    fn a_positive_bend_leaves_to_the_casters_right() {
        let target = Some(Vec3::new(0.0, 9.0, 400.0));
        let Flight::Arc(f) = launch(&arc(LASER, [0, 0], 1), target).remove(0).flight else { panic!() };
        assert!(f.frame.fwd.x < -0.2, "right of a caster facing +z is -x: {:?}", f.frame.fwd);
        let Flight::Arc(g) = launch(&arc([-80, 160, 8, -25, -15, 2], [0, -150], 1), target).remove(0).flight else { panic!() };
        assert!(g.frame.fwd.x > 0.2, "a negative bend leaves to the left: {:?}", g.frame.fwd);
    }

    /// The fire matrix is tipped by the press's pitch (`FUN_140258180` -> `+0x12f8c`, `FUN_1402514c0`): a
    /// target point above the caster's centre (negative pitch) lifts the arc's plane, one below lowers it, and
    /// the bend and tilt stay those of the tipped frame.
    #[test]
    fn the_press_pitch_tips_the_arc() {
        let d = arc(LASER, [0, 0], 1);
        let target = Some(Vec3::new(0.0, 9.0, 400.0));
        let at = |pitch: f32| {
            let l = Launch { spawn: Vec3::new(0.0, 12.0, 0.0), feet: Vec3::ZERO, pitch, facing: Vec3::Z, target, index: 0, col: None, marks: &[] };
            let Flight::Arc(f) = Arc::from_def(&d).launch(&d, &l).remove(0).flight else { panic!() };
            f.frame.fwd
        };
        let (flat, up, down) = (at(0.0), at(-3.0 * DEG), at(3.0 * DEG));
        assert!(up.y > flat.y + 0.03 && down.y < flat.y - 0.03, "{up:?} {flat:?} {down:?}");
        // The fire matrix itself: tipped about its right row only.
        let f = Frame::fire(Vec3::Z, -3.0 * DEG);
        assert!((f.fwd.angle_between(Vec3::Z) - 3.0 * DEG).abs() < 1e-4 && f.fwd.y > 0.0, "{f:?}");
        assert_eq!(f.right, Frame::look(Vec3::Z).right);
    }

    /// Shot `m` of a fan starts at marker `(index + m) mod count` (`FUN_1402626d0`:
    /// `local_c8[(counter + i) % count]`); with no markers every shot starts at `spawn`.
    #[test]
    fn each_fan_shot_starts_at_its_own_marker() {
        let d = arc([-80, 140, 8, 20, 20, 3], [0, -80], 1);
        let marks = [Vec3::new(13.0, 5.0, 10.0), Vec3::new(0.0, 27.0, 10.0), Vec3::new(-13.0, 5.0, 10.0)];
        let target = Some(Vec3::new(0.0, 9.0, 400.0));
        let l = Launch { spawn: marks[0], feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target, index: 0, col: None, marks: &marks };
        let shots = Arc::from_def(&d).launch(&d, &l);
        assert_eq!(shots.iter().map(|s| s.pos).collect::<Vec<_>>(), marks.to_vec());
        // The caster's running counter shifts which marker each shot takes.
        let l = Launch { index: 1, ..l };
        let shots = Arc::from_def(&d).launch(&d, &l);
        assert_eq!(shots.iter().map(|s| s.pos).collect::<Vec<_>>(), vec![marks[1], marks[2], marks[0]]);
        let none = Launch { marks: &[], spawn: Vec3::new(0.0, 12.0, 0.0), index: 0, ..l };
        assert!(Arc::from_def(&d).launch(&d, &none).iter().all(|s| s.pos == Vec3::new(0.0, 12.0, 0.0)));
    }

    /// Flown with the target still, it comes back to the target: the circle
    /// closes on the aim point.
    #[test]
    fn laser_passes_through_a_still_target() {
        let d = arc(LASER, [0, 0], 1);
        let target = Vec3::new(0.0, 12.0, 400.0);
        let col = Collision::new(vec![]);
        let mut s = launch(&d, Some(target)).remove(0);
        let mut closest = f32::MAX;
        for _ in 0..80 {
            s.step(Some(target), &col);
            closest = closest.min(s.pos.distance(target));
        }
        assert!(closest < 12.0, "closest approach {closest} u");
    }

    /// An Optical laser cannot be aimed closer than 300 u (s44 = 1): at a
    /// target 150 u away its circle closes at 300 u and passes to the side
    /// (about 26 u at 150 u); with the target dropped it does not re-aim.
    #[test]
    fn the_range_band_pulls_a_close_target_out_to_300_u() {
        let d = arc(LASER, [0, 0], 1);
        let near = Vec3::new(0.0, 12.0, 150.0);
        let s = launch(&d, Some(near)).remove(0);
        assert_eq!(s.homing, 0.0, "the target is dropped");
        let Flight::Arc(f) = s.flight else { panic!() };
        let r300 = 300.0 / (2.0 * 20.0f32.to_radians().sin());
        assert!((f.r - r300).abs() < 4.0, "circle for a 300 u chord: {} vs {r300}", f.r);
        // A Slide Laser-sized band-0 skill is left alone at 150 u.
        let d0 = arc(LASER, [0, 0], 0);
        let s0 = launch(&d0, Some(near)).remove(0);
        assert_eq!(s0.homing, 1.0);
    }

    /// Octolaser-style fans: shot k is tilted s68 + k x s71 and bent s67 + k x
    /// s70 (here -25 bend, tilt 22 - 32 k).
    #[test]
    fn fan_angles_step_per_shot() {
        let d = arc([-80, 160, 8, -25, 22, 8], [0, -32], 1);
        let a = Arc::from_def(&d);
        assert_eq!(a.count(), 8);
        let (b0, t0) = a.angles(0);
        let (b7, t7) = a.angles(7);
        assert!((b0 + 25.0 * DEG).abs() < 1e-5 && (b7 + 25.0 * DEG).abs() < 1e-5);
        assert!((t0 - 22.0 * DEG).abs() < 1e-5 && (t7 - (22.0 - 224.0) * DEG).abs() < 1e-4);
        let shots = launch(&d, Some(Vec3::new(0.0, 12.0, 400.0)));
        assert_eq!(shots.len(), 8);
        let leaves: Vec<Vec3> = shots.iter().map(|s| s.heading()).collect();
        assert!(leaves[0].distance(leaves[4]) > 0.1, "different planes leave different ways");
    }
}
