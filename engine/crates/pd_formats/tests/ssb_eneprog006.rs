//! The real `eneprog006.ssb` (the CPU fighter of P0-AI.md) run on the VM against a stub host.
//!
//! The stub answers the group 4 natives from a two-fighter toy world and logs every call. It is
//! NOT the game's host (lane C writes that); it only has to be plausible enough that the script
//! walks its own decision paths, so the call sequence can be compared with P0-AI.md section 2.3.
//! Skipped when `game_files` is absent. Set `PD_SSB_LOG=1` to print the call log.

use pd_formats::epa::EnemyParams;
use pd_formats::ssb::{self, Ssb};
use pd_formats::ssb_vm::{Memory, MsvcRand, Natives, RunStatus, Vm};
use std::path::PathBuf;

fn com_dir() -> PathBuf {
    std::env::var_os("PD_GAME_FILES_COM").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"..\..\..\game_files\Assets\Data\com"))
}

/// Toy skills: (ai type s42, capsule type s9). Skill ids 100.. are made up for the stub.
fn skill(id: i64) -> (i64, i64) {
    match id {
        100 => (12, 1), // type 12: mid-range straight shot, fire band 5-30 script units (Appendix B)
        101 => (10, 1), // type 10: melee, band 0-5
        _ => (100, 1),  // type 100: never used by the AI
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Call {
    tick: i64,
    func: u16,
    args: Vec<i64>,
    ret: i64,
}

struct Stub {
    tick: i64,
    regs: [i64; 48],
    vecs: [[f64; 3]; 20],
    pos: [[f64; 3]; 4],
    start: [[f64; 3]; 4],
    target: i64,
    steer: Option<[f64; 3]>,
    rng: MsvcRand,
    /// hand slots of player 0: skill ids, -1 empty
    hand: [i64; 4],
    /// an incoming enemy projectile (object 0) is visible to `scan_next` while this is true
    threat: bool,
    scan_pos: i64,
    log: Vec<Call>,
    /// (tick, slot, distance to the enemy in world units) at every PRESS
    presses: Vec<(i64, i64, f64)>,
    unknown: std::collections::BTreeSet<(u8, u16)>,
}

impl Stub {
    fn new(row: &[(usize, i64)]) -> Stub {
        let mut regs = [0i64; 48];
        for &(r, v) in row {
            regs[r] = v;
        }
        Stub {
            tick: 0,
            regs,
            vecs: [[0.0; 3]; 20],
            pos: [[0.0, 0.0, 0.0], [200.0, 0.0, 0.0], [0.0; 3], [0.0; 3]],
            start: [[-150.0, 0.0, 0.0], [350.0, 0.0, 0.0], [0.0; 3], [0.0; 3]],
            target: 0,
            steer: None,
            rng: MsvcRand::new(12345),
            hand: [100, 101, -1, -1],
            threat: false,
            scan_pos: 0,
            log: Vec::new(),
            presses: Vec::new(),
            unknown: Default::default(),
        }
    }
    fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }
    /// End of the program tick: the body walks 4 u toward the persistent steer target (a stand-in
    /// for the movement the game would do from the pad block).
    fn end_tick(&mut self) {
        if let Some(t) = self.steer {
            let d = Self::dist(self.pos[0], t);
            if d > 5.0 {
                for k in 0..3 {
                    self.pos[0][k] += (t[k] - self.pos[0][k]) / d * 4.0;
                }
            }
        }
        self.tick += 1;
    }
}

impl Natives for Stub {
    fn call(&mut self, group: u8, func: u16, a: &[i64], _mem: &Memory) -> i64 {
        let v = |i: usize| (a[i] as usize).min(19);
        let ret: i64 = if group == 4 {
            match func {
                0 => {
                    self.regs[(a[0] as usize).min(47)] = a[1];
                    0
                }
                1 => self.regs[(a[0] as usize).min(47)],
                2 => {
                    self.vecs[v(0)] = self.vecs[v(1)];
                    0
                }
                16 => (Self::dist(self.vecs[v(0)], self.vecs[v(1)]) / 10.0) as i64,
                18 | 31 => 5,
                19 => {
                    self.steer = Some(self.vecs[v(0)]);
                    0
                }
                20 => 0,
                21 => {
                    for k in 0..3 {
                        self.vecs[v(0)][k] += a[1 + k] as f64 * 10.0;
                    }
                    0
                }
                23 => {
                    let (p, q) = (self.vecs[v(0)], self.vecs[v(1)]);
                    ((q[0] - p[0]).atan2(q[2] - p[2]).to_degrees()) as i64
                }
                24 | 53 => 1,
                28 => (self.vecs[v(0)][0] * a[1] as f64 / 10.0) as i64,
                29 => (self.vecs[v(0)][1] * a[1] as f64 / 10.0) as i64,
                30 => (self.vecs[v(0)][2] * a[1] as f64 / 10.0) as i64,
                32 => self.rng.below(a[0]),
                33 => {
                    let d = (a[0] - a[1]).rem_euclid(360);
                    if d >= 180 {
                        d - 360
                    } else {
                        d
                    }
                }
                37 => 10,
                48 => {
                    self.presses.push((self.tick, a[0], Self::dist(self.pos[0], self.pos[1])));
                    0
                }
                49 | 51 => 0,
                56 => 0,
                // path_step(): 0 = no path, 1 = following, 2 = arrived (within 5.5 u, Appendix A)
                60 => match self.steer {
                    None => 0,
                    Some(t) if Self::dist(self.pos[0], t) > 5.5 => 1,
                    Some(_) => 2,
                },
                82 | 85 => 0,
                96 => {
                    self.target = a[0];
                    0
                }
                97 => self.target,
                112 => 0,
                113 => {
                    let p = (a[0] as usize).min(3);
                    if p < 2 {
                        self.vecs[v(1)] = self.pos[p];
                    }
                    0
                }
                114 => {
                    let p = (a[0] as usize).min(3);
                    self.vecs[v(1)] = self.start[p];
                    0
                }
                115 => 0,
                117 => 1,
                118 => 20,
                119 | 120 => 40,
                121 => 0,
                122 => a[0], // team = player index: 0 vs 1
                123 => 5,
                124 => (a[0] < 2) as i64, // 1 = idle, 0 = absent
                125 => 0,
                134 => 751,
                135 => -1,
                144 => self.hand.get(a[1] as usize).copied().unwrap_or(-1),
                146 => 1,
                147 => 0,
                148 => (self.hand.get(a[1] as usize).copied().unwrap_or(-1) != -1) as i64,
                150 => (a[2] == 0) as i64,
                161 => skill(a[0]).1,
                162 => 0,
                165 => 2,
                166 | 168 | 169 => 0,
                175 => skill(a[0]).0,
                176 => {
                    self.scan_pos = 0;
                    0
                }
                177 => {
                    self.scan_pos += 1;
                    if self.threat && self.scan_pos == 1 {
                        0
                    } else {
                        -1
                    }
                }
                178 => 100,
                179 => {
                    self.vecs[v(1)] = [100.0, 0.0, 0.0];
                    0
                }
                186 => if self.threat { 12 } else { 0 },
                192 | 200 | 197 => -1,
                194 | 195 | 198 | 229 | 234 | 235 => 0,
                208 => self.tick,
                210 => 76,
                425 => 3,
                426 => 0,
                _ => {
                    self.unknown.insert((group, func));
                    0
                }
            }
        } else {
            self.unknown.insert((group, func));
            0
        };
        self.log.push(Call { tick: self.tick, func: if group == 4 { func } else { 1000 + func }, args: a.to_vec(), ret });
        ret
    }
}

fn load() -> Option<(Ssb, Vec<(usize, i64)>)> {
    let dir = com_dir();
    let path = dir.join("ai/eneprog006.ssb");
    if !path.exists() {
        eprintln!("no game_files: eneprog006 test skipped");
        return None;
    }
    let s = ssb::load(&path).unwrap();
    // difficulty row 0 = the partner row; registers v30..v38
    let epa = EnemyParams::parse(&std::fs::read(dir.join("param/enemyparam.epa")).unwrap()).unwrap();
    Some((s, epa.row(0).unwrap().registers().to_vec()))
}

/// Run `n` ticks; returns the stub (with its log) after asserting every tick ended normally.
fn run_ticks(n: i64, row: &[(usize, i64)], s: &Ssb) -> (Stub, Vm) {
    let mut vm = Vm::new(s);
    let mut host = Stub::new(row);
    for t in 0..n {
        let st = vm.tick(&mut host);
        assert_eq!(st, RunStatus::Finished, "tick {t}: the script must return from main every tick");
        host.end_tick();
    }
    (host, vm)
}

#[test]
fn all_sixteen_ai_scripts_parse_and_disassemble() {
    let dir = com_dir().join("ai");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("no game_files: ai scripts test skipped");
        return;
    };
    let mut n = 0;
    for e in rd {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "ssb") {
            let s = ssb::load(&p).unwrap();
            let d = s.disassemble();
            assert!(s.function("main").is_some(), "{}", p.display());
            // every reachable word decodes to a defined opcode
            for i in d.insns.values() {
                assert!(i.op() <= 0x1F && !matches!(i.op(), 0x0B | 0x0C), "{}: op {:02x} at {:04x}", p.display(), i.op(), i.off);
            }
            n += 1;
        }
    }
    assert_eq!(n, 16, "P0-AI.md lists 16 files under com/ai");
}

