//! Shared state and delivery for the non-attack capsules: erase (purple),
//! status (green), special (yellow), environment (aqua).
//!
//! Delivery (who a skill reaches) is decided here from the table; what it
//! then *does* lives in one module per capsule colour (`erase.rs`,
//! `status.rs`, `special.rs`, `environment.rs`). Those modules only change
//! the state below and the public fields of `Combat` / `Fighter`; the sim
//! (combat.rs, sim.rs) already reads every field here. Deterministic, no Bevy.
//!
//! Delivery, from the table (FORMATS-SKILLS.md):
//! - s43 = 4 ("Self") or 3 (moves/teleports yourself) -> the caster;
//! - s43 = 1 ("Capsule") -> the field (the capsule you touch / all capsules);
//! - s43 = 5 -> an environment crystal;
//! - motion (s63) 4 with s64 = 2 -> everyone ("reduces everyone's ...");
//!   motion 4 otherwise -> the target, at once (Frighten);
//! - anything else travels like an attack (homing shell, Hole, Crawl, mine,
//!   short-range) and takes effect on the target if it lands. Defences stop it
//!   as usual (STR 0 is always blocked, never breaks a guard).

use crate::combat::SkillDef;
use pd_formats::skills::field;

/// Who a non-attack skill takes effect on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// The caster only ("Self").
    Caster,
    /// The other player (it landed on them, or it is instant).
    Target(usize),
    /// Both players ("everyone's ..." — includes the caster).
    Everyone,
    /// Capsules on the battlefield ("capsule you are touching", "all capsules").
    Field,
}

/// How a non-attack skill is delivered (the registry turns targeted ones into
/// ordinary shots / areas / dashes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Caster,
    Everyone,
    Field,
    /// At once on the other player, no projectile.
    TargetNow,
    Environment,
    /// Travels like an attack; takes effect where it lands.
    Travels,
}

pub fn delivery(def: &SkillDef) -> Delivery {
    let target_kind = def.raw[43];
    match (target_kind, def.motion()) {
        (5, _) => Delivery::Environment,
        (4 | 3, _) => Delivery::Caster,
        (1, _) => Delivery::Field,
        (_, 4) if def.param(0) == 2 => Delivery::Everyone,
        (_, 4) => Delivery::TargetNow,
        _ => Delivery::Travels,
    }
}

/// The attributes the guide says status skills change ("strength of attack,
/// speed of movement, homing, and aura recovery speed"). Amounts: attack in
/// STR points, the rest in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    Attack,
    Speed,
    Homing,
    AuraRate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifier {
    pub stat: Stat,
    pub amount: i32,
    /// Sim ticks left; None = until reset (Neutralization, Refresh...).
    pub ticks: Option<u16>,
}

/// A fighter's status (the sim reads all of it; 0 / empty = normal).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub mods: Vec<Modifier>,
    /// Can't move (Paralyze, Pressure, Entangle...).
    pub bound_ticks: u16,
    /// No one can lock on to you (Optical Camo).
    pub invisible_ticks: u16,
    /// Directional controls inverted (Brain Concussion).
    pub inverted_ticks: u16,
    /// Skill buttons scrambled (Dazzle): button k uses slot `scramble[k]`.
    pub scrambled_ticks: u16,
    pub scramble: [u8; 4],
    /// Attack damage does nothing (Void).
    pub immune_ticks: u16,
    /// One skill button does nothing (Frostbite: "Prevents target from using
    /// skill assigned to same button"): which slot, and for how long.
    pub locked_slot: u8,
    pub locked_ticks: u16,
    /// Held this high above the floor (Lift Up, Skywalk, Levitate).
    pub hover_ticks: u16,
    pub hover_height: f32,
    /// Regenerate: +1 Health every this many sim ticks while standing still (0 = off).
    pub regen_every: u16,
    /// Invulnerability: no damage while standing still.
    pub still_immune: bool,
    /// Sim ticks standing still (kept by the sim).
    pub still_ticks: u16,
}

