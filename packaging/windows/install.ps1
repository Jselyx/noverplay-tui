param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Noverplay")
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    throw "LOCALAPPDATA не найден"
}

$source = Join-Path $PSScriptRoot "noverplay.exe"
$npSource = Join-Path $PSScriptRoot "np.exe"
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Рядом с install.ps1 нет noverplay.exe"
}
if (-not (Test-Path -LiteralPath $npSource -PathType Leaf)) {
    throw "Рядом с install.ps1 нет np.exe"
}

$targetDir = [IO.Path]::GetFullPath($InstallDir)
New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $targetDir "noverplay.exe") -Force
Copy-Item -LiteralPath $npSource -Destination (Join-Path $targetDir "np.exe") -Force

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$known = $parts | Where-Object {
    $_.Trim().TrimEnd("\") -ieq $targetDir.TrimEnd("\")
}
if (-not $known) {
    $nextPath = (@($parts) + $targetDir) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $nextPath, "User")
}

Write-Output "Noverplay установлен в $targetDir"
Write-Output "Открой новую консоль и напиши noverplay или np play <трек>"
