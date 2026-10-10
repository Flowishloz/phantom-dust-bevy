# Battle HUD (`pd_game/src/hud.rs`)

Rebuilt from the original game's own HUD art and laid out after screenshots of the 2017 release (single-player battles, 1280x720 and 1920x1080 captures found via image search, 2026-09-26). One HUD per player view.

## Source art

`Data/Menu/BtMenuEn.alr` holds **three textures and no models**, so the layout is not stored with the art. It's probably in the exe or `.ssb` scripts; not examined. Convert: `pd_convert hudtex $D\Menu\BtMenuEn.alr cache\ui` → `cache/ui/hud0.png` (256x512), `hud1.png` (512x512), `hud2.png` (1024x256). Find piece rectangles with `pd_convert sprites <alr> <texture index> [min alpha]`.

| Atlas | Contents (confirmed by eye) |
|---|---|
| hud0 | angular dark panels (health panels), the metal **skill dial** with four knobs (0,236 96x97), knob bezel (101,237 31x32), centre gauge (0,380 56x56), small panels, stick/L/R icons, lock/skull/check marks, coloured orbs (blue, cyan, magenta, yellow, green), white ball (26,492 18x18) |
| hud1 | generic lock-on ring art (grey, tintable; top-right ring 438,0 60x60), glow sprites (70x70 cells: aura/capsule glows, likely in-world capsule art), white panel |
| hud2 | banners: MISSION CLEARED / FAILED, **YOU WIN** (6,163), **YOU LOSE** (7,111), DRAW (292,111), **START** (538,6), countdown digits **3** (543,134), **2** (542,62), **1** (624,61) |

Exact rectangles are the `Piece` constants in `hud.rs`.

## Layout (from the reference screenshots)

- **Top left:** your name, then a panel with your health as a big number and one tick per health point. Below it, the other players' panels (see "Opponent panel" at the end: Health number and dots, a pill per bound skill, arsenal left, **aura / level**).
- **Top right:** a small panel with your arsenal count and the colour of the next capsule.
- **Bottom right:** the skill dial. The centre shows **aura** (big, orange) over **level** (small). Each knob shows the skill's cost on a disc coloured by capsule type, with the skill's name on a ribbon fading outward. Ours: X = left, B = right, LB = top, RB = bottom.
- **Lock-on:** a translucent reticle (`hud_marker.rs`) on the target's upper body, drawn from the game's own `hud1` sprites: a ring that depends on range (big ring close, medium ring + small ring, small crosshair far), the dim segmented arc and the bright rotating scythe beside it; orange close, yellow medium, green far. Static once settled; a 10-tick fly-in when it lands. Render-only; gone on unlock.
- **Banners:** 3-2-1-START at the start of each round, YOU WIN / YOU LOSE at a knock-out (per view).

