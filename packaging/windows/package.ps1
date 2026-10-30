<#
.SYNOPSIS
  Build, sign and package GridCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    gridcraft-<version>-windows-<arch>.msi            per-machine installer (WiX v5)
    gridcraft-<version>-windows-<arch>-portable.zip   app, CLI, native OPC and CMS signing workers

  The binaries link the C runtime statically (+crt-static), so neither the MSI nor the portable
  zip needs the Visual C++ redistributable. Signing is delegated to sign.ps1 (skipped with a
  warning when no signing secrets are set).

  Needs: Rust (MSVC toolchain + the target), the Windows SDK (rc.exe, signtool.exe),
  and WiX v5: dotnet tool install --global wix --version 5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1 -Arch x64
  pwsh packaging/windows/package.ps1 -Arch x86 -SkipBuild
  pwsh packaging/windows/package.ps1 -Arch arm64     # cross-compiled; needs the MSVC ARM64 build tools
#>
param(
  [ValidateSet('x64', 'x86', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild,
  [string] $OpcWorkerPath = $env:GRIDCRAFT_OPC_WORKER_PATH,
  [string] $CmsWorkerPath = $env:GRIDCRAFT_CMS_WORKER_PATH,
  [switch] $AllowUnsignedQa
)
$ErrorActionPreference = 'Stop'
# Never mistake an unsigned QA build for a distributable installer.
$azureKeys = 'AZURE_TENANT_ID','AZURE_CLIENT_ID','AZURE_CLIENT_SECRET','AZURE_SIGNING_ENDPOINT','AZURE_SIGNING_ACCOUNT','AZURE_CERT_PROFILE'
$hasSigning = [bool]$env:WINDOWS_CERTIFICATE -or -not @($azureKeys | Where-Object { -not [Environment]::GetEnvironmentVariable($_) }).Count
if (-not $hasSigning -and -not $AllowUnsignedQa) {
  throw 'Production MSI requires Authenticode signing. For a PRIVATE non-installable QA artifact, explicitly specify -AllowUnsignedQa.'
}
if ($AllowUnsignedQa) { Write-Warning 'PRIVATE QA ONLY: this installer may be unsigned. Do not distribute or install on production machines.' }
if ($Arch -ne 'x64') {
  throw 'Native OPC signing worker packaging currently supports Windows x64 only. Refusing mislabeled x86/ARM64 installers.'
}
# WiX .NET tools may be installed privately alongside the user's .NET SDK.
# A remote/non-interactive process can lack DOTNET_ROOT even when the runtime exists.
# Resolve it only for this packaging process; do not install runtimes or change Windows globally.
if (-not $env:DOTNET_ROOT) {
  $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
  if ($dotnet) {
    $candidate = Split-Path -Parent $dotnet.Source
    if (Test-Path -LiteralPath (Join-Path $candidate 'shared\Microsoft.NETCore.App')) {
      $env:DOTNET_ROOT = $candidate
      if (-not $env:DOTNET_ROOT_X64) { $env:DOTNET_ROOT_X64 = $candidate }
    }
  }
}
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
  throw 'WiX v5 command not available; MSI cannot be packaged.'
}
& wix --version | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'WiX v5 cannot start; check DOTNET_ROOT/.NET runtime.' }
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:GRIDCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not $env:GRIDCRAFT_BUILD_SHA) { $env:GRIDCRAFT_BUILD_SHA = (git -C $Root rev-parse HEAD 2>$null) }
if (-not $env:GRIDCRAFT_BUILD_DATE) { $env:GRIDCRAFT_BUILD_DATE = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd') }

Write-Output "GridCraft $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  # Static CRT: no VC++ redistributable needed. Scoped to the target so host build scripts and
  # proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  # Fail the build (rather than warn) if the icon/VERSIONINFO can't be embedded.
  $env:GRIDCRAFT_REQUIRE_WINRES = '1'
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p gridcraft -p gridcraft-cli --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"

# Standalone clean clones build their helpers from THIS repository. Explicit
# pinned QA paths are still accepted, but neither local sibling worktrees nor
# pre-existing unsigned binaries are required to make a release candidate.
if (-not $OpcWorkerPath) {
  $OpcWorkerPath = Join-Path $Bin 'craft-opc-worker.exe'
  $nativeManifest = Join-Path $Root 'vendor\craft-signing-core\Cargo.toml'
  if (!(Test-Path -LiteralPath $nativeManifest -PathType Leaf)) {
    throw 'Vendored native signing-core source is missing'
  }
  if (-not $SkipBuild) {
    Invoke-Native "cargo build native OPC worker ($Target)" {
      cargo build --release --locked --manifest-path $nativeManifest --bin craft-opc-worker --target $Target
    }
  }
}
if (-not $CmsWorkerPath) {
  $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
  $cmsSource = Join-Path $Root 'tools\detached-cms\CraftXlsxCms.cs'
  $cmsDir = Join-Path $TargetDir 'windows-package\support'
  New-Item -ItemType Directory -Force -Path $cmsDir | Out-Null
  $CmsWorkerPath = Join-Path $cmsDir 'craft-xlsx-cms.exe'
  if (!(Test-Path -LiteralPath $csc -PathType Leaf) -or !(Test-Path -LiteralPath $cmsSource -PathType Leaf)) {
    throw 'Bundled CMS helper source / .NET Framework compiler missing'
  }
  Invoke-Native 'build detached CMS worker from bundled C# source' {
    & $csc /nologo /target:exe /optimize+ ("/out:"+$CmsWorkerPath) /r:System.Security.dll $cmsSource
  }
}
foreach ($pair in @(@('craft-opc-worker.exe',$OpcWorkerPath),@('craft-xlsx-cms.exe',$CmsWorkerPath))) {
  if (-not $pair[1] -or -not (Test-Path -LiteralPath $pair[1] -PathType Leaf)) {
    throw "Signing helper $($pair[0]) unavailable after build; refusing incomplete MSI"
  }
  if ((Split-Path -Leaf $pair[1]) -ine $pair[0]) {
    throw "Expected explicitly named signing worker $($pair[0])"
  }
}

# Check both binaries' PE headers before packaging. Machine (COFF header) must match -Arch, so an
# x64 build can never ship labelled arm64. Subsystem (optional header): 2 = Windows GUI, 3 = console.
# The app must be GUI (no console window opens with it); the CLI must stay console so its output
# reaches the terminal.
function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = switch ($Arch) { 'x64' { 0x8664 } 'x86' { 0x14C } 'arm64' { 0xAA64 } }
foreach ($check in @(@('gridcraft.exe', 2), @('gridcraft-cli.exe', 3))) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne $Machine) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}
$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'gridcraft.exe'), (Join-Path $Bin 'gridcraft-cli.exe') $Stage
# Signer helpers must be next to gridcraft.exe: UI resolves them using current_exe().
# The CMS worker is .NET Framework AnyCPU; the native OPC worker MUST be x64.
$nativeHeader = Get-PeHeader $OpcWorkerPath
if ($nativeHeader.Machine -ne 0x8664) {
  throw "Native signing worker is not a Windows x64 PE executable: machine=$($nativeHeader.Machine)"
}
Copy-Item -LiteralPath $OpcWorkerPath -Destination (Join-Path $Stage 'craft-opc-worker.exe')
Copy-Item -LiteralPath $CmsWorkerPath -Destination (Join-Path $Stage 'craft-xlsx-cms.exe')
foreach ($worker in 'craft-opc-worker.exe','craft-xlsx-cms.exe') {
  if (-not (Test-Path -LiteralPath (Join-Path $Stage $worker) -PathType Leaf)) {
    throw "Failed to stage $worker; blocking incomplete MSI"
  }
}
$stageExecutables = @(
  (Join-Path $Stage 'gridcraft.exe'),
  (Join-Path $Stage 'gridcraft-cli.exe'),
  (Join-Path $Stage 'craft-opc-worker.exe'),
  (Join-Path $Stage 'craft-xlsx-cms.exe')
)
# PowerShell requires splatting @variable here. Passing @(...), an expression,
# binds one concatenated string and makes sign.ps1 resolve a nonexistent path.
& (Join-Path $PSScriptRoot 'sign.ps1') @stageExecutables
if (-not $AllowUnsignedQa) {
  foreach ($worker in 'gridcraft.exe','gridcraft-cli.exe','craft-opc-worker.exe','craft-xlsx-cms.exe') {
    $result=Get-AuthenticodeSignature -LiteralPath (Join-Path $Stage $worker)
    if ($result.Status -ne 'Valid') { throw "Authenticode not valid for $worker : $($result.Status)" }
  }
}

