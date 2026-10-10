//! Lock-on targets: who a player can lock onto, which target they hold, and
//! how the target button cycles it. Deterministic (no time, no randomness, no
//! Bevy). Read from the program (`PDUWP.exe`, `engine/re-out/decomp_all.c`;
//! docs/audits/P0-FOURPLAYER.md "1b. C, D"), checked again for this lane:
//!
//! * `0x1402543e0(lock, idx)` -- **is slot `idx` a valid target for this
//!   player**: `idx` is -1 (none) or >= 32 -> no; the 32-entry slot table
//!   (`DAT_1404bcb50`) must hold something; a slot with a controller needs it
//!   present (`*ctrl != 0`) and the slot's own flags `+0x14`, `+0x1c` non-zero;
//!   for a controller-driven slot: **not yourself** (player `+0x14` equal),
//!   **health > 0** (`ctrl[3] >= 1`), **not under status 8** (Optical Camo,
//!   `0x14024d4d0(ctrl, 8)`), **not a teammate** (player `+0x1c` equal) unless
//!   the friendly-lock option `DAT_14052dfb9` is set; finally, if *you* are
//!   under status 9 the only valid slot is the one status 9 names (a forced
//!   target). Status 9 is written by the attack Add code 40 (`0x140272bc0`
//!   case 0x28: `0x14024d650(target, 9, decoy player + 1, s52 ticks)`), which
//!   only Decoy carries: `Fighter::forced_lock`.
//! * `0x140252f70(lock, from)` -- **first valid slot scanning from `from`**,
//!   wrapping modulo 32, at most 32 tries; none valid leaves the target at -1.
//! * `0x1402531d0(lock)` -- per tick: a target of -1 re-picks from 0; when the
//!   lock is not held (or the battle state is not 6) the target is re-picked
//!   from the current one (so a dead or camouflaged target hands over even
//!   while you are not locked); while locked, **the target button (input mask
//!   `0x400`, on its rising edge) picks from `current + 1`**: the next valid
//!   slot in index order, not the nearest. (The nearest-first walk over a float
//!   table at lock `+0x90` is the other mode, option `DAT_14052dfb8 != 0`,
//!   whose default is unread, audit R1b; not built.)
//!
//! Our slot table is the roster: slots 0..MAX_PLAYERS are players, the other
//! 28 of the program's 32 are empty (invalid), so scanning modulo 32 and
//! skipping them is exactly the program's order.
//!
//! One deliberate difference, UNCONFIRMED: the program leaves a locked target
//! that became invalid (knocked out, camouflaged) untouched until the cycle
//! button is pressed (the camera just stops aiming); here it hands over to the
//! next valid player straight away, as it does when the lock is not held. The
//! original can never show the difference in a duel (the round ends with the
//! only enemy's knock-out) and Optical Camo is handled by the lock release
//! below.

use super::{Combat, MAX_PLAYERS};

/// The program's lockable-slot table size (`0x1402543e0`: `idx < 32`).
pub const LOCK_SLOTS: usize = 32;

impl Combat {
    /// Whether slot `idx` is a valid lock target for player `me`
    /// (`0x1402543e0`; see the module docs for the rule).
    pub fn valid_target(&self, me: usize, idx: usize) -> bool {
        if idx >= LOCK_SLOTS || idx >= MAX_PLAYERS || !self.roster.is_active(idx) {
            return false; // slot table entry empty / controller not present
        }
        if idx == me {
            return false; // not yourself
        }
        let f = &self.fighters[idx];
        if f.health < 1 {
            return false; // knocked out (`ctrl[3] < 1`)
        }
        if f.status.invisible_ticks > 0 {
            return false; // Optical Camo: "no one can lock onto you" (status 8)
        }
        if self.roster.same_team(me, idx) && !self.friendly_lock {
            return false; // a teammate, unless the friendly-lock option is on
        }
        // Under status 9 (Decoy, add code 40: the value is the decoy's player
        // index + 1) the only valid slot is the one it names.
        if let Some((decoy, _)) = self.fighters[me].forced_lock {
            return decoy == idx;
        }
        true
    }

