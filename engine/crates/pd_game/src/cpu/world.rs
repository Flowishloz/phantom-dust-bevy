//! What the CPU natives read of the real sim (`Match`): fighters, hand slots, statuses, capsules, attack objects, line of
//! sight. Read-only; every function says which program field it stands for (FORMATS-CPU.md section 4.2 / 4.3 / 4.7).
//! Where our sim has no equivalent of a program field the doc comment says so and the value is an explicit constant
//! (never a guess dressed up as a read).

use super::nav::Cad;
use super::threat::{Obj, SkillFields};
use crate::combat::{Action, Capsule, Combat, MAX_CAPSULES_PER_PLAYER, MAX_PLAYERS, SLOTS};
use crate::moves::effects::Stat;
use crate::moves::{self, body_centre, Flight, HIT_BODY_CENTRE, HIT_BODY_RADIUS};
use crate::sim::{Dodge, Match};
use glam::Vec3;
use pd_formats::collision::Collision;

/// The body's radius as the predictor sees it (`P+0x12e9c`, character table +0x10 / 10).
pub const BODY_MARGIN: f32 = HIT_BODY_RADIUS;
/// Height of the body's centre above the feet (`P+0x12e98`, character table +0x12 / 10).
#[allow(dead_code)]
pub const AIM_HEIGHT: f32 = HIT_BODY_CENTRE;
/// Objects per player in the program's object table (`FUN_14020f4f0`: object number = player * 0x30 + slot).
pub const OBJ_PER_PLAYER: usize = 0x30;
/// The program's nominal dodge-record ids (`charaset` dodge field): the Jump character move and the ground move.
pub const DODGE_JUMP: i32 = 750;
pub const DODGE_GROUND: i32 = 751;

/// The skill record fields the natives read, one per skill id (`DAT_1404bdc50 + 0x90 * id`; field numbers = our `raw[s]`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SkRec {
    #[allow(dead_code)]
    pub id: i32,
    pub s9: i32,
    pub s25: i32,
    pub s42: i32,
    pub s43: i32,
    pub s44: i32,
    pub s45: i32,
    pub s52: i32,
    pub s55: i32,
    pub s63: i32,
    pub s64: i32,
    pub s65: i32,
    pub cost: i32,
    pub cost_per_use: i32,
}

#[derive(Clone, Debug, Default)]
pub struct SkillTable {
    rec: Vec<Option<SkRec>>,
}

impl SkillTable {
    pub fn from_combat(c: &Combat) -> SkillTable {
        let max = c.skills.iter().map(|s| s.id.max(0) as usize).max().unwrap_or(0);
        let mut rec = vec![None; max + 1];
        for s in &c.skills {
            if s.id < 0 {
                continue;
            }
            let r = &s.raw;
            rec[s.id as usize] = Some(SkRec {
                id: s.id as i32,
                s9: r[9] as i32,
                s25: r[25] as i32,
                s42: r[42] as i32,
                s43: r[43] as i32,
                s44: r[44] as i32,
                s45: r[45] as i32,
                s52: r[52] as i32,
                s55: r[55] as i32,
                s63: r[63] as i32,
                s64: r[64] as i32,
                s65: r[65] as i32,
                cost: s.cost,
                cost_per_use: s.raw[24] as i32,
            });
        }
        SkillTable { rec }
    }

    pub fn get(&self, id: i64) -> Option<&SkRec> {
        usize::try_from(id).ok().and_then(|i| self.rec.get(i)).and_then(|r| r.as_ref())
    }

    pub fn fields(&self, id: i32) -> Option<SkillFields> {
        self.get(id as i64).map(|r| SkillFields { s43: r.s43, s63: r.s63, s55: r.s55, s52: r.s52, s64: r.s64, s65: r.s65 })
    }

    /// `FUN_140167910`: an attack whose motion is 3, 12, 13 or 14 (short-range / no-aim).
    pub fn no_aim(&self, id: i64) -> bool {
        self.get(id).is_some_and(|r| r.s43 == 0 && matches!(r.s63, 3 | 12 | 13 | 14))
    }

