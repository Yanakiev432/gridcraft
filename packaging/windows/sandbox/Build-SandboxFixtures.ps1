<#
.SYNOPSIS
  Build isolated, unsigned, synthetic MSI fixtures for a disposable Windows Sandbox.
.DESCRIPTION
  Runs only WiX and the .NET Framework compiler on the host. NEVER installs an MSI.
  The generated .wsb mounts input read-only, results write-only from the sandbox
  perspective (host-writable results folder), with vGPU disabled; networking is needed for the Sandbox remote session.
#>
param(
 [string]$OutputRoot
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if(-not $OutputRoot){$OutputRoot=Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'CRAFT_GRIDCRAFT_V52_SANDBOX_QA_20261104'}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$env:DOTNET_ROOT=Join-Path $env:USERPROFILE '.dotnet'
$env:DOTNET_ROOT_X64=$env:DOTNET_ROOT
$wix=Join-Path $env:DOTNET_ROOT 'tools\wix.exe'
$csc=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319/csc.exe'
$icon=Join-Path $repo 'assets\app-icon\gridcraft.ico'
foreach($f in @($wix,$csc,$icon,(Join-Path $PSScriptRoot 'Run-SandboxAcceptance.ps1'))){
 if(!(Test-Path $f)){throw "Missing build prerequisite: $f"}
}
if(Test-Path $OutputRoot){throw "Refusing to overwrite existing QA root: $OutputRoot"}
$input=Join-Path $OutputRoot 'Input'
$results=Join-Path $OutputRoot 'Results'
$bin=Join-Path $OutputRoot 'Build\bin'
New-Item -ItemType Directory -Force -Path $input,$results,$bin|Out-Null
$source=Join-Path $OutputRoot 'Build\Stub.cs'
[IO.File]::WriteAllText($source,'public static class Program { public static void Main() { } }',[Text.UTF8Encoding]::new($false))
foreach($entry in @(@('gridcraft.exe','winexe'),@('gridcraft-cli.exe','exe'),
                 @('craft-opc-worker.exe','exe'),@('craft-xlsx-cms.exe','exe'))){
 & $csc /nologo /platform:x64 ("/target:"+$entry[1]) ("/out:"+(Join-Path $bin $entry[0])) $source
 if($LASTEXITCODE -ne 0){throw "Failed to compile fixture $($entry[0])"}
}
foreach($version in @('90.0.0','90.0.1','90.0.2')){
 $msi=Join-Path $input "fork-$version.msi"
 & $wix build (Join-Path $repo 'packaging\windows\gridcraft.wxs') -arch x64 -d "Version=$version" -d "BinDir=$bin" -d "IconPath=$icon" -o $msi
 if($LASTEXITCODE -ne 0){throw "WiX fork fixture failed: $version"}
 & (Join-Path $repo 'packaging\windows\verify-isolated-msi-identity.ps1') -Msi $msi -IdentityJson (Join-Path $repo 'packaging\windows\identity-v48.json') | Out-Null
 if($LASTEXITCODE -ne 0){throw "MSI identity check failed: $version"}
}
$legacy=Join-Path $input 'legacy-sentinel.msi'
& $wix build (Join-Path $repo 'packaging\windows\fixtures\legacy-gridcraft-sentinel.wxs') -arch x64 -d "LegacyExe=$(Join-Path $bin 'gridcraft.exe')" -o $legacy
if($LASTEXITCODE -ne 0){throw 'Legacy sentinel WiX build failed'}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Run-SandboxAcceptance.ps1') -Destination (Join-Path $input 'Run-SandboxAcceptance.ps1')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'bootstrap.cmd') -Destination (Join-Path $input 'bootstrap.cmd')
[IO.File]::WriteAllText((Join-Path $input 'SANDBOX_ONLY_MARKER.txt'),'GridCraft synthetic MSI acceptance. NEVER run on host.',[Text.UTF8Encoding]::new($false))
$pathIn='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Input'
$pathOut='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results'
$wsb=@"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Enable</Networking>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <PrinterRedirection>Disable</PrinterRedirection>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$input</HostFolder>
      <SandboxFolder>$pathIn</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$results</HostFolder>
      <SandboxFolder>$pathOut</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>cmd.exe /c $pathIn\bootstrap.cmd</Command>
  </LogonCommand>
</Configuration>
"@
$wsbPath=Join-Path $OutputRoot 'GridCraft-V52-Disposable-MSI-Acceptance.wsb'
[IO.File]::WriteAllText($wsbPath,$wsb,[Text.UTF8Encoding]::new($false))
[xml]$parsed=Get-Content $wsbPath -Raw
if($parsed.Configuration.VGpu -ne 'Disable' -or $parsed.Configuration.Networking -ne 'Enable'){throw 'Sandbox isolation settings incorrect'}
$manifest=@(Get-ChildItem $input -File|ForEach-Object{[ordered]@{file=$_.Name;sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash;bytes=$_.Length}})
[IO.File]::WriteAllText((Join-Path $OutputRoot 'INPUT_MANIFEST.json'),($manifest|ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
'V52_SYNTHETIC_MSI_FIXTURES_BUILT=PASS'
'V52_WINDOWS_SANDBOX_CONFIG='+$wsbPath
'V52_INPUT_MANIFEST='+ (Join-Path $OutputRoot 'INPUT_MANIFEST.json')
'V52_HOST_MSI_INSTALL=NEVER'
