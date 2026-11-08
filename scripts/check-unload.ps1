param([Parameter(Mandatory=$true)][string]$Binary)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$runDir = Join-Path $root 'memory-measure/roundtrip'
if (Test-Path -LiteralPath $runDir) { throw "directory already exists: $runDir" }
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
Copy-Item -LiteralPath 'E:/tmp/flytest/ops.json' -Destination (Join-Path $runDir 'ops.json')
$data = Join-Path $root 'target/release/generated'
@"
server-ip=127.0.0.1
server-port=25599
online-mode=false
level-seed=12345
level-type=minecraft:normal
view-distance=10
simulation-distance=6
spawn-protection=0
generated-dir=$data
memory-profile=normal
"@ | Set-Content -LiteralPath (Join-Path $runDir 'server.properties') -Encoding utf8
$server = $null
$oldGap = $env:GAP_MS
try {
    $server = Start-Process -FilePath $Binary -ArgumentList @('run','--dir',$runDir) -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $runDir 'server.out') -RedirectStandardError (Join-Path $runDir 'server.err')
    $ready = $false
    for ($i=0; $i -lt 120; $i++) {
        Start-Sleep -Seconds 1
        if ($server.HasExited) { throw "server exited $($server.ExitCode)" }
        if (Select-String -LiteralPath (Join-Path $runDir 'server.out') -Pattern 'listening on' -Quiet -ErrorAction SilentlyContinue) { $ready = $true; break }
    }
    if (!$ready) { throw 'server did not start' }
    $env:GAP_MS = '9000'
    $bot = Join-Path $root 'target/alt/release/examples/cmd_probe.exe'
    $packets = Join-Path $data '26.3/reports/packets.json'
    & $bot '127.0.0.1:25599' $packets '0,0,0' `
        'setblock ~x ~y ~z minecraft:air' `
        'setblock ~x ~y ~z minecraft:gold_block' `
        'tp carter 20000 200 20000' `
        'time query daytime' `
        'time query daytime' `
        'tp carter ~x ~y ~z' `
        'fill ~x ~y ~z ~x ~y ~z minecraft:diamond_block replace minecraft:gold_block' |
        Set-Content -LiteralPath (Join-Path $runDir 'probe.out') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "cmd_probe exited $LASTEXITCODE" }
    Get-Content -LiteralPath (Join-Path $runDir 'probe.out')
    Select-String -LiteralPath (Join-Path $runDir 'server.out') -Pattern 'unloaded|error|panic' | Select-Object -First 10
} finally {
    if ($server -and !$server.HasExited) { Stop-Process -Id $server.Id -Force }
    $env:GAP_MS = $oldGap
}
