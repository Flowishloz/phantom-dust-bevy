# Design: Replays

A replay is nothing new to build inside the sim: the sim is already deterministic (`sim.rs`'s contract — same
state + same inputs + same collision always gives the same result, no clocks, no unseeded randomness, no reading
the renderer). A replay is just **the match's starting setup plus the recorded list of every tick's quantised
input**, played back through that same `Match::step`. Recording is mechanical (the inputs already exist as a small
struct, `gather_input` already produces one per tick); the only genuinely new piece is a checksum to prove replay
and original agree, which doubles as an early-warning test for hidden non-determinism — the exact thing online
play will need fixed first (ROADMAP.md "Phase 4 — online prep").

## Where inputs are captured today

`main.rs::gather_input` (Update schedule) reads keyboard/pad state and writes `PendingInputs([Input; 2])` once per
frame; `step_sim` (FixedUpdate, 60 Hz) hands that same `Input` pair to `Match::step`. `Input` (`sim.rs`) is already
small and already documented as "exactly what will go over the network": `{ stick: [i8; 2], jump: bool, lock:
bool, skills: u8, cam: i8, recenter: bool }` — 2+1+1+1+1+1 = 7 meaningful bytes, no floats, no allocation. This is
the one and only thing a replay needs to store per tick.

## File format (versioned, small)

A replay file needs three things: a header identifying the exact match, the full input stream, and periodic
checksums to catch drift early rather than only at the end.

| Section | Contents |
|---|---|
| Header | format version (u8), characters `[String; 2]`, stage id, arsenals `[Vec<i16>; 2]`, `combat::Rules` (start level/hand/capsule timing, and Brawl's flags if that lands first), the initial RNG seed, engine build/version string (for "this replay may not match a newer build") |
| Input stream | one `Input` per tick, in order; at 60 Hz a 5-minute match is ~18,000 entries × 7 bytes ≈ 126 KB uncompressed — small enough to not need a fancy format |
| Checksums | one `(tick, hash)` pair every N ticks (see below) |

A simple length-prefixed binary encoding (`Input` deriving `serde::Serialize`/`Deserialize` via `bincode`, or a
hand-rolled byte writer given how small `Input` is) is enough; no need for a text format. The version byte lets a
future format change fail loudly ("this replay is from an older build") instead of silently desyncing.

## `Match::checksum()` design

A hash over the parts of `Match`/`Combat` state that determine everything downstream: both players' position,
velocity and yaw; both fighters' health, level, aura, slot contents, action state and timers; the capsule and
projectile lists; the props/objects state; the RNG value; the tick counter. Floats are hashed via `f32::to_bits()`
(never formatted as text, which can differ across platforms) so the hash is exact-bit, not "close enough."

```rust
impl Match {
    pub fn checksum(&self) -> u64 {
        // FNV-1a (or similar) over to_bits() of every f32, plus every int/enum
        // field, in a fixed field order — same order every call, same binary.
    }
}
```

Call it every **60 ticks (1 second)** by default (configurable) during both recording and playback; store the
`(tick, hash)` pairs in the replay file. On playback, compare each new hash against the stored one — a mismatch
immediately names the first tick that diverged, instead of "the replay looked wrong somewhere," which is exactly
what a determinism bug hunt needs.

## What currently breaks determinism across runs/machines

The sim's own doc comment already flags float determinism as same-binary/same-CPU-architecture only (`sim.rs`,
`GAME.md` "Determinism contract"). Concretely, today's trig call sites in the sim's own code (not render-only
code) are:

| File : line | Call | Used for |
|---|---|---|
| `sim.rs:303` | `atan2` | facing/yaw from a direction vector |
| `sim.rs:323` | `sin`/`cos` | turning a facing angle into a movement vector |
| `combat.rs:1911` | `sin`/`cos` | area/projectile placement math |
| `moves/arc.rs:63` | `sin` | Arc family's curved flight (Laser/Slide Laser's bend) |
| `moves/barrier.rs:139` | `sin`/`cos` | guard shape placement |
| `moves/special.rs:325` | `sin`/`cos` | a special-family placement |

None of these read the renderer or the clock, so same-binary/same-machine replay (record and play back on this
one PC) is safe today — which is all a *replay* feature needs. They matter for **cross-platform** determinism
(a different CPU/compiler producing a one-bit-different `sin` result that compounds over ticks), which is why
ROADMAP's Phase 4 online-prep list calls for replacing them with unit-vector facing math before any lockstep/
rollback netcode — replays are the cheap way to find out if that risk is real before online work starts.

Other things checked and already clean:
- **Time-based code**: nothing inside `Match::step`/`Combat::step`/`moves/*` reads a clock; `moves/*.rs` files
  carry an explicit doc comment ("Deterministic: no clocks, no HashMap iteration; random picks use [the seeded
  rng]"). The one place `time.elapsed_secs()` is read (`main.rs`'s `--demo` test-bot script) only decides what a
  *test* input generator presses — the resulting choice still goes through the ordinary quantised `Input`, so it's
  recorded and replayed like any other input; it just means a `--demo` run isn't itself a meaningful "replay" of
  real play.
- **HashMap iteration order**: `combat.rs` has two `HashMap`s (`clip_ticks`, `action_rows`) but both are static
  lookup tables loaded once at startup and only ever read by key, never iterated over for a per-tick decision —
  safe as-is. Worth a one-line rule for future skill/effect work: never add a `.values()`/`.keys()`/`for (k, v)
  in map` loop whose *order* changes sim state.

## Playback UI (from the result screen)

`frontend/result.rs` (shown after a match ends) is the natural place to offer "Watch Replay": it already has the
`MatchSetup` and final state on hand. Proposed flow: Result screen → "Watch Replay" → a new `AppState::Replay` that
re-runs `enter_match`'s setup (same characters/stage/rules) but drives `step_sim` from the stored input stream
instead of `gather_input`, at normal speed (later: pause/scrub, out of scope for a first version). Falling off the
end of the input stream returns to the Result screen.

## Tests

- **Round-trip**: record a scripted match (the existing `--demo` bot, or a fixed `PD_TEST_PRESS` script), replay
  its input stream through a fresh `Match`, assert every stored checksum matches and the final state
  (`Match::checksum()` at the last tick) is bit-identical. This is close to the determinism test already listed in
  GAME.md's `cargo test -p pd_game` line ("replay determinism") — this feature turns that existing internal test
  into a real file format and a player-facing button.
- **Version guard**: a replay with a bumped format version is rejected with a clear message, not a panic.
- **Divergence detection**: deliberately mutate one recorded input mid-stream in a test and confirm playback flags
  the exact tick where the checksum first disagrees.

## How this prepares online

This is Wave 5C's stated purpose (PLAN.md): "if a replay ever desyncs, that's a determinism bug to fix before
[online] starts." Recording/replay, the checksum, and `serde` on `Input` are three of the four items ROADMAP.md's
"Phase 4 — online prep" already lists as prerequisites for lockstep/rollback netcode — building replays now does
that work once, for both features, instead of twice.
