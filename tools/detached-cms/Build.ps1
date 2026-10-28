$ErrorActionPreference='Stop'
$dir=Split-Path -Parent $MyInvocation.MyCommand.Path
$source=Join-Path $dir 'CraftXlsxCms.cs'
$output=Join-Path $dir 'craft-xlsx-cms.exe'
$compiler='C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if(!(Test-Path $compiler -PathType Leaf)){throw 'Requires Windows .NET Framework 4.x C# compiler'}
if(!(Test-Path $source -PathType Leaf)){throw 'Missing detached CMS source'}
& $compiler /nologo /target:exe /optimize+ ("/out:"+$output) /r:System.Security.dll $source
if($LASTEXITCODE -ne 0){throw 'CMS helper compilation failed'}
$hash=(Get-FileHash $output -Algorithm SHA256).Hash
Write-Output "CRAFT_CMS_HELPER_BUILD=PASS"
Write-Output "HELPER_SHA256=$hash"
Write-Output "SIGN_MODE=STANDARD_CMS_DETACHED_PKCS7"
Write-Output "NOT_AUTHENTICODE; TRUST_NOT_EVALUATED; QES_NOT_EVALUATED"
