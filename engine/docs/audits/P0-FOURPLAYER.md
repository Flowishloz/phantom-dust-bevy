# P0 audit: FOURPLAYER (deep-pass P6 -- four-player split screen, 2v2 and free-for-all)

2026-09-30. Read-only audit. Read: AGENTS.md, CLAUDE-DEEP-PASSES.md (P6), METHOD-FIDELITY.md, DECISIONS.md,
DESIGN-2V2.md, handoffs/H5-orb-ownership.md, PROGRAM-RE.md (Lock-on, Capsules, Holes), FORMATS-STAGES.md
(start points), all of `sim.rs` / `combat.rs` (production half, lines 1-2513) / `moves/{mod,effects,erase,special,status}.rs`,
the 2-view code paths in `main.rs`, `hud.rs`, `cam_fade.rs` + `dissolve.wgsl`, `fx.rs`, `sfx.rs`, `school_aura.rs`,
`surface_ground.rs`, `frontend/*`, `menu.rs`; and the program (`decomp_all.c`, addresses below).

## Verdict in six lines

- Our runtime is a true two-player engine: fixed `[X; 2]` arrays in the sim, `1 - i` as "the opponent" in 21 production lines
  of `combat.rs` alone, no target concept, no team id, two cameras, two HUDs, two setup columns.
- The original is built for **four players and three rule sets**: Battle Royale (free-for-all), Tag Team (2v2) and One-on-One.
  It has a per-player **team id**, a **32-slot lock-target list with a validity rule and a target-cycle button**, **per-team
  "alive" resolution**, and a **revive** that works exactly like a capsule capture (read below, mostly confirmed).
- Stage data already carries **four start points per set, each with three capsule spots** (st02-st26; st01 has 2, st09 has 8).
  The converter output (`cache/*.props.json`) and the four costume colours per character (`cache/chars/*.c0..c3.png`) are
  already four-player ready; only our code uses two.
- Three findings change the plan DESIGN-2V2 sketched: (1) target selection in the original is *first valid slot from the current
  one, cycling on a button*, not nearest; (2) attack objects exclude **only the caster**, no team test found in the hit path
  (friendly fire is probably ON, SUSPECTED); (3) a revived player **respawns at their start point with half max health and 90
  ticks of invulnerability** (not in place).
- The approved wall-dissolve depends on a two-view heuristic in the shader (`dissolve.wgsl:59`). It must be replaced by a
  per-view identity without changing one pixel of its look; a safe design is given below.
- Nothing here needs a code change to the approved dissolve look, footsteps (common35), capsule ownership (H5) or the
  locked-on movement; each must stay green through every step (regression list at the end of section 4).

## 1. Ledger

Legend: mapped = we know what the original does; impl = code exists; repro = our runtime seen doing it; verified = tested against
the original's rule; approved = Laurence said so. Everything below the line "four-player" is **not built**, so repro/verified/approved
are X for all of it unless stated.

