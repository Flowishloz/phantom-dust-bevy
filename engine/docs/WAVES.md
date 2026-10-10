# Waves — orchestration state (started 2026-09-30 night)

Laurence: Claude (Opus) is sole editor/orchestrator with Sonnet lanes;
budget no issue; work through every area not yet correct; one review sheet
per wave. Ledgers: `audits/P0-*.md` (brief `audits/P0-BRIEF.md`). Method:
`METHOD-FIDELITY.md`. Open choices default to the original (DECISIONS
2026-10-01).

## Wave 2 — running (worktrees `..\pd-lane-<name>`, branch `lane/<name>`)

| Lane | Owns | Spec |
|---|---|---|
| builder (F1) | arsenal.rs, frontend/builder*.rs, arsenals.rs, discovery.rs, script.rs | P0-FRONTEND F1 |
| charlight (L1) | stage_look.rs, new char_light.rs, gltf `get_lit`, few main.rs material lines | P0-LIGHTING lane 1 |
| aura (A1) | school_aura.rs, new aura_fx.rs, effects.rs tint hook | P0-AURA lane 1 |
| fourp (S1a–c) | sim.rs, combat.rs, moves/* (+ compile fixes elsewhere) | P0-FOURPLAYER lane S |
| animargs | main.rs animation fns, new anim_args.rs | P0-ANIMSTATES ANIM-ARGS |
| live tracer | engine/re/live_trace.py, tools/bats/PLAY_ORIGINAL_TRACED.bat | debugger on the Store original |

## combat.rs / sim.rs queue (one owner at a time)

1. fourp S1a–c (running).
2. **CAST+RULES core** = P0-ANIMSTATES CAST-STATE + P0-SKILLS S1 (both own
   cast/fire/refill; merged into one lane): payment at fire, cast gate
   (states 0/1/8 + s40), aim capture + 20 deg/tick turn, air-cast hover,
   refill curve + handicap, Level cap 99, respawn 14.5 s, pickup reach.
3. In parallel after 2 (disjoint function fences): REACTIONS (P0-ANIMSTATES),
   skills S2 guards/erase/status/environment, skills S3 projectile families,
   sound combat events (P0-SOUND lane A/B: appear only on respawn, erase
   event + 92, activation layers, 6/14/99/82/81/5), F2 RuleSet (P0-FRONTEND).
4. fourp S1d–e (teams/round end), then render S2–S3 (views, 2x2, per-quadrant
   dissolve), then HUD/frontend S4–S6, then F3 modes.

Render queue: charlight L1 -> shadows L2 -> stage per-pixel L3; aura A1 ->
halo A2 (only if Laurence wants); effects P3 lanes after P0-EFFECTS lands.

## Merged so far (wave 2)

- aura (A1): effect-600 wisps, rules from program. Follow-ups: cam_fade
  pub(crate) coverage accessor (delete the mirror in aura_fx.rs); stale docs
  GAME.md "School aura", VFX-AUDIT item 6, cam_fade header, H3 handoff;
  TIER3_NEEDS_RESTART const is Laurence's call; halo lane A2 only if he asks.
- builder (F1). Follow-ups: setup.rs:141 use `check_kind`; F2 takes
  `arsenal::aura_handicap(kind)` at combat.rs aura_rate_pct call; quick decks
  into builtins (main.rs).
- animargs. Requests carried into sim lanes:
  - CAST-STATE: aim turn 20 deg/tick instead of snap (then flip
    `faces_sim_exactly` to exact for casts); jump wind-up state (JUMP_START,
    blend 5, ~13 ticks) before take-off; expose combo seek.
  - REACTIONS: per-hit counter (restart same clip), DOWN_* loop after
    BLOW_LAND, landing clip by fall-height class, KO pose from killing blow.

- charlight (L1): SWBOS port; all 25 characters re-exported on the main
  cache (backup `~/.pdtools/cache-chars-backup-20261001`). Open: script
  colour order (default "reversed" vs `PD_CHAR_LIGHT_ORDER=forward`) --
  Laurence's pick; stage lane must re-check `gltf.rs::light_vertices` order;
  scene-light fade trigger unread (audit R6); LIGHT02 height unverified.

- fourp (S1a-e): merged 0952fcc. FriendlyFire default On in code (faithful
  suspected); DECISIONS says setup option default off -- the four-player
  setup lane adds the option and flips the default there. Deviation: invalid
  locked target hands over at once (program waits for the button).

## Wave 3 -- running

- castrules: CAST-STATE + skills S1 rules core + F2 RuleSet (owns sim).
- fourview: four-player input/cameras/views/per-quadrant dissolve (owns main.rs
  non-anim parts, cam_fade, views.rs, seats.rs).
- still from wave 2: fxrender (effects.rs), shadows, collision.
- next after castrules: REACTIONS, skills S2/S3, sound combat events, FX-C
  events; after fourview: HUD S5, frontend S6 + F3 modes.

- collision (C1): merged; main cache = 0x94 + render walls (rails kept),
  stairs walk. Open: Laurence checks Refinery deck edges (pure 0x94 vs hybrid).
- fxrender (FX-A): merged; PD_FX_BLEND default 2 until glow pass; PDEFTAURA
  full-alpha premultiply added at merge. Effects re-exported on main.
- castrules: merged; skills.json regenerated (rules, handicaps, quick decks).
  Follow-ups: hud.rs health bar `TICKS = START_HEALTH` -> max_health (HUD
  lane); F3 wires Game Rule row (`Rules.set`, `use_rule_time_limit`,
  `set_arsenal_kinds`); fx/sfx use `CapsuleItem::appear` (events lane).

## Wave 4 -- running (SIM-LANE-BRIEF.md fences)

- reactions, guards, projectiles (parallel, function fences in combat.rs).
- still running: fourview, shadows, fxtime.
- next: EVENTS lane (sound P0-SOUND A/B + effects FX-C: new CombatEvents ->
  sfx.rs + fx.rs) after the three sim lanes merge; HUD lane after fourview.

- fourview: merged (1-4 players, seats, per-quadrant dissolve/aura). Target
  switch on LT unconfirmed. P4 camera can sit inside a near fighter at spawn.
- shadows (L2): merged; stages re-exported (`stages --collision dat+walls`).
  Gaps: st03's big window-light pools (fmt 0x10 volume never extruded,
  SSHADOWP point-light variants), fighter hull casters (character.rs export),
  darkening a bit weaker than original (needs SBTS bump, lane L3).
- fxtime (FX-B): merged; effects re-exported (vertex colours, _t<n>.png).
  Open: PD_FX_STEP stepped motion (Laurence), BB07/BB08 env reflection
  constants, msEFTBB01S slot 3 vs 0, word-28 modes 3/5/6/7/8 (68 pieces),
  glow pass (read, not built: separate job).

- setup4: merged (1-4 seats, teams, rules, friendly fire, results). Open:
  spare pad mid-match can take a free seat (seats.rs); settings split-screen
  row not forced for 3-4 players; a drawn round with Rounds=1 ends as DRAW.
- running wave 5: hud, stagelight (L3), glow, + reactions/guards/projectiles.

- reactions: merged. Open: Jump skill shows WAIT0 after take-off (should be
  JUMP_UP/DOWN); reaction root motion; kinds 7/8/9 + paralysed struggle;
  left/right side-hit mapping swapped by judgement (Laurence look).
  Demo note: st03 tree pit = fall-out (3 HP), camera dark during the fall.
- hud: merged (foe list, allies, max-health bars, pause banner).
- glow: merged. The original has NO glow target (dual-source blend; re-read);
  optional modern bloom, OFF by default (`glow::GLOW_DEFAULT_ON`).
- guards: merged. Open: front-guard disc (half-plane now), status 12, Unison;
  crystal-captured event (0x58 sound) for the events lane; PD_TEST_PRESS2.
- running: projectiles, stagelight (L3), fx4 (4-player KO crash in fx/sfx).
- NEXT: EVENTS lane (all new sounds + effect events) after projectiles + fx4.

## Added by Laurence 2026-10-01

- **Collision**: some maps need investigating -- e.g. the stairs in Lost
  Palace (st03, default arena) can't be walked up, only jumped. Audit
  `audits/P0-COLLISION.md` (running), then a fix lane.

- castrules phase 1 (CAST-STATE + S1 rules core), branch `lane/castrules`: see DECISIONS 2026-10-01 row. Requests carried: main.rs `clip_for` plays JUMP_START while `Player::jump_windup() > 0` (blend 5); HUD/hitstop use `Fighter::max_health` instead of `START_HEALTH`; effects/sfx can read `CapsuleItem::appear` (appear sound / fade-in); combo seek not exposed (combos unbuilt).

- stagelight (L3): see STAGE-LIGHT.md. Open: SSHADOWP/CHP in-battle variants, LIGHT02 emitter
  height, SBTSREF reflection family, placed-object volumes still shadow maps. Requests: main.rs
  none; re-export stages + chars; Laurence to look at the st03 pools and the light direction.
- reactions (branch `lane/reactions`): see DECISIONS 2026-10-01 "Lane reactions". Requests left: sound (landing/wall-hit/KO events need a lane: no new CombatEvent was added), effects/HUD none, animation: the Jump skill plays WAIT0 (cast row) after its wind-up while airborne -- should show JUMP_UP/JUMP_DOWN (castrules/animargs), root motion for reactions/get-ups still unbuilt (Lane 4), bind/pinned/lift kinds (7/8/9) and the paralysed struggle still unbuilt.

- **Opponent HUD panel (Laurence 2026-10-01):** "top-left opponent bars are
  wrong; the original shows what type of moves they have bound; ours shows a
  second health underneath". Original panel (his footage): name, Health +
  dot row, bottom-left = coloured pills per bound skill kind, middle =
  arsenal left, right = Aura/Level. Lane `foepanel` running.

- Critic fixes merged: fixbomb (additive puffs not cut-outs), fixst08 (stage
  aux models not exported; also removed invisible halo walls from collision).
  Stages re-exported. Open: st09_07 black slabs (other vertex family);
  Timer Mine x10 blast fills the screen (check scale reading); Bomb fireball
  faint dark stepped edge; aura gaps in its 30-frame loop (loop flag unread);
  4-player "PLAYER n" world marker overprints the foe list.
- foepanel: merged (opponent panel = original: Health + dots with >20 tier,
  bound-skill pills from atlas hud0 78,402, arsenal left, Aura/Level, red
  loss trail, target glow). Unconfirmed: pill diamond slot mapping, white
  Aura Particle pill, downed look, teammate colour (program uses green).
- RUNNING (2026-10-01 morning), worktrees `..\pd-lane-<name>`:
  - charselect: MERGED (746 tests). Open: rules-screen seat column shows
    "124" style seat lists; real-pad join untested. Was: join prompts ("press any button"), portrait previews facing
    camera, rules moved to a second screen, no mid-match seat grabbing.
  - crosshair: lock-on marker from Laurence's Lock-On Animation.mp4 (too
    opaque, wrong shape), moving marker code to hud_marker.rs.
  - auradial: MERGED (753 tests; FUN_140151f90, 10 segments). Open: dial
    fade/tint modes, digit colour a bit redder in footage. Was: bottom-right dial centre lights up clockwise while
    Aura recharges (new hud_dial.rs, small hud.rs hook).
  - caststill: MERGED (758 tests). Program has NO hold for Aura Particle
    use or capture (0x14025bd40 states 10/11 only for real clips); hold of
    80 ticks built because Laurence asked. Follow-up running: idle pose +
    hold length from footage. Was: using an Aura Particle or a green (Status) skill
    "etc" holds the fighter still -- rule read from the program (sim).
  - caststill follow-up MERGED: idle pose, hold 46 ticks (footage glow). SUPERSEDED: hold + capture lock removed (live trace, see below).
  - crosshair: MERGED (771 tests). Open: colours fitted to footage (vertex
    colours untraced, orange least sure); distance readout number not built;
    name label full-view only (NAME_IN_SPLIT); Y strokes thinner than footage.
  - aurapulse: green body glow on Aura Particle use (0.77 s in footage),
    maybe on capture too (FUN_14022e8a0).
  - aurapulse MERGED (778 tests). Glow MEASURED (64 additive ghost draws,
    draw method UNCONFIRMED, slightly more ring-like than footage haze);
    sound 89 READ + 90 MEASURED (audio fit, source unread); capture has no
    glow. HOLD: none -- settled by the moverec live trace (1.85 u/tick through the
    use, 4 runs); caststill removed the hold and the capture lock.
  - capbirth MERGED (784 tests; capsule sprites re-exported on main cache).
    Open: pickup/erase bursts (slots 2/3) still stand-ins (data now exported);
    fx.rs fades a respawned orb from alpha 0.05 (program: 255); opponent view
    = dark specks (READ, footage can't confirm); quad corners UNCONFIRMED.
  Rule from 2026-10-01: every value READ or MEASURED, never guessed.
  - skillaudit MERGED: 341 skills, 263 clean, 27 crosses, 35 need a take;
    top-50 used: 0 crosses. Ledger audits/SKILL-BY-SKILL.md, tool
    engine/re/skill_audit.py --run. RUNNING fix lanes (disjoint fences):
    decks (INDEX, discovery.rs), instants (INSTANT-HOLD decided = program,
    INSTANT s65 delay, RULE26; combat.rs), moveskills (MOVE, SWAP, GIFT,
    CHARGE; moves/*, sim.rs), loaders (LOAD 257/294; main.rs/sfx.rs loaders).
  - MERGED: loaders (257/294 own records), decks (built-ins read table
    indices; Psycho starter holds 2x Bullet of Fire as stored), instants
    (WAIT0 rows fire at the press; s65 delay; rule 26), moverec v2 (video via
    gfxcapture hwnd; pilot sheet pd-lane-moverec/engine/shots/mr_pilot_sheet.png).
    783 game tests, 119 py. Still running: moveskills. Then regen
    SKILL-BY-SKILL.md (skill_audit.py --run; INDEX row text hardcoded).
    Recorder limits: ASUS OLED screen saver after 20-30 min stalls runs
    (batches of ~60 or Laurence turns it off); split-screen views only.
  - skillaudit (Laurence: next focus): per-skill program-vs-ours tool, usage
    ranking, ledger audits/SKILL-BY-SKILL.md, fix lanes by (skills x usage).
    Scope decision: versus only (no story mode).
  - moverec MERGED (84 py tests). Proven in logs: memory pad input, casts,
    match start without menus, any skill in any slot (unowned too), idle P2.
    No video yet (screen saver -> capture denied; needs Laurence at the PC).
    Laurence owns 126/399 skills. LIVE: Aura Particle use does NOT hold
    (1.85 u/tick through it, 4 runs) -> caststill removing the hold.
    Lead to skillaudit: deck ids may be catalogue ids vs game text_id.
    Was: automatic move recorder -- input
    written into the original's memory via the debugger (no OS input), decks
    in memory, window capture + aligned trace. His save backed up to
    .pdtools/original-save-backup-20261001. Pilot also settles the Aura hold.
  Merge each after looking at its shots; then rebuild pd_game for PLAY.bat.

## Wave 6 -- running (brief: WAVE6-BRIEF.md), Laurence away
- fxopen (Timer Mine scale, aura loop gaps, Bomb edge, heat haze PDEFTOPT05)
- capfx (pickup/erase bursts slots 2/3, orb fade-in from 255)
- reactions2 (Jump airborne clip, kinds 7/8/9 + struggle, root motion, combos)
- hudbits (4p world marker, rules "124", crosshair distance readout, foe panel)
- probes (audit coverage: guards, auto shelters, STR21, INDEX row; regen ledger)
- aiaudit (read-only P0-AI.md: the original's CPU opponent)
- MERGED: hudbits (790), reactions2 (804; root motion: BLOW_LAND_LF/LB drift
  -60/+72 u in colour file UNCONFIRMED -- check vs trace/footage; combos NOT
  built: needs cast-frame tracking lane), capfx (807; orb fade = per-spot
  memory, READ), aiaudit (P0-AI.md: original versus has NO CPU seat; CPU =
  eneprog006 bytecode VM; 5 questions for Laurence), probes (ledger regen:
  286 clean / 26 cross / 24 take; new lanes BLAST3 GUARDADD AUTOSHELTER REQ5
  ORBPAY(T) COPYNO).
- CPU (Laurence: yes, from the original's AI): RUNNING cpureads (lane A:
  close reads, FORMATS-CPU.md) + ssbvm (lane B: .ssb/.epa parser + VM in
  pd_formats) -- ssbvm MERGED (pd_formats 87; real eneprog006 runs 600 ticks
  on a stub); cpureads MERGED (FORMATS-CPU.md, py reference VM/cad/threat).
  cpuhost MERGED (873; cache/com exported; --cpu <seat>:<row>; 83,689-call
  xcheck vs ssb_run.py 0 mismatches). RUNNING D cpuseat. Next: C cpuhost (pd_game/src/cpu, needs main.rs input + sim lock
  hooks), D cpuseat (setup; decide strength/arsenal/style then).
- guardfix MERGED (818): ledger 2 crosses left (ORBPAY, needs a take).
- fxopen MERGED (825; effects cache regenerated with 311 bump PNGs):
  Timer Mine fixed (MSPARTICLE size words), aura wisps now spawners (still
  sparse, unverified vs footage), heat haze built (Psycho Wave; bump layout
  UNCONFIRMED; looks strong -- Laurence look), Bomb dark rim PARTIAL (needs
  gamma-space subtract via scene copy), guard stand-in bubble removed (6
  defences show nothing at cast = program). Open: area effects not played as
  slot 1 (mine dome).
- fxfollow MERGED (831; fx cache regenerated): 392 reverse-subtract/MAX
  pieces now gamma-space via scene copy (Bomb rim gone, Psycho/Photon Burst
  slabs gone). Mine dome already showed (fxopen note was wrong). Aura wisps
  READ-built but footage halo is ~3x bigger/softer -> RUNNING auralook (find
  what draws the halo). Open: Rain column slot 1 stop (needs trace).
- Open for later: HUD names use a thin smooth font (game has
  Assets/Data/media/ArialUni.Ttf) -- ours pixel font; combos lane.
- moverec trace-only pass DONE: 334 ok, 7 refused (expected), 0 wrong-skill;
  live audit 1870 match / 5 cross / 47 unconfirmed. Summary
  .pdtools/moverec/full_trace_summary.csv. RUNNING livefix (auto-guard cast
  sound x4, Dash fire tick, root-motion drift measured live) -- MERGED: live
  audit 0 crosses. Open: small-landing slide 1.7 vs 6.3 u per skill (no
  program reason found), pc00a large landing needs a swapped take.
- VIDEO PART 1 DONE 2026-10-01 18:11: top-100 skills filmed, 100 ok,
  takes_full/. Save untouched by the run (5 files already differed from the
  morning backup before it started). Remaining 241 = same command with
  all_skills.csv (resumes).
- VIDEO PART 2 DONE 18:50: ranks 101-200, 197 ok / 3 refused (req-5
  skills). 200 of 341 filmed.
- VIDEO PART 3 DONE 19:10: ranks 201-241; 235 ok / 6 refused (all expected:
  req-5 x3, Orb, Amulet, Recollection). Left: 100 (ranks 242-341, ~35 min).
- VIDEO PART 4 DONE 21:35: ranks 242-291, 284 ok / 7 refused total. 291 of
  341 filmed; left 50 (ranks 292-341).
- combos MERGED (910; skills.json regenerated with action.chain): HSP, Rapid
  Cannon, Trench Mortar chain (READ). LIVE check script in combos report.
- stagefix MERGED (stages regenerated): st09_07 aux models (AR0n MD00) fixed;
  17 versus stages swept clean; st14/st26 dark (unchecked); st03 pools: no
  unbuilt part identified.
- VIDEO COMPLETE 21:58: all 341 filmed, 334 ok / 7 refused (expected).
- UNLOCK DONE 22:00 (Laurence asked directly): engine/re/mr_unlock_all.py
  raised stock to >=9 (Aura Particle 99) and called the game's own profile
  save FUN_1401cff20; relaunch shows 363 owned / 34 not (= the unregistered
  enemy-only records).
- 2026-10-02 FILM COMPARE: film_compare.py merged; 341 pair sheets in
  .pdtools/compare (index.csv worst-first). 89 clean / 121 one mismatch / ...
  Classes: sounds (194 missing / 140 extra, systematic), ours-hits-original-
  misses (~53), melee hits 10-16 ticks late, effect deltas (Judge of Anubis,
  Meltdown, Boost Mine, Panic Gas, Psycho Kinesis...). RUNNING: soundsys,
  hittiming, skilleffects. Next: matched-camera visual pass (effects look
  bigger/brighter; Bomb hand shells -- bombhand partial NOT merged: global
  gamma-add caused Splashdown black blob).
- skilleffects MERGED (911): holes hurt caster, Boost Mine to tripper, Fusion
  target-only; harness fixes (fall-outs, KO restart). soundsys MERGED (912):
  shot hit sound 6/7 by motion, landing 70, self-status sounds. NEXT after
  hittiming: lane soundevents = 0.61 jump/float start event, 0.86 capture +
  Add-effect status sounds (log StatusEffect where applied), 0.91 button-erased
  rule (erase.rs); then a full film_compare re-run; then matched-camera visuals.
- hittiming MERGED (914): cast-clip root motion, charge cycle; harness
  set-asides. soundevents MERGED (922): jump take-off 61, Add-effect/capture
  sounds, erase-capsule 91; film sound diffs 60->38 on 31 skills. Open: Demon's
  Wing take-off (moveskills), landing sound by floor material (70/71/72
  UNCONFIRMED), recorder should log P1's real buttons. leftovers MERGED (929): STR28 one roll per cast,
  Antigravity Trap no hover, regen freeze 60 prog ticks after deciding KO,
  film_compare --real. Open: Stone Shot aim origin (+0x68), Lucifer's Arrow
  random, Thor's Hammer platform. RUNNING filmrerun (split-camera full re-run
  into .pdtools/compare2 + visual classes).
- 2026-10-02 visual wave MERGED: fxlight (no floor disc), fxlook (ribbon
  emitters), visualpass (fair top/bottom crop; compare3 60 sheets), menus
  (sounds READ+LIVE, fades), moverec LIVE menu/camera/clouds, camcloud
  (camera 55/13 free, 52/31 -19.1 locked, hFOV 74.7; MID setting = original;
  menu cloud scroll; per-stage sky drift, cache stNN.sky.json). Open visual
  classes (compare3): effects bigger/brighter on ~24 skills, colour on ~10
  (fxlook: likely aura ribbons in the original frames -- aura PARKED by
  Laurence), flat sheets (28,149,126,64,39), pose stays in cast after fire
  (102,19,120,21), hard red floor discs (22,86,147,148,175,312,319,120),
  camera swing/pull-in UNCONFIRMED.
- fxsheets (2026-10-02, NO code change: no READ cause found for "flat sheets" 28/149/126/64/39 + Bomb hand slab). Findings, so the next lane
  does not repeat them:
  - Not word 19 / culling / billboard: Psychic Wall `PD_FX_CULL=0` and `PD_FX_BILLBOARD=0` renders are pixel-identical to default.
  - Psychic Wall eft0117 slot 0 (m0) = half-ring meshes (z >= 0 only, 11 columns over 180 deg, radius 10..19.8, height 19.7, vertex alpha 0 on the
    bottom and top rows) x7 (d2-d6, d20, d21) + 13 flat lightning quads (d7-d19, 4x2 flipbook, word 19 = 2) + a ground swirl. The mesh data really is
    a half dome (every vertex of the buffer is used; nothing dropped by the exporter). Our wedge look = that half dome seen from outside. Hide-bisect
    (`PD_FX_HIDE`) shows the wedge comes from the m0 dome pieces together (no single piece removes it).
  - Original trace (`takes_full/028_*_trace.csv`): effect 0.117 starts at press t39 and again at fire t41, same as ours; but the recorder's film shows only
    thin blue streaks until ~t60+ and the purple dome around the caster only ~2 s after the fire (video ~4.3-5.5 s), a high far camera. Why the original's
    dome is late/absent at press+0.5 s is UNCONFIRMED (not a piece-type/blend difference we can read).
  - Photon Burst (eft0110): the lavender sheets in our frames are the FLAIR01 emitter (650e9921, model 0, frames 0-20, 7 rays/frame, life 7, length
    r*20 -> r'*40, tip width r*30 -> 0), flat wedges (READ `FUN_1401a1620`: base corners both at the origin, tip corners +-0.5 width along the rotated Z
    of three random whole-degree Euler angles, `FUN_1401a1170`); hiding all seven m0 pieces leaves them. The original take shows NO such burst at
    t42-t50 (it shows green flames later). UNCONFIRMED whether the original draws them at all there. Small READ gap: the per-ray angle `+0x14` grows by
    emitter word 20 (as int) every drawn frame (`FUN_1401a1620`), ours never spins the rays.
  - Bomb hand slab: it is the eight shells m0_d1..d8 (hide them and it is gone; one shell alone is a thin ring). Each is a 51-vertex barrel band
    (radius .47-.55, height .3, u = -1..2 around, v .5..1.5), grown x11 (word 16-18, curve 9 = 2), ONE,ONE, word 28 = 1 latched. Eight stacked bands
    over the dark body read as a filled orange disc; the original (bombhand lane measurement) shows thin soft rings with dark gaps. Cause UNCONFIRMED.
  - Shader facts re-read (tools/disasm, PDEFTBB01_vs/ps): vs `o1.xyz = (v1*c35).w * (v1*c35).xyz`, `o1.w = (v1*c35).w` (weight = vertex a x piece a: ours
    matches); ps clamps the interpolated colour to [-1,1], discards texture alpha == 0 when `cb0[23].x != 0` and `alpharef >= tex.a * colour.a`,
    writes `tex.rgb * colour.rgb`, `a = tex.a * colour.a`. Nothing there explains the sheets.
- fxbright (2026-10-02, lane/fxbright): "effects bigger/brighter" has a READ cause for part of the class, and it is the TAKES, not our renderer.
  `FUN_14018a040` (effect request) returns -1 and only asks for the load (`FUN_140189e80` -> `FUN_14018cd10`, asynchronous, resource "Effect") when the effect is not resident; the
  request is DROPPED. The program preloads on hand-selection change (`FUN_14012d150` -> `FUN_140189e80`). A recorder take that selects a cold skill and presses within a few ticks has an
  original with no cast effect on screen (Meltdown: requests at press, +30, +56 rel; the dome shows only from the third, frames 26-45 = t95-114/t121-140 as filmed; Dragon Punch,
  Photon Wave, Photon Burst, Lightning Sword, Thunder Storm, Empty Hand ... have requests only within 25 ticks of the press: nothing but the school aura on screen). Warm effects (Fast Bomb
  shares eft0097 with Timer Mine) play from the request. So the compare3 coverage ratios for cold takes measure "effect dropped", not size. Tool: `engine/re/fx_measure.py` (coverage + request
  ticks per sheet). Recorder fix (orchestrator): select the skill, wait >= ~60 ticks (load time UNCONFIRMED, between 30 and 56 ticks for eft0265), then press; or re-film cold takes.
  Also found: the whole original blends in GAMMA space (pixel shaders do no conversion; `CreateRenderTargetView(pDesc = 0)` on the flip-model backbuffer = UNORM), ours adds the stored value in
  LINEAR light on an sRGB target, so a stored 0.05 over black shows as 0.26 (original 0.05): our additive glows read brighter in the faint tails over dark floors. Exact emulation needs gamma
  blending in the main target: Bevy 0.19 `CompositingSpace::Srgb` does that for 2D only (no 3D shader support), `fx_gamma.wgsl` modes 1-3 would then output the stored value unchanged; NOT
  built (global change, every custom shader would have to encode). Any per-piece curve without the destination is a guess (stacked layers sum in gamma space): not shipped. Photon Burst
  FLAIR01 word 20 (READ `FUN_1401a1620`: `+0x14 += (int)w20` per drawn frame, units 1/0x8000 turn): the data has w20 = 0 for eft0110; only eft0235 has w20 = 30 (0.33 deg per frame): implemented
  (`flare_spin`, test) but invisible on Photon Burst, which therefore has no spin to add. Laurence's footage (Laser, Blaster; `shots/kfx2_laser_blaster_footage.png`) shows our effect core as
  bright as the original (p99 luma 252 vs 231-255), the stage under it brighter (mean 70-90 vs 41-56), so no global brightness change from that evidence.
- superblast (2026-10-02, lane/superblast): (1) world-wide blasts (s43 = 0, s44 = 2, motion 4, s64 = 2: Meltdown, Quantum Decay/Amp, Fairy Ring, Turbulence, Chill Breeze, Blessed Rain...). READ `FUN_140265f50`: after s65 program ticks it calls the hit `FUN_14026f870` on every live player, the caster included; its vtable `+0x48` (`FUN_14024fd80`) plays the s60 reaction row (bearing = the victim's own facing for motion 4) and only when the Add effect `FUN_140272bc0` returned nonzero. That function returns 0 for "nothing changed" only on some codes (erase 0, 1-4, 9-11, 17, 39, 45, 47-49, 51: `add_can_return_zero`); the status codes and Health/Level +n (43/44) always return 1. Built in `land_delayed_effects` + `status_skill_reaction`. LIVE: both fighters state 14 ~51 ticks (Purify / Armaros / Return to Nature / Vicious Balance / Judge of Ares with nothing to change stay idle: matches). Meltdown's Level loss needs on-field Aura Particles (spot state 2, kind 0: `FUN_140272bc0` cases 0x30/0x31); the film harness has none, so Level/Aura still differ there (harness, not rule). Spore Schism is NOT this class (s44 = 0, a Crawl): the original crawl blows up ~13 ticks after the fire on/next to the caster (Health -8, state 17); cause (object in path / owner not excluded) UNCONFIRMED. (2) guards: `FUN_140271760` calls the holder's `FUN_14024ff20` (controller vtable `+0x88`) on every block except the Course / Reverse kinds: the cast clip restarts at frame w44 (30 on all guard rows) and the cast state's timer becomes `clip length - w44` (so press + 38 for a 40-tick clip, block at 66). `FUN_140264020` / `FUN_140264340` (guard create / update, decompiled with DecompAt) only set size / life (s46 - 2) and never touch the holder. Built as `guard_block_recoil` (holder still in the guard's own cast, not the automatic Shelter). Film (102 skills, --real): mismatches 125 -> 111, cast_end fixed on 10 guards + 6 blasts, no new kinds except Blessed Rain (a fall-out of P1 on st06's edge after the reaction's root drift; pre-existing P0 fall-out). Open: reaction root motion of these s60 rows (originals drift +-4 u and return), reaction on an IDLE holder of a standing guard (program starts state 10 anyway; not built).
- hitdiff (2026-10-03, lane/hitdiff; corrected z-mirrored `--real` harness only): film_compare field + fairness + one rule. Results `.pdtools/compare_hitdiff_base` (before, all 341, main's tool) and `.pdtools/compare_hitdiff` (after, 147 skills = the 58 with a non-sound mismatch + 16 touched + 50 random clean + extras; no new mismatch anywhere). Before: 267 mismatches in 154 skills (58 skills with a non-sound one: hit_tick 34, p1_health 25, cast_end 10, fire_tick 6, original_no_fire 7, aura/level 21, effect 6). After, on the 147: 194 -> 169 (non-sound 112 -> 87; 91 -> 79 skills). Fixed: (1) Task 0: the trace does not log the field; `PD_FILM_FIELD` (inert) puts capsules on the stage's own spots and `field_from_take` infers the counts from what the skill did (Meltdown s51 = 49: Level loss / s52 per fighter = 2 + 1 Aura Particles, LIVE `102_Meltdown`; Overheat STR 3 = the same 2 + 1; Evaporate two common/92; Violent Change six; Particle Convert / Change the World six capsule effects: the recorder's stage held 3 capsules per fighter). Meltdown 5 -> 0 mismatches (Level, Aura, cast end), Evaporate / Violent Change sounds 0.92 now match. (2) READ `FUN_14025c720`: a charge shot (Gungnir, Ki Cannon) released inside the wind-up keeps the cast to press + clip + 1 program tick (LIVE: state 10 until tick 86 = press + 47; ours restarted the full clip at the release, +13). Ki Palm (melee charge) already matched. (3) Tool: a real start our collision moves 2+ u (bimodal 0-1 u vs 3-5 u) or whose floor is not at the take's height falls back to the flat catwalk; fall-outs found by the respawn jump incl. UP from below the floor (a body that slid 9 u off an edge and respawns 14 u away was counted as a "hit"), ignored after the take's last tick; scratch frames deleted after each sheet. NOT fixed, by class (non-sound mismatches before): collision 24 (10 skills where our shot dies on a `dat+walls` stage wall, plus Block / Spore Schism; A/B with a plain-`dat` st06 via `PD_FILM_CACHE`: 10 of the 29 hit / no-hit skills tried go clean, 53 -> 33 mismatches; the collide lane's `dat+walls` finding, their class-0x10/0x12/0x14 shot skip is not built); edge falls 16 (root motion slides ours off an edge the original stops at, e.g. P1 after its Blaster at anchor (286, -317); tool sets them aside); recorder never fired 20 (5, 11, 14, 180, 205, 206, 299); shell / bomb flight time 5 (ours earlier by 1.5-3.5 program ticks on ATK_THROW / FAITH_SHOT / SHOT shells, Rapid Cannon (ATK_LASER) exact: UNCONFIRMED, not distance and not speed; fire-event vs object creation delay is a candidate); 13 hit / no-hit singletons UNCONFIRMED (Empty Hand dash stalls at z 140.88 in the original: nothing there in ours; Sensor Mine: the original's own mine hit its owner at fire + 64 (armed 60 + 4), ours never triggers; Reverse: the reversed Blaster hits P1 21 ticks later in the original, ours at once; True Muramasa / Level Baton swords in range never hit in the original; Thor's Hammer; Lucifer's Arrow random; Tornado / Blazing Storm rain cover; Restrain, Panic Gas, Boost Mine Level changes). Swift Kick: the original's dash goes on to dash_end (23 > fire 20) after the hit, chasing the knocked foe at ~2.1 u/tick (ours' foe flies 1.2-1.4: the knock launch speed, reactions lane). Dash (240): ours holds Cast through the dash, the original leaves state 10 at the fire tick (metric mapping only).
- csflicker MERGED: char-select swap waits until posed+textured.
- cpuseat MERGED (893): CPU seats, grades = epa rows 64..60 (nearest target),
  school decks or saved arsenals. PLAY vs CPU works.
- auralook MERGED (898): school aura glow = HUD aura object silhouette blur
  (not eft0600 wisps). Width ~2x narrow vs footage -> moverec live trace of
  FUN_14019b920/289140/2864e0 args -- DONE: LIVE skip shows HUD aura object,
  eft0600 wisps and fx600 are NOT the halo; aura_glow now OFF by default
  (PD_AURA_GLOW=1). Bisection round 2: drawer NOT found; colour = school
  table 0x140599080 (LIVE) but its only users were ruled out -> contradiction:
  re-check the fx600 never-spawn test (alternate spawn path? measure picks up
  red clothing?) with camera close + steady phase before more bisection. Video run still
  waits for Laurence (screen saver off).
- (history) moverec: index-mapping fix, then full 341-skill TRACE-ONLY run now (screen
  saver ok for traces); video run when Laurence is home (he disables the
  ASUS OLED screen saver himself -- system setting, not ours).
- moveskills MERGED (788 tests): Levitate/Skywalk rise+float, Dash 100 u /
  30 ticks, SWAP, GIFT, charge = s37==2 (Dragon Bullet is NOT charge). Float
  clips requested from reactions2.

## modes lane (2026-10-02): game-modes framework + Close Quarters

- MODES title row, mode list screen, `Rules` flags `capsules` / `infinite_uses` / `fixed_hand` (combat.rs, additive; defaults = normal match), setup locks for a mode. First mode Close Quarters (hand 3 / 29 / 240 / 75, Level 10, no capsules). Details: DESIGN-BRAWL.md "Game modes (built)", DECISIONS 2026-10-02.
- Files: `frontend/modes.rs` (new), `frontend/{title,setup,state,mod}.rs`, `combat.rs` (Rules + 4 small guards), `main.rs` (wiring, `--mode`, `--open-screen modes`).
- UNCONFIRMED: Aura refill rate uses the neutral kind 2 (1.0), my choice; a mode's CPU plays the fixed hand with the original AI as is (not tuned for this hand).

- settingsux (2026-10-03, lane/settingsux; proof `engine/shots/settingsux/`: `sx_sheet_match.png` = slider row, split live behind the dim, browsed resolution `A: APPLY`, the same row after A, browsed 21:9, after A with bars; `sx_slider_fov.png`, `sx_title_options.png`, `sx_look_page.png`): Laurence's TV list. (1) Display rows (mode, resolution, aspect, HUD) browse: left/right shows the next choice dimmed with an `A: APPLY` badge, **A applies at once and saves**, leaving the row drops it; the Apply row and the 10 s keep prompt are gone (`KeepPrompt`, `KEEP_SECS`, `keep_prompt_tick`, `PD_TEST_KEEP_PROMPT` and their tests removed). (2) Split, camera height and every Look row apply as you move, in a match too (tests drive the pause menu with mocked pads and the cameras / layout systems). (3) FOV and a new CAMERA DISTANCE row are sliders (bar, knob, value; tap 1, holding repeats and speeds up 1/2/4/8; saved on release); they use `menu::FOV_MIN/MAX` and `CAM_DIST_*`, the camera lane owns what they mean (distance is not yet read by `follow_cameras`: camfov). (4) First run (no display choice saved) = borderless fullscreen, native physical size, aspect Auto (`DisplaySettings::first_run`); capture runs stay windowed and never write `settings.json` (`PD_SETTINGS=<file>` redirects). (5) 4K: causes in BUG-GRAVEYARD ("4K never offered..."): lists were cut to the current desktop size and exclusive to 21 fixed sizes; the monitor never followed the window to another screen (`Added<OnMonitor>`); the default 1600x900 window is LOGICAL (4800x2700 physical at 300 %). Now exclusive lists every reported mode, windowed everything up to the biggest mode, `track_monitor` runs every frame, a "native" window is fitted to 90 % of the desktop. Startup log `display: monitor WxH scale S ... largest mode ... modes [...]` to stderr and `display_log.txt` beside `settings.json` (this PC: `monitor 2880x1800 scale 2.0`, MEASURED). `PLAY.bat` and `toolsats\*.bat` pass no window size or --windowed (checked). Files: `display.rs`, `menu.rs`, `frontend/input.rs` (`Repeat::held_left_right`, additive), `main.rs` (settings path, wiring, tests), docs FRONTEND.md "Display options", MENUS.md. Tests: 1033 game (display::tests 4K desktop 4K / 1080p x scale 1, 2, 3, first run, monitor follows window; menu::tests; pause_menu_input_tests browse / leave / sliders / every row in a match / cameras while paused / menu script). UNCONFIRMED: what Laurence's TV reports (needs `display_log.txt` from his run: the 4K cause on his TV is one of the three above); real borderless / exclusive switching on the TV (not testable offscreen); the camera-height row only moves the camera in top/bottom split (`follow_cameras` uses it only for strips: camfov's file); `engine\cache\settings.json` holds exclusive / 21:9 / HUD 75 % / FOV 110 and counts as a saved choice, so first-run auto-detect will not fire on it: delete it before Laurence's next run. Merge note: camfov changes `fov_deg` meaning and the settings key in `menu.rs` (same `Row::Fov`, `Settings::load/save`): expect a small conflict there; my build used a local pd_game version bump to avoid the shared-exe clash (reverted, not committed).

## camfov lane (2026-10-03): FOV and camera distance

- **Cause (Laurence: FOV does nothing, TB camera too close, camera too close in general):** `apply_fov` overwrote every top/bottom strip with the original's 74.7 degrees horizontal across 32:9 (about 23 degrees vertical, setting ignored); other layouts read the setting as the vertical angle of an 8:9 half, so a 16:9 view got about 27 degrees vertical (original: 46.7). A 1-human-vs-CPU match is a 2-player split (the CPU seat is an active roster seat with its own view), so Laurence's "too close" was these two cases.
- **Built:** the FOV setting (`Settings::fov_deg`, json `fov_h16`, old `fov_deg` ignored) is the horizontal FOV of a 16:9 view, default 75.0, range 50-110 (`menu::FOV_MIN/MAX/DEFAULT`). `views::fov_for(h16, aspect)` per camera from its own viewport: at least 16:9 keeps the 16:9 vertical angle, narrower keeps the 16:9 horizontal. Strip override and floor removed. `cam_dist_pct` (70-160, 100 = today) scales boom, height (incl. strip camera-height drop) and look offsets in `follow_cameras`; boom snaps when it changes. The camera-height row now also works in every layout (offset from MID, 0 at default). Capture switches: `PD_CAM_HFOV`, `PD_CAM_DIST_PCT`.
- **LIVE correction:** the projection's x scale 1.3032 is 75.00 degrees (cot 37.5), y scale 2.3159 is 46.71; GAME.md's 74.7 / 46.78 were arithmetic slips. Default is 75.0 (brief said 74.7).
- **Files:** `main.rs` (`follow_cameras`, `apply_fov`, `cam_dist_scale`, tests), `views.rs` (`fov_for`, `viewport_aspect`, `REF_ASPECT`, tests; unused `ViewLayout::aspect` removed), `menu.rs` (constants, key), tests in `display.rs` / `look.rs` adjusted for the old key, GAME.md, FRONTEND.md, BUG-GRAVEYARD.md.
- **Tests:** `views::tests::{fov_rule_per_aspect, the_default_is_the_originals_projection, no_view_shows_less_than_the_16_9_picture, a_bad_aspect_falls_back_to_16_9}`; `camera_setting_tests::*` (real systems: 100 = today's free and locked positions, scaling keeps the angle, strip drop scales, live apply both ways, per-camera aspect, json key).
- **Pictures** (`engine/shots/camfov/`): `before_after_1080.png`, `before_after_4k.png` (1 view, 2P side by side, 2P top/bottom, 3P at defaults, fresh settings), `sheet_settings.png` (FOV 50/75/110, distance 70/100/160, also in a top/bottom strip), `tbsweep_sheet.png` (strip look-drop sweep, locked vs free), `film105_original_vs_ours.png` (original take 105 free camera vs ours, 16:9).
- **UNCONFIRMED / for Laurence's eyes:** (1) a duel starts locked on, so the default match camera is the approved 60/30 chest-aim; with the wider FOV the fighter still sits low in a top/bottom strip when the foe is above (the free camera sits well: original framing). `PD_TB_LOOK_DROP` 4/12/20 sweep is in `tbsweep_sheet.png`; no new default chosen. (2) Side-by-side halves are now 8:9 with the 16:9 horizontal angle (about 81 degrees vertical, fighter about 20% of the height); the original letterboxes each half to 16:9 instead. (3) Pull-in 3 u / 4 u limits left absolute (not scaled with distance) to keep the approved near-wall dissolve (9 to 4 u) unchanged; the brief said scale them. (4) Film framing is similar, not pixel-matched (placement/yaw).

## Blocked on Laurence

- Ear checks (review sheet): pickup 86, denied 2, menu cues 3/0/1, layers.
- xemu: all files now present in Downloads (xemu 0.8.136 zip, Xbox-Emulator-Files.zip
  with mcpx_1.0.bin + Complex_4627 + hdd). Laurence 2026-10-01: "park it, use if
  you get stuck". PC live tracer stays the main reference. His two traced runs
  (trace/sound_20260930_*.csv) are header-only -- no match was recorded yet.

- collide (2026-10-02, lane/collide): the film harness's `--real` placement was the MIRROR IMAGE of the stage (trace z is the .dat's; ours is z-mirrored). That alone explains the Jump take falling 'through' floors (it stood under a plate/ramp that is not there in the original) and the crawlers stopping early / our start pushed off a wall. Fixed in `film_compare.py` (`to_sim_xz`, `to_sim_yaw`; `PD_FILM_NOMIRROR=1` = old). 74-skill sample 106 -> 78 mismatches, Jump 1 -> 0. No sim change, no cache regeneration. READ: player ground test has no class rule beyond 0x1F / 0x14(states 5,7); object wall sweep (`FUN_140163e80`) skips classes 0x10/0x12/0x14/0x1F (not built: needs a class trailer in the .pdcl); `dat+walls` render walls are not the program's collision (Spore Schism, Throttle Blaster fixed with plain `dat`). Open: Tornado 311 / Blazing Storm 146 hit differences with the correct geometry. Details: COLLISION.md 'The takes are in the .dat's space'.

- collclass (2026-10-03, lane/collclass): (1) `.pdcl` gets a `CLAS` per-triangle class trailer (0x94 chunk flags / OC00 hull flags); object queries use the READ rules: shot sweep, crawler / tornado / hole ground, area roofs skip classes 0x10 / 0x12 / 0x14 / 0x1F (`FUN_140163e80`, `140163cb0`, `140164330`); the line-of-fire ray (`FUN_1401641b0`: shot-to-body, areas, blasts) is stopped by everything but 0x10 and now sees the class-0x1F faces (kept in the file in their own grid, invisible to bodies). Fighter rules unchanged (the airborne 0x14 rule only matters on st01: not built). (2) `dat+walls` retired: `pd_convert tracecheck` (new) walks the 682 take traces through both files: with `dat+walls` the original's fighters stand / walk inside the added render walls (22,749 steps 1+ u deep, 21,209 of them in a class-0xFE wall, 16,719 while standing; 172 of 6,333 moves cross a face, 162 an added wall); plain `dat` has no render-wall contact. Shipped collision = 0x94 + hulls (hull-less objects get no collider). Stairs walk on it. Film B set 77 -> 42 mismatches (19 -> 32 clean), 12 hit / no-hit skills fixed (130, 139, 140, 141, 149, 209, 251, 302, 305, 306, 260 ...); Triple Lasers 70 newly hits (arc shape, UNCONFIRMED). Cache: `pd_convert stages <Map> <cache>` (21 `.pdcl` changed, all else byte-identical). Details, READ table, numbers: COLLISION.md "Per-triangle class ...". UNCONFIRMED: Tornado 311 / Blazing Storm 146 / Leg Throw 183 as before; edge-fall slides (reaction difference, the floor exists in both files); `st04_entA_` invisible class-0x10 hull; the shot filter byte `DAT_1404bec60+0x10`.
- shotclass (2026-10-03, lane/shotclass; corrected z-mirrored harness; after-runs `.pdtools/compare_shotclass` (pre-collclass main) and `compare_shotclass_m` (merged with collclass main), identical): one READ rule explains items 1, 2, 3 and 6, and the lane's "hit tick later in the original" bias everywhere. READ `FUN_1402638d0` (every object's per-tick updater) + `FUN_140263a00` (creator): a new motion object is in state 1, its first update only turns it into state 2 and returns, so it first moves / counts down / is tested one program tick after the fire tick; LIVE: all 16 punches / swords fire 59 -> health change 60, Meteor 24 vs 23, Tremor family 32 vs 31, the four delayed instants (Meltdown, Quantum Decay, Quantum Amp: fire 69, Level at 95 = s65 25 + 1), Sensor Mine (fire 43, off at 103 = s66 60). Built: shots (`Shot::armed`), areas / mines / holes / rain (`Area::settled`), blasts (`Blast::armed`: the explosion object's first test is a tick later), delayed instants (+1 tick); `+0x1c` age is counted before the handler runs so ramps see 1 on the first step (Psycho Sniper unchanged, constant-speed shells +1 tick; a simulation of the 15 free-flight shells with both rules lands within +-1 recorder tick of every take except Seal of Death +2). Item 3: `FUN_14026cd70` code 6 (Reverse / About Face) sets `+0x20 = 0x14`: the shell waits 20 program ticks (`REVERSE_WAIT_TICKS`, via the guard-contact path `send_back(.., wait)`; Reflect code 5 waits 0): Reverse 88 vs 88. Item 2: `FUN_140267b90` puts a mine on the floor under the fire matrix's x / z (marker or hand, probe +-10 u around its y), not under the caster, and the sensor / blast test includes the owner: Sensor Mine now hits the owner 4 ticks into the blast and the foe at 28 (take: 4 / 28), Boost Mine Level +2 both (take), Panic Gas, Restrain now match; a mine's blast ray starts 1 u above the floor (a ray starting on the floor triangle caught it: nobody was hit before). Item 6: metric only: the program leaves cast state 10 for state 3 ("special move", PROGRAM-RE.md state list) at the fire tick of a movement special (LIVE `240_Dash`: 10 at 39, 3 from 40 to 59); the sim keeps Cast (Aura refill and controls pause in both states), `film_compare.orig_metrics` now ends such a cast with the state-3 run: Dash 60 vs 60.5. Before / after on 127 skills (37 named + 40 projectile / mine / instant sample + 50 currently-clean controls, all `--real`): non-sound mismatches 51 -> 21, 18 skills better, 0 worse (hit_tick 21 -> 7, p1_health 14 -> 7, Level / aura 8 -> 2, cast_end 2 -> 1). Replay goldens: all seven re-recorded (every scripted duel's hit ticks move one program tick), movement-only golden unchanged; tests 986 game / 95 formats / 9 convert / 42 viewer + 278 Python. NOT fixed: item 4 swords: Level Baton and Empty Hand never dash in the original (P0 stops at z 140.88, same z in both: a stage wall the original has, collision lane; with the collclass cache_dat still not clean); True Muramasa UNCONFIRMED (same blade geometry as Muramasa Blade which matches; take shows P0 facing P1 within 5 deg, no guard: not geometry as far as read); item 5 Swift Kick UNCONFIRMED: the foe's xz speed after the kick is 1.19 u/tick in the take and 1.17-1.25 in ours (no knock-launch difference found); what differs is that our fighters stop 9.0 u apart (`sim.rs` BODY_RADIUS 4.5 x 2, an unconfirmed first guess) while the take stops at exactly 10.0 u (charaset body radius 5, measured in Swift Kick and Psycho Knife), so after the knock the take's foe is 11.2 u away (> the dash's 11 u stop: the dash chases again, state 5 at 72 = P0 falling off the edge) and ours' 9.3 u; player-player separation 10.0 is a candidate for the movement/collision owner (not changed: movement golden); item 7: Thor's Hammer (ours hits at +22, the take never; path passes P1 ~10 u above its centre by the read geometry) and Tornado (take hits 27 ticks after fire; P1 stands on the lower 270 deck 60 u below P0, ours' tornado ends at the deck edge) UNCONFIRMED; Excalibur / Power / Stone Shot / Lucifer's Arrow unchanged (recorder never fired / random / pre-existing). Files: `moves/mod.rs`, `combat.rs`, `re/film_compare.py`, `PROGRAM-RE.md` ("Object life: the arming tick").

- bodyradius (2026-10-03, lane/bodyradius; runs `.pdtools/compare_bodyradius` (46 melee / dash skills, exe B), `compare_bodyradius_before` (same 46, main exe `pd_game_base_1003.exe`), `compare_bodyradius_A` (spacing only, 6 skills), `compare_bodyradius_sample` (60 skills), baseline `compare_full_1003`): READ `FUN_14017d300` / `FUN_14024e190`: the charaset fields are `+0x0C` height, `+0x0E` **contact size** (10), `+0x10` hit radius (5, `+0x130`), `+0x12` centre height (9). (1) Fighter vs fighter `FUN_140250780` (vtable slot 0x70, first call of the mover `FUN_140254590`): horizontal gap < `size_a / 2 + size_b / 2` = 10.0 and `|centre_b.y - centre_a.y| < (h_a + h_b) / 2` -> pushed apart half the overlap each. Built (`body.rs`, `Player::body`, table of all 44 distinct records checked against the file). (2) Fighter vs stage `FUN_140255020` -> `FUN_1401959c0`: three spheres of radius `(h/2) / (h / size)` = size / 2 = 5 (`0x140255102 divss xmm14, [rax + 0x12ea4]`). Built as the radius of our cylinder (separate commit; the cylinder's top stays `BODY_HEIGHT` 20, a model parameter, because its three sample heights decide where a sloped edge pushes). (3) Hit tests (`HIT_BODY_RADIUS` 5, `HIT_BODY_CENTRE` 9), capsule pickup (15, a different function), lock-on bands, CPU margin (= hit radius): already READ and equal to the charaset value for all 11 playable fighters (test); enemies differ (6 - 7.5) but are not playable, not wired. No other constant was tuned for 4.5 (checked: `LUNGE_STOP_SHORT` 11 and `MELEE_DASH_RANGE` 80 are READ; `combat.rs` guard-field minimum was `BODY_RADIUS * 3` = 13.5, now a named UNCONFIRMED 13.5). MEASURED spacing, smallest gap while on one deck, original vs ours before -> after (42 melee / dash skills that close in): ours 8.9 - 9.0 -> 9.9 - 10.0 against the original 9.99 - 10.0 (Swift Kick 9.99: 8.93 -> 9.93; Psycho Knife 10.00: 8.94 -> 9.96; Swift Punch 9.99: 8.92 -> 9.90; Leg Throw 9.99: 8.93 -> 9.93); mean |ours - original| 1.11 -> 0.23 u. The 0.1 u dips below 10 are within the last dash step (ours at 60 Hz). Residual gaps are dash-end quirks, not radius: Ice Sword (orig 9.52, ours 8.56), Swift Blow (orig 9.00, ours 9.79), Ki Palm, Dhampir Fang, True Muramasa. Wall radius is also MEASURED: Level Baton's original P0 stops at z 140.88 (a stage wall), ours ended at z 125.2 with radius 4.5 (slipped through) and at 140.9 with 5; Empty Hand likewise (min gap to P1: original 25.13, ours 9.01 -> 25.24). Film (all `--real`): 46 melee / dash skills non-sound mismatches 17 -> 13 (Empty Hand and Level Baton 2 -> 0, hit_tick 4 -> 3, p1_health 4 -> 3, p1_aura and p1_level 1 -> 0), 0 worse, clean skills 38 -> 40; 60-skill sample (45 clean controls + the 15 non-melee skills with mismatches in the full run): 34 -> 34, all 45 controls stay clean, 0 worse; Swift Kick / Leg Throw / Swift Punch keep their cast_end (+3.5 / +3.5 / +10.5) and the P1 fall-out (our knock leaves 15 deg off the take's: attacker approach angle at the hit, not spacing; shotclass item 5's "dash chases again" is not what happens: our P0 is in Cast, not Lunge, 11 u out). Replay goldens: effects, effects2, effects3 moved (spacing: with the contact set back to 9.0 all three return exactly); weapons, traps, tricks, capsules unchanged. The movement-only golden moved for seed 0xdec0de1 only (wall radius: the script walks into the arena's centre wall; no push-apart fired in either seed; wall radius 4.5 returns the exact old hash; per-character height in the wall cylinder also moved it, which is why that was not shipped). Tests: 1013 game / 95 formats / 9 convert / 42 viewer + 278 Python. Feel: speeds, turning and the walk are untouched; the camera has no body-radius input (fixed follow distance, pitch from the foe distance, which now bottoms out at 10 instead of 9 u). NOT built: the program's three-sphere wall shape, `FUN_1402559f0` (arena rim, `DAT_1403e0304 % 10 == 1`), asymmetric push for a fighter whose `+0x34` is 0 (none in versus), our tools (`collcompare`, `tracecheck`, `walkprobe`) still use 4.5 and their documented numbers are with it.
- residue (2026-10-03, lane/residue; runs `.pdtools/compare_residue/q5` (88 skills: 16 targets + 44 melee + 40 seeded controls from `compare_full_1003`), baseline `compare_full_1003`): READ `FUN_1402513a0` / `FUN_14025d380` / `FUN_14024d440`: every object test, the melee dash (`FUN_14025be40`), the press aim (`FUN_140258180`) and the knock angle (`FUN_14024fd80`) read the fighter's collider entity `+0x60 / +0x68`, which is the world x / z of its **BODY1 bone** (bone table hashes at `0x1402dcdd8`), not its position. MEASURED offset in the fighter's frame (idle pose): pc01a (-2.76 fwd, -1.74 right) 265 takes 0.58 deg RMS; pc00a (-1.96, -0.50) 43 takes 0.10 deg RMS; other characters not measured (0). Built as `Body::lock` / `Player::target_point` (not for fighters in a reaction): dash bearing / distance, aim capture, melee + shot + area hit tests, knock direction. Dash tail disassembled (`step - (dist - 11)` when dist - 11 < step, dist to that point): our dash lands on the take within 0.04-0.5 u from the press to the hit (Swift Punch 189, True Muramasa 317, Swift Kick up to the hit). Melee blows are armed one program tick after the fire tick (`FUN_1402638d0`; LIVE fire 59 -> health change 60 in every take); Psycho Kinesis / Power cast with no object in reach (LIVE 018 / 017: state 10 for the clip, fire at +10, 2 Aura). Film tool: `PD_FILM_P1LOCK` from the take's own `p1_lock` (an unlocked P1 keeps its facing; ours turned it to the foe, which put the target point in the wrong place), film log gets `p0_yaw / p1_yaw`. Before / after on the 88: non-sound mismatches 35 -> 28, 4 skills fixed (Power, Psycho Kinesis, Empty Hand, Level Baton), 1 worse (Ricochet Laser 77: the harness now leaves P1 unlocked like the take, and ours hits where the original never does; with the old locked P1 it is clean, so it is exposed, not caused: UNCONFIRMED laser geometry), 83 unchanged, 39 of 40 controls stay clean. NOT fixed, UNCONFIRMED: Swift Kick 182 / Leg Throw 183 (+3.5) and Swift Punch 189 (+10.5): the sim follows the take to the hit; after it the take's P1 is shoved 3.6 u by P0's last dash step (P1 at 11.19 u, just over the 11 u stop, ours 10.8) and P0 then walks off an edge (state 5), ours stays: a 0.4 u / half-tick difference at a threshold; the knock direction rule is the READ one (median 2.8 deg over 21 knock-downs, 16 deg with the position) but the victim's reaction-pose offset is not known; Ice Sword / Swift Blow / Ki Palm gaps now match; Dhampir Fang 242 and True Muramasa 317 (ours hit, the take never: blade / sphere margin at the Q distance, geometry beyond what is read); Tornado 311, Thor's Hammer 261, Blazing Storm 146, Triple Lasers 70, Stone Shot 7, Lucifer's Arrow 265, Spore Schism 260 (+2.5) unchanged. BODY1 through the current clip (reaction poses, other characters) is not built. Goldens: weapons, traps, tricks, effects2 re-recorded (melee arming + Psycho Kinesis casting; effects, effects3, capsules and the movement-only golden unmoved; the target offset is not in them, default body); tests 1013 game / 95 formats / 9 convert / 42 viewer.
- soundfill2 (2026-10-03, lane/soundfill2; merged with main incl. bodyradius): sound classes of `compare_full_1003` grouped by sound id and trigger (P0-SOUND.md section 12). Tool: never-fired takes, capsule_touched (86) and unseen-hand erase-on-hit (91) set aside (244 -> 252 clean on the old logs). Game: blast victims over the blast's life (no s21 / 6 / 7), Add-effect sounds only when the effect took hold (READ FUN_140272bc0: Purify / Return to Nature / Vicious Balance / Angel's Breath / Meltdown), knock-down landing probe radius by fall class around the body centre, shot / burst surface-hit sounds (UNCONFIRMED rule, A/B-kept). Run `.pdtools/compare_soundfill2` (147 skills): clean 65 -> 77, missing-sound skills 50 -> 42, extra 45 -> 39, two controls gained an extra 0.9 (173, 174). NOT built: decal gating, which face of a mixed neighbourhood wins, flight / cast-time contacts, 127 / 128, bind reaction sound 100.
- stairs (2026-10-03, lane/stairs): Laurence's QoL request, "the staircase on the main arena has a bad hitbox underneath ... thick hitbox that obscures the player when nothing visually should be" (a deliberate departure from the original; default ON, `PD_STAIRS_ORIGINAL=1` restores it). Main arena = Lost Palace (st03 / st13 / st23). Cause (MEASURED, COLLISION.md "The invisible block under the Palace flights"): the 0x94 mesh holds three class-0x12 walls per flight filling the wedge between the slab's soffit and the floor (open-side wall + end wall at z -204.6), with no render triangle behind them; a body stops at them, shots stop on them and the camera's wall pull-in is pinned 11 u from the head (boom 11.2 of 61.6), so the player is hidden. Fix: `pd_game/src/stairs_under.rs`, a load-time filter on the stage `.pdcl` (no cache regeneration): the three walls per flight go and only the headroom pocket (soffit under 22 u = `BODY_HEIGHT` + 2) keeps a side wall and a cross wall; soffit, ramp, rail hulls and the outer wall untouched. Also `pd_formats::collision::RawCollision` (parse / edit / bucket). Tests `stairs_under::tests` (walk under, pocket stays closed, still walks up in both gaits, camera ray, st03 / st13 / st23 only, once only). Pictures `engine/shots/stairs/stairs_before_after_sheet.png`, `overlay_side_x-100.png`, `overlay_plan_y58.png`. UNCONFIRMED / for Laurence's eyes: the 22 u headroom is our choice (a body is 18-20 u tall); the st04 / st14 ramps have the same kind of invisible class-0x12 walls (plus closed sides) and were NOT changed; the render's outer side wall is taken as the real wall it looks like (the render has a wall plane at x -112.1 / +112.x). The film comparisons run on st06, so they are not affected.
- stairs, part 2 (2026-10-03): Panorama (st04 / st14) ramps checked for the Palace-style block at Laurence's request: nothing to open (the wedge is already only the headroom pocket, its sides are real render walls); COLLISION.md "Panorama ramps", test `panorama_ramp_wedge_is_already_just_the_headroom_pocket`. UNCONFIRMED: whether Laurence meant a different Panorama structure (the larger stairwells have class-8 walls matching the render).

## fxfix lane (2026-10-03): Rapid Cannon black smoke fixed; Psycho Knife "square" not pinned down

- **Rapid Cannon (fixed, READ):** MSPARTICLE emitters with word 26 = 1 are an alpha blend (0x5401, `FUN_14010e260`), but were drawn opaque with the colour fading to black, so Rapid Cannon's cast smoke (`0x337f7f7f -> 0`, 5 -> 20 u) became a black cloud over the caster. Now the puff's opacity follows the colour's alpha byte (`ms_particle.rs` `ramp_alpha`, blend SrcAlpha / OneMinusSrcAlpha; `PD_MSP_OPAQUE=1` = old). 91 skills (49 attacks) use word 26 = 1. FORMATS-EFFECTS.md "MSPARTICLE blend 1"; BUG-GRAVEYARD entry.
- **PARTICLE01/02 points (READ `Part1V_vs`):** one screen-facing quad per particle instead of three crossed planes (`PD_POINT_BILLBOARD=0` = old). Little visible change on Psycho Knife.
- **Dev switch:** `PD_FX_OFF=blade,flare,points,ms,sprite,trail,piece,haze` leaves those kinds out (bisecting a look).
- **Psycho Knife "weird square": cause UNCONFIRMED.** Bisected with `PD_FX_OFF` / `PD_FX_HIDE`: the hard-edged pink rounded rectangle at the start of the swing (+0.4 s) is the BLADE01 sword ribbon (`m0_d0` blade, texture t4); the big white fog is PARTICLE01 sparkles trailing the dashing hand; the pink floor wash is LIGHT02. READ: the ribbon's draw sets 0x5401 and its pixel shader outputs `(colour + refracted scene) x white vertex colour`, which equals the additive draw we already use, so it is not a blend bug (a plain alpha blend was tried, looked like a dark purple paddle, and was reverted). The scene refraction by the second texture and the ribbon sample spacing at the first frames are the unread parts. Nothing was changed for it.
- **Tests:** `ms_particle::tests::blend_one_puffs_fade_by_the_colours_alpha`, `blend_words` (blend 1 = SrcAlpha / OneMinusSrcAlpha).
- **Pictures** (`engine/shots/fxfix/`, local): `final/rapid_cannon_before_after.png`, `final/psycho_knife_before_after.png` (old additive vs now: identical blade), `final/sweep_before_after_00..13.png` (original / ours before / ours after for 40 skills), `sweep/ours_attack_00..24.png` (all 156 attack skills, ours only), originals `orig/`, `orig2/`.
- **UNCONFIRMED:** how the original turns the colour alpha of blend-1 puffs into transparency; point-sprite size scale (`POINT_SIZE`); blade scene refraction. Merge note: `pd_game` version in Cargo.toml was bumped locally for a unique build hash and NOT committed.

## 2026-10-03 lane `rumble`: controller vibration (Laurence: "the original vibrated when taking damage etc.")

- **Read from the program** (docs/RUMBLE.md has the full table): the original uses WinRT `Gamepad.Vibration` (left / right motor, triggers always 0) through `FUN_1400de3f0`, fed by a per-pad timeline system (`pdvkFBManager` / `pdvkComFeedBack`, `FUN_14022c780` per 30 Hz tick, `FUN_14022c480` flush). Cast rows (action-table w5..w25, 385 rows), the fire block (w26..w37, 3 rows), every hit-reaction row (gsdata 0x2B00C: left motor 30-50 % for 20 ticks) and the knock-down landing rows 34-36 all carry rumble. CPU seats never rumble; the per-pad option nibble `DAT_14052dfb2` turns it off.
- **Built:** `pd_game/src/rumble.rs` (timeline model + driver, presentation only, reads `cast_seq`, `hit_seq`, `reaction_row`, `land_class`, `SkillFired`, `KnockLanded`), `Settings::rumble` + a VIBRATION ON/OFF row (`menu.rs`, row before Quit; `ROW_PITCH` 46 -> 42 so 11 rows fit), `engine/vendor/bevy_gilrs` + `[patch.crates-io]` (upstream 0.19.1 sends weak-motor requests to the strong motor) plus a startup log line `gamepad found: ... (force feedback: yes|NO)`.
- **Tests:** 14 in `rumble::tests` (timeline arithmetic, row choice, mocked two-seat driver: only the affected seat's pad, CPU seat none, pause stops, leaving the match stops, setting off sends nothing, buzz ends by itself, real-cache row check); menu / pause tests updated (`ROWS` 11).
- **UNCONFIRMED / not implemented:** stage-object screen-shake rumble (`FUN_140220f10`), the capsule / environment effect (`FUN_14021c540`), the spatial emitter (`FUN_14022bf80`, effect event 0x40/0x41), reaction rows 37-43 call sites (event mapping), the default of the original's vibration option, exact real-time feel of the 0.18 s request hold. The menu row spacing change and the row were not screenshotted. Needs Laurence's pad.

- split3 (2026-10-03, lane/split3; pictures `engine/shots/split3/final_3p_1080_t0300.png`, `final_3p_4k_t0300.png`, `final_4p_1080_t0300.png`, `final_2p_1080_t0300.png`; Laurence: "3 player split screen must give player 3 the entire bottom half", "lag / frame drops in 3 player"). **Layout (done):** `Kind::Trio` = P1 top-left, P2 top-right, P3 the whole bottom half (`views.rs`); the map + scoreboard quarter is deleted (it was a UI overlay on the HUD camera, no overview camera existed); 1P / 2P rects are the old numbers (`two_player_layouts_equal_the_old_numbers`), 4P is 2x2. Per-view `rect` / `aspect_of`; each player's HUD is fitted to its own view (`hud_layout::hud_view`, `HudScale::of`), lock-on markers size by their own view's height; P3 is quadrant 2 for the shaders. Tests: 7 new in `views.rs`, 3 in `hud_layout.rs`, 1 in `shadows.rs`. Not touched: `apply_fov` / `fov_for` (camfov): until they use each camera's own aspect, P3 shows the quarter's vertical FOV, so twice the horizontal coverage (visible in the picture); `is_strip()` camera drop is per layout, P3 (a 32:9 strip) does not get it. **Frame drops (measured, NOT fixed):** tool `perf_probe.rs` (`PD_FRAME_STATS=<frames>,<skip>` now also prints p99 / max / window, a CPU split of the main world and render thread by Bevy render phase and material churn; `PD_GPU_TIMING=1` adds GPU pass times). RTX 5060 Laptop, Vulkan, window 3840x2160 and 1920x1080 physical (`PD_WINDOW` is LOGICAL: x2 here), `PD_TEST_ASPECT=auto`, other lanes compiling at the same time (noise +-30%; runs at ~16.7 ms are the 60 Hz present cap, so idle runs only show "under 60 fps"). (1) The sim is not the cost: 0.14 ms a tick. (2) The GPU is not the cost: pass sums 1.4 / 2.0 / 2.9 / 5.0 ms for 1 / 2 / 3 / 4 players, the game's 3D engine ~8 % busy during 4P. (3) The render thread's CPU is: idle fighters, 1080p, ms per frame: 1P 11.6, 2P 20.4, 3P 22.7, 4P 23.8 (4K: 9.2 / 8.6 / 11.0 / 19.0); most of it is `render(encode)` (6.2 -> 13.5) and bind groups / phase buffers; shadows off takes 4P from 23.8 to 17.0 (Bevy builds every cascade of both shadow lights per camera: 7 depth passes a view, 21-28 at 3-4 views). (4) THE SPIKES are material churn: with skills firing, 4P shows `prepare-assets` 16.7 ms (3P idle 0.5): 106 effect materials and 106 fighter dissolve materials rewritten per frame, 178 live effect pieces (`effects::animate_pieces` writes each piece's material every frame, `cam_fade` the fighters'), each re-prepared by the render thread; this is what makes p99 57-140 ms. Before / after (`final_table.txt`, same machine, 1 rep idle, 2 reps demo, mean / p99 ms): idle 3P 4K 16.7 / 19.9 -> 16.8 / 25.0, 4P 4K 16.9 / 22.3 -> 16.8 / 21.9, 3P 1080p 16.7 / 20.0 -> 16.7 / 19.5, 4P 1080p 17.2 / 20.3 -> 19.9 / 30.8; demo 3P 4K 22.8 / 57.3 and 25.1 / 76.0 -> 22.0 / 58.2 and 23.1 / 57.6, demo 4P 4K 30.4 / 66.8 and 42.7 / 133.8 -> 52.0 / 140.0 and 44.4 / 138.0. i.e. NO measurable change: removing the spare quarter and trimming nothing did not move frame time beyond the noise. `PD_SHADOW_CASCADES_TRIM=1` (2 + 2 cascades from 3 players) is built but OFF (its look changes and the wall-clock gain was not visible). **UNCONFIRMED:** whether the spare quarter's per-frame UI writes mattered on Laurence's 21:9 setup (his settings: 21:9 area, FOV 110, top/bottom); whether the cap-limited idle runs hide a gain. **Next step (not done):** make `animate_pieces` / `cam_fade` write a material only when a value changes (the `char_light` pattern) and skip hidden pieces; share or trim shadow cascades per view.

## infomusic lane (2026-10-03, branch lane/infomusic): skill-card range, music, character-select duplicates

Three Laurence play-test items, one commit each. Full write-ups: (a) HUD.md "Skill card", (b) PIPELINE.md "Music (lane infomusic)", (c) BUG-GRAVEYARD.md "Character select: preview models doubled up".
- **a) Skill info range.** The D-pad panel is now the original's card (`hud_card.rs`): window, tinted title bar, name, school icon, COST / STR-or-DEF / uses / range icon, wrapped note. The range icon (short, med, long, self, all, mine, capsule, auto, env) is chosen by the program's own rule (`FUN_140167970`, `FUN_140167a10`, `0x140169b05`), ported and tested against the real table. An empty button shows nothing (as the original). Tests `hud_card::tests::*` (8 incl. the real table). Pictures: `engine/shots/infomusic/cards_sheet.png` (12 cards: attack short/med/long/mine, defence, [Course] with no DEF, environment, erase capsule, status/special self, X cost and X strength), `card_A0.png` (in a match). No original picture of the card exists, so no side by side. UNCONFIRMED: card position, the infinity glyph, digit spacing, note font width (long notes can run a 4th line past the window), the Aura Particle's card.
- **b) Music.** READ: one track per match, picked at the start from a stage table in the quest scripts (Lane / Sein random of three variants), menus 31 / 34 / 29, results 37 / 38, intro once then the STX loop region, no mid-battle or low-health switch. Fixed: 10 wrong stage tracks, loop region, menu and result music. Tests `music::tests::*`, `stx::tests::*`. Needs cache regeneration by the orchestrator (below). UNCONFIRMED: see PIPELINE.md.
- **c) Character select.** Cause: preview models had no `DespawnOnExit` and the generation counter restarted each visit, so a second visit stacked a new model on the old one. Fixed and pinned by `charselect::lifecycle_tests` (5), reproduced failing first. Picture: not produced (a model preview needs the real glTF load; the tests run the real systems headless).
- **Cache commands for the orchestrator** (do not write into the shared cache from the lane): `pd_convert musictracks "<game_files>\Assets\Data\BGM" "<cache>\music"` (writes `<name>.pdm`, about 1.2 GB, 38 tracks; it never touches the `stNN.wav` files), and optionally `pd_convert stages "<Data>\Map" "<cache>"` to rewrite `stages.json` and the fallback `music/stNN.wav` with the corrected tracks.
- Shared edits: `main.rs` (`mod hud_card`, `mod music`, the one `spawn_scene` music line, `music::build`), `pd_formats::stx`, `pd_convert` (new subcommand + the stage track list).
- online (2026-10-03, lane/online): Brett's `online-mp` merged (`--no-ff`, 17 commits kept) plus Laurence's menu structure. (1) Conflicts, all by hand: `main.rs` (legacy-switch list, `enter_match`, `follow_cameras`, `layout_viewports` keep main's `GameArea` code plus `LocalViews`; `apply_fov` is main's camfov version), `hud.rs` / `hud_layout.rs` (main's scales; Brett's `full_scales` only when `LocalViews` is restricted, both tests kept), `frontend/title.rs` / `mod.rs`. Brett's full-window 74.7 degree FOV is redundant with camfov and was dropped. (2) The Steam DLL + `steam_appid.txt` moved to tracked `engine/vendor/steam/`; `build.rs` copies them next to the built exe; `.gitignore` keeps `engine/play/` ignored. **Orchestrator: both files must sit next to `engine\play\pd_game.exe` on refresh.** (3) Menus: MODES row and `AppState::Modes` removed, ONLINE row added; GAMEMODES row (first row of the local setup's rules page: STANDARD, CLOSE QUARTERS, every later `MODES` entry; locks as `set_mode`); online lobby GAMEMODE row (host picks, joiner read-only; ARSENAL locked to FIXED HAND in a mode). `modes.rs` stays the single table. (4) Online determinism: `LobbyRules` carries the mode id + `Rules` fingerprint (PROTOCOL 2); joiner and host both refuse to start on a mismatch (no match, error page); the mode table is folded into the handshake data hash; the `--online-*` hook mixes `--mode` into the Hello. `Combat::hash_state` already includes `rules`, `fighters` and `arsenals`, so a slipped difference is also an "Out of sync" within 1 s. (5) Local play: only human seats get views (`LocalViews::for_kinds`): 1 human + CPUs = one full-window view. (6) Tests: 1113 game (+ `frontend::modes`, `online`, `setup` gamemodes row, `seats::local_views_tests`) / 95 formats / 6 convert / 43 viewer, all green, incl. `net::lan_tests`. (7) Live: two copies over 127.0.0.1:7777 with `--online-host/--online-join --demo --near --mode close_quarters` played a Close Quarters match (hand 3/29/240/75, Level 10), ~900 frames, no "Out of sync"; host `--mode close_quarters` vs joiner without `--mode` both refused at the handshake. Pictures `engine/shots/online/`. UNCONFIRMED / not done: two real PCs, Steam (not touched), `engine/Cargo.lock` on main carries a stray `0.1.0-rumble` version for pd_game (from another lane; not changed here).


## prophunt lane (2026-10-07, cloud container, no game data): Prop Hunt gamemode (Laurence's design)

Full write-up: `DESIGN-PROPHUNT.md` (brief, rules, controls, code map, every choice made, PC checklist). Choices made beyond the brief: DECISIONS.md 2026-10-07 rows.

- **What it is:** local 2-4 people, rounds, the seeker rotates in seat order. Seeker: Psycho Knife on X, 10 attempts, each costs 2 Health (20 total), Aura refilled every tick, 12 s blind and frozen countdown, then pad rumble, full-window view, hiders' views gone. Hiders: X becomes the nearest believable stage object in reach, B toggles freeze, no skills. A knife hit catches (fighter again, knocked down for the round, seeker +1 kill). Round ends on time (hunt clock starts at the release; 2:00 default), all caught, or seeker out of knives / Health. Match ends at 5 kills or the round limit (default 5); most kills wins, a tie is a draw. Between rounds a 5 s card; then the result screen by kills.
- **Sim (deterministic):** new `prophunt.rs` (`PropHunt` in `sim::Match::prophunt`, `filter_inputs` before each tick, `after_step` after); `props.rs` `Prop::disguisable` / `nearest_disguise` (`DISGUISE_REACH` 30, `DISGUISE_MAX_SIZE` 60, UNCONFIRMED); `combat.rs` only `rounds_by_mode` (default off, not in `hash_state`) + `restart_round`; `sim.rs` the two hooks, `start_prop_hunt`, checksum hashes the prop hunt state only when present. **Normal versus unchanged:** the 1v1 replay hashes (which fail on Linux against the Windows goldens anyway) give bit-identical values before and after this lane on this machine.
- **Front end:** `frontend/modes.rs` (`Kind::{Versus, PropHunt}`, `online` flag, `step_online`, `online_choices`, `is_prop_hunt`; Prop Hunt is entry 1, after Close Quarters; `resolve_lobby` refuses it; the table hash now includes kind and online). `frontend/online.rs` GAMEMODE row uses `step_online` (and its test loops over `online_choices()`). `frontend/setup.rs` ROUNDS / ROUND TIME rows, fixed rows, no CPUs (Y refused, a CPU blocks Start with the reason), 2+ people. `frontend/state.rs` `MatchSetup::prop_hunt`, `start_mode`. `frontend/result.rs` `prop_hunt_results`, "KILLS n". `main.rs` `enter_match` / `check_match_end` / `sync_visuals` (disguised fighter hidden) / `--mode prop_hunt [--ph-rounds N] [--ph-time secs]`.
- **Presentation:** new `prophunt_view.rs` (`drive_views` swaps `LocalViews` to the seeker while hunting, `restore_views` on leaving the match; overlay on the HUD camera: seeker cover with countdown, hider lines, seeker status line, between-rounds card; `sync_disguises` copies the object's placed meshes via `PropView::nodes_of` and moves them with the hider). `rumble.rs` `Timeline::buzz` + release buzz (both motors 80 %, 0.6 s, UNCONFIRMED feel). `hud.rs` timer shows the prop hunt clock.
- **Tests:** +28, all green: `prophunt::tests` (15: rotation, starting hands, countdown blocks the seeker but not hiders, infinite Aura + 10 knives + 2 Health each + 11th refused, X pick / switch / held-over-round-start, B freeze and unfreeze, catch -> kill + knock-down + stays down, hiders never knocked out otherwise, each round-end condition, first to 5, round limit + draw, lock-on off, determinism + checksum), `props::tests` (2), `frontend::modes::tests` (1), `frontend::setup::tests` (2), `prophunt_view::tests` (6: views, the real `drive_views`, cover, released line, card, disguise copy follows and turns and goes when caught), `rumble::tests` (1: only the seeker's pad, stops by itself), `main.rs prop_hunt_view_tests` (1: the real `layout_viewports`, seeker full window, hider cameras off, split back between rounds). Two existing tests updated for the new table entry (`setup::tests::the_gamemodes_row_cycles...`: back to STANDARD is now left from Close Quarters; `online::tests::standard_goes_back...`: loops `online_choices()`). Totals on Linux: 1162 passed, 4 failed (the known Linux-only 3 `combat::tests::replay` goldens + `sfx` footstep-in-cache), 2 ignored.
- **NOT verified (no game data, no GPU in the container):** nothing has been seen on screen. Needs a PC screenshot check: the seeker cover and text, hider lines, the card, the full-window switch at frame ~720 (`--mode prop_hunt`), the disguise copy (model, position, orientation, lighting; whether a stage object's meshes sit under its `propNNN_*` nodes as expected), that a disguised fighter's body / shadow / aura are all hidden, Psycho Knife cast without lock-on (reach, direction) and that its hit lands as `CombatEvent::Hit` with skill 3, the caught hider's knock-down with the real reaction table, the release rumble on a real pad, overlay text size on the 4K TV (Bevy default font, not the menu pixel font). All tests that need the cache's real skills were not run here (none of the new ones need it).
- **UNCONFIRMED values:** disguise reach / size limits, ROUNDS and ROUND TIME option lists, card length, rumble strength/length, seeker Level 10. Laurence to confirm: hunt clock from the release, tie = draw.

## perf lane (2026-10-03, branch lane/perf, Laurence's PC on battery; merged 2026-10-07; entry added 2026-10-08 from PERF.md)

Full write-up: `PERF.md` (round 1).
- **Fighters:** coverage, grain and the character light moved out of every fighter piece's material into one shared storage buffer (`FighterBlock`, `cam_fade.rs` / `char_light.rs` / `dissolve.wgsl`), written only when it changes. Fighter material rewrites per frame ~100 -> 0.
- **Effect pieces:** `animate_pieces` writes a piece's material only when a value differs (`PieceWrite`); blades, light decals, trails, ms_particle, aura_wisp and tick_particles compare before writing. `FxGamma`'s seven uniform buffers merged into one (`FxUniform`, binding 100).
- **Measured (noisy, battery, median of 3, mean ms):** 4P 1080p 60.0 -> 48.6 (p99 148 -> 95, prepare-assets 16.9 -> 9.2); 3P 1080p 33.5 -> 29.4; 4P 4K 109 -> 102 (prepare-assets 25.2 -> 6.6). Fixed-tick pictures byte-identical; effect frames within run-to-run noise.
- **Open then:** shared effect materials + per-instance buffer, re-measure on mains power, shadow cost.

## fxperf lane (2026-10-08, cloud container, no GPU / game data): casting frame drops, round 2

Laurence: "the effects are great but they cause frame drops when casting moves"; "implement any other performance enhancements that we haven't yet". Full write-up: `PERF.md` "Round 2"; BUG-GRAVEYARD "Frame drops when casting" second fix line.
- **Material cache / pool** (`effects/mat_cache.rs`, `PD_FX_MATCACHE=0` = old): never-written effect materials (flares, MSPARTICLE ribbons) shared per exact value; animated ones (pieces, trails, blades, lights, points, puffs, MSPARTICLE quads) reused from a pool once their last user is gone, reset to the fresh value. Aura wisp bodies unchanged.
- **Mesh reuse** (`effects/mesh_reuse.rs`, `PD_FX_MESHREUSE=0` = old): blade / flare / ribbon / wisp meshes refilled in place, untouched when their inputs did not change.
- **Pipeline warm-up** (`effects/warmup.rs`, `PD_FX_WARMUP=0` = off): at match start, one zero-area (invisible by construction) draw per material variant x vertex layout the arsenals' effects use, so Bevy compiles those pipelines before the first cast. Material recipes factored out of the spawn sites so the warm-up uses the same ones.
- **Small waste:** effect env switches read once (`effects::env_once`); emitter / blade / flare specs shared instead of cloned each frame. `PD_FRAME_STATS` prints `[fx-churn]` (materials / meshes created, freed, reused per effect started).
- **Tests:** 1185 pass (+16), the 4 known Linux-only failures unchanged; `pd_viewer` checks clean.
- **UNCONFIRMED:** every speed gain (nothing measured, nothing seen on screen); PC recipe and picture check in PERF.md "Round 2". Not done: per-instance effect material buffer (needs a GPU to verify the shader change).

## settingstabs lane (2026-10-08, cloud container, no GPU / game data / audio device): Options tabs, sound, controls, button mapping

Laurence: "Put the current video settings under 'Video Settings' tab, then add a 'Sound' and 'Controls' settings tab too", then "add button remapping to the controls tab too". Full write-up: FRONTEND.md "Options tabs"; DECISIONS.md four rows of 2026-10-08.
- **Tabs** (`menu.rs`): VIDEO SETTINGS (the old main page minus vibration, unchanged), SOUND, CONTROLS; LB / RB or Q / E, wrapping, a tab strip under the header; quit to title last on every tab in a match. Same overlay from the title and the pause menu.
- **SOUND** (`mixer.rs`, new; `music.rs`, `sfx.rs`, `frontend/sound.rs` tag their sounds): master / music / effects 0-100, default 100 = gain exactly 1.0; live on playing sounds and new ones.
- **CONTROLS:** vibration (moved), invert camera Y, camera sensitivity 50-200 % (`Settings::shape_look` in `gather_input`, before quantising; the turn is held to the sim's top speed, the pitch is render-only), BUTTON MAPPING page (`bindings.rs`, new: eight actions, pad + keyboard, swap, reset, listen with cancel / timeout; read in `gather_input`; menus unaffected).
- **settings.json:** new keys `vol_master`, `vol_music`, `vol_effects`, `invert_cam_y`, `cam_sens_pct`, `bindings`; old files load the defaults (= the old behaviour).
- **Files:** `pd_game/src/menu.rs`, `mixer.rs` (new), `bindings.rs` (new), `main.rs` (`gather_input`, module list, `PD_TEST_OPTIONS_TAB`, tests), `music.rs`, `sfx.rs`, `frontend/sound.rs`; docs FRONTEND.md, GAME.md, MENUS.md, RUMBLE.md, DECISIONS.md, this entry. The sim, `frontend/setup.rs`, prop hunt and effects code untouched.
- **Tests:** 1206 pass (+21 over the 1185 before), the 4 known Linux-only failures unchanged (3 `combat::tests::replay` goldens, `sfx` footstep wav). New: `menu::tests` 5 (and the row-list test rewritten for the tabs), `mixer::tests` 3, `bindings::tests` 3, `pause_menu_input_tests` 10 (tab switching both ways and wrap incl. Q / E and player 2's pad, quit on every tab, vibration moved, sound sliders, byte-identical `Input` for every pad button and key with the defaults, sensitivity / invert through the real `gather_input`, listen / bind / swap / refuse / cancel / timeout, reset / back, a remapped jump and skills drive `Input` while menus keep A / B / Start, menu script). Every old overlay test runs unchanged on the Video tab.
- **UNCONFIRMED: not seen on screen and not heard.** Needs a PC screenshot of each tab (`PD_TEST_OPTIONS_TAB=video|sound|controls|buttons` with `--screenshot ... --shot-frame 120`, or `--menu-script start,rb,shot:sound,rb,shot:controls,down,down,down,confirm,shot:buttons`): tab strip text width, slider bars on the new rows, the CAM SENSITIVITY label, the mapping page. By ear: the volume curve and the live change on playing music. On a pad: binding by press, the five-second timeout.