    /// `FUN_140167970` (+ `FUN_14020f330`: 3 reads as 4): the delivery class the script switches on.
    pub fn class(&self, id: i64) -> i64 {
        let Some(r) = self.get(id) else { return 0 };
        let c = if r.s43 == 0 {
            if self.no_aim(id) {
                1
            } else if r.s63 == 8 || r.s63 == 9 {
                5
            } else if r.s63 != 4 && r.s63 != 0x10 {
                if r.s44 != 1 {
                    2
                } else if !matches!(r.s63, 0 | 5 | 10 | 11) {
                    4
                } else {
                    3
                }
            } else {
                0
            }
        } else {
            0
        };
        if c == 3 {
            4
        } else {
            c
        }
    }
}

// ---- fighters --------------------------------------------------------------------------------------------------

pub fn present(m: &Match, p: usize) -> bool {
    p < MAX_PLAYERS && m.combat.roster.is_active(p)
}

/// `f124 cond(p)` (`0x14020e980`): 0 absent, 1 idle, 2 casting (states 10..12), 3 state 0x11 (airborne knock-down), 4 acting
/// (any other reaction state), 5 out. 6 ("not entered") does not exist in our matches.
/// Mapping from `Action` (UNCONFIRMED for the dodge and landing states, which the predicate counts as not acting).
pub fn cond(m: &Match, p: usize) -> i64 {
    if !present(m, p) {
        return 0;
    }
    let f = &m.combat.fighters[p];
    if f.health < 1 || f.action == Action::Down {
        return 5;
    }
    match f.action {
        Action::Cast | Action::Lunge => 2,
        Action::Knocked if f.flying => 3,
        Action::Hit | Action::Knocked => 4,
        _ => 1,
    }
}

/// `f125`: the skill being cast while the state is a cast state, else -1.
pub fn cast_skill(m: &Match, p: usize) -> i64 {
    if !present(m, p) {
        return -1;
    }
    let f = &m.combat.fighters[p];
    if matches!(f.action, Action::Cast | Action::Lunge) {
        f.cast_skill as i64
    } else {
        -1
    }
}

/// `f134`: the character's dodge record id: 750 (the Jump move) or 751 (a ground move), `charaset` `+0x36`.
pub fn dodge_skill(m: &Match, p: usize) -> i64 {
    if !present(m, p) {
        return 0;
    }
    if m.players[p].dodge == Dodge::Jump { DODGE_JUMP as i64 } else { DODGE_GROUND as i64 }
}

/// `f122 team_of(p)`: a team number that is equal for teammates (-1 for an empty slot). The versus setup writes the seat index
/// (`FUN_1401d0750`) and the script only compares teams, so the lowest member's slot stands for the team.
pub fn team_of(m: &Match, p: usize) -> i64 {
    if !present(m, p) {
        return -1;
    }
    (0..MAX_PLAYERS).find(|&j| present(m, j) && m.combat.roster.same_team(p, j)).unwrap_or(p) as i64
}

/// `f121 has_status(p, id)` (`FUN_14024d4d0`), for the ids the script reads (FORMATS-CPU 4.5). 12 and 17 are never active
/// (12: UNCONFIRMED writer; 17: no registered skill writes it).
pub fn has_status(m: &Match, p: usize, id: i64) -> i64 {
    if !present(m, p) {
        return 0;
    }
    let (c, f) = (&m.combat, &m.combat.fighters[p]);
    match id {
        0 => c.stat(p, Stat::Attack) as i64,
        1 => c.stat(p, Stat::Speed) as i64,
        2 => c.stat(p, Stat::Homing) as i64,
        3 => c.stat(p, Stat::AuraRate) as i64,
        4 => (f.status.bound_ticks > 0) as i64,
        5 => (f.frozen_ticks > 0) as i64,
        6 => (f.poison_ticks > 0) as i64,
        8 => (f.status.invisible_ticks > 0) as i64,
        9 => f.forced_lock.map_or(0, |(who, _)| who as i64 + 1),
        13..=16 => (f.status.locked_ticks > 0 && f.status.locked_slot as i64 == id - 13) as i64,
        35 => c.inverted(p) as i64,
        36 => (f.status.scrambled_ticks > 0) as i64,
        _ => 0,
    }
}

/// The position fed to the vectors (`P+0x12e40`): the feet.
pub fn pos(m: &Match, p: usize) -> Vec3 {
    m.players[p].pos
}