| Feature | Mapped | Impl | Repro | Verif | Approved | Evidence |
|---|---|---|---|---|---|---|
| Two-view split screen, side-by-side and top/bottom | n/a (the UWP program's own local views unread, R6) | yes `main.rs:1647-1670` | yes | ~ (no test) | yes (DECISIONS 2026-09-26/27 camera + split sheets) | `layout_viewports`, `apply_fov` `main.rs:1675` |
| Per-view wall dissolve (2 views) | n/a (our design of an original effect) | yes `cam_fade.rs`, `dissolve.wgsl` | yes | yes (9 tests incl. `wall_lifecycle`) | **yes, complete** (DECISIONS 2026-09-29) | must be preserved |
| Capsule own/ghost isolation by render layer (2 views) | yes (`FUN_14021b840`, CAPSULE-VISUALS) | yes `fx.rs:596,792,813,1327` | yes | yes (`spawn_orb_children_carry_their_views_layer`) | ~ (H5 fix; no explicit sign-off found) | H5-orb-ownership.md |
| Capsule ownership (owner-only capture, 3 fixed spots per player) | yes (`FUN_14014ee70/14014fd10/14014ed10`) | yes `combat.rs:1343,2330` | yes | ~ | yes (DECISIONS 2026-09-26) | PROGRAM-RE "Capsules" |
| Team id per player | **yes** player +0x1c = setup record short [0x18] (`FUN_14024e950` 0x14024e950) | no | - | - | yes: "teams chosen on setup" (DECISIONS 2026-09-27) | section 1b A |
| Lock target list + validity rule | **yes** `0x1402543e0` | no (one opponent, `1 - i`) | - | - | - | section 1b C |
| Target pick / cycle (button mask 0x400) | **yes** `0x140252f70`, `0x1402531d0` | no | - | - | ~ (DESIGN-2V2 names "left trigger" from fan text, physical button unread R1) | section 1b D |
| Optical Camo excludes you from lock-on (status 8) | yes `0x1402543e0` via `0x14024d4d0(p,8)` | ~ `sim.rs:439-442` (2-player form) | yes | ~ | - | PROGRAM-RE Lock-on |
| Hit mask: caster bit cleared only; one hit per victim per object | **yes** `0x14026cd70`, `0x14026dbd0`, `0x14026c1c0`, `0x14026ecc0` | no (single victim) | - | - | - | section 1b E |
| Friendly fire policy | **suspected** (no team test found in hit path) | no | - | - | - | R5 |
| Round end = teams alive <= 1; result kind; ranking | **yes** `0x14012c510`, `0x14012c440`, `0x14012c790`, `0x14012c8c0` | no (`hurt` awards `wins[1 - victim]`, `combat.rs:1910-1915`) | - | - | - | section 1b F |
| Rule sets OneOnOne / TagTeam / BattleRoyale | yes (dispatch `0x14010a680`, Win events) | no | - | - | scope: P8 | section 1b F |
| Revive (capture body, hold in slot, use) | **yes, mostly** `0x140251550`, `0x1402516a0`, `0x14027cc10/cca0/cdc0`, `0x140261de0`, `0x140251f80` | no | - | - | Laurence: "add the original's revive once its rules are read" (DECISIONS 2026-09-27) | section 1b G |
| Four start points per set, three capsule spots each | yes FORMATS-STAGES "Start points"; `0x14012a5e0` | ~ parsed, only first two used `main.rs:1070-1072` | yes (data) | - | - | `cache/st03.props.json` etc. |
| Four costume colours per character, colour per player | yes (`0x140250f70`, record short [1], FORMATS-ALR 206) | ~ only colour 1 for P2 `main.rs:908,1989`, `charselect.rs:178` | yes | - | yes (P2 alt costume, DECISIONS 2026-09-27) | cache has c0..c3.png |
| Quarter viewports (2x2) + HUD per view | n/a | no | - | - | yes "four quarters" (DECISIONS 2026-09-27) | - |
| Input: 4 seats (pads joining, keyboard seat, disconnect) | n/a | no (`pads.iter().take(2)` by entity order `main.rs:1183`) | - | - | yes "pads, keyboard fallback for P3/P4" (DECISIONS 2026-09-27, see Q4) | - |
| Deterministic 4-player input struct + replay | n/a | no (`Input` has no target-cycle; `[Input; 2]`) | - | - | - | `sim.rs:114-126` |
| 1- and 2-player behaviour | - | yes | yes | yes (345 game tests) | yes | regression net |

### 1b. What the original does with four players (program reads, addresses are VAs in `PDUWP.exe`)

**A. Player struct** (`DAT_1404bc9d8[4]`, made by `0x14012d200`, filled by `0x14024e950` from the lobby's setup record).
+0x14 slot 0-3; +0x18 **start-point index** = set*4 + n (`0x14012a5e0`: set = idx>>2, start = (idx&3) mod starts-in-set,
0x50-byte records, facing float at +0xc; `0x14012d200` makes the four players take distinct starts of one set); +0x1c **team**
(record short [0x18]); +0x12c70 = character id | **costume colour << 16** (record shorts [0],[1]; path `player\<id><colour>.alr`,
`0x140250f70`); +0x20 taking part, +0x24 alive, +0x4c/+0x50 health/max health, +0x68 end tick (-1 while alive). The controller array
`DAT_1404bc608[4]` points at the same players (controller +0x61a0 -> player). Each player struct also holds three fighter
slots (0x6400 stride, loops of 3 in `0x14024e950`, `0x14012d200`): a story/party feature, not used in versus (R8).

**B. Input word** (player +0x1425c; mapping table +0x1427c..+0x142bc written by `0x14024e190`): lock-on = 0x800, **switch
target = 0x400** (both read in code, `0x140252a20`/`0x1402531d0`). The table also lists 0x100, 0x80, 0x20, 0x10 (probably the four skill
buttons), 1, 4, 8, 2 (probably the D-pad) and 0x40 (toggles a prompt flag in `0x140251550`); those roles are unverified.

**C. Lock target validity** (`0x1402543e0(lock, idx)`): idx < 32; slot filled (`DAT_1404bcb50[idx]`, **32 lockable slots**);
target controller present and active (+0 != 0); slot flags +0x14, +0x1c non-zero; **not yourself** (player +0x14 equal);
**health > 0**; **not under status 8** (Optical Camo, Place of Invisibility); **not a teammate** (player +0x1c equal to
yours) **unless option `DAT_14052dfb9`** (friendly lock); and if *you* are under status 9 the only valid slot is the one status 9
names (forced target, who sets it R2).

**D. Pick and cycle** (`0x140252f70`, `0x1402531d0`, every tick per player): the lock keeps a target index (+0x10, -1 = none).
With option `DAT_14052dfb8 == 0` the target is **re-picked every tick as the first valid slot scanning from the current
one** (wrapping mod 32, up to 32 tries): a dead or camouflaged target hands over to the next valid slot even while you are not
locked, so casts always have a valid target. Pressing the lock cycle button (0x400, edge, unless `DAT_1404bbd88[player*0x24+4]`
disables it) while locked does `0x140252f70(current + 1)` = **next valid slot in index order, not nearest**. With
`DAT_14052dfb8 != 0` the target starts at -1 and cycling (`0x140252ff0`) walks the valid targets sorted by a float at
lock +0x90 + 0x14*idx (what it measures R1b). Distance band for the dash gate: 1 <= 80 u, 2 <= 300 u, 3 beyond (`0x1402f8040`,
`0x1402f8138` = 80.0 / 300.0, read).

**E. Who an attack can hit.** Every skill object carries a 4-bit hit mask (object +0x24); at impact/init the mask is **all four
players minus the caster** (`0x14026cd70`: `~(1 << caster_slot) & 0xf`; `0x14026dbd0`; mines and wall-blasts set all four
bits so the thrower can be hit, PROGRAM-RE line 181). The swept-sphere test (`0x14026c1c0`) requires the victim's slot bit in
the mask and the victim alive, and `0x14026ecc0` clears the victim's bit after a hit (**one hit per victim per object, so a blast
can hit every body in radius once**). No team comparison appears anywhere in `0x14026c1c0 / 0x14026f870 / 0x14026ecc0 /
0x14026dbd0 / 0x14026cd70`. The one team test in skill code is `0x14026fcb0`: the unlimited Hole (motion 16) creates a hole at
**every player's feet**, but for the record variant (record shorts +0x52 = 1, +0x54 = 4) only at players **whose team differs
from the caster's**; variants 2 and 5 hit everyone (PROGRAM-RE line 280's "partner in 2-v-2" question lands here).

**F. Rounds, teams and results.** `DAT_1404bc8b8` = rule set: dispatch at `0x14010a680` calls `0x1400b7880` (Battle Royale
Win event `FUN_1400b2b60`), `0x1400b93f0` (**Tag Team**, `TagTeamWin` `0x1400b6ad0`), `0x1400b8e20` (One-on-One, `FUN_1400b5d00`),
so 0 = Battle Royale (free-for-all), 1 = Tag Team, 2 = One-on-One (callee names read, index order is surprising but consistent).
Per tick `0x14012c510` computes, through `0x14012c440`, one **alive flag per team** (`DAT_1404bc890[team]`: a team is alive while
any member has end tick -1, or passes a 60-tick window `0x3c`, R4; the team's score `DAT_1404bbeb0[team]` = latest member end
tick) and counts alive teams; **the battle ends when 1 or fewer teams remain** (`1 < bVar4` keeps it going, plus a
post-round delay). `0x14012c790` then sets the result kind (win, lose, draw=6) from the winning team id (`DAT_1404bbe04`, -1 = draw),
`0x14012c8c0` ranks everyone: Tag Team = win/lose by team; other rules = rank by alive flag, then by survival score
(1st..4th). Xbox Live events `TagTeamOver` (param `TeamAlive` = winning-team members still in), `TagTeamRevive` (Count),
`TagTeamWin` (`0x1400b66f0`, `0x1400b68e0`, `0x1400b6ad0`) confirm TeamAlive and revive counts are tracked per team.
No "holder dies -> team loses" literal rule exists: it follows from "team eliminated when both members are dead".

