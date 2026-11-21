# A/B: Chunkwake vs Paper 26.3 на одинаковой нагрузке (плоский мир, бот в игре).
#
#   powershell -ExecutionPolicy Bypass -File scripts/ab_paper.ps1 [-Cows 200] [-Cows2 1000]
#
# Paper jar: $env:PAPER_JAR или E:\minecraft_server\data\paper-26.3-248.jar.
# Один бот-игрок подаёт всю нагрузку и снимает /mspt (Chunkwake и Paper).
param(
    [int]$Cows = 200,
    [int]$Cows2 = 1000,
    [string]$PaperJar = $env:PAPER_JAR
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
$java = Join-Path $jdk 'bin\java.exe'
$bot = Join-Path $root 'target\release\examples\opcmd_probe.exe'
$packets = Join-Path $root 'target\release\generated\26.3\reports\packets.json'
$ab = Join-Path $root 'target\ab'
$paperDir = Join-Path $ab 'paper'
$cwDir = Join-Path $ab 'chunkwake'
$player = 'boater'

function Reset-Dir([string]$dir) {
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

function Wait-Log([string]$path, [string]$pattern, [int]$seconds) {
    for ($i = 0; $i -lt $seconds; $i++) {
        Start-Sleep -Seconds 1
        if (Test-Path $path) {
            $text = Get-Content $path -Raw -ErrorAction SilentlyContinue
            if ($text -match $pattern) { return $true }
        }
    }
    return $false
}

function Write-Lines([string]$path, $lines) {
    Set-Content -Path $path -Value $lines -Encoding ascii
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

# Один сценарий: N коров, потом N2, потом редстоун; между — паузы и /mspt.
function Build-Script([string]$path) {
    $l = New-Object System.Collections.Generic.List[string]
    $l.Add('gamerule doMobSpawning false')
    $l.Add('gamerule maxEntityCramming 0')
    Add-Cows $l $Cows
    $l.Add('wait 8000')
    $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt')
    $l.Add('execute if entity @e[type=minecraft:cow]')
    Add-Cows $l ($Cows2 - $Cows)
    $l.Add('wait 10000')
    $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt')
    $l.Add('execute if entity @e[type=minecraft:cow]')
    $l.Add("execute at $player run fill ~4 ~ ~8 ~4 ~ ~72 redstone_wire")
    $l.Add("execute at $player run setblock ~4 ~ ~7 lever[face=floor,facing=south,powered=false]")
    for ($i = 0; $i -lt 20; $i++) {
        $on = if ($i % 2 -eq 0) { 'true' } else { 'false' }
        $l.Add("execute at $player run setblock ~4 ~ ~7 lever[face=floor,facing=south,powered=$on]")
        $l.Add('wait 250')
    }
    $l.Add('wait 4000')
    $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt'); $l.Add('wait 2000'); $l.Add('mspt')
    $l.Add('execute if entity @e[type=minecraft:cow]')
    $l.Add('stop')
    Write-Lines $path $l.ToArray()
}

function Parse-Samples([string]$logName) {
    $path = Join-Path $ab $logName
    $out = New-Object System.Collections.Generic.List[double]
    if (-not (Test-Path $path)) { return $out }
    $raw = [System.IO.File]::ReadAllLines($path, [System.Text.Encoding]::UTF8)
    foreach ($line in $raw) {
        if ($line -notmatch '^\s*\|') { continue }
        $direct = [regex]::Match($line, 'MSPT:\s*([0-9]+[.,][0-9]+)')
        if ($direct.Success) {
            $out.Add([double]($direct.Groups[1].Value.Replace(',', '.')))
            continue
        }
        $nums = [regex]::Matches($line, '(?<![\d.])([0-9]+[.,][0-9]+)(?![\d.])')
        if ($nums.Count -ge 6 -and $line -match 'tick|avg|min|max|[,.]') {
            $out.Add([double]($nums[0].Groups[1].Value.Replace(',', '.')))
        }
    }
    return $out
}

function Show-Triple([string]$tag, $values, [int]$offset) {
    $part = @()
    for ($i = $offset; $i -lt [math]::Min($offset + 3, $values.Count); $i++) { $part += $values[$i] }
    if ($part.Count -eq 0) {
        Write-Output ("{0}: no samples" -f $tag)
        return
    }
    $sorted = $part | Sort-Object
    $median = $sorted[[int][math]::Floor($sorted.Count / 2)]
    Write-Output ("{0}: median={1:N2} ms samples=[{2}]" -f $tag, $median,
        (($part | ForEach-Object { $_.ToString('N2') }) -join ', '))
}

function Run-Scenario([string]$name, [int]$port, [string]$logPath, [string]$runFile) {
    Write-Output ("=== {0} ===" -f $name)
    if ($name -eq 'Paper') {
        $proc = Start-Process -FilePath $java -ArgumentList ('-Xms2G -Xmx2G -jar "' + $PaperJar + '" --nogui') `
            -WorkingDirectory $paperDir -RedirectStandardOutput $logPath `
            -RedirectStandardError ($logPath + '.err') -WindowStyle Hidden -PassThru
        $ready = Wait-Log $logPath 'Done \(' 180
    } else {
        $proc = Start-Process -FilePath (Join-Path $root 'target\debug\chunkwake.exe') `
            -ArgumentList '--dir', ('"' + $cwDir + '"') -RedirectStandardOutput $logPath `
            -RedirectStandardError ($logPath + '.err') -WindowStyle Hidden -PassThru
        $ready = Wait-Log $logPath 'listening on' 120
    }
    if (-not $ready) { throw ("{0} did not start (see {1})" -f $name, $logPath) }
    Start-Sleep -Seconds 3

    $env:CMD_FILE = $runFile
    $env:GAP_MS = '30'
    $env:HOLD_MS = '0'
    $botLog = Join-Path $ab ('bot-' + $name + '.log')
    $bp = Start-Process -FilePath $bot -ArgumentList "127.0.0.1:$port", $packets, $player, 'list' `
        -RedirectStandardOutput $botLog -RedirectStandardError ($botLog + '.err') -WindowStyle Hidden -PassThru
    Remove-Item Env:CMD_FILE,Env:GAP_MS,Env:HOLD_MS -ErrorAction SilentlyContinue

    for ($i = 0; $i -lt 300; $i++) {
        if ($bp.HasExited) { break }
        Start-Sleep -Seconds 1
    }
    for ($i = 0; $i -lt 60; $i++) {
        if ($proc.HasExited) { break }
        Start-Sleep -Seconds 1
    }
    if (-not $proc.HasExited) { try { $proc.Kill() } catch { } }
    try { Stop-Process -Id $bp.Id -Force } catch { }

    $counts = @()
    $botPath = Join-Path $ab ('bot-' + $name + '.log')
    if (Test-Path $botPath) {
        foreach ($line in [System.IO.File]::ReadAllLines($botPath, [System.Text.Encoding]::UTF8)) {
            if ($line -match 'cow' -and $line -match '(\d+)') {
                if ($line -notmatch '/execute|/summon|summon.success') { $counts += $line.Trim() }
            }
        }
    }
    if ($counts.Count -gt 0) { Write-Output ('entity count replies: ' + (($counts | Select-Object -Last 3) -join ' || ')) }
    $values = Parse-Samples ('bot-' + $name + '.log')
    Write-Output ("raw mspt lines: {0}" -f $values.Count)
    Show-Triple ($name + " mobs$Cows") $values 0
    Show-Triple ($name + " mobs$Cows2") $values 3
    Show-Triple ($name + ' mobs+redstone') $values 6
}

Reset-Dir $ab
Reset-Dir $paperDir
Reset-Dir $cwDir

$ops = '[{"uuid":"a0ad1a0e-6879-328a-b398-747f79be8dc7","name":"boater","level":4,"bypassesPlayerLimit":true}]'
Set-Content -Path (Join-Path $paperDir 'ops.json') -Value $ops -Encoding ascii
Set-Content -Path (Join-Path $paperDir 'eula.txt') -Value 'eula=true' -Encoding ascii
$paperProps = @(
    'server-ip=127.0.0.1',
    'server-port=25611',
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
    'motd=ab'
)
Set-Content -Path (Join-Path $paperDir 'server.properties') -Value $paperProps -Encoding ascii

New-Item -ItemType Directory -Force -Path (Join-Path $cwDir 'api\libs') | Out-Null
Copy-Item (Join-Path $root 'java\dist\chunkwake-api.jar') (Join-Path $cwDir 'api\chunkwake-api.jar') -Force
Copy-Item (Join-Path $root 'java\dist\libs\*.jar') (Join-Path $cwDir 'api\libs') -Force
$cwProps = @(
    'server-ip=127.0.0.1',
    'server-port=25612',
    'online-mode=false',
    'level-type=flat',
    'level-seed=1',
    'spawn-protection=0',
    'max-players=10',
    'view-distance=8',
    'simulation-distance=8',
    'log-level=info',
    ("generated-dir=" + (Join-Path $root 'target\release\generated')),
    'velocity-enabled=false'
)
Set-Content -Path (Join-Path $cwDir 'server.properties') -Value $cwProps -Encoding ascii
Set-Content -Path (Join-Path $cwDir 'ops.json') -Value $ops -Encoding ascii

$runFile = Join-Path $ab 'run.txt'
Build-Script $runFile

Run-Scenario 'Chunkwake' 25612 (Join-Path $ab 'chunkwake.log') $runFile
Run-Scenario 'Paper' 25611 (Join-Path $ab 'paper.log') $runFile
Get-Process chunkwake, java, opcmd_probe -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output "logs: target\ab"
