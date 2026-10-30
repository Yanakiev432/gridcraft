# GridCraft V47 — Windows signing worker integration

Status: **private integration QA, not a release** (2026-10-30).

## What changed

The Windows WiX MSI and portable ZIP now contain all four executables:

- `gridcraft.exe` — spreadsheet GUI.
- `gridcraft-cli.exe` — CLI.
- `craft-opc-worker.exe` — isolated native OPC XML-DSig signer.
- `craft-xlsx-cms.exe` — detached CMS/PKCS#7 signer and verifier.

Both workers are resolved by GridCraft next to its own executable. In earlier development builds, helpers were copied manually into a test folder; the previous MSI recipe did not include them. This discrepancy is now addressed in the package recipe and the WiX installation components. Installation and uninstallation assertions on a disposable GitHub-hosted Windows runner also check both helpers.

## Deterministic inputs

A standalone clone now includes the source for both signing helpers: vendor/craft-signing-core (pinned provenance) and tools/detached-cms. Normal Windows packaging compiles these locally. Explicit worker paths may override the source build for controlled QA; missing or incompatible helpers fail closed.

Only the x64 native worker is accepted. Other architectures are rejected until corresponding native worker implementations have been built and reviewed.

For private QA without an Authenticode signing certificate, `-AllowUnsignedQa` must be explicit. Without this switch, production packaging refuses to proceed if no code signing credentials are configured. With credentials, all four staged executables and the MSI must pass Authenticode verification before the package is considered ready.

## Unresolved release gates

1. External Microsoft Excel acceptance of V46 with aggregates and cross-sheet dependencies (the V44 and V45 positive/negative Office tests are already recorded; **do not repeat them**).
2. A real authorized Windows certificate provider / B-Trust signing session and independent qualified-status evaluation. Never handle PINs in code or require private-key export.
3. Independent installation/repair/upgrade/uninstall verification of a distinct release candidate on a disposable machine. **Do not install the QA MSI on the user's Lenovo**.
4. Independent brand, publisher and MSI UpgradeCode identity review before public distribution. The existing WiX ProductName/UpgradeCode were inherited from the earlier GridCraft build and must not accidentally replace another installed GridCraft.
5. Authenticode-signed final binaries and installer; private unsigned QA outputs must not be distributed as a final release.
6. Comprehensive real-data XLSX compatibility and any outstanding application beta gates. Office signature success alone is not full product acceptance.

## Command for a private x64 installer build

```powershell
$env:GRIDCRAFT_OPC_WORKER_PATH = 'C:\Path\to\craft-opc-worker.exe'
$env:GRIDCRAFT_CMS_WORKER_PATH = 'C:\Path\to\craft-xlsx-cms.exe'
$env:DOTNET_ROOT = 'C:\Users\User\.dotnet'
$env:CARGO_NET_OFFLINE = 'true'
.\packaging\windows\package.ps1 -Arch x64 -AllowUnsignedQa
```

The QA package must remain isolated and clearly marked unsigned. The release path omits `-AllowUnsignedQa` and requires verified Authenticode signatures. The original installed Craft Suite and production data are untouched.
