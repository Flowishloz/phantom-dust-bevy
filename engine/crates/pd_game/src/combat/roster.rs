//! Who is in the match: the player slots, which are active, and who is on
//! whose team. Part of the deterministic simulation (no time, no
//! randomness, no Bevy).
//!
//! The original is built for four players and three rule sets (Battle
//! Royale, Tag Team, One-on-One): a player has a slot 0-3 (`+0x14`) and a
//! team id (`+0x1c`, filled from the lobby's setup record short [0x18] by
//! `0x14024e950`; docs/audits/P0-FOURPLAYER.md "1b. A"). In free-for-all
//! every player is their own team, like the original where team == slot.
//!
//! Slots that are not active are never iterated, never draw random numbers
//! and never appear in an event, so a two-player match runs bit-identical to
//! the old two-player-only engine (`combat::tests::replay`).

/// The most players one match can hold (the original's four).
pub const MAX_PLAYERS: usize = 4;

/// `Combat::round_over`'s winner when nobody won the round (time up with
/// nothing to choose between, or everyone knocked out together). It is
/// `usize::MAX`, not a slot number, so it can never be mistaken for one.
/// (With two players the old spelling was the magic winner `2`; consumers
/// that test `winner > 1` still read this as a draw.)
pub const DRAW: usize = usize::MAX;

/// The active player slots and their teams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roster {
    pub active: [bool; MAX_PLAYERS],
    /// Team id per slot (only read for active slots).
    pub team: [u8; MAX_PLAYERS],
}

impl Roster {
    /// One-on-one: slots 0 and 1, one team each.
    #[allow(dead_code)] // tests and `Combat::with_arsenals` (the game builds rosters with `with_teams` now)
    pub fn duel() -> Self {
        Self::free_for_all(2)
    }

    /// Everyone against everyone: the first `n` slots (1..=4), one team each.
    #[allow(dead_code)] // as `duel`
    pub fn free_for_all(n: usize) -> Self {
        let n = n.min(MAX_PLAYERS);
        Roster { active: std::array::from_fn(|i| i < n), team: std::array::from_fn(|i| i as u8) }
    }

    /// The first `teams.len()` slots (up to four), slot `i` on team `teams[i]`:
    /// `[0, 0, 1, 1]` is a 2v2. (The setup screen builds rosters with this;
    /// the front end is another lane's, so nothing in the game calls it yet.)
    #[allow(dead_code)]
    pub fn with_teams(teams: &[u8]) -> Self {
        let mut r = Roster { active: [false; MAX_PLAYERS], team: std::array::from_fn(|i| i as u8) };
        for (i, &t) in teams.iter().take(MAX_PLAYERS).enumerate() {
            r.active[i] = true;
            r.team[i] = t;
        }
        r
    }

    pub fn is_active(&self, i: usize) -> bool {
        i < MAX_PLAYERS && self.active[i]
    }

    /// Number of active players.
    pub fn count(&self) -> usize {
        self.active.iter().filter(|&&a| a).count()
    }

    /// Active slots in ascending order: every loop over players in the sim
    /// goes through this, so the order is fixed and inactive slots are inert.
    pub fn slots(&self) -> impl Iterator<Item = usize> {
        let active = self.active;
        (0..MAX_PLAYERS).filter(move |&i| active[i])
    }

    /// Whether active slots `a` and `b` are on one team (a player is on their
    /// own team).
    pub fn same_team(&self, a: usize, b: usize) -> bool {
        self.team[a] == self.team[b]
    }

    /// Active players not on `i`'s team, in slot order.
    pub fn enemies_of(&self, i: usize) -> impl Iterator<Item = usize> {
        let r = *self;
        r.slots().filter(move |&j| !r.same_team(i, j))
    }

    /// Active players on `i`'s team other than `i`, in slot order.
    #[allow(dead_code)]
    pub fn teammates_of(&self, i: usize) -> impl Iterator<Item = usize> {
        let r = *self;
        r.slots().filter(move |&j| j != i && r.same_team(i, j))
    }

    /// The lowest-numbered enemy of `i`, if any.
    pub fn first_enemy(&self, i: usize) -> Option<usize> {
        self.enemies_of(i).next()
    }

