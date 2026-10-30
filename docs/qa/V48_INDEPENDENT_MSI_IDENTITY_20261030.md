# GridCraft V48 — independent MSI identity, read-only acceptance

**Date:** 2026-10-30
**Scope:** Windows x64 independent packaging; no changes to workbook signing algorithms, Office V44–V46 evidence, GUI semantics, or installed applications.

## Problem

V47 correctly included the two signing helper executables but inherited the original GridCraft MSI's `UpgradeCode`, publisher string, install directory, shell shortcuts, and Windows registry registrations. On the Windows test machine the original `GridCraft 0.8.0` by `Learning Machines LLC` was already installed. Using the same upgrade code would create a risk of silently upgrading or removing the existing application.

## Independent package identity

The isolated QA edition is `GridCraft (Craft Open Source)`. The string `Craft Open Source` is a **project/edition display label, not an established legal entity or a validated certificate subject**.

| Field | Original | V48 independent edition |
| --- | --- | --- |
| Product name | GridCraft | GridCraft (Craft Open Source) |
| MSI UpgradeCode | `5511327B-F305-41BE-92D0-EB84944E7C4D` | `F4724585-B6F6-5A6B-8A6B-D1F31AC9A52F` |
| QA ProductCode | Installed `{B7034F48-84D1-4FC1-AE2E-8AF00DA41BC7}` | `{68D8E480-E4A8-4CFC-AB4B-E6154D427774}` |
| Program Files directory | GridCraft | GridCraft-CraftOS |
| Start Menu and optional Desktop shortcut | GridCraft | GridCraft (Craft Open Source) |
| App Paths entry | gridcraft.exe | gridcraft-craftos.exe |
| ProgIds | GridCraft.Workbook, GridCraft.TextData | CraftOpenSource.GridCraft.Workbook, CraftOpenSource.GridCraft.TextData |
| Installer registry namespace | Software\GridCraft | Software\CraftOpenSource\GridCraft |

The fixed component GUID for the application was also changed for the independent directory.

The MSI and portable ZIP still contain all four executable files, including `craft-opc-worker.exe` and `craft-xlsx-cms.exe`; they were generated with existing known V47 release binaries by `-SkipBuild`. No application recompilation or Excel tests were repeated.

## Acceptance

**Read-only MSI structure: PASS**

- Separate fixed upgrade code and auto-generated product code.
- Separate installation directory and both shortcuts.
- No legacy `Software\GridCraft\...` registry writes.
- No legacy app-path or ProgId overwrite.
- Existing original GridCraft detected before and after the package creation.
- `packaging/windows/verify-isolated-msi-identity.ps1` reads MSI tables; it does **not** install MSI.
- Old V47 MSI deliberately rejected by the V48 identity guard (negative control): `UpgradeCode unexpected: 5511327B-F305-41BE-92D0-EB84944E7C4D`.
- `package.ps1` now invokes the isolated-identity guard on every new MSI.
- `packaging/windows/verify-signing-payload.ps1` confirms all four executable files in both package formats.

**QA files on Lenovo:**
`C:\Users\User\Documents\CRAFT_GRIDCRAFT_V48_ISOLATED_INSTALLER_QA_20261030`

**SHA-256:**
- MSI: `FA08CB350F7E0CA2FC95ABDAA243085CC580EFAEE79489C14BA445F74F592A4D`
- Portable ZIP: `AA8D80AF5B573E2653A6E57144C52B781FDD7A3FFB589661D577275929FB510A`

## Release gate

**NO GO:** both QA artifacts are unsigned and no end-to-end install/uninstall sequence has yet been run on a disposable Windows host. Original app remains installed and untouched on Lenovo. Real Authenticode certificate and legal publisher are not established. The app's inherited runtime about/asset metadata require separate rights and brand review before public distribution. Existing Excel V44/V45 acceptance is already recorded; V46 external Excel acceptance remains pending without re-running previously accepted checks.

Do not rename the independent `UpgradeCode` back to the original value, modify the original GridCraft MSI, install the QA MSI on Lenovo, or silently switch users' default XLSX handlers.
