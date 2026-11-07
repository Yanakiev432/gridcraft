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

## External XAdES owner-present bridge state (2026-11-07)

The separate European XAdES/ASiC-S signing source is now in the craft-signing-core QA branch:
https://github.com/Yanakiev432/craft-signing-core/tree/qa/dss-asic-xades-20261107/experimental/dss-asic-xades

The EC DSS 6.5.1 bridge generates a separate .asics container and exchanges raw SignedInfo data with a .NET 8 sidecar. Disposable PKCS#12 test certificates and a strictly restricted self-issued Windows RSACng software certificate have passed DOCX/XLSX sign/verify and negative tampering checks. The Windows local software signer has passed certificate selection and output-collision guardrails.

The owner-present B-Trust hardware QA mode is compiled and has passed read-only public certificate policy/chain checks for two candidate certificates. Its opt-in Lenovo scripts require Windows user certificate selection, a second explicit confirmation, and provider-native PIN handling. **No real B-Trust private-key signing has yet been performed.** No EU-qualified validation report, trusted timestamp or eIDAS QES verdict exists.

**Do not conflate** native Office OPC/XMLDSig inside DOCX/XLSX with external XAdES Baseline-B in ASiC-S. The current GridCraft QA GUI/portable ZIP/MSI contains the previous native OPC signature feature only; the DSS sidecar and B-Trust manual launchers are **NOT** bundled in that installer. The external ASiC mode is still under independent QA and requires separate GUI/installer integration after owner-present acceptance. Official installed products remain unchanged.