# ---- MSI ---------------------------------------------------------------------------------------
$Msi = Join-Path $Dist "gridcraft-$Version-windows-$Arch.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'gridcraft.wxs') -arch $Arch `
    -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\app-icon\gridcraft.ico')" `
    -o $Msi
}
# This independent build must never upgrade/remove original GridCraft.
# Gate every generated MSI against the unique Craft Open Source identity.
$IdentitySpec = Join-Path $PSScriptRoot 'identity-v48.json'
$IdentityChecker = Join-Path $PSScriptRoot 'verify-isolated-msi-identity.ps1'
if (!(Test-Path -LiteralPath $IdentitySpec -PathType Leaf) -or !(Test-Path -LiteralPath $IdentityChecker -PathType Leaf)) {
  throw 'Independent MSI identity guard is missing; do not distribute this package.'
}
& $IdentityChecker -Msi $Msi -IdentityJson $IdentitySpec | Out-Null

# wix writes its debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
& (Join-Path $PSScriptRoot 'sign.ps1') $Msi
if (-not $AllowUnsignedQa) {
  $msiSignature=Get-AuthenticodeSignature -LiteralPath $Msi
  if ($msiSignature.Status -ne 'Valid') { throw "MSI Authenticode invalid: $($msiSignature.Status)" }
}

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\gridcraft-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
foreach ($f in 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
# Builds made with craft-fonts (CRAFT_FONTS_DIR) embed its fonts: ship their licences.
if ($env:CRAFT_FONTS_DIR) {
  foreach ($ofl in Get-ChildItem -Path (Join-Path $env:CRAFT_FONTS_DIR 'fonts\*\OFL.txt') -ErrorAction SilentlyContinue) {
    Copy-Item $ofl.FullName (Join-Path $Portable "OFL-$($ofl.Directory.Name).txt")
  }
}
$Zip = Join-Path $Dist "gridcraft-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

# Smoke-test the CLI when this machine can run it. An ARM64 build made on an x64 runner can't run
# here; .github/workflows/windows-arm64.yml installs and runs it on ARM64 instead.
$HostArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
if ($Arch -ne 'arm64' -or $HostArch -eq 'arm64') {
  Invoke-Native 'gridcraft-cli --version' { & (Join-Path $Stage 'gridcraft-cli.exe') --version }
} else {
  Write-Output "skipping gridcraft-cli --version: an $Arch build doesn't run on this $HostArch machine"
}
Get-Item $Msi, $Zip | Format-Table Name, Length
