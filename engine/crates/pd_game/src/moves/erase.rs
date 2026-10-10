//! Erase skills (purple capsules). Owned by the erase lane: only this
//! file changes. Contract: docs/MOVES.md "Capsule colours" and moves/effects.rs.
//!
//! `apply` runs when one of these skills takes effect: `reach` says on whom
//! (`Reach::Caster` for "your ...", `Target(i)` when it landed on player i,
//! `Everyone`, `Field` for capsules). Change `c.fighters[..]` (health, level,
//! `aura_ticks`, `slots`, `arsenal`, `used`, `status`, `frozen_ticks`,
//! `poison_ticks`, `capsule_anchor`), `c.capsules`, `c.crystal`, and `players`
//! (positions) as the skill says. Return false for skills not built yet.
//! Deterministic: no clocks, no HashMap iteration; random picks use
//! `c.random(n)`.
//!
//! ## How the 44 erase skills' table fields decode (2026-09-26, from erase_skills.json)
//!
//! Two independent sub-schemes share the erase capsule's s48-s52 fields:
//!
//! - **Attribute effects** (s51 = `ADD_EFFECT`, s52 = `ADD_AMOUNT`, same field
//!   names as attack/defence "Add:" effects, but erase-specific codes):
//!   - 1 = Aura to 0, amount 99 always ("Reduces everyone's Aura to 0", Fairy Ring)
//!   - 2 = reduce Level by s52 (Interference -2, Restrain -1, Cooling -1)
//!   - 4 = halve Level, s52 = 2 in every record we have (Exhaust, Bloody
//!     Ritual, Panic Gas, Quantum Decay: all "Reduces ... Level by 50%")
//!   - 10 = absorb all of the target's Aura into the caster, amount 99 (Steal Power)
//!   - 17 = reset status/attributes to normal (Neutralization: target; Purify: everyone)
//!   - 38 = erase the environment crystal (Starting Point, Judgment, Crush)
//!   - 39 = erase the crystal AND reset everyone's status (Return to Nature)
//!   - 47 = everyone's Level down to whichever player's is lowest (Vicious Balance)
//!   - 49 = Meltdown: Level -= s52 per the player's own Aura Particles standing
//!     on the battlefield (capsules with `skill == AURA_PARTICLE` and that
//!     player as owner)
//!   - special case: s51 = 2 with s52 = 99 on a [Hole] (motion 15/16) is
//!     Disintegrate, "[Hole] Reduces everyone's Level to 0" -- unlike every
//!     other s51=2 record it hits *everyone*, not just the player the Hole
//!     landed on, so it's handled before the normal per-`Reach` split below.
//!
//! - **Erase equipped skill(s)** (s51 = 0, s48/s49/s50 instead): s48 is which
//!   capsule colour(s) in the target's 4 slots are eligible, using the same
//!   numbering as the table's own `CAPSULE` field (FORMATS-SKILLS.md s9: 1
//!   attack, 2 defence) for 1/2, plus two erase-only codes: 8 = neither attack
//!   nor defence ("non-attack and non-defensive": Wind Pressure, Turbulence),
//!   9 = no filter, any slot (Giant Slayer, Lack, Hungry Demon, Disremember,
//!   Confusion Gas, Calamity). s50 is 1 (pick a single slot) or 4 (every
//!   matching slot -- 4 being all `SLOTS`); s49 then says *which* single slot
//!   when s50 = 1: 1 = highest Aura cost (Giant Slayer), 2 = lowest cost when
//!   there's no capsule filter (Disremember) or otherwise just "1 of ..." at
//!   random (Tiger Slayer, Dragon Slayer, Tiger Prevails) -- **unconfirmed**:
//!   Shield Breaker has s49 = 1 for the same "1 of ..." wording, so s49 = 1
//!   isn't reserved for "highest cost" once a capsule filter is set; we only
//!   trust it as "highest cost" when s48 = 9, matching Giant Slayer's own
//!   note; 4 = the slot the caster pressed to cast this skill ("same
//!   button", Hungry Demon). s49 = 3 (Lack) is "all + unlimited-use only";
//!   unlimited use = s26 = 0 (FORMATS-SKILLS.md, probable).
//!
//! Field (capsule-touching / battlefield) skills use s43 = 1 and are told
//! apart by s44/s45 instead (no s51 involved): s44 = 0 "erase the capsule
//! you're touching" (Abyss, Diabolical Trick), plus s45 = 1 on the same s44 =
//! 0 template turns it into a *capture* instead of an erase (Thieving Imp);
//! s44 = 1 restricts that touch to Aura Particles only (Eliminate); s44 = 2 is
//! battlefield-wide and Aura-Particle-only (Evaporate); s44 = 3 is the touch
//! version *excluding* Aura Particles (Shatter, "not Aura" per its own note);
//! s44 = 5 is battlefield-wide over every capsule, and s45 then picks erase
//! (0, Violent Change; 2, Change the World -- capsules reappear on their own
//! once the battlefield is emptied, since `spawn_capsule` in combat.rs spawns
//! whenever a player is under `MAX_CAPSULES_PER_PLAYER`, no timer reset
//! needed) or convert-to-Aura-Particle (3, Particle Convert).
//!
//! Orb (Special #180, "Automatically negates any Erase skill used. Can't be
//! erased or overwritten") and Amulet (Special #299, "Automatically reverses
//! effects of last Erase skill that hit you") live in the *target's* slots,
//! so this lane resolves them before anything else: Orb fully cancels an
//! erase effect aimed at its holder (and is itself never a candidate for
//! "erase equipped skill(s)"); Amulet is built as the simplest version that
//! matches its note -- the effect lands on the caster instead of the holder.
//! Neither ever blocks a caster's effect on themselves (Refresh, Angel
//! Tears...), since those aren't "used against" anyone.

use super::effects::Reach;
use super::motion;
use crate::combat::{in_pickup_reach, Capsule, CapsuleItem, Combat, CombatEvent, Fighter, SkillDef, AURA_PARTICLE, AURA_TICKS_PER_POINT, SLOTS};
use crate::sim::Player;
use pd_formats::skills::field;

/// Special skill #180: "Automatically negates any Erase skill used. Can't be
/// erased or overwritten."
const ORB: i16 = 180;
/// Special skill #299: "Automatically reverses effects of last Erase skill
/// that hit you." Simplified per the lane brief: the erase bounces to the
/// caster instead of the holder.
const AMULET: i16 = 299;

/// s48 on "erase equipped skill(s)" variants: which equipped skills are on the
/// list (`eligible`).
const ERASE_FILTER: usize = 48;
/// s49: the order the list is put in (`erase_equipped`).
const ERASE_PICK: usize = 49;
/// s50: how many of the list, from the front, are erased.
const ERASE_COUNT: usize = 50;

