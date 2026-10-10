//! CPU host tests: one per native group (program address in each name / comment), the pad conversion, determinism, and the call-log
//! cross-check against `engine/re/ssb_run.py`. Tests that need the game's data (cache `st03`, `eneprog006.ssb`, `.cad`) skip when the
//! files are absent.

use super::host::{stick_for_heading, CpuData, CpuHost};
use super::nav::{Cad, YAW_UNITS_PER_RAD};
use super::natives::{Effect, Env, PairTable};
use super::world::{self, SkillTable};
use crate::combat::{self, Combat, Roster};
use crate::sim::{process_axis, Dodge, Input, Match};
use glam::Vec3;
use pd_formats::collision::Collision;
use pd_formats::ssb_vm::{Memory, Natives};
use std::path::PathBuf;

pub const CACHE: &str = r"..\..\cache";

fn cache() -> Option<PathBuf> {
    let p = PathBuf::from(CACHE);
    p.join("skills.json").exists().then_some(p)
}

/// A two-player match on a stage (its own start points), countdown skipped.
pub struct Rig {
    pub m: Match,
    pub col: Collision,
    pub host: CpuHost,
}

impl Rig {
    pub fn new(stage: &str, cpu_row: Option<u8>) -> Option<Rig> {
        let cache = cache()?;
        let col = Collision::from_bytes(&std::fs::read(cache.join(format!("{stage}.pdcl"))).ok()?).ok()?;
        let layout = crate::stage_layout(&cache, stage, 2)?;
        let mut combat = Combat::with_roster(crate::load_skills(&cache.join("skills.json"), false), Roster::duel(), std::array::from_fn(|_| crate::test_arsenal()));
        combat.set_capsule_spots(layout.capsule_spots.clone());
        combat.props = layout.props.clone();
        let mut m = Match::with_spawns(&layout.spawns, combat);
        m.jump_zones = layout.jump_zones;
        m.combat.round_ticks = combat::COUNTDOWN_TICKS;
        let data: CpuData = super::load_data(&cache).ok()?;
        let cad = super::load_cad(&cache, stage);
        let mut host = CpuHost::new(data, cad, &m, &col, super::stage_code(stage));
        if let Some(row) = cpu_row {
            host.add_seat(&m, 1, row);
        }
        Some(Rig { m, col, host })
    }

    /// Run `f` with an `Env` for the seat in slot `slot`.
    pub fn env<R>(&mut self, slot: usize, f: impl FnOnce(&mut Env) -> R) -> R {
        let objs = world::objects(&self.m, &self.host.tab);
        let mut effects = Vec::new();
        let si = self.host.seats.iter().position(|s| s.core.slot == slot).expect("seat");
        let h = &mut self.host;
        let mut env = Env {
            m: &self.m,
            col: &self.col,
            core: &mut h.seats[si].core,
            cad: h.cad.as_ref(),
            tab: &h.tab,
            pairs: &h.pairs,
            objs: &objs,
            tick30: 7,
            stage_code: h.stage_code,
            effects: &mut effects,
            log: None,
            unknown: &mut h.unknown,
        };
        f(&mut env)
    }

    /// A group 4 native with `a0` first (the top of stack).
    pub fn call(&mut self, slot: usize, f: u16, a: &[i64]) -> i64 {
        let img = [0u8; 4];
        self.env(slot, |e| e.call(4, f, a, &Memory { image: &img }))
    }
}

macro_rules! rig_or_skip {
    () => {
        match Rig::new("st03", Some(20)) {
            Some(r) => r,
            None => {
                eprintln!("skipped: game data not present");
                return;
            }
        }
    };
}

// ---- the pad -> Input conversion (FUN_140178ad0 -> sim) ------------------------------------------------------------

