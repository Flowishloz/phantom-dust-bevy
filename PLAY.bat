@echo off
rem Phantom Dust Rewrite -- one launcher for everything. Double-click, then press a number.
rem The extra launchers it calls live in tools\bats\ (they also work on their own).
title Phantom Dust Rewrite
cd /d "%~dp0"
set "GAME=%~dp0engine\play\pd_game.exe"
set "CACHE=%~dp0engine\cache"
if not exist "%GAME%" (
  echo The game is not built yet. Ask Claude to build it, then run this again.
  pause
  exit /b 1
)

:menu
cls
echo.
echo   PHANTOM DUST REWRITE
echo   ====================
echo.
echo   1  Play                  title menu: Versus (add CPUs with Y on an empty seat),
echo                            Online, Arsenal builder, Options
echo   2  Quick test match      straight into a match, one skill of every family each
echo   3  Play + control log    like 1, and records keyboard/controller input for bug reports
echo.
echo   4  Original game + recorder   play the Store original while sounds/captures are logged
echo   5  Record original moves      film every skill from the original (about 90 min, resumes)
echo.
echo   Q  Quit
echo.
echo   Controls -- P1: WASD move, Space jump, skills J K U I, Q/E turn camera, Enter/Esc in menus.
echo               Controller: left stick move (click to jump), X B Y A skills, right trigger lock-on.
echo.
choice /c 12345Q /n /m "  Choose: "
if errorlevel 6 exit /b 0
if errorlevel 5 (call "%~dp0tools\bats\RECORD_ALL_MOVES.bat" & goto menu)
if errorlevel 4 (call "%~dp0tools\bats\PLAY_ORIGINAL_TRACED.bat" & goto menu)
if errorlevel 3 (call "%~dp0tools\bats\PLAY_DEBUG_CONTROLS.bat" & goto menu)
if errorlevel 2 (start "" "%GAME%" "%CACHE%" --arsenal mixed & exit /b 0)
start "" "%GAME%" "%CACHE%"
exit /b 0
