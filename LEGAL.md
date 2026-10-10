# Legal notes

Short version: **this repository contains no part of Phantom Dust.** You need your own copy of the game to use it.

## What is NOT in this repository

- No game files of any kind: no models, textures, animations, stages, music, sound effects, voices, fonts, text, menus, videos, save files, or the game's executable.
- No data converted or decoded from those files (no glTF, PNG, WAV, collision files, tables). The converter writes those to a local `engine/cache/` folder **on your own computer**; that folder and `game_files/` are in `.gitignore` and must never be committed or shared.
- No screenshots or recordings of the original game's assets.
- No copy of the game's shader files, scripts or program code, and no decompiler output. Where our code reproduces a behaviour of the original (a formula, a timing, a number), we wrote it ourselves in our own words and our own engine. Some of our shaders re-implement formulas that we read out of the original's compiled shaders on our own machine; we ship none of the originals. That is the part of this project closest to the line, so if you are a rights holder and see a problem, please tell us (see below).
- No tool for removing the Microsoft Store's protection from the game, and no instructions for doing so. How you obtain the game's data files from your own copy is up to you and the law where you live.
- No Valve code or files. Online play uses the public [`steamworks`](https://crates.io/crates/steamworks) Rust crate with Steam's public test App ID 480. Valve's `steam_api64.dll` is not redistributed here; the `steamworks-sys` crate brings its own copy at build time under its own terms.
- No code from other fan research projects. Community research (for example the GPLv3 Esper-RE project) was read as reference only; the file-format code here was written independently and nothing was copied from it.

## What is in this repository

- Our own Rust source code (a Bevy-based engine, a file-format reader, a converter, an asset viewer, and a simulation of the game's rules), our own Python research helpers, and our own documentation of what we found out.
- Facts about how the game works: file layouts, timings, constants, rules. These are our own findings, written in our own words. A few code comments quote short phrases from the community wiki for context; they are credited and can be removed on request.

## How it is meant to be used

1. You legally own, or are legally entitled to play, a copy of Phantom Dust.
2. You point the converter at your own copy on your own machine.
3. It makes a private, local cache that only you use to play on your own computer.

This follows the same idea as other fan "bring your own game files" runtimes. Please do not share the cache, and please do not open issues or pull requests that contain game assets.

## Trademarks and no affiliation

*Phantom Dust* and related names, characters and artwork belong to their respective owners (including Microsoft Corporation and the original developers and publishers). This is an unofficial, non-commercial fan project. It is **not** made, endorsed, sponsored or approved by Microsoft, Xbox Game Studios, the original developers, the developers of the 2017 Windows release, Valve, or anyone else mentioned in this repository. Names are used only to say what the software works with.

## Licence of our code

Our source code and documentation are available under either of

- the MIT licence ([LICENSE-MIT](LICENSE-MIT)), or
- the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)),

at your option. These licences cover only what we wrote. They give you no rights to Phantom Dust itself.

Third-party code keeps its own licence: [`engine/vendor/bevy_gilrs`](engine/vendor/bevy_gilrs) is a patched copy of `bevy_gilrs` (MIT / Apache-2.0, licence files included), and the Rust crates in `engine/Cargo.lock` (Bevy and others) are used under their own licences. Run `cargo tree` or `cargo license` to list them.

## If you are a rights holder

If you believe anything here should not be public, please open an issue or contact the repository owner and we will look at it promptly and remove it if it should go. We would rather be asked first.

## No legal advice

This page is a good-faith description of the project, not legal advice. Laws about reverse engineering, interoperability and game files differ from country to country. You are responsible for your own use.
