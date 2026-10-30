<#
V48 independent MSI identity audit. Read-only: NEVER installs or executes MSI.
Checks the source-independent WiX result for collisions with original GridCraft.
#>
param(
  [Parameter(Mandatory=$true)][string]$Msi,
  [Parameter(Mandatory=$true)][string]$IdentityJson
)
$ErrorActionPreference='Stop'
if(!(Test-Path $Msi -PathType Leaf)){throw 'MSI input absent'}
if(!(Test-Path $IdentityJson -PathType Leaf)){throw 'Identity spec absent'}
$spec=Get-Content -LiteralPath $IdentityJson -Raw|ConvertFrom-Json
$wi=$null;$db=$null
function Fetch-All([string] $Table) {
  $v=$db.OpenView("SELECT * FROM $Table")
  $rows=@()
  try {
    $null=$v.Execute()
    while($null -ne ($rec=$v.Fetch())) {
      try {
        $fields=@()
                $columnCount=switch($Table) {
          'Property' {2}
          'Directory' {3}
          'Registry' {6}
          'Shortcut' {12}
          'Component' {6}
          default {throw "Unrecognized MSI table: $Table"}
        }
        for($i=1;$i -le $columnCount;$i++){$fields+= $rec.StringData($i)}
        $rows += [pscustomobject]@{Cells = [string[]]$fields}
      }
      finally{[Runtime.InteropServices.Marshal]::ReleaseComObject($rec)|Out-Null}
    }
  }
  finally{$null=$v.Close();[Runtime.InteropServices.Marshal]::ReleaseComObject($v)|Out-Null}
  return $rows
}
try {
  $wi=New-Object -ComObject WindowsInstaller.Installer
  $db=$wi.OpenDatabase($Msi,0)
  $props=@{}
  foreach($row in (Fetch-All 'Property')) { $props[$row.Cells[0]]=$row.Cells[1] }
  $guid=$props['UpgradeCode'].Trim('{}').ToUpperInvariant()
  if($guid -ne $spec.msi_upgrade_code){throw "UpgradeCode unexpected: $guid"}
  if($guid -eq $spec.original_msi_upgrade_code_do_not_reuse){throw 'This MSI would upgrade the original GridCraft!'}
  if($props['ProductName'] -ne $spec.display_name){throw "ProductName collision or mismatch: $($props['ProductName'])"}
  if($props['Manufacturer'] -ne $spec.manufacturer_display_only){throw 'Unreviewed publisher label'}
  $dirs=@(Fetch-All 'Directory')
  $match=@($dirs|Where-Object{$_.Cells.Count -ge 3 -and $_.Cells[0] -eq 'INSTALLFOLDER'})
  if($match.Count -ne 1 -or (($match[0].Cells[2] -split '\|')[-1]) -ne $spec.install_folder) {
    throw "INSTALLFOLDER collision or mismatch"
  }
  $regs=@(Fetch-All 'Registry')
  $oldKeys=@($regs|Where-Object{ $_.Cells.Count -ge 3 -and $_.Cells[2] -like 'Software\GridCraft\*' })
  if($oldKeys.Count -gt 0){throw 'Legacy GridCraft registry keys present'}
  $oldAppPaths=@($regs|Where-Object{ $_.Cells.Count -ge 3 -and $_.Cells[2] -like '*App Paths\gridcraft.exe' })
  if($oldAppPaths.Count -gt 0){throw 'Global original GridCraft App Paths overwritten'}
  $legacyProg=@($regs|Where-Object{ $_.Cells.Count -ge 3 -and ($_.Cells[3] -eq 'GridCraft.Workbook' -or $_.Cells[3] -eq 'GridCraft.TextData') })
  if($legacyProg.Count -gt 0){throw 'Legacy OpenWith ProgIds overwritten'}
  $alias=@($regs|Where-Object{ $_.Cells.Count -ge 3 -and $_.Cells[2] -like "*App Paths\$($spec.app_paths_alias)" })
  if($alias.Count -ne 1){throw 'Missing independent App Paths alias'}
  $indep=@($regs|Where-Object{ $_.Cells.Count -ge 3 -and $_.Cells[2] -like "$($spec.registry_prefix)*" })
  if($indep.Count -lt 5){throw 'Independent registry namespace missing'}
  $shortcuts=@(Fetch-All 'Shortcut')
  $shortcutNames=@($shortcuts|Where-Object { $_.Cells.Count -ge 3 }|ForEach-Object { $_.Cells[2] })
  if(@($shortcutNames|Where-Object {$_ -eq 'GridCraft'}).Count -gt 0){throw 'Legacy shortcut name would collide'}
  if(@($shortcutNames|Where-Object {$_ -like '*Craft Open Source*'}).Count -lt 1) {
    throw 'Independent start-menu/desktop shortcut absent'
  }
  $components=@(Fetch-All 'Component')
  $oldComponent=@($components|Where-Object {$_.Count -ge 2 -and $_.Cells[1].Trim('{}') -eq $spec.original_app_component_guid_do_not_reuse})
  if($oldComponent.Count){throw 'Legacy fixed component GUID was reused'}
  $installed=@(Get-ItemProperty 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue|Where-Object {$_.DisplayName -eq 'GridCraft'})
  if($installed.Count -gt 0 -and @($installed|Where-Object {$_.PSChildName -ieq $props['ProductCode']}).Count -gt 0) {
    throw 'CRITICAL: MSI ProductCode collides with installed original GridCraft'
  }
  [ordered]@{
    schema='craft.gridcraft.v48.windows.identity.read-only'
    separate_product_code=$props['ProductCode']
    status='PASS'
    old_GridCraft_installation_detected=($installed.Count -gt 0)
    old_GridCraft_uninstall_product_code=if($installed.Count -gt 0){$installed[0].PSChildName}else{$null}
    separate_upgrade_code=$guid
    isolated_install_folder=(($match[0].Cells[2] -split '\|')[-1])
    isolated_product_name=$props['ProductName']
    independent_app_paths=$spec.app_paths_alias
    unique_shortcuts=$true
    original_legacy_registry_not_overwritten=$true
    original_component_guid_not_reused=$true
    msi_sha256=(Get-FileHash -LiteralPath $Msi -Algorithm SHA256).Hash
    installation_performed=$false
    upgraded_existing_product=$false
  }|ConvertTo-Json -Depth 5
}
finally{
  if($db){[Runtime.InteropServices.Marshal]::ReleaseComObject($db)|Out-Null}
  if($wi){[Runtime.InteropServices.Marshal]::ReleaseComObject($wi)|Out-Null}
}