#[test]
fn stick_conversion_reproduces_the_absolute_heading_through_the_dead_zone() {
    // `0x140256600` dead zone, `sim::step_player_at` camera frame: for every heading and camera the processed stick points along the
    // requested absolute yaw with magnitude ~1 (so the gait is Run: > 0.7).
    for cam_deg in [0.0f32, 37.0, -120.0, 200.0] {
        for h in 0..72 {
            let yaw = (h as f32 * 5.0).to_radians();
            let cam = cam_deg.to_radians();
            let s = stick_for_heading(yaw, cam);
            let (x, y) = (process_axis(s[0] as f32 / 127.0), process_axis(s[1] as f32 / 127.0));
            let fwd = Vec3::new(cam.sin(), 0.0, cam.cos());
            let right = Vec3::new(-fwd.z, 0.0, fwd.x);
            let world = right * x + fwd * y;
            assert!(world.length() > 0.9 && world.length() < 1.1, "mag {} at {h}", world.length());
            let got = world.x.atan2(world.z);
            let d = (got - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            assert!(d.abs() < 0.03, "heading {h} cam {cam_deg}: off by {d}");
        }
    }
}

// ---- pure natives (0x14020d460..) ---------------------------------------------------------------------------------

#[test]
fn registers_and_vectors_f0_f1_f2_f16_f21_f28_to_f30() {
    let mut r = rig_or_skip!();
    // 0x14020d460 / 480 setvar / getvar; v30..v38 hold the epa row (FUN_14017b020): row 20 = think 30 (v31), 88 % attack (v32).
    assert_eq!((r.call(1, 1, &[31]), r.call(1, 1, &[32])), (30, 88));
    r.call(1, 0, &[5, 123]);
    assert_eq!(r.call(1, 1, &[5]), 123);
    // 0x14020d490 vcopy, 0x14020d4d0 dist = (int)(|a - b| * 0.1), 0x14020d6f0 vadd with float-bit arguments (-0.9f -> -9 world units).
    r.env(1, |e| {
        e.core.vecs[3] = Vec3::new(100.0, 40.0, 0.0);
        e.core.vecs[4] = Vec3::ZERO;
    });
    assert_eq!(r.call(1, 16, &[3, 4]), 10);
    r.call(1, 2, &[5, 3]);
    assert_eq!(r.env(1, |e| e.core.vecs[5]), Vec3::new(100.0, 40.0, 0.0));
    let minus_09 = (-0.9f32).to_bits() as i32 as i64;
    r.call(1, 21, &[5, 0, minus_09, 0]);
    assert!((r.env(1, |e| e.core.vecs[5]) - Vec3::new(100.0, 31.0, 0.0)).length() < 1e-3);
    // 0x14020d900 / 940 / 980: (int)(v * 0.1 * k)
    assert_eq!((r.call(1, 28, &[3, 2]), r.call(1, 29, &[3, 10]), r.call(1, 30, &[3, 5])), (20, 40, 0));
}

#[test]
fn heading_and_angdiff_f23_f33() {
    let mut r = rig_or_skip!();
    // 0x14020d7b0: degrees from a to b = atan2(dx, dz); 0 along +z, 90 along +x.
    r.env(1, |e| {
        e.core.vecs[1] = Vec3::ZERO;
        e.core.vecs[2] = Vec3::new(50.0, 0.0, 0.0);
        e.core.vecs[3] = Vec3::new(0.0, 0.0, 50.0);
    });
    assert_eq!(r.call(1, 23, &[1, 2]), 90);
    assert_eq!(r.call(1, 23, &[1, 3]), 0);
    // 0x14020dd10: d = a - b; only the low side is wrapped.
    assert_eq!((r.call(1, 33, &[10, 20]), r.call(1, 33, &[-170, 170]), r.call(1, 33, &[170, -170])), (-10, 20, 340));
}

#[test]
fn rand_is_uniform_in_range_and_deterministic_f32_g0f5() {
    // 0x14020dcf0 = (rand & 0x7fff) * n >> 15: 0..n-1; seeded from the sim's RNG state, so two rigs draw the same numbers.
    let mut a = rig_or_skip!();
    let mut b = Rig::new("st03", Some(20)).unwrap();
    let xs: Vec<i64> = (0..200).map(|_| a.call(1, 32, &[100])).collect();
    let ys: Vec<i64> = (0..200).map(|_| b.call(1, 32, &[100])).collect();
    assert_eq!(xs, ys);
    assert!(xs.iter().all(|&x| (0..100).contains(&x)));
    assert!(xs.iter().any(|&x| x < 10) && xs.iter().any(|&x| x > 90));
    let img = [0u8; 4];
    let g0 = a.env(1, |e| e.call(0, 5, &[100], &Memory { image: &img }));
    assert!((0..100).contains(&g0));
}

#[test]
fn target_self_tick_stage_f96_f97_f112_f208_f425_f426() {
    let mut r = rig_or_skip!();
    r.call(1, 96, &[0]);
    assert_eq!((r.call(1, 97, &[]), r.call(1, 112, &[]), r.call(1, 208, &[]), r.call(1, 425, &[]), r.call(1, 426, &[])), (0, 1, 7, 3, 0));
    r.call(1, 96, &[2]);
    assert_eq!(r.call(1, 97, &[]), 2);
}

// ---- fighters (0x14020e690..) ---------------------------------------------------------------------------------------

#[test]
fn fighter_queries_f113_to_f125_and_f134() {
    let mut r = rig_or_skip!();
    let p0 = r.m.players[0].pos;
    // 0x14020e690 / e700: positions and start points (our space); absent slots leave the vector alone.
    r.call(1, 113, &[0, 6]);
    assert_eq!(r.env(1, |e| e.core.vecs[6]), p0);
    r.call(1, 114, &[1, 7]);
    assert_eq!(r.env(1, |e| e.core.vecs[7]), r.m.players[1].spawn);
    r.env(1, |e| e.core.vecs[8] = Vec3::splat(5.0));
    r.call(1, 113, &[3, 8]);
    assert_eq!(r.env(1, |e| e.core.vecs[8]), Vec3::splat(5.0));
    // 0x14020e760 yaw in degrees, 0x14020e810..e850 Health / max Aura (Level) / Aura, 0x14020e8c0 team, 0x14020e980 cond.
    r.m.players[1].yaw = std::f32::consts::FRAC_PI_2;
    assert_eq!(r.call(1, 115, &[1]), 90);
    assert_eq!((r.call(1, 118, &[1]), r.call(1, 118, &[3])), (20, 0));
    r.m.combat.fighters[1].level = 5;
    assert_eq!(r.call(1, 119, &[1]), 5);
    assert_ne!(r.call(1, 122, &[0]), r.call(1, 122, &[1]), "duel: two teams");
    assert_eq!((r.call(1, 124, &[1]), r.call(1, 124, &[3])), (1, 0));
    r.m.combat.fighters[0].action = combat::Action::Cast;
    r.m.combat.fighters[0].cast_skill = 65;
    assert_eq!((r.call(1, 124, &[0]), r.call(1, 125, &[0]), r.call(1, 125, &[1])), (2, 65, -1));
    r.m.combat.fighters[0].action = combat::Action::Knocked;
    r.m.combat.fighters[0].flying = true;
    assert_eq!(r.call(1, 124, &[0]), 3);
    r.m.combat.fighters[0].health = 0;
    assert_eq!(r.call(1, 124, &[0]), 5);
    // 0x14020ed30: 750 for the Jump dodge, 751 otherwise.
    r.m.players[0].dodge = Dodge::Tumble;
    assert_eq!((r.call(1, 134, &[1]), r.call(1, 134, &[0])), (750, 751));
}

#[test]
fn pair_table_refreshes_one_pair_per_tick_in_the_programs_order_f117() {
    // FUN_14012a900: pairs (0,1) (0,2) (0,3) (1,2) (1,3) (2,3), one per program tick, copied symmetrically; absent pairs are zeroed.
    let r = rig_or_skip!();
    let mut t = PairTable::new();
    assert_eq!(t.flag[0][1], 0, "nothing known before the first refresh");
    t.update(&r.m, &r.col);
    let seen = t.flag[0][1];
    assert_eq!(t.flag[1][0], seen, "symmetric copy");
    assert!(t.dist[0][1] > 50.0, "pair distance stored with the flag");
    for _ in 0..5 {
        t.update(&r.m, &r.col);
    }
    // After a full cycle of six the next refresh is (0,1) again: poison its flag and watch the 7th update restore it.
    t.flag[0][1] = 9;
    t.update(&r.m, &r.col);
    assert_eq!(t.flag[0][1], seen);
}

#[test]
fn statuses_f121() {
    let mut r = rig_or_skip!();
    let f = &mut r.m.combat.fighters[0];
    f.frozen_ticks = 5;
    f.poison_ticks = 5;
    f.status.invisible_ticks = 5;
    f.forced_lock = Some((1, 30));
    f.status.locked_slot = 2;
    f.status.locked_ticks = 30;
    // 0x14020e880 -> FUN_14024d4d0 (a0 = player, a1 = id): 5 buttons frozen, 6 poison, 8 invisible, 9 decoy = player + 1, 13 + k = button k frozen.
    assert_eq!([5, 6, 8, 9, 15, 13, 12, 17].map(|id| r.call(1, 121, &[0, id])), [1, 1, 1, 2, 1, 0, 0, 0]);
}

// ---- hand slots and the skill table (0x14020ee00..0x14020f4d0) -----------------------------------------------------

#[test]
fn hand_slots_and_skill_fields_f144_to_f175() {
    let mut r = rig_or_skip!();
    r.m.combat.fighters[1].slots = [Some(1), Some(65), None, Some(0)];
    // 0x14020ee00 id or -1; 0x14020ee70 s9; 0x14020ef00 ready; 0x14020ef90 state.
    assert_eq!([0, 1, 2, 3].map(|k| r.call(1, 144, &[1, k])), [1, 65, -1, 0]);
    let t = SkillTable::from_combat(&r.m.combat);
    let (psy, laser, wall) = (*t.get(1).unwrap(), *t.get(65).unwrap(), *t.get(26).unwrap());
    assert_eq!(r.call(1, 146, &[1, 0]), psy.s9 as i64);
    assert_eq!(r.call(1, 146, &[1, 2]), -1);
    assert_eq!([0, 1, 2].map(|k| r.call(1, 148, &[1, k])), [1, 1, 0]);
    assert_eq!((r.call(1, 150, &[1, 0, 0]), r.call(1, 150, &[1, 0, 2]), r.call(1, 150, &[1, 2, 0])), (1, 0, 0));
    r.m.combat.fighters[1].status.locked_ticks = 9;
    r.m.combat.fighters[1].status.locked_slot = 1;
    assert_eq!(r.call(1, 148, &[1, 1]), 0, "a Frostbite-locked button is not ready (slot+0x40 != -1)");
    // 0x14020f2a0.. record fields: s9, s43, s63 (attack only), s44 (defence only), s25, s42.
    assert_eq!(r.call(1, 161, &[1]), psy.s9 as i64);
    assert_eq!((r.call(1, 162, &[26]), r.call(1, 166, &[1]), r.call(1, 166, &[26]), r.call(1, 168, &[26]), r.call(1, 168, &[1])), (wall.s43 as i64, psy.s63 as i64, -1, wall.s44 as i64, -1));
    assert_eq!((r.call(1, 169, &[1]), r.call(1, 175, &[65])), (psy.s25 as i64, laser.s42 as i64));
    assert_eq!(r.call(1, 175, &[0]), 0, "the aura particle is AI type 0");
    assert_eq!(r.call(1, 161, &[-1]), -1);
}

#[test]
fn delivery_class_f165_follows_fun_140167970() {
    let r = rig_or_skip!();
    let t = SkillTable::from_combat(&r.m.combat);
    // 0x140167970: defence (s43 != 0) -> 0; attacks: no-aim motions 3/12/13/14 -> 1, motion 8/9 -> 5, s44 != 1 -> 2, else 3 (f165 reads 3 as 4).
    assert_eq!(t.class(26), 0, "a defence skill");
    for id in 1..120 {
        let Some(s) = t.get(id) else { continue };
        let want = if s.s43 != 0 {
            0
        } else if matches!(s.s63, 3 | 12 | 13 | 14) {
            1
        } else if matches!(s.s63, 8 | 9) {
            5
        } else if s.s63 == 4 || s.s63 == 16 {
            0
        } else if s.s44 != 1 {
            2
        } else if matches!(s.s63, 0 | 5 | 10 | 11) {
            4
        } else {
            4
        };
        assert_eq!(t.class(id), want, "skill {id}");
    }
}

// ---- attack objects and the predictor (0x14020f550..0x1402105a0) -------------------------------------------------

fn fire_at(r: &mut Rig, skill: i16) {
    // Player 0 casts `skill` from slot 0 at player 1 (who stays still); run until the shot is in flight.
    r.m.combat.fighters[0].level = 8;
    r.m.combat.fighters[0].aura_ticks = 8 * combat::AURA_TICKS_PER_POINT;
    r.m.combat.fighters[0].slots[0] = Some(skill);
    r.m.players[0].target = Some(1);
    r.m.players[0].cpu_aim = true;
    r.m.players[0].locked = false;
    for t in 0..80 {
        let mut ins = [Input::default(); 2];
        if t == 0 {
            ins[0].skills = 1;
        }
        r.m.step(ins, &mut r.col);
        if !r.m.combat.projectiles.is_empty() {
            return;
        }
    }
}

#[test]
fn object_scan_and_threat_predictor_f176_f177_f178_f179_f186() {
    let mut r = rig_or_skip!();
    let a = r.m.players[0].pos;
    r.m.players[1].pos = a + Vec3::new(40.0, 0.0, 0.0);
    fire_at(&mut r, 80); // Blaster (shell)
    assert!(!r.m.combat.projectiles.is_empty(), "the shot is in flight");
    // 0x14020f550 / 560: the scan starts at object 0 of player 0 (number = player * 0x30 + slot) and ends with -1; it does not test the owner.
    r.call(1, 176, &[]);
    assert_eq!(r.call(1, 177, &[]), 0);
    assert_eq!(r.call(1, 177, &[]), -1);
    assert_eq!(r.call(1, 178, &[0]), 80);
    r.call(1, 179, &[0, 2]);
    let p = r.m.combat.projectiles[0].shot().pos;
    assert_eq!(r.env(1, |e| e.core.vecs[2]), p);
    // 0x1402105a0 -> FUN_14020f9a0: the shot homes on player 1: non-negative ticks; player 1's own object number does not exist -> 0.
    assert!(r.call(1, 186, &[0]) >= 0);
    assert_eq!(r.call(1, 186, &[48]), 0);
}

// ---- capsules (0x140210a50..) -------------------------------------------------------------------------------------

#[test]
fn capsule_lists_f192_to_f200() {
    let mut r = rig_or_skip!();
    r.m.combat.capsules.clear();
    let spot = r.m.combat.capsule_spots[1][0];
    r.m.combat.capsules.push(combat::CapsuleItem { id: 1, owner: 1, skill: 65, pos: spot, appear: 0 });
    // 0x140210a50 / a90: record (3p + k): skill id, position = the orb (9 u above the ground point; the script lowers it with vadd).
    assert_eq!((r.call(1, 192, &[1, 0]), r.call(1, 192, &[1, 1]), r.call(1, 192, &[0, 0])), (65, -1, -1));
    r.call(1, 193, &[1, 0, 4]);
    assert_eq!(r.env(1, |e| e.core.vecs[4]), spot + Vec3::Y * 9.0);
    // 0x140210b10 / b30: standing on my own capsule -> (1, index), else (0, -1).
    assert_eq!((r.call(1, 194, &[]), r.call(1, 195, &[])), (0, -1));
    r.m.players[1].pos = spot;
    assert_eq!((r.call(1, 194, &[]), r.call(1, 195, &[])), (1, 0));
    // A capsule still appearing is not placed.
    r.m.combat.capsules[0].appear = 10;
    assert_eq!(r.call(1, 192, &[1, 0]), -1);
    // The shared list (0x140210b70..cc0) is empty in our matches: no capsule, nothing due.
    assert_eq!((r.call(1, 197, &[]), r.call(1, 198, &[]), r.call(1, 200, &[])), (-1, 0, -1));
    // f123 (`0x14020e920`): skills still reachable = undealt + capsules out; an appearing capsule counts like a placed one.
    let before = r.call(1, 123, &[1]);
    r.m.combat.capsules[0].appear = 0;
    assert_eq!(r.call(1, 123, &[1]), before);
}

// ---- effects on the match -------------------------------------------------------------------------------------------

#[test]
fn health_and_respawn_are_queued_f229_f235() {
    let mut r = rig_or_skip!();
    let img = [0u8; 4];
    let q = r.env(1, |e| {
        e.call(4, 229, &[1, -100], &Memory { image: &img });
        e.call(4, 235, &[1], &Memory { image: &img });
        e.effects.clone()
    });
    assert_eq!(q, vec![Effect::Health(1, -100), Effect::Respawn(1)]);
}

// ---- the pad and the nav controller ---------------------------------------------------------------------------------

#[test]
fn nav_natives_f18_f19_f20_f24_f31_f37_f49_f53_f48_f56_f60_f82_f85_f210() {
    let mut r = rig_or_skip!();
    let Some(n) = r.host.cad.as_ref().map(|c| c.node_count()) else { return };
    assert_eq!(r.call(1, 210, &[]) as usize, n);
    // Player 1's start point is on the graph (strict and loose agree); a far point is off it (strict -1, loose nearest node).
    r.call(1, 114, &[1, 2]);
    let strict = r.call(1, 31, &[2]);
    assert!(strict >= 0, "the start point is on a nav node");
    assert_eq!(r.call(1, 18, &[2]), strict);
    r.env(1, |e| e.core.vecs[3] = Vec3::new(9000.0, 0.0, 9000.0));
    assert_eq!(r.call(1, 31, &[3]), -1);
    assert!(r.call(1, 18, &[3]) >= 0);
    // 0x14020d660 MOVE_TO and 0x14020d6b0 nav_step set the steer target while the lock is off.
    r.call(1, 19, &[2]);
    assert_eq!(r.env(1, |e| e.core.com.steer), r.env(1, |e| e.core.vecs[2]));
    assert_eq!(r.call(1, 20, &[0]), 1);
    let c0 = r.host.cad.as_ref().unwrap().centroid(0);
    assert_eq!(r.env(1, |e| e.core.com.steer), c0);
    assert_eq!(r.call(1, 24, &[2, 2]), 1, "a point sees itself");
    assert_eq!(r.call(1, 37, &[0, 0]), 0);
    // f82 / f85 planner style, f56 stuck.
    r.call(1, 85, &[]);
    assert_eq!(r.env(1, |e| e.core.com.planner_style), 2);
    r.call(1, 82, &[]);
    assert_eq!((r.env(1, |e| e.core.com.planner_style), r.call(1, 56, &[])), (0, 0));
    // 0x14020df20 path49(deg): COM+0x44 = deg * 91.0222 yaw units.
    r.call(1, 49, &[90]);
    assert_eq!(r.env(1, |e| (e.core.com.dodge_req, e.core.com.dodge_yaw)), (true, 8191));
    assert_eq!(r.call(1, 53, &[0]), 1, "dodge records 750/751 are not in skills.json: the program's `else 1`");
    // 0x14020deb0 PRESS(slot): bit k of the pressed mask.
    r.call(1, 48, &[2]);
    assert_eq!(r.env(1, |e| e.core.pressed), 4);
}

// ---- the real script on the real sim ---------------------------------------------------------------------------------

fn state_hash(m: &Match) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x100000001b3);
    };
    mix(m.tick);
    for i in m.combat.roster.slots() {
        let (p, f) = (&m.players[i], &m.combat.fighters[i]);
        for v in [p.pos.x, p.pos.y, p.pos.z, p.yaw] {
            mix(v.to_bits() as u64);
        }
        mix(f.health as u64);
        mix(f.aura_ticks as u64);
        mix(f.slots.iter().fold(0u64, |a, s| a * 1000 + s.map_or(999, |x| x as u64)));
    }
    mix(m.combat.projectiles.len() as u64);
    h
}

