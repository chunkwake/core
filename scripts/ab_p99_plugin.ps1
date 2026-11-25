# Plugin-driven Paper vs Chunkwake A/B (no opcmd_probe load commands).
# The ChunkwakeTest plugin spawns 200/1000 cows and toggles redstone, while an
# idle opcmd_probe player keeps the spawn area simulated.
param(
    [int]$Rounds = 5,
    [int]$AffinityMask = 15,
    [string]$ProcessPriority = 'BelowNormal'
)
$ErrorActionPreference = 'Continue'
$root = 'E:\paper_rust'
$paperJar = 'E:\minecraft_server\data\paper-26.3-248.jar'
$jdk = (Get-ChildItem "$env:USERPROFILE\.gradle\jdks" -Directory |
    Where-Object { $_.Name -like '*25*' } | Select-Object -First 1).FullName
$java = Join-Path $jdk 'bin\java.exe'
$server = Join-Path $root 'target\final-checks\chunkwake-270674c.exe'
$bot = Join-Path $root 'target\release\examples\opcmd_probe.exe'
$packets = Join-Path $root 'target\release\generated\26.3\reports\packets.json'
$probe = Join-Path $root 'java\perf-probe\build\libs\chunkwake-perf-probe.jar'
$testPlugin = Join-Path $root 'java\test-plugin\build\libs\chunkwake-test-plugin.jar'
$ab = Join-Path $root 'target\ab-p99-plugin'
$player = 'boater'

foreach ($path in @($java, $server, $bot, $packets, $probe, $testPlugin)) {
    if (-not (Test-Path $path)) { throw "missing: $path" }
}


function Stop-Stale {
    Get-Process chunkwake-270674c,opcmd_probe -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-CimInstance Win32_Process -Filter "Name='java.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like '*paper-26.3-248.jar*' -or $_.CommandLine -like '*ab-p99-plugin*' } |
        ForEach-Object { try { Stop-Process -Id $_.ProcessId -Force } catch { } }
    Start-Sleep -Seconds 2
}

function Reset-Dir([string]$dir) {
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

function Set-Limit([System.Diagnostics.Process]$p) {
    if ($ProcessPriority -ne '') { try { $p.PriorityClass = $ProcessPriority } catch { } }
    if ($AffinityMask -gt 0) { try { $p.ProcessorAffinity = [IntPtr]$AffinityMask } catch { } }
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

function Setup-Stand([string]$dir, [string]$kind, [int]$port) {
    Reset-Dir $dir
    New-Item -ItemType Directory -Force -Path (Join-Path $dir 'plugins') | Out-Null
    Copy-Item $probe (Join-Path $dir 'plugins\PerfProbe.jar') -Force
    Copy-Item $testPlugin (Join-Path $dir 'plugins\chunkwake-test-plugin.jar') -Force
    $ops = '[{"uuid":"a0ad1a0e-6879-328a-b398-747f79be8dc7","name":"boater","level":4,"bypassesPlayerLimit":true}]'
    Set-Content (Join-Path $dir 'ops.json') $ops -Encoding ascii
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
        'motd=ab-p99-plugin'
    )
    if ($kind -eq 'Chunkwake') {
        $props += ("generated-dir=" + (Join-Path $root 'target\release\generated'))
        $props += 'velocity-enabled=false'
    } else {
        Set-Content (Join-Path $dir 'eula.txt') 'eula=true' -Encoding ascii
        foreach ($cached in @('cache', 'versions')) {
            $source = Join-Path 'E:\minecraft_server\data' $cached
            if (Test-Path $source) { Copy-Item $source $dir -Recurse -Force }
        }
    }
    Set-Content (Join-Path $dir 'server.properties') $props -Encoding ascii
}

function Run-One([string]$kind, [int]$round) {
    $base = Join-Path $ab ("r" + $round)
    $stand = Join-Path $base $kind.ToLowerInvariant()
    $log = Join-Path $base ($kind.ToLowerInvariant() + '.log')
    $botLog = Join-Path $base ('bot-' + $kind.ToLowerInvariant() + '.log')
    $hold = Join-Path $base ('hold-' + $kind.ToLowerInvariant() + '.txt')
    $port = if ($kind -eq 'Paper') { 25611 } else { 25612 }
    Setup-Stand $stand $kind $port
    Set-Content $hold 'wait 120000' -Encoding ascii

    Write-Output ("[{0}] round {1}: starting {2}" -f (Get-Date -Format 'HH:mm:ss'), $round, $kind)
    $env:CHUNKWAKE_AB_LOAD = [string]$round
    if ($kind -eq 'Paper') {
        $proc = Start-Process -FilePath $java `
            -ArgumentList ('-Xms2G -Xmx2G -jar "' + $paperJar + '" --nogui') `
            -WorkingDirectory $stand -RedirectStandardOutput $log `
            -RedirectStandardError ($log + '.err') -WindowStyle Hidden -PassThru
        Set-Limit $proc
        $ready = Wait-Log $log 'Done \(' 240
    } else {
        $env:PATH = (Join-Path $jdk 'bin') + ';' + $env:PATH
        $proc = Start-Process -FilePath $server `
            -ArgumentList '--dir', ('"' + $stand + '"') -RedirectStandardOutput $log `
            -RedirectStandardError ($log + '.err') -WindowStyle Hidden -PassThru
        Set-Limit $proc
        $ready = Wait-Log $log 'listening on' 240
    }
    Remove-Item Env:CHUNKWAKE_AB_LOAD -ErrorAction SilentlyContinue
    if (-not $ready) {
        if (-not $proc.HasExited) { try { $proc.Kill() } catch { } }
        throw ("{0} round {1} did not start" -f $kind, $round)
    }
    Start-Sleep -Seconds 3

    $env:CMD_FILE = $hold
    $env:GAP_MS = '30'
    $env:HOLD_MS = '0'
    $bp = Start-Process -FilePath $bot `
        -ArgumentList "127.0.0.1:$port", $packets, $player, 'list' `
        -RedirectStandardOutput $botLog -RedirectStandardError ($botLog + '.err') `
        -WindowStyle Hidden -PassThru
    Remove-Item Env:CMD_FILE, Env:GAP_MS, Env:HOLD_MS -ErrorAction SilentlyContinue

    $ok = Wait-Log $log ("PERFPROBE SAMPLE label=r" + $round + "-redstone") 240
    if (-not $ok) {
        Write-Output ("WARNING: {0} round {1}: no redstone sample" -f $kind, $round)
    }
    if (-not $bp.HasExited) { try { Stop-Process -Id $bp.Id -Force } catch { } }
    if (-not $proc.HasExited) { try { $proc.Kill() } catch { } }
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
    Stop-Stale
}

Reset-Dir $ab
Stop-Stale
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
Write-Output ("raw: {0}" -f $ab)
