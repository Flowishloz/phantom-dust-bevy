# PC-side reverse-engineering tools (engine/re)

Python 3 standard library only. Each tool has `--help` and a `test_*.py` unittest that uses
synthetic data: `cd engine/re && python3 -m unittest discover -p "test_*.py"`.

**Never commit anything these tools produce from game data** (sheets, CSVs, inventories, string
dumps, label files holding decoded audio paths). Only the tools, tests and this README are ours.

| Tool | Purpose | Usage | Expected input |
|---|---|---|---|
| `sound_sheet.py` | HTML listening sheet to label unused sound ids; exports `labels.json` from the page | `python3 sound_sheet.py [--cache engine/cache] [--ids 44-300,355-500] [--out engine/shots/sound_sheet.html]` | `cache/sfx/common/<id>.wav`; optional `cache/skills.json` (ids found under sound-like keys count as used). `sound_labels_template.json` = empty labels for the default ranges |
| `effect_fields.py` | Histogram every 0x13 word (and .eft model/slot fields) to spot enums/flags; correlate with shader; diff two caches | `python3 effect_fields.py cache/fx [--csv out.csv]`, `python3 effect_fields.py --diff cacheA/fx cacheB/fx` | `eftNNNN.fx.json` from `pd_convert effects` (`pieces[].raw`, `.shader`) and/or raw 412-byte `eftNNNN.eft`; corrupt files are skipped |
| `strings_scan.py` | ASCII + UTF-16 string scan with offsets; compare two binaries | `python3 strings_scan.py a.xbe b.exe [--min 5] [--pattern RE] [--all] [--mask-sdk]`, `python3 strings_scan.py --compare a.bin b.bin` | any binary; default patterns allib, albase, alfn_, pdeft, mseffect, .alr/.ala/.alm, ssb, xact, c:\usr\xbox, assert, .cpp, .h |
| `xbe_info.py` | Identify an Xbox build: title, title id, timestamps, sections, libraries, kernel ordinal imports | `python3 xbe_info.py default.xbe [--json]` | original Xbox `.xbe`. Offsets and XOR keys are from memory of the xboxdevwiki layout: **unverified** against a real XBE |
| `file_inventory.py` | CSV inventory (path, size, ext, sha256 of first 64 KiB, magic) and diff of two inventories | `python3 file_inventory.py <folder> -o inv.csv`, `python3 file_inventory.py --diff old.csv new.csv` | any folder / extracted disc |
| `live_trace.py` | Live recorder for the running ORIGINAL PC game: attaches as a debugger, breaks on functions given by Ghidra address and logs their arguments (which sound ids, captures, erases it really fires) | `python live_trace.py --bp 0x140276330:sound [--bp ADDR:NAME[:ARGS]] [--seconds N] [--out f.csv] [--summary] [--quiet]`, `--restore` to repair; double-click `tools/bats/PLAY_ORIGINAL_TRACED.bat` | a running `PDUWP.exe` (Store app `Microsoft.MSEsper`); `game_files/PDUWP.exe` is used to cross-check the image base. CSV goes to `%USERPROFILE%\.pdtools\trace\` (never the repo) |

## Live tracing the original (`live_trace.py`)

Our own minimal Win32 debugger (stdlib + ctypes). It answers "which sound / effect does the original
really fire here?" from the running game instead of by ear.

- **Run:** start the game (`explorer.exe shell:AppsFolder\Microsoft.MSEsper_8wekyb3d8bbwe!App`), then
  `python engine/re/live_trace.py --bp 0x140276330:sound --summary --seconds 60`. Addresses are Ghidra
  addresses (image base `0x140000000`); the tool reads the live base (ASLR) and maps them. Presets: `sound`
  = `FUN_140276330(bank, id, ...)` (logs ecx, edx, r8d, r9d, two stack args), `capture` = `FUN_140261a00`,
  `erase` = `FUN_14014fd10`. Custom args: `--bp 0x1401c7630:frame:rcx,[rsp+0x28]`.
- **For Laurence:** double-click `tools/bats/PLAY_ORIGINAL_TRACED.bat`, play, close the window when done.
- **CSV columns:** `ms` (since attach), `tid`, `bp`, `ret` (caller, as a Ghidra address), `a0..aN` (the logged
  args; numbers >= 0x10000 in hex). For `sound`: a0 = bank, a1 = id (id N in bank 0 = `cache/sfx/common/N.wav`).
  `ret` tells you which call site played it (compare with `engine/docs/audits/P0-SOUND.md`).
- **Safety:** never uses gdb (it killed the game). `DebugSetProcessKillOnExit(False)` is set straight after
  attaching, so quitting never takes the game down. On finish, Ctrl+C, closing the console or an error, it
  writes the original bytes back, handles breakpoint events still queued, then `DebugActiveProcessStop`.
  Before planting it compares the bytes in memory with `game_files/PDUWP.exe` and refuses on a mismatch.
  While one thread steps over a breakpoint the others are paused for microseconds so no call is missed.
  The game is frozen only while it is being attached to and while each hit is logged (microseconds).
- **If the tool is killed hard** (Task Manager) with breakpoints planted, the game will crash at the next
  hit: run `python engine/re/live_trace.py --restore --bp <the same addresses>` at once.
- Sends no input to the game. Do not attach two tracers at once. Keep traces outside the repo.
- `python -m unittest test_live_trace` (in `engine/re`) covers the pure parts (address mapping, arg parsing,
  CSV, PE mapping).
