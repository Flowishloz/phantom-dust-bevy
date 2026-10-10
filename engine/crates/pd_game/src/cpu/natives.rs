//! The 75 group-4 natives `eneprog006` calls, plus group 0 `rand`, implemented against the real sim (FORMATS-CPU.md section 4;
//! every arm cites the native's program address). `Env` is the `Natives` the VM calls into for one CPU seat for one program tick.
//!
//! Anything the script asks for that the program answers from state our sim does not have is answered with an explicit, documented
//! value and noted in `world.rs` (never silently): the global capsule list (`f197-f200`: empty), hand-slot states 2 / 4, status 12 / 17.

use super::nav::{Cad, Com};
use super::threat;
use super::world::{self, ObjEntry, SkillTable};
use crate::sim::Match;
use glam::Vec3;
use pd_formats::collision::Collision;
use pd_formats::ssb_vm::{Memory, Natives};

pub const NUM_REGS: usize = 50;
pub const NUM_VECS: usize = 20;

/// A deterministic 15-bit stream for `rand` (`FUN_14020dcf0` = `(rand() & 0x7fff) * n >> 15`). The program uses the MSVC CRT `rand`
/// seeded by the clock; we seed a private xorshift from the sim's seeded RNG (`Combat::rng_state`) so a replay of the same match
/// draws the same numbers and the sim's own stream is left untouched.
#[derive(Clone, Debug)]
pub struct CpuRand(pub u32);

impl CpuRand {
    pub fn next15(&mut self) -> i64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        ((x >> 16) & 0x7fff) as i64
    }
    /// `0..n-1` (`f32` / `g0.f5`).
    pub fn below(&mut self, n: i64) -> i64 {
        (self.next15() * n) >> 15
    }
}

/// The 4 x 4 pair table (`0x1404bbe20`): line of fire flag and distance, refreshed one unordered pair per program tick in the order
/// (0,1) (0,2) (0,3) (1,2) (1,3) (2,3) (`FUN_14012a900`), copied symmetrically; an absent slot zeroes the pair.
#[derive(Clone, Debug, Default)]
pub struct PairTable {
    pub flag: [[i32; 4]; 4],
    pub dist: [[f32; 4]; 4],
    a: usize,
    b: usize,
}

impl PairTable {
    pub fn new() -> PairTable {
        PairTable { a: 0, b: 1, ..Default::default() }
    }

    /// One program tick of `FUN_14012a900`.
    pub fn update(&mut self, m: &Match, col: &Collision) {
        let (a, b) = (self.a, self.b);
        let (flag, dist) = if world::present(m, a) && world::present(m, b) {
            let (pa, pb) = (crate::moves::body_centre(m.players[a].pos), crate::moves::body_centre(m.players[b].pos));
            (world::ray_clear(col, pa, pb) as i32, pa.distance(pb))
        } else {
            (0, 0.0)
        };
        self.flag[a][b] = flag;
        self.dist[a][b] = dist;
        self.flag[b][a] = flag;
        self.dist[b][a] = dist;
        self.b += 1;
        if self.b > 3 {
            self.a += 1;
            if self.a > 2 {
                self.a = 0;
            }
            self.b = self.a + 1;
        }
    }
}

/// Things a native asks the host to do to the match after the script returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// `f229`: Health += x (`FUN_14024d7a0`).
    Health(usize, i32),
    /// `f235`: put the player back on its start point (`FUN_1402501d0`).
    Respawn(usize),
}

/// One recorded native call, for the call-log cross-check and the per-tick log.
#[derive(Clone, Debug, PartialEq)]
pub struct CallRec {
    pub tick: i64,
    pub group: u8,
    pub func: u16,
    /// The argument words, low 32 bits sign-extended (as the natives read them).
    pub args: Vec<i64>,
    pub ret: i64,
    /// The vector register the call wrote, with its new value (natives f2, f21, f113, f114, f179, f193): lets the cross-check
    /// follow the vector file without knowing the world.
    pub vec_out: Option<(i32, [f32; 3])>,
}

/// One CPU seat's script state: the AI struct's registers and vectors (`ai+0x2074`, `ai+0x213c`), the target, the nav controller.
#[derive(Clone, Debug)]
pub struct SeatCore {
    pub slot: usize,
    pub regs: [i32; NUM_REGS],
    /// `R[-1]`: the object scan cursor (`*DAT_1404bc5f0`).
    pub scan: i32,
    pub vecs: [Vec3; NUM_VECS],
    /// `ai+0x0c`: the target player index (-1 none).
    pub target: i32,
    pub com: Com,
    pub rng: CpuRand,
    /// Skill buttons pressed this program tick (`P+0x14264`): bit k = slot k.
    pub pressed: u8,
}

