# Render one screenshot with pd_viewer.
#   ./shot.ps1 <out name> <character glb name> [extra viewer args...]
param([string]$Name, [string]$Character, [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest)
$root = $PSScriptRoot
New-Item -ItemType Directory -Force "$root\shots" | Out-Null
$argList = @("`"$root\cache`"", 'st06', $Character, '--screenshot', "`"$root\shots\$Name.png`"") + $Rest
$p = Start-Process "$root\target\debug\pd_viewer.exe" -ArgumentList $argList -PassThru -Wait -RedirectStandardError "$root\shots\$Name.log"
Get-Content "$root\shots\$Name.log" | Select-String 'ERROR|WARN|panicked' | Select-Object -First 8
"exit $($p.ExitCode) -> shots\$Name.png"
