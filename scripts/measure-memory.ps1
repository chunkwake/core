param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [Parameter(Mandatory=$true)][string]$Label,
    [ValidateSet('auto','low','normal')][string]$Profile = 'normal',
    [int]$View = 10,
    [int]$Seconds = 60
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$runDir = Join-Path $root (Join-Path 'memory-measure' $Label)
if (Test-Path -LiteralPath $runDir) { throw "measurement directory already exists: $runDir" }
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
$data = Join-Path $root 'target/release/generated'
$packets = Join-Path $data '26.3/reports/packets.json'
$bot = Join-Path $root 'target/alt/release/examples/fly_probe.exe'
if (!(Test-Path -LiteralPath $Binary) -or !(Test-Path -LiteralPath $packets) -or !(Test-Path -LiteralPath $bot)) {
    throw 'server binary, packet report, or fly_probe is missing'
}
@"
server-ip=127.0.0.1
server-port=25599
online-mode=false
level-seed=12345
level-type=minecraft:normal
view-distance=$View
simulation-distance=6
spawn-protection=0
max-players=5
log-level=info
generated-dir=$($data.Replace('\','/'))
memory-profile=$Profile
"@ | Set-Content -LiteralPath (Join-Path $runDir 'server.properties') -Encoding utf8

$oldStats = $env:CHUNKWAKE_MEMSTATS
$env:CHUNKWAKE_MEMSTATS = '1'
$server = $null
try {
    $server = Start-Process -FilePath $Binary -ArgumentList @('run','--dir',$runDir) -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $runDir 'server.out') -RedirectStandardError (Join-Path $runDir 'server.err')
    $ready = $false
    for ($i=0; $i -lt 120; $i++) {
        Start-Sleep -Seconds 1
        if ($server.HasExited) { throw "server exited $($server.ExitCode)" }
        if (Select-String -LiteralPath (Join-Path $runDir 'server.out') -Pattern 'listening on' -Quiet -ErrorAction SilentlyContinue) {
            $ready = $true; break
        }
    }
    if (!$ready) { throw 'server did not start in 120 seconds' }
    Start-Sleep -Seconds 5
    $sample = {
        param([string]$phase)
        $p = Get-Process -Id $server.Id
        [pscustomobject]@{
            Phase=$phase
            PrivateMB=[math]::Round($p.PrivateMemorySize64/1MB,1)
            WorkingMB=[math]::Round($p.WorkingSet64/1MB,1)
        }
    }
    $samples = @(& $sample 'idle')
    $probe = Start-Process -FilePath $bot -ArgumentList @('127.0.0.1:25599',$packets,'20',$Seconds,$View) `
        -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $runDir 'probe.out') `
        -RedirectStandardError (Join-Path $runDir 'probe.err')
    while (!$probe.HasExited) {
        Start-Sleep -Seconds 2
        $probe.Refresh()
        $samples += & $sample 'flight'
    }
    Start-Sleep -Seconds 10
    $samples += & $sample 'after10s'
    Start-Sleep -Seconds 20
    $samples += & $sample 'after30s'
    $samples | Export-Csv -LiteralPath (Join-Path $runDir 'samples.csv') -NoTypeInformation
    $samples | Format-Table -AutoSize
    Get-Content -LiteralPath (Join-Path $runDir 'probe.out') -Tail 2
    Get-Content -LiteralPath (Join-Path $runDir 'server.out') | Select-String 'memstats' | Select-Object -Last 3
} finally {
    if ($server -and !$server.HasExited) { Stop-Process -Id $server.Id -Force }
    $env:CHUNKWAKE_MEMSTATS = $oldStats
}
