<#
.SYNOPSIS
  Downloads the pinned Real-ESRGAN ncnn Vulkan engine into engine/bin.

.DESCRIPTION
  Reads engine/engine.lock.json, downloads the release archive, verifies its
  SHA-256, and copies only the files Upscalee needs (exe, OpenMP runtime and the
  two x4 models) into engine/bin. Binaries are never committed to git.

.PARAMETER Force
  Re-download even if engine/bin already contains every required file.
#>
[CmdletBinding()]
param([switch]$Force)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$lock = Get-Content (Join-Path $root 'engine/engine.lock.json') -Raw | ConvertFrom-Json
$bin = Join-Path $root 'engine/bin'
$download = Join-Path $root 'engine/.download'

$missing = $lock.files | Where-Object { -not (Test-Path (Join-Path $bin $_)) }
if (-not $Force -and -not $missing) {
    Write-Host "Engine already present in $bin"
    return
}

New-Item -ItemType Directory -Force -Path $download | Out-Null
$archive = Join-Path $download 'engine.zip'

$needsDownload = $true
if (Test-Path $archive) {
    $needsDownload = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $lock.sha256
}
if ($needsDownload) {
    Write-Host "Downloading $($lock.release)..."
    Invoke-WebRequest -Uri $lock.url -OutFile $archive -UseBasicParsing
}

$hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash -ne $lock.sha256) {
    Remove-Item $archive -Force
    throw "SHA-256 mismatch for engine archive: expected $($lock.sha256), got $hash"
}

$extract = Join-Path $download 'extract'
if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
Expand-Archive -Path $archive -DestinationPath $extract

foreach ($file in $lock.files) {
    $source = Get-ChildItem -Path $extract -Recurse -File |
        Where-Object { $_.FullName.Replace('\', '/').EndsWith('/' + $file) } |
        Select-Object -First 1
    if (-not $source) { throw "Archive does not contain $file" }
    $target = Join-Path $bin $file
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
    Copy-Item $source.FullName $target -Force
}

Remove-Item $extract -Recurse -Force
Write-Host "Engine ready in $bin"