pub fn apply(c: &mut Combat, players: &mut [Player], caster: usize, reach: Reach, def: &SkillDef) -> bool {
    let add_effect = def.raw[field::ADD_EFFECT];
    let add_amount = def.raw[field::ADD_AMOUNT] as i32;

    // Disintegrate: "[Hole] Reduces everyone's Level to 0" -- hits everyone
    // even though the Hole itself only lands on one player. Handled up front
    // since every other s51=2 record only ever affects its own `reach`.
    if add_effect == 2 && add_amount == 99 && matches!(def.motion(), motion::HOLE_A | motion::HOLE_B) {
        for p in c.roster.slots() {
            if let Some(p) = resolve(c, caster, p) {
                c.fighters[p].level = 0;
                clamp_aura(&mut c.fighters[p]);
            }
        }
        return true;
    }

    // The erase Holes (Disintegrate, Calamity) both say "everyone's": the
    // Hole marks one player's feet but the effect reaches both.
    let reach = if matches!(def.motion(), motion::HOLE_A | motion::HOLE_B) && matches!(reach, Reach::Target(_)) { Reach::Everyone } else { reach };
    match reach {
        Reach::Field => field_effect(c, players, caster, def),
        Reach::Caster => {
            // Refresh / Angel Tears (s43 = 4, "Resets your status and
            // attributes to their initial state"). Angel Tears also "Add: 1
            // Level" via the skill's own `strength` field (s46); Refresh's is
            // 0, so the same line covers both.
            let f = &mut c.fighters[caster];
            reset_status(f);
            f.level = (f.level + def.strength).min(f.level_max);
            clamp_aura(f);
            true
        }
        Reach::Target(t) => match resolve(c, caster, t) {
            None => true, // Orb: negated.
            Some(t) => apply_attribute_or_erase(c, caster, &[t], add_effect, add_amount, def),
        },
        Reach::Everyone => {
            let mut targets: Vec<usize> = c.roster.slots().filter_map(|p| resolve(c, caster, p)).collect();
            // An Amulet bouncing an "everyone" erase back doesn't hit the caster twice.
            targets.dedup();
            apply_attribute_or_erase(c, caster, &targets, add_effect, add_amount, def)
        }
    }
}

/// Who an erase effect aimed at `victim` actually lands on, after Orb/Amulet
/// in their slots. `None` = fully negated (Orb). A caster is never blocked
/// from their own self-cast effect.
fn resolve(c: &Combat, caster: usize, victim: usize) -> Option<usize> {
    if victim == caster {
        return Some(victim);
    }
    if holds(&c.fighters[victim], ORB) {
        None
    } else if holds(&c.fighters[victim], AMULET) {
        Some(caster)
    } else {
        Some(victim)
    }
}

/// An attack's own erase ("Add: Erases 1 equipped defensive skill from
/// target" — Lightning Sword, Tiger Kick, Storm Blade, Piece of Wind…; "all
/// equipped defensive skills" — Thunder Storm) when it lands: the same
/// s48 / s49 / s50 fields as the erase skills. Orb and Amulet answer "Erase
/// skills", which these are not, so they don't step in (unconfirmed).
pub fn on_hit(c: &mut Combat, attacker: usize, victim: usize, def: &SkillDef) -> bool {
    def.raw[ERASE_FILTER] != 0 && erase_equipped(c, attacker, victim, def)
}

/// "Can't be erased or overwritten" (Orb, Seal of Death, Sign of Saints: s41 = 2).
fn is_protected(c: &Combat, id: i16) -> bool {
    id == ORB || c.skill(id).is_some_and(SkillDef::protected)
}

/// "Resets ... status and attributes to their initial state": the status
/// plus the freeze / poison timers the HUD shows beside it.
fn reset_status(f: &mut Fighter) {
    f.status.reset();
    f.frozen_ticks = 0;
    f.poison_ticks = 0;
    f.forced_lock = None;
    f.status_hover = false;
}

/// The reset the erase skills run, for tests in other modules.
#[cfg(test)]
pub fn reset_status_for_tests(f: &mut Fighter) {
    reset_status(f)
}

fn holds(f: &Fighter, id: i16) -> bool {
    f.slots.contains(&Some(id))
}

fn clamp_aura(f: &mut Fighter) {
    f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
}

/// The s51/s52 "attribute effect" family, or (s51 = 0) the "erase equipped
/// skill(s)" family -- see the module doc comment for the evidence.
fn apply_attribute_or_erase(c: &mut Combat, caster: usize, targets: &[usize], add_effect: i16, add_amount: i32, def: &SkillDef) -> bool {
    match add_effect {
        // Fairy Ring: "Reduces everyone's Aura to 0" (amount is always 99, "to 0").
        1 => {
            for &p in targets {
                c.fighters[p].aura_ticks = 0;
            }
        }
        // Interference (-2), Restrain (-1), Cooling (-1): "Reduces target's Level by N".
        2 => {
            for &p in targets {
                c.fighters[p].level = (c.fighters[p].level - add_amount).max(0);
                clamp_aura(&mut c.fighters[p]);
            }
        }
        // Exhaust, Bloody Ritual, Panic Gas, Quantum Decay: "... Level by 50%" (amount is always 2 in the data).
        4 => {
            for &p in targets {
                c.fighters[p].level /= 2;
                clamp_aura(&mut c.fighters[p]);
            }
        }
        // Steal Power: "Absorbs all target's Aura."
        10 => {
            for &p in targets {
                let amount = c.fighters[p].aura_ticks;
                c.fighters[p].aura_ticks = 0;
                let cap = c.fighters[caster].level * AURA_TICKS_PER_POINT;
                c.fighters[caster].aura_ticks = (c.fighters[caster].aura_ticks + amount).min(cap);
            }
        }
        // Neutralization (target), Purify (everyone): "Resets ... status and attributes to their initial state."
        17 => {
            for &p in targets {
                reset_status(&mut c.fighters[p]);
            }
        }
        // Starting Point, Judgment, Crush: "Erases Environmental crystals from battlefield."
        38 => cancel_environment(c),
        // Return to Nature: "Cancels Environmental effects and resets everyone's status and attributes ...".
        39 => {
            cancel_environment(c);
            for &p in targets {
                reset_status(&mut c.fighters[p]);
            }
        }
        // Vicious Balance: "Everyone's Level decreases to player's lowest Level."
        47 => {
            let lowest = c.roster.slots().map(|p| c.fighters[p].level).min().unwrap_or(0);
            for &p in targets {
                c.fighters[p].level = lowest;
                clamp_aura(&mut c.fighters[p]);
            }
        }
        // Meltdown: "Reduces everyone's Level by 2 for each of their Aura Particles on battlefield."
        // READ `FUN_140272bc0` cases 0x30 / 0x31: counts the victim's three capsule spots that are in
        // state 2 (present, not still appearing) and of kind 0 (Aura Particle); no particle = no effect.
        49 => {
            for &p in targets {
                let particles = c.capsules.iter().filter(|item| item.owner == p && item.skill == AURA_PARTICLE && item.appear == 0).count() as i32;
                c.fighters[p].level = (c.fighters[p].level - add_amount * particles).max(0);
                clamp_aura(&mut c.fighters[p]);
            }
        }
        // s51 = 0: "erase equipped skill(s)" -- decoded from s48/s49/s50 instead.
        _ => {
            // One common/91 per use when any button was handed to the erase (`0x14026fb8e`; LIVE:
            // Calamity / Turbulence, both players' buttons, log it once).
            let mut first = None;
            for &p in targets {
                if erase_equipped(c, caster, p, def) {
                    first.get_or_insert(p);
                }
            }
            if let Some(victim) = first {
                c.log(CombatEvent::ButtonErased { attacker: caster, victim, skill: def.id });
            }
        }
    }
    true
}

/// Starting Point / Judgment / Crush / Return to Nature: the standing crystal is
/// cancelled on the spot (`FUN_14017ba00`: expiry = now, effect 0x14 and common/94
/// at the crystal, `0x14017bb6d`). A natural timeout or a take plays no 94.
fn cancel_environment(c: &mut Combat) {
    if let Some(cr) = c.crystal.take() {
        c.log(CombatEvent::EnvironmentEnded { pos: cr.pos });
    }
}

/// The table's colour number (s9) of a capsule type: 1 attack, 2 defence,
/// 3 erase, 4 status, 5 special, 6 environment.
fn colour(capsule: Capsule) -> i16 {
    match capsule {
        Capsule::Attack => 1,
        Capsule::Defence => 2,
        Capsule::Erase => 3,
        Capsule::Status => 4,
        Capsule::Special => 5,
        Capsule::Environment => 6,
    }
}

