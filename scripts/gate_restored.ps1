$ErrorActionPreference = 'Continue'
$root = 'E:\paper_rust'
$tmp = Join-Path $env:TEMP 'opencode'
$stand = Join-Path $tmp 'load-gate-extra'
$plugins = Join-Path $stand 'plugins'
New-Item -ItemType Directory -Force -Path $plugins | Out-Null

# Local probes and the repository test plugin.
Copy-Item (Join-Path $tmp 'ce-parity-cw\plugins\chunkwake-test-plugin.jar') $plugins -Force -ErrorAction SilentlyContinue
Copy-Item (Join-Path $tmp 'ce-parity-cw\plugins\craft-engine-paper-plugin-26.9.2.jar') $plugins -Force -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root 'java\perf-probe\build\libs\chunkwake-perf-probe.jar') (Join-Path $plugins 'PerfProbe.jar') -Force -ErrorAction SilentlyContinue

$props = @(
    'server-ip=127.0.0.1',
    'server-port=25630',
    'online-mode=false',
    'level-type=flat',
    'level-seed=1',
    'generate-structures=false',
    'spawn-protection=0',
    'max-players=10',
    'view-distance=4',
    'simulation-distance=4',
    'difficulty=peaceful',
    'allow-nether=false',
    'log-level=info',
    ("generated-dir=" + (Join-Path $root 'target\release\generated')),
    'velocity-enabled=false'
)
Set-Content (Join-Path $stand 'server.properties') $props -Encoding ascii
$ops = '[{"uuid":"a0ad1a0e-6879-328a-b398-747f79be8dc7","name":"boater","level":4,"bypassesPlayerLimit":true}]'
Set-Content (Join-Path $stand 'ops.json') $ops -Encoding ascii

Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$log = Join-Path $tmp 'gate_restored.log'
Remove-Item $log -ErrorAction SilentlyContinue
$p = Start-Process -FilePath (Join-Path $root 'target\release\chunkwake.exe') `
    -ArgumentList '--dir', ('"' + $stand + '"') `
    -RedirectStandardOutput $log -RedirectStandardError ($log + '.err') -WindowStyle Hidden -PassThru
for ($i = 0; $i -lt 300; $i++) {
    Start-Sleep -Seconds 2
    $t = Get-Content $log -Raw -ErrorAction SilentlyContinue
    if ($t -match 'plugin\(s\) in|of \d+ enabled') { break }
    if ($p.HasExited) { break }
}
$summary = (Get-Content $log -ErrorAction SilentlyContinue | Select-String 'of \d+ enabled|plugin\(s\) in' | Select-Object -Last 1).Line
$failed = (Get-Content $log -ErrorAction SilentlyContinue | Select-String 'failed in' | ForEach-Object { $_.Line.Split(']')[-1].Trim() }) -join '; '
$loaderr = (Get-Content $log -ErrorAction SilentlyContinue | Select-String 'Could not load' | ForEach-Object { $_.Line.Split(']')[-1].Trim() }) -join '; '
$panics = (Get-Content $log -ErrorAction SilentlyContinue | Select-String 'panicked').Count
$skip = (Get-Content $log -ErrorAction SilentlyContinue | Select-String 'skipped \(missing server internals\)' | Select-Object -Last 1).Line
Start-Sleep -Seconds 3
Get-Process chunkwake -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output ('SUMMARY: ' + $summary)
Write-Output ('FAILED: ' + $failed)
Write-Output ('LOADERR: ' + $loaderr)
Write-Output ('SKIPPED: ' + $skip)
Write-Output ('PANICS: ' + $panics)
Write-Output ('JARS: ' + (Get-ChildItem $plugins -Filter '*.jar').Count)
