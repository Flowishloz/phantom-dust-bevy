//! Defence skills: every kind in s44 (FORMATS-SKILLS.md).
//!
//! Guide quotes (the spec):
//! - Barrier: "in front of the Esper as they intercept attacks ... they don't
//!   provide full body coverage ... leaving the back unprotected".
//! - Reflect: "similar to the Barrier types ... Attacks that are successfully
//!   defended will be reflected back to the attacker as long as the Attack
//!   strength is not more powerful than the Defense ... only successful ...
//!   against some attacks."
//! - Shelter: "360 degrees of protection against most attacks."
//! - Absorb: "similar to Barrier types, in that protection lies in front of the
//!   Esper; however, they are ineffective against close-proximity attacks."
//! - Orbit Changing ([Course]): "changing the trajectory of a blocked attack ...
//!   most effective against Shell types and are ineffective against
//!   close-proximity attacks and Rain-type skills ... do not have a DEF rating,
//!   but they are strong and effective against any attack" (other than those).
//! - Orbit Reversing (About Face, Reverse): "with the additional effect of
//!   reversing the attack back to the original attacker."
//! - Brush: "protect against attacks in front of the Esper. When Brush types
//!   are effectively used as defense against close-proximity attacks, they will
//!   for an instant paralyze the opponent."
//! - Automatic: "automatically react and take effect based on the opponent's
//!   attacks. If there isn't enough Aura, however, to cover their Cost, such
//!   skills will not take effect."
//! - STR vs DEF: STR > DEF: attack has no effect, defence destroyed; STR = DEF
//!   or STR < DEF: attack has no effect. Pierce gets STR - DEF through;
//!   Anti-Pierce "protects from damage regardless of STR".

use super::{table_ticks, TABLE_DIST};
use crate::combat::SkillDef;
use glam::Vec3;
use pd_formats::skills::field;

/// Defence kind (s44).
pub mod kind {
    pub const BARRIER: i16 = 0;
    pub const SHELTER: i16 = 1;
    pub const COURSE: i16 = 2;
    pub const BRUSH: i16 = 3;
    pub const REFLECT: i16 = 4;
    pub const ABSORB: i16 = 5;
    pub const REVERSE: i16 = 6;
    pub const AUTO_SHELTER: i16 = 7;
}

/// Extra effects of defence skills (s51) when they block.
pub mod add {
    pub const HEALTH_FROM_DAMAGE: i16 = 1;
    pub const AURA_FROM_COST: i16 = 2;
    pub const ERASE_ATTACK: i16 = 3;
    pub const PARALYSE: i16 = 4;
    pub const FREEZE: i16 = 5;
    /// Status 6 on the attacker (poison: each button press costs 1 Health),
    /// duration s52 (`FUN_140271ed0` case 6; no skill in the roster carries it).
    pub const POISON: i16 = 6;
    /// The blocked attack skill lands on the guard's own button
    /// (`FUN_140261a00(guard slot, attack skill)`; only on a block that
    /// leaves the guard standing).
    pub const CAPTURE: i16 = 7;
}

/// Used when a defence has no time in the table.
pub const DEFAULT_TICKS: u16 = 60;
/// A Brush block stuns a close-range attacker this long: reaction 26
/// (DMG_SPHERE_FRONT/BACK, 30 program frames = 1.0 s; `FUN_140271760` ->
/// `FUN_14026cd70` code 7 -> `FUN_140258690(.., 0x1a, ..)`).
pub const BRUSH_STUN_TICKS: u16 = 60;
/// A guard that has blocked something ends this many sim ticks later: the
/// first block calls `FUN_14026cd70(guard, 9)`, which puts the object in state
/// 6 with a 5-program-tick life (`+0x18 = 5`); the DEF it lost stays lost and
/// further blocks inside the window keep draining it (Octolaser's STR-1 shots
/// break a DEF-3 wall on the 4th).
pub const LINGER_TICKS: u16 = 10;

