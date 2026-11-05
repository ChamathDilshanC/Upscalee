<#
.SYNOPSIS
  Builds and signs UpscaleeContextMenu.msix (Windows 11 modern context menu).

.DESCRIPTION
  The sparse package carries only the manifest and logos; its executable and
  UpscaleeShell.dll live in the install folder (external location). Windows only
  installs it when the signature chains to a certificate trusted on the machine,
  and the manifest Publisher must equal the certificate subject.

.EXAMPLE
  ./scripts/build-sparse-package.ps1 -CertificatePath C:\keys\upscalee.pfx -Publisher "CN=Upscalee Dev"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$CertificatePath,
    [Parameter(Mandatory)] [string]$Publisher,
    [SecureString]$CertificatePassword,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Find-SdkTool([string]$name) {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $tool = Get-ChildItem -Path $kits -Recurse -Filter $name -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object { [version]($_.Directory.Parent.Name -replace '[^\d\.]', '') } -Descending |
        Select-Object -First 1
    if (-not $tool) { throw "$name not found. Install the Windows 10/11 SDK." }
    $tool.FullName
}

if (-not $Version) {
    $conf = Get-Content (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
    $Version = $conf.version
}
$packageVersion = (($Version -split '[-+]')[0] + '.0.0.0').Split('.')[0..3] -join '.'

$staging = Join-Path $root 'target/sparse-package'
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'Assets') | Out-Null

$manifest = Get-Content (Join-Path $root 'apps/shell-extension/package/AppxManifest.xml') -Raw
$manifest = $manifest.Replace('__PUBLISHER__', [Security.SecurityElement]::Escape($Publisher)).Replace('__VERSION__', $packageVersion)
Set-Content -Path (Join-Path $staging 'AppxManifest.xml') -Value $manifest -Encoding UTF8

$icons = Join-Path $root 'apps/desktop/src-tauri/icons'
foreach ($logo in 'StoreLogo.png', 'Square44x44Logo.png', 'Square150x150Logo.png') {
    Copy-Item (Join-Path $icons $logo) (Join-Path $staging "Assets/$logo")
}

$output = Join-Path $root 'target/release/UpscaleeContextMenu.msix'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $output) | Out-Null
& (Find-SdkTool 'makeappx.exe') pack /d $staging /p $output /nv /o
if ($LASTEXITCODE -ne 0) { throw 'makeappx failed' }

$signArgs = @('sign', '/fd', 'SHA256', '/f', $CertificatePath)
if ($CertificatePassword) {
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR(
        [Runtime.InteropServices.Marshal]::SecureStringToBSTR($CertificatePassword))
    $signArgs += @('/p', $plain)
}
& (Find-SdkTool 'signtool.exe') @signArgs $output
if ($LASTEXITCODE -ne 0) { throw 'signtool failed' }

Write-Host "Signed sparse package: $output"
