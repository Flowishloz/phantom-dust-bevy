# Footage check — Laurence's recordings of the original game, 2026-09-27

Laurence recorded the installed original (Xbox/PC re-release) to check the numbers the program-reading pilot pulled out of `PDUWP.exe` (see `PROGRAM-RE.md`). This doc measures that footage and gives a verdict per claim: **confirmed** / **refuted** / **can't tell**, with the evidence.

Rules followed: footage is read-only (never moved/modified), all working files (extracted frames, audio, contact sheets) live in `%USERPROFILE%\.pdtools\footage\` outside git. Only 2–4 contact sheets were kept, copied into `engine/shots/` with prefix `ft_` (gitignored).

## Method actually used

- `ffprobe` on every clip's video stream, using `best_effort_timestamp_time` per frame (the `pkt_pts_time` field ffprobe 8.1 reports is empty for this container) to get a full per-frame timestamp list, then diffed consecutive timestamps to check for dropped/duplicated/variable-rate frames.
- Extracted the audio track (`-ac 1 -ar 22050`) and cross-correlated it (normalized, FFT-based, zero-padded so the reference can also match right at a clip's start/end) against the actual decoded cast-sound WAVs already in `engine/cache/sfx/common/{242,243,253,203}.wav`, to find the cast-sound onset time(s) in each clip. Picked the highest-scoring, well-separated peaks.
- Extracted every video frame as a PNG (native rate, no re-encoding) and built labelled contact sheets (frame number + ms since start, and ms relative to the detected cast onset) around each cast, which I looked at directly.
- Cross-checked the audio-derived cast onset against the frame where the cast visual effect first appears, wherever both were visible.

## Recording quality (applies to all 8 clips)

All eight clips (7 original + the added Psycho Knife one) are **clean, constant 30.000 fps** captures — every frame-to-frame timestamp delta is exactly 0.0333 s except a single short first-frame delta per file (an encoder/trim artefact, not a dropped/duplicated frame). No variable frame timing, no duplicated frames, anywhere. This gives **±1 frame (≈33 ms) precision** throughout — good enough to tell 1.13 s from 1.4 s (an 8-frame gap), the crux of the 24-vs-30 fps question.

| Clip | Frames | Duration | Frame timing |
|---|---|---|---|
| Blaster - Blocked.mp4 | 79 | 2.569 s | clean 30 fps |
| Blaster - Too far away & Knockdown.mp4 | 241 | 7.996 s | clean 30 fps |
| Laser - Miss.mp4 | 87 | 2.865 s | clean 30 fps |
| Slide Laser - Blocked.mp4 | 44 | 1.410 s | clean 30 fps |
| Slide Laser - Miss & Death.mp4 | 410 | 13.614 s | clean 30 fps |
| blaster 1.mp4 | 73 | 2.375 s | clean 30 fps |
| blaster 2.mp4 | 40 | 1.292 s | clean 30 fps |
| Psycho Knife - Opponent POV.mp4 | 295 | 9.797 s | clean 30 fps |

## The 24-vs-30 fps verdict (the important one)

**The numbers, first** (from the converted cache, `engine/cache/chars/pc00a.glb`, checked against `engine/crates/pd_formats/src/character.rs:17` and `FORMATS-ANIMATION.md:18`):

| Clip | Frames (raw ALR field) | Cache duration @ our 24 fps | Duration if the original plays it at 30 Hz/tick |
|---|---|---|---|
| ATK_LASER | 34 (35 keyframes) | **1.417 s** | 1.133 s |
| ATK_SHOT | 45 (46 keyframes) | **1.875 s** | 1.500 s |
| ATK_SORD | 40 (41 keyframes) | **1.667 s** | 1.333 s |

Two different things are being asked, and it's worth separating them:
- The raw ALR track format explicitly stores "length in frames" **assuming 24 fps** (`FORMATS-ANIMATION.md:18`), and our converter (`pd_formats::character`, `const FPS: f32 = 24.0`) turns that frame count into seconds using that same 24 fps constant. That's a documented fact about the **file format's own counting convention**, not a measurement of how fast the original Xbox/PC game actually *played* those frames back.
- The open question from `PROGRAM-RE.md` is about **playback rate on real hardware**: if the original engine advances one keyframe per 30 Hz logic tick (as the pilot guessed from `FUN_140257510`), the same 34-frame clip would take 1.133 s in-game, not 1.417 s — i.e. our current pipeline could be playing these clips 1.25× too slow (30/24) relative to the original.

**Lean 30 Hz / 30 fps for real playback, but not a clean, direct frame-exact measurement of the cast-pose lock duration — that part is can't-tell.**

What the footage actually nails down is the **fire tick** (cast start → projectile/beam created), which is driven by the game's 30 Hz logic clock regardless of how fast the animation *renders* — and that measurement is unambiguous and repeated three times:

1. **Slide Laser – Blocked**: the cast-sound cross-correlation gives a very confident match (score 0.76, far above the noise floor of the other candidates) at **t=0.569 s**. The green charge-swirl effect visibly starts at **frame 18 = 0.567 s** — the same frame, within measurement resolution. A thin green beam line becomes visible at **frames 32–33 = 1.033–1.067 s**, i.e. **467–500 ms after cast start** — matching the program's predicted fire tick of **13 ticks = 433 ms** to within 1–2 frames.
2. **Laser – Miss**: cast-sound onset at **t=0.552 s** (score 0.41). A visible beam-streak departs the charge orb at **frame 31 = 1.000 s**, i.e. **448 ms after cast** — again within one frame of 433 ms.
3. **Slide Laser – Miss & Death** (a later cast in the same clip): cast-sound onset at **t=6.401 s** (score 0.775, the cleanest match in the whole dataset). The swirl starts visually at frame 191–192 (1 frame early). The beam becomes visible at frames 209–211, i.e. **533–600 ms after cast** — a bit later than the other two reads, but still in the same ballpark (the beam needs a couple of frames to become bright/thick enough to see against the charge-glow, which likely explains the small "always a little late" bias across all three reads).

So: **cast sound and the cast's visual effect start on the same 30 Hz tick** (three independent clips agree to within 1 frame), and **the beam consistently leaves 15–20 frames later, matching the documented 13-tick fire time** rather than anything a 24 fps-only clock would predict (13 ticks would still be 433 ms regardless of render fps — this measurement mainly confirms the 30 Hz *logic* clock, which was already ✅ certain, but it also shows the *rendered effect* is not running noticeably slow or fast relative to that clock).

**What I could not pin down**: the actual open question — whether the ATK_LASER/ATK_SHOT/ATK_SORD *animation clips* play back at 30 fps (34 frames = 1.13 s) or 24 fps (34 frames = 1.417 s) — needs a clean read of "cast start → caster free to move again," and none of the 8 clips gave me one cleanly:
- **Slide Laser – Blocked** is only 1.41 s long and cuts off while the caster is still mid-pose (beam still on screen at the last frame) — never reaches recovery.
- **Laser – Miss**: the caster is already mid-walk-cycle (alternating footsteps visible) all the way through the cast and after — I initially read frame ~76 as "recovery," but re-checking the walk cycle shows they were moving the whole time, so "first footstep" isn't a valid recovery marker here. Can't isolate the pose-lock end.
- **Blaster – Blocked** gives the only usable number: cast pose (raised-fist stance + glow) starts at frame 33 (1.067 s) and the character's stance visibly returns to its resting/idle glow by about frame 73 (2.400 s) — a **~1.33 s** lock for ATK_SHOT. That sits much closer to the 30 Hz-tick prediction (**1.5 s**, 45 ticks) than to our cache's own 24 fps-derived duration (**1.875 s**) — it's about 5 frames short of the 30 Hz number, plausibly because the "blocked" flash makes the exact recovery frame hard to call precisely, or because the character can act again slightly before the pose fully visually resolves. It is **542 ms (16 frames) short** of the 24 fps number, which would have been unmistakable at this frame rate.

Net read: **the evidence leans toward the original game playing these clips at 30 Hz-per-tick, not the 24 fps our converter currently assumes**, on the strength of (a) the fire-tick timing matching the 30 Hz-tick prediction across three independent Laser/Slide-Laser casts (documented above), and (b) the one usable pose-duration number (Blaster ATK_SHOT, ~1.33 s measured) sitting close to the 30 Hz-tick prediction (1.5 s) and clearly short of our cache's 24 fps-derived duration (1.875 s — a pose nearly twice as long as the fire delay again, which none of the clips show). Since the raw ALR format's own "frames" field is just a frame *count*, and the 24 fps in `pd_formats::character::FPS` is our converter's own assumption about how to turn that count into seconds (not something read from the file per se — see the exact source line above), this footage is a real, if not frame-exact, data point that **our current 24 fps playback constant is probably too slow by a factor of 30/24 = 1.25×**, and worth revisiting against `PROGRAM-RE.md`'s open item on the animation clock. It is **not a frame-exact confirmation** — no clip cleanly shows a caster going from "locked in cast pose" to "free to move" without either cutting off first or already being in motion throughout.

## Program claims vs footage

| # | Claim (PROGRAM-RE.md) | Verdict | Evidence |
|---|---|---|---|
| 1 | Laser cast sound is `common/242` (not 305) | **Confirmed** | Cross-correlation against the real `242.wav` finds a clean, well-timed onset (0.552 s) in Laser - Miss.mp4, coincident with the visible charge effect. |
| 2 | Slide Laser cast sound is `common/243` | **Confirmed** | Two independent, high-confidence onsets (score 0.76 and 0.78) in Slide Laser - Blocked.mp4 and Slide Laser - Miss & Death.mp4, both landing on the exact frame the charge swirl appears. |
| 3 | Blaster cast sound is `common/253` | **Can't tell (weak)** | Best cross-correlation scores were only 0.18–0.44 across 4 Blaster clips — well below the Laser/Slide Laser confidence. The visible cast flash is unambiguous, but I can't be as sure the *audio* onset it lines up with is really the cast sound and not something else in a busier mix. |
| 4 | Laser/Slide Laser fire at tick 13 = 0.433 s after cast start | **Confirmed** | 3 independent measurements: 448 ms, 467–500 ms, 533–600 ms after cast-sound onset — all within a few frames of 433 ms (see 24-vs-30 section above). |
| 5 | Blaster fires at tick 20 = 0.667 s after cast start | **Roughly confirmed / can't tell precisely** | Blaster - Blocked: cast flash at frame 33 (1.067 s), a brighter orange flash (probable fire) at frame 49 (1.600 s) — **533 ms** delta, a bit short of 667 ms. blaster_1.mp4: raised-arm pose (matching a 0.534 s audio onset) holds through ~1.47 s before a projectile becomes distinguishable around 1.5 s — consistent with, but not a tight pin on, 667 ms. The charge glow and the projectile look too similar in these clips to mark an exact "departure" frame the way the thin Laser beam allowed. |
| 6 | Laser/Slide Laser arc: swings out and curves back, on a plane tilted ~20° (rises/dips) | **Confirmed (qualitatively)** | Laser - Miss.mp4: the beam visibly launches up and to the caster's right, then curves back over the top toward the left across frames 31–45 (1.00–1.50 s) — a rising, hooking arc consistent with a tilted swing-plane. Can't measure the exact 20° from video. |
| 7 | Which side the arc swings, and whether it bends after a strafing target | **Partly confirmed / can't tell** | In the one clean case (Laser - Miss.mp4) it swings up-and-right first, then hooks back left/over the top (clockwise as seen from behind the caster). No footage showed a clearly strafing target to test the homing re-aim claim specifically — can't tell. |
| 8 | Caster stays in the cast pose for the whole clip (~1.13 s @ 30 fps for Laser/Slide Laser) | **Can't tell** | No clip captured a clean recovery moment for either move (see 24-vs-30 section). |
| 9 | Blaster range ~300 u, vanishes after ~0.77 s if it doesn't hit | **Can't tell** | No clip shows a Blaster shot visibly disappearing in flight without a hit. "Blaster – Too far away & Knockdown.mp4" (opponent POV) shows a shot that *does* eventually connect, after a long, hard-to-time flight (see below) — it doesn't test the vanish-on-miss case. |
| 10 | Blaster homing: turns toward a sideways target within ±60° of heading, stops homing if the target passes behind it | **Can't tell** | No side-by-side footage of a target strafing vs. fleeing behind the caster while a Blaster is in flight. |
| 11 | Block reaction on a blocked Laser/Slide Laser/Blaster | **Confirmed (a flash occurs), block VFX itself unverified** | Slide Laser - Blocked.mp4 and Blaster - Blocked.mp4 both show a bright flash/impact-style effect at roughly the predicted fire time, but the camera in both clips stays on the caster, so the target-side "block" effect itself isn't directly visible on screen. |
| 12 | Knockdown on a Blaster hit | **Confirmed** | Blaster - Too far away & Knockdown.mp4: a fast blue-white comet-trailed projectile crosses a large part of the frame (frames ~213–228, 7.07–7.57 s) and impacts the target, who immediately shows a floating damage number and a knockdown/stagger reaction. |
| 13 | Psycho Knife: ~12-frame (0.4 s) stand-and-turn pause before the lunge | **Can't tell** | Filmed from the *opponent's* point of view — the attacker is off-screen during the pre-lunge stand/turn and only becomes visible once already dashing in. |
| 14 | Psycho Knife: lunge covers at most ~80 u (~4 body heights), stopping ~11 u short | **Can't tell** | No reliable on-screen scale reference in this framing to convert screen distance to world units or body heights. |
| 15 | Psycho Knife: hit lands at ticks 18–22 (0.6–0.73 s after cast) | **Consistent, not a tight confirmation** | Best-supported hit (frames 100–145): opponent stands still until ~3.30 s, a magenta dash-blur becomes visible at ~3.60 s, first damage number appears at frame 118 = **3.90 s**. If cast start is taken as ~3.30 s (when the target visibly braces / the reticle behaviour changes), the hit lands **~600 ms** later — at the low edge of the 600–730 ms window. Audio cross-correlation against `common/203.wav` never rose above score 0.13 anywhere in this clip (busy combat mix), so the cast-start time is an inference, not a direct measurement. |
| 16 | Psycho Knife: cast sound `common/203` | **Can't tell** | No confident cross-correlation match (all candidates scored 0.10–0.13, indistinguishable from noise). |
| 17 | Psycho Knife: whole move ~1.33 s (clip end at tick 40) | **Can't tell** | The clip appears to be a compilation of several separate hit instances with different HUD/camera states spliced together; the segment containing the frame-118 hit cuts to a different HUD state about 1.5 s later, which is in the right range but the cut itself (not a visible recovery) makes this unconfirmable. |
| 18 | Psycho Knife: no dash without lock-on | **Not tested** | All visible attempts in this footage are hits or near-hits with an already-approaching target; no clip shows an attempt with no target locked. |

## Per-clip notes

- **Laser - Miss.mp4**: cast at 0.552 s, beam departs ~1.00 s, arcs up-right-then-left through ~1.50 s, then no further beam is visible (consistent with a miss — the shot presumably continues straight off-frame). Caster is walking/strafing throughout, so recovery timing couldn't be isolated.
- **Slide Laser - Blocked.mp4**: cleanest single measurement in the set — cast-sound, visual-effect and fire timing all line up with the program to within 1–2 frames. Clip too short to see recovery.
- **Slide Laser - Miss & Death.mp4**: 13.6 s, at least 4–5 separate casts (onsets around 2.44 s, 4.49 s, 6.40 s, 8.24 s). The 6.40 s cast is the cleanest (score 0.78) and reproduces the same ~530–600 ms fire timing as the other two Laser/Slide Laser reads. Did not trace through to the "Death" at the end of the clip in detail.
- **Blaster - Blocked.mp4**: cast (raised-fist pose + green/yellow flash) at frame 33 (1.067 s); a brighter orange flash (probable projectile/impact) at frame 49 (1.600 s); a long-lived orange glow through frame ~72 (2.37 s) before fading to the idle look — likely the blocked-shot's impact flare lingering at (or very near) the caster rather than a separate "muzzle flash."
- **Blaster - Too far away & Knockdown.mp4** (8 s, camera on the *target*): around 6.47–6.60 s a small orange orb appears near the (distant, mostly off-screen) caster and separates into a comet-trailed projectile; it's tracked crossing a large part of the frame from ~7.07 s to impact at ~7.57 s, then the target is knocked down. Total visible flight of roughly 0.5–1.0 s depending on which frame counts as "departure" — in the right neighbourhood of the predicted 0.77 s, but the caster being off-screen for the actual cast start means this isn't a tight fire-tick measurement, and since it hits, it doesn't test the "vanishes if too far" behaviour.
- **blaster 1.mp4** / **blaster 2.mp4**: both are short, single-cast takes with the camera on the caster. blaster_1's raised-arm cast pose begins at ~0.53 s (matches its audio onset almost exactly); blaster_2 starts already mid-charge (cropped before the cast began) so its audio onset (0.21 s) is less trustworthy. Neither cleanly isolates the fire frame from the sustained charge glow.
- **Psycho Knife - Opponent POV.mp4** (new; 9.8 s, filmed from the target's side): contains several separate lunge/hit instances, apparently a compilation (HUD/camera state changes between them) rather than one continuous take. The clearest is frames 100–145 (see claim 15 above): opponent idle → magenta dash-blur appears → hit registers ~600 ms later → attacker visible up close afterward (consistent with the ~11 u stopping distance, though unmeasurable in absolute units here).

## Contact sheets kept (`engine/shots/`, gitignored)

- `ft_slide_laser_cast_and_fire.png` — Slide Laser - Blocked, frames 12–44: cast-sound-synced charge swirl at frame 18, beam visible at frames 32–33. The single best piece of evidence in this set.
- `ft_laser_beam_arc.png` — Laser - Miss, frames 31–55 (every other frame): the beam's rising, hooking arc after departure.
- `ft_blaster_flight_and_knockdown.png` — Blaster - Too far away & Knockdown, frames 195–241: the projectile separating from the caster's fist, the long comet-trail flight, and the knockdown impact.
- `ft_psycho_knife_dash_hit.png` — Psycho Knife - Opponent POV, frames 100–145: the dash-blur closing in and the first hit / damage numbers.

## What Psycho Knife footage should show next (for a cleaner read)

The new "Opponent POV" clip answers "does a hit happen on roughly the right timescale" but can't confirm most of the specific tick claims, because the camera is on the *target*, not the caster, and the clip splices together several separate instances. For a clip that can actually confirm/refute claims 13–18, it would help to have:

1. **Camera on the caster**, full body, from a couple of frames before the button press through well after the hit (or miss) — so the stand-and-turn pause, the dash, and the recovery are all in frame.
2. **A single continuous take** per attempt, not a compilation — so a "whole move ~1.33 s" measurement isn't broken by a cut.
3. **A visible scale reference** — the caster's own height (~1 body height), or floor tiles of known size, in frame throughout, so the ~80 u lunge distance and ~11 u stopping distance can be converted from pixels.
4. Ideally **one take against a stationary or slow target** (to test the "stops ~11 u short" and "hits regardless of whether the dash arrived" claims cleanly) **and one against a target with no lock-on** (to confirm "no dash without lock-on").
5. If possible, capture the on-screen skill-button press or a visible UI cue for cast start — cross-correlating `common/203.wav` in the current clip only ever scored 0.10–0.13 (indistinguishable from noise) because of the busy combat audio mix, so audio alone won't pin down cast start for this move as reliably as it did for the two laser moves.

---

# Round 2 — Jump, Lock-On, and a cleaner Psycho Knife take, 2026-09-27

Laurence recorded a second batch in `%USERPROFILE%\Pictures\Phantom Dust - Moves 2\`: `Jump.mp4`, `Lock-On Animation.mp4`, `Psycho Knife - Not locked-on And Locked-on.mp4`. Same method, same rules (working files in `%USERPROFILE%\.pdtools\footage\`, contact sheets prefixed `ft2_` in `engine/shots/`, nothing committed, no other docs touched).

**Format note:** all three clips are split-screen (2878×772 = two 1439×772 halves side by side). Per Laurence: for Jump and Lock-On, only the **left half** matters (cropped before extracting frames); for the Psycho Knife clip, **both halves matter and share one clock** — left = the knife user's own view (wide, both fighters visible), right = the target's view (close-up on the knife user closing in). All three are clean, constant 30.000 fps captures (verified the same way as Round 1: every frame delta is exactly 0.0333 s except one short first-frame artefact) — no dropped or duplicated frames.

| Clip | Frames | Duration |
|---|---|---|
| Jump.mp4 | 100 | 3.295 s |
| Lock-On Animation.mp4 | 68 | 2.215 s |
| Psycho Knife - Not locked-on And Locked-on.mp4 | 282 | 9.348 s |

## Psycho Knife: not-locked vs. locked-on

This clip actually shows both cases Laurence was asked for, back to back, on the same character:

**Attempt 1 (~0.9 s, not locked-on) — a whiff in place.** A bright magenta starburst (the knife/swing effect) fires directly on the attacker at frame 28 (0.900 s) while they are still far from the target. Checked frame-by-frame from frame 20 to frame 60: **the gap between the two fighters does not close at all** — the attacker never dashes, the swing effect plays and fades on the spot, and the target never reacts (no damage text, no stagger). This is a clean, visual **confirmation of "no lunge without lock-on."** No confident cast-sound match was found for this attempt specifically (best nearby correlation scores were 0.11–0.15, not clearly above noise), so its exact tick timing can't be pinned down, but the *lack of any dash* is unambiguous from the frames alone.

**Attempt 2 (~2.9–3.6 s, locked-on) — closes in and lands a hit.** Cross-correlating the clip's audio against `common/203.wav` gives a moderate-confidence onset (score 0.38 — clearly above the round-1 noise floor of ~0.13, though below the laser moves' best scores) at **t = 2.874 s**. From there: the attacker (already closing distance on foot through the preceding second) keeps closing, a magenta starburst (the same swing effect as attempt 1) flares at **frame 104 = 3.433 s**, and the target's view (right half) shows the impact flash with **"2 damage[Psycho Knife]"** appearing at **frame 108 = 3.567 s**. That's **693 ms from the cast-sound onset to the confirmed hit** — landing right at the top of the program's predicted 18–22 tick (600–730 ms) window. **Confirmed**, and a noticeably better-timed read than Round 1's opponent-POV clip managed (that one could only get to "roughly 600 ms, consistent but not tight").
- **Recovery**: both fighters are back to a normal standing pose with no more effects or damage text by about **frame 128 = 4.233 s** — **1.36 s after the 2.874 s cast start**, very close to the documented **~1.33 s** whole-move length. **Confirmed** (to within ~1 frame).
- **Stand-and-turn pause / dash distance / stop-short distance**: **can't tell precisely**. Between the audio cast-onset (2.874 s) and the swing (3.433 s), the attacker's on-screen pose is already mid-run toward the target the whole time (frames 88–100 all show a running stride, not a planted stand-and-turn) — so either this take's "approach" was mostly the player walking into range before triggering the skill (with the skill's own dash covering only the last short stretch, too brief to see as a distinct pose change against the already-running gait), or the stand-and-turn phase is shorter/less visually distinct than 0.4 s at this camera angle. No usable scale reference (tile size, body height) is in frame throughout to convert the closing distance into world units, so the ~80 u lunge cap and ~11 u stopping distance remain unmeasured.
- **Effect attachment**: the magenta starburst/slash both times appears centred on the attacker's weapon hand/forearm, not as a separate thrown object — consistent with a melee weapon-trail effect rather than a projectile.

Net: **this clip is a real improvement over Round 1's Psycho Knife evidence.** It directly confirms "no dash without lock-on" (unambiguous) and gives a tight hit-timing and whole-move-length match (both within about a frame of the documented numbers), upgrading claims 15 ("hit at 0.6–0.73 s") and 17 ("whole move ~1.33 s") from Round 1's "consistent, not tight" / "can't tell" to **confirmed**. The stand-and-turn pause and the absolute lunge/stop distances are still **can't tell** — this take doesn't isolate them any better than Round 1 did, for the reasons above.

## Jump: dodge type, airtime, and recovery

The character in this clip is `pc00a`, the cache's generic "player" rig — `engine/cache/chars.json` lists it as **dodge type "jump"** (the plain vertical-hop dodge used by Hero/Freia/Meister/Chunky/Know/Tsubutaki), not Tumble or Cartwheel, so this footage is testing the "Jump" archetype specifically.

Tracking the character's leg pose against the ground (the chase camera pans with them, so absolute screen height alone isn't a reliable cue — foot-ground contact is):
- **Take-off** (both feet leave the ground, legs tuck up): **~frame 72–74 (2.37–2.43 s)**.
- **Landing** (a reddish impact/dust flash plus a crouch pose): **~frame 87 (2.87 s)**.
- **Airtime**: **~430–500 ms**, matching `GAME.md`'s own jump constants (`JUMP_VELOCITY` 75 u/s → apex ≈ 9 u, **airtime ≈ 0.5 s**) well — **confirmed**, to within the frame resolution here.
- **Recovery** (crouch → fully upright, able to act normally again): by about **frame 96–98 (3.17–3.23 s)**, i.e. **~300–370 ms after landing**. That's shorter than both cache-derived predictions for `JUMP_LAND1` (13 frames: 542 ms @ 24 fps, 433 ms @ 30 fps) but noticeably closer to the 30 fps one.
- **Apex height / horizontal travel**: **can't tell in absolute units** — no scale reference (tile size, body height) was tracked through the jump to convert screen displacement into world units; qualitatively the hop is modest, not a dramatic leap, consistent with a small apex rather than a large one.
- **Total visible jump "event"** (first pose change out of the run cycle to fully back to normal standing): roughly **frame 68 to frame 98, ~1.0 s**. This is well *short* of summing the cache's own JUMP_START+JUMP_UP+JUMP_DOWN+JUMP_LAND1 frame counts at either fps (2.1 s @ 30 fps, 2.625 s @ 24 fps — see table below), which suggests the original game does not play these four clips back-to-back at full length for an ordinary short hop; it more likely blends/scales the rise-and-fall portion to match the actual physics-driven air time (a longer jump, e.g. off a ledge, would presumably show more of JUMP_UP/JUMP_DOWN). Flagging this as a real finding rather than forcing it to fit — **can't tell** whether the raw clips are ever played at full length in-game, only that this short hop doesn't do so.

## Lock-On: what happens on screen

No dedicated character animation exists for locking on — the round-1 research agent already confirmed none of `pc00a`'s 129 clips has "lock" or targeting in its name, and this footage matches that: **the character's own pose never changes** when lock-on toggles; they keep whatever they were already doing (idle sway here).

The only visible change is a **small green rotating reticle/ring icon that appears above the target's head**, pulsing in and out roughly every 500–600 ms (on for ~400–530 ms, off for ~360–370 ms) over the 2.2 s clip. This reads as the reticle's own idle spin (parts of a rotating ring going edge-on/invisible at certain angles) rather than repeated lock/unlock presses, but that's an inference, not something confirmed frame-by-frame against an input log. **Camera behaviour** (the docs mention the free camera easing back to level and following the target once locked on): **can't tell** from this clip — the framing and camera angle look effectively static across the whole recording, but no rigorous camera-position measurement was done, so a subtle pan/zoom on lock can't be ruled out. In short: **lock-on is a HUD/reticle effect layered over unchanged character motion, not a pose or camera event big enough to show up clearly in a 2.2 s clip of idle standing.**

**Correction (2026-10-01, lock-on marker lane):** the on/off "pulse" above was the clip's two lock/unlock cycles (lock at frames 2 and 34, unlock at 18 and 61). The settled marker neither spins nor pulses; it fades in over a 10-tick fly-in and holds. The marker also sits on the target's upper body (the target is hidden behind a tree in this clip), at about a ninth of the view height across. Details and the sprite table: `HUD.md` "Lock-on marker".

## More 24-vs-30 fps evidence

The Jump clip adds one more data point in the same direction as Round 1: the light landing recovery measured here (~300–370 ms) sits closer to the cache's 30 Hz-tick reading of `JUMP_LAND1` (433 ms, 13 ticks) than its 24 fps-derived duration (542 ms) — a third independent clip (after Slide Laser's fire-tick timing and Blaster's cast-pose length) leaning the same way. It does **not** raise the confidence to "frame-exact proof" — if anything, the fact that the *whole* jump event (1.0 s) undershoots even the 30 fps sum of all four clips (2.1 s) by more than half shows that simple clip-length arithmetic doesn't fully describe what's on screen, and a short hop likely doesn't play every clip at full length regardless of which fps is right. **Verdict unchanged from Round 1: leans 30 Hz/30 fps, not frame-exact proof.**

## Contact sheets kept from Round 2 (`engine/shots/`, gitignored)

- `ft2_pk_notlocked_whiff.png` — Psycho Knife clip, frames 12–44 (both halves): the not-locked-on swing happening in place, no distance closed.
- `ft2_pk_locked_hit.png` — Psycho Knife clip, frames 84–116 (both halves): the locked-on approach, the swing flash, and the "2 damage[Psycho Knife]" hit text.
- `ft2_jump_takeoff_land.png` — Jump clip, frames 62–93: take-off, apex, and the landing impact/crouch.
- `ft2_lockon_reticle.png` — Lock-On clip: the pulsing green reticle above the target's head, character pose unchanged throughout.
