# Menu look — rebuilt from the original's menu art (2026-09-27)

The versus front end (title, versus setup, arsenal builder, result) and the
settings overlay (`menu.rs`) are drawn in the 2017 release's own style, from
its own files, after `Reference Images/Main Menu.png`. Code:
`pd_game/src/frontend/ui.rs` (shared look), the screens in `frontend/`,
`menu.rs`; converter `pd_convert/src/menus.rs`. Behaviour of the screens is
in FRONTEND.md.

**Menu structure (2026-10-03):** Title: VERSUS MODE, ONLINE, ARSENAL, OPTIONS MODE, EXIT GAME. Versus setup page 2 starts with GAMEMODES (STANDARD, CLOSE QUARTERS, ...); the online lobby has its own GAMEMODE row (host picks, joiner reads). The separate MODES screen was removed. Both selectors use the row style of the existing rules rows (label left, `< value >` right, dimmed and arrowless when fixed). Proof: `engine/shots/online/`.

## Convert (rebuild the cache)

```powershell
& $c menus "$D" cache\ui\menu     # $D = ..\game_files\Assets\Data (the Data folder itself)
```

Writes `cache/ui/menu/`: `st0..6.png` (StMenu.alr), `vs0..3.png`
(VsMode.alr), `wus0..7.png` (WusMnEn.alr), `logo.png` (st2's logo as a white
mask), `font.png` + `font.json` (the menu font, below), `bios.json`
(character names + bios). The HUD's `cache/ui/hud.ttf` (PIPELINE.md) is
still used for body text. Without `cache/ui/menu/` every screen still works
with plain panels and the HUD font (`MenuArt::has_art`).

## Where each piece comes from (all checked by eye on decoded PNGs)

| Piece | Source |
|---|---|
| Row panel (dark square tab + grey gradient bar, tinted teal / red when selected) | `StMenu.alr` tex 1, rect 0,0 306x38 — the tab is 46/306 of the width, the same share as in the reference |
| Ghosted "Phantom Dust" logo | `StMenu.alr` tex 2, rect 12,17 233x49 |
| Teal cloud backdrop | `StMenu.alr` tex 3 (512x512) |
| A / B button glyphs (dark disc, green A / red B) | `WusMnEn.alr` tex 0: A 97,129 19x19; B 97,153; X 97,177; Y 97,201. The glossy big buttons are at 67,2 / 67,62 (not used) |
| Up/down "Select" arrow, left/right arrow | `WusMnEn.alr` tex 3: 474,44 11x21 and 6,112 21x11 |
| Character portraits | `VsMode.alr` tex 1 (5 at x 0, y 0/98/196/294/392, 134x96), tex 2 (5 more), tex 3 (1, 130x96) |
| Stage previews | `VsMode.alr` tex 0: 17 pictures 254x112 — a 4x4 grid at x 642/898/1154/1410, y 0/114/228/342 (1410,342 is a red frame, not a picture) plus 0,382 and 256,382. 0,0 640x380 is one large teal-tinted picture, not used |
| Scan-lines, dark side fade, letterbox bands | made in code (`ui.rs`), not from files |

Rects from `pd_convert sprites <alr> <tex> 32` and by eye.

## The menu font — found: `System/dbfont.ddx`

Lead: the community wiki (phantomdust.miraheze.org, Modding/File_Formats/Font)
says it's a 1024x2048 compressed texture with no header. Verified: the file
is exactly 1,048,576 bytes = 1024x2048 DXT1 (BC1, 8 bytes per 4x4 block, no
header). Decoded as 2048x1024 it looks squashed (every other block row side
by side), as 1024x2048 it's a clean glyph atlas.

Layout (worked out from the image, confirmed by rendering test strings):
16x16 cells, 64 per row, **cell index = character code** in Windows-1252 —
row 1 starts `@ A B C…` at code 0x40, digits sit at 0x30, row 3 is
`À Á Â Ã Ä Å Æ Ç…` at 0xC0. Codes 256+ are Japanese/other glyphs (not used).
Glyphs are left-aligned in their cell, caps about 10x11 pixels — the same
blocky caps as the reference's "MAIN MENU_" and row labels. The converter
keeps codes 0–255 (`font.png`, 1024x64) as white letters with a baked
1-pixel dark shadow, and writes each letter's left edge + width
(`font.json`); the game spaces letters by width + 4 pixels (space = 9), which
matches the even gaps in the reference. The reference's smaller text
(description line, "Select / Confirm / Back", "Sign-in") is a smooth font
that isn't this atlas; we use the HUD font (Bahnschrift) there.

