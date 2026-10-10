@echo off
rem Play the ORIGINAL Phantom Dust (Store version) while a recorder logs what happens to the
rem fighters around an AURA PARTICLE USE: every program tick each fighter's state number,
rem clip and position, the Aura Particle button press, the skill fire, the Level change,
rem every sound and every capsule capture. Double-click to use.
rem
rem What to do in the game (a versus or practice match):
rem   1. Walk onto an Aura Particle orb to capture it (it goes into a skill slot).
rem   2. HOLD THE LEFT STICK in any direction and, still holding it, press the button of the
rem      Aura Particle slot. Keep the stick held for about 3 seconds.
rem   3. Close this window. Repeat once while standing still if you like.
rem If the fighter keeps running through the glow its position keeps changing and the state
rem number stays the same; if the use holds it, the position stops and the state changes.
rem The recorder only watches; it sends nothing to the game. Closing this window (or
rem Ctrl+C) puts everything back and lets go of the game, which keeps running.
rem The log goes to %USERPROFILE%\.pdtools\trace\aura_use.csv (outside the project).
title Phantom Dust recorder (Aura Particle use) - close this window when you are done
cd /d "%~dp0..\.."
set "SYS=%SystemRoot%\System32"
set tries=0

"%SYS%\tasklist.exe" /fi "imagename eq PDUWP.exe" 2>nul | "%SYS%\find.exe" /i "PDUWP.exe" >nul
if not errorlevel 1 goto ready

echo Starting the original game...
start "" explorer.exe shell:AppsFolder\Microsoft.MSEsper_8wekyb3d8bbwe!App

:waitgame
"%SYS%\timeout.exe" /t 2 /nobreak >nul
"%SYS%\tasklist.exe" /fi "imagename eq PDUWP.exe" 2>nul | "%SYS%\find.exe" /i "PDUWP.exe" >nul
if not errorlevel 1 goto gamegot
set /a tries+=1
if %tries% lss 30 goto waitgame
echo The game did not start. Start it from the Start menu, then run this again.
pause
exit /b 1

:gamegot
echo Game is up. Waiting for it to reach the title...
"%SYS%\timeout.exe" /t 15 /nobreak >nul

:ready
echo.
echo ==================================================================
echo  Capture an Aura Particle, then HOLD THE STICK and press its
echo  button. Keep holding the stick for 3 seconds. Close this window
echo  when done.
echo ==================================================================
echo.
rem 0x14024ec80 = the fighter's per-tick update (rcx = fighter): player, state (+0x12f04),
rem clip (+0x12f00), position x y z (+0x12e40..48).  0x140258180 = skill press (rdx = slot,
rem +0x34 = skill record).  0x1402626d0 = skill fire (edx = record; 0 = Aura Particle).
rem 0x14024d930 = Level add (edx = amount).  0x140276330 = sound.  0x140261a00 = capture.
python "%~dp0..\..\engine\re\live_trace.py" --out "%USERPROFILE%\.pdtools\trace\aura_use.csv" --bp 0x14024ec80:fighter:d[rcx+0x14]*,d[rcx+0x12f04]*,d[rcx+0x12f00],f[rcx+0x12e40],f[rcx+0x12e44],f[rcx+0x12e48] --bp 0x140258180:aura_press:rdx,d[rdx+0x34]*,r8d*,r9d --bp 0x1402626d0:skill_fire:rcx,edx*,r8d --bp 0x14024d930:level_add:rcx,edx* --bp 0x140276330:sound --bp 0x140261a00:capture --summary
echo.
echo Recorder stopped. The log is in %USERPROFILE%\.pdtools\trace\
pause