**G. Revive** (net messages `PlayerCaptureReq/Ans`, `PlayerRevive`: table at 0x1403fa8e0, make/receive `0x14027c3b0/0x14027cc10`,
`0x14027c3d0/0x14027cca0`, `0x14027c3f0/0x14027cdc0`):
1. `0x140251550` per tick, for a living player, sets the **capture target kind** (+0x17160): **2 = a dead teammate**
   (`0x1402516a0`: another filled slot, same team +0x1c, dead (+0x24 == 0) or not in play (+0x20 == 0), not already being
   captured (+0x17174 == -1), within **15.0 u** `0x1402f7fb0` (operand partly lost in the decompile R3)), else 1 = one of
   your capsules (`FUN_14014f630`), else 3 = an environment crystal. Teammate body has priority over capsule.
2. A skill-button press at kind 2 sends `PlayerCaptureReq`; `PlayerCaptureAns` (`0x14027cca0`) either refuses (clears
   +0x17174) or puts the **player into your skill slot** (`FUN_140261cb0`, slot struct at +0x388 + 0x50*k, captured player at
   slot +0x3c) and counts `TagTeamRevive`.
3. Using that slot (`0x140261de0`, or `PlayerRevive` `0x14027cdc0`) calls `0x140251f80(target)`: alive flag +0x24 = 1,
   end tick -1, **respawn at their start point** (`0x14012a5e0`), facing reset, stance clip, **invulnerable 0x5a = 90 ticks**
   (`0x140252400`), **max health := (max + 1) / 2 and health := max** (skipped in mode `DAT_1404bc5dc == 10`, R8). The fan text
   in DESIGN-2V2 ("halved, rounded up") is therefore confirmed; "in place" is not: it is at the start point.

**H. Stage data** (FORMATS-STAGES, re-counted from `cache/*.props.json`): most stages have 5-6 start sets of 4 starts, each start
with 3 capsule spots (st01: sets of 2; st09: one set of 8; st00/st09_xx: none). st03 set 0 has two starts at y ~ 52 and two at
y ~ 112 (two floors), set 1 is a tight cluster (<250 u). Which set and which starts a 2v2 pairs up is chosen by the lobby (R7).

## 2. Discrepancies, most important first

CONFIRMED = both sides read.

1. **No target concept (CONFIRMED).** Original: per-player lock target over a slot list with a validity rule and a cycle button
   (C, D above). Ours: `opponent = positions[1 - i]` (`sim.rs:423-426`), camera foe `1 - c.0` (`main.rs:1571`), aim `players[1 - i]`
   in `cast` (`combat.rs:1411`), `fire` (`1527`), `lunge` (`1639`), `dash_eligible` (`974`), homing/fields (`2133-2162`), sensor mines
   (`2283`), HUD marker/colour (`hud.rs:700,827,835`). Effect: with 3-4 players nobody can aim or lock at anything but the "other"
   index; no cycling; the camera, marker, homing and instant skills cannot agree on one target.
2. **Fixed `[X; 2]` state and `1 - i` logic everywhere (CONFIRMED).** 22 `[Player; 2]` signatures and 21 production `1 - x` in
   `combat.rs` (full list in section 3), 3 in `sim.rs`, `moves/` (Everyone = both players, `erase.rs:108,138`, `status.rs:108`).
   Effect: physically impossible to add players; every function assumes exactly one other body.
3. **Victim is derived, not found (CONFIRMED).** `strike`/`damage`/`hurt`/`attack_extra`/`guard_extra`/`send_back`/`brush_away` take the
   victim as `1 - attacker` (`combat.rs:1686,1745,1913,2012,2051,2104`). `CombatEvent::Hit`/`Blocked` carry no attacker
   (`combat.rs:607-609`), so `fx.rs:144,152` and `frontend/state.rs:142-165` (`observe`: damage credited to `1 - i`) and
   `sfx.rs:543` guess it. Projectile and blast hit tests check one body (`2162,2202,2241`; original hits every body in the mask, E).
4. **Rounds end on the first knock-out, winner `1 - victim` (CONFIRMED).** `hurt` (`combat.rs:1910-1915`) scores and ends the round
   at once; draw is the magic winner `2` (`1100,1154-1157`, `main.rs:1050`, `sfx.rs:543`, `frontend/result.rs:47`). Original: per-team
   alive flags, round ends when at most one team has a living member (F). Time-up here = healthier player (`combat.rs:1094-1101`);
   the original's time-up resolution is unread (R4).
5. **Teams do not exist (CONFIRMED).** Original team id gates lock-on (C), holes (E), results (F) and revive (G). Ours: none; `Rules`,
   `MatchSetup` have no team field (`frontend/state.rs:22`).
6. **Friendly fire (SUSPECTED on the original side).** Attack objects exclude only the caster (E). Ours cannot express it. Needs an
   explicit policy hook (section 4 step S1d) and Laurence's call (Q2).
7. **Revive is unbuilt and now readable (CONFIRMED read, unbuilt).** G above replaces the fan-text proposal in DESIGN-2V2 "Revive".
8. **Viewports, FOV and cameras are 2-view (CONFIRMED).** Loop `for i in 0..2` spawns two cameras with layers `{0, 1+i}` and `order: i`
   (`main.rs:926,951-961`); `layout_viewports` has only left/right or top/bottom halves (`main.rs:1662-1669`); `apply_fov` derives the
   aspect from those two layouts (`1675-1699`); `follow_cameras` uses `Local<[f32;2]>` and the single foe (`1558-1571`).
9. **Wall dissolve identifies the view by viewport origin (CONFIRMED, approved, must be preserved).** `dissolve.wgsl:59-60`:
   `side = step(0.5, viewport.x + viewport.y)`, `coverage = mix(x, y, side)`; uniform is a `Vec4` holding only views 0 and 1 plus grain
   (`cam_fade.rs:152-158,305-313,392-400`); `FadeState.live: [[f32;2];2]` (`187`); every loop is `0..2` (`374-375,390`). With four
   quarters, views 1, 2, 3 all read as "view 1" (their origins are non-zero) and would share view 1's coverage: the wrong
   fighter would dissolve in the wrong quarter. Safe design in step S3b.
