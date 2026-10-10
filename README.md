# Phantom Dust Rebuild

An unofficial, fan-made game engine for **Phantom Dust**, written in Rust with [Bevy](https://bevyengine.org/). It plays the game's fights on your own PC, from your own copy of the game, with couch split-screen for up to four people.

> ## Read this first: you must own the game
>
> **This repository contains none of Phantom Dust.** No art, no models, no music, no sounds, no text, no game files, no converted data, no screenshots of the original's assets.
>
> To use it you need **your own legal copy of the game's data**. A converter in this repository reads that copy on your computer and builds a private cache that only you use. Do not share that cache and never commit it. [LEGAL.md](LEGAL.md) has the details.
>
> Not affiliated with, or endorsed by, Microsoft or anyone else who made or published the game.

## The best part: better local split-screen, up to four players

Phantom Dust is a game made for playing against your friends, so we put the most work into playing together on one screen. Up to **four players on one PC**, side by side, top and bottom, or three-way, with computer opponents filling any empty seat, each player with their own view and HUD, any resolution up to 4K, and rebindable gamepads.

## More game modes

Besides the standard versus match there are new modes, picked on the setup screen:

- **Close Quarters**: a close-to-mid-range brawl where everyone gets the same four skills, they never run out, and there are no capsules to hunt for. It is pure fighting, so it is quick to pick up for a mixed group. It also works online.
- **Prop Hunt**: hide and seek for two to four people on one screen. Hiders turn into objects in the arena and freeze in place; the seeker gets a short head start countdown, a full-screen view and a limited number of knife attempts to catch them. Players take turns as seeker, and the first to reach the kill target wins.

The mode list is a simple table in the code, so adding another one is easy (see [engine/docs/DESIGN-BRAWL.md](engine/docs/DESIGN-BRAWL.md)).

## Why this exists

We wanted to play Phantom Dust on a big TV with friends on the same couch. We could not find a way to do that the way we wanted, so we built it, and while doing so we worked out and wrote down how the original really behaves. That record is as much a part of this project as the engine. See [docs/WHAT-WE-FOUND.md](docs/WHAT-WE-FOUND.md).

## What you get

Working today, on the project's Windows 11 machine:

- **Local versus for 1 to 4 players**, split-screen (side by side, top and bottom, or three-way), keyboard and gamepads, with computer opponents at five skill grades driven by the original's own opponent script.
- **The original's rules, read from the program**: movement, lock-on, collision, camera, skills and their timings, Aura and capsules. Many of the 341 skills have been filmed on the original and compared with our version (see "How accurate is it?").
- **An arsenal builder**, game modes (Close Quarters, Prop Hunt), a battle HUD, music and sound effects played from your own converted files.
- **An Options screen**: pick which monitor the game opens on (it defaults to your biggest one, for example a 4K TV), window / borderless / fullscreen, resolution, aspect ratio, HUD size, field of view, camera distance, lighting, volumes and button mapping.
- **Controller vibration** rebuilt from the original's tables.
- **Online versus for two PCs** (LAN / IP, and Steam's public test App ID). It has only been tried as two copies on one PC so far.
- **Speed**: four players at a steady 60 frames per second on the project's test machine.

Not included: the original's cutscenes and story mode.

## How accurate is it?

Honestly: close, but not finished. At the last full comparison (2026-10-03) **266 of 341 skills matched** the original's recorded takes. The mismatches left are mostly missing or extra sounds and a handful of skills with small gameplay differences. Facts we could not prove are marked **UNCONFIRMED** in the docs instead of being guessed. The method is in [engine/docs/METHOD-FIDELITY.md](engine/docs/METHOD-FIDELITY.md).

## Getting started (Windows)

You will need:

1. **Your own copy of Phantom Dust**, with its data folder placed at `game_files/Assets/Data/` in this repository. (The 2017 Windows release is the one this was built against. How you get its files out of the Microsoft Store package is up to you; we do not provide tools or instructions for that.)
2. **Rust** (stable, with the `x86_64-pc-windows-gnu` target) and a portable **MinGW-w64**. Setup and the traps we hit are in [engine/docs/TOOLCHAIN.md](engine/docs/TOOLCHAIN.md).
3. **Git Bash** (for the one-shot conversion script).

Then, from the repository root:

```powershell
. .\engine\env.ps1                       # puts MinGW and cargo on PATH
cargo build --release -p pd_convert      # the converter
bash regen_cache.sh                      # converts YOUR game files into engine/cache.new (takes a while, about 1.2 GB of music)
Rename-Item cache.new cache              # use it as engine/cache
cargo build --profile play -p pd_game    # the game (the first build is slow)
.\target\play\pd_game.exe .\cache        # play
```

`PLAY.bat` in the repository root is a small launcher menu; it expects the game built and copied to `engine/play/pd_game.exe`. What each converter command does is in [engine/docs/PIPELINE.md](engine/docs/PIPELINE.md).

Default controls: player 1 uses **WASD** to move, **Space** to jump, **J K U I** for skills, **Q / E** to turn the camera. On a controller: left stick moves, **X B Y A** are skills, right trigger locks on. Everything can be remapped in Options.

## Layout

| Folder | What is in it |
|---|---|
| `engine/crates/pd_formats` | readers for the game's file formats |
| `engine/crates/pd_convert` | the converter: your game files to a local cache |
| `engine/crates/pd_viewer` | a viewer for stages, characters and animations |
| `engine/crates/pd_game` | the game: simulation, rules, menus, rendering, online |
| `engine/docs` | everything we know, per area; start at [INDEX.md](engine/docs/INDEX.md) |
| `engine/re` | Python research tools (film comparison and similar; they work with your own copy of the original) |
| `docs/WHAT-WE-FOUND.md` | a plain-English summary of the discoveries |

The simulation is **deterministic** (same inputs, same result, no clock or unseeded randomness), which is what makes replay checks and online play possible.

## Contributing

Issues and pull requests are welcome, with one hard rule: **never include anything from the game itself**, not even a screenshot of its art. Describe what you see, or send a number. Read [AGENTS.md](AGENTS.md) and [engine/docs/INDEX.md](engine/docs/INDEX.md) first. If you add a fact about the game, say how you established it (read from the program, measured on footage, or unconfirmed).

## Credits and thanks

- The community research that made a start possible, in particular the [Esper-RE](https://github.com/Torphedo/Esper-RE) file-format notes (read for reference only; no code was copied) and the Phantom Dust wiki and community skill lists.
- [iw4L](https://github.com/vladtrc/iw4L), whose "bring your own game files" approach this project follows.
- The [Bevy](https://bevyengine.org/) engine and its contributors.
- The people who made Phantom Dust. This project exists because we love it.

## Licence

Our code and documentation: MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). That does not cover Phantom Dust. See [LEGAL.md](LEGAL.md).