#[test]
fn eneprog006_structure_matches_the_audit() {
    let Some((s, _)) = load() else { return };
    // P0-AI 2.3: main at file offset 0x6B8C, 28 KB, 86 functions
    assert_eq!(s.data.len(), 28004);
    assert_eq!(s.function("main").unwrap().offset, 0x6B8C);
    assert_eq!(s.functions.len(), 1, "only main is exported");
    let d = s.disassemble();
    assert_eq!(d.functions.len(), 87, "main + the 86 local functions");
    // every call target is a function entry the disassembler also reached
    for i in d.insns.values().filter(|i| i.op() == pd_formats::ssb::op::CALL) {
        assert!(d.insns.contains_key(&i.rel_target()), "call at {:04x}", i.off);
    }
    // the dump names natives and has the program's functions
    let t = s.disassembly_text();
    for needle in ["main:", "PRESS", "set_target", "rand", "tbl"] {
        assert!(t.contains(needle), "{needle}");
    }
}

#[test]
fn eneprog006_runs_for_600_ticks_and_thinks_every_thirty() {
    let Some((s, row)) = load() else { return };
    let (host, vm) = run_ticks(600, &row, &s);
    assert!(!host.log.is_empty());
    let unknown: Vec<_> = host.unknown.iter().collect();
    assert!(unknown.is_empty(), "natives the stub does not answer: {unknown:?}");
    assert!(vm.steps > 0);

    if std::env::var_os("PD_SSB_LOG").is_some() {
        for c in &host.log {
            if (c.func == 0 && matches!(c.args[0], 0 | 1 | 8 | 9 | 20 | 41)) || matches!(c.func, 48 | 49 | 96) {
                eprintln!("t={:4} f{:<3} {:?} -> {}", c.tick, c.func, c.args, c.ret);
            }
        }
    }

    // the mode register v0: setvar(0, mode)
    let modes: Vec<(i64, i64)> = host.log.iter().filter(|c| c.func == 0 && c.args[0] == 0).map(|c| (c.tick, c.args[1])).collect();
    assert_eq!(modes.first().map(|m| m.1), Some(1), "tick 0: init sets mode 1 (script 0x620C: v0=1)");
    assert_eq!(modes.first().map(|m| m.0), Some(0));
    // P0-AI 2.3: the think timer v20 is reloaded from v31 (= 30, row 0) and the attack mode (v0 = 2) is
    // entered at most once per think, so entries are >= 30 ticks apart
    let entries: Vec<i64> = modes.iter().filter(|m| m.1 == 2).map(|m| m.0).collect();
    assert!(entries.len() >= 10 && entries.len() <= 20, "attack decisions in 600 ticks (88 % at each think): {entries:?}");
    for w in entries.windows(2) {
        assert!(w[1] - w[0] >= 30, "think interval: {entries:?}");
    }
    // v20 counts down by one per tick between thinks
    let v20: Vec<(i64, i64)> = host.log.iter().filter(|c| c.func == 0 && c.args[0] == 20).map(|c| (c.tick, c.args[1])).collect();
    assert!(v20.windows(2).all(|w| w[1].1 == w[0].1 - 1 || w[1].1 == 30 || w[0].1 == 30 && w[1].1 == 29), "v20 only counts down or reloads to 30");
    // the attack mode (2) is entered (mode 1 decision list -> attack), and an attack skill is pressed
    let presses = &host.presses;
    assert!(!presses.is_empty(), "the CPU presses a skill slot");
    assert!(presses.iter().all(|p| (0..4).contains(&p.1)), "slots are 0..3: {presses:?}");
    // an attack decision is followed by a press within the mode's 150-tick give-up window (script 0x6430:
    // the chosen skill is fired when in its band, else the CPU positions first)
    for e in &entries {
        if *e < 400 {
            assert!(presses.iter().any(|p| p.0 > *e && p.0 <= e + 150), "press after the decision at {e}: {presses:?}");
        }
    }
    // the target is set to the only enemy (player 1)
    assert!(host.log.iter().any(|c| c.func == 96 && c.args == vec![1]), "set_target(1)");
}

