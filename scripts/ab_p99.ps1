# Strict Paper vs Chunkwake A/B: p99 MSPT, TPS and raw tick samples.
#
#   powershell -ExecutionPolicy Bypass -File scripts/ab_p99.ps1 -Rounds 5
#
# Every round gets fresh Paper/Chunkwake stands, the same bot command script,
# and three measurement windows: 200 cows, 1000 cows, 1000 cows + redstone.
# PerfProbe records ServerTickEndEvent durations and reports p50/p95/p99 + TPS.
#
# Prerequisites (built by the normal gate):
#   java/dist/chunkwake-api.jar
#   java/perf-probe/build/libs/chunkwake-perf-probe.jar  (gradlew :perf-probe:jar)
#   target/debug/chunkwake.exe                           (cargo build -p chunkwake-server)
#   target/release/examples/opcmd_probe.exe
param(
    [int]$Rounds = 5,
    [int]$Cows = 200,
    [int]$Cows2 = 1000,
    [int]$SampleMs = 15000,
    [string]$PaperJar = $env:PAPER_JAR,
    # Optional low-load mode for shared workstations. 0 = no affinity change.
    [int]$AffinityMask = 0,
    [string]$ProcessPriority = ''
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
if (-not $PaperJar -or -not (Test-Path $PaperJar)) {
    $PaperJar = 'E:\minecraft_server\data\paper-26.3-248.jar'
}
if (-not (Test-Path $PaperJar)) {
    throw "Paper jar not found. Set PAPER_JAR to a Paper 26.3 server jar."
}

$jdk = (Get-ChildItem "$env:USERPROFILE\.gradle\jdks" -Directory |
    Where-Object { $_.Name -like '*25*' } | Select-Object -First 1).FullName
if (-not $jdk) { throw "JDK 25 not found in $env:USERPROFILE\.gradle\jdks" }
$java = Join-Path $jdk 'bin\java.exe'
$bot = Join-Path $root 'target\release\examples\opcmd_probe.exe'
$packets = Join-Path $root 'target\release\generated\26.3\reports\packets.json'
$probe = Join-Path $root 'java\perf-probe\build\libs\chunkwake-perf-probe.jar'
$server = Join-Path $root 'target\release\chunkwake.exe'
$apiJar = Join-Path $root 'java\dist\chunkwake-api.jar'
$ab = Join-Path $root 'target\ab-p99'
$player = 'boater'

foreach ($path in @($java, $bot, $packets, $probe, $server, $apiJar)) {
    if (-not (Test-Path $path)) { throw "missing benchmark dependency: $path" }
}
if ((Get-Item $server).LastWriteTime -lt (Get-Item $apiJar).LastWriteTime) {
    throw "chunkwake.exe is older than java/dist/chunkwake-api.jar; run cargo build --release -p chunkwake-server"
}

# Only touch Java processes that belong to this benchmark. Never kill a random
# server the user may be running.
function Stop-StaleServers {
    Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-CimInstance Win32_Process -Filter "Name='java.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like '*paper-26.3-248.jar*' -or $_.CommandLine -like '*ab-p99*' } |
        ForEach-Object { try { Stop-Process -Id $_.ProcessId -Force } catch { } }
    Start-Sleep -Seconds 2
}

function Reset-Dir([string]$dir) {
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

function Wait-Log([string]$path, [string]$pattern, [int]$seconds) {
    for ($i = 0; $i -lt $seconds * 2; $i++) {
        Start-Sleep -Milliseconds 500
        if (Test-Path $path) {
            $text = Get-Content $path -Raw -ErrorAction SilentlyContinue
            if ($text -match $pattern) { return $true }
        }
    }
    return $false
}

function Add-Cows($lines, [int]$count) {
    for ($i = 0; $i -lt $count; $i++) {
        $a = 2 * [math]::PI * $i / [math]::Max($count, 1)
        $r = 6 + ($i % 12)
        $dx = [math]::Round($r * [math]::Cos($a), 1)
        $dz = [math]::Round($r * [math]::Sin($a), 1)
        $lines.Add("execute at $player run summon minecraft:cow ~$dx ~ ~$dz")
    }
}

# Bot command file. A window starts with /perfprobe reset after the load has
# settled and ends with /perfprobe sample <round>-<scenario>.
function Build-Script([string]$path, [int]$round) {
    $l = New-Object System.Collections.Generic.List[string]
    $l.Add('gamerule spawn_mobs false')
    $l.Add('gamerule max_entity_cramming 0')
    Add-Cows $l $Cows
    $l.Add(('wait {0}' -f ($Cows * 34 + 6000)))
    $l.Add('perfprobe reset')
    $l.Add("wait $SampleMs")
    $l.Add("perfprobe sample r$round-cows$Cows")
    $l.Add('tick query')
    $l.Add('tps')

    Add-Cows $l ($Cows2 - $Cows)
    $l.Add(('wait {0}' -f (($Cows2 - $Cows) * 34 + 8000)))
    $l.Add('perfprobe reset')
    $l.Add("wait $SampleMs")
    $l.Add("perfprobe sample r$round-cows$Cows2")
    $l.Add('tick query')
    $l.Add('tps')

    $l.Add("execute at $player run fill ~4 ~ ~8 ~4 ~ ~72 redstone_wire")
    $l.Add("execute at $player run setblock ~4 ~ ~7 lever[face=floor,facing=south,powered=false]")
    for ($i = 0; $i -lt 20; $i++) {
        $on = if ($i % 2 -eq 0) { 'true' } else { 'false' }
        $l.Add("execute at $player run setblock ~4 ~ ~7 lever[face=floor,facing=south,powered=$on]")
        $l.Add('wait 250')
    }
    $l.Add('wait 2000')
    $l.Add('perfprobe reset')
    $toggles = [int][math]::Ceiling($SampleMs / 280.0)
    for ($i = 0; $i -lt $toggles; $i++) {
        $on = if ($i % 2 -eq 0) { 'true' } else { 'false' }
        $l.Add("execute at $player run setblock ~4 ~ ~7 lever[face=floor,facing=south,powered=$on]")
        $l.Add('wait 250')
    }
    $l.Add("perfprobe sample r$round-redstone")
    $l.Add('tick query')
    $l.Add('tps')
    $l.Add('stop')
    Set-Content -Path $path -Value $l.ToArray() -Encoding ascii
}

function Setup-Stand([string]$dir, [string]$kind, [int]$port) {
    Reset-Dir $dir
    New-Item -ItemType Directory -Force -Path (Join-Path $dir 'plugins') | Out-Null
    Copy-Item $probe (Join-Path $dir 'plugins\PerfProbe.jar') -Force
    $ops = '[{"uuid":"a0ad1a0e-6879-328a-b398-747f79be8dc7","name":"boater","level":4,"bypassesPlayerLimit":true}]'
    Set-Content -Path (Join-Path $dir 'ops.json') -Value $ops -Encoding ascii

    $props = @(
        'server-ip=127.0.0.1',
        ("server-port=" + $port),
        'online-mode=false',
        'enforce-secure-profile=false',
        'level-type=flat',
        'level-seed=1',
        'generate-structures=false',
        'spawn-protection=0',
        'max-players=10',
        'view-distance=8',
        'simulation-distance=8',
        'difficulty=peaceful',
        'allow-nether=false',
        'sync-chunk-writes=false',
        'enable-command-block=false',
        'motd=ab-p99'
    )
    if ($kind -eq 'Chunkwake') {
        $props += ("generated-dir=" + (Join-Path $root 'target\release\generated'))
        $props += 'velocity-enabled=false'
    } else {
        Set-Content -Path (Join-Path $dir 'eula.txt') -Value 'eula=true' -Encoding ascii
        # Paperclip downloads Mojang/Paper jars into cache/versions on first
        # start. A network hiccup must not abort a multi-round benchmark.
        $paperData = 'E:\minecraft_server\data'
        foreach ($cached in @('cache', 'versions')) {
            $source = Join-Path $paperData $cached
            if (Test-Path $source) {
                Copy-Item $source $dir -Recurse -Force
            }
        }
    }
    Set-Content -Path (Join-Path $dir 'server.properties') -Value $props -Encoding ascii
}

function Set-LoadLimit([System.Diagnostics.Process]$p) {
    if ($ProcessPriority -ne '') {
        try { $p.PriorityClass = $ProcessPriority } catch { }
    }
    if ($AffinityMask -gt 0) {
        try { $p.ProcessorAffinity = [IntPtr]$AffinityMask } catch { }
    }
}

function Run-One([string]$kind, [int]$round) {
    $base = Join-Path $ab ("r" + $round)
    $stand = Join-Path $base $kind.ToLowerInvariant()
    $log = Join-Path $base ($kind.ToLowerInvariant() + '.log')
    $botLog = Join-Path $base ('bot-' + $kind.ToLowerInvariant() + '.log')
    $runFile = Join-Path $base ('run-' + $kind.ToLowerInvariant() + '.txt')
    $port = if ($kind -eq 'Paper') { 25611 } else { 25612 }
    Setup-Stand $stand $kind $port
    Build-Script $runFile $round

    if (Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue) {
        throw "port $port is already in use"
    }

    Write-Output ("[{0}] round {1}: starting {2}" -f (Get-Date -Format 'HH:mm:ss'), $round, $kind)
    if ($kind -eq 'Paper') {
        $proc = Start-Process -FilePath $java `
            -ArgumentList ('-Xms2G -Xmx2G -jar "' + $PaperJar + '" --nogui') `
            -WorkingDirectory $stand -RedirectStandardOutput $log `
            -RedirectStandardError ($log + '.err') -WindowStyle Hidden -PassThru
        Set-LoadLimit $proc
        $ready = Wait-Log $log 'Done \(' 240
    } else {
        $env:PATH = (Join-Path $jdk 'bin') + ';' + $env:PATH
        $proc = Start-Process -FilePath $server `
            -ArgumentList '--dir', ('"' + $stand + '"') -RedirectStandardOutput $log `
            -RedirectStandardError ($log + '.err') -WindowStyle Hidden -PassThru
        Set-LoadLimit $proc
        $ready = Wait-Log $log 'listening on' 240
    }
    if (-not $ready) {
        if (-not $proc.HasExited) { try { $proc.Kill() } catch { } }
        throw ("{0} round {1} did not start; see {2}" -f $kind, $round, $log)
    }
    Start-Sleep -Seconds 3

    $env:CMD_FILE = $runFile
    $env:GAP_MS = '30'
    $env:HOLD_MS = '0'
    $bp = Start-Process -FilePath $bot `
        -ArgumentList "127.0.0.1:$port", $packets, $player, 'list' `
        -RedirectStandardOutput $botLog -RedirectStandardError ($botLog + '.err') `
        -WindowStyle Hidden -PassThru
    Remove-Item Env:CMD_FILE, Env:GAP_MS, Env:HOLD_MS -ErrorAction SilentlyContinue

    for ($i = 0; $i -lt 600; $i++) {
        if ($bp.HasExited) { break }
        Start-Sleep -Seconds 1
    }
    for ($i = 0; $i -lt 90; $i++) {
        if ($proc.HasExited) { break }
        Start-Sleep -Seconds 1
    }
    if (-not $proc.HasExited) { try { $proc.Kill() } catch { } }
    try { Stop-Process -Id $bp.Id -Force } catch { }
    Start-Sleep -Seconds 2

    $samples = Join-Path $base ($kind.ToLowerInvariant() + '-samples')
    New-Item -ItemType Directory -Force -Path $samples | Out-Null
    Get-ChildItem (Join-Path $stand 'plugins\PerfProbe') -Filter 'samples-*.csv' -ErrorAction SilentlyContinue |
        Copy-Item -Destination $samples -Force

    $perf = @()
    if (Test-Path $log) {
        $perf = @(Select-String -Path $log -Pattern '\[PerfProbe\] PERFPROBE SAMPLE' |
            ForEach-Object { $_.Line.Trim() })
    }
    Write-Output ("[{0}] {1} round {2}: {3} sample line(s)" -f (Get-Date -Format 'HH:mm:ss'), $kind, $round, $perf.Count)
    foreach ($line in $perf) { Write-Output ("  " + $line) }
    if ($perf.Count -ne 3) {
        Write-Output ("WARNING: {0} round {1} expected 3 windows, got {2}" -f $kind, $round, $perf.Count)
    }

    $botTail = @()
    if (Test-Path $botLog) {
        $botTail = @(Get-Content $botLog -Encoding utf8 -Tail 2)
    }
    foreach ($line in $botTail) { Write-Output ("  bot: " + $line.Trim()) }
    Stop-StaleServers
}

Reset-Dir $ab
Stop-StaleServers
$started = Get-Date
foreach ($round in 1..$Rounds) {
    Write-Output ("=== round {0}/{1} ===" -f $round, $Rounds)
    if ($round % 2 -eq 1) {
        Run-One 'Chunkwake' $round
        Run-One 'Paper' $round
    } else {
        Run-One 'Paper' $round
        Run-One 'Chunkwake' $round
    }
}
Write-Output ("done in {0:N1} min" -f ((Get-Date) - $started).TotalMinutes)
Write-Output ("raw logs and samples: {0}" -f $ab)
