# Dot-source before building:  . .\env.ps1
# Puts a portable MinGW-w64 and Rust on PATH and moves to the engine folder.
# MinGW lives outside the project because its linker breaks on paths with spaces:
# default %USERPROFILE%\.pdtools\mingw64 (override with $env:PD_MINGW). See docs\TOOLCHAIN.md.
$mingw = if ($env:PD_MINGW) { $env:PD_MINGW } else { "$env:USERPROFILE\.pdtools\mingw64" }
$env:Path = "$mingw\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location $PSScriptRoot