    /// Number of distinct teams with at least one of the players passing
    /// `alive` (the original counts one alive flag per team, `0x14012c510`
    /// through `0x14012c440`), and the lowest alive slot.
    pub fn alive_teams(&self, alive: impl Fn(usize) -> bool) -> (usize, Option<usize>) {
        let mut seen: Vec<u8> = Vec::new();
        let mut first = None;
        for i in self.slots().filter(|&i| alive(i)) {
            first.get_or_insert(i);
            if !seen.contains(&self.team[i]) {
                seen.push(self.team[i]);
            }
        }
        (seen.len(), first)
    }
}

/// Whether an attack can hurt the attacker's own teammates.
///
/// UNCONFIRMED. The program's hit path gives an attack object a hit mask of
/// "all four players minus the caster" (`0x14026cd70`: `~(1 << caster_slot) &
/// 0xf`; `0x14026dbd0`) and the swept-sphere test (`0x14026c1c0`) and
/// hit-clearing (`0x14026ecc0`) compare slots only; no team comparison was
/// found in any of them, so the original is probably `On` (audit R5 has not
/// read the object allocator `0x140263a00/0x140263ce0`, nor
/// `0x14026f530/0x140272510/0x140272bc0`, for a team test). The one team
/// test in skill code is the unlimited Hole (`0x14026fcb0`), which skips
/// teammates in one variant. `Off` is the house rule Laurence may prefer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FriendlyFire {
    /// Attacks reach everyone but their caster (the original, as far as read).
    #[default]
    On,
    /// Attacks skip the caster's teammates.
    #[allow(dead_code)]
    Off,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duel_has_two_active_slots_on_their_own_teams() {
        let r = Roster::duel();
        assert_eq!(r.slots().collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(r.count(), 2);
        assert_eq!(r.first_enemy(0), Some(1));
        assert_eq!(r.first_enemy(1), Some(0));
        assert!(!r.is_active(2) && !r.is_active(3) && !r.is_active(9));
        assert_eq!(r.teammates_of(0).count(), 0);
    }

    #[test]
    fn free_for_all_makes_everyone_else_an_enemy_in_slot_order() {
        let r = Roster::free_for_all(4);
        assert_eq!(r.enemies_of(2).collect::<Vec<_>>(), vec![0, 1, 3]);
        assert_eq!(r.first_enemy(0), Some(1));
        assert_eq!(Roster::free_for_all(3).slots().collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(Roster::free_for_all(9).count(), MAX_PLAYERS, "clamped");
    }

    #[test]
    fn teams_split_enemies_from_teammates() {
        let r = Roster::with_teams(&[0, 0, 1, 1]);
        assert_eq!(r.teammates_of(0).collect::<Vec<_>>(), vec![1]);
        assert_eq!(r.enemies_of(0).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(r.enemies_of(3).collect::<Vec<_>>(), vec![0, 1]);
        assert!(r.same_team(2, 3) && !r.same_team(1, 2));
        // A mixed shape: two on one team against two loners.
        let r = Roster::with_teams(&[0, 1, 0, 2]);
        assert_eq!(r.enemies_of(0).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(r.first_enemy(2), Some(1));
    }

    #[test]
    fn alive_teams_counts_teams_not_players() {
        let r = Roster::with_teams(&[0, 0, 1, 1]);
        assert_eq!(r.alive_teams(|_| true), (2, Some(0)));
        assert_eq!(r.alive_teams(|i| i == 1 || i == 3), (2, Some(1)));
        assert_eq!(r.alive_teams(|i| i < 2), (1, Some(0)), "both of team 0 alive, team 1 wiped: one team left");
        assert_eq!(r.alive_teams(|_| false), (0, None));
        // Free for all: every player is a team.
        assert_eq!(Roster::free_for_all(4).alive_teams(|i| i != 2).0, 3);
    }

    #[test]
    fn draw_is_not_a_slot() {
        assert!(DRAW >= MAX_PLAYERS && DRAW > 1, "reads as a draw to code that tests `winner > 1`");
    }

    #[test]
    fn friendly_fire_defaults_to_the_originals_reading() {
        assert_eq!(FriendlyFire::default(), FriendlyFire::On);
    }
}
