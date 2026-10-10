# Verification

"Done" means proven, at the level the change deserves.

| Change | Minimum proof |
|---|---|
| Format parser change | `pd_convert scan ..\game_files\Assets\Data` still reports `831 converted, 0 failed`, plus the specific file's probe output |
| Stage render | `pd_convert model ...` then a viewer or game screenshot, looked at |
| Character / animation | viewer screenshots of at least WAIT0, RUN, one attack, one hit reaction (montage) |
| Sim / gameplay | `pd_game --demo --screenshot` at two different frames; for feel (speed, jump, flicker) ask Laurence to play |
| Camera | two frames + Laurence plays (flicker does not show in stills) |

## Screenshot tools

- Viewer: `.\shot.ps1 <name> <char path without .glb, e.g. chars/pc00a> --close <dist> --angle <rad> --anim <NAME> --anim-time <s>` → `shots/<name>.png` (stage st06 in the background). `--angle 1.57` ≈ front view.
- Game: `.\target\debug\pd_game.exe "$PWD\cache" --demo --screenshot "$PWD\shots\x.png" --shot-frame 300`.
- Montage several PNGs into one image with PowerShell `System.Drawing` (see any recent session; draw labels with `DrawString`). One montage costs far fewer tokens to inspect than several full images.

## A–F calibration sheets (use for any ambiguous visual)

When the data can't tell you which of several interpretations is right (rotation order, sign conventions, UV flips, colour channel order):

1. Implement the interpretations behind a switch (e.g. `--order c:012:0`).
2. Render each with the **same camera and frame**, 2 contrasting poses/views.
3. Montage into a labelled grid (A, B, C…), max 6 per sheet.
4. Send to Laurence with a multi-select panel ("which look right? note mixed cases like 'C legs, E arms'").
5. Make the winner the default, document it with the date, delete the losers' cache files.

Laurence knows the original game well; one sheet beats rounds of guessing. Don't ship a visual guess as fact.

## Automatic checks worth keeping

- `pd_convert eulertest` — bend-direction sanity (only narrows the candidates, see FORMATS-ANIMATION.md).
- Audio sanity: lag-1 autocorrelation of decoded samples > 0.9 means real audio, ≈ 0 means noise.
