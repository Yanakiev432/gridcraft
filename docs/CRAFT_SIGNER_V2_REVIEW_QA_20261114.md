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
The native GridCraft CLI compilation also PASSED:
cargo check -p gridcraft-cli --locked --offline, exit 0.
Visual Review and installed-module positive acceptance remain separate release gates.

No GPU load, hardware signing, paid API, official Craft app install,
global system trust changes or local publisher approval are performed.
## Saved document view-binding acceptance

A completed positive Verify/Verify v2 result is now tied to the exact
saved document path at dispatch. When the visible editor changes to a
different source, the file is closed, or unsaved changes appear, the
old result becomes invalid rather than applying to the new editor view.

The native Windows UI test
completed_verification_does_not_apply_to_another_active_document
PASSED (1/1, exit 0). This is distinct from the signed-module trust gate
and does not claim certificate trust or eIDAS qualified status.

## CPU-only Review accessibility acceptance — 14 November 2026

A deterministic egui_kittest test builds the actual native Review ribbon and
AccessKit tree (not a copied UI mock). The renderer is explicitly disabled
using LazyRenderer::Uninitialized to ensure there is no GPU rendering.

Scope:
- Viewport: 1440 x 900 logical points.
- Confirm accessible Role::Button controls Sign, Verify and Verify v2.
- Confirm all three controls have compact ribbon-row dimensions rather than
  the old large signature tiles, with nonnegative visible coordinates.
- Confirm no signing operation was started.
- Test target: review_shows_compact_sign_verify_and_verify_v2_without_gpu.
- Actual result: 1 passed, 0 failed, exit code 0, locked/offline Windows build.

This is CPU-only layout/accessibility evidence. It does NOT replace a visual
screenshot review of the actual installed app, nor does it approve the
developer publisher or exercise a real B-Trust token.

## Distinct native and verified-broker progress messages — 14 November 2026

The UI now tracks the verification backend rather than displaying the
legacy native OPC progress string for both methods. Native Verify shows
native OPC verification, while Verify v2 explicitly names the authenticated
Craft Signer v2 broker and states that OPC integrity is not eIDAS QES.
No signing or trust policy is changed by this copy correction.

Actual targeted Windows execution:
cargo test -p gridcraft-ui-egui office_sign_ui::tests --lib --locked --offline
RESULT: 6 PASS, 0 FAIL, exit code 0. The regression includes the
source context/dirty guard, saved-file requirement, and the distinct
native/broker progress messages without a false QES PASS assertion.

## Pinned host minimum-version gate -- 14 November 2026

The native CLI and UI now both pin the exact shared core Git commit
fafbc0b993abbb80df8457d1af2a019077ac3429. The dependency change
was made only in the two direct Cargo manifests and Cargo.lock.

The shared Rust host resolver rejects any signed module version below
0.2.2, including the independently authenticated 0.2.1-qa release
whose EU DSS unsigned-PDF parser is known to be incorrect. A disposable
dual-signed-package test proves the host rejects a registration rollback,
holds an authenticated broker file lock against mutation, accepts the
0.2.2 release, and refuses a partial current.json registration.
This does not claim globally persistent anti-rollback for all future
versions or production owner approval.

The updated host compile checks pass on Windows with --locked --offline,
both for native UI and CLI. The Office dialog tests for each host contain
6 passing tests (dirty/unsaved guard, active document view-binding,
backend-specific progress message and existing signing guards).
No production Craft app or real user publisher policy was changed.