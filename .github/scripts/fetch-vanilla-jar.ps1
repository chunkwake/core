# Скачивает `server.jar` нужной версии по манифесту Mojang — с повторами.
#
# Запуск: fetch-vanilla-jar.ps1 -Version 26.3 [-Dir .]
#
# `server.jar` уже на месте — ничего не делает: в CI его приносит кэш.
#
# Сам файл качается через `curl.exe`, а не `Invoke-WebRequest`: на 60 МБ
# последний отваливается по таймауту соединения («connected party did not
# properly respond»), и повторить его нечем. У curl есть и свои повторы,
# и `--retry-all-errors` для сетевых обрывов.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Dir = '.'
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Dir)) { New-Item -ItemType Directory -Path $Dir -Force | Out-Null }
Set-Location $Dir

if (Test-Path server.jar) {
    Write-Host 'server.jar уже в кэше'
    exit 0
}

# Повторяет действие с растущей паузой: манифест Mojang тоже иногда не отвечает.
function Invoke-Retry {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Action,
        [Parameter(Mandatory = $true)][string]$What,
        [int]$Attempts = 5
    )
    for ($i = 1; $i -le $Attempts; $i++) {
        try {
            return & $Action
        } catch {
            if ($i -eq $Attempts) { throw "${What}: $($_.Exception.Message)" }
            Write-Host "${What}: попытка $i из $Attempts не удалась — $($_.Exception.Message)"
            Start-Sleep -Seconds (5 * $i)
        }
    }
}

$manifest = Invoke-Retry -What 'манифест Mojang' -Action {
    Invoke-RestMethod 'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json' -TimeoutSec 60
}
$entry = $manifest.versions | Where-Object { $_.id -eq $Version }
if (-not $entry) { throw "версии $Version нет в манифесте Mojang" }

$meta = Invoke-Retry -What "манифест $Version" -Action {
    Invoke-RestMethod $entry.url -TimeoutSec 60
}
$server = $meta.downloads.server

Invoke-Retry -What 'скачивание server.jar' -Action {
    # Недокачанный файл от прошлого запуска не должен попасть в проверку SHA-1.
    if (Test-Path server.jar) { Remove-Item server.jar -Force }
    & curl.exe -fsSL --retry 5 --retry-delay 5 --retry-all-errors `
        --connect-timeout 20 --max-time 600 -o server.jar $server.url
    if ($LASTEXITCODE -ne 0) { throw "curl.exe вышел с кодом $LASTEXITCODE" }
    if (-not (Test-Path server.jar)) { throw 'curl.exe не создал server.jar' }
}

$actual = (Get-FileHash server.jar -Algorithm SHA1).Hash.ToLower()
if ($actual -ne $server.sha1) {
    throw "SHA-1 не совпал: ждали $($server.sha1), получили $actual"
}
Write-Host "server.jar скачан и проверен ($((Get-Item server.jar).Length / 1MB -as [int]) МБ)"
