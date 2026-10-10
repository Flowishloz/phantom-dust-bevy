# Attack-skills audit vs the original (2026-09-30, cloud, read-only)

Document-and-code audit only: no game data, no Ghidra export, nothing built or run. "Program read" = what `PROGRAM-RE.md` (PR), `FORMATS-SKILLS.md` (FS), `MOVES.md`, `MOVES-PLAN.md` or `FOOTAGE.md` (FT) state; nothing here is a new fact about the program. Every "check on PC" names functions from PR. Verdicts: MATCH / ADDED BY US (our invention, program silent) / DIFFERS (program read says otherwise) / UNKNOWN (docs do not settle it). Code paths are under `engine/crates/pd_game/src/`; `mv/` = `moves/`. Tick = 1/30 s in the program, our sim runs 60 Hz (`sim.rs:12`).

## 0. The headline: the shot modules still implement the pre-phase-2 readings

`mv/parabola.rs`, `mv/arc.rs`, `mv/crawl.rs`, `mv/fall.rs`, `mv/ricochet.rs`, `mv/shell.rs`, `mv/area.rs` were written on 2026-09-26 from the first s64-s69 guesses. PR phase 2/3 (2026-09-27) and FS "Program evidence" then corrected most of them and the code was not revisited (only melee/cast timing were rebuilt). MOVES.md rows still describe the old readings, so the docs and code agree with each other but disagree with PR. Laurence should treat every projectile *path* as placeholder and every *timing* (fire tick, clip hold, dash/hit window) as program-based.

## 1. Unit guesses (every one)