impl Stat {
    /// The most a status can reach either way (the table at `0x1402dcf30`,
    /// read from `PDUWP.exe`: attack 9, speed / homing / Aura rate 90;
    /// `FUN_14024d5c0` clamps a permanent value to it and `FUN_14024d4d0`
    /// clamps the sum a reader sees).
    pub fn cap(self) -> i32 {
        match self {
            Stat::Attack => 9,
            Stat::Speed | Stat::Homing | Stat::AuraRate => 90,
        }
    }
}

impl Status {
    /// The value a reader sees for `stat`: the permanent value plus the timed
    /// amount, kept within the stat's cap (`FUN_14024d4d0`).
    #[allow(dead_code)] // the sim reads `raw_total` + the environment, then caps (`Combat::stat`)
    pub fn total(&self, stat: Stat) -> i32 {
        self.raw_total(stat).clamp(-stat.cap(), stat.cap())
    }

    /// The same sum before the cap, for callers that add the environment's
    /// part first (`Combat::stat`).
    pub fn raw_total(&self, stat: Stat) -> i32 {
        self.mods.iter().filter(|m| m.stat == stat).map(|m| m.amount).sum()
    }

    /// Applies a status change the way the program's slots hold it
    /// (`FUN_14024d5c0`): a status has one permanent value and ONE timed
    /// amount with its timer. A change with no duration (`ticks` None) is added
    /// to the permanent value and clamped to the cap; a timed change REPLACES
    /// the timed amount and restarts its timer (so the same timed buff twice is
    /// not twice the buff).
    pub fn add(&mut self, stat: Stat, amount: i32, ticks: Option<u16>) {
        match ticks {
            None => match self.mods.iter_mut().find(|m| m.stat == stat && m.ticks.is_none()) {
                Some(m) => m.amount = (m.amount + amount).clamp(-stat.cap(), stat.cap()),
                None => self.mods.push(Modifier { stat, amount: amount.clamp(-stat.cap(), stat.cap()), ticks: None }),
            },
            Some(_) => match self.mods.iter_mut().find(|m| m.stat == stat && m.ticks.is_some()) {
                Some(m) => {
                    m.amount = amount;
                    m.ticks = ticks;
                }
                None => self.mods.push(Modifier { stat, amount, ticks }),
            },
        }
    }

    /// One sim tick: timers run down, timed modifiers expire.
    pub fn tick(&mut self) {
        for t in [&mut self.bound_ticks, &mut self.invisible_ticks, &mut self.inverted_ticks, &mut self.scrambled_ticks, &mut self.immune_ticks, &mut self.hover_ticks, &mut self.locked_ticks] {
            *t = t.saturating_sub(1);
        }
        for m in &mut self.mods {
            if let Some(t) = &mut m.ticks {
                *t = t.saturating_sub(1);
            }
        }
        self.mods.retain(|m| m.ticks != Some(0));
    }

    /// Back to normal ("Resets ... status and attributes to their initial state").
    pub fn reset(&mut self) {
        let still = self.still_ticks;
        *self = Status { still_ticks: still, ..Status::default() };
    }
}

/// An environment skill set on the battlefield: "an environmental crystal
/// appears at one's feet ... Only one crystal at a time is effective".
#[derive(Clone, Debug, PartialEq)]
pub struct Crystal {
    pub id: u32,
    pub owner: usize,
    pub skill: i16,
    pub pos: glam::Vec3,
    /// Sim ticks since it was set.
    pub age: u32,
}

/// "they cannot be captured until 15 seconds have passed since the
/// Environmental skill was set" (guide).
pub const CRYSTAL_CAPTURE_AFTER: u32 = 15 * 60;

impl SkillDef {
    /// School (s11): 1 Psycho, 2 Optical, 3 Nature, 4 Ki, 5 Faith.
    pub fn school(&self) -> i16 {
        self.raw[field::SCHOOL]
    }
}
