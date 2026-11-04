# V52 — Disposable Windows MSI acceptance (2026-11-04)

**Overall status: BLOCKED / NOT ACCEPTED.** This document distinguishes verified offline MSI identity from real installation lifecycle acceptance.

## Scope

Independent `GridCraft (Craft Open Source)` MSI, side-by-side with a **synthetic** legacy GridCraft sentinel. No original application, user files, real B-Trust key, or production MSI was installed inside the test VM. The physical Lenovo installation was never modified.

## Completed — PASS

1. Four synthetic MSI fixtures built using local WiX 5.0.2 and .NET Framework C# stub binaries: legacy sentinel plus fork versions `90.0.0`, `90.0.1`, `90.0.2`.
2. Read-only MSI identity guard PASS on **all three** independent fork MSI packages.
3. Negative control PASS: the identity guard rejects the synthetic legacy MSI.
4. Two defects in `verify-isolated-msi-identity.ps1` fixed: Component table row access used `$_.Count` instead of `$_.Cells.Count`, and strict-mode Registry enumeration assumed every uninstall key had a `DisplayName` property.
5. Both V52 PowerShell scripts parse successfully.
6. V52 install script refuses to run on the physical Lenovo before any MSI operation: `HARD STOP: not Windows Sandbox user`.
7. Sandbox configuration maps only QA fixture inputs read-only and QA results writable, disables vGPU, audio/video input, clipboard, and printer redirection. Networking is enabled for the Windows Sandbox remote session; the QA scripts do not use network APIs.
8. The initial Sandbox run with networking enabled wrote a real receipt and returned **FAIL**, before any MSI install, because strict-mode enumeration again assumed all uninstall entries had `DisplayName`. The test reported `leftover_products=0`, `host_msi_install=false`. The defect has been fixed in the source. That failed receipt is preserved in `Results-Attempt1` in the local V52D QA directory.

## Not completed

- Subsequent Sandbox sessions lost their remote connection during guest startup. No successful full install/repair/upgrade/uninstall result has been obtained.
- The hosted GitHub Actions workflow was also blocked. The authenticated GitHub CLI returned **HTTP 422: Actions has been disabled for this repository** on dispatch. GitHub's read-only repository settings nevertheless report `enabled=true` and the Packaging lint workflow `state=active`. This contradiction requires account/platform-level investigation, not an invented CI PASS.
- **Do not claim V52 MSI lifecycle PASS.**

## Source and reproducibility

- `packaging/windows/sandbox/Build-SandboxFixtures.ps1` builds the four synthetic MSIs without installing anything on the host.
- `packaging/windows/sandbox/Run-SandboxAcceptance.ps1` checks actual install, legacy preservation, repair, major upgrade, uninstall, desktop shortcut opt-in/cleanup, and default file handler preservation, with exact-ProductCode cleanup.
- `packaging/windows/sandbox/bootstrap.cmd` captures the guest console output and exit status to the mapped results folder.
- `.github/workflows/packaging-lint.yml` contains a manually dispatchable, disposable Windows-hosted acceptance job on the isolated feature branch.
- The separate `.github/workflows/gridcraft-msi-lifecycle.yml` also exists on the feature branch, but is not registered on the repository default branch.

Local fixture evidence: `Documents\CRAFT_GRIDCRAFT_V52D_SANDBOX_QA_20261104\INPUT_MANIFEST.json`. The `Results-Attempt1` receipt contains the **expected FAIL** for the now-fixed strict-mode error. No personal documents or certificates are included in these QA fixtures.

## Release gates still outstanding

1. Obtain a **real PASS** for the disposable Windows MSI lifecycle, preferably on GitHub-hosted Windows after Actions dispatch is restored. Do not install the QA MSI on Lenovo to bypass the gate.
2. Verify the exact installer packaging and cleanup using the real release payload (not only no-op synthetic executables).
3. Validate licensing, branding, and publisher/legal entity. Current `Craft Open Source` manufacturer is a display label, not a verified legal identity.
4. Use a trusted, appropriate **Authenticode code-signing** certificate and timestamp for public MSI release. A personal B-Trust XLSX XML-DSig signature is **not** an Authenticode code-signing certificate.
5. Complete the release review and approve merge/release separately. No `main` merge or public release is authorized by this QA report.

## Already accepted, not repeated

V46 external Microsoft Excel formula/signature acceptance and V51 real B-Trust personal XLSX XML-DSig acceptance in Microsoft Excel were previously marked PASS in their own scopes. Neither was rerun during V52.

## File type registrations

The engine source implements `.xlsb` and `.ods` as **import-only** formats; these are offered via `Open with`, not as default handlers. They must not be advertised as writable formats in public release notes.