| Guess | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Speed 3 u/s per point | s/10.8 u per tick = 2.78 u/s per point, float `0x1402f7cd8` (PR "Units"; FS "Program evidence") | `mv/mod.rs:39` `TABLE_SPEED` = 0.1 x 30 | DIFFERS (8 % fast; not yet fixed though the docs say "not 3") | None needed for the number; confirm the *same* 1/10.8 applies to Fall/Crawl/Arc/Tornado (PR says yes per handler). |
| Homing 0.1 rad/s per point | s x 0.1 degrees per tick x homing % = 3 deg/s = 0.052 rad/s per point (PR "Units") | `mv/mod.rs:43` `HOMING_PER_POINT` = 0.1 rad/s | DIFFERS (1.9x too sharp) | `FUN_140264710` (shell update): confirm degrees not radians, and that Arc s66 / Fall s67 / Parabola s67 use their own scales (they do in PR, ours reuses one constant). |
| Distance 1/10 u | 0.1 u for reach, blast, hole radius, mine radius (PR) | `mv/mod.rs:36` | MATCH | - |
| Time 1/30 s, x2 sim ticks | confirmed by notes ("10 sec" = 300) | `mv/mod.rs:33` | MATCH | - |
| Parabola throw speed s64 x 1 u/s | s64 / 10.8 per tick horizontal (PR "Motion 1") | `mv/parabola.rs:16` `SPEED_PER_POINT` = 1.0; flight time then clamped 0.5-1.6 s (`:18-19`) | DIFFERS | `FUN_140264b00` / `FUN_140265080`: horizontal speed vs aim distance -> flight time T; there is no 0.5-1.6 s clamp in the read. |
| Parabola arc = gravity 150 "by eye" | height s66 x 10 u at mid-flight, g = 8h/T^2 (PR) | `mv/parabola.rs:23` GRAVITY 150, fixed | DIFFERS | Same pair. Our Bomb peaks ~40 u by luck (s66 = 4 -> 40 u); Throwing Stone should peak 10 u, Delay Bomb 100 u. |
| Parabola steering = s66 x 0.1 rad/s | s66 is arc height; steering is **s67** / 10.8 x homing % per tick, sliding sideways (`FUN_14026bf50`) | `mv/parabola.rs:37` `param(2)` = s66 | DIFFERS (wrong field; the code comment still says "s67 unknown") | `FUN_14026bf50`: predicted-landing slide; "keeps last correction once below target height". |
| Arc bend s67 deg, fan step s68 | s67 = bend, **s68 = tilt of the arc's plane (roll)**, s66 = homing, per-shot tilt s68 + k x s71, bend s67 + k x s70 (PR "Arc additions") | `mv/arc.rs:39-42` step = abs(s68) as a yaw fan; count = s69 | DIFFERS (see 3.3) | `FUN_1402666b0` / `FUN_140266bf0`. |
| Crawl homing cap 4 rad/s, Tornado 1.5 rad/s | turn accelerates s68 x 0.01 deg/tick^2 x homing % up to s69 x 0.1 deg/tick (Crawl); **Tornado has no homing** (PR) | `mv/crawl.rs:22-23` | DIFFERS (both constants are ours) | `FUN_140266290` (crawl update), `FUN_140268940` (tornado update). |
| Tornado speed = s64 u/s | s64 is lifetime; speed ramps s67 -> s68 over s69 ticks (PR "Motion 10") | `mv/crawl.rs:34` | DIFFERS (guess that PR replaced) | `FUN_140268590`. |
| Fall speed s66 x 3 | s66 / 10.8 per tick (PR) | `mv/fall.rs:26` | DIFFERS (same 8 %) | - |
| Fall height 150 u straight above | s64 x 10 up and s65 x 10 back toward caster, diagonal flight (PR) | `mv/fall.rs:16` `HEIGHT`, straight down | DIFFERS | `FUN_140265470` / `FUN_1402657f0`. |
| Ricochet speed s65 x 3, fan +-45 flat | speed ramp s65 -> s66 over s67, X pattern by roll s69 + k s72 then yaw s68 + k s71 (PR "Motion 17") | `mv/ricochet.rs:28-31` yaw-only fan | DIFFERS | `FUN_14026af40`. |
| Melee reach s64 x 0.1 from caster centre | punch: sphere r = s64 x 0.1 on the SHOT01 marker / caster matrix + body radius; sword: blade segment of that length tested at 12 u around target centre (PR motions 12/13) | `combat.rs:1600-1603` flat distance <= reach + BODY_RADIUS from caster pos; `mv/melee.rs:28-30` | DIFFERS (semantics; FS row still says "reach") | `FUN_14026c730`, `FUN_14026c340`; which bone `+0x13db8` is (PR marks it). |
| Dash 240 u/s constant | s65 x 10 / dash ticks per tick, cap 10 u/tick; turn cap s66 deg/tick (PR) | `combat.rs:103`, `:1592` | DIFFERS for any row whose window is not 10 ticks (kicks 12 ticks, ATK_BODY 10, WHAND_SORD 13); no turn cap | `FUN_14025be40`, `FUN_14018f050` / `FUN_14018f0f0`. |
| Hit radius of a projectile 2.5 u | swept sphere 5 u + body radius (PR impact codes) | `combat.rs:112` `PROJECTILE_RADIUS`; body radius `sim.rs:86` 4.5 (ours) | DIFFERS (half) | `FUN_14026c1c0`: the program's body radius value (PR gives none). |
| Shell range 900 u | s64 < 0 -> range = -s64 x 10 u, lifetime = range / speed; s64 >= 0 = lifetime ticks (FS "Program evidence"; Blaster 300 u = 23 ticks) | `mv/shell.rs:12`, `:40` | DIFFERS (Blaster flies 3x too far; FT claim 9 could not test) | `FUN_1402636f0` (end of life), `FUN_140264570` init. |
| Shell speed ramp / wait | s67 != 0 -> speed ramps s65 -> s66 over s67 ticks; Psycho Sniper does **not** wait (FS) | `mv/shell.rs:34-36` waits s67 then flies at s66; MOVES.md still says "waits" | DIFFERS | `FUN_140264710`. |
| Shell homing always on | only while target within 60 deg of heading (dot > 0.5); otherwise straight (PR pilot) | `combat.rs:2095` `steer` always | DIFFERS (Blaster 86 deg/s vs 45 deg/s) | `FUN_140264710`. FT claim 10 "can't tell". |
| Placeholder shell 170 u/s, 1.5 rad/s | no such thing; motions 18/19 have no handler | `mv/shell.rs:31`, used at `mv/mod.rs:215` for every unbuilt attack (Impact Burst, Diffused Laser rows, etc.) | ADDED BY US | Which attacks actually fall through (motion 17 with s67 != 0, motion 3, 4 attacks). |
| Charge 1 Aura per 20 sim ticks | 1 Aura per 10 ticks after a w39 = 13 tick wind-up (PR "Charge, finished") | `combat.rs:668` `CHARGE_TICKS_PER_POINT` = 20 | MATCH on rate; DIFFERS: no 13-tick wind-up before counting (`combat.rs:1181`) | `FUN_14025c720`; whether status 30 halves/doubles the charge cost. |
| Octolaser up to 8 damage per cast | one object per SHOT marker, one hit per object per player (PR); each shot's STR = s46 (Octolaser s46 not stated in docs) | `mv/arc.rs:39` count = s69 | UNKNOWN damage | Octolaser s46 in `skills.json`; hits after a knock-down are gated by s62 (PR "s62"). Ours applies that gate (`combat.rs:1699-1702`), so it likely matches once shots exist; PR says STR-1 shots break a DEF-3 wall on the 4th hit (guard lane, not re-checked). |

