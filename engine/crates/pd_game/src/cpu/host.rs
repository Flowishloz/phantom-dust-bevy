//! The CPU host: one script VM per CPU seat, run at the program's 30 Hz against the real sim, writing the same `Input` a human pad
//! produces (FORMATS-CPU.md section 3; P0-AI.md section 2.2).
//!
//! Per program tick (every 2nd sim tick, after the countdown), per CPU seat, in the program's order (`FUN_14024ec80` ->
//! `FUN_1401775b0` -> `FUN_14017b220`):
//! 1. clear the pad block, `COM+0x38 = 1.0`, the dodge request off;
//! 2. run the script (`Vm::tick`: re-enter `main`, budget 50,000);
//! 3. the lock block: the script's target goes to `Player::target` (`P+0x13f20`; casts and homing read it through `aim_at`, with
//!    `Player::cpu_aim` standing for "lock block set, lock flag not": D5);
//! 4. steering: `Com::after_script` -> the pad output (stick magnitude 1.0 or 0, an absolute yaw, the dodge button);
//! 5. the pad becomes an `Input` (the stick re-expressed in the body's camera frame through the program's dead zone, so the sim sees
//!    the absolute heading exactly; skill presses only in the states the press handler acts in).
//!
//! The stick is held on the two sim ticks of a program tick; presses and the dodge are held on the first one only (one program tick =
//! one press, like the program's pressed-this-tick mask).

use super::nav::{BodyCtx, Cad, PadOut, Rays, YAW_UNITS_PER_RAD};
use super::natives::{CallRec, Effect, Env, PairTable, SeatCore};
use super::world::{self, SkillTable};
use crate::combat::MAX_PLAYERS;
use crate::sim::{Input, Match, STICK_AXIS_DEAD_ZONE};
use glam::Vec3;
use pd_formats::collision::Collision;
use pd_formats::epa::EnemyParams;
use pd_formats::ssb::Ssb;
use pd_formats::ssb_vm::{RunStatus, Vm};
use std::collections::BTreeSet;

/// The stage ray as the steering code's `Rays`.
pub struct StageRays<'a>(pub &'a Collision);
impl Rays for StageRays<'_> {
    fn clear(&self, a: Vec3, b: Vec3) -> bool {
        world::ray_clear(self.0, a, b)
    }
}

/// `eneprog006` and the difficulty rows: what a CPU seat is built from.
#[derive(Clone)]
pub struct CpuData {
    pub ssb: Ssb,
    pub epa: EnemyParams,
}

/// One CPU seat.
pub struct CpuSeat {
    pub core: SeatCore,
    vm: Vm,
    pub row: u8,
    /// The held stick, in the sim's quantised camera frame, and the one-shot buttons of the current program tick.
    stick: [i8; 2],
    skills: u8,
    jump: bool,
    /// Set when the script faulted (the seat stands still from then on).
    pub faulted: bool,
    /// The last program tick's pad output (for logs and tests).
    pub last_pad: PadOut,
    /// Round number the seat was last (re)built for.
    round: u32,
}

pub struct CpuHost {
    data: CpuData,
    pub seats: Vec<CpuSeat>,
    pub cad: Option<Cad>,
    pub tab: SkillTable,
    pub pairs: PairTable,
    pub stage_code: i32,
    pub effects_applied: Vec<Effect>,
    pub unknown: BTreeSet<(u8, u16)>,
    /// Native-call recording (tests and the cross-check): every call of every program tick.
    pub calls: Option<Vec<CallRec>>,
    /// Per-tick text log lines (`PD_CPU_LOG`).
    pub lines: Vec<String>,
    pub log_lines: bool,
    seed: u32,
}

/// The stick (camera frame, quantised) that makes the sim's movement read the absolute heading `yaw` (radians, our space) at full
/// push: the sim sends the pad through `process_axis` and then `right * x + fwd * y` with `fwd = (sin cam, cos cam)`,
/// `right = (-cos cam, sin cam)`, so the processed axes must be `(-sin(yaw - cam), cos(yaw - cam))` and the raw ones their inverse
/// through the dead zone. `None` for magnitude 0.
pub fn stick_for_heading(yaw: f32, cam: f32) -> [i8; 2] {
    let rel = yaw - cam;
    let (x, y) = (-rel.sin(), rel.cos());
    let raw = |v: f32| -> i8 {
        if v.abs() < 1e-3 {
            return 0;
        }
        let a = v.abs() * (1.0 - STICK_AXIS_DEAD_ZONE) + STICK_AXIS_DEAD_ZONE;
        (v.signum() * a * 127.0).round() as i8
    };
    [raw(x), raw(y)]
}