/// s48 on "erase equipped skill(s)": which of the target's equipped skills
/// are on the list (`FUN_140272510`): 1-5 the capsule colour (s9), 6 the Aura
/// Particles, 7 everything but Aura Particles, 8 colours other than attack
/// and defence, 9 anything on a button.
fn eligible(c: &Combat, id: i16, filter: i16) -> bool {
    let capsule = if id == AURA_PARTICLE { None } else { c.skill(id).map(|d| d.capsule) };
    match filter {
        1..=5 => capsule.is_some_and(|k| colour(k) == filter),
        6 => id == AURA_PARTICLE,
        7 => id != AURA_PARTICLE,
        // READ `FUN_140272510` mode 8: `(type - 1) as u16 > 1`. The Aura Particle's record (program
        // record 0, file offset 0x18 of gsdata: s9 = 0) passes it, so it is on the list too.
        8 => id == AURA_PARTICLE || capsule.is_some_and(|k| !matches!(k, Capsule::Attack | Capsule::Defence)),
        9 => true,
        _ => false,
    }
}

/// Erases whatever sits on button `k` of player `p` the way the program does
/// (`FUN_140262fc0`). A "can't be erased" skill (s41 = 2: Orb, Seal of Death,
/// Sign of Saints) stays. A Retain skill (s41 = 1) with the Aura to cover it
/// is paid for again right now and stays (not an attack skill under
/// Necronomicon, status 31). Anything else leaves the button and is
/// "consumed" (`FUN_140263570`: Recall and Relearn can bring it back).
/// True if the button was emptied.
pub fn erase_slot(c: &mut Combat, p: usize, k: usize) -> bool {
    let Some(id) = c.fighters[p].slots[k] else { return false };
    if is_protected(c, id) {
        return false;
    }
    if let Some(def) = c.skill(id).cloned() {
        let necro = c.env_status(31) != 0;
        if def.retained() && !(def.capsule == Capsule::Attack && necro) {
            let base = def.cost + def.cost_per_use() * c.fighters[p].spent[k] as i32;
            let cost = c.env_def().map_or(base, |e| super::environment::cost(e, &def, base)).max(0);
            let f = &c.fighters[p];
            if f.aura_ticks >= cost * AURA_TICKS_PER_POINT && f.level >= def.level_penalty {
                c.fighters[p].aura_ticks -= cost * AURA_TICKS_PER_POINT;
                c.change_health(p, -def.health_cost);
                let f = &mut c.fighters[p];
                f.level = (f.level - def.level_penalty.max(0)).max(f.level_min);
                f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
                return false;
            }
        }
    }
    let f = &mut c.fighters[p];
    f.set_slot(k, None);
    f.used.push(id);
    true
}

/// "Erase equipped skill(s)" (an erase skill's own, or an attack's erase on
/// hit): `FUN_140272510`. s48 picks the candidates (above), s49 orders them:
/// 0 shuffled, 1 highest Aura cost first, 2 lowest first (ties shuffled), 3
/// shuffled and then only the unlimited-use ones (s26 = 0: Lack), 4 only the
/// button the attack was cast from (Hungry Demon); s50 of them from the front
/// are erased. (So Tiger Slayer, Dragon Slayer, Lightning Sword... take the
/// lowest-cost skill of their colour, Shield Breaker the highest-cost
/// defence.)
fn erase_equipped(c: &mut Combat, caster: usize, p: usize, def: &SkillDef) -> bool {
    let filter = def.raw[ERASE_FILTER];
    if filter == 0 {
        return false;
    }
    let mut list: Vec<usize> = (0..SLOTS).filter(|&k| c.fighters[p].slots[k].is_some_and(|id| eligible(c, id, filter))).collect();
    let n = list.len();
    let cost_of = |c: &Combat, k: usize| c.fighters[p].slots[k].and_then(|id| c.skill(id)).map_or(0, |d| d.cost);
    match def.raw[ERASE_PICK] {
        order @ (1 | 2) if n > 1 => {
            // The program's bubble sort: n - 1 passes of n - 1 neighbour
            // comparisons, equal costs swapped on a 49 % roll.
            for _ in 0..n - 1 {
                for j in 0..n - 1 {
                    let (a, b) = (cost_of(c, list[j]), cost_of(c, list[j + 1]));
                    let out_of_order = if order == 1 { a < b } else { b < a };
                    if out_of_order || (a == b && c.random(100) > 50) {
                        list.swap(j, j + 1);
                    }
                }
            }
        }
        0 | 3 => {
            for idx in 0..n {
                let j = c.random(n as u32) as usize;
                list.swap(idx, j);
            }
            if def.raw[ERASE_PICK] == 3 {
                list.retain(|&k| c.fighters[p].slots[k].and_then(|id| c.skill(id)).is_some_and(|d| d.uses().is_none()));
            }
        }
        4 => {
            let button = c.fighters[caster].last_slot;
            list = if list.contains(&button) { vec![button] } else { Vec::new() };
        }
        _ => {}
    }
    // READ (capstone, `0x14027298e`: `mov eax, r13d`): `FUN_140272510` returns 1 as soon as it has
    // handed one listed button to `FUN_140262fc0`, whether or not that call then empties it (a
    // "can't be erased" skill stays, a Retain skill pays again): the caller plays common/91 for it.
    let mut attempted = false;
    for &k in list.iter().take(def.raw[ERASE_COUNT].max(0) as usize) {
        erase_slot(c, p, k);
        attempted = true;
    }
    attempted
}

/// Field (capsule-touching / battlefield) skills: s43 = 1, told apart by
/// s44/s45 (see module docs).
fn field_effect(c: &mut Combat, players: &[Player], caster: usize, def: &SkillDef) -> bool {
    let s44 = def.raw[44];
    let s45 = def.raw[45];
    match (s44, s45) {
        // Evaporate: "Erases all Aura Particles from battlefield."
        (2, _) => {
            let (gone, kept): (Vec<CapsuleItem>, Vec<CapsuleItem>) = std::mem::take(&mut c.capsules).into_iter().partition(|item| item.skill == AURA_PARTICLE);
            c.capsules = kept;
            log_erased(c, gone);
        }
        // Particle Convert: "Converts all capsules on battlefield to Aura Particles."
        (5, 3) => {
            for item in &mut c.capsules {
                item.skill = AURA_PARTICLE;
            }
        }
        // Change the World (s44 = 5, s45 = 2): every ready capsule swaps its skill for a random
        // undrawn card of its owner's deck (`FUN_14026ff20` -> `FUN_14014fa20`); nothing is erased.
        (5, 2) => swap_capsules(c),
        // Violent Change: "Erases all capsules from battlefield." (s45 = 0).
        // The new capsules come from `Combat::spawn_capsule`, which already
        // spawns whenever a player has fewer than `MAX_CAPSULES_PER_PLAYER`
        // capsules out.
        (5, _) => {
            let gone = std::mem::take(&mut c.capsules);
            log_erased(c, gone);
        }
        // Thieving Imp: "Captures capsule you are touching."
        (0, 1) => capture_touched(c, players, caster),
        // Abyss / Diabolical Trick (s44=0,s45=0): any capsule. Eliminate
        // (s44=1): "Erases Aura Particle you are touching" -- Aura only.
        // Shatter (s44=3): "... (not Aura)" -- excludes Aura Particles.
        _ => erase_touched(c, players, caster, s44 == 1, s44 == 3),
    }
    true
}

