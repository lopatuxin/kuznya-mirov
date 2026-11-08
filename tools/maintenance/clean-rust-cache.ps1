[CmdletBinding()]
param(
    [string]$ProjectRoot = (Join-Path $PSScriptRoot '..\..'),
    [ValidateRange(1, [long]::MaxValue)][long]$MaxBytes = 8GB,
    [ValidateRange(1, 3650)][int]$MaxAgeDays = 7,
    [ValidateRange(0, 1440)][int]$MinimumIdleMinutes = 30,
    [string]$StatusPath,
    [switch]$Apply
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $ProjectRoot).ProviderPath.TrimEnd('\')
$report = [ordered]@{ Time = (Get-Date).ToString('o'); Status = 'ok'; BytesBefore = 0L; BytesAfter = 0L; Cleaned = @(); Skipped = @() }

function Test-BuildRunning {
    return @(Get-Process -Name cargo,rustc,rustdoc,wasm-pack,rust-lld -ErrorAction SilentlyContinue).Count -gt 0
}

function Assert-ContainedPath([string]$Path) {
    $resolved = (Resolve-Path -LiteralPath $Path).ProviderPath.TrimEnd('\')
    if (-not $resolved.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path is outside project: $resolved"
    }
    $entry = Get-Item -LiteralPath $resolved -Force
    while ($null -ne $entry) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Linked path is not eligible for cleanup: $($entry.FullName)"
        }
        if ($entry.PSIsContainer) { $entry = $entry.Parent }
        else { $entry = $entry.Directory }
    }
}

function Get-ProfileFiles([string]$Path) {
    Assert-ContainedPath $Path
    $entries = @(Get-ChildItem -LiteralPath $Path -Force -Recurse)
    if (@($entries | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }).Count -gt 0) {
        throw "Linked content is not eligible for cleanup: $Path"
    }
    return @($entries | Where-Object { -not $_.PSIsContainer -and $_.Name -notmatch '^\.cargo-.*lock$' })
}

try {
    if (Test-BuildRunning) {
        $report.Status = 'build-running'
        return
    }
    $checkouts = @($root)
    $worktrees = Join-Path $root '.claude\worktrees'
    if (Test-Path -LiteralPath $worktrees) {
        Assert-ContainedPath $worktrees
        $checkouts += @(Get-ChildItem -LiteralPath $worktrees -Directory -Force | ForEach-Object { $_.FullName })
    }
    $profiles = @(foreach ($checkout in $checkouts) {
        foreach ($crate in @('engine', 'importer', 'luars', 'cosmic-text')) {
            $crateRoot = Join-Path $checkout $crate
            $target = Join-Path $crateRoot 'target'
            if (-not (Test-Path -LiteralPath $target) -or -not (Test-Path -LiteralPath (Join-Path $crateRoot 'Cargo.toml'))) { continue }
            Assert-ContainedPath $target
            $entries = @(Get-ChildItem -LiteralPath $target -Force -Recurse)
            if (@($entries | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }).Count -gt 0) {
                throw "Linked content is not eligible for cleanup: $target"
            }
            foreach ($lock in @($entries | Where-Object { -not $_.PSIsContainer -and $_.Name -eq '.cargo-lock' })) {
                if ($lock.Directory.Parent.FullName -ne $target -and $lock.Directory.Parent.Parent.FullName -ne $target) { continue }
                $files = @(Get-ProfileFiles $lock.DirectoryName)
                if ($files.Count -eq 0) { continue }
                [pscustomobject]@{
                    Path = $lock.DirectoryName
                    Lock = $lock.FullName
                    Bytes = [long](($files | Measure-Object Length -Sum).Sum)
                    Latest = ($files | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc
                }
            }
        }
    })
    $total = [long](($profiles | Measure-Object Bytes -Sum).Sum)
    $report.BytesBefore = $total
    $report.BytesAfter = $total
    foreach ($profile in ($profiles | Sort-Object Latest)) {
        if ($total -le $MaxBytes -and $profile.Latest -gt [DateTime]::UtcNow.AddDays(-$MaxAgeDays)) { continue }
        if ($profile.Latest -gt [DateTime]::UtcNow.AddMinutes(-$MinimumIdleMinutes)) {
            $report.Skipped += $profile.Path
            continue
        }
        if (-not $Apply) {
            $report.Cleaned += $profile.Path
            $total -= $profile.Bytes
            continue
        }
        if (Test-BuildRunning) { $report.Status = 'build-running'; break }
        $handle = $null
        $locked = $false
        try {
            Assert-ContainedPath $profile.Lock
            $handle = [IO.File]::Open($profile.Lock, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
            try {
                $handle.Lock(0, [long]::MaxValue)
                $locked = $true
            } catch [IO.IOException] {
                $report.Skipped += $profile.Path
                continue
            }
            # Cargo 1.96+ also takes this compatibility lock; keep the inode and all lock files in place.
            if (Test-BuildRunning) { $report.Status = 'build-running'; break }
            $files = @(Get-ProfileFiles $profile.Path)
            if (@($files | Where-Object { $_.LastWriteTimeUtc -gt [DateTime]::UtcNow.AddMinutes(-$MinimumIdleMinutes) }).Count -gt 0) {
                $report.Skipped += $profile.Path
                continue
            }
            foreach ($entry in @(Get-ChildItem -LiteralPath $profile.Path -Force)) {
                if ($entry.Name -match '^\.cargo-.*lock$') { continue }
                Assert-ContainedPath $entry.FullName
                Remove-Item -LiteralPath $entry.FullName -Recurse -Force
            }
            $total -= $profile.Bytes
            $report.BytesAfter = $total
            $report.Cleaned += $profile.Path
        } finally {
            if ($locked) { $handle.Unlock(0, [long]::MaxValue) }
            if ($null -ne $handle) { $handle.Dispose() }
        }
    }
    if (-not $Apply) { $report.Status = 'preview'; $report.BytesAfter = $total }
    elseif ($total -gt $MaxBytes -and $report.Status -eq 'ok') { $report.Status = 'deferred' }
} catch {
    $report.Status = 'error'
    $report.Error = $_.Exception.Message
    throw
} finally {
    $json = $report | ConvertTo-Json -Depth 4
    if ($StatusPath) { $json | Set-Content -LiteralPath $StatusPath -Encoding UTF8 }
    Write-Output $json
}