## 2. "Tuned by eye" / engine-side constants (no program source)

| Constant | Where | Program says | Verdict |
|---|---|---|---|
| Parabola min/max flight 0.5 / 1.6 s | `mv/parabola.rs:18-19` | nothing; T follows speed and distance | ADDED BY US |
| Parabola gravity 150 u/s^2 ("half the jump gravity") | `mv/parabola.rs:23` | per-shot g from s66 (PR) | ADDED BY US |
| Arc OVERSHOOT 300 u straight after the circle | `mv/arc.rs:19` | "straight for 30 ticks" (PR pilot: 390/440 u) | DIFFERS (close for Laser, 300 vs 389 u) |
| Arc MAX_BEND 80 deg clamp | `mv/arc.rs:21` | none read | ADDED BY US |
| Crawl MAX_RANGE 700 u | `mv/crawl.rs:20` | s64 (-80 -> 800 u, Crawling Bullet -30 -> 300 u) | DIFFERS |
| Ricochet MAX_RANGE 900 u, BOUNCES 3, COUNT 4 | `mv/ricochet.rs:14-17` | lifetime s64 (Ricochet 90 ticks = 3 s), s73 = bounces (3 coincides), count = SHOTnn markers | DIFFERS (range), MATCH by luck (bounces/count) |
| Mine life 30 s | `mv/area.rs:21` | not read | ADDED BY US |
| Rain height 150 u | `mv/area.rs:23` | cloud 100 u (s69 x 0.1) over target, drops fall 130 u | DIFFERS |
| Default radius 30 u / default reach 14 u | `mv/area.rs:25`, `mv/melee.rs:12` | n/a (fallbacks when field is 0) | ADDED BY US |
| Shot spawn at chest (+12 u) toward the target's chest | `combat.rs:114`, `mv/mod.rs:273` | spawn at the `SHOTnn` marker position/orientation in the cast effect (`FUN_14018a310`, `FUN_1402626d0`) | DIFFERS |
| Shot hangs `s.life.max(90)` after a Course/About Face | `combat.rs:2091` | "60 more ticks" for a brushed shot only (PR code 7) | ADDED BY US |
| Reflected/About Face shot homing floor 1.5 rad/s | `combat.rs:2079` | re-aimed at owner, slot 1 re-created (PR codes 5/6) | ADDED BY US |
| Ricochet bounce direction (tries 4 axis flips, then reverse) | `combat.rs:2329-2338` | reflects off the wall normal (`FUN_14026ca50`); our data has no normals (MOVES.md) | ADDED BY US |
| LUNGE_MAX_TICKS 36, melee fallbacks when no action row | `combat.rs:111`, `:1574-1576` | rows exist for every used skill | ADDED BY US (only for test fixtures / 14 enemy skills) |

## 3. Rules, family by family

