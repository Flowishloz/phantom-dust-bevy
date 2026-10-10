//! The CPU's threat predictor `FUN_14020f9a0` (script native g4.f186), a port of the executable model
//! `engine/re/threat_predict.py` (FORMATS-CPU.md section 4.4, read from the assembly). Pure arithmetic: the
//! predicted number of 30 Hz ticks until an attack object reaches player `me`, 0 = harmless.
//!
//! The object fields are the program's (`O+0x10 state`, `+0x14 skill`, `+0x18 age`, `+0x24 hit mask`,
//! `+0x40 attacker`, `+0x48 homing target`, `+0xf0 dir`, `+0x100 pos`, `+0x190 radius`, `+0x1c0..0x1d4`);
//! how our sim's projectiles fill them is `world.rs` (`obj_from_projectile`), which is the UNCONFIRMED part
//! (FORMATS-CPU 7 item 5): the formulas here are READ, the field mapping is ours and is listed there.

use glam::Vec3;

/// `FUN_140167910` plus the predictor's own `s63 == 4` test: motions the predictor ignores.
pub const NO_AIM_MOTIONS: [i32; 4] = [3, 12, 13, 14];
/// `0x1402f7fd8`.
pub const TICKS_PER_SECOND: f32 = 30.0;

/// The skill record fields the predictor reads (`DAT_1404bdc50 + 0x90 * id`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SkillFields {
    pub s43: i32,
    pub s63: i32,
    pub s55: i32,
    pub s52: i32,
    pub s64: i32,
    pub s65: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Obj {
    pub state: i32,
    pub skill: i32,
    pub age: i32,
    pub hitmask: u32,
    pub attacker: i32,
    pub target: Option<i32>,
    pub dir: Vec3,
    pub pos: Vec3,
    pub radius: f32,
    pub f1c0: f32,
    pub f1c4: f32,
    pub f1cc: f32,
    pub f1d0: f32,
    pub i1cc: i32,
    pub i1d4: i32,
    pub yaw: i32,
}