10. **Capsule presentation makes exactly one ghost (CONFIRMED).** `orb_presentations` returns `[(owner, own), (1 - owner, ghost)]`
    (`fx.rs:596-599`) and `sync_combat_visuals` spawns two entities (`1327`); camera layers are `{0, 1+i}` (`main.rs:959`). For
    N views the ghost must be seen by every non-owner view.
11. **Input routing is positional and two-wide (CONFIRMED).** `PendingInputs([Input; 2])` (`main.rs:564`), `pads.iter().take(2)` by
    entity query order (`1183`), keyboard = P1 (WASD/J K U I, Space, Y, Q/E) plus a half keyboard for P2 (Enter, RShift, numpad
    skills, no movement; `1159-1182`), menus use `pads.iter().nth(i)` (`frontend/input.rs:44,79`). No seat/join/disconnect model, so pad
    order can silently swap after a reconnect. `Input` has no "switch target" button (`sim.rs:114-126`).
12. **HUD is one-own plus one-foe panel (CONFIRMED).** `spawn_hud` loops `0..2` (`hud.rs:496`), names `PLAYER {i+1}` / `PLAYER {2-i}`
    (`537-538`), the foe panel reads `fighters[1 - i]` (`708-719`), `layout_hud` only knows the two layouts (`923-941`). Scale is
    `min(view.x/1280, view.y/720) * 1.2` (`927`): a quarter of 1920x1080 gives 0.9 (same as today's top/bottom at 1080p, which Laurence
    plays), of 1280x720 gives 0.6 (16 px design text becomes ~10 px), of 4K gives 1.8.
13. **Front end is two columns (CONFIRMED).** `MatchSetup { chars: [String;2], arsenals: [Vec;2] }` (`frontend/state.rs:22-23`), setup
    P1 | shared | P2 columns and `p1_*/p2_*` fields (`setup.rs:21-24,45-53,152,223-224,265-276`), two live previews (`charselect.rs:189`),
    result screen two portraits at fixed x (`result.rs:60-61`), winner from `wins[0]` vs `wins[1]` (`result.rs:35-44`).
14. **Audio ownership is two-wide (CONFIRMED).** `voices`/`voice_chars: [..;2]` (`sfx.rs:332,338,446`), per-player cast and lock loops
    `0..2` (`351,495,523`), KO voice plays for `1 - winner` (`543-544`). Audio is one global non-positional mix: per-frame de-dup by path
    and `MAX_PLAYING` budget (`sfx.rs` `started`, `budget`) would swallow identical simultaneous cues from several players.
15. **Start points and capsule spots use only two starts (CONFIRMED).** `stage_layout` reads `start_sets[0]` and the first two starts,
    `capsule_spots` for those two only (`main.rs:1059-1081`); `pick_spawns` makes two points 100-160 u apart (`1087-1139`).
    Original: one set of four starts (A, H).
16. **Costume rule is "P2 gets colour 1 iff same character" (CONFIRMED).** `p2_alt_colour_id`, `apply_p2_colour` (`charselect.rs:178`,
    `main.rs:1989`) only touch slot 1 and colour 1. Original: colour is a per-player setup field 0..3 (A).
17. **Body separation handles one pair (CONFIRMED).** `let [a, b] = &mut self.players` (`sim.rs:490-499`).
18. **Skills that mention "the other / everyone" (CONFIRMED).** `Reach::Target` "the other player", `Reach::Everyone` "both players"
    (`moves/effects.rs:19-27`); Hole "everyone" = `0..2` (`erase.rs:108,138`); Vicious Balance lowest of two (`erase.rs:244`);
    Doppelganger copies `1 - caster` (`status.rs:196`); Surprise Attack goes to `players[1 - caster].spawn` (`special.rs:279`), where the
    original picks a *random opponent in 4-player* (PROGRAM-RE line 558); holes by team (E).
19. **Optical Camo unlocks the wrong player (CONFIRMED).** `sim.rs:439-442` drops your lock when the *other* fighter is invisible. With
    several candidates only that one target becomes invalid; the lock must move on (D) and only release when none is valid.
20. **One costume pipeline, per-slot arrays of 2 in presentation code (CONFIRMED).** `school_aura.rs:193,280,315`, `surface_ground.rs:39-47`
    (`WetCycle.next: [u8;2]`, `player.min(1)`), `sfx.rs:314` (`LockSfx.seen`), `fx.rs:58` (`seen_cast`), `main.rs:606-609,1465,1558-1559`.

Determinism notes (CONFIRMED hazards): `start_round` draws RNG per player in index order (`combat.rs:985-991`); moving to four slots
must keep 1v1 draw order identical (inactive slots draw nothing) or every seeded test/golden changes. Pairwise body pushout and "hits"
must iterate in a fixed slot order.

## 3. Inventory: every two-player assumption (file:line)

S = sim (step S1), R = render (S3), A = audio/fx events (S4), H = HUD (S5), F = front end/input (S2/S6).

**sim.rs (S):** 115 stick doc "toward the opponent"; 114-126 `Input` (no switch-target, 4 seats); 284-290 `locked`/`cam_yaw` "the opponent";
385 `players: [Player; 2]`; 399-401 `Match::new(spawns: [Vec3; 2])`; 406-413 `reset_positions` (`0..2`, `spawns[1 - i]`, faces the other);
417 `step(inputs: [Input; 2])`; 423-426 `positions`, `fell`, `opponent = positions[1 - i]`; 439-442 camo vs `fighters[1 - i]`;
443 `step_player_at(.., opponent, ..)` (601-604 lock camera toward it); 465-469 `fell`; 470 `[inputs[0].skills, inputs[1].skills]`; 489-499 pair pushout.

