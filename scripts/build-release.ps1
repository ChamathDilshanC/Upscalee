<#
.SYNOPSIS
  Builds the Upscalee installer (target/release/bundle/nsis/Upscalee_<ver>_x64-setup.exe).

.DESCRIPTION
  1. Fetches the pinned upscale engine (scripts/fetch-engine.ps1).
  2. Runs the Rust and front-end test suites (skip with -SkipTests).
  3. Builds UpscaleeShell.dll in release mode.
  4. Optionally builds + signs the Windows 11 sparse package (-CertificatePath).
  5. Runs `tauri build` with tauri.release.conf.json, which bundles the engine,
     models, shell extension and notices into the NSIS installer.

.EXAMPLE
  ./scripts/build-release.ps1
  ./scripts/build-release.ps1 -CertificatePath target/dev-signing.pfx -Publisher "CN=Upscalee Dev" -CertificatePassword (Read-Host -AsSecureString)
#>
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [string]$CertificatePath,
    [string]$Publisher,
    [SecureString]$CertificatePassword
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    & (Join-Path $PSScriptRoot 'fetch-engine.ps1')

    if (-not $SkipTests) {
        cargo test --workspace
        if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
        pnpm --filter upscalee-desktop test
        if ($LASTEXITCODE -ne 0) { throw 'Front-end tests failed' }
    }

    cargo build -p upscalee-shell --release
    if ($LASTEXITCODE -ne 0) { throw 'Shell extension build failed' }

    $configs = @('src-tauri/tauri.release.conf.json')
    if ($CertificatePath) {
        if (-not $Publisher) { throw '-Publisher is required with -CertificatePath (must match the certificate subject)' }
        $sparseArgs = @{ CertificatePath = $CertificatePath; Publisher = $Publisher }
        if ($CertificatePassword) { $sparseArgs.CertificatePassword = $CertificatePassword }
        & (Join-Path $PSScriptRoot 'build-sparse-package.ps1') @sparseArgs
        $msixConfig = Join-Path $root 'target/tauri.msix.conf.json'
        @{ bundle = @{ resources = @{ '../../../target/release/UpscaleeContextMenu.msix' = 'UpscaleeContextMenu.msix' } } } |
            ConvertTo-Json -Depth 5 | Set-Content -Path $msixConfig -Encoding UTF8
        $configs += $msixConfig
    }

    $configArgs = $configs | ForEach-Object { @('--config', $_) }
    # `exec` passes flags through untouched (pnpm would otherwise claim --config).
    pnpm --dir apps/desktop exec tauri build @configArgs
    if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }

    Get-ChildItem (Join-Path $root 'target/release/bundle/nsis') -Filter '*.exe' |
        ForEach-Object { Write-Host "Installer: $($_.FullName)" }
}
finally {
    Pop-Location
}