### 3.1 Cast timing and dispatch (action table, w-columns)

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Fire at the row's w43 tick, not a flat tick | `FUN_14025a3e0`, `FUN_14025c720` (PR "Cast") | `combat.rs:1362`, `:1407`, `:1216` (x2 to sim ticks) | MATCH | - |
| Caster held for the whole clip (w38 length) | state timer = clip length (PR; pose length only weakly supported, FT "can't tell", Blaster ~1.33 s vs 1.5 s) | `combat.rs:1406` `clip_ticks.max(fire_ticks)` | MATCH (evidence weak; clip length source = our glb at 30 fps) | FT asks: a clean "cast start to free" clip; PR `FUN_14025bd40`. |
| Mid-air row: s13 when s40 = 1/3, or 2 while jumping/falling | PR "Dispatch map" (states 5 or 7) | `combat.rs:856-868` uses `airborne` | MATCH | Does `airborne` equal states 5 **and** 7 (fall and float)? `FUN_140258180`. |
| Cast yaw | keeps turning to face the target at the character's turn rate during the cast (`FUN_14025bd40`) | `combat.rs:1365`, `:1474`, `:1586` snap yaw to the target instantly at cast, fire and every lunge tick | DIFFERS | Turn-rate limit during cast; is aim taken at the press or at the fire tick? (`FUN_140258180` picks the dash target at the press; `FUN_1402626d0` creates objects at fire.) |
| Shots aim at the opponent whether or not locked on | target = lock target; with no target the aimed moves use 190 u (s44 = 0) / 300 u (s44 = 1) ahead (PR "Attack s44") | `combat.rs:1356`, `:1472` always `players[1 - i]` | DIFFERS / UNKNOWN for unlocked casts | `FUN_140252a20`: is the lock auto-acquired (option `0x14052dfb8`) so "no target" never happens in versus? |
| Hit cancels an unfired cast and ends live melee | `FUN_140258690` clears +0x68; `FUN_14026f430` | `combat.rs:1207-1209`, `:1618` | MATCH | Is the skill still spent? (PR: not read) Ours spends at the press (`:1385`). |
| Air cast of a skill with no air row | s13 = 0 -> not castable? | falls back to the ground row (`combat.rs:865-866`) | UNKNOWN | `FUN_140258180` for s40 = 2 with s13 = 0. |

### 3.2 Shell (motion 0), range bands, homing, Course

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Straight shot at target, homing | yes, 60 deg cone | `combat.rs:2095` | DIFFERS (cone) | see section 1 |
| Range band s44 (80-300 / 300-600 / any) clamps the **aim point**; clamped aim drops the target (no homing) | `FUN_140273d40` via `FUN_14026e410` (FS s44 row, PR "Attack s44") | **Not implemented anywhere** (grep: no attack reads s44) | DIFFERS (missing). Consequence: a Laser at 150 u hits in ours; in the original its circle closes at 300 u and passes ~26 u to the side | Applies to Parabola, Fall, Arc, both Rains, Hole, and motion-4 instants; confirm Shell (motion 0) does not use it. |
| "Penetrates obstacles" (s55 = 3): no wall, no LOS test | PR "Walls and line of sight" | `mv/mod.rs:161`, `combat.rs:2126` | MATCH for 3 | s55 = 2 (walls ignored ⚠️, Giga Ruin / Particle Mortar) and 1 (Ricochet, other query ⚠️) are not modelled; `FUN_14026ca50`, `FUN_1401641b0`. |
| Shot stopped by wall: segment test last -> this tick | `FUN_14026ca50` | `combat.rs:2126-2141` | MATCH | - |
| Body hit needs clear line from the object's origin to the target | PR | no separate LOS test (wall raycast per step instead) | UNKNOWN (equivalent for straight shots; not for arcs/lobs) | `FUN_1401641b0`. |
| Course field makes shots steer **away** at their own homing rate | PR "Course deflection" | `combat.rs:2075-2091` bends aside with a fixed rotation, homing 0 | DIFFERS (guard lane; listed because it is shot behaviour) | `FUN_140271760`. |
| One hit per object per player; ends on first hit | `FUN_14026ecc0` | `combat.rs:2143-2146` | MATCH | - |
| Owner immunity | cleared at init except mines, wall-impact blasts, Tornado after 30 ticks | two-player sim: victim is always the other player (`combat.rs:1690`) | ADDED BY US (self-hits impossible by construction) | Decide whether versus needs owner-hurting blasts; would need the sim to test the caster. |
| Hit reaction on a hit shot | per `s57-s59` | via `strike` | MATCH (other audit) | - |