**combat.rs (S), production lines:** 635 `arsenals`; 640 `fighters`; 648 `capsule_spots`; 655 `wins`; 657 `round_over: Option<(usize,u16)>` (2 = draw);
664 `airborne`; 679 `clip_ticks`; 811-845 ctor arrays; 851 `set_rules` wins; 859 `set_clip_ticks`; 974-977 `dash_eligible`; 985-1001 `start_round` (RNG);
1080 `step(players: &mut [Player;2], buttons: [u8;2])`; 1092 `airborne`; 1094-1101 time-up `[a, b]`; 1105 powerless loop; 1113 still/regen loop;
1134 fighter loop; 1148 capsule loop; 1153-1158 "both down = draw"; 1162 `tick_fighter`; 1343 `capsule_in_reach` (owner filter stays);
1366-1420 `cast` (`target = players[1 - i]`, yaw); 1503 `release_charge`; 1525-1529 `fire`; 1577-1582 `take_effect` (`Reach::Target(1 - i)`);
1621-1665 `lunge` (`1639` target, `1665` bodies); 1684-1686 `damage`; 1740-1745 `strike`; 1910-1915 `hurt` wins; 2011-2012 `guard_extra`;
2049-2051 `attack_extra`; 2086 `brush_away`; 2101-2104 `send_back`; 2110-2162 `move_projectiles` (bodies `2111`, fields `[0, 1]` `2113`, `1 - owner` at
2133/2136/2141/2153/2162, blast `2241`); 2268-2307 `tick_areas` (`2283` sensor, `2299` bodies, `2307` victim); 2330-2381 `spawn_capsule`;
2514-2538 testkit `setup/run/press/use_skill` (2 players, `[0; 2]`).

**moves/ (S):** `effects.rs:19-27,30-33,41-57` Reach/Delivery "other"/"both"; `mod.rs:264` `apply_effect(players: &mut [Player;2])`;
`erase.rs:100,108,138,244,383,411,426,437`; `status.rs:108-109,196`; `special.rs:129,141,255,274,279,352,476`; `environment.rs` (fighters `[0]`,`[1]` in tests only).

**main.rs (F/R):** 129 `set_dodges(chars: &[String;2])`; 529 `load_clip_ticks`; 536 `Config.chars`; 564 `PendingInputs`; 569 `CamPitchInput`; 575-588 `PlayerVisual`,
`P2AltColour`, `PlayerCamera`; 606-609 `CharacterAnims`; 907-971 `spawn_scene` (908, 926 loop, 951-961 cameras/layers); 978-1009 `setup`/`enter_match`;
1041-1054 `check_match_end` (`winner <= 1`); 1059-1081 `StageLayout`; 1087-1139 `pick_spawns`; 1142-1205 `gather_input` (1150, 1159-1183);
1206-1268 demo script (`0..2`, `other = 1 - i`); 1383 `pending.0 = [0, 1].map`; 1412 `step_sim`; 1465 `sync_visuals`; 1550-1643 `follow_cameras` (1558-1559, 1571);
1647-1670 `layout_viewports`; 1675-1699 `apply_fov`; 1701-1747 `build_graphs`/`attach_anim_slots` (1702); 1989-2015 `apply_p2_colour`.

**cam_fade.rs + dissolve.wgsl (R):** shader 59-60; `Dissolve` uniform 152-158; `FadeState` 187-196; prep 305-313; `update_fade` 341-401 (`roots`/`campos`/`bodypos` `[Option;2]`, `0..2`x`0..2`).

**fx.rs (R/A):** 58 `seen_cast`; 64-67 log; 87 cast loop; 131 `airborne_of`; 144,152 `1 - victim`; 484-491 test hook; 568 `view_layer`; 596-599 `orb_presentations`;
792,813 layers; 1274 preview; 1310-1344 `sync_combat_visuals` (two presentations); tests 1525-1634.
**sfx.rs (A):** 314, 332, 338, 351, 446, 466, 495, 523, 543-544. **hitstop.rs (A):** `combat.fighters.iter().all(Down)` KO slow-down (`150`), victim focus (`132`).
**school_aura.rs (R):** 193, 280, 315-316. **surface_ground.rs (R):** 39-47. **footsteps.rs:** `players.get(i)` already index-generic (196-198).
**hud.rs (H):** 211, 481, 496, 537-538, 550-562, 688-705, 708-719, 827/835, 912-947.
**frontend (F):** `state.rs` 22-23, 100-105, 142-165 (`observe`), 175; `setup.rs` 21-24, 45-53, 79-81, 152, 223-224, 251-252, 265-276; `charselect.rs` 178-183, 189;
`result.rs` 35-44, 60-61, 114-115; `input.rs` 26-53, 44, 79; `title.rs` 68-69; `ui.rs` 39-40 (`P1`,`P2` colours), 579 (`KEYS_TWO_PLAYER`);
`menu.rs` 13, 106, 261, 304 (`Split` has 2 values). **trace.rs:** P1 only (fine). **Legacy CLI `--demo/--near`** builds 2 players (`Config.chars`, `pick_spawns`).

## 4. Proposed build lanes (sequential; this is cross-cutting, one integrator owns `main.rs` per step)

Target architecture (from the reads): `MAX_PLAYERS = 4`; `Roster { active: [bool;4], team: [u8;4] }` (free-for-all = each player
their own team, like the original where team == slot); `Player.target: Option<u8>` plus `Input.switch_target` (edge) in the quantised
input; helpers `enemies_of(i)`, `teammates_of(i)`, `valid_target(i, j)` (C), `current_target(i)` (D); events carry `attacker` and `victim`;
`Outcome { Winner(team), Draw }` replaces the "2 = draw" convention; rule-set enum `OneOnOne | TagTeam | BattleRoyale` from F (feeds P8).
Fixed-size arrays, inactive slots never iterated and never draw RNG, so 1v1 stays bit-identical. 1/2-player matches keep their current
layouts (side-by-side / top-bottom); 3-4 players use a 2x2 quad (Q1).

### Lane S -- simulation first (one owner; Opus, high effort; no Bevy work beyond mechanical compile fixes)

Owns: `sim.rs`, `combat.rs`, `moves/*.rs`. May touch (mechanical type fixes only, flagged in the receipt): `main.rs` lines that construct `Match`/call
`step` (`417`, `1002`, `1383-1417`), `frontend/state.rs` array types. Commits in order; the 345 existing tests must stay green at every commit.

- S1a (no behaviour change, arrays still 2): introduce explicit `victim`/`target` parameters instead of `1 - i` in `cast/fire/lunge/strike/damage/hurt/attack_extra/
  guard_extra/send_back/brush_away/move_projectiles/tick_areas/dash_eligible`; add `attacker` to `CombatEvent::Hit/Blocked/Burst`; `Outcome` enum; per-team `wins`.
  Test: all existing tests unchanged; new test that `strike` with an explicit victim equals the old result.
