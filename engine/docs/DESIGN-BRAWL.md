# Design: Brawl mode

Laurence's brief: "all players spawn with 4 set, infinite-use skills they can't change; a fixed amount of aura with
no refill; no capsule pickups. Pure aura management + the moves you're given. Zero menu time — made for
pass-the-pad." No unlocks, no progression — this is a rules variant, not a new game.

The good news: almost everything Brawl needs already exists as data. The game already has unlimited-use skills
(s26 = 0, about half the 399 — Laser and friends never run out), already deals a starting hand onto the 4 buttons
(`Rules.start_hand`), already tracks Aura against a Level ceiling, and already spawns capsules on a timer we can
just turn off. Brawl is a small `combat::Rules` extension plus a menu shortcut, not a new sim mode.

## How the 4 skills are chosen

"Can't change" falls out for free if every skill in the hand is unlimited-use: with no capsules on the ground
there's nothing to capture into a slot anyway, so the 4 starting skills are the 4 skills for the whole round,
automatically. The only real question is which 4.

| Option | How it works | Trade-off |
|---|---|---|
| Fixed sets | A handful of curated 4-skill hands (shared, or one per character) written by hand into `skills.json`/a small Brawl table | Predictable, easy to balance by feel, no risk of a dud or broken hand |
| Seeded random from a legal pool | At round start, `Combat`'s existing xorshift (`rng` field) deterministically draws 4 skills from every unlimited-use skill (s26 = 0) | More variety and replay value; risk of a weak hand (four defences) or a strong one (four attacks) |
| Seeded random from a curated pool | Same draw, but from a hand-filtered subset of unlimited-use skills (excludes ones that don't suit a no-refill, no-capsule match) | Middle ground: variety with a safety net |

Either way the draw (fixed or random) becomes an ordinary arsenal of exactly 4 skills, dealt with the existing
`start_hand = 4` path — no new "hand" concept in the sim.

## Aura: fixed amount, no refill

- Starting aura is already "full to Level" (`Fighter::new` sets `aura_ticks = level * AURA_TICKS_PER_POINT`), so
  the fixed pool is just `Rules.start_level` — pick a Level and that's the Brawl aura pool.
- No refill needs one new `Rules` flag, e.g. `aura_refill: bool` (default `true`), read where `tick_fighter`
  computes the recovery `rate` (`combat.rs` ~line 903): `if !rules.aura_refill { 0 } else { <existing formula> }`.
  One guard clause, not a Brawl-specific branch — any future "no regen" ruleset gets it free.

## No capsule pickups

One `Rules` flag, e.g. `capsules: bool` (default `true`), checked at the top of the per-tick capsule-spawn function
(`combat.rs` ~line 1903-1918: `if !rules.capsules { return; }`). Existing capsules already on the field (there
won't be any at round start if the flag is set before spawn) never appear; nothing else changes.

## What ends a round

Nothing new: health-0 knock-out and the existing time-limit rule ("healthier player wins" on time-out,
DECISIONS.md 2026-09-27) already cover a no-refill, no-capsule match — if anything they matter *more* here, since
a stalemate is now possible (two players hoarding aura). No new end condition needed.

## UI entry point

Laurence's brief says "zero menu time." The title menu (`frontend/title.rs`) is 4 rows today (VERSUS MODE,
ARSENAL, OPTIONS MODE, EXIT GAME); a 5th row is the natural entry point. See decision 3 below for how much picking
it should still allow.

## Maps onto `combat::Rules` as data

```rust
pub struct Rules {
    pub start_level: i32,
    pub start_hand: usize,
    pub capsule_respawn_ticks: u16,
    pub aura_refill: bool,   // new
    pub capsules: bool,      // new
}
```
Brawl = `Rules { start_level: <picked>, start_hand: 4, aura_refill: false, capsules: false, .. }` plus a 4-skill
arsenal built by whichever method decision 1 picks. `Match::step`/`Combat::step` gain two `if` checks, not a
`GameMode` enum or a parallel code path.

## Determinism

The skill draw (if random) uses `Combat`'s existing seeded xorshift — the same generator combat effects already
use — so it's deterministic and replay-compatible as long as the seed is chosen once, before the match starts, and
carried in the match setup (front end), never re-rolled from wall-clock time.

## Tests

Extend `cargo test -p pd_game` (`combat.rs`'s existing `Combat::setup`/`run`/`press` test helpers):
- Aura never rises above its starting value over N ticks when `aura_refill = false` (only ever falls, on spend).
- No capsule appears over N ticks when `capsules = false`, even past `capsule_respawn_ticks`.
- A skill with `uses() == None` survives any number of casts (slot never clears itself).
- Same seed → same 4-skill draw (if the random option is picked) — a one-line determinism check, same shape as
  the existing replay-determinism test.

## Decisions for Laurence

1. **How are the 4 skills chosen?**
   - A: Fixed curated sets (one per character, or a few universal hands) — safest balance, easiest to tune by eye. (Recommended)
   - B: Seeded random from every unlimited-use skill — maximum variety, occasional dud or brutal hand.
   - C: Seeded random from a hand-filtered "Brawl-safe" subset of unlimited-use skills — variety with a floor.

2. **Starting aura amount (the fixed pool)?**
   - A: Level 4 — a handful of casts of most skills before you're empty. (Recommended)
   - B: Level 8 — generous, favours spammy/cheap skills.
   - C: Make it a setup option (like Level already is) labelled "Brawl aura," tune later from play-testing.

3. **How minimal is the title-menu entry?**
   - A: One press from the title starts a match immediately — random characters and stage, fresh seeded hands. Truest to "zero menu time." (Recommended)
   - B: One press opens a trimmed one-screen picker (characters + stage only; no arsenal/level/hand rows, since Brawl fixes those).
   - C: Reuse the existing Setup screen with Brawl as a rules toggle next to Level/Hand — most flexible, least new UI, but not "zero menu time."

## Decided (Laurence, 2026-09-27)

- Skills: fixed curated sets (option A).
- Starting Aura: a setup option (option C), no refill.
- Entry: one press from the title opens a trimmed one-screen picker (characters + stage) (option B).

## Game modes (built 2026-10-02, lane modes)

The framework this design asked for ("Rules flags + a fixed hand, no parallel sim path") now exists, driven by Laurence's
first mode rather than Brawl itself (Brawl's no-refill Aura and its quick-picker entry are still unbuilt).

- **Sim (`combat::Rules`, all default to the normal match; `Rules::NORMAL` is the constant)**:
  - `capsules: bool` (default true): `spawn_capsule` returns at once and `pending_capsules` is empty when off, so the opening deal, respawns and the timer capsules never happen.
  - `infinite_uses: bool` (default false): `consume_use` leaves a button's skill and use count alone. (Close Quarters' four skills are already s26 = 0 in the data; the flag keeps any finite-use or Lapse-exhausted skill in its slot, for the next mode.)
  - `fixed_hand: Option<[i16; 4]>` (default None): at every round start the hand is put on buttons 1-4 in order and the arsenal is left empty (nothing to draw), instead of the `start_hand` deal.
  - Aura "refills as normal": nothing changed there. `start_level: 10` = 10 Aura (the cap is Level), recovery is the ordinary formula; the fighters take the neutral Aura kind (`arsenal::DEFAULT_KIND`, rate 1.0).
- **Table (`frontend/modes.rs`)**: `MODES: &[GameMode]` = `{ id, name, blurb, hand, rules }`; `GameMode::rules()` adds the hand. A new mode is one new entry. Skill ids are `skills.json` `id`s (the program's own index is `record + 1`, not used here).
- **Flow (changed 2026-10-03, lane online)**: modes are chosen in two places, both over `MODES`: the **GAMEMODES** row of the local versus setup's game-rules page (STANDARD + every entry; `SetupState::set_mode`) and the **GAMEMODE** row of the online host's lobby. Either way the choice lands in `MatchSetup.mode` and `apply_to_combat` uses the mode's `Rules`. The title's MODES row, the `AppState::Modes` list and `ActiveMode` are gone; the setup's choice survives a rematch. `--mode <index|id>` starts it from the command line (legacy launch).
- **Setup in a mode**: the Arsenal line of every seat (people and CPUs) shows "FIXED HAND" without arrows and the cursor skips it; Game rule, Starting level and Starting skills show their fixed values dimmed and are skipped; the header stays "VERSUS SETUP"; character, colour, team, stage, time, rounds, CPUs (Y) all work as in versus.
- **Tests**: `frontend::modes::tests` (table entry -> Rules + hand, mode -> combat, hand and 10 Aura at the start of every round, no capsule over two respawn periods on both capsule paths with a capsules-on control, the four skills in their slots after 120 casts, a finite-use skill kept by the flag only, Aura refill identical to a normal Level-10 match and stopping at 10), `frontend::setup::tests::a_mode_fixes_the_hand_...`.
- **Proof**: `engine/shots/gm_titlemodes.png`, `gm_list.png`, `gm_seat1.png`, `gm_cpu.png`, `gm_rules.png`, `gm_match.png` (HUD: four skills, 10/10), `gm_late.png` (tick 1500, past the respawn time: no capsules).
