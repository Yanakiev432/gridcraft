# GridCraft upstream candidate — authenticated Office Sign only

Date: 2026-11-17. Branch `qa/upstream-authenticated-sign-only-20261117`.

## Why

Previous isolated Windows QA showed two choices in the signing dialog: an obsolete direct-CLI legacy QA signer and the signed, authenticated v2 module. A production user could select the wrong method; keeping both would undermine the trusted-module boundary and complicate native host upgrades.

This branch **removes** the legacy signer method, its command-line process spawning and `taskkill.exe` use, the second UI button and the obsolete progress state. The only Sign button now says **Sign new copy**, and invokes the strictly authenticated `craft_signing_core::office_host_sign::sign_office_via_installed_signer`. Existing native read-only Verify and authenticated signed-module Verify both remain available. The UI never accepts a token PIN; it must be entered through the native Windows/B-Trust provider dialog.

The pinned core `5b5c14c4e7668cf15cce128750b80a6113fc5f31` rejects signed module versions below **0.2.3** before signing IPC; it demands a saved XLSX, explicit consent, exact selected certificate, new output copy and supported additional-signature policy. Office OPC integrity cannot be called an independently qualified eIDAS QES.

## Actual Lenovo acceptance

- Windows Rust Office GUI tests: **11/11 PASS**.
- Review ribbon UI test: **1/1 PASS**.
- `cargo build -p gridcraft --locked --offline`: **exit 0**.
- `cargo check -p gridcraft-cli --locked --offline`: **exit 0**.
- Unsigned QA GUI build `target/debug/gridcraft.exe`: **53,757,952 bytes**; SHA-256 `ABDD3862E8663076AE9CE40D93A70BDC471F5EDC7DD353FB14EA710E8D64C673`.
- Source scan: **0 references** to `taskkill.exe`, legacy button or `resolve_worker()` in the native signing dialog. No official installed binary modified.

## Maintainer integration prerequisites

The feature branch is source-level QA on the fork, not a PR merged into `storytold/gridcraft`. The official shared plugin ABI effort `storytold/craft-libs#432` currently proposes *sandboxed, unsigned WASM plugins*, not privileged native Windows certificate signing; its host RFC is `storytold/craft-libs#414`. The separate `craft-esign#661` is remote/request-signatures UX and does not replace hardware-backed Office OPC signing. The correct upstream seam needs maintainers' approval, with the separate signed native broker kept outside the WASM guest sandbox.

This branch does not provide a publicly trusted developer publisher, real owner approved module installation, hardware PIN sign E2E, QES qualification verdict, official release integration or rollback through the official updater. Those remain **NO-GO** gates.