/// Run `ticks` sim ticks: seat 0 idle, seat 1 the CPU. Returns the hash trail (every 60 ticks) and the rig.
pub fn run_match(ticks: u32, row: u8, log: bool) -> Option<(Vec<u64>, Rig)> {
    let mut r = Rig::new("st03", Some(row))?;
    r.m.combat.round_ticks = 0; // the real countdown runs first
    r.host.log_lines = log;
    let mut trail = Vec::new();
    for t in 0..ticks {
        let mut ins = [Input::default(); 4];
        let Rig { m, host, col } = &mut r;
        host.drive(m, col, &mut ins);
        m.step(&ins[..2], col);
        if t % 60 == 0 {
            trail.push(state_hash(&r.m));
        }
    }
    Some((trail, r))
}

#[test]
fn a_cpu_seat_is_deterministic_and_actually_plays() {
    // Same match twice: identical hash trail (the CPU's rand is seeded from the sim; nothing reads a clock).
    let Some((a, ra)) = run_match(1500, 20, false) else { return eprintln!("skipped") };
    let (b, _) = run_match(1500, 20, false).unwrap();
    assert_eq!(a, b, "replay determinism with a CPU seat");
    assert!(ra.host.unknown.is_empty(), "unimplemented natives: {:?}", ra.host.unknown);
    assert!(!ra.host.seats[0].faulted);
    let start = Rig::new("st03", None).unwrap().m.players[1].pos;
    assert!(ra.m.players[1].pos.distance(start) > 20.0, "the CPU moved: {:?} -> {:?}", start, ra.m.players[1].pos);
    // A different difficulty row plays differently, still deterministically.
    let (c, _) = run_match(1500, 70, false).unwrap();
    assert_ne!(a, c);
}