### 3.3 Arc (motion 6) and multi-shot fans

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Laser leaves tilted s68 deg about its own axis, bends s67 deg in that tilted frame (20/20, 20/40) | PR pilot; FT claims 6-7 qualitatively confirmed (up-right then hooks back left) | `mv/arc.rs:60-70` flat circle in the ground plane, vertical component linear | DIFFERS | Which side and sign of s68: FT saw up-and-right first. `FUN_140266bf0`. |
| Arc re-aims at the target's current position each tick, at most s66 x 0.1 deg x homing % | PR pilot | `mv/arc.rs:66-67` `homing: 0.0` (aims at where the target stood) | DIFFERS (FT claim 7: no strafing-target footage) | Footage: laser at a strafing target. |
| Curve time = arc length / speed, then 30 ticks straight | PR | `mv/arc.rs:62-63`, `:76` | MATCH in shape | - |
| Shot count from `SHOT01..n` markers in the cast effect (Double 2, Triple 3, Octo 8) | `FUN_14018a310`, name mask `0xB3D8C500` (PR "Multi-shot") | `mv/arc.rs:40` from s69, clamped 1-8 | DIFFERS in source (same numbers today) | FS says "s69 picks only per-shot angles"; confirm s69 is not read at all. |
| Fan = planes rotated by s71 per shot (Double -15/-165, Triple -10/-90/-170, Octo 22 - 32k); bend s67 + k s70; bend -25 | PR | `mv/arc.rs:44-50` symmetric **yaw** spread by |s68|; `s70_73` is in `skills.json` but unused | DIFFERS | `FUN_140266bf0`: sign/order of roll vs bend. Octolaser would currently fan left-right, not over a 224 deg roll range. |
| Shot index k = caster's running counter + marker index mod count | PR ("fighter +8, never reset") | not modelled | DIFFERS | Only matters for which angle each marker gets. |
| Range band 300-600 u on Optical lasers | PR | absent | DIFFERS (3.2) | - |
| Bend cannot pass 90 deg | ours | `mv/arc.rs:21` | ADDED BY US | - |

### 3.4 Parabola / lob, Fall, Crawl, Tornado

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Lob lands on the target (centre through the range band 80-300, mortars 300-600) | PR motion 1 | lands on the target's chest, no band (`mv/parabola.rs:43-46`) | DIFFERS | `FUN_140273d40`. |
| Sideways steering toward target, then keeps last correction once below target height | PR `FUN_14026bf50` | `combat.rs:2095` flat `steer` for the whole flight at s66 x 0.1 rad/s | DIFFERS | - |
| Lifetime T + 30 ticks; ends on floor/wall or hit; bombs blast | PR | life (t + 1) s (`mv/parabola.rs:58`), blast via `blast_radius` | DIFFERS (small) | - |
| Fall diagonal, steering s67 / 10.8, lifetime 2x flight distance, range band | PR motion 2 | straight down, no steering, no band | DIFFERS | `FUN_1402657f0`. |
| Roof stops a Fall unless s55 = 3 | PR | raycast per step (`combat.rs:2126`) | MATCH | - |
| Crawl: heading = caster facing flattened; speed ramp s65 -> s66 over s67; only steers while on the floor; falls off edges and re-snaps; ends at walls or range | PR motion 5 | aims at target (equal to facing here), constant speed, ends at a gap (`combat.rs:2104-2121`) | DIFFERS (ramp, acceleration turn, edges) | PR marks floor-snap vs wall interplay unread. |
| Crawl misses a target in the air | guide text | 3D body hit, floor-following y | MATCH (by effect) | - |
| Tornado: no homing, ramps s67 -> s68 over s69, column radius s65 x 0.1, height s66 x 10 - 2, passes through players, hits own caster after 30 ticks, reaction 45 straight up | PR motion 10 | folded into Crawl with homing 1.5 (`mv/mod.rs:207`); knock-down "not built" (MOVES.md) | DIFFERS | - |
| [Object] (Psycho Kinesis, Power): lift s68 x 0.1 over s69 ticks, then fly s65 -> s66, 60 deg cone, turn cap s71 | PR motion 11 | shell stand-in + `props::THROW_*` (`combat.rs:1484-1501`) | DIFFERS / UNKNOWN (constants in `props.rs`, not audited) | `FUN_1402694e0`; s71 argument is ⚠️ in PR. |

