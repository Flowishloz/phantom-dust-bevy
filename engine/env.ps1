# Dot-source before building:  . .\env.ps1
# Puts the portable MinGW and Rust on PATH and moves to the engine folder.
$env:Path = "C:\Users\laure\.pdtools\mingw64\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location $PSScriptRoot
