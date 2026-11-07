# GridCraft 0.16.1 — isolated Windows signing QA MSI (2026-11-07)

This is a **reproducibility record for a private QA Draft Pre-release**, not a trusted production release. The QA MSI is **NOT installed** on Yanakiev-Lenovo.

## Installer design

- WiX Toolset 5.0.2, x64, using `packaging/windows/signing-qa.wxs`.
- ProductName: `GridCraft Signing QA (UNTRUSTED)`.
- Independent manufacturer metadata: `Craft Open Source QA (independent test build)`, **not** upstream vendor Learning Machines LLC.
- Separate, fixed QA UpgradeCode and installation root `Program Files\Craft Open Source QA\GridCraft Signing QA`; it cannot upgrade the installed official GridCraft.
- No file associations, no default apps registration, no shortcuts, no auto-launch, no hidden Windows settings.
- Included gridcraft.exe and gridcraft-cli.exe, the QA-only launcher scripts, anonymous unsigned test fixture, upstream licenses, NOTICE, ATTRIBUTION and QA manifest (11 files).
- Fresh installation has an explicit MSI LaunchCondition `Installed OR QA_INSTALL = "YES"`, so double-clicking the package cannot silently install it. The `Installed` allowance preserves normal uninstall/repair behavior if QA installation is explicitly authorized later.
- No code or extension-signing private keys are distributed in the MSI.

## Build / audit

On the authorized Lenovo with WiX 5 and local .NET runtime, build from the signed portable QA stage using `wix build packaging/windows/signing-qa.wxs -arch x64 -d Stage=<absolute-staged-folder> -o <qa.msi>`.

The resulting MSI and its two contained executable files are embedded-signed with a **locally self-signed software test certificate**. On Windows, `Get-AuthenticodeSignature` reports **UnknownError** and `signtool verify /pa` correctly returns nonzero: **NOT A PUBLICLY TRUSTED CODE SIGNATURE**.

On 2026-11-07 both MSI databases passed the read-only Windows Installer COM audit: correct product/manufacturer/UpgradeCode, 11 files, one GUI + one CLI, launch guard, independent install path, no file associations or shortcuts. Receipt on Lenovo: `MSI-QA-20261107/QA_MSI_READONLY_METADATA_PASS_20261107.json`.

## Separate signing tests

On 2026-11-07, targeted GUI Rust tests passed **2/2 for GridCraft**. The synthetic software-certificate DOCX/XLSX sign/verify/parse tests passed and left unsigned original fixtures unchanged. These do not establish actual B-Trust QES operation.

## Release gates still open

- No user-present B-Trust hardware PIN-flow GUI acceptance.
- No independent validation of a B-Trust-created document signature against EU Trusted Lists / qualified validation services.
- No publicly trusted Authenticode release identity for .exe/.msi.
- No user-approved production rollout.

Do not distribute or install this QA MSI as an official release. Microsoft Windows Installer launch-condition and public-property references:
https://learn.microsoft.com/en-us/windows/win32/msi/launchcondition-table
https://learn.microsoft.com/en-us/windows/win32/msi/command-line-options