### 3.5 Areas: Rain, Hole, mines, waves, blasts

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Rain hits **once per cast** | shared hit mask, `FUN_14026ecc0` (PR: "our choice was right") | `combat.rs:2245`, `:2263-2266` (`struck`) | MATCH (MOVES.md "Open questions" still says "the original may hit per drop": stale) | - |
| Rain (motion 20): cloud 100 u above target, follows at 42 u/s for 60 ticks; 6 drops from tick 30, one per 6 ticks, each at a random point within 30 u, 5 u sphere, 250 u/s | PR `FUN_14026b480`, `FUN_14026b8d0` | one instant disc at the target's spot at fire time, radius = s65 (30 u), pulses timed s67 (`mv/area.rs:86-95`); target inside the disc is always hit | DIFFERS (hit chance: ours certain if still, original only if a random drop lands within ~9.5 u of them) | Seed: sim needs own RNG (`Combat::random`) for drop positions. |
| Rain column (motion 7: Lightning Bolt, Thunder Storm, Killer Satellite): warning s64 with sliding marker, active s65 = 10 ticks, column 60 u tall, radius s68 x 0.1, roof ends it | PR `FUN_140267410` | treated as motion 20: radius = **s65 = 10 (active ticks misread as radius)**, pulses = s66 (= column height misread as drop count) | DIFFERS (both fields wrong; Lightning Bolt radius 10 u vs 30 u) | `FUN_140266fb0`. |
| Rain needs open sky; only Shelter stops it | PR: Barrier and Reflect never stop Rain/Tornado | `mv/area.rs:95`, `combat.rs:2251-2255`, `from = None` | MATCH | - |
| Hole: marker grows over s64 and slides with the target's run; active s65 = 10 ticks, sphere s66 x 0.1 + body radius, grounded players only; motion 16 = a hole under every player, no range limit | PR motion 15/16 | one instant strike at the target's first-position marker (`mv/area.rs:79-84`; s65 unused), `ground_only` by `STEP_HEIGHT` (`combat.rs:2256`) | DIFFERS (no slide, no 10-tick window, no "every player") | `FUN_14026a510` / `FUN_14026a7f0`; "grounded" = player +0x12e74, is it the jump state or floor contact? |
| Mines: Timer s66 fuse (Timer Mine 150 = 5 s, note-confirmed), Sensor armed after s66 = 60 then centre within s65 x 0.1 with clear line, Remote on owner's next press; set at fire tick 4 | PR motion 8 | same fields (`mv/area.rs:99-111`); sensor horizontal distance only, no LOS | MATCH (fuse, radius, kinds); DIFFERS: no LOS test (`FUN_14026e340`) | Blast radius s67 x 0.1 (Remote 30 u) is read in `dist(3)`. |
| Mine blast and Sensor Mine hurt the **owner**; Sensor is set off by anyone | PR | only the opponent (`combat.rs:2225`, `:2248`) | DIFFERS (MOVES.md row says "only the opponent is hurt"; contradicts PR) | Decide with Laurence: faithful self-damage is a rules change. |
| Blasts: a separate object living 30 ticks, sphere grows 0 -> radius linearly, tested every tick with LOS from the centre | `FUN_14026dbd0` / `FUN_14026ae10` | instant full sphere at the moment of impact (`combat.rs:2172-2184`, `:2241-2268`) | DIFFERS (Bomb catches a near miss at once; original only as the sphere grows) | Sphere growth vs a target that moves away within 0.5 s. |
| Wall-impact blast hurts the caster; blast skips the player already hit | PR impact codes 1/2 | neither | DIFFERS / ADDED BY US | see owner immunity |
| Around self (motion 9: Photon Wave/Burst, Fast Wave): blast at spawn, growing 30 ticks, caster excluded, fires tick 3 | `FUN_140268050` | `mv/area.rs:112` timer area, instant sphere at the action row's fire tick | MATCH timing (row-driven), DIFFERS growth | - |
| Motion 4 (instant, no travel; s64 = 0 needs target within 300 u; 2 = everyone; effect s65 ticks after fire) | PR | `mv/effects.rs` delivery (other audit) | UNKNOWN here | `FUN_140265ea0`. |

