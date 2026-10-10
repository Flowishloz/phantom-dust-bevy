# Handoff: what's next after the 2026-09-29 session (audit, hit-stop, camera, AA)

**For:** the next model picking up Phantom Dust. **From:** Hermes/Leo, 2026-09-29.
**Verdict up front:** six things shipped, one bug I introduced is fixed, one bug I
introduced is **fixed but unverified**, and the single most valuable lesson of the
day is in the last section — read that first if you read nothing else.

## The short version of today's state

| Area | State |
|---|---|
| Sound | **Parked** for a frontier model. Footstep/capsule id resolution needs the audio DLL or a bank name we don't have. |
| Orbs / capsule glow | **Parked.** Research says the original has no capsule glow sprites at all (see below). |
| Audit of "what's missing" | **Done** — `docs/AUDIT-2026-09-29.md`. The ROADMAP's Next list was badly stale. |
| Hit-stop + KO slow-mo | **Shipped**, `PD_HITSTOP=1`, off by default. Two real bugs found and fixed. |
| Camera | Pitch limits decoded from the original's own file; top/bottom framing fixed; **camera height now a 5-way settings row**. |
| Anti-aliasing | Toggle built, then **parked**. 4× MSAA fixed, and that is what the game was always doing. |
| Skill families | **The one real remaining gameplay gap**, and it is bigger than it looked. |
| Setup→Match transition | **Unverified.** Two bugs reported, cause of one removed, neither seen fixed. |

## What actually shipped (commits, newest first)

- `798cae8` Camera height as a 5-way settings row, MID default
- `fb24174` Restore `TB_CAM_HEIGHT_DROP` to 14 (my regression, see below)
- `5b82105` Park the AA toggle; 4× MSAA fixed, and correct a wrong claim
- `3e352ba` Hit-stop: fix a deadlock that froze the game on the first hit
- `2b71935` Anti-aliasing settings toggle (**superseded by 5b82105**)
- `34f028b` Camera: real pitch limits, top/bottom framing
- `774c1d4` Hit-stop and KO slow-motion (`PD_HITSTOP=1`)
- `573f1f5` Audit of what is actually missing
- `a6787a7` Footstep thread documented, including a measured dead end

Suite is **308 / 36 / 7 / 10 green**. `rustfmt` is not installed for the GNU
toolchain, so lint errors from the patch tool are pre-existing noise.

## The three things the next lane should know before writing code

### 1. The interactive menu path is invisible to the headless harness

`--near` goes straight into a match. It **cannot** reach the Setup→Match
transition or the countdown. Two bugs Laurence found in play — the Setup screen
drawing on top of the countdown, and the crosshair/nametag stamping everywhere
the camera looked — were invisible to every automated capture I ran. `Msaa` is
the identified cause of both (`Some(Msaa::Off)` is not the same as no `Msaa`
component: it also governs MSAA writeback between cameras sharing a target).
The toggle is now parked so that state cannot be reached, but **nobody has seen
it fixed.** First task: press START on a fresh launch and look.

`--menu-script up,down,confirm,...` exists for this and I could not get it
through the menus — it appears to need the right number of `down` presses to
reach the START row (`ROW_START` is 7 of 8) plus a confirm from both players.

### 2. The planning docs are not trustworthy — read the code

`docs/AUDIT-2026-09-29.md` is the audit. Headline: four of the ROADMAP's "Next"
items (props as sim entities, Psycho Kinesis, jump zones, character select)
were **already done**. I twice recommended building duplicates on the strength
of that document. Two of its own claims were also wrong (a candidate filter
listed kind 58 where the code excludes 6; a "not built" camera feature that
was in fact an unmentioned addition of ours). Re-verify anything you read there.

### 3. A crop cannot show that something is off-screen

The top/bottom framing bug was "verified" with a crop of the character — of the
**wrong player**, the one that was fine. The full frame showed the entire
character below the bottom edge. Use full frames for framing claims.

## The one real remaining gameplay gap: skill family 100

`cargo test -p pd_game skill_coverage_audit -- --nocapture` prints the
distribution. Of 397 skills, **0** return `NotBuilt` and only 10 reach the
generic `Shell::PLACEHOLDER`. But that understates it — the real finding is
that **52 skills carry `FAMILY == 100`** (the largest family in the game) and
`moves/mod.rs` has no case for that value at all.

**Scope it before building.** It is not one missing attack type:

- **18 "special"** are *movement and state changes*: Dash, Jump, Levitate,
  Retreat, Return, Rebirth, Regenerate, Charge, Skywalk, Call, Surprise Attack
- **27 "attack"**: Pursuit, Ki Palm, Gungnir, Sky Pursuit, Ki Cannon, the
  egg-bomb cluster, the diffused-laser cluster
- 5 status, 1 erase, 1 defence
- Ids 733–741 are untitled and look like debris/trash/dust-cannon — possibly
  junk data

