<#
.SYNOPSIS
  Prepare or launch a disposable Windows Sandbox MSI acceptance session.
.DESCRIPTION
  Run from the root of the extracted V52 QA bundle, next to Input/.
  Never installs an MSI on the host. The sandbox runs only synthetic fixtures.
  -PrepareOnly creates the .wsb file without starting Windows Sandbox.
#>
param([switch]$PrepareOnly)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$input=Join-Path $PSScriptRoot 'Input'
$results=Join-Path $PSScriptRoot 'Results'
$wsb=Join-Path $PSScriptRoot 'GridCraft-QA-Disposable.wsb'
if(!(Test-Path $input -PathType Container)){throw 'Missing Input directory. Extract the complete QA ZIP first.'}
foreach($name in @('legacy-sentinel.msi','fork-90.0.0.msi','fork-90.0.1.msi','fork-90.0.2.msi',
                    'Run-SandboxAcceptance.ps1','bootstrap.cmd','SANDBOX_ONLY_MARKER.txt')){
 if(!(Test-Path (Join-Path $input $name) -PathType Leaf)){throw "QA fixture missing: $name"}
}
if(Test-Path (Join-Path $results 'sandbox-acceptance.json')){
 throw 'Previous acceptance receipt exists. Archive the Results directory before starting another run.'
}
if(!(Test-Path $results)){New-Item -ItemType Directory $results|Out-Null}
$inXml=[Security.SecurityElement]::Escape((Resolve-Path $input).Path)
$outXml=[Security.SecurityElement]::Escape((Resolve-Path $results).Path)
$guestIn='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Input'
$guestOut='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results'
$config=@"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Enable</Networking>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <PrinterRedirection>Disable</PrinterRedirection>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$inXml</HostFolder>
      <SandboxFolder>$guestIn</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$outXml</HostFolder>
      <SandboxFolder>$guestOut</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>cmd.exe /c $guestIn\bootstrap.cmd</Command>
  </LogonCommand>
</Configuration>
"@
[IO.File]::WriteAllText($wsb,$config,[Text.UTF8Encoding]::new($false))
[xml]$xml=Get-Content $wsb -Raw
if($xml.Configuration.VGpu -ne 'Disable' -or
   $xml.Configuration.MappedFolders.MappedFolder[0].ReadOnly -ne 'true' -or
   $xml.Configuration.MappedFolders.MappedFolder[1].ReadOnly -ne 'false'){
 throw 'Sandbox security settings invalid'
}
'GRIDCRAFT_QA_SANDBOX_CONFIG='+$wsb
'GRIDCRAFT_QA_RESULTS='+$results
'HOST_MSI_INSTALL=NEVER'
if($PrepareOnly){'PREPARE_ONLY=PASS';return}
$sandbox=Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
if(!(Test-Path $sandbox)){throw 'Windows Sandbox not available. Windows 11 Pro/Enterprise and enabled Windows Sandbox are required.'}
if(Get-Process WindowsSandboxRemoteSession -ErrorAction SilentlyContinue){
 throw 'A Windows Sandbox session is already open. Close it before starting this QA bundle.'
}
Start-Process -FilePath $sandbox -ArgumentList ('"' + $wsb + '"') | Out-Null
'SANDBOX_LAUNCHED=YES'
'WAIT_FOR_RESULT=Results\sandbox-acceptance.json'