// ---- cad data -----------------------------------------------------------------------------------------------------

fn game_com() -> PathBuf {
    PathBuf::from(r"..\..\..\game_files\Assets\Data\com")
}

#[test]
fn shipped_cad_files_parse_and_pass_the_consistency_check() {
    // FORMATS-CPU 5.2: the shipped files of stage folders 2..8 (the python `Cad.check`).
    let mut n = 0;
    for st in 2..=8 {
        for v in 0..3 {
            let p = game_com().join(format!("st{st:02}/comdata{v}.cad"));
            let Ok(b) = std::fs::read(&p) else { continue };
            let c = Cad::parse(&b, true).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            assert!(c.check().is_empty(), "{p:?}: {:?}", c.check());
            n += 1;
        }
    }
    if n == 0 {
        eprintln!("skipped: no .cad files");
    } else {
        assert_eq!(n, 16, "2+2+3+2+2+2+3 files in folders st02..st08 (FORMATS-CPU 5.1 says 12 files because variants 0/1 are identical)");
    }
}

#[test]
fn cad_nodes_sit_on_the_stage_floor_only_with_the_z_mirror() {
    // The file is the game's left-handed space; our collision is mirrored. Node centroids must land on stage floor with z -> -z.
    let Some(cache) = cache() else { return };
    let Ok(b) = std::fs::read(game_com().join("st03/comdata0.cad")) else { return };
    let col = Collision::from_bytes(&std::fs::read(cache.join("st03.pdcl")).unwrap()).unwrap();
    let score = |mirror: bool| {
        let c = Cad::parse(&b, mirror).unwrap();
        (0..c.node_count())
            .filter(|&n| {
                let p = c.centroid(n);
                col.ground_below(p.x, p.z, p.y + 15.0).is_some_and(|g| (g - p.y).abs() < 6.0)
            })
            .count()
    };
    let (mirrored, plain) = (score(true), score(false));
    eprintln!("st03 nodes on the floor: mirrored {mirrored}, plain {plain} of 76");
    assert!(mirrored >= 70 && mirrored > plain + 10, "mirrored {mirrored} plain {plain}");
}