#[test]
fn a_far_enemy_is_approached_before_the_first_shot() {
    let Some((s, row)) = load() else { return };
    let mut vm = Vm::new(&s);
    let mut host = Stub::new(&row);
    host.pos[1] = [700.0, 0.0, 0.0]; // 70 script units away, outside the 5-30 band of a type 12 skill
    host.start[1] = [850.0, 0.0, 0.0];
    for _ in 0..400 {
        assert_eq!(vm.tick(&mut host), RunStatus::Finished);
        host.end_tick();
    }
    if std::env::var_os("PD_SSB_LOG").is_some() {
        for c in host.log.iter().filter(|c| c.tick < 150 && ((c.func == 0 && matches!(c.args[0], 0 | 1 | 8 | 9)) || matches!(c.func, 48 | 19 | 20 | 96))) {
            eprintln!("far t={:4} f{:<3} {:?} -> {}", c.tick, c.func, c.args, c.ret);
        }
    }
    assert!(host.log.iter().filter(|c| c.func == 19).count() >= 2, "MOVE_TO is used to close in");
    if std::env::var_os("PD_SSB_LOG").is_some() {
        eprintln!("far presses (tick, slot, dist u): {:?}", host.presses);
    }
    let first = host.presses.first().expect("it fires eventually");
    // Appendix B: fire band 5-30 script units for the type 12 skill; `dist` truncates world units / 10
    let su = |d: f64| (d / 10.0) as i64;
    assert!(first.0 > 60, "it needed time to walk 400 u: first press at tick {}", first.0);
    // and it never fired from outside the skill's band: slot 0 = type 12 (5-30), slot 1 = type 10 melee (0-5)
    for p in &host.presses {
        let ok = match p.1 {
            0 => (5..=30).contains(&su(p.2)),
            1 => (0..=5).contains(&su(p.2)),
            _ => false,
        };
        assert!(ok, "press at tick {} slot {} from {:.0} u", p.0, p.1, p.2);
    }
}