### 3.6 Melee (sword, punch, kick, palm, dash windows, Pursuit)

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Dash only if locked on and target within 80 u **at the press** | `FUN_140258180`, lock band 1 (PR "Lock-on"); FT round 2 confirms no dash when unlocked | `combat.rs:942-945` (flat distance), judged once at press or charge release | MATCH (flat vs 3D distance UNKNOWN) | `FUN_1402531d0` distance metric. |
| Dash window w41..w42, hit window w43..w44 (2-5 ticks; two-handed swords 14-34), hit whether or not the dash arrived | PR | `combat.rs:1574-1603`, windows x2 | MATCH | FT: Psycho Knife timing confirmed to ~1 frame (round 2). |
| Stops 11 u from the target's centre | PR ("a quirk", can end slightly inside) | `combat.rs:106`, `:1591-1592` | MATCH | - |
| Dash speed and turn cap | s65 x 10 / ticks (cap 10 u/tick), s66 deg/tick (swords 1, punches 3) | 240 u/s flat, yaw snaps | DIFFERS | see section 1 |
| Hit = 13 u sphere on the weapon marker; sword = blade segment, 12 u fixed radius, swept last -> this tick; LOS from chest | `FUN_14026c340`, `FUN_14026c730` | point-to-point horizontal distance <= reach + 4.5 from the caster's feet, vertical gate `BODY_HEIGHT` (`combat.rs:1601-1603`) | DIFFERS | FT asks for footage to measure stop-short and reach (round 1/2 "can't tell"). |
| Hits at most once per press | one object per press | `struck` flag (`combat.rs:1600`) | MATCH | - |
| Pursuit / Sky Pursuit: dash ticks 3-21, single test at tick 19, 25 u sphere around caster, downed targets only, reaction 24 | PR motion 14 | generic melee machinery with the action row; `only_hits_downed` | MATCH by row timing; DIFFERS in single-tick test (ours tests the whole hit window) | `FUN_14026f870`. |
| Motion 3/12/13/14 against Absorb: short-range is blocked entirely | PR guard table | `barrier.rs` (guard lane) | UNKNOWN here | - |

