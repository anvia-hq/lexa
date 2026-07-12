param(
    [string]$Version = $env:LEXA_VERSION,
    [string]$InstallDir = $env:LEXA_INSTALL_DIR,
    [string]$Repo = $env:LEXA_REPO
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "latest"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Lexa\bin"
}

if ([string]::IsNullOrWhiteSpace($Repo)) {
    $Repo = "anvia-hq/lexa"
}

$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -notin @("AMD64", "x86_64")) {
    throw "Unsupported platform: Windows $arch. The release build supports Windows x86_64."
}

if ($Version -eq "latest") {
    $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $tag = $latest.tag_name
    if ([string]::IsNullOrWhiteSpace($tag)) {
        throw "Could not determine latest release for $Repo."
    }
} elseif ($Version.StartsWith("v")) {
    $tag = $Version
} else {
    $tag = "v$Version"
}

$assetVersion = $tag.TrimStart("v")
$archive = "lexa-windows-x86_64-$assetVersion.zip"
$url = "https://github.com/$Repo/releases/download/$tag/$archive"
$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
$zipPath = Join-Path $tmpDir $archive
$extractDir = Join-Path $tmpDir "extract"

New-Item -ItemType Directory -Path $tmpDir, $extractDir -Force | Out-Null

try {
    Write-Host "Downloading $url..."
    Invoke-WebRequest -Uri $url -OutFile $zipPath
    $checksumsPath = Join-Path $tmpDir "SHA256SUMS"
    $checksumsUrl = "https://github.com/$Repo/releases/download/$tag/SHA256SUMS"
    Invoke-WebRequest -Uri $checksumsUrl -OutFile $checksumsPath
    $checksumLine = Get-Content $checksumsPath | Where-Object { $_ -match "^[0-9a-fA-F]+\s+$([regex]::Escape($archive))$" } | Select-Object -First 1
    if (-not $checksumLine) {
        throw "Checksum file did not contain $archive."
    }
    $expectedChecksum = ($checksumLine -split "\s+")[0].ToLowerInvariant()
    $actualChecksum = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualChecksum -ne $expectedChecksum) {
        throw "Checksum mismatch for $archive."
    }
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

    $binary = Join-Path $extractDir "lexa-windows-x86_64-$assetVersion\lexa.exe"
    if (-not (Test-Path $binary)) {
        $binaryCandidates = @(Get-ChildItem -Path $extractDir -Recurse -Filter "lexa.exe" -File)
        if ($binaryCandidates.Count -eq 1) {
            $binary = $binaryCandidates[0].FullName
        }
    }
    if (-not (Test-Path $binary)) {
        throw "Archive did not contain expected binary: lexa.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $binary -Destination (Join-Path $InstallDir "lexa.exe") -Force

    Write-Host "Installed lexa $tag to $InstallDir\lexa.exe"

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathEntries = $userPath -split ";" | Where-Object { $_ -ne "" }
    if ($pathEntries -notcontains $InstallDir) {
        $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $InstallDir } else { "$userPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use lexa from anywhere."
    }

    & (Join-Path $InstallDir "lexa.exe") --help | Out-Null
    Write-Host "lexa is ready."
} finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