#[test]
fn eneprog006_is_deterministic_for_a_fixed_rng() {
    let Some((s, row)) = load() else { return };
    let (a, _) = run_ticks(300, &row, &s);
    let (b, _) = run_ticks(300, &row, &s);
    assert_eq!(a.log, b.log);
    // a different RNG seed changes the call sequence (the think roll, wander, slot start)
    let mut vm = Vm::new(&s);
    let mut host = Stub::new(&row);
    host.rng = MsvcRand::new(777);
    for _ in 0..300 {
        assert_eq!(vm.tick(&mut host), RunStatus::Finished);
        host.end_tick();
    }
    assert_ne!(a.log, host.log);
}

#[test]
fn an_incoming_projectile_switches_to_a_defence_response() {
    let Some((s, row)) = load() else { return };
    let mut vm = Vm::new(&s);
    let mut host = Stub::new(&row);
    // slot 2 holds a defence capsule skill (capsule type 2) so a guard can be chosen
    host.hand = [100, 101, 102, -1];
    for t in 0..200 {
        host.threat = (60..120).contains(&t);
        assert_eq!(vm.tick(&mut host), RunStatus::Finished);
        host.end_tick();
    }
    let modes: Vec<(i64, i64)> = host.log.iter().filter(|c| c.func == 0 && c.args[0] == 0).map(|c| (c.tick, c.args[1])).collect();
    // P0-AI 2.7 (script 0x1a74): the bullet check runs when TICK & 3 == the player index, finds the
    // projectile (obj186 = ticks to impact -> v18, its skill -> v15, its motion -> v16), arms the 10-tick
    // cooldown v21, rolls v33 (88 %) and enters BUL_DEFENSE (mode 5); with no guard capsule it dodges
    // (`path49`, because the stub's dodge record is 751, not 750), then returns to mode 1.
    let checks: Vec<i64> = host.log.iter().filter(|c| c.func == 0 && c.args == vec![21, 10]).map(|c| c.tick).collect();
    assert_eq!(checks.first(), Some(&60), "the first check is on the first multiple of 4 after the projectile appears");
    assert!(checks.iter().all(|t| t % 4 == 0), "{checks:?}");
    assert!(checks.windows(2).all(|w| w[1] - w[0] >= 10), "10-tick cooldown: {checks:?}");
    assert!(host.log.iter().any(|c| c.tick == 60 && c.func == 0 && c.args == vec![18, 12]), "v18 = time to impact");
    assert!(host.log.iter().any(|c| c.tick == 60 && c.func == 0 && c.args == vec![15, 100]), "v15 = the incoming skill");
    assert!(modes.contains(&(60, 5)), "mode 5 at the first check: {modes:?}");
    assert!(host.log.iter().any(|c| c.tick == 60 && c.func == 49), "the dodge request (path49)");
    // no projectile before tick 60 and none after 120: no check outside the window
    assert!(checks.iter().all(|t| (60..120).contains(t)), "{checks:?}");
}
