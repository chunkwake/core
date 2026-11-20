# A/B: chunk-tick vs 8x8-region-tick.
#
# Запускает сервер с `CHUNKWAKE_TICK_REGIONS=0|1` и одним и тем же
# нагрузочным стендом (`region_load`: боты + NoAI-свиньи), снимает MSPT
# через `/tick query` и пишет по одному прогону на строку.
#
# Пары чередуются, порядок сбалансирован: нечётные раунды chunks -> regions,
# чётные regions -> chunks, чтобы дрейф машины не складывался в пользу
# одного режима.
#
# Пример:
#   powershell -ExecutionPolicy Bypass -File scripts/ab-regions.ps1 `
#       -ServerDir C:\...\load-gate-synth -Rounds 5 -Mobs 100 -Bots 2 -Seconds 30
param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path,
    [Parameter(Mandatory = $true)][string]$ServerDir,
    [string]$Out = (Join-Path $env:TEMP 'chunkwake-ab-regions'),
    [int]$Rounds = 5,
    [int]$Mobs = 100,
    [int]$Bots = 2,
    [int]$Seconds = 30,
    [int]$Port = 25598,
    [string]$ServerMask = '0x0000FFFF',
    [string]$BotMask = '0xFFFF0000'
)

$ErrorActionPreference = 'Continue'
$inv = [System.Globalization.CultureInfo]::InvariantCulture
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$res = Join-Path $Out 'results.txt'
Set-Content -Path $res -Value ('# round order mode p50_ms p95_ms mean_ms max_ms samples') -Encoding utf8
$serverAffinity = [IntPtr]([Convert]::ToInt64($ServerMask, 16))
$botAffinity = [IntPtr]([Convert]::ToInt64($BotMask, 16))
$serverExe = Join-Path $Root 'target\release\chunkwake.exe'
$loadExe = Join-Path $Root 'target\release\examples\region_load.exe'
$packets = Join-Path $Root 'target\release\generated\26.3\reports\packets.json'
$registries = Join-Path $Root 'target\release\generated\26.3\reports\registries.json'

function PortOpen([int]$p) {
    $c = New-Object System.Net.Sockets.TcpClient
    try {
        $iar = $c.BeginConnect('127.0.0.1', $p, $null, $null)
        if (-not $iar.AsyncWaitHandle.WaitOne(400)) { return $false }
        $c.EndConnect($iar)
        return $true
    } catch { return $false } finally { $c.Close() }
}

function Percentile($sorted, [double]$q) {
    if ($sorted.Count -eq 0) { return 0.0 }
    $idx = [math]::Ceiling($q * $sorted.Count) - 1
    if ($idx -lt 0) { $idx = 0 }
    if ($idx -ge $sorted.Count) { $idx = $sorted.Count - 1 }
    return [double]$sorted[$idx]
}

function RunOnce([string]$mode, [string]$envValue, [int]$round, [int]$order) {
    Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-Process region_load -ErrorAction SilentlyContinue | Stop-Process -Force
    for ($i = 0; $i -lt 40 -and (PortOpen $Port); $i++) { Start-Sleep -Milliseconds 500 }
    Start-Sleep -Seconds 2
    $log = Join-Path $Out ("srv_" + $mode + "_" + $round + ".log")
    $err = Join-Path $Out ("srv_" + $mode + "_" + $round + ".err")
    $llog = Join-Path $Out ("load_" + $mode + "_" + $round + ".log")
    $lerr = Join-Path $Out ("load_" + $mode + "_" + $round + ".err")
    Remove-Item $log, $err, $llog, $lerr -ErrorAction SilentlyContinue
    $env:CHUNKWAKE_TICK_REGIONS = $envValue
    $p = Start-Process -FilePath $serverExe -ArgumentList '--dir', ('"' + $ServerDir + '"') `
        -RedirectStandardOutput $log -RedirectStandardError $err -WindowStyle Hidden -PassThru
    Remove-Item Env:CHUNKWAKE_TICK_REGIONS
    try { $p.ProcessorAffinity = $serverAffinity } catch { }
    $ready = $false
    for ($i = 0; $i -lt 90; $i++) {
        Start-Sleep -Seconds 1
        if (PortOpen $Port) { $ready = $true; break }
        if ($p.HasExited) { break }
    }
    if (-not $ready) {
        Add-Content -Path $res -Value ("{0} {1} {2} FAIL" -f $round, $order, $mode) -Encoding utf8
        Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
        return
    }
    Start-Sleep -Seconds 2
    $env:MOBS = [string]$Mobs
    $env:CLEAN = '1'
    $b = Start-Process -FilePath $loadExe -ArgumentList '127.0.0.1:' + $Port,
        $packets, $registries, [string]$Bots, [string]$Seconds `
        -RedirectStandardOutput $llog -RedirectStandardError $lerr -WindowStyle Hidden -PassThru
    Remove-Item Env:MOBS
    Remove-Item Env:CLEAN
    try { $b.ProcessorAffinity = $botAffinity } catch { }
    Wait-Process -Id $b.Id -Timeout (($Seconds + 60) * 4) -ErrorAction SilentlyContinue
    if (-not $b.HasExited) { Stop-Process -Id $b.Id -Force -ErrorAction SilentlyContinue }

    $vals = @()
    Get-Content $llog -ErrorAction SilentlyContinue | ForEach-Object {
        if ($_ -match '^MSPT ([0-9.]+)') { $vals += [double]::Parse($Matches[1], $inv) }
    }
    if ($vals.Count -gt 0) {
        $sorted = $vals | Sort-Object
        $mean = ($vals | Measure-Object -Average).Average
        $p50 = Percentile $sorted 0.50
        $p95 = Percentile $sorted 0.95
        $max = ($vals | Measure-Object -Maximum).Maximum
        $line = "{0} {1} {2} {3} {4} {5} {6} {7}" -f $round, $order, $mode,
            $p50.ToString('F3', $inv), $p95.ToString('F3', $inv),
            $mean.ToString('F3', $inv), $max.ToString('F3', $inv), $vals.Count
        Add-Content -Path $res -Value $line -Encoding utf8
        Write-Output $line
    } else {
        Add-Content -Path $res -Value ("{0} {1} {2} NOSAMPLES" -f $round, $order, $mode) -Encoding utf8
        Write-Output ("{0} round={1} NO SAMPLES" -f $mode, $round)
    }
    Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-Process region_load -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 3
}

for ($round = 1; $round -le $Rounds; $round++) {
    if ($round % 2 -eq 1) {
        RunOnce 'chunks' '0' $round 1
        RunOnce 'regions' '1' $round 2
    } else {
        RunOnce 'regions' '1' $round 1
        RunOnce 'chunks' '0' $round 2
    }
}
Write-Output "--- $res ---"
Get-Content $res -Encoding utf8
Write-Output "Сводку по парам: py -3 scripts/ab-regions-stats.py $Out"
