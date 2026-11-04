@echo off
setlocal
echo BOOTSTRAP_STARTED > "C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results\bootstrap.txt"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Input\Run-SandboxAcceptance.ps1" > "C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results\console.txt" 2>&1
echo %ERRORLEVEL% > "C:\Users\WDAGUtilityAccount\Desktop\GridCraftQA-Results\exit.txt"
