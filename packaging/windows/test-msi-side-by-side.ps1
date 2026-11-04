<#
V49 new acceptance: disposable GitHub-hosted Windows runner ONLY.
Tests MSI side-by-side with a simulated historical GridCraft installer.
NEVER run on Yanakiev-Lenovo or any machine with real GridCraft installed.
#>
param([string]$WorkDir)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or
   $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP){
   throw 'HARD STOP: MSI writes only on disposable GitHub-hosted Windows runner'
}
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(!$WorkDir){$WorkDir=Join-Path $env:RUNNER_TEMP ('gridcraft-v49-isolation-'+[Guid]::NewGuid().ToString('N'))}
$bin=Join-Path $WorkDir 'bin'
$oldDir=Join-Path $env:ProgramFiles 'GridCraft'
$newDir=Join-Path $env:ProgramFiles 'GridCraft-CraftOS'
$oldLink=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'GridCraft.lnk'
$newLink=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'GridCraft (Craft Open Source).lnk'
$oldKey='HKLM:\Software\GridCraft\Installer'
$uninst='HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
foreach($p in @($oldDir,$newDir,$oldLink,$newLink,$oldKey,'HKLM:\Software\CraftOpenSource\GridCraft')){
 if(Test-Path $p){throw "HARD STOP: existing product/setting at $p"}
}
$existing=@(Get-ItemProperty "$uninst\*",'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object {$_.DisplayName -match '^GridCraft($|\s|\()'})
if($existing.Count){throw 'HARD STOP: real GridCraft already installed on this runner'}
New-Item -ItemType Directory -Force -Path $bin|Out-Null
$source=Join-Path $WorkDir 'Stub.cs'
[IO.File]::WriteAllText($source,'public static class Program { public static void Main() { } }',[Text.UTF8Encoding]::new($false))
$csc=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
foreach($entry in @(@('gridcraft.exe','winexe'),@('gridcraft-cli.exe','exe'),
                    @('craft-opc-worker.exe','exe'),@('craft-xlsx-cms.exe','exe'))){
 & $csc /nologo /platform:x64 ("/target:"+$entry[1]) ("/out:"+(Join-Path $bin $entry[0])) $source
 if($LASTEXITCODE -ne 0){throw "Stub executable creation failed: $($entry[0])"}
}
$legacy=Join-Path $WorkDir 'legacy-sentinel.msi'
$fork=Join-Path $WorkDir 'independent-fork.msi'
& wix build (Join-Path $PSScriptRoot 'fixtures\legacy-gridcraft-sentinel.wxs') -arch x64 -d "LegacyExe=$(Join-Path $bin 'gridcraft.exe')" -o $legacy
if($LASTEXITCODE -ne 0){throw 'Synthetic legacy MSI build failed'}
& wix build (Join-Path $PSScriptRoot 'gridcraft.wxs') -arch x64 -d 'Version=90.0.0' -d "BinDir=$bin" -d "IconPath=$(Join-Path $root 'assets\app-icon\gridcraft.ico')" -o $fork
if($LASTEXITCODE -ne 0){throw 'Synthetic independent MSI build failed'}
& (Join-Path $PSScriptRoot 'verify-isolated-msi-identity.ps1') -Msi $fork -IdentityJson (Join-Path $PSScriptRoot 'identity-v48.json') | Out-Null

function Product-Code([string]$path) {
 $wi=New-Object -ComObject WindowsInstaller.Installer
 $db=$null;$view=$null;$rec=$null
 try{
  $db=$wi.OpenDatabase($path,0)
  $view=$db.OpenView('SELECT * FROM Property')
  $null=$view.Execute()
  while($null -ne ($rec=$view.Fetch())){
   $key=$rec.StringData(1);$value=$rec.StringData(2)
   [Runtime.InteropServices.Marshal]::FinalReleaseComObject($rec)|Out-Null;$rec=$null
   if($key -eq 'ProductCode'){return $value}
  }
  throw 'MSI ProductCode missing'
 }finally{
  if($rec){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($rec)|Out-Null}
  if($view){$null=$view.Close();[Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)|Out-Null}
  if($db){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($db)|Out-Null}
  if($wi){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($wi)|Out-Null}
 }
}
function Require([bool]$yes,[string]$message){if(-not $yes){throw $message}}
function IsInstalled([string]$code){Test-Path (Join-Path $uninst $code)}
function Invoke-Msi([string]$name,[string[]]$args) {
 $log=Join-Path $WorkDir ($name+'.log')
 $all=@($args)+@('/qn','/norestart','/l*v',('"' + $log + '"'))
 $p=Start-Process (Join-Path $env:WINDIR 'System32\msiexec.exe') -ArgumentList $all -PassThru
 try{
  if(-not $p.WaitForExit(120000)){$p.Kill();throw "MSI $name timeout"}
  if($p.ExitCode -ne 0){throw "MSI $name failed: $($p.ExitCode); log $log"}
 }finally{$p.Dispose()}
}
$oldCode=Product-Code $legacy
$newCode=Product-Code $fork
Require ($oldCode -ne $newCode) 'CRITICAL: MSI product codes collide'
$oldInstalled=$false;$newInstalled=$false;$errorRecord=$null
try {
 Invoke-Msi 'legacy-install' @('/i',('"' + $legacy + '"'))
 $oldInstalled=$true
 Require (IsInstalled $oldCode) 'Legacy fixture registration missing'
 $oldExe=Join-Path $oldDir 'gridcraft.exe'
 Require (Test-Path $oldExe) 'Legacy fixture executable missing'
 Require (Test-Path $oldLink) 'Legacy fixture Start Menu shortcut missing'
 Require (Test-Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\gridcraft.exe') 'Legacy App Paths missing'
 $sha=(Get-FileHash $oldExe -Algorithm SHA256).Hash
 $marker=Get-ItemPropertyValue -Path $oldKey -Name 'ProtectedLegacyFixture'
 Require ($marker -eq 'V49-DO-NOT-TOUCH') 'Legacy marker missing'
 Invoke-Msi 'fork-install' @('/i',('"' + $fork + '"'),'DESKTOPSHORTCUT=0')
 $newInstalled=$true
 Require (IsInstalled $newCode) 'Fork product registration missing'
 Require (IsInstalled $oldCode) 'CRITICAL: fork upgrade removed original installation'
 Require ((Get-FileHash $oldExe -Algorithm SHA256).Hash -eq $sha) 'CRITICAL: original binary changed'
 Require ((Get-ItemPropertyValue -Path $oldKey -Name 'ProtectedLegacyFixture') -eq $marker) 'CRITICAL: original registry overwritten'
 Require (Test-Path $oldLink) 'CRITICAL: original shortcut removed'
 Require (Test-Path $newLink) 'New fork shortcut missing'
 Require (Test-Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\gridcraft-craftos.exe') 'Independent App Paths missing'
 foreach($exe in @('gridcraft.exe','gridcraft-cli.exe','craft-opc-worker.exe','craft-xlsx-cms.exe')){
  Require (Test-Path (Join-Path $newDir $exe)) "MSI installed without $exe"
 }
 Invoke-Msi 'fork-uninstall' @('/x',$newCode)
 $newInstalled=$false
 Require (-not (IsInstalled $newCode)) 'Fork uninstall incomplete'
 Require (IsInstalled $oldCode) 'CRITICAL: uninstall removed original installation'
 Require ((Get-FileHash $oldExe -Algorithm SHA256).Hash -eq $sha) 'CRITICAL: original binary altered after uninstall'
 Require ((Get-ItemPropertyValue -Path $oldKey -Name 'ProtectedLegacyFixture') -eq $marker) 'CRITICAL: original registry altered after uninstall'
 Require (Test-Path $oldLink) 'CRITICAL: original shortcut removed by fork uninstall'
 Require (-not (Test-Path $newLink)) 'Fork shortcut not removed'
 Require (-not (Test-Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\gridcraft-craftos.exe')) 'Fork App Paths not removed'
 'V49_SIDE_BY_SIDE_INSTALL_AND_UNINSTALL=PASS'
 'V49_ORIGINAL_GRIDCRAFT_PRESERVED=PASS'
}catch{$errorRecord=$_}
finally{
 if(IsInstalled $newCode){try{Invoke-Msi 'cleanup-fork' @('/x',$newCode)}catch{if(!$errorRecord){$errorRecord=$_}}}
 if(IsInstalled $oldCode){try{Invoke-Msi 'cleanup-legacy' @('/x',$oldCode)}catch{if(!$errorRecord){$errorRecord=$_}}}
}
if($errorRecord){throw $errorRecord}
Require (-not (IsInstalled $newCode)) 'Fork cleanup incomplete'
Require (-not (IsInstalled $oldCode)) 'Legacy cleanup incomplete'
'V49_DISPOSABLE_TEST_FIXTURES_REMOVED=PASS'