/// `f123(p)`: Skills I still hold access to = skills not yet dealt + capsules not retired (lying, appearing or waiting to
/// regenerate). Derived from the program's own sum `arr_b[p] - arr_a[p] + 30` = `(30 - dealt) + DAT_140585c20[p]`, the same
/// quantity `Combat::requirement_met` reads for the "N or fewer skills left" rule (`FUN_140262470` case 5). The script only tests `<= 1`
/// and `<= 3`.
pub fn skills_left(m: &Match, p: usize) -> i64 {
    if !present(m, p) {
        return 0;
    }
    let c = &m.combat;
    let lying = c.capsules.iter().filter(|k| k.owner == p).count();
    let waiting = c.pending_capsules().iter().filter(|k| k.owner == p).count();
    (c.fighters[p].arsenal.len() + lying + waiting) as i64
}

// ---- hand slots (f144-f150) ------------------------------------------------------------------------------------

pub fn slot_skill(m: &Match, p: usize, k: usize) -> i64 {
    if !present(m, p) || k >= SLOTS {
        return -1;
    }
    m.combat.fighters[p].slots[k].map_or(-1, |s| s as i64)
}

/// `f147`: the Aura the slot's skill costs now (`slot+0x18`: base + per-use change).
pub fn slot_cost(m: &Match, p: usize, k: usize, t: &SkillTable) -> i64 {
    let id = slot_skill(m, p, k);
    let Some(r) = t.get(id) else { return 0 };
    (r.cost + r.cost_per_use * m.combat.fighters[p].spent[k] as i32) as i64
}

/// `f148 slot_ready`: occupied, state 0 and not locked (`slot+0x40 == -1`). State 0 = idle; our slots have no other state, so
/// the only things that make a slot "not ready" are a Frostbite lock on that button. The state values 2 / 4 the program also
/// has (accepted by the capture / defence checks) are UNCONFIRMED and never reported (`slot_state`).
pub fn slot_ready(m: &Match, p: usize, k: usize) -> i64 {
    if slot_skill(m, p, k) < 0 {
        return 0;
    }
    let st = &m.combat.fighters[p].status;
    (!(st.locked_ticks > 0 && st.locked_slot as usize == k)) as i64
}

/// `f150 slot_state(p, k, x)`: 1 iff occupied and the slot state equals `x`. Our slots are always state 0 (see `slot_ready`).
pub fn slot_state(m: &Match, p: usize, k: usize, x: i64) -> i64 {
    (slot_skill(m, p, k) >= 0 && x == 0) as i64
}

// ---- capsules (f192-f200) --------------------------------------------------------------------------------------

/// The capsules deployed for player `p` (the program's per-player list of 3, `0x140585c90`): index `k` = the stage spot the
/// capsule sits on, `Some((orb position, skill))` while placed and pickable. The program's record position is the orb (9 u above
/// the ground point; the script lowers it by 9 with `vadd`).
pub fn own_capsule(m: &Match, p: usize, k: usize) -> Option<(Vec3, i64)> {
    if !present(m, p) || k >= MAX_CAPSULES_PER_PLAYER {
        return None;
    }
    let c = &m.combat;
    c.capsules.iter().find(|it| it.owner == p && it.appear == 0 && spot_index(c, p, it.pos) == k).map(|it| (it.pos + Vec3::Y * HIT_BODY_CENTRE, it.skill as i64))
}

fn spot_index(c: &Combat, p: usize, at: Vec3) -> usize {
    c.capsule_spots[p].iter().position(|s| s.distance(at) < 1.0).unwrap_or(0).min(MAX_CAPSULES_PER_PLAYER - 1)
}

/// `f194 / f195` (`P+0x17160 / P+0x17164`): standing on one of my own capsules (1 and its index), else 0 / -1. The program's 3
/// ("a foreign capsule") needs capsules other players can capture, which our sim does not have (`capsule_in_reach` is owner-only).
pub fn contact(m: &Match, p: usize) -> (i64, i64) {
    if !present(m, p) {
        return (0, -1);
    }
    let c = &m.combat;
    match c.capsule_in_reach(p, &m.players[p]) {
        Some(i) => (1, spot_index(c, p, c.capsules[i].pos) as i64),
        None => (0, -1),
    }
}

// ---- attack objects (f176-f186) --------------------------------------------------------------------------------

/// One attack object as the script scans it.
#[derive(Clone, Copy, Debug)]
pub struct ObjEntry {
    /// Object number: `owner * 0x30 + slot` (`FUN_14020f4f0`).
    pub num: usize,
    pub obj: Obj,
}

