# Research: Phantom Dust engine lineage ("allib") and shared-engine routes

Date: 2026-09-30. Method: WebSearch + WebFetch. Marks: READ = page fetched and read; SNIPPET = search-result summary only;
403 = fetch blocked (miraheze, MobyGames, TCRF all returned 403 or a bot-check page, so their content is snippet only).

## 0. Headline correction to the brief

The brief says Phantom Dust was developed by Media.Vision. No source found says that.
- Wikipedia (READ, https://en.wikipedia.org/wiki/Phantom_Dust): developed by Microsoft Game Studios Japan. Media.Vision is not mentioned.
  Staff: Futatsugi (director/producer), Toshiharu Tange (designer), Hiroyuki Ogasawara (programmer), Takehiko Yamamoto (artist).
- Unseen64 (READ, https://www.unseen64.net/2010/05/18/phantom-dust-xbox-proto-beta/): "developed by Microsoft Game Studios".
- MobyGames credits (SNIPPET, https://www.mobygames.com/game/19055/phantom-dust/credits/xbox/): developer listed as Microsoft Game Studios Japan.
- Media.Vision's link to the Xbox is Sneakers (2002), "in partnership with Microsoft Game Studios Japan"
  (READ, https://en.wikipedia.org/wiki/Sneakers_(2002_video_game)). That is where the Media.Vision idea likely came from.
Phantom Dust is best treated as an MGS Japan in-house team (Futatsugi, ex-Sega/Smilebit) game, possibly with contractors. Unverified.

## 1. What is documented about allib itself

- Phantom Dust wiki, Modding/Executable (SNIPPET, 403 on fetch, https://phantomdust.miraheze.org/wiki/Modding/Executable):
  - allib is not called through named symbols. It was presumably dropped into the project and compiled with the game code.
  - Source paths are at c:\usr\xbox\allib. The wiki says it is unclear whether allib pre-existed or was written for Phantom Dust.
  - All known executables are stripped. This includes the 2003 alpha (12 XBEs, several debug builds, still no symbols), the 2004 retail XBE and the 2017 UWP build.
  - Project paths: headers under c:\...\pdp, cpp files under \usr\...\PDP (game project "pdp").
  - alfn_ files (alfn_msBillBeam00.cpp, alfn_msPARTICLE00.cpp, alfn_ykFADE00.cpp) sit in the same layout.
    Note that "ms" and "yk" look like programmer or author initials, not "Microsoft". This is my inference.
- Phantom Dust wiki, ALR (SNIPPET, https://phantomdust.miraheze.org/wiki/Modding/File_Formats/ALR):
  - .alr = "AL Resource", .ala = "AL Animation", .alm = "AL Mesh (dev builds only)". These are the wiki's guesses.
  - ALR is a RIFF-like chunked format, with a Resource Buffer Offset near the header.
  - The wiki mentions Windows and Linux ALR tool builds with source on GitHub. I did not find or read the repo URL.
- No search hit for "allib" outside Phantom Dust material. Searches combining allib with Media.Vision, Sneakers, Wild Arms or Shining Force returned nothing relevant (SNIPPET).
  This is absence of evidence, not proof.

## 2. Candidate-by-candidate

| Candidate | Finding | Evidence |
|---|---|---|
| Sneakers (Media.Vision, 2002) | No documented shared engine. TCRF page exists but covers unused graphics (error.dds), not code. No symbol or decomp leak found. | SNIPPET https://tcrf.net/Sneakers_(Xbox) (403) |
| Sneakers vs PD credits | Sneakers programmers: Junichi Tani, Atsushi Kurosaka, Satoshi Kodaira, Makoto Ochi, Junichi Ishiba. PD programmers: Ogasawara, Iwase, Omuro, Saito, Fukasawa, Yajima, Kakei, Tsunoda, Nitanda, Suda. No name overlap. | SNIPPET https://www.mobygames.com/game/xbox/sneakers_/credits and PD credits URL above. MobyGames 403; names are from search summaries. |
| Other Media.Vision titles | Nothing found tying Media.Vision to "allib" or PD. No Media.Vision engine talk found (CEDEC search returned only MT Framework, Vision Engine and similar). | SNIPPET searches |
| Crimson Dragon (Grounding Inc, 2013) | Reported as Unreal Engine 3, so no shared code. PD's art lead joined Grounding. | SNIPPET https://en.wikipedia.org/wiki/Crimson_Dragon and https://medium.com/super-jump/phantom-dust-revisiting-a-cult-classic-99a21caceb77 (not read) |
| Panzer Dragoon Orta (Smilebit) | Only a people link (Futatsugi, ex-Sega), no code link. I did not search Orta's engine. PD is 2003-04 and Orta is 2002. Unverified. | none |
| Blue Dragon, Lost Odyssey, Ninety-Nine Nights | Not researched. Blue Dragon and Lost Odyssey are Mistwalker/Microsoft. Nothing suggests a link. | none |
| Wild Arms, Shining Force EXA/Neo, Shining Wind (PS2) | Not researched individually. No allib mention surfaced. The PS2 and Xbox toolchains differ. | none |
| Rebellious Bullet (unreleased Xbox game, same dev kit as the PD builds) | Panzer-Dragoon-like shooter, apparently from Futatsugi's MGS Japan team. This is the strongest candidate for a sibling that uses allib. Publicly dumped, but no symbols reported. | READ https://www.resetera.com/threads/unreleased-og-xbox-game-and-early-phantom-dust-builds-found-on-xbox-dev-console.141263/ |
| Code Mystics (2017 port) | Microsoft's Adam Isgreen: "There was no final source code"; the port was done by reverse-engineering and hacking the binaries. So no source or symbols exist even inside Microsoft or Code Mystics. Code Mystics is an emulation and port shop (founder Jeff Vavasour), and their other ports do not share this engine. | READ https://www.player.one/phantom-dust-re-release-information-was-scarce-because-source-code-had-be-reassembled-595327 ; Wikipedia PD (READ) |

## 3. Dumps, leaks and symbols

- 2003 alpha plus two TGS demo builds were dumped from an auctioned Xbox dev kit (Strong Museum curator verified), public since Sept 2019.
  - ResetEra thread (READ, URL above) and the Retro Reversing Xbox page (SNIPPET, https://www.retroreversing.com/xbox).
  - Several XBEs are debug builds but stripped. The .alm files exist in them per the wiki.
- Retro Reversing "Original Xbox Games with Debug Symbols" (READ, https://www.retroreversing.com/xbox-game-debug-symbols):
  - Lists .pdb/.map games: ATV3 Lawless beta, Crusty Demons, Gauntlet Dark Legacy, Xyanide, Buffy, Miami Vice, Predator, Sphinx, Red Star, Wrath Unleashed, X-Men.
  - Neither Phantom Dust, Sneakers nor any Media.Vision game is listed.
  - None of these use allib as far as I know. They could help only as generic Xbox SDK/D3D/XDK library signatures.
- The 2002/2004 Xbox OS source leak (Engadget, SNIPPET, https://www.engadget.com/xbox-os-windows-source-code-leak-142213929.html) covers the kernel and XDK. It is irrelevant to allib.
- No ALR documentation for any other game was found. The only ALR spec is the PD wiki.
- Not found: Unseen64 or Hidden Palace dumps of Sneakers with symbols, any Sneakers XBE analysis, any decompilation project for PD or Sneakers.

## 4. Verdict

Shared-engine route to symbols or format matching: probably does not exist. Confidence about 75% for "no usable route". About 90% that no public symbols exist for any allib-based binary.
- The Media.Vision premise is unsupported, and the PD and Sneakers programmer lists do not overlap.
- allib's own path (c:\usr\xbox\allib) and the "pdp" project name look like a Microsoft Japan in-house tree.
- The only plausible sibling is Futatsugi's unreleased Rebellious Bullet, which was dumped from the same dev kit.
  It may contain the same allib code, possibly as a different build or revision (same-source cross-check, not external symbols).
- The remaining upside is the 2003 alpha. It predates retail, includes debug-configuration XBEs and leftover .alm/dev paths, so names, asserts and RTTI-like strings may be richer than retail.

## 5. Three best next steps

1. Diff the 2003 alpha debug XBEs against the retail XBE and the 2017 build. Match functions by assert/path strings (albase.h, alfile.cpp, alxsound.cpp, algraphic.cpp, alfont2.cpp), then propagate names with a function-signature diff (Ghidra BSim or similar).
   Also check for Xbox XDK static libs (D3D8, DSOUND, xgraphics) first. Their known signatures strip out noise and isolate allib and game code.
2. Obtain and analyse the Rebellious Bullet XBE from the same dump. Grep for allib, alfn_, .alr and .ala strings. If present, it is a second allib consumer, which helps separate engine from game code.
   Its module layout may also differ from PD's.
3. Fetch the real PD wiki pages (Executable, ALR) in a browser, or via the wiki's GitHub ALR tool and archive.org copies, since the proxy hit a Miraheze bot-check. Import the wiki's chunk and field tables into engine docs and cross-check against pd_formats.
   In parallel, use archive.org (phantom_dust_pc_undub and similar) for any further dev-kit material. Do not spend time on Media.Vision or Sneakers beyond a quick check of one Sneakers XBE for "allib" strings.

## Gaps and caveats

Not checked: Blue Dragon, Lost Odyssey, NNN engine credits; Media.Vision PS2/PSP engines; Hidden Palace; Code Mystics' other ports; Orta's engine.
The MobyGames and PD wiki claims are from search summaries only and should be re-read when a browser is available.
