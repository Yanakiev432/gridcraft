# Windows Sandbox ONLY. Never installs on the physical Lenovo host.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
# Sandbox mapped folders use simple Desktop paths
$in='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Input'
$out='C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results'
$uninstall='HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$oldDir=Join-Path $env:ProgramFiles 'GridCraft'
$newDir=Join-Path $env:ProgramFiles 'GridCraft-CraftOS'
$oldLink=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'GridCraft.lnk'
$newLink=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'GridCraft (Craft Open Source).lnk'
$desktopLink=Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'GridCraft (Craft Open Source).lnk'
$oldApp='HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\gridcraft.exe'
$newApp='HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\gridcraft-craftos.exe'
$oldKey='HKLM:\Software\GridCraft\Installer'
$codes=@();$checks=New-Object System.Collections.Generic.List[string];$failure=$null
function Assert([bool]$ok,[string]$msg){if(!$ok){throw $msg}}
function Pass([string]$msg){$checks.Add($msg);Write-Host "PASS $msg"}
function MsiProperty([string]$msi,[string]$name){
 $wi=New-Object -ComObject WindowsInstaller.Installer;$db=$null;$view=$null;$rec=$null
 try{$db=$wi.OpenDatabase($msi,0);$view=$db.OpenView('SELECT * FROM Property');$null=$view.Execute()
  while($null -ne ($rec=$view.Fetch())){
   $k=$rec.StringData(1);$v=$rec.StringData(2)
   [Runtime.InteropServices.Marshal]::FinalReleaseComObject($rec)|Out-Null;$rec=$null
   if($k -eq $name){return $v}
  };throw "Missing $name in $msi"
 }finally{
  if($rec){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($rec)|Out-Null}
  if($view){$null=$view.Close();[Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)|Out-Null}
  if($db){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($db)|Out-Null}
  if($wi){[Runtime.InteropServices.Marshal]::FinalReleaseComObject($wi)|Out-Null}
 }
}
function Installed([string]$code){Test-Path (Join-Path $uninstall $code)}
function Msi([string]$step,[string[]]$args){
 $log=Join-Path $out ($step+'.log')
 $all=@($args)+@('/qn','/norestart','/l*v',('"' + $log + '"'))
 $p=Start-Process (Join-Path $env:WINDIR 'System32\msiexec.exe') -ArgumentList $all -PassThru
 try{if(!$p.WaitForExit(120000)){$p.Kill();throw "Timeout: $step"}
     if($p.ExitCode -ne 0){throw "MSI $step failed: $($p.ExitCode); $log"}
 }finally{$p.Dispose()}
}
function Default([string]$ext){
 $k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Classes\'+$ext)
 try{if(!$k){return '<absent>'};return [string]$k.GetValue('','<unset>')}finally{if($k){$k.Close()}}
}
function UserDefault([string]$ext){
 $k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\'+$ext+'\UserChoice')
 try{if(!$k){return '<absent>'};return [string]$k.GetValue('ProgId','<unset>')}finally{if($k){$k.Close()}}
}
function AssertOld([string]$sha){
 Assert (Test-Path $oldLink) 'Legacy shortcut removed'
 Assert (Test-Path $oldApp) 'Legacy App Paths removed'
 Assert ((Get-ItemPropertyValue -Path $oldKey -Name 'ProtectedLegacyFixture') -eq 'V49-DO-NOT-TOUCH') 'Legacy registry changed'
 Assert ((Get-FileHash (Join-Path $oldDir 'gridcraft.exe') -Algorithm SHA256).Hash -eq $sha) 'Legacy executable modified'
}
function AssertNew {
 foreach($exe in @('gridcraft.exe','gridcraft-cli.exe','craft-opc-worker.exe','craft-xlsx-cms.exe')){
  Assert (Test-Path (Join-Path $newDir $exe)) "Missing fork executable: $exe"
 }
 Assert (Test-Path $newLink) 'Fork shortcut missing'
 Assert (Test-Path $newApp) 'Fork App Paths missing'
}
function AssertDefaults([hashtable]$base){
 foreach($ext in @('.xlsx','.xlsm','.csv','.tsv')){
  Assert ((Default $ext) -ceq $base["HKLM:$ext"]) "HKLM file association changed: $ext"
  Assert ((UserDefault $ext) -ceq $base["HKCU:$ext"]) "HKCU default changed: $ext"
 }
}
$started=(Get-Date).ToString('o')
try{
 # Fail closed before ANY MSI writes if invoked on the real host.
 $system=Get-CimInstance Win32_ComputerSystem
 Assert ($env:USERNAME -eq 'WDAGUtilityAccount') 'HARD STOP: not Windows Sandbox user'
 Assert ($system.Manufacturer -eq 'Microsoft Corporation' -and $system.Model -match 'Virtual Machine') 'HARD STOP: not a Microsoft VM'
 Assert (Test-Path (Join-Path $in 'SANDBOX_ONLY_MARKER.txt')) 'HARD STOP: input mount absent'
 Assert (Test-Path $out) 'HARD STOP: results mount absent'
 foreach($p in @($oldDir,$newDir,$oldLink,$newLink,$desktopLink,$oldKey,$oldApp,$newApp,'HKLM:\Software\CraftOpenSource\GridCraft')){
  Assert (!(Test-Path $p)) "HARD STOP: existing GridCraft state at $p"
 }
 $existing=@(Get-ItemProperty "$uninstall\*", 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue|Where-Object{$_.PSObject.Properties['DisplayName'] -and $_.PSObject.Properties['DisplayName'].Value -match '^GridCraft($|\s|\()'})
 Assert ($existing.Count -eq 0) 'HARD STOP: real GridCraft installed'
 $legacy=Join-Path $in 'legacy-sentinel.msi'
 $fork0=Join-Path $in 'fork-90.0.0.msi'
 $fork1=Join-Path $in 'fork-90.0.1.msi'
 $fork2=Join-Path $in 'fork-90.0.2.msi'
 foreach($f in @($legacy,$fork0,$fork1,$fork2)){Assert (Test-Path $f) "Missing MSI fixture: $f"}
 $oldCode=MsiProperty $legacy 'ProductCode'
 $code0=MsiProperty $fork0 'ProductCode'
 $code1=MsiProperty $fork1 'ProductCode'
 $code2=MsiProperty $fork2 'ProductCode'
 $codes=@($code2,$code1,$code0,$oldCode)
 Assert (@($codes|Select-Object -Unique).Count -eq 4) 'MSI ProductCode collision'
 $oldUpgrade=MsiProperty $legacy 'UpgradeCode'
 foreach($f in @($fork0,$fork1,$fork2)){Assert ((MsiProperty $f 'UpgradeCode') -ne $oldUpgrade) 'Legacy UpgradeCode collision'}
 Pass 'Independent MSI identity'
 $defaults=@{}
 foreach($ext in @('.xlsx','.xlsm','.csv','.tsv')){$defaults["HKLM:$ext"]=Default $ext;$defaults["HKCU:$ext"]=UserDefault $ext}
 Msi '01-legacy-install' @('/i',('"' + $legacy + '"'))
 Assert (Installed $oldCode) 'Legacy install not registered'
 $sha=(Get-FileHash (Join-Path $oldDir 'gridcraft.exe') -Algorithm SHA256).Hash
 AssertOld $sha;Pass 'Legacy sentinel installed'
 Msi '02-fork-install' @('/i',('"' + $fork0 + '"'),'DESKTOPSHORTCUT=0')
 Assert (Installed $code0) 'Fork not installed'
 Assert (Installed $oldCode) 'Fork install removed legacy'
 AssertOld $sha;AssertNew;Assert (!(Test-Path $desktopLink)) 'Unexpected desktop shortcut'
 AssertDefaults $defaults;Pass 'Side-by-side installation and default handlers'
 Msi '03-fork-repair' @('/famus',$code0)
 Assert (Installed $code0) 'Repair removed fork'
 AssertOld $sha;AssertNew;AssertDefaults $defaults;Pass 'Repair isolation'
 Msi '04-fork-upgrade' @('/i',('"' + $fork1 + '"'),'DESKTOPSHORTCUT=0')
 Assert (Installed $code1) 'New fork version missing'
 Assert (!(Installed $code0)) 'Old fork version still installed'
 Assert (Installed $oldCode) 'Upgrade removed legacy'
 AssertOld $sha;AssertNew;AssertDefaults $defaults;Pass 'Independent major upgrade'
 Msi '05-fork-uninstall' @('/x',$code1)
 Assert (!(Installed $code1)) 'Fork uninstall incomplete'
 Assert (Installed $oldCode) 'Fork uninstall removed legacy'
 AssertOld $sha;Assert (!(Test-Path $newLink)) 'Fork shortcut remains'
 Assert (!(Test-Path $newApp)) 'Fork App Paths remains'
 AssertDefaults $defaults;Pass 'Uninstall preserves legacy'
 Msi '06-fork-desktop-optin' @('/i',('"' + $fork2 + '"'),'DESKTOPSHORTCUT=1')
 Assert (Installed $code2) 'Desktop opt-in install missing'
 Assert (Test-Path $desktopLink) 'Desktop shortcut not created'
 AssertOld $sha;AssertDefaults $defaults;Pass 'Optional desktop shortcut'
 Msi '07-fork-desktop-remove' @('/x',$code2)
 Assert (!(Installed $code2)) 'Desktop opt-in uninstall incomplete'
 Assert (!(Test-Path $desktopLink)) 'Desktop shortcut not removed'
 AssertOld $sha;AssertDefaults $defaults;Pass 'Desktop shortcut cleanup'
 Msi '08-legacy-remove' @('/x',$oldCode)
 Assert (!(Installed $oldCode)) 'Legacy fixture uninstall incomplete'
 AssertDefaults $defaults;Pass 'Fixture cleanup'
}catch{$failure=$_.Exception.Message;Write-Host "FAIL $failure"}
finally{
 foreach($code in $codes){try{if(Installed $code){Msi ('cleanup-'+$code.Trim('{}')) @('/x',$code)}}catch{if(!$failure){$failure="Cleanup failed: $($_.Exception.Message)"}}}
 $left=@($codes|Where-Object{Installed $_})
 if($left.Count){$failure="CRITICAL: $($left.Count) fixture product(s) left installed"}
 if(Test-Path $out){
  $receipt=[ordered]@{schema='gridcraft.v52.sandbox-acceptance.v1';started=$started;finished=(Get-Date).ToString('o');pass=(!$failure);checks=@($checks.ToArray());failure=$failure;leftover_products=$left.Count;host_msi_install=$false}
  [IO.File]::WriteAllText((Join-Path $out 'sandbox-acceptance.json'),($receipt|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
 }
}
if($failure){throw $failure}
'V52_WINDOWS_SANDBOX_MSI_LIFECYCLE=PASS'
