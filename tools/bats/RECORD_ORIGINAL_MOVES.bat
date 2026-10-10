@echo off
rem Record controlled reference footage of skills from the ORIGINAL Phantom Dust (Store version).
rem Double-click for the five pilot takes (Laser, Psycho Knife, Blaster, Speed Up, Aura Particle
rem while the stick is held). To record your own list, drag a skill-list file (CSV with an "id"
rem column) onto this file.
rem
rem What it does (all by changing the game's own memory; it sends NO keyboard, mouse or pad input):
rem   * starts the original game if it is not running (and closes THAT copy when it is done);
rem   * loads your profile, starts a one-on-one versus match, puts each skill in a slot,
rem     lines the two fighters up, presses the buttons from a script and films the game window;
rem   * writes <id>_<name>.mp4 + <id>_<name>_trace.csv + <id>_<name>.json per skill into
rem     %USERPROFILE%\.pdtools\moverec\takes\  (outside the project, never committed);
rem   * copies your save folder first and puts it back afterwards if the game changed it.
rem Do not play the original at the same time. Leave the PC alone while it runs (a few minutes).
title Phantom Dust move recorder - do not close until it says done
cd /d "%~dp0..\.."
set "SYS=%SystemRoot%\System32"
echo.
echo ==================================================================
echo  Recording original-game moves. This opens the game on your screen.
echo  Please do not touch the PC until this window says it is done.
echo ==================================================================
echo.
if "%~1"=="" (
  python "%~dp0..\..\engine\re\move_recorder.py" --pilot
) else (
  python "%~dp0..\..\engine\re\move_recorder.py" --list "%~1"
)
echo.
echo Takes are in %USERPROFILE%\.pdtools\moverec\takes\
echo Save check (should say OK):
python "%~dp0..\..\engine\re\save_guard.py" --ref "%USERPROFILE%\.pdtools\original-save-backup-20261001"
echo.
pause