impl CpuSeat {
    fn new(slot: usize, row: u8, m: &Match, data: &CpuData, seed: u32) -> CpuSeat {
        let mut core = SeatCore::new(slot, m.players[slot].pos, seed ^ (slot as u32 + 1).wrapping_mul(0x9E37_79B1));
        // `FUN_14017b020` / `FUN_14017b100`: copy the row's nine signed bytes into v30..v38 (rows past the file are zero).
        if let Some(r) = data.epa.row(row as usize) {
            for (reg, v) in r.registers() {
                core.regs[reg] = v as i32;
            }
        }
        CpuSeat { core, vm: Vm::new(&data.ssb), row, stick: [0; 2], skills: 0, jump: false, faulted: false, last_pad: PadOut::default(), round: m.combat.round }
    }
}

impl CpuHost {
    /// `cad`: the stage's nav graph (None = the seats are inert, as when the program found no `comdata*.cad`). `col` is the stage
    /// collision (the floor test of `FUN_140176710`).
    pub fn new(data: CpuData, mut cad: Option<Cad>, m: &Match, col: &Collision, stage_code: i32) -> CpuHost {
        if let Some(c) = &mut cad {
            c.block_floorless(&|a, b| col.raycast(a, (b - a).normalize_or_zero(), a.distance(b)).is_some());
        }
        CpuHost {
            data,
            seats: Vec::new(),
            cad,
            tab: SkillTable::from_combat(&m.combat),
            pairs: PairTable::new(),
            stage_code,
            effects_applied: Vec::new(),
            unknown: BTreeSet::new(),
            calls: None,
            lines: Vec::new(),
            log_lines: false,
            seed: m.combat.rng_state(),
        }
    }

    /// Make slot `slot` a CPU seat running difficulty row `row` (a seat built with the human flag 0, an AI info struct and an `.epa`
    /// row: FORMATS-CPU 6).
    pub fn add_seat(&mut self, m: &Match, slot: usize, row: u8) {
        self.seats.retain(|s| s.core.slot != slot);
        self.seats.push(CpuSeat::new(slot, row, m, &self.data, self.seed));
    }

    #[allow(dead_code)] // for the setup screen (lane D)
    pub fn is_cpu(&self, slot: usize) -> bool {
        self.seats.iter().any(|s| s.core.slot == slot)
    }

    /// One sim tick: fill the CPU seats' entries of `inputs` (humans' entries are untouched). Call it right before `Match::step`.
    pub fn drive(&mut self, m: &mut Match, col: &Collision, inputs: &mut [Input]) {
        if self.seats.is_empty() {
            return;
        }
        let program_tick = m.tick % 2 == 0;
        // The nav doors and the pair table advance once per program tick for the whole match (`FUN_140176980`, `FUN_14012a900`).
        let running = !m.combat.counting_down() && m.combat.round_over.is_none();
        if program_tick && running {
            self.pairs.update(m, col);
            if let Some(c) = &mut self.cad {
                c.update_doors(&|a, b| world::ray_clear(col, a, b));
            }
        }
        for si in 0..self.seats.len() {
            let slot = self.seats[si].core.slot;
            // A new round puts everyone back on the start points: the program re-creates its fighters (FUN_140128cc0 per battle),
            // so the script's memory starts fresh. UNCONFIRMED for versus rounds (no CPU seat exists there in the program).
            if self.seats[si].round != m.combat.round {
                let row = self.seats[si].row;
                let rng = self.seats[si].core.rng.0;
                self.seats[si] = CpuSeat::new(slot, row, m, &self.data, rng);
            }
            let p = &mut m.players[slot];
            p.locked = false;
            p.cpu_aim = true;
            let seat = &mut self.seats[si];
            if program_tick {
                seat.skills = 0;
                seat.jump = false;
            }
            if program_tick && running && self.cad.is_some() && !seat.faulted && m.combat.fighters[slot].health > 0 {
                self.tick_seat(si, m, col);
            } else if program_tick {
                self.seats[si].skills = 0;
            }
            let seat = &mut self.seats[si];
            let first_sim_tick = program_tick;
            let mut inp = Input { stick: seat.stick, ..Input::default() };
            if first_sim_tick {
                inp.skills = seat.skills;
                inp.jump = seat.jump;
            }
            if let Some(slot_in) = inputs.get_mut(slot) {
                *slot_in = inp;
            }
        }
    }