Capsule colours (discs, ribbons, next-capsule orb): attack red, defence blue, erase purple, status green, special yellow, environment aqua, aura particle white (the program's own colour table, CAPSULE-VISUALS.md "Program read"; the pills of the opponent panel use the same order, below). An earlier note here guessed erase cyan, status magenta, environment green from the order of the atlas orbs; that was wrong.

## Lock-on marker (rebuilt 2026-10-01, `hud_marker.rs`)

**It is the game's own art.** `hud1.png` holds every piece; the earlier note
that the Y icon, arcs and scythe were missing was wrong: the sprite rectangles
are not in the atlas image but in the ALR's own sprite table (header at file
offset 0xa4: 3 textures, 180 sprite records of 0x3c bytes from 0x158; per
record the texture index at +0 and the UV rectangle as four floats at +0x24;
the program's "sprite index" is the record number). `hud_marker.rs` carries the
29 rectangles it uses. Re-derived from `FUN_140150360` (table build),
`FUN_1401983e0` (sprite draw; its last argument is a rotation) and
`FUN_140153a10` (the marker's draw, 0x140153a10), `FUN_1401536e0` (its update).

What the program draws (record numbers; x, y from the target point; the
full-view set / the split-view set, which is a smaller redraw):
- Band (the lock struct's `+0xc`): 1 within 80 u, 2 up to 300 u, 3 beyond.
  Close: ring 15 (98x98, 0,256) / 120 (76x76). Medium: ring 20 (60x60, 438,0) + dot 23
  / ring 125 (52x52) + dot 128. Far: crosshair 25 (32x32, thick variant 130 in splits).
- Then in every band: arc 14 (38x56) at (-41, -17) and the scythe, sprite 77 (100x100,
  0,52) centred on the target point, **rotated about its centre by `gauge * 1.8125 - 116`
  degrees** (gauge 0..64 from the distance inside the band; that is why the
  scythe swings as you move; the formula and its three bands are in
  `scythe_angle`). Full views also print the target's name and a distance
  readout `(distance - 10) * 100` with an underline; split views print neither
  (Laurence's side-by-side recording shows none). We draw the name and the
  readout in a full view only (`NAME_IN_SPLIT`, `READOUT_IN_SPLIT`); see "Lock-on readout and world labels" below.
- A new target runs a 10-tick intro (counter 10..1): three arcs fly in from
  `2 * counter^2` px round a fixed centre piece (far: the Y icon, record 28 / 133;
  medium: the small ring; close: none), then the settled marker and the lock cue.
  Footage: Y icon on frames 2-6, arcs closing on 7-10, settled from 11. All three
  bands' intro lists are in `intro`.
- A band change ramps 0.2, 0.4 .. 1.0 (+0.2 per tick, `DAT_1402f7d08`) with the
  bigger ring growing or shrinking; our version of which sprite does what is an
  approximation of the program's branches (unconfirmed detail).
- **No spin, no pulse.** The "pulsing" in `FOOTAGE.md` was the clip's two
  lock/unlock cycles. Sizes: `view_height / 480` px per sprite px (full and split
  views alike: 3.4 on the 1P recording, 1.6 on the 2P one).

Measured from the recordings (fit of the atlas pieces over frames; the
program's vertex colour could not be traced): the art is dark grey, the
band colour lands as `(255,235,62)` yellow, `(185,255,82)` lime green and about
`(217,122,71)` orange at an opacity of 0.32 (a ring pixel over a dark background
rises by 0.30-0.34 of that colour; the screen values of ours match the footage's within
a few counts, `fit` in the lane notes). The game blends in gamma space; the UI blends in
linear light, so the alpha is `0.32^1.7` and the art is multiplied up by its own grey
(`image_colour`). The anchor is the target at `LOCK_TARGET_HEIGHT = CHEST * 1.15`
(about 70 % of its height, as in the 1P recording). Orange is
the least certain (one clip, blurred).

Test hooks (screenshots only): `PD_TEST_LOCK_MARKER=1` marks the opponent in every view,
`PD_TEST_LOCK_MARKER_TICK=N` freezes the age at N ticks, `PD_TEST_LOCK_MARKER_BAND=
close|medium|far` forces the band (with a typical distance for the scythe),
`PD_TEST_LOCK_MARKER_LOG=<file>` writes `<file>.<view>` with the anchor and scale.
For tick-exact stills use `PD_SHOT_TICKS=400` with `--near` (no `--demo`: nobody moves).
Comparison sheets are in `engine/shots/xh_*.png` (never committed).

Layout is written in a 1280x720 design space and scaled per view: `min(view_w/1280, view_h/720) × 1.2` (the 1.2 keeps text readable in half-screen views). Anchors: top-left, top-right, bottom-right, centre, so it fits both split layouts. `layout_hud` re-places everything each frame; `update_hud` fills values from the sim.

## Not matched yet

- ~~Orange segments lighting up round the dial's centre gauge (aura meter).~~ Done, see "Aura recharge gauge" below.
- Top-right extras seen in one screenshot (row of orbs, rings), and the number next to the lock-on name.
- Player names: we show PLAYER 1 / PLAYER 2. The program's own names come from the player-name table (`DAT_1404bca30`); versus has none for local seats, and in its two-view split mode it prints "PLAYER 1" / "PLAYER 2" (`FUN_140155310`, `sprintf("PLAYER%d")`).
- Names and skill names in the battle HUD are drawn by the program in a thin smooth font (visible in every clip), not the pixel font; the pixel font (`dbfont`) is what its readout digits use. We have no source for the smooth one, so names keep the stand-in.

## Pixel font for names / skill names / timer (2026-09-27, UI polish lane)

Names (both players' panel labels, the lock-on name), the four skill-dial
names, and the battle timer now draw in the menu's own pixel font
(`System/dbfont.ddx`, MENUS.md) instead of the Bahnschrift stand-in, so the
HUD reads as one system with the menus around it. Everything else (health
numbers, aura/level, arsenal counts, skill costs/uses, status tags, the
skill-card info box) stays in Bahnschrift — the brief named only names,
skill names and the timer.

No reference screenshot of the original's battle HUD text exists locally
(`Reference Images/` has only menu and Lost Palace scenery shots) and a web
lookup for one was blocked, so this wasn't checked eye-to-eye against the
original the way the menu font was; it follows the brief's direction
directly and is a one-line revert (back to `hud.rs`'s old `text` closure) if
Laurence prefers the smooth font here once he's played it.

Implementation: `hud.rs` loads the shared `frontend::ui::MenuArt` (harmless
to load twice; `--demo`/legacy runs that skip the menus entirely are the
first thing to load it). A `pixel(text, colour, font_px)` closure
(`ui::glyphs_on`, building glyph-image children directly on the HUD's own
`Place`/`Bind` entity rather than a fresh one, since the per-element spawn
closures only get `&mut EntityCommands`) replaces the Bahnschrift `text`
closure at those call sites. `update_hud` retexts them and `layout_hud`
rescales them every frame through the same `ui::set_pixel` the menus use
(extended with a `scale` parameter so content and size updates don't
clobber each other regardless of which system runs first). Known small
regression: pixel-font text is always left-aligned (no centre/right
justify), so the two left-hand skill knobs' names sit at the same left edge
as the right-hand ones instead of hugging the dial — a cosmetic difference
from the original's inward-facing layout, not a functional one.


## Status tags and environment banner (2026-09-26)

Guide: "When these change, an icon will be displayed next to your Health bar." We don't have the icon art mapped yet, so `hud.rs` shows short green text tags beside each health panel (`Bind::OwnStatus` / `FoeStatus`): attribute totals (ATK±n, SPD±n%, HOMING±n%, AURA±n%, status + environment) and conditions (BOUND, FROZEN, POISON, CAMO, VOID, INVERTED, SCRAMBLED, FLOATING, REGEN, INVULNERABLE). The environment skill in force shows as an aqua banner near the top centre (`Bind::Environment`). The crystal itself is an aqua spinning gem in the world (`fx.rs`).

## Status icon search (2026-09-27) — not found; text tags stay

Searched every other menu/battle-looking `.alr` for a set of small icons matching the guide's 10 conditions + 4 attribute arrows. Method: `pd_convert info <alr>` to shortlist files with several small textures (an icon atlas looks different from a single full-screen background), then `pd_convert hudtex` to dump every texture as PNG and look at each with the `Read` tool; `pd_convert sprites <alr> 0 32` to get exact piece rects where a file looked promising.

**Files checked** (all read as PNGs, not just listed):
- `Menu/BtMenuJa.alr` — **byte-identical** to `BtMenuEn.alr`'s `hud0.png` (confirmed: 131,072/131,072 pixels match). Same 3 textures, same sizes (256x512 / 512x512 / 1024x256). Not a separate source of art — Ja and En share the whole non-text atlas, English banner text included (`YOU WIN`/`YOU LOSE`/`DRAW`/`START` are plain English glyphs in the "Ja" file too, so the localisation split for this menu is elsewhere, maybe .ssb or a table we haven't examined).
- `Menu/VsMode.alr` (4 textures) — versus-setup art: character portraits, stage preview screenshots. No icons.
- `Menu/Mission.alr` (5 textures) — story mission-select: NPC portraits, a polaroid-photo panel. No icons.
- `Menu/WusMnEn.alr` (8 textures, incl. a 32x32 and a 512x32) — a **system/network menu** sheet: gamepad face buttons (A/B/X/Y in 4 colours), an exclamation-mark and question-mark bubble, `L`/`R`/no-entry glyphs, arrows, decorative lightning-bolt "static" shapes, list-row panels. Network/error-prompt UI, not battle status.
- `Menu/StMenu.alr` (7 textures + 1 model) — title screen: the "Phantom Dust" logo, sky/cloud backgrounds, gradients. No icons.
- `Menu/Shop.alr` (5 textures) — the junk-shop background render plus two enter/exit arrow buttons. No icons.
- `Menu/DckMenu.alr` (2 textures) — the arsenal-builder's stormy-sky backdrop (already used, see PIPELINE.md); second texture near-blank at normal alpha.
- `System/aura.alr` (1 texture, 3 models) — the aura-particle effect model + its one texture; not a 2D icon.
- Spot-checked one file from each remaining Menu family and found them single full-screen images (concept-art/loading-screen style), consistent with the rest of that family by naming: `Menu/Mpt000.alr` (1 texture), `Menu/LdCg00.alr` (1 texture), `Menu/Cpt1En.alr` (1 texture, chapter title card), `Menu/MovMode.alr` (1 texture). Didn't open every one of the ~150 `MptNNN`/`LdCgNN` files individually — same naming family, same single-full-image shape as the ones opened, and none of that family's names suggest battle HUD.
- `MenuD/` has **no `.alr` files at all** — only `.ssb` scripts (menu/tutorial text and logic). Dumped the readable strings from `battlestart_en.ssb` and `battleend_en.ssb` (the two most battle-adjacent scripts): tutorial tip text and result-screen strings only, no icon names or references.

**What IS in `BtMenuEn.alr` / `BtMenuJa.alr`'s `hud0` beyond what was documented above** — a column of 21 small pieces at x=192 (plus the already-documented orbs at x=226), found with `pd_convert sprites Menu\BtMenuJa.alr 0 32` and read at 8x zoom:

| Rect (x,y,w,h) | Icon |
|---|---|
| 192,0,32,24 | a plain beige rounded panel (button backing, not an icon) |
| 192,28,30,28 / 192,58,30,28 | "L" trigger prompt (two states — normal / pressed?) |
| 192,88,30,28 / 192,118,30,28 | "R" trigger prompt (two states) |
| 192,148,22,28 | padlock (closed) |
| 192,180,26,39 | question mark |
| 192,226,24,16 | "X" in a rounded box (cancel) |
| 192,244,24,16 | skull and crossbones |
| 192,262,24,16 / 192,280,24,16 | a dagger/blade with a motion streak, mirrored left/right (a directional selector cursor, not "crossed swords") |
| 192,298,24,16 / 192,316,24,16 | circular rotate arrow, CCW / CW |
| 192,334,24,16 | the letter "S" |
| 192,352,24,16 | rewind (`◀◀`) |
| 192,370,24,16 | fast-forward (`▶▶`) |
| 192,388,24,16 | pause (`\|\|`) |
| 192,406,24,16 / 192,424,24,16 | a bullseye/target reticle (two sizes) |
| 192,442,24,16 | left-right double arrow |
| 192,460,24,23 | a rough pale-grey ball (a second, smaller "white ball" like the one at 26,492) |
| 192,486,18,18 | a **red/orange orb**, matching the other 5 coloured orbs at x=226 — this completes the 7 capsule colours (attack red, defence blue, erase cyan, status magenta, special yellow, environment green, aura white); `hud.rs`'s `capsule_color()` currently synthesises these colours procedurally rather than sampling them, which still matches by eye. |

**Conclusion — this is not the status-icon set the guide describes.** It reads as a generic, reused icon library (trigger prompts, lock, help, cancel, game-over, a rotate-camera hint, and — unmistakably — video-scrubber controls: rewind/fast-forward/pause, which belong to a transport bar, most likely the BGM-test jukebox (`MenuD/bgmtest_en.ssb`) or a save/load or character-viewer screen that shares this same atlas file, not to a per-condition battle icon). None of it maps convincingly onto the 10 named conditions (BOUND, FROZEN, POISON, CAMO, VOID, INVERTED, SCRAMBLED, FLOATING, REGEN, INVULNERABLE) or the 4 attribute arrows (ATK/SPD/HOMING/AURA up/down) — there's no snowflake, poison drop, eye/cloak, shield, or up/down chevron anywhere in it. The padlock is the one piece that *could* plausibly read as "BOUND" by shape alone, but nothing here confirms it's used that way (no reference screenshot shows it beside a health bar), so it's marked **unconfirmed** and not wired up — a lone, unconfirmed icon for 1 of 14 needed conditions isn't a found icon set. Per-condition text tags in `hud.rs` (`Bind::OwnStatus`/`FoeStatus`) stay as the only implementation; no code changed for this pass.

## Skill dial: cost centring and uses position (2026-09-27, HUD dial lane)

Two fixes to the dial knobs, from Laurence's own recordings
(`Phantom Dust - Moves`/`Moves 2`; frames extracted with ffmpeg to
`.pdtools/footage/`, not committed): a close-up of the dial from
`Psycho Knife - Opponent POV.mp4` (Rail Gun/Cluster Bomb/Psychic Wall, all
unlimited-use) shows each knob's Aura-cost digit sitting dead centre — both
axes — in its coloured disc emblem.

**Cost centring**, round 2: the first pass (below) hand-tuned a fixed y
offset from one screenshot measurement and overshot the other way (digit
too low, touching the disc's bottom edge) — a fixed offset only fits one
font/size/digit-count. Replaced with a measured fix: Bevy UI text draws
from the *top* of its node, not centred in it (confirmed by reading
`bevy_ui`'s own `extract_text_cursor`, which places glyphs at
`content_box().min`), and separately populates `TextLayoutInfo.size` with
the glyphs' own actual rendered size once text layout runs. `update_hud`
now reads that component each frame for `Bind::Cost` and sets
`place.pos.y = knob.y - (text_layout_info.size.y / scale) / 2.0` — the
digit's own measured height centred on the disc's real geometric centre,
in design pixels (`scale` is the existing `HudScale`). This is self-correcting
for any digit count/font/scale instead of a guessed constant. Measured
result (screenshot pixel analysis, digit bbox vs. the disc's computed
centre, excluding the bezel rim): **≤ 1.3 design px off-centre on both axes,
all four knobs, both `PD_TEST_SPLIT=tb` and `=sbs`** (was ~6px high before
the first pass, ~10px low after it). Also confirmed on the one 2-digit cost
in the game (skill 374 "Phantom Dust", cost 99, `registered: false` —
likely unused/debug): still centres correctly; it overflows the disc's
*width* at two digits, which is a disc-size limit for that outlier value,
not a centring bug, and stays out of scope.

**Uses left** (`Bind::Uses`, on limited skills, `s26` > 0 — DECISIONS.md)
isn't visible in any recording we have (every clip we have uses only
unlimited-use skills), so there's no original layout to match for it.
Round 1 made it a small dark floating badge above/left of the knob,
inconsistent between the two side knobs (above) and the two top/bottom ones
(left) — "out of place" per Laurence's brief. Round 1's fix (moved it inline
beside the name, same pixel font) was on the right track but had two
problems the orchestrator review caught: an Aura-orange digit read poorly
against the yellow ribbons, and — because the reserved gap was sized for a
single digit and the display then switched to an "xN" prefix — the badge
butted straight up against the name text with no gap, reading as one smeared
word ("x3Amulet"). Fixed by: (1) recolouring it to the name's own `LABEL`
colour, which already proved legible against every ribbon colour; (2)
widening the gap reserved between the bezel and the name at spawn
(`USES_GAP` = 30 design px, both ribbon directions) so "xN" has room; (3)
sizing and positioning the badge every frame from its own measured width
(`MenuArt::text_width`, the pixel font's real glyph metrics) rather than a
guessed fixed width, so it always sits flush against the bezel with a
consistent 2px clearance regardless of digit count. Now reads inline and
legibly as "x3 Amulet" / "Power x3" (name-side order following the ribbon
direction) in both splits.

Verified with a loadout of limited-use skills (`--loadout
17,322,299,239` — Power/Knife Grind/Amulet/Jump; their registered uses are
3/3/3/5, not 3/3/1/5 as round 1's note wrongly said — misread
FORMATS-SKILLS.md's "3 × 3 (Power, Knife Grind, Amulet)" as Amulet=1). Sheet
at `engine/shots/hud2_dial_sheet.png` (original dial crop vs. after, both
splits, 3x-zoomed).

### "Jump shows x3, should be x5" (2026-09-27, orchestrator review round 2) — not a bug, screenshot coincidence

The round-2 sheet's `--shot-frame 400` caught Jump (`s26` = 5) reading "x3"
— the same digit as Power/Knife Grind/Amulet, whose max is genuinely 3.
Checked both of the orchestrator's suggested causes and found neither:

- **Wrong value read?** No. Added a temporary `eprintln!` in `Bind::Uses`
  logging the slot id, `d.uses()` (max) and `f.spent[k]` every frame
  (`PD_HUD_DEBUG_USES=1` env var, removed again after this check — not in
  the committed code), then matched the log line against the exact tick the
  "Screenshot saved" line printed on. At that tick: `uses_max=Some(5)
  spent=2 left=Some(3)` — the HUD wrote exactly what it displayed. Jump is a
  movement skill the `--demo` AI presses often, so by tick ~450 (frame 400,
  after the 180-tick round-start countdown) it had already spent 2 of its 5
  jumps. Re-ran at `--shot-frame 172` (right as the round starts, `spent=0`
  logged at that exact tick): the dial reads "x5 Jump", clearly different
  from the others.
- **Does the pixel font's "5" render like "3"?** No. Cropped `font.png`'s
  own cells for `0`-`9` (`cache/ui/menu/font.json` gives each glyph's
  `(left, width)`; `hud2_dial_sheet.png`'s bottom strip) — ten distinct,
  legible shapes; "3" and "5" aren't close.

So: correct, live simulation state (`f.spent[3]`), coincidentally equal to
the other three skills' static max at the moment that screenshot was taken.
No code change. Re-shot the sheet at `--shot-frame 172` instead of `400` so
all four values differ and the coincidence can't recur.

## Skill dial: bezel/dial rims not concentric with their fill (2026-09-27, orchestrator review round 3)

Laurence: the rims (the four knob bezels, and the big ring housing the
dial) sit off-centre from what they surround (the knob disc, the aura
gauge) — visible as uneven metal-ring thickness around each circle. The
placement code centred `BEZEL`'s and `DIAL`'s *declared piece rect* on the
same point as `DISC`/`GAUGE`, which would be correct if each piece's own
artwork were centred within its own rect — but it isn't: **both `BEZEL` and
`DIAL` are ring shapes whose own hole is not centred in the rect that
`pd_convert sprites` measured for them**, most likely because that tool's
alpha-bounding-box measurement (like a plain "content bbox" reading) is
pulled off-centre by each ring's own drop shadow (heavier on one side),
while `GAUGE` and `DISC` are simple filled circles with no such shadow and
measure centred correctly.

**Measured** on `cache/ui/hud0.png` directly (no in-game rendering,
so no background/lighting/AI-demo noise): flood-filled each piece's
transparent (`DIAL`: opaque-black, since its hole is a baked-in black fill,
not a transparent cutout) hole from a small window centred on the piece,
separating it from the piece's own transparent margin outside the ring, then
took that hole's bounding-box centre vs. the rect's plain geometric centre:

| Piece | Rect (texture px) | Hole bbox (texture px) | Hole centre vs. rect centre |
|---|---|---|---|
| `DIAL` (96×97) | pos 0,236 | x14-77, y15-77 | **-2.5, -2.5** px |
| `BEZEL` (31×32) | pos 101,237 | x8-19, y9-20 | **-2.0, -1.5** px |
| `GAUGE` (56×56) | pos 0,380 | (plain filled circle, no hole) | -0.5, -0.5 (pixel-grid rounding only) |
| `DISC` (18×18) | pos 26,492 | (plain filled circle, no hole) | -0.5, -0.5 (pixel-grid rounding only) |

`GAUGE`/`DISC`'s ±0.5 is the expected discretisation for an even-sized rect
(content spans 0..w-1, whose midpoint is `(w-1)/2`, half a pixel short of
`w/2`) — i.e. genuinely centred, confirming the fault is in `DIAL`/`BEZEL`
alone. Converted each piece's texture-px hole offset to design px at its own
render scale (`DIAL_HOLE_OFFSET` = -3.02,-3.02 at 116/96×117/97; `BEZEL_HOLE_OFFSET`
= -1.94,-1.41 at 30/31×30/32) and applied it as a placement correction: `DIAL`
and `BEZEL` are now positioned `-DIAL_HOLE_OFFSET`/`-BEZEL_HOLE_OFFSET` design
px past the naive centred position, so their *hole* — not their rect — lands
on `GAUGE`/`DISC`. Moved the ring to meet the fill rather than the other way
round, since `GAUGE`/`DISC`/the knob positions/aura+level text are all
already correctly centred on `DIAL_AT`/each knob centre.

Confirmed by re-screenshotting and zooming both `PD_TEST_SPLIT=tb` and
`=sbs`: the dial ring now shows even metal thickness on all sides around the
gauge (was visibly thicker top-left, thinner/overlapping bottom-right); the
red/white knob (Power) shows the clearest before/after difference since its
disc previously overhung the bezel's bottom-right edge. The two yellow knobs
(Amulet/Jump) had a smaller original offset and the fix there is correspondingly
more subtle at normal zoom, but is the same measured, mathematically-derived
correction. Sheet at `engine/shots/hud3_dial_sheet.png` (original crop, all
four knobs before/after in both splits, dial ring before/after).

## Skill dial round 4: main ring default position, knob bezel too thin (2026-09-27, HUD dial lane 4)

Laurence picked from an A-J calibration sheet: the **main ring is right**
with `PD_DIAL_OFF=-1.5,-1.5` (now the default -- `DIAL_HOLE_OFFSET` changed
from `(-3.02,-3.02)` to `(-1.5,-1.5)`; the `PD_DIAL_OFF` env override still
works for further calibration). For the four knobs, the bezel-position fix
(round 3, above) was closest but still wrong: "the colour circles are
marginally off, but the rings are further off" -- pointing at size, not
position.

**Measured against the original**: extracted static frames of the dial from
`Psycho Knife - Opponent POV.mp4` (`.pdtools/footage/hud4/`, not committed;
picked a frame with an idle, undamaged dial and no motion blur). Measured the
Rail Gun knob (top) by radial sampling from its own disc centre outward at
angles clear of the ring's connecting spoke and neighbouring text (a plain
alpha-bbox reads the spoke/text as part of the knob): disc outer radius
~28px, bezel outer radius ~42px, i.e. **disc/bezel diameter ratio ~0.65-0.68**
in the real recording -- video compression and the shallow, blurry source
keep this a ballpark figure, not a sub-pixel one.

Compared against the source art (`cache/ui/hud0.png`, alpha channel read
directly): BEZEL's own rect (31x32) to DISC's own rect (18x18) is
**18/31 = 0.581** -- close to the measured ratio and, tellingly, almost
exactly the ratio DIAL and GAUGE are already drawn at (116x117 / 68x68 =
0.586, matching their native rect ratio 56/96 = 0.583 almost exactly) --
the pairing Laurence just confirmed as correct. BEZEL and DISC, by contrast,
were drawn at **24/30 = 0.8**, a scale relationship the two pieces don't
share in their own art (BEZEL was left near its native size, scale ~0.97,
while DISC was scaled up 1.333x) -- explaining exactly Laurence's call: the
fill (DISC, close to its own correct look) marginally off, the ring (BEZEL,
scaled inconsistently with its partner) further off.

**Fix**: gave BEZEL the same scale as DISC (`BEZEL_SCALE = 24/18 = 1.333`,
`hud.rs`) instead of drawing it near-native, growing it from 30x30 to
~41.3x42.7 design px and scaling `BEZEL_HOLE_OFFSET` by the same factor
(`(-2.0, -1.5) * BEZEL_SCALE`) so the round-3 hole-centring fix still lands
correctly at the new size. DISC is unchanged (24x24), matching "marginally
off" -- no evidence it needs to move.

**Verified**: `cargo test -p pd_game -p pd_formats -p pd_convert` still 261 +
36 + 7 green. Built an overlay sheet (`engine/shots/hud4_overlay.png`):
original footage crop, our dial before/after (scaled to the original by the
measured bezel-diameter/knob-spacing ratio, ~1.37x), a 50/50 blend overlay
of each against the original, a zoomed single-knob overlay, and full
normal-size in-game views in both `PD_TEST_SPLIT=tb` and `=sbs`. The
before-overlay shows a visible gap between our (thinner) ring and the
original's ring outline around each knob; the after-overlay closes that gap
-- the ring now tracks the original's outer edge closely at every knob, with
no code path left that scales BEZEL and DISC by different factors.

## Four-player HUD (lane hud, 2026-10-01)

Geometry lives in `pd_game/src/hud_layout.rs` (pure, unit-tested); `hud.rs` places and fills.

**Source in the original.** No four-player layout was found. The HUD atlas (`hud0`) has no per-player-count pieces and the layout is not in any atlas or `.ssb`; the program draws it and that function is unread. What the atlas does hold: besides the own panel (0,36 141x25) and the big foe panel (0,78 134x36) there are a **short panel** (0,116 134x20) and a **dimmer twin** (0,138 134x20), and two more big panels (0,160 / 0,198 134x36, the second dim). (2026-10-01, foepanel: the short panels and their dim twins are no longer used. In Laurence's four-player recording every other player has the full panel; the short row is the program's compact variant for other settings, `FUN_140155310` with `+0x44 == 0`, not built.) The bullseye (192,406 24x16) marks the target. Everything else (the list, ALLY tag/colour, the over-head marker, the pause banner) is our reduction from that art, not a copy.

**List of the other players** (top left, under your own panel): one row per active player other than you, in slot order. Each row is the original's opponent panel ("Opponent panel" below), the lock target's lit with a glow. A teammate is blue and tagged `ALLY`. A player at 0 Health shows a half-dark panel and "DOWN".

**Health bar from `Fighter::max_health` (own panel).** Up to 40 ticks are spawned per bar; tick `k` shows if `k < max_health`. The bar keeps its length, so 20 Health is the old one-tick-per-point bar, Heavy (40) is twice as many half-pitch ticks, Puny (10) and KO Battle (5) are fewer and chunkier (`hud_layout::tick`). The other players' panels follow the original instead (20 dots, one per point, a second colour past 20; below).

**Teammate marker.** In your view a blue orb + name floats above a living teammate (`ALLY_MARK_HEIGHT`); it hides while that teammate is your locked target (the lock marker is on `players[i].target` through `aim_at`, as before). The lock marker, lock name and the list's target row all follow `Player::target`.

**Pause banner.** `PAUSED / PLAYER 2 PAD DISCONNECTED / RECONNECT THE PAD TO CONTINUE` is drawn once, centred on the whole screen (the HUD's screen layer `HudScreenRoot`, items tagged `ScreenItem`, at the largest view's scale; in every view until 2026-10-09, de-clutter item 12) while `Seats::paused(players)`; several lost seats read "PLAYERS 2 AND 4 PADS DISCONNECTED". Test hook: `PD_TEST_PAD_LOST=2,4` (seat numbers from 1) shows it without pausing the sim.

**Scale and legibility.** `hud_layout::scales`: full and half views keep `min(view/1280x720) x 1.2`; a quarter or the third player's wide half (3-4 players; `ViewLayout::is_grid`) draws the panels at x1.5 and the dial cluster at x1.35 with its own text (skill names, costs, uses, timer) another x1.25 (`QUAD_DIAL_TEXT`). The timer moves below the dial in a quarter (above it, it collided with the lock-on name). A left knob's name moves in to end short of its "xN" when the larger text would run into it. The capture hint ("Laser (aura 1) - press a skill button to capture") is centred in the free width beside the dial's ribbons when the view is short enough for them to reach it (top/bottom and quarters), the whole width otherwise; it used to print over the ribbons.

Measured on a 1920x1080 TV (quarter 960x540; cap height of the glyphs, from captures `engine/shots/hud_4ffa_t0400.png` x 0.6): own name 15.6 px, own Health number 25.8 px, other-player names 20 design px (cell 22.5 px, cap about 15.5 px), compact Health 17 px, Aura/Level 16-19 px, dial skill names about 14 px (cell 20 px), timer cell 28 px. Before: names 12-13 px, dial names about 11 px. A duel in the two half layouts is unchanged.

**Test hooks (hud.rs, screenshots only):** `PD_TEST_RULE=<0-11>` starts the match under that rule once (4 = Heavy: 40 Health, level 5; 10 = KO Battle: 5 Health), `PD_TEST_PAD_LOST=<seats>`, `PD_TEST_HINT=1` (show the capture hint for the skill on button 0 without walking to a capsule).

**Not verified.** The "down" row (dim panel, DOWN tag) has no capture: four-player demo matches panic at the first knock-out in `fx.rs:144` / `sfx.rs:533` (index past two players; the events lane S4, not this one).

## Opponent panel (2026-10-01, lane foepanel)

Laurence: the opponents' panel was wrong (a second Health bar under the first); in the original it shows the kinds of skills they have bound. Rebuilt from his recordings (`Phantom Dust - Moves`: the 4-player clips show three panels at once, the Psycho Knife clip is a 1v1) and from the program's panel drawing function (`FUN_140155310` in `decomp_all.c`; parameters: the HUD object, "is another player", the player's slot, name index, "is a teammate", x, y, lock-target slot). Panel art is the same `hud0` piece as before (0,78 134x36), drawn at the own panel's scale (x1.59, `hud_layout::FOE_S`). Positions below are in the panel's own units (134x36), from the program and measured on the clips (3.33 px per unit in the 2878-px captures; every position matched within a unit).

| What | Where (panel units) | Meaning and evidence |
|---|---|---|
| Name | above the panel | player name; program colour 0x6f3c14 (dim), lifted a little because we draw no black shadow pass |
| Big number, top left | 13, 3 | **Health.** Falls when hit; 00 for the downed player; its colour is the tier colour (below). Confirmed |
| 20 small squares, top | from 45, step 4, 3x3.5 | **Health, one dot per point** up to 20: JD 09 had 9 dots, Andro 14 had 14, Partheno 12 had 12, Freia 20 had 20, Health 0 none. Past 20 the second tier is painted over the bar: Health 37 = 17 dots in the tier-1 colour (amber) and the other 3 of the 20 in the tier-0 colour (red-orange), exactly the 17 + 3 of the 1v1 clip. **Red dots** (0xc80000) are Health lost a moment ago: the program keeps a trailing Health that jumps up with Health, holds 30 frames after a drop, then falls 0.25 a frame (30 Hz). Checked on the own panel in the clip (7 to 3): the red trail holds about 1 s and drains in about 0.5 s. Confirmed |
| Pills, lower-left cell | diamond: top 20,21; left 13,24; right 27,24; bottom 20,27 (pill 7x4) | **One pill per skill bound on the player's buttons, coloured by capsule kind.** Andro in the 4-player clip: a pill appeared when he captured (3 to 4 pills while arsenal fell 19 to 18 to 17 and level rose 02 to 03) and a blue pill vanished when he used it (4 to 3). Colours: the pill sprites are hud0 (78,402) + 6 per row: red, blue, aqua, magenta, yellow, green; the program's kind table (0x1402f1348: attack 3, defence 5, environment 7, erase 9, special 11, status 13) puts attack red, defence blue, environment aqua, erase magenta, special yellow, status green, equal to the capsule colour table. Colours measured on the clip match the atlas (red 146,38,52 vs atlas 156,52,49; yellow 169,173,34 vs 181,166,24; magenta 146,36,119 vs 156,52,123). No pill for an empty button. Unconfirmed: which of our four buttons uses which point (the program's slot order is top, left, right, bottom; ours uses the compass point of our own dial's knob, `SLOT_COMPASS`); and the Aura Particle on a button, which gets a white pill (the atlas has none) |
| Number, lower middle | 47, 13 | **Arsenal left** (capsules still to come). Falls by 1 on every capture (Andro 20, 19, 18, 17 with each new pill), starts at the deck size. Confirmed. The program shows a different number here when the arsenal is empty (a countdown from 30): not built |
| "AA/LL", lower right | 77, 13 | **Aura / Level.** Level rises with captures (Andro 06/06 to 06/07, 01/02 to 02/03); aura refills over time and is spent by skills (Andro 10/11 to 11/11 while idle). Confirmed; same as the own dial's aura over level |
| Orange glow | 7 units around | the lock target's panel only (`GLOW`, the soft white shape at hud0 0,0, tinted 0x824600). The earlier bullseye icon is gone: the original marks the target with the glow alone |

Colours (program table `0x1402f2a40`, entries 0x11-0x13 for the lock target, 0x15-0x17 for the others): tier colours, target 0xad3a03 / 0xb06b05 / 0xcc9835, others 0x642604 / 0x613b03 / 0x8a6625; arsenal / aura / level text 0xad3a03 (target) or 0x642604; panel tint 0xf57e27 (target) or 0xe98646. The tier is `(trailing Health - 1) / 20`. The program shows teammates in green (0x89bc18...); the four-player lane's blue is kept (red and blue swapped from the orange values). A downed player is drawn at half brightness (the program dims by a flag; the exact fade is unconfirmed).

Not drawn: a small marker at (12, 12) when a per-player flag is set (sprite 0x22c, meaning unknown), and the status icons right of the panel at x+140 (our text tags sit there). The program also has a compact row for the other players (28 units: panel, dots, four pills in a row 8 apart) when its setting `+0x44` is 0, and, in its local split-screen mode (`+0xc3c`), panels with only the Health number and dots and no pills; the recordings are single-view and show the full panel for every player, which is what we draw in every layout.

Own panel: the clip shows the red trail on the own Health ticks; added (ticks between Health and the trailing value are red). Tier colours past 20 on the own panel are not drawn (we keep one tick per point). Number font: still Bahnschrift with a little letter spacing; the original's wide square digits come from a font texture that is not in the HUD atlas.

Test hook added: `PD_TEST_HEALTH=<player 1-4>,<health>` pins a fighter's Health (screenshots only). Proof (not committed): `engine/shots/foe_sheet_1v1.png` (original 1v1 crop at Health 37 next to ours at 37) and `foe_sheet_4p.png` (original 4-player crop next to ours), made with `PD_TEST_RULE=4 PD_TEST_HEALTH=2,37 pd_game --demo --near` and `--players 4`.

## Aura recharge gauge (2026-10-01, lane auradial)

Laurence: "the bottom right hud element is supposed to be animated when recharging aura, it lights up clockwise." A ring of **ten orange segments** round the dial's centre gauge lights up clockwise as the next Aura point refills. Code: `hud_dial.rs` (rule, table, tests), spawn/update hooks in `hud.rs` (`Bind::DialSeg(k)`).

**The program's rule** (own-player dial, `FUN_140151f90`, after the Aura / Level digits, around `0x140152490`..`0x140152560`):
- Drawn only while **Aura < Level**; at full Aura (== Level) there is no segment.
- `t = (aura - (int)aura) * 10.0` (`0x1402f7f7c` = 10.0); **`(int)t + 1` segments** are drawn, entries 0.. of the table at **`0x1402f1470`**. So the first segment lights the moment a point starts refilling, one more every tenth of a point, and the whole ring is on just before the point lands; then the Aura digit ticks up and the ring drops back to one segment.
- Each 12-byte entry is `(piece, x, y)`: pieces 58..67 at `(29,4) (41,9) (48,20) (42,35) (29,45) (14,45) (6,36) (4,20) (6,9) (14,4)` from the top-left of the gauge piece (57, our `GAUGE`, hud0 0,380 56x56): clockwise, starting just right of top centre.
- Colour `0xff5a1e02` (same as the Level digits), drawn as a plain tint over the white art. The gauge ring under it is tinted `0xff6e2500` (ours was a much brighter guess, now the program's value; without that the lit segments did not read against the ring).

**Atlas pieces** (hud0, white art, below the gauge; found by connected-component scan, matched one-to-one to the ring's shapes): `(60,398,14x7) (60,408,10x12) (78,438,4x15) (60,426,10x11) (60,444,13x7) (60,454,13x7) (60,464,9x11) (78,456,4x15) (60,482,9x12) (60,500,13x7)`. The piece-id to atlas-rectangle map is not in the exe (not found as ints/floats there nor in `BtMenuEn.alr`); the order is fixed by the coordinates above and the shapes.

**Checked against the footage** (Laurence's Slide Laser clip, `Orb Collect` clip; frames kept under `.pdtools/footage/auradial/`, not committed): measuring the ring per frame, segments light in exactly this order (the first, top right, is lit as soon as Aura < Level; seg k lights at fraction k/10), one every 8 frames (0.27 s) at Aura 9 where the curve gives 0.35 points/s, all drop when the digit ticks up, and nothing is lit once Aura reaches Level (Orb clip, "01/01"). Lit colour in the video is about (90, 25, 3) like the Level digits. Sheet: `engine/shots/ad_dial_sheet.png` (footage on top, ours below, 1 to 10 segments lit).

Capture: `PD_TEST_AURA=1,9.0,11` (screenshots only; fighter 1's Aura in points and Level, set once) with `--near` (no `--demo`, so nobody spends Aura), `PD_MUTE=1` and `PD_SHOT_TICKS=230,240,...`; crop the dial of the first view.

Not confirmed: the fade (`0x390` state) and the grey / purple / yellow tint states of the dial (spectator-style modes, `+0x38c` = 0/1/2) are not drawn; the aura/level digit colours (`0xffad3a03` / `0xff5a1e02`) differ from our `ORANGE` / `ORANGE_DIM` (ours is lighter and yellower than the footage's), left alone as outside this lane.

## Lock-on readout and world labels (2026-10-01, lane hudbits)

**Readout.** `FUN_140153a10` 0x1401544fc..0x1401548dc, built in `hud_marker.rs`. READ unless marked.
- Text `sprintf("%5d", int((distance - 10) * 100))`, the value taken as 0 up to 0.9 past 10 (`readout_text`). Checked on the footage: 6182 / 22858 / 34100 are the three bands.
- Drawn only in a settled marker (after the 10-tick intro), in every view mode except 2 (`cmp [view+0x2a0], 2`). Laurence's side-by-side recording is the mode that shows none, so `READOUT_IN_SPLIT` is false and a quarter or half view prints none. UNCONFIRMED which of our two-view layouts the program's mode 1 would correspond to (it prints the readout, but not the name).
- Position: row `64 - gauge` of a 65-pair signed-byte table at 0x1402f1370 (`READOUT_TAB`; the same gauge that turns the scythe), text top-left at `(tab_x - gl - 50, tab_y - ly - 15)` sprite px from the target point, underline at `(tab_x - gl - 63, tab_y - ly)`; `gl` 64 (63 when the gauge reaches 64), `ly` 7 below gauge 28, 29 from 28, 30 at 64 (`readout_places`). MEASURED check: medium, readout 22858 on the Laser clip, text 100 px left and 10 below the point as the table gives.
- Font: the HUD's pixel font (16 px cell, 1024-wide atlas, 64 per row), glyph advance `width + 1` from the table at 0x1402f2890 (digits 9, space 7), a black copy one px right and down. Ours cuts the same atlas (`ui::MenuArt::glyph_ink`, `font_image`, additive) and uses that advance.
- Colour: the band's own word, close 0xffff7725, medium 0xffffd525, far 0xffa5ff25 (ARGB, 0x140154642 / 0x14015463b / 0x140154634), for the text and the underline. The marker's art colours (`COLOUR_*`, fitted to the footage) are left as they were; they sit near these words once the opacity is applied (footage ring colours (217,122,71) / (255,235,62) / (185,255,82)), so the earlier note that the program's colour could not be traced is settled: it is this word, drawn at 0.32 opacity for the rings.
- Underline: sprite record 76, hud1 texture, a 64 x 1 line (0,466), tinted by the band word.
- The name ("PLAYER n", at +4, +30, shadow +1, +1) is drawn in the same band colour but in the program's smooth font (see above); we keep the pixel stand-in at the fitted tint.
- Proof: `engine/shots/hb_dist.png` (footage on the left, ours on the right, close / medium / far; ours made with `PD_TEST_LOCK_MARKER=1 PD_TEST_LOCK_MARKER_TICK=40 PD_TEST_LOCK_MARKER_AT=0.5,0.45 PD_TEST_LOCK_MARKER_BAND=<band> pd_game --players 1 --near`, `PD_SHOT_TICKS=400`). `PD_TEST_READOUT_SPLIT=1` shows it in split views too. Tests `readout_*`, `glyph_advance_*`.
- Also in the program, not built: a team icon (sprite record 75) at (-20, -20) of the marker when the target is on your team (`0x140154aba`).

**World labels.** `FUN_140150c30` (the HUD's per-frame loop) prints a white name at every other visible, uncamouflaged player's projected point (`+0x13f94/98` plus the screen centre, text 16 high, black shadow) **only when the view mode is 0**; the branch is not reached in either split mode, and the panels are drawn after it. So in a split view there is no label above a fighter at all (the footage's white "Partheno" is a full-view clip). Our teammate marker (blue orb and name, an invention of the four-player lane) overprinted the opponent list in 3-4 player views; it is now drawn only in a full view (`WORLD_LABEL_IN_SPLIT` false; `PD_TEST_WORLD_LABEL_SPLIT=1` restores it for a capture). Proof: `engine/shots/hb_4p.png` (before / after, `--players 4 --teams 0,0,1,1`).

**Opponent panel leftovers, settled.**
- Pill slots: the panel's pill diamond and the own dial walk the same four slot records in the same order, top, left, right, bottom (`FUN_140155310` offsets (20,21) (13,24) (27,24) (20,27); `FUN_140151f90` knob offsets (+19,+0) (-18,+18) (+55,+18) (+19,+57)), so a pill sits at the compass point of the knob that holds the skill. Our `SLOT_COMPASS` does exactly that. READ.
- Aura Particle pill: the pill table at 0x1402f1348 maps kind 0 to sprite record 1, hud0 (46,504) 8x4, a grey (173,170,173) pill the atlas does hold. It replaces the synthetic white one. READ.
- Teammate colour: green. Panel 0x89bc18 on the lock target and 0x829f2a otherwise, tiers 0x89bc18 / 0x7fb20e / 0x75a804 (lit) and 0x5a8102 / 0x466d02 / 0x325902, text 0x89bc18 / 0x5a8102, name 0x89bc18 (palette 0x1402f2a40 entries 0x1a-0x1c and 0x1e-0x20). The earlier blue is gone. READ.
- Downed look: none. The program has no per-player dim for Health 0 (its `+0x21c` fade is a script-driven fade of flagged parts, `FUN_14022c780`), and the Laser clip shows JD at 00 with the same brightness as the live panels (bright-pixel mean (143,38,49) against (143,39,47)), no dots, the pills still there and no tag. So the half-dark panel and the DOWN tag are removed and the glow follows the lock alone. MEASURED + READ.
- Not settled, unchanged: the status-condition tint of the panel (`FUN_1401509f0`: the panel colours cycle to grey / purple / yellow while the player has certain statuses; the status numbers 5, 6, 0x24 and a set of four are unnamed), the compact row and the status icons beside the panel.

## HUD size option (2026-10-02, lane display)

`layout_hud` multiplies `hud_layout::scales` by the HUD size setting (75 / 100 / 125 / 150%, `settings.display.hud`) and sizes the views from the game area (`display::GameArea`) instead of the window, so the HUD sits inside the bars of a fixed aspect ratio. 100% is the old numbers exactly. The atlas textures keep their default (linear) sampling; on a 3840x2160 capture at 150% the art edges stay clean (`engine/shots/display_4k_hud150.png`).

**Three players (2026-10-03, split3).** P3's view is the whole bottom half, so each player's HUD is fitted to its own view (`hud_layout::hud_view`, `HudScale::of`). The scales follow the view height, so P3's HUD matches P1's and P2's size; its panels sit at the left of the wide view, its dial at the bottom right, and the capture hint is centred in the free width beside the ribbons. Tests: `the_third_players_hud_fits_the_wide_bottom_view`, `hud_views_for_one_two_and_four_players_are_unchanged`, `the_capture_hint_stays_off_the_dial_in_the_wide_view`.

## Skill card (D-pad skill info), rebuilt from the program (2026-10-03, lane infomusic)

Laurence: the D-pad info "doesn't show the range (close, medium, far)". Cause: our panel was a plain text box (name, school, kind, cost, uses, STR/DEF, note) and the range was never shown. The original draws a **card** with a range icon. Code: `hud_card.rs` (rules, layout, tests), `hud.rs` (`Bind::Card(CardPart)`, spawn + `update_hud` arm).

READ (`FUN_140168410` = the card, called by `FUN_140155160` = the battle D-pad hook and by the arsenal list `FUN_140158930`; sprite names are in the sprite records of `Menu/WusMnEn.alr`, which carry their own file names):
- A 234 x 96 window (`detail_window`, record 22, texture 7) with a 230 x 26 title bar (`detail_window_title`, record 23, texture 3) tinted by the capsule colour (stack array at the start of the function, from `0x1402f8610`: attack 255,165,165; defence 165,170,225; erase 225,165,205; status 166,225,165; special 225,223,165; environment 165,204,225; Aura Particle white). Name dark (0x262626) at (10, 3), school icon at (181, 1) (`detail_icon_type_pk/op/na/ki/ma/aj`; table written by the menu object's constructor: 1 Psycho pk, 2 Optical op, 3 Nature na, 4 Ki ki, 5 Faith ma, 0 aj).
- One stats row (sprite y 25, glyphs y 22): `COST` label (record 1) + cost ("X" for a hold-to-charge skill, s37 = 2); `STR` (record 14, attacks) or `DEF` (record 2, defence) label + value ("X" when an attack's strength is computed, s45 != 0; no DEF label on [Course] / About Face: the blank box record 0 covers it; other kinds show the blank box); a small box icon (record 13) + uses (the infinity sign when s26 < 1); **the range icon** (66 x 12, the word and an arrow).
- The range icon is one of nine words: `short`, `med`, `long`, `self`, `all`, `mine`, `capsule`, `auto`, `env` (records 12, 9, 8, 10, 3, 4, 5, 6, 7). It comes from `FUN_140167970` (attacks: short-range motions 3/12/13/14 = short; motions 8/9 = mine; s44 != 1 = med; s44 = 1 = long; motions 4/16 are left to the next rule), `FUN_140167a10` (the target kind from s43 and s44) and the branch at `0x140169b05` (defence = self, environment = env, motion 4 = auto, class 1 = self, 3 = all, 4/5 = capsule). Ported in `hud_card::range_icon`; on the real table: Psycho Wave short, Laser long, Bomb med, Remote Mine mine, every defence self, every environment crystal env, Shatter capsule, Massive Quake all.
- The description is the skill table's own note, wrapped at 229 px, grey shadow at (3, 46) then dark text (0x3e3e3e) at (2, 45), lines 16 apart, drawn at 7 x 14 (the name 8 x 16).
- An empty button draws no card (`FUN_140155160` passes skill -1 and the card function returns); ours used to print "Empty / Capture a capsule...", now nothing, as the original.
- The Aura Particle has no table record (text 301, "Increases Level by 1."): shown with a white title, blank box, unlimited, no range icon (UNCONFIRMED: the original's card for it was not traced).
- The arsenal builder's text card gets "Range <word>" in its stats line.

UNCONFIRMED: where the card sits (the program calls it at (465,198) full view / (-39,210), (446,210) split views in a frame we have not mapped; ours sits above the dial); the infinity sign (the original draws a font glyph at dbfont (352,1856) 16 x 12; ours is a hand-made stand-in); digit spacing (the original draws the numerals as 14 px glyphs from the font's row 0x74); the note font width (7 x 14 per glyph is read, our pixel font's advance is its own); whether the battle call also draws the card's extra backing (`FUN_140168410`'s last arguments are not visible in the caller). No footage or screenshot of the original's card exists in Laurence's folders (checked Pictures / Screenshots / Reference Images), so there is no side-by-side; the proof picture is ours rendered from the original's sprites.

## A/B switches from the de-clutter wave (2026-10-09, not the default)

Built for Laurence to pick from (DECISIONS 2026-10-09, UI-DECLUTTER-PROPOSALS.md items 14 and 15); the defaults stay as the original (a full panel for every other player, a timer in every view).

- `PD_HUD_SLIM_FOES=1`: with two or more other players, only the lock target keeps the full panel; the others are one line each, "PLAYER 3   20 / LV 1" (Health / Level), stacked tighter (`hud_layout::rows_slim`, `SLIM_H`).
- `PD_HUD_SHARED_TIMER=1`: the per-view timers hide and one timer on a dark plate sits at the centre of the screen, across the split (`Bind::SharedTimer`, `SharedTimerBack` on the screen layer).
- Sheets: `engine/shots/declutter/built/ab_14_foe_panels_4p.png`, `ab_15_shared_timer.png` (same tick, `PD_SHOT_TICKS=600`).


**Match timer (2026-10-09, Laurence):** with two or more player views the match clock is drawn once, centred on the split on a dark plate (`Bind::SharedTimer`), instead of in every view; a single full-window view keeps the original's lower-right clock. `PD_HUD_SHARED_TIMER=0` gives the per-view clocks back. Proof (local): `engine/shots/tm_1p.png`, `tm_2p.png`. Test `the_shared_clock_needs_two_views`.
