# Prop Hunt (built 2026-10-07, lane prophunt)

A local hide-and-seek mode for 2-4 people on one screen, designed by Laurence. Status: **built and unit-tested;
NOT yet seen on screen** (the lane ran in a cloud container with no game data and no GPU). Everything visual below is
"should", not "verified": see "Needs a PC check".

## Laurence's brief (his words)

- "The seeker gets the move psycho knife and 10 attempts to use it (infinite mana, 20 life, each knife use costs 2 life)."
- A knife hit catches a hider: "visually, they turn back into a player and play the 'knockdown' animation."
- "Hiders will be able to click X on a prop to become it, B to freeze themselves in place."
- "The round is time based." 2 minutes by default, "can be set in the settings too".
- "The seeker has a 12 second countdown before they can move. Their controller rumbles once the countdown ends. The
  seeker's screen becomes full screen - hider's screens vanish." Hiders "can still move blind".
- "The gamemode consists of rounds where each player takes it in turns to be seeker. Round limit can be set in the
  settings - baseline 5. First to get 5 kills wins."
- "2-4 local humans."

## How a match plays

1. **Setup.** VERSUS MODE, join 2-4 people on page 1, then on page 2 set **GAMEMODES** to PROP HUNT (after STANDARD
   and CLOSE QUARTERS). Its rows: **ROUND TIME** (1:00 / 1:30 / **2:00** / 3:00 / 5:00) and **KILLS TO WIN** (1 / 2 / 3 / 4 /
   **5** / 6 / 8 / 10; no round limit, Laurence 2026-10-08). Team play (SEEKER AND HIDERS), game rule, starting level (N/A), starting skills
   (SEEKER: KNIFE) and friendly fire (N/A) are fixed, dimmed and skipped by the cursor, the way Close Quarters locks
   its rows. Stage works as normal. The seat columns' arsenal reads SEEKER KNIFE / HIDER NONE.
   - **No CPUs:** while Prop Hunt is the mode, Y does not add a CPU and empty seats are not offered for one. A CPU
     added before the mode was picked blocks Start with: "Prop Hunt is for 2-4 people, no CPUs: remove the CPU seats on
     the previous screen (B)." One person alone gets "Prop Hunt needs at least two people ...".
   - **Not online:** the online host's GAMEMODE row skips Prop Hunt, and a lobby announcing it is refused.
2. **Round start.** Round 1's seeker is player 1, round 2's player 2, and so on in seat order, wrapping. Everyone is on
   their start point. The seeker has Psycho Knife on **X** (Level 10, 20 Health); the hiders have no skills, no
   capsules and no Aura. There is no 3-2-1 START countdown of the normal match: the 12 s seeker countdown is the start.
3. **Countdown (12 s).** The seeker cannot move, turn, jump, lock on or cast, and their view is covered black with
   "YOU ARE THE SEEKER" and the seconds left. Each hider sees their own split view with a line at the top: the
   countdown, "X: BECOME AN OBJECT  B: FREEZE", and [DISGUISED] / [FROZEN] when they are.
4. **Release.** The seeker's pad buzzes (both motors at 80 % for 0.6 s), the seeker's view becomes the whole window
   and the hiders' views disappear. The hunt clock starts now (decision below). The seeker's line shows round, time,
   knives left, hiders left and kills. The HUD's own clock (dial) shows the countdown, then the hunt clock.
