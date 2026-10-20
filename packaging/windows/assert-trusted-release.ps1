<#
.SYNOPSIS
  Fail-closed Authenticode release gate for EXE/DLL/MSI artifacts.
.DESCRIPTION
  Read-only verification. No policy changes, certificate imports, PIN entry,
  MSI execution, signature fabrication, self-signed roots or bypass flags.
  An unsigned package remains a development artifact, not a trusted release.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
  [string[]] $Paths
)
$ErrorActionPreference = 'Stop'
if ($Paths.Count -eq 0) { throw 'At least one explicit release file is required' }
foreach ($path in $Paths) {
  $file = Get-Item -LiteralPath $path -ErrorAction Stop
  if ($file.PSIsContainer -or $file.Extension.ToLowerInvariant() -notin @('.exe', '.dll', '.msi')) {
    throw "Not a supported signed release file: $path"
  }
  $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName -ErrorAction Stop
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "CODE_SIGNING_NO_GO: $($file.Name) status=$($signature.Status)"
  }
  if ($null -eq $signature.SignerCertificate) {
    throw "CODE_SIGNING_NO_GO: $($file.Name) has no publisher certificate"
  }
  $cert = $signature.SignerCertificate
  if (-not ($cert.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' })) {
    throw "CODE_SIGNING_NO_GO: $($file.Name) lacks Code Signing EKU"
  }
  # Conservative release policy for this SAC-enforced device: require RSA here.
  # Microsoft's older English guidance says RSA-only, while newer localized
  # versions mention ECC support. Actual SAC compatibility must still be tested.
  # This is a project-specific gate, not a general claim that ECC is unsupported.
  if ($cert.PublicKey.Oid.Value -ne '1.2.840.113549.1.1.1') {
    throw "CODE_SIGNING_NO_GO: $($file.Name) has a non-RSA publisher key"
  }
  if ($null -eq $signature.TimeStamperCertificate) {
    throw "CODE_SIGNING_NO_GO: $($file.Name) lacks a verifiable timestamp signer"
  }
  [pscustomobject]@{
    file = $file.FullName
    sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    authenticode = 'Valid'
    timestamp_certificate_present = $true
    publisher_code_signing_eku = $true
    rsa_publisher_certificate = $true
  } | ConvertTo-Json -Compress
}
Write-Output 'TRUSTED_RELEASE_GATE=PASS'
