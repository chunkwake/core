$ErrorActionPreference = 'Continue'
$dest = Join-Path $env:TEMP 'opencode\load-gate-extra\plugins'
Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$ua = @{ 'User-Agent' = 'chunkwake-test/1.0' }
$slugs = @(
'advancedban','ajleaderboards','askyblock','auctionhouse','auraskills','axgraves',
'baltop','beautifultrails','bentobox','betonquest','betterhud','bluemap',
'boostedyaml','chestshop','chestsort','chunky','chunkyborder','citizens','cloud',
'combatlogx','commandapi','configlib','coreprotect','craftbook','craftengine',
'crazyauctions','crazycrates','crazyenvoys','custom-player-models','customcrops',
'customdiscs','customdiscs-svc','customfishing','customheads','customscoreboard',
'deathannouncer','deluxehub','deluxemenus','discordsrv','dreamdisplays','dynmap',
'ecoenchants','ecoitems','economy','economyshopgui','elitemobs','emotecraft',
'essentialsx','evenmorefish','excellentclaims','excellentcrates','excellentenchants',
'executableblocks','executableitems','factions','fancyholograms','fancynpcs',
'fastasyncworldedit','fawe','floodgate','geyser','gravesx','griefdefender',
'griefprevention','headdatabase','headhunter','headsplus','huskhomes','imageframe',
'invsee-plus-plus','itemjoin','itemsadder','jobs','krypton','lands','levelledmobs',
'libertybans','litebans','maintenance','matrix','mcmmo','minimotd','multiverse-core',
'mythiccrucible','mythicdrops','mythicenchants','mythicmobs','nbtapi','nbteditor',
'nocheatplus','npc-plugin','oraxen','placeholderapi','plan','playerheads',
'playerwarps','plotsquared','plugmanx','pyrofishing','quests','quickshop-hikari',
'redprotect','residence','rosechat','scoreboard-library','sethome','shopkeepers',
'simplebackpack','simpleclaimsystem','simpleutils','spark','spartan','squaremap',
'tab','tabtps','talismans','terra','towny','tps-hud','traders','trmenu',
'ultimateclaims','ultimateshop','vault','veinminer','viabackwards','villager-market',
'vulcan','webcam-mod','worldedit','worldguard','zmenu'
)
$loaders = '%5B%22paper%22,%22bukkit%22,%22spigot%22%5D'
$ok = 0
$fail = @()
foreach ($slug in $slugs) {
    try {
        $uri = "https://api.modrinth.com/v2/project/$slug/version?game_versions=%5B%2226.3%22%5D&loaders=$loaders"
        $versions = Invoke-RestMethod -Uri $uri -Headers $ua -TimeoutSec 30
        if (-not $versions -or $versions.Count -eq 0) {
            $fail += "$slug : no Paper/Bukkit/Spigot 26.3 version"
            continue
        }
        $v = $versions[0]
        $file = $v.files | Where-Object { $_.primary } | Select-Object -First 1
        if (-not $file) { $file = $v.files[0] }
        $out = Join-Path $dest $file.filename
        Invoke-WebRequest -Uri $file.url -OutFile $out -Headers $ua -TimeoutSec 180
        $ok++
        Write-Output ("OK   {0} -> {1}" -f $slug, $file.filename)
    } catch {
        $fail += ("{0} : {1}" -f $slug, $_.Exception.Message)
    }
    Start-Sleep -Milliseconds 150
}
Write-Output ("DOWNLOADED: {0}; FAILED: {1}" -f $ok, $fail.Count)
$fail | ForEach-Object { Write-Output ("  " + $_) }
Write-Output ("jar count: " + (Get-ChildItem $dest -Filter '*.jar').Count)