#[test]
fn start_points_are_on_nav_nodes_and_routes_connect_them() {
    // Our start points (stage layout) stand on nav nodes of the mirrored graph, and the stored route table joins them.
    let Some(r) = Rig::new("st03", None) else { return };
    let Some(c) = &r.host.cad else { return };
    let (a, b) = (c.node_at(r.m.players[0].pos, true), c.node_at(r.m.players[1].pos, true));
    assert!(a >= 0 && b >= 0, "start nodes {a} {b}");
    assert!(a == b || c.route(0, a, b).is_some(), "T0 route between the start nodes");
}

#[test]
fn yaw_unit_constant_is_a_full_turn() {
    assert!((YAW_UNITS_PER_RAD * std::f32::consts::TAU - 32768.0).abs() < 0.5);
}

/// Diagnostic: `cargo test -p pd_game print_cpu_log -- --ignored --nocapture` prints the CPU's per-tick log (every 15th program tick).
#[test]
#[ignore]
fn print_cpu_log() {
    let row: u8 = std::env::var("PD_CPU_ROW").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let (_, r) = run_match(2400, row, true).unwrap();
    for (i, l) in r.host.lines.iter().enumerate() {
        let w: Option<(usize,usize)> = std::env::var("PD_CPU_WIN").ok().and_then(|v| { let mut it=v.split(','); Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?)) });
        if w.map_or(i % 15 == 0, |(a, b)| i * 2 + 180 >= a && i * 2 + 180 < b) {
            println!("{l}");
        }
    }
    println!("unknown natives: {:?}; faulted {}", r.host.unknown, r.host.seats[0].faulted);
}

