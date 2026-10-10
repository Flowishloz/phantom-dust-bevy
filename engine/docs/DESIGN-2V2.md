# Design: 2v2 with resuscitation

An original feature request (not in the 2004/2017 game's local split-screen, which was 2 players): two teams of two
share a screen, and a downed teammate can be brought back into the fight. This is the largest "Large/uncertain
item" on the roadmap (PLAN.md sizes it L, Opus) because the sim, camera, HUD and front end all assume exactly two
players today. This doc is the design pass PLAN.md asked for before any lane starts building.

**What the original itself supports, read from the program (PROGRAM-RE.md "Lock-on"):** the 2004/2017 game already
has team-mate awareness baked in — lock-on already excludes team-mates by default (unless an option allows
targeting them), it already tracks up to 32 lockable slots, and a second button already cycles to the next valid
target. Some area attacks already carry a "can't hurt the caster" exclusion that reads like the seed of a
team-immunity mask (MOVES.md/PROGRAM-RE.md motion-16 Hole: "the caster's own hole can't hurt the caster ⚠️ partner
in 2-v-2"). We are not inventing team mechanics from nothing — we're building the half the original had (Tag Team
mode existed on Xbox, per the fan-guide research in the confidence-branch `MATCH-RULES.md` §4) and generalising our
2-player engine to carry it.

**Resuscitation**, per that same research (single web source, ⚠️ unverified against the real game): "an alive
teammate can revive: capture the downed body like a capsule, hold it in a slot, then use it. The revived player's
max health is halved (rounded up). If the holder dies, the team loses." Treat this as a strong lead, not gospel —
decision 2 below offers a softer house rule if the strict reading turns out to feel bad.

## 4 players, one screen: split layout options

| Option | Layout | Note |
|---|---|---|
| 2×2 quad | Each player their own quarter | Simplest, most symmetric, easiest to reason about per-player camera code |
| Team halves | Screen split in two (one per team), each half split top/bottom for its two members | Visually groups teammates; reuses the existing side-by-side *and* top/bottom split code nested, not new |

Either way this is new code: `layout_viewports` and the per-player `Camera3d` spawn loop (`main.rs`) only know
about 2 viewports today; the settings menu's "split screen" row (side-by-side / top-bottom) needs a 4-way option.

## 2 pads + keyboard realities

A real 2v2 needs 4 independent human inputs — there is no AI in scope to fill a slot. `gather_input` today reads
`pads.iter().take(2)` and gives P2 a keyboard fallback (arrows already taken by P1's D-pad, so P2 already only
gets a pad in practice per GAME.md). Laurence very likely owns 2 pads, not 4. This is a logistics problem as much
as a code one — see decision 4.

## Teams

Every `Fighter`/`Player` needs a team id (0 or 1 for two teams of two). Lock-on target selection, HUD colouring,
win/lose condition and revive eligibility all key off it.

## Lock-on target cycling

Mirror what the original already does (PROGRAM-RE.md "Lock-on"): default lock-on excludes team-mates; a second
button cycles through the remaining valid (enemy, alive, not team-mate) targets. GAME.md already lists "left
trigger = switch target" under "Not built" — this is exactly where it plugs in, no new button needed.

## Revive

Proposed shape, closest to the research finding: a knocked-down player's body becomes a capturable object at
their fall spot (like a capsule); a living teammate stands at it and presses a skill button to capture it into a
slot ("Revive" occupies a slot like any skill); a later press on that slot brings the teammate back at half their
max health (rounded up). See decision 2 for how strictly to keep the "holder dies → team loses" clause.

## HUD

Four health/aura panels instead of two; `HUD.md`'s layout (one panel per view, from `BtMenuEn.alr`) needs a
smaller per-player variant, plus a way to show "down, awaiting revive" and which teammate is carrying a captured
revive.

## Camera

Each of 4 players needs their own camera and viewport (as today, ×2). Lock-on aim math that currently hard-codes
"the opponent" (`positions[1 - i]` in `sim.rs`) needs to resolve to "the current lock-on target," which may now be
either enemy, chosen by the cycling above.

## Sim changes: generalising from 2 players

Every one of these is a real place in the code that assumes exactly 2 players (`[X; 2]` arrays, or `1 - i` as
"the opponent"). This is the bulk of why the item is sized L / Opus:

- `sim.rs`: `Match.players: [Player; 2]`, `Match::new(spawns: [Vec3; 2], ..)`, `reset_positions` (`for i in 0..2`,
  `spawns[1 - i]`), `pub fn step(&mut self, inputs: [Input; 2], ..)`.
- `combat.rs`: `Combat.fighters: [Fighter; 2]`, and essentially every function signature in the file —
  `dash_eligible`, `step`, `tick_fighter`, `cast`, `release_charge`, `fire`, `take_effect`, `lunge`,
  `move_projectiles`, `tick_areas`, plus the test helpers (`setup`, `run`, `press`, `use_skill`, `guard_vs`,
  `cast_skill`, `cast_and_release`) — all take `&mut [Player; 2]` / `&[Player; 2]`.
- `main.rs`: `PendingInputs([Input; 2])`, `gather_input`'s `pads.iter().take(2)`, the per-player camera/viewport
  loop, and scattered `for i in 0..2` in HUD/effects glue.
- `frontend/state.rs` / `setup.rs`: `MatchSetup.chars: [String; 2]`, `.arsenals: [Vec<i16>; 2]`, and the setup
  screen's hard-coded P1/P2 two-column layout.
- `hud.rs`: per-player panels assume exactly 2 view slots.

Recommended shape: `[X; 2]` → `[X; 4]` (fixed-size, still no heap allocation in the sim's hot path) rather than
`Vec<X>`, keeping the determinism contract's "no surprises from allocation/iteration order" as simple as it is
today; unused slots for a 1v1 or 3-player match are just inactive. `1 - i` opponent math is replaced by small
team-aware helpers (`teammates_of(i)`, `enemies_of(i)`, `current_target(i)`).

## Phases

1. **Generalise the sim** — `[X; 2]` → `[X; 4]`, team ids, 4-controller input, 4-way split screen. 1v1 must keep
   working unchanged (existing tests as the regression net) before anything team-specific is added.
2. **Team-aware rules** — lock-on excludes team-mates + cycles targets, team-coloured HUD, team win/lose condition
   (a team is out when both members are down, unless a revive keeps one up).
3. **Revive** — the capture/hold/use mechanic itself, the half-health rule, the "holder dies" clause (or its
   softened version, decision 2).
4. **Polish** — camera framing on a 4-way split, HUD legibility, playtest with Laurence, docs.

## Risks

- The `[X; 2]` → `[X; 4]` refactor touches nearly every function in `combat.rs` (3,500 lines) and `sim.rs` —
  real regression risk for the 1v1 game that already works; needs the existing test suite green throughout, run
  in small reviewable steps rather than one big rewrite.
- 4-way split screen halves each player's screen real estate again (already a tight fit at top/bottom split for
  2) — HUD and camera framing will need real playtesting, not just code review.
- Controller availability: a true 2v2 needs 4 physical inputs; see decision 4.
- The "holder dies → team loses" reading is a single unverified web source — building it literally risks a rule
  that feels arbitrary or overly punishing without ever having been confirmed against the real game.
- Determinism must survive the whole generalisation: still only tick counters and the existing seeded rng, no
  floats from rendering, no per-player behaviour that depends on array/iteration order rather than team id.

## Decisions for Laurence

1. **Team assignment?**
   - A: Fixed teams chosen on the setup screen (who's with whom, like characters are chosen today). (Recommended)
   - B: Teams randomised each match (seeded, so it's still deterministic/replayable).
   - C: Defer — keep teams simple/fixed for 2v2's first version, revisit dynamic teams alongside the separate
     "winner-stays-on" roadmap idea later.

2. **How strictly to build resuscitation?**
   - A: Follow the fan research exactly — capture the downed body into a slot, revive at half max health, and if
     the holder dies before using it the team loses immediately. Closest to the original, but harsh and unverified.
   - B: Softer house rule — stand next to a downed teammate for a few seconds to revive them at half health, no
     capture/slot step, no instant team-loss if the reviver dies. Simpler to build, less punishing. (Recommended)
   - C: Ship 2v2 without revive first (straight 2v2, team loses when both are down), add revive as a fast-follow
     once teams/lock-on/camera are proven solid.

3. **Split layout?**
   - A: 2×2 quad — each player their own quarter. (Recommended)
   - B: Team halves, each split top/bottom for its two members.

4. **Controller requirement?**
   - A: Require 4 pads for 2v2 (truest to a real 2v2; Laurence may need to buy 2 more).
   - B: Support keyboard as a cramped P3/P4 fallback for a quick playtest, pads recommended for real couch nights. (Recommended)

## Decided (Laurence, 2026-09-27)

- Teams chosen on the setup screen.
- Revive: ship straight 2v2 first; add the original's revive after its rules are confirmed from the program (PROGRAM-RE.md).
- Layout: four quarters.
- Controllers: pads, with keyboard as a fallback for P3/P4.
