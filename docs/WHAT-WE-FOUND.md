# What we found out

Phantom Dust shipped without documentation, and its makers never published how it works. This is a plain-English summary of what rebuilding it taught us. Everything here is our own finding, written in our own words; there is nothing from the game itself in this repository (see [LEGAL.md](../LEGAL.md)).

Each claim in the detailed docs carries a tag saying how we know it:

- **READ**: read out of the original program (its code and tables), with the address noted.
- **MEASURED**: measured on footage of the original.
- **LIVE**: traced while the original was running on our machine.
- **UNCONFIRMED**: our best reading, not proved. We say so rather than guess.

The detailed, per-area versions live in [`engine/docs`](../engine/docs/INDEX.md); the file names in brackets below say where.

## 1. How the data is stored

- All 831 of the game's model and stage archives (`.alr`) open with one reader: a list of chunks followed by a raw data block. [FORMATS-ALR.md]
- The skill table holds 399 records, of which 365 are skills you can collect and 34 belong to enemies, bosses and story. A few fields that look empty in the file are used by 19 records. [FORMATS-SKILLS.md, PROGRAM-RE.md]
- Every skill points into a separate action table of 574 rows. That row decides which visual effect plays, which sound plays on cast, which animation is used, and on which tick the skill actually fires. [PROGRAM-RE.md]
- The game plays its skill effects from a packed archive, not from the loose effect files lying next to it. 103 of the packed effects differ from their loose copies, and the packed ones are what you see. [PROGRAM-RE.md, FORMATS-EFFECTS.md]
- Stage collision is a whole-stage mesh with a per-surface class. That same class is what chooses the footstep puff (sand, dust, water, none). We first thought the class could not be reached from converted data; one header field away it turned out to be there. [COLLISION.md]
- The computer opponent is a small virtual machine running the game's own script, with 50 integer registers and 20 vector slots. Its main script starts again from the top on every tick, so the registers are its only memory. We rebuilt it from that script rather than writing our own AI. [FORMATS-CPU.md]

## 2. How the game really behaves

- **Time:** the game logic runs on a fixed 30 ticks a second, and each animation key advances once per tick, so animations play at 30 frames per second. Our first guess was 24, which made every animation 25% too slow. Our own simulation runs at 60 ticks a second and converts. [PROGRAM-RE.md, GAME.md]
- **Skill timing is not what you would guess.** A laser fires about 0.43 seconds after you cast it, not the 1.26 seconds we first believed (we had mistaken a sound number from a different skill for the launch). Across all skills the commonest fire times are 20 and 25 ticks. [PROGRAM-RE.md]
- **Movement:** the run speed, walk speed, how far the stick must move before the gait changes (and back), and how fast a fighter turns (a full about-face takes about nine ticks) are all fixed numbers in the program. [PROGRAM-RE.md]
- **Lock-on and melee:** lock-on works in two distance bands, and the melee dash only fires at a locked target inside the near one. Strafing uses one-shot pivot animations, not loops. [PROGRAM-RE.md]
- **Bodies:** fighters are all about the same height and have the same small hit radius; two fighters are pushed apart to an exact fixed distance, which we measured on the original. [PROGRAM-RE.md]
- **Camera:** the original's field of view is 75 degrees across for a 16:9 picture, measured live. The original does not stop the camera from clipping into walls with a ray test; the pull-in our version has is our own addition. [GAME.md]
- **Aura** refills faster the lower it is, and pauses while you cast or dodge. [PROGRAM-RE.md]

## 3. How we proved it

- **Film everything.** A recorder drove the original through all 341 skills, one take each (about two hours, restarting the game regularly so it stayed healthy), and logged what happened. 334 takes worked first time and 7 were refused for a missing set-up; none crashed and none fired the wrong skill. [MOVE-RECORDER.md]
- **Compare skill by skill.** A comparison tool plays the same take in our version and the original and reports the numeric differences and picture sheets. The first trustworthy scorecard (2026-10-03) was **244 of 341 skills matching**, rising the same day to **266 of 341**. [NEXT-SESSION.md]
- **Read before tuning.** When locked-on movement felt wrong we spent about a dozen rounds patching how it looked. One rebuild that followed the program's whole state machine removed about 700 lines of those compensations and passed. The lesson is written down as a method. [METHOD-FIDELITY.md]
- **A deterministic simulation.** Same inputs always give the same fight, with no clock and no unseeded randomness inside it. That is what makes replay checks possible and what online play stands on. [AGENTS.md, GAME.md]

## 4. What we got wrong first

- Animation speed: assumed 24 frames a second, really 30. [FORMATS-ANIMATION.md]
- Laser timing: believed 1.26 s, really about 0.43 s. [PROGRAM-RE.md]
- Strafing took several rounds: the pivot looped when it should not, the body faced the target, a hip turn was counted twice, and the hand-over snapped by up to 90 degrees. Each symptom had its own cause. [BUG-GRAVEYARD.md]
- The field-of-view slider did nothing in split screen, because an override forced a tiny vertical angle. The setting is now defined as a 16:9 horizontal angle. [BUG-GRAVEYARD.md]
- We declared the footstep surface class unreachable. It was not. [COLLISION.md]

## 5. What is still open

- Whether a hit that interrupts a cast still spends the skill and its Aura, and whether chained combo hits pay Aura again.
- How levelling up from Aura Particles works, and the real level cap.
- Exact dodge airtime and ground-dodge distance (the program and the footage disagree slightly).
- Where the school aura's halo comes from. We ruled out the obvious draw and have not found the real one.
- A number of sound classes, and a short list of skills whose behaviour differs from the original. [NEXT-SESSION.md, audits/P0-SOUND.md]

If you can settle one of these, please say how you know and open an issue.
