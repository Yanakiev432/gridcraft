<#
Offline structure check only: do not install an MSI or execute its payload.
Required signing helpers must be present in BOTH MSI and portable ZIP.
#>
param(
  [Parameter(Mandatory=$true)][string]$Msi,
  [Parameter(Mandatory=$true)][string]$Portable
)
$ErrorActionPreference='Stop'
$required=@('gridcraft.exe','gridcraft-cli.exe','craft-opc-worker.exe','craft-xlsx-cms.exe')
foreach($path in @($Msi,$Portable)) {
  if(!(Test-Path -LiteralPath $path -PathType Leaf)){throw "Required package missing: $path"}
}
$installer=$null
$db=$null
$view=$null
$record=$null
try {
  $installer=New-Object -ComObject WindowsInstaller.Installer
  $db=$installer.OpenDatabase($Msi,0)
  $view=$db.OpenView('SELECT * FROM File')
  $view.Execute()
  $names=@()
  while($null -ne ($record=$view.Fetch())){
    $field=$record.StringData(3)
    $names+=($field -split '\|')[-1]
    [Runtime.InteropServices.Marshal]::ReleaseComObject($record)|Out-Null
    $record=$null
  }
  foreach($name in $required){
    if($names -notcontains $name) {throw "MSI File table missing $name"}
  }
}
finally {
  if($record){[Runtime.InteropServices.Marshal]::ReleaseComObject($record)|Out-Null}
  if($view){$view.Close();[Runtime.InteropServices.Marshal]::ReleaseComObject($view)|Out-Null}
  if($db){[Runtime.InteropServices.Marshal]::ReleaseComObject($db)|Out-Null}
  if($installer){[Runtime.InteropServices.Marshal]::ReleaseComObject($installer)|Out-Null}
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($Portable)
try {
  $entries=@($zip.Entries|ForEach-Object{$_.FullName.Replace('\','/')})
  foreach($name in $required){
    if(!($entries|Where-Object{$_ -ieq $name -or $_ -ilike "*/$name"})){
      throw "Portable ZIP missing $name"
    }
  }
}
finally {$zip.Dispose()}
[pscustomobject]@{
  schema='craft.gridcraft.windows.signer-payload.qa'
  status='PASS'
  installation_performed=$false
  required_executables=$required
  msi_file_table_valid=$true
  portable_zip_entries_valid=$true
  msi_sha256=(Get-FileHash -LiteralPath $Msi -Algorithm SHA256).Hash
  zip_sha256=(Get-FileHash -LiteralPath $Portable -Algorithm SHA256).Hash
}|ConvertTo-Json -Depth 5