5. **Hunting.** Each X press with attempts left casts the knife: one attempt and 2 Health gone (ten knives = all 20
   Health). A knife hit on a hider catches them: they turn back into their fighter, play the knock-down (the sim's
   knock-out: the hit's reaction, then lying for good, with the KO voice), and the seeker scores a kill. A caught hider
   stays down and has no control for the rest of the round.
6. **Round end**, whichever comes first: the hunt clock runs out (TIME UP - THE HIDERS SURVIVED), every hider is caught
   (EVERY HIDER CAUGHT), or the seeker has no knives or no Health left (THE SEEKER IS OUT OF KNIVES; it waits for the
   last swing to finish so a last-knife hit still counts). A card covers the window for 5 s: why the round ended,
   everyone's kills, and the next seeker.
7. **Match end.** At the end of the round in which someone reaches KILLS TO WIN (default 5); there is no round limit (Laurence
   2026-10-08: "No round limit, play to 5"). The card says MATCH OVER - PLAYER n WINS, then the result screen ranks everyone by kills ("KILLS
   n" on each card).

## Hider controls

**2026-10-08 change (Laurence):** the hiders' head start is **20 s** (was 12; `SEEKER_COUNTDOWN_TICKS`), and **A, X or Y** all become an object (was X only: "Pressing A did not turn me into a prop"). A press with nothing in reach now shows "NOTHING IN REACH - GET CLOSER TO AN OBJECT" on the hider's line for 1.5 s (`Hider::nothing_in_reach`). Where the text below says 12 s or X, read 20 s and A / X / Y.

- **X (button 1): become an object.** Picks the nearest stage object within 30 units (about two metres) whose box is on
  the hider's level (the Psycho Kinesis level rule: not upstairs, not downstairs) and that is a believable disguise:
  intact, at most 60 units on its longest side (the Psycho Kinesis size limit), not a structural piece (floors,
  bridges, road plates, pillars, the collapsing container), not a scripted piece, not a thin panel (railings, stair
  sides). That excludes floors, containers and the heli. Breakable or not does not matter: a sign is a fine hiding
  place. X again near another object switches; X with nothing in reach changes nothing. The object you copy stays
  where it was (the disguise is a second copy).
- **B (button 2): freeze.** Toggles. Frozen: no movement and no jump, the facing is locked, and the body is held where
  it froze even if pushed (the height still follows the floor). The camera can still look around.
- Hiders never cast (their buttons are taken for X and B and they hold no skills). Nobody can lock on.
- On the keyboard (player 1) X is J and B is K, as for skills.

## Where the code is

| Part | File | Notes |
|---|---|---|
| Rules (deterministic sim) | `pd_game/src/prophunt.rs` | `PropHunt` in `sim::Match::prophunt`; `filter_inputs` before each tick, `after_step` after it; hashed by `Match::checksum` only when present |
| Disguise rule | `pd_game/src/props.rs` | `Prop::disguisable`, `nearest_disguise`, `DISGUISE_REACH`, `DISGUISE_MAX_SIZE` |
| Combat hook | `pd_game/src/combat.rs` | `Combat::rounds_by_mode` (default off) stops knock-outs and the time limit ending the round; `restart_round` |
| Mode table | `pd_game/src/frontend/modes.rs` | `Kind::{Versus, PropHunt}`, `online` flag, `step_online`, `is_prop_hunt`; Prop Hunt is entry 1 |
| Setup | `pd_game/src/frontend/setup.rs` | ROUNDS / ROUND TIME rows, locked rows, no CPUs, 2+ people |
| Match start / end | `frontend/state.rs` (`MatchSetup::prop_hunt`, `start_mode`), `main.rs` (`enter_match`, `check_match_end`, `--mode prop_hunt`) | |
| Presentation | `pd_game/src/prophunt_view.rs` | views switch, overlay (cover, lines, card), disguise copies |
| Fighter hidden while disguised | `main.rs::sync_visuals` | hides the whole fighter: body, shadow hulls, aura |
| Release rumble | `rumble.rs` | `Timeline::buzz`, `RELEASE_BUZZ_*` |
| HUD clock | `hud.rs` (Bind::Timer) | the prop hunt clock when present |
| Result screen | `frontend/result.rs` | `prop_hunt_results`, "KILLS n" |

**Sim impact on other modes: none.** `Match::prophunt` is `None` and `Combat::rounds_by_mode` is false everywhere
else; neither is in `Combat::hash_state`, and `Match::checksum` hashes the prop hunt state only when it exists, so
normal-versus checksums and replays are unchanged. Online is unchanged except that the GAMEMODE row and
`resolve_lobby` skip / refuse the local-only mode; the mode table's handshake hash changed (a new entry plus the `kind`
and `online` fields), which only means two builds of different tables refuse each other, as designed.

## Command line (for screenshots)

`pd_game.exe <cache> --mode prop_hunt --p1 pc00a --p2 pc01a [--ph-time <seconds>] [--ph-kills N] --screenshot x.png
--shot-frame <frame>`. The countdown is 720 ticks; a shot after about frame 760 shows the seeker full-screen. The
legacy path never goes to the result screen (it stays on the final card).

## Choices made beyond the brief (also in DECISIONS.md)

1. **The hunt clock starts at the release**, not at round start: a round is 12 s + ROUND TIME (confirmed by Laurence 2026-10-08: "Yes 2:12").
2. **Lock-on is off for everyone.** The lock target and its marker would point the seeker at a hidden player.
3. **Hiders' collision body is the normal fighter body**, whatever they look like (simplest; no per-object hull).
4. **Frozen = held in place** (x, z, facing restored every tick; height follows the floor), not just "no stick".
5. **Hiders' Health is kept full**; only the knife catches. (Empty-handed fighters slowly lose Health in a normal
   match, and a fall costs 3: neither should take a hider out.)