- S1b: `MAX_PLAYERS`/`Roster`; arrays to `[_; MAX_PLAYERS]`; every loop over active slots in index order; `Match::new(&[Vec3], Roster, Combat)`;
  `testkit::setup` marks 2 active. Test: 1v1 golden replay hash identical before/after (same RNG draws); new 4-player smoke: all four tick, inactive slots inert.
- S1c: targets. `Player.target`, validity rule (`0x1402543e0`: active, not self, health > 0, not status 8, not teammate unless friendly-lock), pick = first valid
  from current wrapping (`0x140252f70`), cycle on `switch_target` edge (`0x1402531d0`: `current + 1`), camera yaw toward target; Optical Camo via validity (fixes D19).
  Tests (from the reads): with 4 slots [self, teammate, dead, enemy] the pick skips self/teammate/dead; cycling order is slot order with wrap; a camouflaged target hands
  over; when no valid target the lock releases; 1v1 behaviour unchanged.
- S1d: teams and hits. `valid_target` teammate exclusion; hit tests run against every body except the caster (`0x14026cd70` mask), one hit per victim per object
  (`0x14026ecc0`), blasts/areas/mines reach all bodies in radius; friendly-fire policy flag (default per Q2); `Reach::Everyone` = active players; Hole by team
  (`0x14026fcb0` variants); pairwise body pushout in slot order; round end = alive teams <= 1 (`0x14012c510/0x14012c440`), win/lose/draw (`0x14012c790`), same-tick
  wipe = draw (keeps the current 2-player draw rule), FFA rank (`0x14012c8c0`). Tests cite those addresses; a 4-player deterministic replay (two runs, identical state hash
  over >= 3000 ticks with scripted casts, lock cycling, kills) is mandatory.
- S1e: skills that name "target/opponent": Surprise Attack picks a random enemy's start (PROGRAM-RE 558, `c.random`), Doppelganger/Fusion/Teleport use the cast target,
  Vicious Balance = lowest active Level (`erase.rs:244`), `Reach::Target` = the caster's `current_target`. Tests per skill.
- Proof: `cargo test` green; new tests named `four_player_*`; 1v1 tick-hash unchanged; no screenshots needed yet.

### Lane R -- input, match plumbing, cameras, render ownership (after S1 merged; one owner; integrator for `main.rs`)

Owns: `main.rs` (all of it in this step), new `views.rs` (layout), new `seats.rs` (seat names/colours/team colours), `cam_fade.rs`, `dissolve.wgsl`, `fx.rs`
(orb/layer/capsule functions and tests; `skill_effects` is step S4), `school_aura.rs`, `surface_ground.rs`, `footsteps.rs` (if array types need it).

- S2 (plumbing, part of Lane R): `MatchSetup.players: Vec<PlayerSetup { char, colour 0..3, arsenal, team, seat }>`; `Seats` resource mapping a pad entity to a seat with
  join-by-press, reconnect-to-same-seat, pause-on-drop (Q3); keyboard seat per Q4; `PendingInputs([Input; 4])`; `gather_input` per seat; `enter_match`/`spawn_scene`/
  `set_dodges`/`load_clip_ticks`/`CharacterAnims`/`build_graphs` loops over `players.len()`; `stage_layout` returns one start set of N starts (all 4, or 2 for st01)
  with each start's three capsule spots; costume per player via `apply_colour(slot)` (cache already has c0..c3); `WetCycle`, `LockSfx`, `seen_cast` arrays to `MAX_PLAYERS`.
  Test: 1/2-player matches start exactly as today (spawns, capsule spots, costumes: same-char P2 still gets colour 1); 4 seats spawn four characters on distinct starts.
- S3a cameras/layout: `ViewLayout` (1 player fullscreen; 2 = today's two layouts; 3-4 = 2x2, Q1), per-view rects used by `layout_viewports`, `apply_fov` (aspect per
  view) and later `hud.rs`; `follow_cameras` reads `players[i].target` for the foe, `booms/pitches` sized `MAX_PLAYERS`. Test: rect arithmetic for each layout; the
  two-player layouts equal today's numbers exactly.