/// `cvttss2si`: truncate toward zero; NaN / out of range gives `i32::MIN` like the CPU.
pub fn trunc(x: f32) -> i32 {
    if x.is_nan() || x.abs() >= 2_147_483_648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

fn closest_point_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 == 0.0 {
        return a;
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    a + ab * t
}

/// `FUN_14020f8c0`: an object flying straight at `pos`. `speed` is per SECOND (divided by 30).
pub fn straight_shot(pos: Vec3, obj_pos: Vec3, margin: f32, radius: f32, speed: f32, ray_clear: &dyn Fn(Vec3, Vec3) -> bool) -> i32 {
    if !ray_clear(pos, obj_pos) {
        return 0;
    }
    let d = pos.distance(obj_pos) - margin;
    if speed < d {
        return 0;
    }
    trunc((d - radius) / (speed / TICKS_PER_SECOND)).max(1)
}

fn free_segment(obj: &Obj, pos: Vec3, m2: f32, v: f32) -> i32 {
    let p0 = obj.pos;
    let e = p0 + obj.dir * v * obj.age as f32;
    let c = closest_point_on_segment(pos, p0, e);
    if m2 <= pos.distance(c) {
        return 0;
    }
    trunc(p0.distance(c) / v)
}

/// Ticks until `obj` reaches player `me` standing at `pos` (`margin` = my body radius, `P+0x12e9c`).
pub fn predict(obj: Option<&Obj>, me: i32, pos: Vec3, margin: f32, ground_ok: bool, skill_of: &dyn Fn(i32) -> Option<SkillFields>, ray_clear: &dyn Fn(Vec3, Vec3) -> bool) -> i32 {
    let Some(obj) = obj else { return 0 };
    if obj.state == 0 || (obj.hitmask >> (me as u32 & 31)) & 1 == 0 {
        return 0;
    }
    let Some(sk) = skill_of(obj.skill) else { return 0 };
    if sk.s43 != 0 || NO_AIM_MOTIONS.contains(&sk.s63) || sk.s63 == 4 {
        return 0;
    }
    if obj.state == 3 {
        return straight_shot(pos, obj.pos, margin, obj.radius, obj.f1c0, ray_clear);
    }
    let tgt = obj.target;
    let m2 = margin + obj.radius;
    match sk.s63 {
        0 | 11 => {
            let v = if sk.s63 == 0 { obj.f1c0 } else { obj.f1d0 };
            match tgt {
                Some(t) => {
                    if t != me {
                        return 0;
                    }
                    if !matches!(sk.s55, 2 | 3) && !ray_clear(pos, obj.pos) {
                        return 0;
                    }
                    let d = pos - obj.pos;
                    let dist = d.length();
                    if dist == 0.0 {
                        return 0;
                    }
                    if (d / dist).dot(obj.dir) <= 0.5 {
                        return 0;
                    }
                    let t = trunc((dist - m2) / v);
                    if t > obj.age {
                        0
                    } else {
                        t
                    }
                }
                None => free_segment(obj, pos, m2, v),
            }
        }
        1 => {
            if let Some(t) = tgt {
                return if t == me { obj.i1d4 } else { 0 };
            }
            let (a, vy, y0, py) = (obj.f1cc, obj.f1c4, obj.pos.y, pos.y);
            let t_apex = vy / a;
            let apex = (a * t_apex * t_apex * 0.5).abs() + y0;
            if py > apex {
                return 0;
            }
            let fall = ((apex - py) * 2.0 / a).sqrt();
            let t_total = t_apex + fall;
            if (py > y0 && vy < 0.0) || t_total <= 0.0 {
                return 0;
            }
            let yaw = (obj.yaw & 0x7FFF) as f32 * std::f32::consts::TAU / 32768.0;
            let dd = t_total * obj.f1c0;
            let e = Vec3::new(obj.pos.x + yaw.sin() * dd, obj.pos.y, obj.pos.z + yaw.cos() * dd);
            let dist2d = (pos.x - e.x).hypot(pos.z - e.z);
            let hit_radius = sk.s52 as i16 as f32 * 0.1;
            if hit_radius <= dist2d {
                return 0;
            }
            trunc(t_total)
        }
        2 => match tgt {
            None => free_segment(obj, pos, m2, obj.f1c0),
            Some(t) => {
                if t != me {
                    return 0;
                }
                if !matches!(sk.s55, 2 | 3) && !ray_clear(pos, obj.pos) {
                    return 0;
                }
                if 0.0 <= pos.y - obj.pos.y {
                    return 0;
                }
                trunc((pos.distance(obj.pos) - m2) / obj.f1c0)
            }
        },
        5 => {
            if !ground_ok {
                return 0;
            }
            match tgt {
                None => free_segment(obj, pos, m2, obj.f1c4),
                Some(t) => {
                    if t != me {
                        return 0;
                    }
                    let d2 = (pos.x - obj.pos.x).hypot(pos.z - obj.pos.z);
                    trunc((d2 - m2) / obj.f1c4)
                }
            }
        }
        6 => match tgt {
            Some(t) if t == me => obj.i1cc,
            _ => 0,
        },
        7 | 15 | 16 => {
            if obj.attacker == me {
                0
            } else {
                obj.age - sk.s65 as i16 as i32
            }
        }
        9 => straight_shot(pos, obj.pos, margin, obj.radius, sk.s64 as i16 as f32 * 0.1, ray_clear),
        10 => {
            if !ground_ok {
                return 0;
            }
            let p0 = Vec3::new(obj.pos.x, 0.0, obj.pos.z);
            let w = obj.age as f32 * obj.f1c4;
            let e = Vec3::new(p0.x + obj.dir.x * w, obj.dir.y * w, p0.z + obj.dir.z * w);
            let flat = Vec3::new(pos.x, 0.0, pos.z);
            let c = closest_point_on_segment(flat, p0, e);
            if m2 <= flat.distance(c) {
                return 0;
            }
            let d2 = (p0.x - flat.x).hypot(p0.z - flat.z);
            trunc((d2 - m2) / obj.f1c4)
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sk(s63: i32) -> impl Fn(i32) -> Option<SkillFields> {
        move |_| Some(SkillFields { s43: 0, s63, s55: 0, s52: 100, s64: 600, s65: 4 })
    }
    fn clear(_: Vec3, _: Vec3) -> bool {
        true
    }
    fn blocked(_: Vec3, _: Vec3) -> bool {
        false
    }
    fn shell(pos: Vec3, dir: Vec3, v: f32, tgt: Option<i32>, age: i32) -> Obj {
        Obj { state: 1, skill: 1, age, hitmask: 0b11, attacker: 1, target: tgt, dir, pos, radius: 5.0, f1c0: v, f1c4: v, f1cc: 0.0, f1d0: v, ..Obj::default() }
    }

    /// `FUN_14020f9a0` s63 0, homing target = me (the shell's `T` set): `trunc((|pos - O.pos| - m2) / v)`, needs `n . dir > 0.5` and `t <= age`.
    #[test]
    fn homing_shell_ticks_to_impact_and_its_gates() {
        let me = Vec3::new(0.0, 9.0, 0.0);
        let o = shell(Vec3::new(0.0, 9.0, 100.0), Vec3::new(0.0, 0.0, -1.0), 5.0, Some(0), 40);
        // distance 100, m2 = 4.5 + 5 -> (100 - 9.5) / 5 = 18
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(0), &clear), 18);
        // Flying away (n . dir < 0.5): harmless.
        let away = shell(Vec3::new(0.0, 9.0, 100.0), Vec3::new(0.0, 0.0, 1.0), 5.0, Some(0), 40);
        assert_eq!(predict(Some(&away), 0, me, 4.5, true, &sk(0), &clear), 0);
        // Homing on someone else: harmless. The stage ray blocked: harmless (s55 not 2 / 3).
        let other = shell(o.pos, o.dir, 5.0, Some(1), 40);
        assert_eq!(predict(Some(&other), 0, me, 4.5, true, &sk(0), &clear), 0);
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(0), &blocked), 0);
        // t > age: not yet (the age gate).
        let young = shell(o.pos, o.dir, 5.0, Some(0), 10);
        assert_eq!(predict(Some(&young), 0, me, 4.5, true, &sk(0), &clear), 0);
        // The hit mask bit of `me` is required (`O+0x24 >> me & 1`).
        let masked = Obj { hitmask: 0b10, ..o };
        assert_eq!(predict(Some(&masked), 0, me, 4.5, true, &sk(0), &clear), 0);
    }

    /// s63 0 with `T` null: free flight, the closest point of the segment `O.pos .. O.pos + dir * v * age` to me.
    #[test]
    fn free_flight_shell_uses_the_segment_closest_point() {
        let me = Vec3::new(0.0, 9.0, 0.0);
        // It started 100 away flying at me, 20 ticks ago at 5 u/tick: its segment reaches me; ticks = |O.pos - C| / v = 100 / 5.
        let o = shell(Vec3::new(0.0, 9.0, 100.0), Vec3::new(0.0, 0.0, -1.0), 5.0, None, 20);
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(0), &clear), 20);
        // Passing 30 to the side: the closest approach is beyond m2 -> 0.
        let wide = shell(Vec3::new(30.0, 9.0, 100.0), Vec3::new(0.0, 0.0, -1.0), 5.0, None, 20);
        assert_eq!(predict(Some(&wide), 0, me, 4.5, true, &sk(0), &clear), 0);
    }

    /// Skills that are not attacks (s43 != 0), the "short range" motions 3 / 12 / 13 / 14 and motion 4 are never threats.
    #[test]
    fn non_threat_skills_are_ignored() {
        let me = Vec3::new(0.0, 9.0, 0.0);
        let o = shell(Vec3::new(0.0, 9.0, 100.0), Vec3::new(0.0, 0.0, -1.0), 5.0, Some(0), 40);
        for m in [3, 4, 12, 13, 14, 8] {
            assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(m), &clear), 0, "s63 {m}");
        }
        let defence = |_: i32| Some(SkillFields { s43: 2, ..SkillFields::default() });
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &defence, &clear), 0);
        assert_eq!(predict(None, 0, me, 4.5, true, &sk(0), &clear), 0);
        assert_eq!(predict(Some(&Obj { state: 0, ..o }), 0, me, 4.5, true, &sk(0), &clear), 0);
    }

    /// State 3 objects and s63 9: `FUN_14020f8c0` straight shot, `max(1, trunc((d - radius) / (speed / 30)))`, 0 when the speed per second cannot cover `d`.
    #[test]
    fn straight_shot_rule() {
        let me = Vec3::new(0.0, 9.0, 0.0);
        let p = Vec3::new(0.0, 9.0, 100.0);
        // d = 100 - 4.5 = 95.5; speed 300 / s = 10 per tick: (95.5 - 5) / 10 = 9.05 -> 9
        assert_eq!(straight_shot(me, p, 4.5, 5.0, 300.0, &clear), 9);
        assert_eq!(straight_shot(me, p, 4.5, 5.0, 90.0, &clear), 0, "cannot cover the gap in one second");
        assert_eq!(straight_shot(me, p, 4.5, 5.0, 300.0, &blocked), 0);
        assert_eq!(straight_shot(me, Vec3::new(0.0, 9.0, 9.0), 4.5, 50.0, 300.0, &clear), 1, "never below 1");
        let o = Obj { state: 3, f1c0: 300.0, ..shell(p, Vec3::NEG_Z, 5.0, None, 1) };
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(0), &clear), 9);
    }

    /// s63 1 (lob) with `T` null: apex, fall time, landing point, `s52 * 0.1` hit radius.
    #[test]
    fn lob_free_flight_lands_on_me_or_not() {
        let me = Vec3::new(0.0, 0.0, 0.0);
        // vy 3 up, g 0.12: t_apex 25, apex 37.5 above y0 = 0; fall to y 0 takes 25 -> 50 ticks, horizontal 1.0 u/tick along yaw 0 (+z)
        let o = Obj { state: 1, skill: 10, hitmask: 1, pos: Vec3::new(0.0, 0.0, -50.0), f1c0: 1.0, f1c4: 3.0, f1cc: 0.12, yaw: 0, ..Obj::default() };
        // landing z = -50 + 50 = 0 -> within s52 * 0.1 = 10: 50 ticks
        assert_eq!(predict(Some(&o), 0, me, 4.5, true, &sk(1), &clear), 50);
        let far = Obj { pos: Vec3::new(0.0, 0.0, -150.0), ..o };
        assert_eq!(predict(Some(&far), 0, me, 4.5, true, &sk(1), &clear), 0);
    }

    /// s63 2 (fall) with `T = me`: needs me below the object; s63 5 needs the ground line; s63 7 / 15 / 16: `age - s65`.
    #[test]
    fn fall_crawl_and_delayed_kinds() {
        let me = Vec3::new(0.0, 0.0, 0.0);
        let f = Obj { state: 1, skill: 3, hitmask: 1, target: Some(0), pos: Vec3::new(0.0, 100.0, 0.0), f1c0: 5.0, radius: 5.0, ..Obj::default() };
        // (100 - 9.5) / 5 = 18.1 -> 18
        assert_eq!(predict(Some(&f), 0, me, 4.5, true, &sk(2), &clear), 18);
        let above = Obj { pos: Vec3::new(0.0, -100.0, 0.0), ..f };
        assert_eq!(predict(Some(&above), 0, me, 4.5, true, &sk(2), &clear), 0, "I must be below it");
        let c = Obj { f1c4: 4.0, pos: Vec3::new(0.0, 0.0, 60.0), ..f };
        assert_eq!(predict(Some(&c), 0, me, 4.5, true, &sk(5), &clear), 12);
        assert_eq!(predict(Some(&c), 0, me, 4.5, false, &sk(5), &clear), 0, "ground line needed");
        let d = Obj { age: 9, attacker: 1, ..f };
        assert_eq!(predict(Some(&d), 0, me, 4.5, true, &sk(15), &clear), 5);
        assert_eq!(predict(Some(&Obj { attacker: 0, ..d }), 0, me, 4.5, true, &sk(15), &clear), 0, "my own delayed attack");
    }

    /// `trunc` is `cvttss2si`: NaN and huge values give INT_MIN.
    #[test]
    fn trunc_matches_cvttss2si() {
        assert_eq!((trunc(2.9), trunc(-2.9), trunc(f32::NAN), trunc(3.0e10)), (2, -2, i32::MIN, i32::MIN));
    }
}