/// Change the World's body: for each capsule lying ready on the field (`appear == 0`: the spot's
/// state 2 -- an appearing one is skipped, `FUN_14026ff20` returns unless the state is 2), in field
/// order, `FUN_14014fa20(deck, owner, spot, -1)`:
/// 1. `n` = the undrawn cards left in the owner's deck (`30 - cursor`); with none, nothing swaps and
///    the capsule keeps its skill (`n < 1` jumps past the swap);
/// 2. a random undrawn card `r` (`rand() * n >> 15`, here the sim's seeded generator) is taken and the
///    capsule's old skill goes into the deck at `r`;
/// 3. the spot is then "taken" with `FUN_14014fd10(.., mode 0)` (state 2 only), whose first branch
///    draws the deck's NEXT card (`cursor++`, the respawn draw; it is overwritten at once);
/// 4. the capsule shows the card taken in 2, ready, same place.
/// Our `Fighter::arsenal` is the undrawn cards with the next one last, so step 3 pops the last card (after
/// step 2 has stored the old skill, which can itself be that last card: it is then drawn and lost).
/// Net: the capsule holds a random undrawn card, and the owner's undrawn pool loses one card.
fn swap_capsules(c: &mut Combat) {
    for k in 0..c.capsules.len() {
        if c.capsules[k].appear != 0 {
            continue;
        }
        let owner = c.capsules[k].owner;
        let n = c.fighters[owner].arsenal.len();
        if n == 0 {
            continue;
        }
        let r = c.random(n as u32) as usize;
        let old = c.capsules[k].skill;
        let taken = std::mem::replace(&mut c.fighters[owner].arsenal[r], old);
        c.fighters[owner].arsenal.pop();
        c.capsules[k].skill = taken;
    }
}

fn erase_touched(c: &mut Combat, players: &[Player], caster: usize, aura_only: bool, exclude_aura: bool) {
    let matches = |skill: i16| {
        if aura_only {
            skill == AURA_PARTICLE
        } else if exclude_aura {
            skill != AURA_PARTICLE
        } else {
            true
        }
    };
    if let Some(k) = touching(c, players, caster, matches) {
        let item = c.capsules.remove(k);
        log_erased(c, vec![item]);
    }
}

/// One `CapsuleErased` event per capsule an erase skill removed (the program's
/// `FUN_14014fd10` mode 1, which plays common/92 and the cap04 burst for each:
/// `0x14014edc7` / `0x14014ff07`). Not for Particle Convert, which rewrites the
/// capsules in place and is silent.
fn log_erased(c: &mut Combat, gone: Vec<CapsuleItem>) {
    for item in gone {
        c.log(CombatEvent::CapsuleErased { who: item.owner, pos: item.pos, skill: item.skill });
    }
}

fn capture_touched(c: &mut Combat, players: &[Player], caster: usize) {
    let Some(k) = touching(c, players, caster, |_| true) else { return };
    let Some(slot) = c.fighters[caster].slots.iter().position(Option::is_none) else { return };
    let item = c.capsules.remove(k);
    c.fighters[caster].set_slot(slot, Some(item.skill));
    // Thieving Imp plays the pickup sound only (common/86 via `FUN_140261a00`,
    // `0x1402700e3`): its `FUN_14014fd10` call passes mode 0 (taken, silent) --
    // the first audit read took the erase 92 for it too, wrongly (r9 = 0 at
    // `0x1402700f3`; only s45 = 0 erases with mode 1, `0x1402700fb`).
    c.log(CombatEvent::Capsule { who: caster, slot, skill: item.skill });
}