    /// The first valid target scanning from slot `from` (`0x140252f70`):
    /// wrapping modulo 32, up to 32 tries; `None` = nobody is valid (-1).
    pub fn pick_target(&self, me: usize, from: usize) -> Option<usize> {
        (0..LOCK_SLOTS).map(|k| (from + k) % LOCK_SLOTS).find(|&idx| self.valid_target(me, idx))
    }

    /// The target player `me` holds after this tick (`0x1402531d0`). `current`
    /// is the target they held (`None` = -1), `locked` whether the lock is on
    /// this tick, `cycle` the rising edge of the target button.
    pub fn next_target(&self, me: usize, current: Option<usize>, locked: bool, cycle: bool) -> Option<usize> {
        if locked && cycle {
            // `0x140252f70(current + 1)`: -1 + 1 = 0.
            return self.pick_target(me, current.map_or(0, |c| c + 1));
        }
        if locked && current.is_some_and(|c| self.valid_target(me, c)) {
            return current; // held until it stops being valid
        }
        // Not locked: re-picked from the current one every tick (a valid one is its own pick).
        // Locked with an invalid one: hands over (see the module docs).
        self.pick_target(me, current.unwrap_or(0))
    }

    /// Whether `me` has anyone they could lock onto at all, ignoring who is
    /// knocked out (Optical Camo on the last candidate means "no one can lock
    /// onto you": the lock drops; a dead enemy ends the round instead).
    pub fn lockable_exists(&self, me: usize) -> bool {
        self.roster.slots().any(|j| j != me && (self.friendly_lock || !self.roster.same_team(me, j)) && self.fighters[j].status.invisible_ticks == 0)
    }

    /// The player `me` aims at given the lock target they hold: the target if
    /// there is one, else their foe (`foe_of`: the first enemy). Every aimed
    /// thing -- casts, homing, instant skills, the lock camera -- asks this.
    pub fn aim_at(&self, me: usize, target: Option<usize>) -> usize {
        target.filter(|&t| t != me && self.roster.is_active(t)).unwrap_or_else(|| self.foe_of(me))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Combat, Roster, AURA_PARTICLE, MAX_PLAYERS};

    fn four(roster: Roster) -> Combat {
        Combat::with_roster(Vec::new(), roster, std::array::from_fn(|_| vec![AURA_PARTICLE; 30]))
    }

    /// `0x1402543e0` with four slots [self, teammate, dead, enemy]: only the enemy is valid.
    #[test]
    fn valid_target_skips_self_teammate_and_the_dead() {
        let mut c = four(Roster::with_teams(&[0, 0, 1, 1]));
        c.fighters[2].health = 0; // knocked out (an enemy, dead)
        // Player 0's view: slot 0 is self, 1 a teammate, 2 dead, 3 an enemy.
        assert_eq!([0, 1, 2, 3].map(|j| c.valid_target(0, j)), [false, false, false, true]);
        // The friendly-lock option (`DAT_14052dfb9`) makes the teammate valid.
        c.friendly_lock = true;
        assert_eq!([0, 1, 2, 3].map(|j| c.valid_target(0, j)), [false, true, false, true]);
    }

    #[test]
    fn valid_target_rejects_camouflage_empty_slots_and_out_of_range_indices() {
        let mut c = four(Roster::free_for_all(3));
        assert_eq!([0, 1, 2].map(|j| c.valid_target(0, j)), [false, true, true]);
        c.fighters[1].status.invisible_ticks = 5; // status 8: Optical Camo
        assert_eq!([0, 1, 2].map(|j| c.valid_target(0, j)), [false, false, true]);
        // Slot 3 is not in the roster; 4..31 are empty slots of the program's table; 32+ is out of range.
        assert!(![3, 4, 31, 32, 40, usize::MAX].iter().any(|&j| c.valid_target(0, j)));
        assert!(!c.valid_target(0, 3));
        // Everyone is someone else's enemy in a free-for-all, never their own.
        assert!(c.valid_target(2, 0) && c.valid_target(1, 2) && !c.valid_target(2, 2));
    }