/// How an attack arrives, for the rules above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// A short-range strike (close proximity).
    Melee,
    /// A projectile.
    Shot,
    /// A set area going off (Hole, Rain, mines, Photon Wave).
    Area,
    /// The growing sphere of an exploding attack: the program's object state 3
    /// (`FUN_14026dbd0`), whether it came from a shot (Bomb, Bomb Shot...) or a
    /// mine (`Combat::tick_blasts`).
    Blast,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guard {
    pub kind: i16,
    pub def: i32,
    /// Sim ticks it stays up (s46 on defence skills, 1/30 s: Barrier 23/28,
    /// Brush 18, automatic Shelter 10). Unconfirmed.
    pub ticks: u16,
    pub anti_pierce: bool,
    /// Orbit changing/reversing: shots are turned within this distance
    /// (s45: 800 on Course skills -> 80 u, 130 on the rest). Unconfirmed.
    pub field: f32,
    /// Extra effect on a block (`add::*`) and its amount/time (table ticks).
    pub add: i16,
    pub add_amount: i16,
    /// "Erases any short-range attack skill it blocks" (s50 = 1) vs any attack.
    pub short_range_only: bool,
}

impl Guard {
    pub fn from_def(def: &SkillDef) -> Option<Self> {
        let k = def.raw[field::DEFENCE_KIND];
        // Only a defence record has a guard kind in s44 (a special's s44 is its
        // own kind: Surprise Attack's 7 is not an automatic Shelter).
        if def.capsule != crate::combat::Capsule::Defence || !(kind::BARRIER..=kind::AUTO_SHELTER).contains(&k) {
            return None;
        }
        // The object's countdown starts at s46 - 2 (`0x140264020`) and ends
        // the update it is found at 0: it stands for s46 - 1 program ticks
        // (Crystal Wall 23 -> 22 ticks = 0.73 s), not s46.
        let ticks = table_ticks((def.raw[field::STRENGTH] - 1).max(0));
        Some(Guard {
            kind: k,
            def: def.defence,
            ticks: if ticks > 0 { ticks } else { DEFAULT_TICKS },
            anti_pierce: def.raw[field::ANTI_PIERCE] == 1,
            field: def.raw[field::STRENGTH_RULE].max(0) as f32 * TABLE_DIST,
            add: def.raw[field::ADD_EFFECT],
            add_amount: def.raw[field::ADD_AMOUNT],
            short_range_only: def.raw[field::SHORT_RANGE_ONLY] == 1,
        })
    }

    /// DEF = s47 + s48 x (times this button has been used before this use)
    /// (`FUN_140272400`; Failing Wall 7 - uses, Dark Protection 3 + uses).
    pub fn with_uses(mut self, def: &SkillDef, uses_before: u8) -> Self {
        self.def = def.defence + def.raw[48] as i32 * uses_before as i32;
        self
    }

    /// Covers every direction: Shelter, Absorb and the automatic Shelter are a
    /// sphere around the holder in the contact test; Barrier, Brush and
    /// Reflect are a disc in front (`FUN_14026dcb0`: cases 0/3/4 against the
    /// default). (Course and About Face are spheres too, but turn shots
    /// before they arrive: `Combat::move_projectiles`.)
    pub fn all_round(&self) -> bool {
        matches!(self.kind, kind::SHELTER | kind::AUTO_SHELTER | kind::ABSORB)
    }

    /// Turns shots aside or back before they arrive (Course / Reverse).
    pub fn turns_shots(&self) -> bool {
        matches!(self.kind, kind::COURSE | kind::REVERSE)
    }

    /// Only works when triggered by an attack, not cast (automatic Shelter).
    pub fn automatic(&self) -> bool {
        self.kind == kind::AUTO_SHELTER
    }
}

/// What a block does to the attack that was stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockEffect {
    /// The attack is erased (`FUN_14026cd70` code 3).
    Stop,
    /// A Reflect sends a shell or arc back at its owner (code 5).
    Reflect,
    /// A Brush: a close-range attacker is stunned (reaction 26), a projectile
    /// is turned aside and loses its target (code 7).
    Brush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The guard didn't cover it; normal damage.
    Through,
    /// Stopped (the caller drains the guard's DEF by the attack's STR and runs
    /// its lifecycle: `Combat::strike`). `broken`: STR > DEF destroyed the
    /// defence. `damage`: what a Pierce attack still does.
    Blocked { broken: bool, damage: i32, effect: BlockEffect },
    /// Orbit changing: the shot was turned aside (Course) or back (Reverse).
    Turned { back: bool },
}

/// Is `from` in front of someone at `pos` facing `yaw`?
pub fn in_front(pos: Vec3, yaw: f32, from: Vec3) -> bool {
    let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    facing.dot(Vec3::new(from.x - pos.x, 0.0, from.z - pos.z)) >= 0.0
}