/// A duel on a flat stretch of the stage: the idle-or-shooting human in slot 0 at its start point, the CPU 120 units away on the
/// same floor, countdown over.
fn duel_close(row: u8) -> Option<Rig> {
    let mut r = Rig::new("st03", Some(row))?;
    let a = r.m.players[0].pos;
    for k in 0..16 {
        let ang = k as f32 * std::f32::consts::TAU / 16.0;
        let b = a + Vec3::new(ang.sin(), 0.0, ang.cos()) * 120.0;
        if r.col.ground_below(b.x, b.z, a.y + 5.0).is_some_and(|g| (g - a.y).abs() < 1.0) && super::world::ray_clear(&r.col, a + Vec3::Y * 9.0, b + Vec3::Y * 9.0) {
            r.m.players[1].pos = b;
            r.m.players[1].prev_pos = b;
            break;
        }
    }
    r.m.combat.round_ticks = combat::COUNTDOWN_TICKS;
    r.m.combat.set_rules(combat::Rules { start_hand: 0, ..combat::Rules::default() });
    r.m.combat.test_loadout = Some(([80, 80, 80, 80], 8));
    Some(r)
}

#[test]
fn the_cpu_answers_incoming_shots_with_a_dodge_or_guard_and_casts_in_its_band() {
    // P0-AI 2.7 / 2.6: row 55 (evasive: react 88 %, dodge 88 %) against a human firing a Blaster every 50 ticks.
    let Some(mut r) = duel_close(55) else { return eprintln!("skipped") };
    r.m.players[1].dodge = Dodge::Cartwheel;
    r.host.log_lines = false;
    let (mut dodges, mut presses, mut shots) = (0, 0, 0);
    for t in 0..1800u32 {
        let mut ins = [Input::default(); 4];
        let Rig { m, host, col } = &mut r;
        host.drive(m, col, &mut ins);
        if t % 50 == 1 {
            ins[0].skills = 1;
        }
        m.step(&ins[..2], col);
        let pad = r.host.seats[0].last_pad;
        if t % 2 == 0 {
            dodges += pad.dodge.is_some() as u32;
            presses += (r.host.seats[0].core.pressed != 0) as u32;
        }
        shots = shots.max(r.m.combat.projectiles.len() as u32);
    }
    eprintln!("dodges {dodges} presses {presses} max shots in flight {shots}");
    assert!(shots > 0 && presses > 0, "the CPU fought back");
    assert!(dodges > 0, "the CPU dodged at least one incoming shot");
}

