#!/bin/bash
# Regenerate the whole converted-asset cache with this checkout's converter
# (Git Bash). Writes into a NEW folder (default engine/cache.new, git-ignored);
# when it finishes, close the game and swap it in for engine/cache:
#   mv engine/cache engine/cache.old && mv engine/cache.new engine/cache
#
# Usage (from anywhere):  bash engine/regen_cache.sh [out_dir]
# MinGW: $PD_MINGW, default %USERPROFILE%\.pdtools\mingw64 (docs/TOOLCHAIN.md).
# Game data: <repo>/game_files (your own unlocked copy; never committed).
#
# Covers every converter command that writes into the cache as of 2026-10-02,
# including the ones PIPELINE.md's list predates (skyscroll, capsules,
# shotpoints, cpu) and musictracks (2026-10-07). settings.json is not converted: copy it from the old cache
# if you want to keep your saved split/FOV.
set -e
ENGINE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$ENGINE")"
OUT="${1:-$ENGINE/cache.new}"
MINGW="${PD_MINGW:-$(cygpath -m "$USERPROFILE")/.pdtools/mingw64}"
export PATH="$(cygpath -u "$MINGW")/bin:$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$MINGW/bin/gcc.exe"
export RUSTFLAGS="-C dlltool=$MINGW/bin/dlltool.exe"

cd "$ENGINE"
cargo build --release -p pd_convert
c="$ENGINE/target/release/pd_convert.exe"
D="$REPO/game_files/Assets/Data"
[ -d "$D" ] || { echo "No game data at $D (put your unlocked game in <repo>/game_files)"; exit 1; }
mkdir -p "$OUT"
"$c" stages "$D/Map" "$OUT"
"$c" skyscroll "$D/Map" "$OUT"
"$c" music "$D/BGM/04_Highway.stx" "$OUT/music.wav"
"$c" musictracks "$D/BGM" "$OUT/music"
"$c" sounds "$D/Sound" "$OUT/sfx"
"$c" skills "$D/gstorage/gsdata_en.dat" "$OUT/skills.json" --decks "$D/setup/deckdata_debug"
"$c" effects "$D/Effect" "$OUT/fx"
"$c" stagefx "$D/Map" "$OUT"
"$c" capsules "$D/Map/st00.alr" "$D/Map/st00.dat" "$OUT/fx/capsules"
"$c" hudtex "$D/Menu/BtMenuEn.alr" "$OUT/ui"
cp /c/Windows/Fonts/bahnschrift.ttf "$OUT/ui/hud.ttf"
"$c" hudtex "$D/Menu/DckMenu.alr" "$OUT/ui/dck"
"$c" menus "$D" "$OUT/ui/menu"
"$c" chars "$D" "$OUT"
mkdir -p "$OUT/chars"
for d in "$D"/Player/*/; do n=$(basename "$d"); [ -f "$d/$n.alr" ] && "$c" character "$d/$n.alr" "$OUT/chars/$n.glb"; done
"$c" shotpoints "$D" "$OUT"
"$c" cpu "$D" "$OUT"
echo "Cache written to $OUT"