    /// One program tick of one seat (steps 1-5 of the module docs).
    fn tick_seat(&mut self, si: usize, m: &mut Match, col: &Collision) {
        let slot = self.seats[si].core.slot;
        let tick30 = (m.tick / 2) as i32;
        let objs = world::objects(m, &self.tab);
        let mut effects = Vec::new();
        let status;
        {
            let seat = &mut self.seats[si];
            seat.core.pressed = 0;
            seat.core.com.dodge_req = false;
            seat.core.com.mag = 1.0;
            let mut env = Env {
                m,
                col,
                core: &mut seat.core,
                cad: self.cad.as_ref(),
                tab: &self.tab,
                pairs: &self.pairs,
                objs: &objs,
                tick30,
                stage_code: self.stage_code,
                effects: &mut effects,
                log: self.calls.as_mut(),
                unknown: &mut self.unknown,
            };
            status = seat.vm.tick(&mut env);
            if status == RunStatus::Faulted {
                seat.faulted = true;
            }
        }
        for e in &effects {
            match *e {
                Effect::Health(p, x) => m.combat.change_health(p, x),
                Effect::Respawn(p) => {
                    let pl = &mut m.players[p];
                    pl.pos = pl.spawn;
                    pl.vel = Vec3::ZERO;
                    pl.prev_pos = pl.pos;
                }
            }
        }
        self.effects_applied.extend(effects);
        // 3. The lock block: the script's target, when it names a live player (`FUN_14017b220` tail).
        let target = self.seats[si].core.target;
        m.players[slot].target = (0..MAX_PLAYERS as i32).contains(&target).then_some(target as usize).filter(|&t| t != slot && world::present(m, t) && m.combat.fighters[t].health > 0);
        // 4. Steering and the pad output.
        let body = body_ctx(m, slot);
        let cad = self.cad.as_ref().expect("checked by drive");
        let pad = self.seats[si].core.com.after_script(cad, &body, &StageRays(col));
        // 5. The pad as an `Input`.
        let seat = &mut self.seats[si];
        seat.last_pad = pad;
        let cam = m.players[slot].cam_yaw;
        seat.stick = if pad.mag > 0.0 { stick_for_heading(pad.yaw_units as f32 / YAW_UNITS_PER_RAD, cam) } else { [0, 0] };
        // The press handler (`FUN_1402517c0`) takes slot presses and the dodge only in states 0, 1, 8: elsewhere they are dropped.
        let acts = body.dodge_state_ok;
        seat.skills = if acts { seat.core.pressed } else { 0 };
        seat.jump = false;
        if let Some(yaw_units) = pad.dodge {
            // The dodge turns the body to the request angle first (`P+0x12e68 = COM+0x44`) and presses the dodge button.
            m.players[slot].yaw = yaw_units as f32 / YAW_UNITS_PER_RAD;
            seat.jump = true;
        }
        if self.log_lines {
            let me = &m.players[slot];
            let tgt = target;
            let dist = if (0..4).contains(&tgt) { m.players[tgt as usize].pos.distance(me.pos) } else { -1.0 };
            let line = format!(
                "[cpu] t {} s{} mode {} style {} pos ({:.0},{:.0},{:.0}) tgt {} dist {:.0} pad mag {:.1} yaw {} press {:#06b} dodge {:?} stick {:?} los {} node {} steer ({:.0},{:.0},{:.0}) code {} hp {} aura {} v6-10 {:?} v41 {} hand {:?}",
                m.tick, slot, seat.core.regs[0], seat.core.regs[1], me.pos.x, me.pos.y, me.pos.z, tgt, dist, pad.mag, pad.yaw_units, seat.core.pressed, pad.dodge, seat.stick, seat.core.com.los, seat.core.com.cur_node,
                seat.core.com.steer.x, seat.core.com.steer.y, seat.core.com.steer.z, seat.core.com.code, m.combat.fighters[slot].health, m.combat.fighters[slot].aura(), &seat.core.regs[6..=10], seat.core.regs[41], m.combat.fighters[slot].slots
            );
            self.lines.push(line);
        }
    }
}

/// What the steering code reads of the body (`P+0x12e74`, `P+0x12f04`, `P+0x12e84`), from the sim's player.
pub fn body_ctx(m: &Match, slot: usize) -> BodyCtx {
    use crate::combat::Action;
    use crate::sim::BodyState;
    let (p, f) = (&m.players[slot], &m.combat.fighters[slot]);
    let ready = f.action == Action::Ready;
    let ground = p.body_state() == BodyState::Ground && ready;
    BodyCtx {
        pos: p.pos,
        vy_tick: p.vel.y / 30.0,
        airborne: !p.grounded,
        standing_or_running: ground,
        dodge_state_ok: ground,
        state4: p.jump_windup() > 0,
        unlock_state: ground || (p.grounded && p.land_timer > 0.0),
        state5: !p.grounded,
    }
}