/// The attack objects in flight, numbered by owner then flight order (`slot` = the order in `Combat::projectiles`, at most 0x30 per
/// owner). Areas, mines and blasts are not listed (UNCONFIRMED how the program fills their `+0x1c0` fields; the predictor's rules for
/// motions 7, 9, 10, 15, 16 are READ but nothing feeds them). The mapping of a `Flight` to the program's float views is ours
/// (FORMATS-CPU 7 item 5, UNCONFIRMED): per-tick speed -> `+0x1c0` (shell, fall, lob horizontal), `+0x1c4` (crawl; lob vertical),
/// gravity -> `+0x1cc`, heading -> `+0xf0`. Arcs and ricochets carry stored times the sim does not have: they predict 0.
pub fn objects(m: &Match, t: &SkillTable) -> Vec<ObjEntry> {
    let c = &m.combat;
    let mut slot = [0usize; MAX_PLAYERS];
    let mut out = Vec::new();
    for p in &c.projectiles {
        if p.owner >= MAX_PLAYERS || slot[p.owner] >= OBJ_PER_PLAYER {
            continue;
        }
        let num = p.owner * OBJ_PER_PLAYER + slot[p.owner];
        slot[p.owner] += 1;
        let sh = p.shot();
        if sh.delay > 0 {
            continue;
        }
        let mut mask = 0u32;
        if !p.harmless {
            for v in c.victims_of(p.owner) {
                mask |= 1 << v;
            }
        }
        let homing = if sh.homing > 0.0 { Some(p.target as i32) } else { None };
        let mut o = Obj { state: 1, skill: p.skill as i32, age: sh.age as i32, hitmask: mask, attacker: p.owner as i32, target: homing, dir: Vec3::Z, pos: sh.pos, radius: sh.radius, ..Obj::default() };
        match &sh.flight {
            Flight::Shell(f) => {
                let v = moves::ramp(f.speed, f.end_speed, f.ramp, sh.age);
                (o.dir, o.f1c0, o.f1d0, o.f1c4) = (f.fwd, v, v, v);
            }
            Flight::Fall(f) => {
                (o.dir, o.f1c0, o.f1c4) = (f.frame.fwd, f.speed, f.speed);
            }
            Flight::Crawl(f) => {
                let v = moves::ramp(f.speed, f.end_speed, f.ramp, sh.age);
                (o.dir, o.f1c0, o.f1c4) = (f.fwd, v, v);
            }
            Flight::Lob(f) => {
                // The free-flight formula (T null): the stored-time branch (`+0x1d4`) has no sim equivalent.
                o.target = None;
                (o.dir, o.f1c0, o.f1c4, o.f1cc) = (f.dir, f.speed, f.vy, f.g);
                o.yaw = ((f.dir.x.atan2(f.dir.z) * super::nav::YAW_UNITS_PER_RAD) as i32) & 0x7FFF;
            }
            Flight::Arc(f) => o.dir = f.frame.fwd,
            Flight::Ricochet(f) => o.dir = f.frame.fwd,
            Flight::Tornado(_) => {}
        }
        // Skill class gates: the program lists attack objects only (s43 == 0) whose motion is not 3, 4, 12, 13, 14.
        let _ = t;
        out.push(ObjEntry { num, obj: o });
    }
    out
}

/// A stage ray: is the straight line `a -> b` free of stage geometry (`FUN_1401641b0`, 1 = clear)?
pub fn ray_clear(col: &Collision, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    len < 1e-3 || col.raycast(a, d / len, len).is_none()
}

/// The predictor's inputs for me (`pos` = my feet lifted by the aim height, `margin` = my body radius, `ground_ok` = the nav line
/// between me and the object).
pub fn threat_inputs(m: &Match, me: usize, cad: Option<&Cad>, o: &Obj) -> (Vec3, f32, bool) {
    let pos = body_centre(m.players[me].pos);
    let ground = cad.is_none_or(|c| c.ground_ok(pos, o.pos));
    (pos, BODY_MARGIN, ground)
}

/// Capsule kinds by the program's s9 numbering (1 attack, 2 defence, ...), for tests.
#[allow(dead_code)]
pub fn capsule_s9(c: Capsule) -> i32 {
    match c {
        Capsule::Attack => 1,
        Capsule::Defence => 2,
        Capsule::Erase => 3,
        Capsule::Status => 4,
        Capsule::Special => 5,
        Capsule::Environment => 6,
    }
}
