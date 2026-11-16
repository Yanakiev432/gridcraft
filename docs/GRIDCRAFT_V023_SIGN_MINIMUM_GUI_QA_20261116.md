# GridCraft — native signed Office v0.2.3 compatibility QA

Date: 2026-11-16. Branch `qa/native-v023-version-gated-ui-20261116`. **Source and local binary QA only. Official ArtCraft builds unchanged.**

## Changes

The GUI and bundled CLI now pin the same source revision of `craft-signing-core`, `5b5c14c4e7668cf15cce128750b80a6113fc5f31`, which rejects signed module versions below 0.2.3 before submitting any `document.sign` request. The source continues to require a saved XLSX, explicit Windows certificate selection, new output file, supported additional-signature policy and user consent. It never accepts a personal PIN in Craft and never labels OPC integrity a qualified signature.

The native dialog's explanatory text now identifies the 0.2.3 minimum; 0.2.2 can continue to serve a separate, read-only Verify path. The deprecated egui `Id::new` call was updated to the replacement `Id::unique`, removing a locally introduced compiler warning. The distinct legacy QA button remains clearly labelled and must **not** be shipped in the upstream official UI; the authenticated v2 path is the candidate for supported releases.

## Actual Windows acceptance

Executed on Lenovo, Windows 11, CPU-only:

- `cargo test -p gridcraft-ui-egui --lib office_sign_ui::tests`: **11 passed, 0 failed**.
- `cargo test -p gridcraft-ui-egui --locked --offline --lib review_shows_compact_sign_verify_and_verify_v2_without_gpu`: **1 passed, 0 failed**.
- `cargo build -p gridcraft --locked --offline`: **PASS, exit 0**.
- `cargo check -p gridcraft-cli --locked --offline`: **PASS, exit 0**.
- Locally compiled GUI `target/debug/gridcraft.exe`: 53,821,440 bytes, SHA-256 `8B2C186A2CAACFC6D627EE8FE5633C012C8169A707BC45D6EFC317B56FADD1EF`.
- No GUI was installed into an official location, no QES signing was run. The build reports upstream optional-patch warnings, not the previously deprecated `egui::Id::new` warning.

## Release boundaries

The signed 0.2.3 QA module has already passed independent RustCrypto CMS and payload validation, disposable version upgrade, synthetic signed-broker QA, and owner policy fail-closed checks. This host update does **not** change the already signed 0.2.3 package, publisher identity, or actual installed module.

A genuine XLSX signature through a hardware-backed B-Trust certificate requires owner-attended installation/approval and provider-owned PIN UI; it is not covered by automated test counts. The office host's native GUI cannot be claimed to ship upstream until ArtCraft maintainers accept and publish it.

**Production: NO-GO** pending owner-attended native GUI and upstream acceptance.
