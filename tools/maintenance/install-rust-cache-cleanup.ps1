$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).ProviderPath
$script = Join-Path $PSScriptRoot 'clean-rust-cache.ps1'
$status = Join-Path $root '.cargo\cache-cleanup-status.json'
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}" -ProjectRoot "{1}" -StatusPath "{2}" -Apply' -f $script, $root, $status
$action = New-ScheduledTaskAction -Execute "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument $arguments -WorkingDirectory $root
$hourly = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Hours 1)
$logon = New-ScheduledTaskTrigger -AtLogOn -User $user
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 15) -Hidden
Register-ScheduledTask -TaskName 'KuznyaMirov-RustCacheCleanup' -Action $action -Trigger @($hourly, $logon) -Principal $principal -Settings $settings -Description 'Keep Kuznya Mirov Rust build caches below 8 GiB when idle; clean profiles unchanged for 7 days. Never clean during a build.' -Force | Select-Object TaskName,State
