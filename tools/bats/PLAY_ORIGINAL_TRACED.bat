@echo off
rem Play the ORIGINAL Phantom Dust (Store version) while a recorder logs every sound it
rem plays, every capsule capture and every capsule erase. Double-click to use.
rem The recorder only watches; it sends nothing to the game. Closing this window (or
rem Ctrl+C) puts everything back and lets go of the game, which keeps running.
rem Log files go to %USERPROFILE%\.pdtools\trace\ (outside the project).
title Phantom Dust recorder - close this window when you are done
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
echo  Play normally; capture a capsule, use one, erase one;
echo  close this window when done.
echo ==================================================================
echo.
python "%~dp0..\..\engine\re\live_trace.py" --bp 0x140276330:sound --bp 0x140261a00:capture --bp 0x14014fd10:erase --summary
echo.
echo Recorder stopped. The log is in %USERPROFILE%\.pdtools\trace\
pause
