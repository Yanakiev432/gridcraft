# GridCraft — signed Craft Signer v2 Review integration QA

Date: 2026-11-14
Branch: qa/verified-signer-v2-review-20261114
Status: source-only QA; NOT an installed app, production release or QES validation.

## Design

- Existing Review > Sign and Review > Verify actions are unchanged.
- New Windows-only Review > Verify v2 action checks a SAVED XLSX document
  using the authenticated installed-module API from craft-signing-core.
- Both the UI and CLI pin the same signing-core Git revision: f01ba3bbdb5ab3c0afa11f7132231e1bdebde7e5.
- The shared core checks module-specific publisher approval, actual .NET CMS
  signed catalog, public publisher SPKI, active versioned registration,
  and SHA-256 of all three signed executables BEFORE starting any broker.
- Verification runs in a dedicated Rust worker, not on the egui UI thread.
  The broker receives only a document.verify request: no PIN,
  signing permission, output path, thumbprint or private key.
- Dirty documents, unsaved content and non-XLSX files are rejected.
  An integrity PASS means OPC package-signature integrity only,
  NOT certificate chain trust and NOT eIDAS/QES confirmation.
- The legacy fixed-worker slot remains independent and is never
  silently treated as the new v2 trusted installed module.

## QA

The host reuses OfficeSignUi result state and busy checks. One new unit
test rejects dirty/unsaved sources. The new button uses the existing
small Review control rather than oversized ribbon tiles.

The shared core passed 34/34 standard tests and 3/3 actual signed-package
interoperability tests. GUI cargo check -p gridcraft-ui-egui --locked --offline
passed on Windows, exit 0. The actual GridCraft UI unit test v2_never_verifies_dirty_or_unsaved_document
PASSED (1/1, exit 0) in the native Windows lib-test binary.
CLI compilation, visual review and installed-module QES acceptance
remain separate release gates.

No GPU load, hardware signing, paid API, official Craft app install,
global system trust changes or local publisher approval are performed.