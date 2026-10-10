# Sister lead: Rebellious Bullet (research only, 2026-09-30, cloud)

Public pages only. No game files were downloaded or opened; this session has none. Claims marked READ were fetched; SNIPPET = search result only.

## What it is
- Unreleased Xbox game, build dated **5 Feb 2003 22:49:06**, found on an **Xbox DVT4 dev kit** bought at a Japanese auction; dump released **10 Sep 2019**, method "Xbox Neighborhood" (Hidden Palace, READ). The same dev kit held several **Phantom Dust builds** including two TGS demos (ResetEra, READ).
- Internet Archive item `rebelliousbullet_201912`: ZIP of 5 files, 204.6 MB, uploaded 31 Dec 2019 by a third party (READ). Hidden Palace's own download link is broken (READ).
- State: runs to the menu in the CxbxR emulator, "no gameplay yet" (ResetEra, READ); described as a shooter in the vein of Panzer Dragoon (forum opinion).
- Attribution to Futatsugi's team at Microsoft Game Studios Japan is an **inference from being on the same dev kit as Phantom Dust builds**, not a stated credit (ResetEra and search snippets).

## What is not known
- No public page lists its files, executable strings or engine. Hidden Palace's page says nothing about allib, `.alr` or symbols. Nobody has published a strings analysis. The AtariAge thread returned 403.
- So "it shares the allib engine" is untested. Reasoning (inference): it predates the Phantom Dust 2003 alpha builds (earliest there is 11 Jun 2003), so it may be an earlier state of the same in-house code, and the wiki says `.alm` files appear only in 2003 builds.

## The cheap test
On a lawfully obtained copy, on Laurence's PC only: run a strings scan over the XBE and file names for `allib`, `albase.h`, `alfn_`, `pdeft`, `.alr`, `.ala`, `.alm`. A hit would make it a second allib consumer, with different or more revealing debug text. No hit settles the lead at almost no cost. Do not commit or redistribute any of it; the repo's rule that game-derived data stays local applies.

## Legal / ethical note (for Laurence to decide)
It is a leaked pre-release build of Microsoft software, hosted by preservation sites. I did not fetch it and would not from the cloud. Whether to obtain it is his call.

## Related tool found on the way: xboxrecomp (MIT; github.com/sp00nznet/xboxrecomp, READ)
Turns an original-Xbox XBE into a native Windows executable by static recompilation: parse XBE, find functions, identify library functions (CRT, D3D, RenderWare), lift x86 to C, run on a D3D8-to-D3D11 shim with NV2A emulation taken from xemu. Worked on Burnout 3 (22,097 functions), Blood Wake, Wreckless. Needs the user's own copy of the game; has Ghidra/IDA symbol-recovery helpers.
Relevance: a native build of the **2004 Xbox** Phantom Dust could be instrumented with our own logging (every effect and sound call), which avoids injecting into the UWP port. Long shot: large effort, needs Laurence's own disc dump, and the Xbox build is not the 2017 port whose behaviour we read.

## Next steps
1. Decide whether to obtain the prototype; if yes, run the strings test above.
2. Re-read the wiki Executable page and TCRF page in a real browser for Rebellious Bullet mentions.
3. Keep xboxrecomp in mind as the route if the UWP injection route stalls.