impl SeatCore {
    pub fn new(slot: usize, pos: Vec3, seed: u32) -> SeatCore {
        SeatCore { slot, regs: [0; NUM_REGS], scan: 0, vecs: [Vec3::ZERO; NUM_VECS], target: 0, com: Com::at(pos), rng: CpuRand(seed | 1), pressed: 0 }
    }
}

pub struct Env<'a> {
    pub m: &'a Match,
    pub col: &'a Collision,
    pub core: &'a mut SeatCore,
    pub cad: Option<&'a Cad>,
    pub tab: &'a SkillTable,
    pub pairs: &'a PairTable,
    pub objs: &'a [ObjEntry],
    /// The global 30 Hz tick counter (`DAT_1404bc63c`).
    pub tick30: i32,
    pub stage_code: i32,
    pub effects: &'a mut Vec<Effect>,
    pub log: Option<&'a mut Vec<CallRec>>,
    /// Natives called that this host does not implement (the script is `eneprog006`: expected to stay empty).
    pub unknown: &'a mut std::collections::BTreeSet<(u8, u16)>,
}

fn f32_arg(a: i64) -> f32 {
    f32::from_bits(a as u32)
}

impl Env<'_> {
    fn me(&self) -> usize {
        self.core.slot
    }

    fn vec(&self, i: i32) -> Vec3 {
        self.core.vecs[(i.max(0) as usize).min(NUM_VECS - 1)]
    }
    fn set_vec(&mut self, i: i32, v: Vec3) {
        let k = (i.max(0) as usize).min(NUM_VECS - 1);
        self.core.vecs[k] = v;
    }

    fn player(&self, p: i32) -> Option<usize> {
        (0..4).contains(&p).then_some(p as usize).filter(|&p| world::present(self.m, p))
    }

    fn obj(&self, num: i32) -> Option<&ObjEntry> {
        self.objs.iter().find(|o| o.num as i32 == num)
    }

    /// `FUN_14020f9a0` through the wrapper `0x1402105a0`.
    fn predict(&self, e: &ObjEntry) -> i64 {
        let me = self.me();
        let (pos, margin, ground) = world::threat_inputs(self.m, me, self.cad, &e.obj);
        let t = self.tab;
        let col = self.col;
        threat::predict(Some(&e.obj), me as i32, pos, margin, ground, &|id| t.fields(id), &|a, b| world::ray_clear(col, a, b)) as i64
    }

    fn group4(&mut self, f: u16, a: &[i64]) -> i64 {
        let ai = |i: usize| a.get(i).copied().unwrap_or(0) as i32;
        let al = |i: usize| a.get(i).copied().unwrap_or(0) as i32 as i64;
        let m = self.m;
        match f {
            // ---- pure helpers: registers and vectors (`0x14020d460`...) ----
            0 => {
                if (0..NUM_REGS as i32).contains(&ai(0)) {
                    self.core.regs[ai(0) as usize] = ai(1);
                }
                0
            }
            1 => {
                if (0..NUM_REGS as i32).contains(&ai(0)) {
                    self.core.regs[ai(0) as usize] as i64
                } else {
                    0
                }
            }
            2 => {
                let v = self.vec(ai(1));
                self.set_vec(ai(0), v);
                0
            }
            16 => ((self.vec(ai(0)) - self.vec(ai(1))).length() * 0.1) as i64,
            21 => {
                let d = Vec3::new(f32_arg(a.get(1).copied().unwrap_or(0)), f32_arg(a.get(2).copied().unwrap_or(0)), f32_arg(a.get(3).copied().unwrap_or(0))) * 10.0;
                let v = self.vec(ai(0)) + d;
                self.set_vec(ai(0), v);
                0
            }
            23 => {
                let (p, q) = (self.vec(ai(0)), self.vec(ai(1)));
                let units = ((q.x - p.x).atan2(q.z - p.z) * super::nav::YAW_UNITS_PER_RAD) as i32;
                (units as f32 * 360.0 * (1.0 / 32768.0)) as i64
            }
            28 => (self.vec(ai(0)).x * 0.1 * ai(1) as f32) as i64,
            29 => (self.vec(ai(0)).y * 0.1 * ai(1) as f32) as i64,
            30 => (self.vec(ai(0)).z * 0.1 * ai(1) as f32) as i64,
            32 => self.core.rng.below(al(0)),
            33 => {
                let d = ai(0) - ai(1);
                (if d < -180 { d + 360 } else { d }) as i64
            }
            49 => {
                self.core.com.request_dodge(ai(0));
                0
            }
            56 => self.core.com.stuck as i64,
            82 => {
                self.core.com.planner_style = 0;
                0
            }
            85 => {
                self.core.com.planner_style = 2;
                0
            }
            96 => {
                self.core.target = ai(0);
                0
            }
            97 => self.core.target as i64,
            112 => self.me() as i64,
            208 => self.tick30 as i64,
            210 => self.cad.map_or(0, |c| c.node_count() as i64),
            234 => self.core.com.hopeless as i64,
            425 => self.stage_code as i64,
            426 => (self.stage_code / 10) as i64,
            // ---- fighters ----
            113 => {
                if let Some(p) = self.player(ai(0)) {
                    let v = world::pos(m, p);
                    self.set_vec(ai(1), v);
                }
                0
            }
            114 => {
                if let Some(p) = self.player(ai(0)) {
                    let v = m.players[p].spawn;
                    self.set_vec(ai(1), v);
                }
                0
            }
            115 => match self.player(ai(0)) {
                Some(p) => {
                    let units = (m.players[p].yaw * super::nav::YAW_UNITS_PER_RAD) as i32;
                    (units as f32 * 360.0 * (1.0 / 32768.0)) as i64
                }
                None => 0,
            },
            117 => {
                let (x, y) = (ai(0), ai(1));
                if (0..4).contains(&x) && (0..4).contains(&y) {
                    self.pairs.flag[x as usize][y as usize] as i64
                } else {
                    0
                }
            }
            118 => self.player(ai(0)).map_or(0, |p| m.combat.fighters[p].health.max(0) as i64),
            119 => self.player(ai(0)).map_or(0, |p| m.combat.fighters[p].level as i64),
            120 => self.player(ai(0)).map_or(0, |p| m.combat.fighters[p].aura() as i64),
            121 => self.player(ai(0)).map_or(0, |p| world::has_status(m, p, al(1))),
            122 => self.player(ai(0)).map_or(-1, |p| world::team_of(m, p)),
            123 => self.player(ai(0)).map_or(0, |p| world::skills_left(m, p)),
            124 => match self.player(ai(0)) {
                Some(p) => world::cond(m, p),
                None => 0,
            },
            125 => self.player(ai(0)).map_or(-1, |p| world::cast_skill(m, p)),
            134 => self.player(ai(0)).map_or(0, |p| world::dodge_skill(m, p)),
            135 => match self.player(ai(0)) {
                Some(p) => crate::props::nearest_throwable(&m.combat.props, m.players[p].pos).map_or(-1, |i| i as i64),
                None => -1,
            },
            // ---- hand slots ----
            144 => self.player(ai(0)).map_or(-1, |p| world::slot_skill(m, p, ai(1).max(0) as usize)),
            146 => self.player(ai(0)).map_or(-1, |p| match self.tab.get(world::slot_skill(m, p, ai(1).max(0) as usize)) {
                Some(r) => r.s9 as i64,
                None => -1,
            }),
            147 => self.player(ai(0)).map_or(0, |p| world::slot_cost(m, p, ai(1).max(0) as usize, self.tab)),
            148 => self.player(ai(0)).map_or(0, |p| world::slot_ready(m, p, ai(1).max(0) as usize)),
            150 => self.player(ai(0)).map_or(0, |p| world::slot_state(m, p, ai(1).max(0) as usize, al(2))),
            // ---- skill table ----
            161 => self.tab.get(al(0)).map_or(-1, |r| r.s9 as i64),
            162 => self.tab.get(al(0)).map_or(-1, |r| r.s43 as i64),
            165 => self.tab.class(al(0)),
            166 => self.tab.get(al(0)).map_or(-1, |r| if r.s43 == 0 { r.s63 as i64 } else { -1 }),
            168 => self.tab.get(al(0)).map_or(-1, |r| if r.s43 == 2 { r.s44 as i64 } else { -1 }),
            169 => self.tab.get(al(0)).map_or(0, |r| r.s25 as i64),
            175 => self.tab.get(al(0)).map_or(-1, |r| r.s42 as i64),
            // ---- attack objects ----
            176 => {
                self.core.scan = 0;
                0
            }
            177 => {
                let start = self.core.scan.max(0);
                for n in start..(4 * world::OBJ_PER_PLAYER as i32) {
                    let Some(e) = self.obj(n) else { continue };
                    let Some(r) = self.tab.get(e.obj.skill as i64) else { continue };
                    if r.s43 != 0 || matches!(r.s63, 3 | 4 | 12 | 13 | 14) {
                        continue;
                    }
                    self.core.scan = n + 1;
                    return n as i64;
                }
                self.core.scan = 4 * world::OBJ_PER_PLAYER as i32;
                -1
            }
            178 => self.obj(ai(0)).map_or(-1, |e| e.obj.skill as i64),
            179 => {
                if let Some(e) = self.obj(ai(0)) {
                    let p = e.obj.pos;
                    self.set_vec(ai(1), p);
                }
                0
            }
            186 => match self.obj(ai(0)) {
                Some(e) => self.predict(e),
                None => 0,
            },
            // ---- capsules ----
            192 => match self.player(ai(0)).and_then(|p| world::own_capsule(m, p, ai(1).max(0) as usize)) {
                Some((_, skill)) => skill,
                None => -1,
            },
            193 => {
                if let Some((pos, _)) = self.player(ai(0)).and_then(|p| world::own_capsule(m, p, ai(1).max(0) as usize)) {
                    self.set_vec(ai(2), pos);
                }
                0
            }
            194 => world::contact(m, self.me()).0,
            195 => world::contact(m, self.me()).1,
            // The program's shared 4-entry capsule list: nothing in a versus match fills it (UNCONFIRMED what does), so it is empty.
            197 => -1,
            198 => 0,
            199 => 0,
            200 => -1,
            // ---- effects on the match ----
            229 => {
                if let Some(p) = self.player(ai(0)) {
                    self.effects.push(Effect::Health(p, ai(1)));
                }
                0
            }
            235 => {
                if let Some(p) = self.player(ai(0)) {
                    self.core.com.stuck = 0;
                    self.effects.push(Effect::Respawn(p));
                }
                0
            }
            // ---- the pad and the nav controller ----
            19 => {
                let v = self.vec(ai(0));
                self.core.com.move_to(v);
                0
            }
            20 => match self.cad {
                Some(c) => self.core.com.nav_step(c, ai(0)) as i64,
                None => 0,
            },
            48 => {
                let s = ai(0);
                if (0..4).contains(&s) {
                    self.core.pressed |= 1 << s;
                }
                0
            }
            18 => self.cad.map_or(-1, |c| c.node_at(self.vec(ai(0)), false) as i64),
            31 => self.cad.map_or(-1, |c| c.node_at(self.vec(ai(0)), true) as i64),
            24 => self.cad.map_or(0, |c| c.ground_ok(self.vec(ai(0)), self.vec(ai(1))) as i64),
            37 => match self.cad {
                Some(c) if (0..c.node_count() as i32).contains(&ai(0)) && (0..c.node_count() as i32).contains(&ai(1)) => (c.centroid(ai(0) as usize).distance(c.centroid(ai(1) as usize)) * 0.1) as i64,
                _ => 0,
            },
            51 => {
                let (avoid, goal) = (self.vec(ai(0)), self.vec(ai(1)));
                let me = world::pos(m, self.me());
                match self.cad {
                    Some(c) => self.core.com.path51(c, me, avoid, goal) as i64,
                    None => 0,
                }
            }
            53 => {
                // `FUN_14020e020`: the dodge record's s43 == 3 && s44 == 8 (else 1); a probe point D = s45 * 10 along the ABSOLUTE heading.
                let me = self.me();
                let rec = self.tab.get(world::dodge_skill(m, me));
                let Some(r) = rec else { return 1 };
                if !(r.s43 == 3 && r.s44 == 8) {
                    return 1;
                }
                let d = r.s45 as i16 as f32 * 10.0;
                let yaw = (ai(0) as f32 * 91.02222) as i32 as f32 / super::nav::YAW_UNITS_PER_RAD;
                let pos = world::pos(m, me);
                let target = pos + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * d;
                self.cad.map_or(1, |c| c.ground_ok(pos, target) as i64)
            }
            60 => {
                let pos = world::pos(m, self.me());
                match self.cad {
                    Some(c) => self.core.com.path_step(c, pos) as i64,
                    None => 0,
                }
            }
            _ => {
                self.unknown.insert((4, f));
                0
            }
        }
    }
}

impl Natives for Env<'_> {
    fn call(&mut self, group: u8, func: u16, args: &[i64], _mem: &Memory) -> i64 {
        let a: Vec<i64> = args.iter().map(|&x| x as i32 as i64).collect();
        let ret = match group {
            4 => self.group4(func, args),
            0 if func == 5 => self.core.rng.below(a.first().copied().unwrap_or(0)),
            _ => {
                self.unknown.insert((group, func));
                0
            }
        };
        if self.log.is_some() {
            let tick = self.tick30 as i64;
            let vi = match (group, func) {
                (4, 2) | (4, 21) => a.first().copied(),
                (4, 113) | (4, 114) | (4, 179) => a.get(1).copied(),
                (4, 193) => a.get(2).copied(),
                _ => None,
            };
            let vec_out = vi.map(|i| (i as i32, self.vec(i as i32).to_array()));
            if let Some(l) = self.log.as_deref_mut() {
                l.push(CallRec { tick, group, func, args: a, ret, vec_out });
            }
        }
        ret
    }
}