Most of the movement ones belong to a different subsystem from the one
`behaviour_of` dispatches. **Measure how many of the 52 are actually reachable
in a real arsenal first** (`deckdata_*.dat` / the arsenal legality checker) —
some may be unused in the real game, and that number sets the scope.

## Other open threads, honestly labelled

- **Destructible stages** — st04 `yuka`, st06 contaB collapse, st08 blocks.
  Needs the OA01/OE00 decode. Not started.
- **Per-prop throw strength is an assumption** — `props.rs::throw_strength`
  derives it from object size, not from the `.dat`. Flagged in the ROADMAP. If
  it's wrong, thrown objects hit harder or softer than the original.
- **Hit-stop freeze lengths are mine, not the original's.** 55/85/120 ms by
  damage, 35 ms for a block, KO at 0.35 speed for 1.6 s. Nothing in the binary
  gave a duration. If it reads as mush, that table is the first thing to change.
- **The `configdata.dat` pitch mapping is inferred, not proven.** ±70° is
  well-supported by the file's structure (78 floats, 6 blocks of 13, four
  columns constant) but the column identity is from symmetry, not from the
  program. Re-check this first if the camera misbehaves at the extremes.
- **Hit-stop and orbs are behind env gates on purpose** — the defaults are what
  was already played and tuned.

## Why the orbs are parked (don't rebuild this)

`CAPSULE-VISUALS.md` proposed using the 28 glow sprites in the `hud1` atlas.
Checked properly: the program's glow system `pdvkGlowExec` (`FUN_14028d7c0`) has
**exactly one caller**, a bit-flag-guarded one-shot init, and the capsule tick
function (`FUN_14014ed10`) contains **zero** references to it. Capsules don't
draw glow sprites. What they draw is already built from the decoded shader
recipe — a real sphere plus a rim cup in `orb_material.rs`. Those `hud1` sprites
are menu art in three colour families that don't match the seven capsule
colours. If the orb reads as dull rather than glowing, the fix is the rim's
opacity on existing code, not new art.

## The sound thread is a measured dead end, not an unfinished job

Documented in `FORMATS-AUDIO.md`. The 20% two-sound footstep split is **a
downward collision raycast, not a random roll** — `FUN_140230cd0` (Type1
classes, the wet one) is chosen only when the ground probe hits, so the 80/20 is
Lost Palace's geometry. And the data that creates it does not survive
conversion: every hull face in all 30 stage files has one of just four `attr`
values (`0x77FC`, `0x34`, `0x0`, `0x0B`), and neither class gate the code
branches on (`0x14`, `0x1f`) appears in any stage. Per-surface footsteps are
unreachable from converted data. Laurence's decision: ship the dominant hard
heel, and if a 20% synthetic variant is wanted, it needs a second clip that has
never been identified.

## Environment and multi-lane rules

- **Never `git checkout --` / `restore` / `reset --hard` / `stash` a file in
  this repo.** It exits 0 and destroys other lanes' uncommitted work. This
  happened today: a `git checkout -- main.rs` intended to strip five of my own
  lines destroyed a live lane's 259-line rewrite and 4 of its tests, visible
  only as the suite count falling 295→291. Use a hand-written patch against
  `git show HEAD:<path>`, or splice your lines into HEAD's text and
  `git update-index --cacheinfo` (never a canned `git add -p` answer sequence —
  it stages a neighbour's hunk).
- `crates/pd_game/src/main.rs` is permanently shared.
- `CARGO_TARGET_DIR` must be Windows form, never `/c/...`.
- Screenshot paths passed to the game must be Windows paths. The cache
  directory is **argv[1]** and must be an absolute path, or assets silently fail
  to load and you get a black frame with only the HUD on it. Characters are
  `pc00a`/`pc01a`, not names.
- `Time<Virtual>` is the clock you slow; anything that must keep counting while
  it is slow reads `Time<Real>`. `FixedUpdate` runs **before** `Update`.
- WGSL: no swizzle assignment, no `smoothstep(..).clamp(..)` chaining.
- Never commit `shots/`, `cache/`, or `game_files/`. Never push.

## If you take one lesson

Today I reported single-decompile reads and single-screenshot reads as
conclusions **and was wrong five times**: there were two footstep dispatchers
not one, a packed value's low bits are not a variant selector, props and
Psycho Kinesis were already built, the camera raycast is ours not the game's,
and 4× MSAA was already on despite my claiming otherwise. Every one of them
looked confident and passed a test.

So: **measure the thing, don't infer it, and say "I don't know" before building
on one read.** Prefer a histogram, a pixel measurement, or a probe over a
mechanism. A passing test next to a wiring line asserts the plan, not the
plan's assumption — and a test gated behind a flag nobody set is not a test at
all, it just reports a green that means nothing.