6. **The seeker's Health cost is not damage**: it is set directly, so the knife that spends the last 2 Health still
   swings and can still catch. A seeker at 0 Health stands until the round ends.
7. **Round end waits for the last swing** when the seeker runs out of knives.
8. **No draw** (superseded 2026-10-08): there is no round limit, and only the seeker scores, so first-to-5 has exactly
   one winner. A match where nobody ever catches anyone runs until the players quit.
9. **The between-rounds card shows for 5 s**; ROUND TIME offers 1:00 / 1:30 /
   2:00 / 3:00 / 5:00.
10. **Disguise reach 30 units, max size 60 units** (the Psycho Kinesis limit), thin panels excluded; the copy turns
    with the hider from the object's own placed orientation.
11. **The seeker's Level is 10** (refilled every tick); hiders have Level 0 and no Aura (so no Aura glow shows on a
    disguised hider).
12. **No 3-2-1 START countdown** in Prop Hunt rounds; the HUD's "START" banner still flashes at round start.
13. **Release buzz: both motors 80 % for 0.6 s.**
14. **A CPU seat blocks Start** (with the reason) rather than being removed silently; Y cannot add one in this mode.

## PC check 2026-10-08 (Windows, real cache)

Seen in screenshots (`engine/shots/w1008/`, looked at): the seeker's black cover with YOU ARE THE SEEKER and the countdown; the hider line (round, HIDE!, countdown, X / B help, [DISGUISED] and [FROZEN]); after release the seeker's view takes the whole window and the hider's view is gone; the seeker's line (round, time, knives, hiders left, kills); knives count down and cost 2 Health each; the between-rounds card (ROUND 1 OVER / TIME UP - THE HIDERS SURVIVED / kills / NEXT SEEKER); seeker rotation (round 2 = player 2, round 3 = player 1); setup rows (GAMEMODES = PROP HUNT, TEAM PLAY = SEEKER AND HIDERS, ROUND TIME 2:00, KILLS TO WIN 5, rule line, "needs at least two people" warning). **Disguise:** a hider next to a street lamp post pressed X: the fighter, shadow and aura vanished and a second lamp post stood at the hider's feet, upright and lit like the stage (`disguise_cmp.png`). Psycho Knife without lock-on swings forward (purple slash). NOT seen: a real knife catch and the knock-down (the scripted seeker could not be steered onto a hider; the sim test covers the logic), the result screen KILLS, rumble, 4K text size. Test hook added for this: `PD_TEST_TAPS2/3/4` (taps for players 2-4, like `PD_TEST_STICK2`).

## Needs a PC check (original list, see the 2026-10-08 results above)

- The whole look: the black cover and its text, the hider lines, the card, the seeker's full-window switch and the
  hiders' views going away (`--mode prop_hunt`, shots before and after frame 720).
- The disguise copy: right model, right place (base on the floor at the hider's feet), right orientation, lit like the
  stage (it reuses the stage's own mesh and material). The prop library meshes may be split across several nodes.
- That a disguised hider's fighter, shadow and aura are all gone.
- Psycho Knife cast **without lock-on** (aimed by the camera): it should swing forward; its reach decides how close
  the seeker must be. The catch rule is any Psycho Knife hit by the seeker.
- The knock-down of a caught hider (the sim's knock-out reaction; with the reaction table loaded the hit's reaction
  plays first).
- The release rumble on a real pad.
- Text sizes on a 4K TV (the overlay uses Bevy's default font, scaled with the window height, not the menu pixel font).