    /// `0x140252f70`: the first valid slot from the start index, wrapping modulo 32.
    #[test]
    fn pick_is_the_first_valid_slot_from_there_wrapping() {
        let mut c = four(Roster::free_for_all(4));
        // Player 1: valid slots are 0, 2, 3.
        assert_eq!([0, 1, 2, 3].map(|from| c.pick_target(1, from)), [Some(0), Some(2), Some(2), Some(3)]);
        // From 4 the scan walks the empty slots 4..31 and wraps round to 0 (current 3 + 1).
        assert_eq!([4, 5, 31].map(|from| c.pick_target(1, from)), [Some(0), Some(0), Some(0)]);
        // The dead are skipped; nobody valid = -1.
        c.fighters[2].health = 0;
        assert_eq!(c.pick_target(1, 1), Some(3));
        c.fighters[0].health = 0;
        c.fighters[3].status.invisible_ticks = 9;
        assert_eq!(c.pick_target(1, 0), None);
        // A target index of `current + 1` past the last slot still lands on slot 0 first.
        assert_eq!(four(Roster::free_for_all(4)).pick_target(0, MAX_PLAYERS), Some(1));
    }

    /// `0x1402531d0`: the button cycles to the next valid slot in index order, only while locked.
    #[test]
    fn cycling_walks_slot_order_with_wrap_and_only_while_locked() {
        let c = four(Roster::free_for_all(4));
        let mut t = Some(1);
        let mut seen = Vec::new();
        for _ in 0..6 {
            t = c.next_target(0, t, true, true);
            seen.push(t.unwrap());
        }
        assert_eq!(seen, vec![2, 3, 1, 2, 3, 1], "next valid slot in index order, wrapping; never the player themselves");
        // Not locked: the button does nothing (the target is re-picked, and a valid one stays).
        assert_eq!(c.next_target(0, Some(2), false, true), Some(2));
        // Locked, no press: the target is held.
        assert_eq!(c.next_target(0, Some(2), true, false), Some(2));
        // Cycling from no target starts at slot 0 (-1 + 1).
        assert_eq!(c.next_target(2, None, true, true), Some(0));
        // A lone valid target: the cycle comes back to it.
        let mut c = four(Roster::free_for_all(3));
        c.fighters[2].health = 0;
        assert_eq!(c.next_target(0, Some(1), true, true), Some(1));
    }

    #[test]
    fn a_dead_or_camouflaged_target_hands_over_to_the_next_valid_slot() {
        let mut c = four(Roster::free_for_all(4));
        c.fighters[2].status.invisible_ticks = 30; // camouflaged
        // Not locked (`0x1402531d0`: re-picked from the current slot every tick).
        assert_eq!(c.next_target(0, Some(2), false, false), Some(3));
        // Locked: ours hands over too (the program would wait for the button; see the docs).
        assert_eq!(c.next_target(0, Some(2), true, false), Some(3));
        c.fighters[3].health = 0;
        assert_eq!(c.next_target(0, Some(2), true, false), Some(1), "wraps past the dead and the camouflaged");
        // Nobody left: the target is -1, and `aim_at` falls back to the foe so casts still have a direction.
        c.fighters[1].health = 0;
        assert_eq!(c.next_target(0, Some(2), true, false), None);
        assert_eq!(c.aim_at(0, None), c.foe_of(0));
    }

    #[test]
    fn a_lock_only_drops_when_no_enemy_is_left_to_lock_onto() {
        let mut c = four(Roster::with_teams(&[0, 0, 1, 1]));
        assert!(c.lockable_exists(0));
        c.fighters[2].status.invisible_ticks = 10;
        assert!(c.lockable_exists(0), "one enemy is still visible");
        c.fighters[3].status.invisible_ticks = 10;
        assert!(!c.lockable_exists(0), "both enemies camouflaged: no one can lock onto you");
        c.friendly_lock = true;
        assert!(c.lockable_exists(0), "the partner is lockable with the friendly option");
        // Knocked-out enemies do not count against the lock: the round logic ends the round.
        let mut c = four(Roster::duel());
        c.fighters[1].health = 0;
        assert!(c.lockable_exists(0));
    }

    #[test]
    fn aim_goes_to_the_target_else_to_the_first_enemy() {
        let c = four(Roster::with_teams(&[0, 0, 1, 1]));
        assert_eq!(c.aim_at(0, Some(3)), 3);
        assert_eq!(c.aim_at(0, None), 2, "first enemy in slot order, never the partner");
        assert_eq!(c.aim_at(0, Some(0)), 2, "never yourself");
        assert_eq!(c.aim_at(0, Some(3)), 3);
        assert_eq!(four(Roster::duel()).aim_at(1, None), 0);
    }
}