/// Close proximity as the guard contact function reads it (`FUN_140167910`:
/// attack-object motions 3, 12, 13, 14).
pub fn is_short_range(attack: &SkillDef) -> bool {
    attack.raw[43] == 0 && matches!(attack.motion(), 3 | 12 | 13 | 14)
}

/// Attacks a guard kind does not stop at all, the attack carries on
/// (`FUN_140271760`, before any DEF is spent):
/// - Barrier, Reflect: Rain (motions 7 and 20) and Tornado (10);
/// - Absorb: all short-range attacks, blasts, Rain, Tornado, mines (8) and
///   around-self (9);
/// - Brush: blasts, Rain, Tornado, mines and around-self.
/// `blast` = the growing sphere of an "explodes on impact" attack (the
/// program's object state 3, tested ahead of the motion list for Absorb and
/// Brush): `Arrival::Blast`, whatever put the sphere there, a shot or a mine
/// (`FUN_140271760`, READ in the audit's BLAST3 lane).
fn never_stops(guard_kind: i16, attack: &SkillDef, arrival: Arrival) -> bool {
    if attack.raw[43] != 0 {
        return false;
    }
    let m = attack.motion();
    let area = matches!(m, 7 | 10 | 20);
    let blast = arrival == Arrival::Blast;
    match guard_kind {
        kind::BARRIER | kind::REFLECT => area,
        kind::ABSORB => area || blast || is_short_range(attack) || matches!(m, 8 | 9),
        kind::BRUSH => area || blast || matches!(m, 8 | 9),
        _ => false,
    }
}

/// What `guard` does to an attack of `strength` (`FUN_140271760`, in the
/// program's order). `from_front`: it arrives in front of the holder (false
/// when it has no direction, e.g. Rain from above). `necronomicon`: status 31,
/// which cancels "penetrates defensive skills". On `Blocked` the caller
/// drains the guard's DEF by `strength` and runs its lifecycle.
pub fn resolve(guard: &Guard, strength: i32, attack: &SkillDef, arrival: Arrival, from_front: bool, necronomicon: bool) -> Outcome {
    let object = attack.raw[43] == 0;
    // s54 = 2 "Penetrates defensive skills" passes Barrier, Shelter, Absorb,
    // Brush and Reflect (not Course / About Face) unless status 31 is on.
    if object && attack.ignores_defence() && !necronomicon && !matches!(guard.kind, kind::COURSE | kind::REVERSE) {
        return Outcome::Through;
    }
    match guard.kind {
        // Orbit changing: shots only; no DEF, so never broken.
        kind::COURSE => match arrival {
            Arrival::Shot => Outcome::Turned { back: false },
            _ => Outcome::Through,
        },
        // About Face / Reverse send shells (0), lobs (1), falls (2), crawlers
        // (5), objects (11) and arcs (6) back at their owner; everything else
        // passes. No DEF is spent.
        kind::REVERSE => match arrival {
            Arrival::Shot if object && matches!(attack.motion(), 0 | 1 | 2 | 5 | 6 | 11) => Outcome::Turned { back: true },
            _ => Outcome::Through,
        },
        _ => {
            if !(guard.all_round() || from_front) || never_stops(guard.kind, attack, arrival) {
                return Outcome::Through;
            }
            let brush = guard.kind == kind::BRUSH;
            if strength > guard.def {
                // "STR > DEF: the defence is destroyed"; a Pierce attack carries
                // on with STR - DEF unless the wall is Anti-Pierce.
                let damage = if attack.pierces() && !guard.anti_pierce { strength - guard.def } else { 0 };
                Outcome::Blocked { broken: true, damage, effect: if brush { BlockEffect::Brush } else { BlockEffect::Stop } }
            } else {
                let effect = match guard.kind {
                    // A blast (state 3) is stopped but never sent back: the Reflect
                    // branch of `FUN_140271760` needs an attack that is not in state 3.
                    kind::REFLECT if object && arrival != Arrival::Blast && matches!(attack.motion(), 0 | 6) => BlockEffect::Reflect,
                    kind::BRUSH => BlockEffect::Brush,
                    _ => BlockEffect::Stop,
                };
                Outcome::Blocked { broken: false, damage: 0, effect }
            }
        }
    }
}
