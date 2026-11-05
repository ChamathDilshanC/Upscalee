<#
.SYNOPSIS
  Creates a self-signed code-signing certificate for LOCAL DEVELOPMENT ONLY.

.DESCRIPTION
  Writes target/dev-signing.pfx (git-ignored) for scripts/build-sparse-package.ps1.
  Windows installs a sparse package only if its signer is trusted. With -Trust
  (run as Administrator) the certificate is added to LocalMachine\TrustedPeople
  on THIS machine. Never ship a self-signed certificate to users; release builds
  need a real code-signing certificate.
#>
[CmdletBinding()]
param(
    [string]$Subject = 'CN=Upscalee Dev',
    [Parameter(Mandatory)] [SecureString]$Password,
    [switch]$Trust
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root 'target/dev-signing.pfx'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $out) | Out-Null

$cert = New-SelfSignedCertificate -Type Custom -Subject $Subject `
    -KeyUsage DigitalSignature -FriendlyName 'Upscalee development signing' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
Export-PfxCertificate -Cert $cert -FilePath $out -Password $Password | Out-Null
Write-Host "Created $out for $Subject (thumbprint $($cert.Thumbprint))"

if ($Trust) {
    Import-PfxCertificate -FilePath $out -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' -Password $Password | Out-Null
    Write-Host 'Trusted on this machine (LocalMachine\TrustedPeople).'
}
else {
    Write-Host 'Not trusted yet. Re-run with -Trust from an elevated prompt to allow sideloading the dev package.'
}
