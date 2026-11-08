param([Parameter(Mandatory=$true)][string]$Binary)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$data = Join-Path $root 'target/release/generated'
$packets = Join-Path $data '26.3/reports/packets.json'
$registries = Join-Path $data '26.3/reports/registries.json'
$examples = Join-Path $root 'target/alt/release/examples'
$cases = @(
    @('leaves_probe','lv_ours.out',10),
    @('dispense_items_probe','di_ours.out',24),
    @('banner_probe','bp_ours.out',24),
    @('crafter_probe','cp_ours.out',24),
    @('respawn_probe','rs_ours.out',24)
)
foreach ($case in $cases) {
    $name, $reference, $simulation = $case
    $runDir = Join-Path $root (Join-Path 'memory-measure' "probe2-$name")
    if (Test-Path -LiteralPath $runDir) { throw "probe directory already exists: $runDir" }
    New-Item -ItemType Directory -Path $runDir -Force | Out-Null
    Copy-Item -LiteralPath 'E:/tmp/flytest/ops.json' -Destination (Join-Path $runDir 'ops.json')
    @"
server-ip=127.0.0.1
server-port=25599
online-mode=false
level-seed=12345
level-type=minecraft:normal
view-distance=32
simulation-distance=$simulation
spawn-protection=0
max-players=5
log-level=info
generated-dir=$data
memory-profile=normal
"@ | Set-Content -LiteralPath (Join-Path $runDir 'server.properties') -Encoding utf8
    $server = $null
    try {
        $server = Start-Process -FilePath $Binary -ArgumentList @('run','--dir',$runDir) -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $runDir 'server.out') -RedirectStandardError (Join-Path $runDir 'server.err')
        $ready = $false
        for ($i=0; $i -lt 120; $i++) {
            Start-Sleep -Seconds 1
            if ($server.HasExited) { throw "${name}: server exited $($server.ExitCode)" }
            if (Select-String -LiteralPath (Join-Path $runDir 'server.out') -Pattern 'listening on' -Quiet -ErrorAction SilentlyContinue) {
                $ready = $true; break
            }
        }
        if (!$ready) { throw "${name}: server did not start" }
        Start-Sleep -Seconds 2
        $probe = Join-Path $examples "$name.exe"
        $out = Join-Path $runDir 'probe.out'
        $err = Join-Path $runDir 'probe.err'
        $task = Start-Process -FilePath $probe -ArgumentList @('127.0.0.1:25599',$packets,$registries) `
            -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
        Wait-Process -Id $task.Id -Timeout 300
        $task.Refresh()
        $baseline = Join-Path 'E:/tmp/flytest' $reference
        $old = Get-Content -LiteralPath $baseline
        $new = Get-Content -LiteralPath $out
        $diff = Compare-Object -ReferenceObject $old -DifferenceObject $new
        [pscustomobject]@{Probe=$name;ExitCode=$task.ExitCode;OldLines=$old.Count;NewLines=$new.Count;DiffLines=@($diff).Count} | Format-Table -AutoSize
        if ($diff) { $diff | Select-Object -First 8 | Out-String | Write-Output }
    } finally {
        if ($server -and !$server.HasExited) { Stop-Process -Id $server.Id -Force }
    }
}
