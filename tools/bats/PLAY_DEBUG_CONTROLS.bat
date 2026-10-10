@echo off
rem Same as PLAY.bat, but records what the game receives from keyboard and controller,
rem plus P1's walk/run/strafe state and the animation playing each tick,
rem into engine\shots\input_log.txt (for fixing control problems). Close the game when done.
set PD_INPUT_LOG=1
set PD_MOVE_LOG=1
"%~dp0..\..\engine\play\pd_game.exe" "%~dp0..\..\engine\cache" > "%~dp0..\..\engine\shots\input_log.txt" 2>&1