/// Index in `c.capsules` of a capsule (any owner) within reach of `caster`
/// and matching `pred`. Same reach as `Combat::capsule_in_reach`
/// (`in_pickup_reach`: 15 u in 3-D), but not owner-filtered: "erases capsule
/// you are touching" works on either player's capsules, not just your own.
fn touching(c: &Combat, players: &[Player], caster: usize, pred: impl Fn(i16) -> bool) -> Option<usize> {
    let pos = players[caster].pos;
    c.capsules.iter().position(|item| {
        pred(item.skill) && item.appear == 0 && in_pickup_reach(item.pos, pos)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use crate::combat::testkit::*;

    // Real skill ids and raw fields (erase_skills.json), hand-copied per capsule colour lane rules.
    const INTERFERENCE: i16 = 96; // s51=2 s52=2: "Reduces target's Level by 2."
    const EXHAUST: i16 = 222; // s51=4 s52=2: "Reduces target's Level by 50%."
    const FAIRY_RING: i16 = 281; // s51=1 s52=99, motion 4 param0=2 (everyone): "Reduces everyone's Aura to 0."
    const STEAL_POWER: i16 = 220; // s51=10 s52=99: "Absorbs all target's Aura."
    const NEUTRALIZATION: i16 = 219; // s51=17: "Resets target's status ..."
    const PURIFY: i16 = 162; // s51=17, motion 4 param0=2 (everyone).
    const REFRESH: i16 = 223; // s43=4 (Caster): "Resets your status ..."
    const ANGEL_TEARS: i16 = 285; // s43=4, top-level strength=1: "... Add: 1 Level."
    const STARTING_POINT: i16 = 159; // s51=38: erase environment crystal.
    const RETURN_TO_NATURE: i16 = 160; // s51=39: erase crystal + reset everyone.
    const VICIOUS_BALANCE: i16 = 280; // s51=47: everyone's Level to the lowest.
    const MELTDOWN: i16 = 102; // s51=49 s52=2: Level -2 per own Aura Particle on the field.
    const DISINTEGRATE: i16 = 104; // [Hole] s51=2 s52=99: everyone's Level to 0.
    const TIGER_SLAYER: i16 = 216; // s48=1 s49=2 s50=1: "1 of ... attack skills".
    const WIND_PRESSURE: i16 = 157; // s48=8 s50=4: "all ... non-attack and non-defensive".
    const GIANT_SLAYER: i16 = 218; // s48=9 s49=1 s50=1: "highest Aura cost".
    const DISREMEMBER: i16 = 316; // s48=9 s49=2 s50=1: "lowest-cost".
    const HUNGRY_DEMON: i16 = 277; // s48=9 s49=4 s50=1: "same button".
    const SHATTER: i16 = 36; // s43=1 s44=3: touch, not Aura.
    const ELIMINATE: i16 = 99; // s43=1 s44=1: touch, Aura only.
    const THIEVING_IMP: i16 = 278; // s43=1 s44=0 s45=1: capture touched.
    const EVAPORATE: i16 = 100; // s43=1 s44=2: all Aura Particles.
    const VIOLENT_CHANGE: i16 = 284; // s43=1 s44=5 s45=0: all capsules.
    const PARTICLE_CONVERT: i16 = 101; // s43=1 s44=5 s45=3: convert all to Aura.
    const CHANGE_THE_WORLD: i16 = 237; // s43=1 s44=5 s45=2: swap every capsule's skill.

    // A dummy attack and defence skill to put in test slots (arbitrary ids/costs; not real game data).
    const DUMMY_ATTACK: i16 = 900;
    const DUMMY_DEFENCE: i16 = 901;
    const DUMMY_ATTACK_CHEAP: i16 = 902;
    const DUMMY_ATTACK_PRICEY: i16 = 903;

    fn erase_skills() -> Vec<SkillDef> {
        vec![
            def(INTERFERENCE, Capsule::Erase, 1, &[(f::ADD_EFFECT, 2), (f::ADD_AMOUNT, 2), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(EXHAUST, Capsule::Erase, 4, &[(f::ADD_EFFECT, 4), (f::ADD_AMOUNT, 2), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(FAIRY_RING, Capsule::Erase, 2, &[(f::ADD_EFFECT, 1), (f::ADD_AMOUNT, 99), (f::MOTION, 4), (P, 2)]),
            def(STEAL_POWER, Capsule::Erase, 3, &[(f::ADD_EFFECT, 10), (f::ADD_AMOUNT, 99), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(NEUTRALIZATION, Capsule::Erase, 1, &[(f::ADD_EFFECT, 17), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(PURIFY, Capsule::Erase, 3, &[(f::ADD_EFFECT, 17), (f::MOTION, 4), (P, 2)]),
            def(REFRESH, Capsule::Erase, 0, &[(43, 4), (f::STRENGTH_RULE, 8)]),
            def(ANGEL_TEARS, Capsule::Erase, 1, &[(43, 4), (f::STRENGTH_RULE, 8), (f::STRENGTH, 1)]),
            def(STARTING_POINT, Capsule::Erase, 1, &[(f::ADD_EFFECT, 38), (f::MOTION, 4), (P, 2)]),
            def(RETURN_TO_NATURE, Capsule::Erase, 3, &[(f::ADD_EFFECT, 39), (f::MOTION, 4), (P, 2)]),
            def(VICIOUS_BALANCE, Capsule::Erase, 2, &[(f::ADD_EFFECT, 47), (f::MOTION, 4), (P, 2)]),
            def(MELTDOWN, Capsule::Erase, 3, &[(f::ADD_EFFECT, 49), (f::ADD_AMOUNT, 2), (f::MOTION, 4), (P, 2)]),
            def(DISINTEGRATE, Capsule::Erase, 6, &[(f::ADD_EFFECT, 2), (f::ADD_AMOUNT, 99), (f::MOTION, 16), (P, 30), (P + 1, 10), (P + 2, 300)]),
            def(TIGER_SLAYER, Capsule::Erase, 2, &[(ERASE_FILTER, 1), (ERASE_PICK, 2), (ERASE_COUNT, 1), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(WIND_PRESSURE, Capsule::Erase, 3, &[(ERASE_FILTER, 8), (ERASE_COUNT, 4), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(GIANT_SLAYER, Capsule::Erase, 3, &[(ERASE_FILTER, 9), (ERASE_PICK, 1), (ERASE_COUNT, 1), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(DISREMEMBER, Capsule::Erase, 8, &[(ERASE_FILTER, 9), (ERASE_PICK, 2), (ERASE_COUNT, 1), (P, -80), (P + 1, 120), (P + 4, 30)]),
            def(HUNGRY_DEMON, Capsule::Erase, 4, &[(ERASE_FILTER, 9), (ERASE_PICK, 4), (ERASE_COUNT, 1), (f::HEALTH_COST, 1), (P, -80), (P + 1, 120), (P + 4, 100)]),
            def(SHATTER, Capsule::Erase, 2, &[(43, 1), (44, 3)]),
            def(ELIMINATE, Capsule::Erase, 2, &[(43, 1), (44, 1)]),
            def(THIEVING_IMP, Capsule::Erase, 2, &[(43, 1), (44, 0), (45, 1), (26, 1)]),
            def(EVAPORATE, Capsule::Erase, 3, &[(43, 1), (44, 2)]),
            def(VIOLENT_CHANGE, Capsule::Erase, 6, &[(43, 1), (44, 5), (45, 0)]),
            def(PARTICLE_CONVERT, Capsule::Erase, 4, &[(43, 1), (44, 5), (45, 3)]),
            def(CHANGE_THE_WORLD, Capsule::Erase, 5, &[(43, 1), (44, 5), (45, 2)]),
            def(DUMMY_ATTACK, Capsule::Attack, 2, &[]),
            def(DUMMY_DEFENCE, Capsule::Defence, 2, &[(f::DEFENCE_KIND, 0)]),
            def(DUMMY_ATTACK_CHEAP, Capsule::Attack, 1, &[]),
            def(DUMMY_ATTACK_PRICEY, Capsule::Attack, 9, &[]),
        ]
    }

    // Erase shells travel (`P`/`P+1`/`P+4` = a homing shell, s64..s68 = -80 120
    // 0 0 100): give the shot time to cross the 40 u gap (speed s65 x 3 u/s =
    // 360 u/s, ~7 ticks) after `use_skill` gets it airborne, same margin as
    // combat.rs's own `a_non_attack_shot_lands_without_damage_or_stun`.
    const SHELL_FLIGHT_TICKS: usize = 20;

    #[test]
    fn interference_reduces_level_by_two() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].level = 8;
        use_skill(&mut c, &mut p, &col, 0, INTERFERENCE);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].level, 6);
    }

    #[test]
    fn exhaust_halves_level() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].level = 8;
        use_skill(&mut c, &mut p, &col, 0, EXHAUST);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].level, 4);
    }

    #[test]
    fn fairy_ring_drops_everyones_aura_to_zero() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 5 * AURA_TICKS_PER_POINT;
        use_skill(&mut c, &mut p, &col, 0, FAIRY_RING);
        assert_eq!((c.fighters[0].aura(), c.fighters[1].aura()), (0, 0));
    }

    #[test]
    fn steal_power_absorbs_all_of_targets_aura() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 5 * AURA_TICKS_PER_POINT;
        use_skill(&mut c, &mut p, &col, 0, STEAL_POWER);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].aura(), 0);
        assert_eq!(c.fighters[0].aura(), 8, "capped at caster's own Level");
    }

    #[test]
    fn neutralization_resets_targets_status() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].status.bound_ticks = 90;
        use_skill(&mut c, &mut p, &col, 0, NEUTRALIZATION);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].status.bound_ticks, 0);
    }

    #[test]
    fn purify_resets_everyones_status() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].status.bound_ticks = 90;
        c.fighters[1].status.bound_ticks = 90;
        use_skill(&mut c, &mut p, &col, 0, PURIFY);
        assert_eq!((c.fighters[0].status.bound_ticks, c.fighters[1].status.bound_ticks), (0, 0));
    }

    #[test]
    fn refresh_resets_the_casters_own_status() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].status.bound_ticks = 90;
        use_skill(&mut c, &mut p, &col, 0, REFRESH);
        assert_eq!(c.fighters[0].status.bound_ticks, 0);
    }

    #[test]
    fn angel_tears_resets_status_and_adds_a_level() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[0].status.bound_ticks = 90;
        use_skill(&mut c, &mut p, &col, 0, ANGEL_TEARS);
        assert_eq!((c.fighters[0].level, c.fighters[0].status.bound_ticks), (9, 0));
    }

    #[test]
    fn starting_point_erases_the_environment_crystal() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.crystal = Some(crate::moves::effects::Crystal { id: 1, owner: 1, skill: 0, pos: Vec3::ZERO, age: 0 });
        use_skill(&mut c, &mut p, &col, 0, STARTING_POINT);
        assert!(c.crystal.is_none());
    }

    #[test]
    fn return_to_nature_erases_crystal_and_resets_everyone() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.crystal = Some(crate::moves::effects::Crystal { id: 1, owner: 1, skill: 0, pos: Vec3::ZERO, age: 0 });
        c.fighters[0].status.bound_ticks = 90;
        c.fighters[1].status.bound_ticks = 90;
        use_skill(&mut c, &mut p, &col, 0, RETURN_TO_NATURE);
        assert!(c.crystal.is_none());
        assert_eq!((c.fighters[0].status.bound_ticks, c.fighters[1].status.bound_ticks), (0, 0));
    }

    #[test]
    fn vicious_balance_drops_everyone_to_the_lowest_level() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 3;
        use_skill(&mut c, &mut p, &col, 0, VICIOUS_BALANCE);
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (3, 3));
    }

    #[test]
    fn meltdown_costs_two_levels_per_own_aura_particle_on_the_field() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 8;
        // Off in a corner, well outside pickup reach of either player's spawn
        // -- otherwise pressing the skill button would capture the capsule
        // standing underfoot instead of casting Meltdown.
        let far = Vec3::new(500.0, 0.0, 500.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: AURA_PARTICLE, pos: far, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 0, skill: AURA_PARTICLE, pos: far, appear: 0 });
        c.capsules.push(CapsuleItem { id: 3, owner: 1, skill: AURA_PARTICLE, pos: far, appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, MELTDOWN);
        // LIVE take 102 (3 particles on the field, 2 P0's, 1 P1's): P0 Level 12 -> 8 and its Aura 9 -> 8
        // (clamped to the new Level), P1 12 -> 10 and 12 -> 10. Our film run has no particles on the field
        // (a harness difference: the take's field came from its memory deck), hence no Meltdown row to fix.
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (4, 6), "caster loses 2x2, target loses 2x1");
    }

    #[test]
    fn disintegrate_drops_everyones_level_to_zero() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 8;
        use_skill(&mut c, &mut p, &col, 0, DISINTEGRATE);
        // [Hole]: marks the target, waits out its warning (s64=30 -> 60 sim
        // ticks), then goes off -- give it plenty of margin.
        run(&mut c, &mut p, &col, 90);
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (0, 0));
    }

    #[test]
    fn tiger_slayer_erases_one_of_the_targets_attack_skills() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None, None];
        use_skill(&mut c, &mut p, &col, 0, TIGER_SLAYER);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [None, Some(DUMMY_DEFENCE), None, None], "only the attack skill is erased");
    }

    #[test]
    fn wind_pressure_erases_all_non_attack_non_defensive_skills() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), Some(NEUTRALIZATION), Some(FAIRY_RING)];
        use_skill(&mut c, &mut p, &col, 0, WIND_PRESSURE);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None, None]);
    }

    #[test]
    fn lack_erases_only_unlimited_use_skills() {
        const LACK: i16 = 221; // s48=9 s49=3: "all of target's equipped unlimited-use skills".
        let mut skills = erase_skills();
        skills.push(def(LACK, Capsule::Erase, 3, &[(ERASE_FILTER, 9), (ERASE_PICK, 3), (ERASE_COUNT, 4), (P, -80), (P + 1, 120), (P + 4, 100)]));
        skills.iter_mut().find(|d| d.id == DUMMY_DEFENCE).unwrap().raw[26] = 1; // single use
        let (mut c, mut p, col) = setup(skills, 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), Some(DUMMY_ATTACK_CHEAP), None];
        use_skill(&mut c, &mut p, &col, 0, LACK);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [None, Some(DUMMY_DEFENCE), None, None]);
    }

    #[test]
    fn giant_slayer_erases_the_equipped_skill_with_the_highest_cost() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK_CHEAP), Some(DUMMY_ATTACK_PRICEY), Some(DUMMY_DEFENCE), None];
        use_skill(&mut c, &mut p, &col, 0, GIANT_SLAYER);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [Some(DUMMY_ATTACK_CHEAP), None, Some(DUMMY_DEFENCE), None]);
    }

    #[test]
    fn disremember_erases_the_equipped_skill_with_the_lowest_cost() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK_CHEAP), Some(DUMMY_ATTACK_PRICEY), Some(DUMMY_DEFENCE), None];
        use_skill(&mut c, &mut p, &col, 0, DISREMEMBER);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [None, Some(DUMMY_ATTACK_PRICEY), Some(DUMMY_DEFENCE), None]);
    }

    #[test]
    fn hungry_demon_erases_the_skill_on_the_same_button() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None, None];
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[1] = Some(HUNGRY_DEMON);
        press(&mut c, &mut p, &col, 0, 1); // button 1
        run(&mut c, &mut p, &col, CAST_TICKS as usize + SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots, [Some(DUMMY_ATTACK), None, None, None]);
    }

    #[test]
    fn hungry_demon_keeps_the_button_it_was_cast_with() {
        // A different button pressed while it is in flight doesn't change which skill it eats.
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), Some(DUMMY_ATTACK), None];
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[0].slots[1] = Some(HUNGRY_DEMON);
        c.fighters[0].slots[2] = Some(DUMMY_ATTACK_CHEAP);
        press(&mut c, &mut p, &col, 0, 1); // button 1
        run(&mut c, &mut p, &col, CAST_TICKS as usize);
        press(&mut c, &mut p, &col, 0, 2); // then button 2, before it lands
        assert_eq!(c.fighters[0].last_slot, 2);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].slots[1], None, "button 1's skill is eaten");
        assert_eq!(c.fighters[1].slots[2], Some(DUMMY_ATTACK), "not button 2's");
    }

    #[test]
    fn orb_negates_an_erase_skill_used_against_its_holder() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[1].level = 8;
        c.fighters[1].slots[3] = Some(ORB);
        use_skill(&mut c, &mut p, &col, 0, INTERFERENCE);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!(c.fighters[1].level, 8, "Orb: \"Automatically negates any Erase skill used\"");
        assert_eq!(c.fighters[1].slots[3], Some(ORB), "Orb itself can't be erased/overwritten");
    }

    #[test]
    fn amulet_bounces_the_erase_effect_back_onto_the_caster() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.fighters[0].level = 8;
        c.fighters[1].level = 8;
        c.fighters[1].slots[3] = Some(AMULET);
        use_skill(&mut c, &mut p, &col, 0, INTERFERENCE);
        run(&mut c, &mut p, &col, SHELL_FLIGHT_TICKS);
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (6, 8), "the caster takes the Level loss instead");
    }

    #[test]
    fn shatter_erases_a_touched_capsule_but_not_aura() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 1, skill: AURA_PARTICLE, pos: p[0].pos, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: DUMMY_ATTACK, pos: p[0].pos, appear: 0 });
        c.fighters[0].slots[0] = Some(SHATTER);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        assert_eq!(c.capsules.len(), 1);
        assert_eq!(c.capsules[0].skill, AURA_PARTICLE, "Aura Particles are untouched by Shatter");
    }

    #[test]
    fn eliminate_only_erases_a_touched_aura_particle() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 1, skill: DUMMY_ATTACK, pos: p[0].pos, appear: 0 });
        c.fighters[0].slots[0] = Some(ELIMINATE);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        assert_eq!(c.capsules.len(), 1, "not an Aura Particle: left alone");
    }

    #[test]
    fn thieving_imp_captures_the_touched_capsule() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 1, skill: DUMMY_ATTACK, pos: p[0].pos, appear: 0 });
        c.fighters[0].slots = [Some(THIEVING_IMP), None, None, None];
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        assert!(c.capsules.is_empty());
        // Slot 0 (Thieving Imp's own) is consumed by `cast()` before the
        // effect resolves, so it's the "first empty slot" the capture lands in.
        assert_eq!(c.fighters[0].slots[0], Some(DUMMY_ATTACK), "captured into the first empty slot");
    }

    #[test]
    fn evaporate_clears_every_aura_particle_but_not_other_capsules() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: AURA_PARTICLE, pos: Vec3::new(500.0, 0.0, 500.0), appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: AURA_PARTICLE, pos: Vec3::new(-500.0, 0.0, -500.0), appear: 0 });
        c.capsules.push(CapsuleItem { id: 3, owner: 1, skill: DUMMY_ATTACK, pos: Vec3::new(-500.0, 0.0, -500.0), appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, EVAPORATE);
        assert_eq!(c.capsules.len(), 1);
        assert_eq!(c.capsules[0].skill, DUMMY_ATTACK);
    }

    #[test]
    fn violent_change_clears_every_capsule_on_the_field() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: Vec3::new(500.0, 0.0, 500.0), appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: AURA_PARTICLE, pos: Vec3::new(-500.0, 0.0, -500.0), appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, VIOLENT_CHANGE);
        assert!(c.capsules.is_empty());
    }

    /// Change the World (s44 = 5, s45 = 2) swaps instead of erasing (`FUN_14026ff20` s45 = 2 ->
    /// `FUN_14014fa20(deck, owner, spot, -1)`): every ready capsule keeps its place and takes a random
    /// undrawn card of its OWNER's deck; the old skill goes back into the deck, and the respawn draw
    /// of `FUN_14014fd10` (mode 0, inside `fa20`) takes the deck's next card, so the pool shrinks by one.
    #[test]
    fn change_the_world_swaps_each_capsule_for_an_undrawn_card_of_its_owners_deck() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        let (a, b, ghost) = (Vec3::new(500.0, 0.0, 500.0), Vec3::new(-500.0, 0.0, -500.0), Vec3::new(0.0, 0.0, 900.0));
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: a, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: AURA_PARTICLE, pos: b, appear: 0 });
        // Still appearing (the spot's state 1): untouched.
        c.capsules.push(CapsuleItem { id: 3, owner: 0, skill: DUMMY_DEFENCE, pos: ghost, appear: 500 });
        c.fighters[0].arsenal = vec![10, 11, 12, 13];
        c.fighters[1].arsenal = vec![20, 21, 22];
        use_skill(&mut c, &mut p, &col, 0, CHANGE_THE_WORLD);
        assert_eq!(c.capsules.len(), 3, "nothing is erased");
        assert_eq!((c.capsules[0].pos, c.capsules[1].pos), (a, b), "same places");
        assert_eq!((c.capsules[0].owner, c.capsules[1].owner), (0, 1));
        assert!([10, 11, 12, 13].contains(&c.capsules[0].skill), "owner 0's capsule took a card from owner 0's deck: {}", c.capsules[0].skill);
        assert!([20, 21, 22].contains(&c.capsules[1].skill), "owner 1's capsule took a card from owner 1's deck: {}", c.capsules[1].skill);
        assert_eq!(c.capsules[2].skill, DUMMY_DEFENCE, "an appearing capsule is skipped");
        // The old skills went into the decks; one card of each deck was drawn and lost.
        assert_eq!(c.fighters[0].arsenal.len(), 3);
        assert_eq!(c.fighters[1].arsenal.len(), 2);
        // Owner 0's five cards (four undrawn + the capsule's) are now four (arsenal + capsule), all from that set.
        let mut left: Vec<i16> = c.fighters[0].arsenal.clone();
        left.push(c.capsules[0].skill);
        assert!(left.iter().all(|s| [10, 11, 12, 13, DUMMY_ATTACK].contains(s)), "{left:?}");
    }

    /// Nothing undrawn: the capsule keeps its skill (`n < 1` jumps past the swap in `FUN_14014fa20`);
    /// the same seeded generator gives the same swaps.
    #[test]
    fn change_the_world_with_an_empty_deck_changes_nothing_and_is_deterministic() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: Vec3::new(500.0, 0.0, 500.0), appear: 0 });
        c.fighters[0].arsenal.clear();
        use_skill(&mut c, &mut p, &col, 0, CHANGE_THE_WORLD);
        assert_eq!(c.capsules[0].skill, DUMMY_ATTACK);
        let run_once = || {
            let (mut c, mut p, col) = setup(erase_skills(), 40.0);
            for k in 0..3 {
                c.capsules.push(CapsuleItem { id: k + 1, owner: 0, skill: DUMMY_ATTACK, pos: Vec3::new(500.0 + k as f32, 0.0, 500.0), appear: 0 });
            }
            c.fighters[0].arsenal = (10..20).collect();
            use_skill(&mut c, &mut p, &col, 0, CHANGE_THE_WORLD);
            (c.capsules.iter().map(|k| k.skill).collect::<Vec<_>>(), c.fighters[0].arsenal.clone())
        };
        assert_eq!(run_once(), run_once());
    }

    #[test]
    fn particle_convert_turns_every_capsule_into_an_aura_particle() {
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: Vec3::new(500.0, 0.0, 500.0), appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: DUMMY_DEFENCE, pos: Vec3::new(-500.0, 0.0, -500.0), appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, PARTICLE_CONVERT);
        assert!(c.capsules.iter().all(|item| item.skill == AURA_PARTICLE));
        assert_eq!(c.capsules.len(), 2, "converted in place, not removed");
    }

    /// The sounds' events (`FUN_14014fd10` mode 1 = erased: common/92 per capsule,
    /// `0x14014edc7` / `0x14014ff07`): one `CapsuleErased` per capsule an erase
    /// skill removes, naming the capsule's owner and place; Particle Convert, which
    /// rewrites capsules in place, logs none; Thieving Imp logs the pickup
    /// (`Capsule`, common/86) and no erase (mode 0 at `0x1402700f3`).
    #[test]
    fn erasing_capsules_logs_one_event_per_capsule() {
        let erased = |c: &Combat| c.events.iter().filter_map(|(_, e)| if let CombatEvent::CapsuleErased { who, pos, skill } = *e { Some((who, pos, skill)) } else { None }).collect::<Vec<_>>();
        let (a, b) = (Vec3::new(500.0, 0.0, 500.0), Vec3::new(-500.0, 0.0, -500.0));
        // Violent Change: every capsule, each its own event with its own owner.
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: a, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: AURA_PARTICLE, pos: b, appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, VIOLENT_CHANGE);
        assert_eq!(erased(&c), vec![(0, a, DUMMY_ATTACK), (1, b, AURA_PARTICLE)]);
        // Evaporate: only the Aura Particles.
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: AURA_PARTICLE, pos: a, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: DUMMY_ATTACK, pos: b, appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, EVAPORATE);
        assert_eq!(erased(&c), vec![(0, a, AURA_PARTICLE)]);
        // Particle Convert: silent.
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: DUMMY_ATTACK, pos: a, appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, PARTICLE_CONVERT);
        assert!(erased(&c).is_empty());
        // Thieving Imp: the pickup only (the audit's 86 + 92 was a misread).
        let (mut c, mut p, col) = setup(erase_skills(), 40.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 1, skill: DUMMY_ATTACK, pos: p[0].pos, appear: 0 });
        c.fighters[0].slots = [Some(THIEVING_IMP), None, None, None];
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        press(&mut c, &mut p, &col, 0, 0);
        run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        let kinds: Vec<&str> = c.events.iter().filter_map(|(_, e)| match e {
            CombatEvent::Capsule { who: 0, .. } => Some("pickup"),
            CombatEvent::CapsuleErased { who: 1, .. } => Some("erase"),
            _ => None,
        }).collect();
        assert_eq!(kinds, vec!["pickup"]);
    }

    /// The 11 skills whose s49 the old code read wrongly (audits/P0-SKILLS.md item 9), with
    /// their real (s48, s49, s50) from the table: ordering is by Aura cost, lowest first for
    /// s49 = 2 and highest first for s49 = 1 (`FUN_140272510` + `140273a80/b80`).
    #[test]
    fn the_eleven_erase_equipped_skills_take_the_lowest_or_highest_cost_of_their_colour() {
        // (id, s48 colour, s49 order, s50 count)
        let table: [(i16, i16, i16, i16); 11] = [
            (216, 1, 2, 1), // Tiger Slayer
            (217, 2, 2, 1), // Dragon Slayer
            (122, 2, 2, 1), // Lightning Sword
            (135, 2, 2, 1), // Lightning Blade
            (187, 2, 2, 1), // Dragon Punch
            (184, 1, 2, 1), // Tiger Kick
            (134, 1, 2, 1), // Storm Blade
            (142, 2, 2, 1), // Lightning Bolt
            (128, 2, 2, 1), // Piece of Thunder
            (309, 1, 2, 1), // Piece of Wind
            (324, 2, 1, 1), // Shield Breaker: the highest-cost defence
        ];
        for (id, filter, order, count) in table {
            let skill = def(id, Capsule::Erase, 2, &[(ERASE_FILTER, filter), (ERASE_PICK, order), (ERASE_COUNT, count)]);
            let mut skills = erase_skills();
            skills.push(def(910, Capsule::Attack, 5, &[]));
            skills.push(def(911, Capsule::Attack, 2, &[]));
            skills.push(def(912, Capsule::Defence, 4, &[(f::DEFENCE_KIND, 0)]));
            skills.push(def(913, Capsule::Defence, 1, &[(f::DEFENCE_KIND, 0)]));
            let (mut c, _p, _col) = setup(skills, 40.0);
            c.fighters[1].slots = [Some(910), Some(911), Some(912), Some(913)];
            erase_equipped(&mut c, 0, 1, &skill);
            let erased: Vec<usize> = (0..SLOTS).filter(|&k| c.fighters[1].slots[k].is_none()).collect();
            let expect = match (filter, order) {
                (1, 2) => 1, // the cost-2 attack
                (2, 2) => 3, // the cost-1 defence
                (2, 1) => 2, // the cost-4 defence
                _ => unreachable!(),
            };
            assert_eq!(erased, vec![expect], "skill {id} (s48 {filter}, s49 {order})");
        }
    }

    #[test]
    fn an_erase_list_counts_a_locked_skill_but_cannot_erase_it() {
        // The list is built from every equipped skill of the colour; a "can't be erased" one
        // (s41 = 2) at the front uses up the erase (`FUN_140262fc0` returns without doing anything).
        let skill = def(216, Capsule::Erase, 2, &[(ERASE_FILTER, 1), (ERASE_PICK, 2), (ERASE_COUNT, 1)]);
        let mut skills = erase_skills();
        skills.push(def(910, Capsule::Attack, 5, &[]));
        skills.push(def(911, Capsule::Attack, 1, &[(41, 2)]));
        let (mut c, _p, _col) = setup(skills, 40.0);
        c.fighters[1].slots = [Some(910), Some(911), None, None];
        erase_equipped(&mut c, 0, 1, &skill);
        assert_eq!(c.fighters[1].slots, [Some(910), Some(911), None, None], "the cheapest attack is the locked one: nothing erased");
    }

    #[test]
    fn s48_6_and_7_pick_aura_particles_and_everything_else() {
        let (mut c, _p, _col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(AURA_PARTICLE), Some(DUMMY_ATTACK), Some(AURA_PARTICLE), None];
        let aura = def(950, Capsule::Erase, 2, &[(ERASE_FILTER, 6), (ERASE_COUNT, 4)]);
        erase_equipped(&mut c, 0, 1, &aura);
        assert_eq!(c.fighters[1].slots, [None, Some(DUMMY_ATTACK), None, None], "s48 = 6: Aura Particles");
        c.fighters[1].slots = [Some(AURA_PARTICLE), Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None];
        let not_aura = def(951, Capsule::Erase, 2, &[(ERASE_FILTER, 7), (ERASE_COUNT, 4)]);
        erase_equipped(&mut c, 0, 1, &not_aura);
        assert_eq!(c.fighters[1].slots, [Some(AURA_PARTICLE), None, None, None], "s48 = 7: everything but Aura Particles");
    }

    #[test]
    fn erase_slot_keeps_a_retain_skill_it_can_pay_for_and_consumes_the_rest() {
        let mut skills = erase_skills();
        skills.push(def(960, Capsule::Attack, 2, &[(41, 1)])); // Retain
        let (mut c, _p, _col) = setup(skills, 40.0);
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 5 * AURA_TICKS_PER_POINT;
        c.fighters[1].slots[0] = Some(960);
        assert!(!erase_slot(&mut c, 1, 0), "paid again and kept");
        assert_eq!((c.fighters[1].slots[0], c.fighters[1].aura()), (Some(960), 3));
        c.fighters[1].aura_ticks = AURA_TICKS_PER_POINT;
        assert!(erase_slot(&mut c, 1, 0), "can't pay: erased");
        assert_eq!((c.fighters[1].slots[0], c.fighters[1].used.clone()), (None, vec![960]));
    }

    #[test]
    fn calamity_erases_everyones_equipped_skills() {
        // Calamity: "[Hole] Erases everyone's equipped skills." s48 9, s50 4, Hole 30 10 300.
        let calamity = def(283, Capsule::Erase, 5, &[(ERASE_FILTER, 9), (ERASE_COUNT, 4), (f::MOTION, 16), (P, 30), (P + 1, 10), (P + 2, 300)]);
        let mut skills = erase_skills();
        skills.push(calamity);
        let (mut c, mut p, col) = setup(skills, 40.0);
        c.fighters[1].slots = [Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None, None];
        use_skill(&mut c, &mut p, &col, 0, 283);
        c.fighters[0].slots = [None, Some(DUMMY_ATTACK), None, None];
        run(&mut c, &mut p, &col, 120);
        assert_eq!(c.fighters[1].slots, [None; SLOTS], "the Hole's target");
        assert_eq!(c.fighters[0].slots, [None; SLOTS], "and the caster: everyone's");
    }

    /// `FUN_140272510` mode 8 tests `(type - 1) as u16 > 1`; the Aura Particle is program record 0 (gsdata
    /// file offset 0x18, s9 = 0), so it is on the "non-attack, non-defence" list (Turbulence, Wind Pressure).
    #[test]
    fn s48_8_lists_the_aura_particle_too() {
        let (mut c, _p, _col) = setup(erase_skills(), 40.0);
        c.fighters[1].slots = [Some(AURA_PARTICLE), Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None];
        let skill = def(957, Capsule::Erase, 2, &[(ERASE_FILTER, 8), (ERASE_COUNT, 4)]);
        assert!(erase_equipped(&mut c, 0, 1, &skill));
        assert_eq!(c.fighters[1].slots, [None, Some(DUMMY_ATTACK), Some(DUMMY_DEFENCE), None]);
    }

    /// `0x14027298e` (`mov eax, r13d`): `FUN_140272510` returns 1 once a listed button was handed to
    /// `FUN_140262fc0`, even if that button is "can't be erased" and stays; common/91 plays for it.
    #[test]
    fn the_erase_reports_a_listed_button_even_when_it_stays() {
        let skill = def(216, Capsule::Erase, 2, &[(ERASE_FILTER, 1), (ERASE_PICK, 2), (ERASE_COUNT, 1)]);
        let mut skills = erase_skills();
        skills.push(def(911, Capsule::Attack, 1, &[(41, 2)]));
        let (mut c, _p, _col) = setup(skills, 40.0);
        c.fighters[1].slots = [Some(911), None, None, None];
        assert!(erase_equipped(&mut c, 0, 1, &skill), "attempted");
        assert_eq!(c.fighters[1].slots[0], Some(911), "but it stays");
        let none = def(217, Capsule::Erase, 2, &[(ERASE_FILTER, 2), (ERASE_PICK, 2), (ERASE_COUNT, 1)]);
        assert!(!erase_equipped(&mut c, 0, 1, &none), "nothing of that colour on a button: no sound");
    }

    /// An erase capsule goes through the same on-hit path (`0x14026fb8e`): Calamity (LIVE: one 0.91 at
    /// the hit tick, both players' buttons erased) logs `ButtonErased` once.
    #[test]
    fn an_erase_skill_logs_one_button_erased_event() {
        let (mut c, mut p, _col) = setup(erase_skills(), 40.0);
        c.fighters[0].slots = [Some(AURA_PARTICLE), None, None, None];
        c.fighters[1].slots = [Some(AURA_PARTICLE), Some(DUMMY_ATTACK), None, None];
        let skill = def(283, Capsule::Erase, 5, &[(ERASE_FILTER, 9), (ERASE_COUNT, 4)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Everyone, &skill));
        let n = c.events.iter().filter(|(_, e)| matches!(e, CombatEvent::ButtonErased { .. })).count();
        assert_eq!(n, 1);
    }
}