The menu scripts use prefixes like `1fSCENARIO MODE` (font 1),
`8#Select`, `2#Confirm`, `3#Back` (inline glyph numbers) and `580r`
(right-align at x=580 of 640) — `Menu/mainmenu_en.ssb`. Not decoded further.

## Character portraits — which is which: **pc00a..pc10a in order** (confirmed by eye)

Evidence: rendered every character model's head in the viewer
(`pd_viewer <cache> st06 chars/pcNNa --close 26 --angle 1.57 --look-y 16`)
and set each beside the portraits in atlas order. Each matched: pc00a spiky
grey hair + cheek marking, pc02a goggles on the forehead + long pale hair,
pc05a big face with a one-lens visor, pc08a long-haired woman, pc09a bald
with glasses and a scarf, pc10a spiky hair and sunglasses, and so on. 11
portraits, 11 `pc` characters.

## Character names and bios

Names: `setup/charaset/charaset_en.dat` (header: size, count, string-table
offset at byte 8; then NUL strings in groups id / name / name / voice bank).
Bios: `Menu/vsmenu_en.ssb` holds entries `"■ Name\nText\n\0"` (UTF-8 ■) — 10 of
11 characters have one (the hero, `player`, has none). Shown under each
player's column on the setup screen until 2026-10-09; Laurence had them removed as clutter (DECISIONS.md), so the game no longer reads `bios.json`.

## Stage previews — mapping ✅ confirmed (2026-09-27, UI polish lane)

The 17 pictures group into families by identical camera shot: three shots
appear in 3 colourings, one in 3, two in 2, one alone (3+3+3+3+2+2+1). That
is exactly the 17 stages the stage table marks selectable (below), whose
families are Highway 3, Palace 3, Refinery 3, City 3, Panorama 2, Lane 2,
Sein 1. Family per shot, now settled by evidence rather than guessed:

- **Family grouping**: confirmed by eye — cropping each of the 17 cells from
  `cache/ui/menu/vs0.png` at 3-4x zoom (`engine/shots/ui_stage_zoom_*.png`)
  shows each family really is one camera composition, recoloured (Refinery's
  truss+cable shot, Lane's rooftop silhouette, Palace's diagonal branches,
  City's overhead girders are each pixel-for-pixel the same framing across
  their 2-3 variants; only exposure/tint differs).