- S3b **dissolve per view, look unchanged**: make the shader's view identity the viewport **quadrant** (`q = step(0.5, vp.x) + 2 * step(0.5, vp.y)` from
  `view.viewport`, with the window size unknown to the shader, so the CPU supplies per-quadrant coverage): uniform becomes `coverage: Vec4` = one coverage **per
  quadrant** and a second uniform/const for grain; the CPU fills quadrant `Q(view)` (from the camera's actual viewport origin) with view `v`'s eased coverage. Top-left
  (0,0) stays quadrant 0 in every layout; side-by-side view 1 is quadrant 1; top/bottom view 1 is quadrant 2; four quarters 0-3. `FadeState.live[fighter][view]` becomes
  `[[f32; 4]; 4]`; `roots/campos/bodypos` over `MAX_PLAYERS`. The eased value, hysteresis, floor 0.15, window 9 to 4 units and grain stay byte-for-byte the same, so the
  look is the approved one. Tests: existing 9 pass unchanged; new test: in a four-view scene a fighter backed to a wall in view 2 only dissolves in quadrant 2's
  coverage, quadrants 0/1/3 stay 1.0; layout switch (side-by-side to top/bottom) moves view 1 from quadrant 1 to 2 without a pop. Runtime proof: four-quarter tick-controlled
  capture (`PD_SHOT_TICKS`), one fighter against a wall, localized pixel diff vs a no-dissolve control in one quarter only.
- S3c capsules: own layer `1 + i` stays; **one ghost per capsule on a per-owner ghost layer** `5 + owner`, and camera `i` sees `{0, 1+i} U {5+j | j != i}` (one entity
  instead of N-1); `orb_presentations(owner, colour)` returns `[(own view, slot), (ghost layer, slot)]`; `OrbFade` keyed by view; teammates' capsules show the ghost
  look unless Laurence says otherwise (R-list). Tests: extend the two H5 tests to 4 owners x 4 cameras (every child carries its mask; no camera sees a foe's own-model).
- S3d presentation: `school_aura.rs` per-slot materials to `MAX_PLAYERS` (coordinate with P5: the aura is being rebuilt; keep the interface `slot -> material`);
  lock-on cue per player; per-player costume textures.
- Proof: `PD_PLAYERS=4` scripted match, `PD_SHOT_TICKS` captures of all four quarters (own capsules full-colour, ghosts for others, no cross-contamination),
  `PD_ORB_LOG=1` log lines, dissolve localized diff, 1/2-player captures unchanged.

### Lane U -- audio/fx events, HUD, front end (after Lane R; three disjoint file groups may run in parallel)

- S4 (events): `fx.rs::skill_effects` (lines 50-200) to use `attacker` from events and `0..MAX_PLAYERS`; `sfx.rs` voices per seat, KO voices for every loser, lock cues per
  player, per-frame de-dup and `MAX_PLAYING` reviewed for four players (same cue from two players in one frame must not vanish), `hitstop.rs` victim focus and the
  "all down" slow-down as "round over"; `Cue::Voice(1 - winner)` removed. Owner: `sfx.rs`, `hitstop.rs`, `fx.rs` (skill_effects only). Test: event -> cue trace for four players.
- S5 (HUD): owns `hud.rs` only (+ `seats.rs` colours from S2). `HudRoot(i)` per view; `layout_hud` from `ViewLayout` rects; own panel stays as today; the "foe" panel becomes
  a compact list of the other players (teammate marked), each with health/aura/level, current target highlighted; lock marker only over `current_target`; names from seats;
  a "down, waiting" state now, "being carried" later (revive). Scale stays `min(view/1280x720)*1.2` (0.9 at a 1080p quarter, same as today's top/bottom); minimum text size
  checked at 1280x720 (0.6) and 1080p. Proof: quarter captures at 1280x720, 1920x1080, 3840x2160; nothing clipped, health/aura/uses/cost/status legible.
- S6 (front end): owns `frontend/{state,setup,charselect,result,title,input}.rs`, `menu.rs`, `ui.rs`. Setup: per-seat columns that join/leave, team selector (teams chosen on
  setup, DECISIONS), character/colour/arsenal per seat, mode (OneOnOne/TagTeam/BattleRoyale) gated by seat count; previews for N; result screen: team win/lose or 1st-4th
  (F), per-player stats with real attacker attribution (`observe` uses `attacker`, not `1 - i`); input routing uses `Seats` not `pads.iter().nth(i)`; settings row "split screen"
  ignored/forced for 3-4 players. Proof: scripted `--menu-script` through four seats, result screens for team and FFA, captures.
- Builder (P7) is NOT touched; it stays per seat 0.

### Step 7 -- acceptance (orchestrator + independent critic)

Four human inputs (two pads + keyboard seat + a third pad, or four pads), full setup to result to rematch; simultaneous casts in all four quarters; target cycling; KO and
round end for 2v2 (both of a team down) and FFA; pad drop and rejoin; second match without leakage (arrays, layers, dissolve, seats reset); deterministic four-player replay;
measured frame time with four views at target resolution (P4 budget). Final close is Laurence playing a four-seat match.

### Step 8 -- revive (after 7, per DECISIONS: "add the original's revive once its rules are read")

Owner: `combat.rs` + `moves/` (new `revive.rs`), HUD markers. Build from G: capture kind 2 (dead teammate within 15 u, same team, nobody else capturing; priority over capsules),
hold the body in a skill slot, use = respawn at the player's start point, max health `(max+1)/2` and health = max, 90-tick invulnerability, alive flag and end tick reset; counts
`TagTeamRevive`. Tests cite `0x140251550/0x1402516a0/0x140251f80/0x140261de0`. (R3 must be read first for the exact reach.)

### Regression list for every step

Dissolve look/lifecycle (9 cam_fade tests + captures), common35 footsteps, H5 capsule ownership tests, locked-on movement (35 sim tests), P2 alt costume when characters match,
1v1 and 2-player side-by-side / top-bottom layouts pixel-equal rects, deterministic 1v1 replay hash, second match / rematch reset, pause/unpause.

## 5. Reads still needed (none launched; Ghidra untouched)

- **R1** Physical pad button for mask 0x400 (switch target) and 0x800 (lock): who fills player +0x1425c from `Xb1GamepadManager`/Windows.Gaming.Input; the input table is written at `0x14024e190`
  (+0x1427c..+0x142bc). R1b: what the float at lock +0x90 + 0x14*idx is (`0x1402536f0`, `0x140253f20`) for the `DAT_14052dfb8 != 0` mode, and the defaults of options `DAT_14052dfb7/8/9` (set by `0x14024b700`, defaults `0x1401cb070`).
- **R2** Status 9 (forced lock target): which skill writes it (`0x1402543e0` reads it via `0x14024d4d0(caster+0x40, 9)`).
- **R3** Reach of the dead-teammate capture: `0x1402516a0` compares a distance to 15.0 but the second operand of `FUN_140124780` is lost in the decompile; needs a disassembly of `0x1402516a0` (from `FUN_140251550`). Also the capture request apply `FUN_1402503f0` and `FUN_140261cb0`.
- **R4** Tie/grace window 0x3c in `0x14012c510` -> `0x14012c440` (what player +0x64/+0x68 are; who writes the end tick for non-local players; `0x14027c4f0`, `0x14027fde0`) and the time-up resolution for Tag Team / Battle Royale (ours: healthier player).
- **R5** Friendly fire: where an attack object's +0x24 mask is first set before `0x14026cd70` (allocator `0x140263a00`, `0x140263ce0`) and whether `0x14026f530`, `0x140272510`, `0x140272bc0` test teams; check fan guides only as leads.
- **R6** Local split-screen in the PC/UWP program: rich-presence string `split_screen` exists (`0x1402f0860`) and the stage glow table is per view (9,216 bytes, FORMATS-STAGES:202), but D3D viewport calls go through vtables and were not found; how many local views/pads the program supports, and its layout for 3-4.
- **R7** Who chooses the start set and which starts partners take (`0x14012d200` argument record short [0x19], `0x14012d5c0`); st03 set 0 splits two floors.
- **R8** Mode `DAT_1404bc5dc == 10` (respawn on death, `0x14025c670`, no health halving) and battle kind `DAT_1404bc9d4` 0..6; the three fighter slots per player (story party?).
- **R9** The 32 lockable slots: who registers into `DAT_1404bcb50` (objects with no controller pass `0x1402543e0`).
- **R10** Whether casts with no lock aim at the auto-picked target (D says the target index is always valid with option 0; confirm in `0x140258180` and the instant-skill target path).

## 6. Questions only Laurence can answer (max 5)

1. **Three players:** how should a 3-player match look? A) 2x2 quad with the fourth quarter showing the arena map/scoreboard (recommended), B) top row two views, bottom row one wide view, C) not supported: 2 or 4 players only.
2. **Friendly fire in 2v2:** the original's attacks seem to exclude only the caster (suspected, R5). A) your attacks can hurt your partner like the original seems to, B) attacks skip teammates, C) setup option, default off (recommended until R5 is settled).
3. **Joining with pads:** A) each player presses a button on their pad in the setup screen to take the next free seat, and the match pauses if a pad drops (recommended), B) seats follow the order the pads were plugged in, C) seats are assigned by hand in setup.
4. **Keyboard in four-player:** today the keyboard plays P1 (and numpad half-plays P2). A) keyboard is seat 1 only, pads for everyone else (recommended), B) keyboard can take a second seat with its own cramped keys, C) no keyboard in four-player.
5. **A knocked-out player's quarter in 2v2 until the round ends:** A) keeps showing their own fallen fighter (as today, recommended), B) switches to a teammate's camera, C) greys out with "waiting".