### 3.7 Charge, hold, combos

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Hold to charge: w39 ticks wind-up, then +1 per tick while held; cost check base + count/10 vs Aura; count stops when unpayable; on release the clip restarts at w40 and the shot leaves w43 - w39 = 12 ticks later; STR = base + count/10 | PR "Charge, finished" (`FUN_14025c720`, `FUN_1402620e0`, `FUN_1402626d0`) | `combat.rs:1178-1190` (count from press, per 20 sim ticks), `:1448-1467` (12 ticks after release), STR = `spent` | MATCH on rate, release timing, STR; DIFFERS: no wind-up, clip does not resume at frame w40 (renderer not audited) | PR line 62/430 (w39 = "charge cycle ⚠️") contradicts PR 578 (finished): trust 578; verify `FUN_1402620e0` re-arm. |
| Which skills charge | rows with w39 != 0 (Gungnir, Ki Cannon, Young White Dragon, Dragon Bullet 13/15/25; Ki Palm 7/9/20) | `mv/mod.rs:121-123` derived from s45 = 21 and cost 0, not w39 | ADDED BY US (heuristic, matches the named skills) | Use w39 != 0 instead; Ki Palm w39 = 7 not 13. |
| A tap | STR = 0 (count 0) | same | MATCH | - |
| Combos: press accepted between w45..w46 of the current clip if the next skill's record is in s27-s36; next cast starts at w47; later hits use s14..s17 rows; High-speed Punch, Rapid Cannon, Trench Mortar | PR "Combos" (`FUN_14024fb90`) | **no combo code** (grep: none in `combat.rs` / `mv/`) | DIFFERS (missing) | Does each chained hit pay Aura again? (PR: ❓) |
| Finisher STR = 2 x Level | PR / notes | `combat.rs:1376` | MATCH (not in this audit's scope) | - |

## 4. Doc-versus-code and doc-versus-doc contradictions

1. **MOVES.md "Families" rows** for Shell, Parabola, Arc, Hole, Rain, Fall, Crawl, Ricochet still quote the 2026-09-26 readings (3 u/s, 0.1 rad/s, range 900, waits s67, s66 steering, gravity 150, straight Fall, yaw-fan Arc). PR phase 2/3 and FS "Program evidence" replace all of them; FS itself says "the rows themselves are left as they were" (FS main table, lines 84-92).
2. **MOVES.md "Open questions"**: "Rain lands once per cast (a choice)", "s67 might be gravity", "units are a guess" are all answered in PR (once per cast confirmed; s66 = arc height, s67 = steering; 2.78 u/s and 3 deg/s per point).
3. **FS "Units"** says speed = 3 u/s "assumed" and homing 0.1 rad/s "a guess"; the same file's "Program evidence" says "2.78 (not 3)", "3 deg/s (not 0.1 rad/s)". The code comments (`mv/mod.rs:37-43`) cite the old text.
4. **FS s64-s69 table**: Arc s68 "spread", Rain s65 radius, Hole s65 "10 unknown", Crawl s68 "homing" vs FS "Program evidence, phase 2" (tilt, active ticks, turn acceleration).
5. **FS "Units" / MOVES-PLAN "Unknowns 2"**: "Range bands: no field found" vs FS s44 row and PR: s44 is the band (implemented nowhere).
6. **MOVES-PLAN "Where things stand"** (Arc "straight projectile", Rain "no", etc.) is stale; MOVES.md marks all built.
7. **PR** `w39` row (line 62, "charge cycle ⚠️") and the phase-2 charge note (line 430) versus "Charge, finished" (line 578): same file, later section supersedes.
8. **MOVES.md Mines** ("only the opponent is hurt") vs PR ("hurts everyone including the owner", Sensor set off by anyone).
9. **MOVES.md Arc** ("s69 count, fan step = |s68|") vs FS/PR ("counts from SHOTnn markers", "s68 = tilt, not a fan step").
10. **MOVES.md Short-range** "reach = s64 x 0.1 (unconfirmed)" vs PR motion 12/13 (hit-sphere radius / blade length, read).
11. **MOVES.md Crawl** ("homing from s68 capped at 4 rad/s") vs PR (turn acceleration s68 x 0.01 deg/tick^2 up to s69 x 0.1 deg/tick).
12. **FS s55**: "2 = walls ignored ⚠️" vs code `ignores_walls` = only 3.

## 5. Ranked PC checks for this area (Ghidra, `engine/re/decomp_fn.py`)

1. **Retune to read values, no new Ghidra needed**: 2.78 u/s and 3 deg/s constants, shell range from s64, Parabola s66/s67 fields, Fall diagonal, Crawl ramp/acceleration, Tornado separate from Crawl, Rain column fields (s65 active, s68 radius), blast growth. These are all stated in PR; the code simply predates them.
2. **Range band clamp** (`FUN_140273d40`, `FUN_14026e410`): biggest gameplay effect (Lasers and Mortars cannot hit close targets). Needs "no target" fallback (190 u / 300 u) and lock behaviour (`FUN_140252a20`).
3. **Arc roll/tilt and SHOTnn fans** (`FUN_14018a310`, `FUN_1402666b0`, `FUN_140266bf0`): where SHOT markers live in our converted effects (FORMATS-EFFECTS.md) and sign conventions of roll; FT claim 7 (which way a Laser leaves) is the only footage check.
4. **Melee hit geometry and dash** (`FUN_14026c340`, `FUN_14026c730`, `FUN_14025be40`): weapon marker bone, body radius value, dash turn cap. Footage round 1/2 asked for a caster-side Psycho Knife clip (FT "What Psycho Knife footage should show next").
5. **Owner-hurting blasts / mines** (`FUN_14026dbd0`, impact code 1/8): rules decision, not only a read.
6. **Combos** (`FUN_14024fb90`): not built; three player skills use them.
7. **Hole/Rain-column marker sliding** (`FUN_14026a7f0`, `FUN_140267410`) and Rain cloud drops (`FUN_14026b8d0`).
8. Footage that would settle things without Ghidra: Laser at a strafing target; Blaster at a far target (vanishes at ~0.77 s?); Blaster at a target 60 deg to the side (homes?); Octolaser fan from behind; Lightning Bolt against a standing vs walking target.

## Not audited here
`main.rs` shot visuals (effect slots, trail), `props.rs` constants, guard resolution (`mv/barrier.rs`), status/erase capsules, hit reactions; the reticle/HUD range colours (MOVES-PLAN "Unknowns 2") beyond the s44 finding.