- **Which colour is which stage**: `setup/stageset/stageset_en.dat`'s third
  string per slot (the *internal code*, e.g. `st02Highway-evening`,
  `st05City-orenge`, `st23Palace-red` — read directly with the same parser as
  the display names, not a guess) names the recolour in words for every one
  of the 17 stages. Matched against the actual crop: st02 `-evening` is the
  warm orange gantry shot, st12 `-thunder` the grey stormy one, st22
  `-summer` the vivid blue one; st03 `-blue`, st13 `-yellow` (a green-yellow
  branch shot — close enough, the game's own word), st23 `-red`; st16
  `-yellow`, st06 `-blue`, st26 `-darkblue`; st05 `-orenge` (sic), st15
  `-gray`, st25 `-blue`. Every one of these 11 code-word/picture pairs
  matched on sight — no code word contradicted its crop.
- **Lane** (st07 `TownofMemory`, st17 `TownofMemory-blue`): only 2 pictures
  for 2 stages, so it's elimination either way; st07's picture is an obvious
  warm sunset (matches its display name "Twilight Lane"), st17 gets the
  other (a desaturated grey-toned version of the same rooftop shot).
- **Panorama** (st04 `Panorama-earth` "Earthquake", st14 `Panorama-fog`
  "Building"): the two interior-with-pillars pictures look near-identical at
  thumbnail size; zoomed side by side (`ui_panorama_ab.png`) st04's has
  visible desks/chairs and a plant silhouette through the window, st14's
  window is blown out to flat white/grey with no exterior detail at all —
  unambiguously the foggy one, matching its code and the existing guess.
- **Sein** (st08 `Ruin`): only one picture, no ambiguity.

Every family/colour pairing above matches what this lane's predecessor had
already guessed (`ui::STAGE_PREVIEWS` in `frontend/ui.rs` is unchanged) — the
guesses were right; they just hadn't been checked against the game's own
code-word evidence or a real crop before. Sheets looked at:
`engine/shots/ui_stage_preview_sheet.png` (all 17, labelled), and the
family-grouped zooms `ui_stage_zoom_lane_refinery.png`,
`ui_stage_zoom_city_panorama.png`, `ui_stage_zoom_palace.png`,
`ui_panorama_ab.png`. Stages without a picture (st01, st24, anything else)
show "No preview picture".

**Stage table fact** (for FORMATS/stages lane): in
`setup/stageset/stageset_en.dat` each 20-byte record (after the 16-byte
header) is 10 int16: three string offsets (each followed by a 0), the stage
number, a family/kind value, then a **selectable flag** — 1 for exactly 17
stages (st02–08, st12–17, st22, st23, st25, st26), 0 for the dummies,
**st01 (Edgar's Dream) and st24 (Panorama Boss)**. That settles
PIPELINE.md's open question: those two are not versus stages in the
original. (Stale note: `cache/stages.json` now marks them `versus: false` and the stage list drops them.)

## Layout

Every screen is laid out in a fixed 1280x720 design frame (`ui::DesignFrame`)
scaled to fit the window with `UiTransform` (`ui::fit_frames`, run each frame
by the screens' own update systems). Don't give the frame `overflow: clip`:
clipping ignores the transform and cuts the scaled frame back to 1280x720.
Bands: black 0–34, grey 34–104 (header), content 104–612 (clouds, scan-lines,
side fade), grey 612–684 (prompt bar + a dim keyboard line), black below.

## Proof (2026-09-27)

Screenshots (main checkout `engine/shots/`, looked at): `menus_vs_original.png`
(our title beside the reference), `menus_sheet.png` (all screens),
`menus_title.png`, `menus_setup.png`, `menus_setup_stage.png` (a stage with a
preview), `menus_arsenal.png`, `menus_result.png`, `menus_options.png` (overlay
on the title), `menus_options_match.png` (overlay in a match),
`menus_to_match.png` (a match started from the title with
`--menu-script confirm,right,down,down,right,right,up,up,up,confirm`),
`menus_legacy_demo.png` (`--demo` unchanged). `cargo test -p pd_game`: 205
passed (3 new: preview cells distinct and inside the atlas, font code map,
rule defaults); `cargo test -p pd_convert`: 2 new (bio + name parsing).

## Options overlay rows (2026-10-02, lane display)

**2026-10-08 (lane settingstabs):** a tab strip `LB [VIDEO SETTINGS] [SOUND] [CONTROLS] RB` sits under the header at panel top + 58 (`menu::TAB_Y`, 32 high, pixel font at 0.875; LB / RB labels 44 wide at the ends, three tabs with 8 px gaps between), and the rows now start at panel top + 100 (`ROWS_Y`, was + 66), 42 px pitch, at most ten rows (`MAX_ROWS`). Any row can carry a slider bar (shown only where that tab's row is a slider). FRONTEND.md "Options tabs".

The overlay panel is 680x648 in the design frame (ten 46px rows, header, prompts, key lines), centred in the game area, which is the window or, with a fixed aspect ratio, the largest rectangle of that ratio inside it (black bars outside; `display.rs`). The design frame is scaled to the area (`ui::fit_frames` reads `display::ui_size`). Field of view and camera distance are slider rows (a track, fill and knob between label and value); the four display rows show a browsed choice dimmed with an `A: APPLY` badge in the row's tab (2026-10-03: no keep-these-settings box any more). See FRONTEND.md "Display options".

## De-clutter (2026-10-09)

Laurence's "de-clutter the space": the menus keep the original's art, but say less. The description line and prompt bar are unchanged in look; the grey keyboard-keys line under the prompt bar only shows while the keyboard is in use; screens drop rows and captions that do nothing (detail: FRONTEND.md "De-clutter wave", list: UI-DECLUTTER-PROPOSALS.md).
