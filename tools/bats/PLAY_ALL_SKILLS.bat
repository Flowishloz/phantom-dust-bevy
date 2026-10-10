@echo off
rem Same as PLAY.bat, but skips the menus and goes straight into a match
rem where both players' arsenals hold one skill of every family
rem (short-range, laser, shell, bomb, hole, rain, mine, wave, crawler, meteor,
rem barrier, shelter, reflect, course, erase, status, special, environment) plus 10 aura particles.
start "" "%~dp0..\..\engine\play\pd_game.exe" "%~dp0..\..\engine\cache" --arsenal mixed
