param(
    [Parameter(Mandatory = $true)]
    [string]$Binary,
    [Parameter(Mandatory = $true)]
    [string]$NpBinary,
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^[0-9][0-9A-Za-z.+~-]*$') {
    throw "Версия не подходит для пакета: $Version"
}
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
$npBinaryPath = (Resolve-Path -LiteralPath $NpBinary).Path
$licensePath = (Resolve-Path -LiteralPath "LICENSE").Path
$installerPath = (Resolve-Path -LiteralPath "packaging/windows/install.ps1").Path
$outputPath = [IO.Path]::GetFullPath($OutputDir)
$stagePath = Join-Path $outputPath "noverplay-windows-x86_64"
$archivePath = Join-Path $outputPath "noverplay-windows-x86_64.zip"
$manifestPath = Join-Path $outputPath "noverplay.json"

if (Test-Path -LiteralPath $stagePath) {
    throw "Каталог сборки уже существует: $stagePath"
}
if (Test-Path -LiteralPath $archivePath) {
    throw "Архив уже существует: $archivePath"
}

New-Item -ItemType Directory -Path $stagePath -Force | Out-Null
Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stagePath "noverplay.exe")
Copy-Item -LiteralPath $npBinaryPath -Destination (Join-Path $stagePath "np.exe")
Copy-Item -LiteralPath $licensePath -Destination (Join-Path $stagePath "LICENSE")
Copy-Item -LiteralPath $installerPath -Destination (Join-Path $stagePath "install.ps1")
Compress-Archive -Path (Join-Path $stagePath "*") -DestinationPath $archivePath

$sha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
$releaseUrl = "https://github.com/Jselyx/noverplay-tui/releases/download/v$Version/noverplay-windows-x86_64.zip"
$manifest = [ordered]@{
    version = $Version
    description = "Терминальный музыкальный клиент Noverplay"
    homepage = "https://github.com/Jselyx/noverplay-tui"
    license = "GPL-3.0-only"
    architecture = [ordered]@{
        "64bit" = [ordered]@{
            url = $releaseUrl
            hash = $sha256
        }
    }
    bin = @("noverplay.exe", "np.exe")
}
$json = $manifest | ConvertTo-Json -Depth 6
[IO.File]::WriteAllText($manifestPath, "$json`n", [Text.UTF8Encoding]::new($false))

Write-Output $archivePath
Write-Output $manifestPath