## 7. Lane S1 results (branch `lane/fourp`, 2026-09-30)

Built: S1a (explicit victim/target, `attacker` on Hit/Blocked/Burst), S1b (`MAX_PLAYERS = 4`, `Roster`, `FriendlyFire`, `DRAW`), S1c (`combat/lock.rs`: targets and cycling, `Input::switch_target`, `Player::target`), S1d (hits on every body, unlimited Hole per victim, guard fields, placements/team_wins/round_outcome), S1e (Surprise Attack, everyone effects). Not built: revive (step 8), teams in the front end, anything render/HUD/audio.

Program reads re-checked in `decomp_all.c` before building (all three confirmed): `0x1402543e0` validity, `0x140252f70` first valid slot from the current one (mod 32, up to 32 tries), `0x1402531d0` re-pick while unlocked and `pick(current + 1)` on the rising edge of input mask `0x400` while locked. Two refinements of section 1b D: (1) the program re-picks **only while the lock is not held**; a locked target that became invalid stays until the cycle button is pressed (our sim hands over at once: UNCONFIRMED deviation, `combat/lock.rs` docs); (2) the cycle only runs in battle state 6 (`DAT_1404bc950`). Lead for R2 (status 9 = forced lock target): `status.rs` already has `code::DECOY` ("no third player to redirect lock-on to"), which fits a skill that writes status 9.

Decisions made in code (all marked in the source): `DRAW = usize::MAX` replaces the magic winner `2` in `Combat::round_over` (consumers that test `winner > 1` keep working); friendly fire default `On` (audit R5 unread); `friendly_lock` off (option `DAT_14052dfb9` default unread); rounds end when at most one team has a player standing, the winner slot is the lowest standing slot of that team, one player per round is credited (a team's score = `team_wins`); Surprise Attack picks a random enemy only when there are several; melee and areas hit every victim in reach once, shells end on their first body (object flag 0x20 = passes through, `PROGRAM-RE.md`).

Proof: `combat::tests::replay` hashes every tick of 7 scripted 1v1 matches (goldens recorded by running the same module against base `01d9896`); `four_player_*` tests (targets, cycling, hits, holes, teams, draws, determinism of 2v2 and free-for-all over 3600 ticks); `cargo test`: 387 game + 42 formats + 7 convert + 10 viewer.

## 8. Lane fourview results (branch `lane/fourview`, 2026-10-01): S2, S3a, S3b, S3c, S3d

Built: `seats.rs` (player list `PlayerSetup`/`MatchPlayers`, costume colours per slot, `Seats` with join-in-order or join-by-press, reconnect to the same seat, match pauses while a seat's pad is missing; keyboard = seat 1, the old half-keyboard only in a duel), `views.rs` (`ViewLayout`: full / side-by-side / top-bottom / 2x2, rectangles equal to the old numbers, `quadrant_of`, `fov_for`, the three-player map + scoreboard quarter), `PendingInputs [Input; 4]` with `switch_target` (**left trigger LT, UNCONFIRMED**: the pad-to-mask conversion (R1) sits behind Windows.Gaming.Input calls and `+0x1427c..` is only the skill-index to mask table, so 0x400 is the program's mask, not a physical button; keyboard T), N-player match start (`Combat::with_roster`, `Match::with_spawns`, first N start points of the stage's first set, st01-style sets of two continue into the next set), `check_match_end` through `round_outcome` / `team_wins`, cameras follow `aim_at(i, players[i].target)`, one ghost per capsule on `GHOST_LAYER_BASE + owner` (camera `i` sees `{0, 1+i} U {5+j, j != i}`), dissolve and aura alpha keyed by screen quadrant, `char_light` / wet-step arrays to four, minimum HUD (four roots from `ViewLayout`, foe panel and lock marker on the lock target, round banner by team).

Test hooks: `--players N` (1-4) with `--p1`..`--p4`, `--teams 0,0,1,1`; `PD_TEST_STICK2/3/4`; `PD_TEST_SWITCH=<player>,<tick>[,<ticks>]`; `PD_FRAME_STATS=<frames>[,<skip>]` (uncapped frame time).

Audit claims that were wrong or needed care: (1) "existing 9 cam_fade tests pass unchanged": two assertions had to widen (`coverage` is now `Vec4::ONE` with a separate `grain`, `live` is 4x4); the other seven are untouched and four new tests were added. (2) `school_aura.rs:193,280,315` has no two-wide arrays (the index lines were `aura_fx.rs`, which does). (3) `fx_gamma` per-view alpha no longer lives in `params`: fxrender moved the blend mode into `params.y` and used binding 101, so the four quadrant alphas are binding 102. (4) The quadrant rule (`vp.x > 0` bit 0, `vp.y > 0` bit 1) needs no window size, as the audit said, and the CPU reads the origin off the camera's real viewport.

Known deviations and open items: capsule near-camera fade is one fade per capsule (the ghost is one entity, so it fades for everyone when any non-owner camera is within 10 u); teammates' capsules show the ghost look; the spare-quarter map is drawn over the whole stage bounds (small on big stages); HUD content is still per-view panels at the old scale (S5 owns the legibility pass).

