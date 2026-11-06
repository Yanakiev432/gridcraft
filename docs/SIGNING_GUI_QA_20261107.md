# gridcraft Windows GUI Office signature integration — QA checkpoint (2026-11-07)

Status: **SOURCE BUILD CHECK PASS; PRODUCTION INSTALL / B-TRUST ACCEPTANCE NOT YET VERIFIED**.

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
- [ ] Full standalone desktop executable build and startup checked
- [ ] Review tab interaction, certificate chooser and error paths tested through real UI
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
