# gridcraft Windows GUI Office signature integration — QA checkpoint (2026-11-07)

Status: **DESKTOP EXECUTABLE BUILD, STARTUP AND REVIEW-RIBBON VISUAL QA PASS; B-TRUST SIGNING AND TRUSTED PRODUCTION MSI NOT YET VERIFIED**.

## Revision under test

- Based on exact official upstream release pinned in this QA branch.
- Signature core: `Yanakiev432/craft-signing-core` v0.1.0 at `7c8dd0f0cee6068094520a43fca309fb8ff48bc1`.
- New native Windows UI module: `crates/ui-egui/src/office_sign_ui.rs`.
- Signature controls are in the **Review** ribbon, only on Windows.
- Existing proprietary Windows private-key provider and PIN UI remain responsible for key operations. No PIN is collected, logged or entered into Craft.
- GUI starts the matching bundled CLI executable as a child process, which calls the pinned OPC signer. It polls the process, supports cancellation and a fixed 180-second timeout; cancellation targets the entire Windows child process tree. Never silently assume interrupted signing succeeded.
- Save-before-sign preflight: source must be a saved DOCX/XLSX, not dirty; destination must be an absolute new filename with the right extension. Existing files are never overwritten.
- Explicit certificate choice from Windows CurrentUser/My; extra signature allowance is user-selectable.
- Result is native OPC/XMLDSig, **not** PDF PAdES and not a native visible signature-line control. The **Verify** button checks cryptographic package integrity, **not** eIDAS qualified status, revocation, timestamp or chain trust.

## Confirmed tests on Yanakiev-Lenovo

- `cargo check -p wordcraft-ui-egui -j 2 --offline`: **PASS**.
- `cargo check -p gridcraft-ui-egui -j 2 --offline`: **PASS**.
- CLI synthetic sign, verify, tamper failure and no-overwrite: **PASS** on disposable test certificate (prior 2026-11-06 QA).
- CurrentUser/My certificate inventory by the owner's Windows session: two valid-now certificates issued by B-Trust Operational Qualified CA, with private key availability flag. This **does not** itself establish that hardware signing succeeded.

## Release gates

- [x] Source changes committed on a QA-only GitHub branch
- [x] Rust GUI crate builds checked on Windows
- [x] Full standalone Windows GUI executable compiled and started in the owner's desktop session
- [x] Real Review-ribbon screenshot confirms the Sign and Verify buttons are present and laid out correctly
- [ ] Certificate chooser, on-click actions, cancellation and PIN-provider interaction accepted through real UI
- [ ] B-Trust-qualified hardware/remote signing completed with provider PIN prompt on disposable test document
- [ ] Independent verification in Microsoft Office / suitable validator and revocation/qualification checks
- [ ] Trusted Authenticode-signed MSI, bundling both GUI and matching CLI
- [ ] Production upgrade approval and rollback verification

Do **not** merge to the main branch or replace the installed release based only on successful `cargo check`. Keep the original source documents and existing signed documents unmodified.

Windows QA logs and receipts: `C:\Users\User\Documents\CraftUpdates\Signing-Rebase-20261106\GUI-QA-20261107`.

## Additional owner-session runtime checks (2026-11-07)

- Rechecked 5 installed QA CLI scenarios on Windows: native OPC integrity returned success for signed DOCX, DOCX with two independent signatures, and signed XLSX; it returned nonzero for deliberately tampered signed DOCX and XLSX. **5/5 PASS.** Receipt: `EXISTING_CLI_INTEGRITY_REGRESSION.json`.
- Parsed already-signed synthetic Word DOCX via `wordcraft-cli info` and GridCraft XLSX via `gridcraft-cli info`: **2/2 exit code 0**. The minimal synthetic XLSX produced a nonfatal warning about its absent optional styles part. Receipt: `SIGNED_DOCUMENT_OPEN_QA.json`.
- Sanitized CurrentUser certificate EKU audit confirmed two B-Trust personal certificates, neither with Code Signing EKU. Receipt: `SIGNING_CERTIFICATE_EKU_AUDIT_SANITIZED.json`.
- All receipts are on Yanakiev-Lenovo at the QA directory described above; no real B-Trust signing or signed MSI is claimed.

## Completed GUI QA and isolated portable package (2026-11-07)

- `cargo build -p gridcraft --offline -j 4`: **PASS**, including the native Windows GUI. Build receipt: `gridcraft-qa-startup-rebuild-receipt.json`.
- Run with `CRAFT_SIGNING_QA=1` to disable **startup** vendor self-updates and select Review as the initial tab for this QA session only; default production startup behavior is unchanged. The portable ZIP contains `Run-GridCraft-QA.cmd` to launch with this environment variable.
- The QA executable opened successfully in the user's interactive Windows session. Visual screenshot file: `gridcraft-REVIEW-QA-20261107.png`, stored privately on the owner's Lenovo, **not uploaded publicly**.
- Two binaries (`gridcraft.exe`, `gridcraft-cli.exe`) have **embedded locally self-signed Authenticode test signatures** created with Windows SDK SignTool. Windows Authenticode result: **UnknownError** because the test publisher is not publicly trusted. This is **not** production code signing.
- The isolated ZIP is named `GridCraft-0.16.1-QES-GUI-QA-LOCALLY-SIGNED-UNTRUSTED-20261107.zip`, available only in the Lenovo QA folder pending final verification. License, notice, attribution and SHA-256 receipt accompany it.
- The OPC CLI regression tests pass with disposable software test certificates; **B-Trust hardware QES, certification chain/revocation, verified timestamp, Word/Excel product parity and trusted MSI remain NO-GO.**

No production release, installed MSI replacement, merge to main, or qualified-signature declaration has occurred.