/// The cross-check against `engine/re/ssb_run.py` (the independent Python VM): run `eneprog006` on the real sim for 900 program ticks
/// (a CPU walking to an idle fighter and casting), write every native call, and have `cpu_xcheck.py` replay the recorded answers
/// through the Python VM: the call sequence must be identical and the pure natives must recompute to the same values.
#[test]
fn native_call_log_matches_the_python_reference_vm() {
    let Some(mut r) = Rig::new("st03", Some(20)) else { return eprintln!("skipped") };
    let re = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../re/cpu_xcheck.py");
    let ssb = PathBuf::from(r"..\..\..\game_files\Assets\Data\com\ai\eneprog006.ssb");
    if !re.exists() || !ssb.exists() || std::process::Command::new("python").arg("--version").output().is_err() {
        return eprintln!("skipped: python or the script is not available");
    }
    r.m.combat.round_ticks = 0;
    r.host.calls = Some(Vec::new());
    for _ in 0..1800 {
        let mut ins = [Input::default(); 4];
        let Rig { m, host, col } = &mut r;
        host.drive(m, col, &mut ins);
        m.step(&ins[..2], col);
    }
    let calls = r.host.calls.take().unwrap();
    assert!(calls.len() > 1000, "{} calls", calls.len());
    let mut out = String::new();
    for c in &calls {
        let v = c.vec_out.map_or("null".to_string(), |(i, p)| format!("[{i},{},{},{}]", p[0], p[1], p[2]));
        out += &format!("{{\"tick\":{},\"g\":{},\"f\":{},\"a\":{:?},\"r\":{},\"v\":{}}}\n", c.tick, c.group, c.func, c.args, c.ret, v);
    }
    let log = std::env::temp_dir().join("pd_cpu_calls.jsonl");
    std::fs::write(&log, out).unwrap();
    let res = std::process::Command::new("python").arg(&re).arg(&ssb).arg(&log).arg("900").current_dir(re.parent().unwrap()).output().unwrap();
    let text = String::from_utf8_lossy(&res.stdout).to_string() + &String::from_utf8_lossy(&res.stderr);
    eprintln!("{text}");
    assert!(res.status.success(), "{text}");
}
