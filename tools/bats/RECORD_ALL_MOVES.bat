@echo off
rem Record EVERY skill (all 340 roster skills + the Aura Particle) from the ORIGINAL Phantom Dust,
rem unattended, about 90 minutes. Turn the screen saver / screen blanking OFF first (the game has
rem no window while it is on). Double-click to start; double-click again to RESUME after any stop
rem (takes already finished are never redone).
rem
rem It changes only the game's own memory (no keyboard, mouse or pad input), starts the original
rem game itself, watches for stalls (relaunches and carries on), copies your save folder first and
rem puts it back after every relaunch and at the end. Do not play the original meanwhile.
rem Results: %USERPROFILE%\.pdtools\moverec\takes_full\  (summary.csv, run.log, one .mp4 + trace + json per skill).
title Phantom Dust - recording all moves (do not close)
cd /d "%~dp0..\.."
if not exist "%USERPROFILE%\.pdtools\moverec\all_skills.csv" python "%~dp0..\..\engine\re\mr_batch.py" --make-list
echo.
echo ==================================================================
echo  Recording all moves from the original game (about 90 minutes).
echo  Screen saver must be OFF. Please leave the PC alone until it
echo  says DONE. To stop: close this window; run it again to resume.
echo ==================================================================
echo.
python "%~dp0..\..\engine\re\move_recorder.py" --list "%USERPROFILE%\.pdtools\moverec\all_skills.csv" --out "%USERPROFILE%\.pdtools\moverec\takes_full"
echo.
echo Final save check (should say OK):
python "%~dp0..\..\engine\re\save_guard.py" --ref "%USERPROFILE%\.pdtools\original-save-backup-20261001"
echo.
echo Summary: %USERPROFILE%\.pdtools\moverec\takes_full\summary.csv
pause
