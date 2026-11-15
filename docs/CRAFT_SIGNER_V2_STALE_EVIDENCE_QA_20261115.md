# Office Verify / Verify v2 — stale-evidence hardening (source QA)

Date: 2026-11-15
Status: **SOURCE CHANGE ONLY — WINDOWS TESTS PENDING**
Branch: `qa/stale-verification-evidence-20261115`
Base: `qa/verified-signer-v2-review-20261114`

## Identified defects

The existing Office signature dialog checked the active document only when
a positive verification had already finished. A user could switch away and
back during an asynchronous verification without necessarily invalidating
the result. In addition, neither native OPC verification nor Craft Signer
v2 verification independently compared the saved file bytes before and
after the verifier ran.

A saved file may also be modified by another program without setting the
editor's in-memory `dirty` flag.

## Changes in this QA branch

1. Both native Verify and Verify v2 now hash the saved DOCX/XLSX file with
   SHA-256 on the **worker thread** before and after running their existing
   verifier. Different hashes, read failures and files above 1 GiB reject
   the result. The 64 KiB read buffer prevents unbounded memory use.
2. While verification is pending, the UI records *any observed* document
   switch or unsaved edit and does not clear that fact when the original
   tab is revisited. The guard runs immediately before and after worker
   polling to avoid a completion-frame race.
3. The UI also snapshots file length, creation time and modification time
   at verification dispatch. It checks them before continuing to display
   a successful verdict. Missing or changed metadata causes a fail-closed
   invalidation. Metadata reads are lightweight; no SHA-256 occurs in the
   egui render loop.
4. Legacy Sign is unchanged and remains separate from Verify v2.
   The worker never invokes a PIN prompt or modifies saved files.

## Added tests (not yet executed on Windows)

- `same_size_rewrite_during_worker_verification_is_rejected`:
  modify a synthetic saved file without changing its length between
  the two SHA-256 reads; require rejection.
- `switching_away_and_back_while_verification_runs_invalidates_success`:
  simulate a different active file during verification, return to the
  original path, finish successfully and require invalidation.
- `external_replacement_after_verification_revokes_cached_success`:
  rewrite the saved file after the worker has finished, without an
  editor `dirty` flag; require invalidation.
- All existing `office_sign_ui::tests` and the CPU-only Review ribbon
  accessibility/layout test must remain green.

## Required Lenovo Windows acceptance commands

Run in this new QA worktree **after the laptop reconnects**:

```powershell
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_INCREMENTAL='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
# Use a dedicated scratch CARGO_TARGET_DIR with adequate free disk space.
cargo check -p gridcraft-ui-egui -p gridcraft-cli --locked --offline
cargo test -p gridcraft-ui-egui office_sign_ui::tests --lib --locked --offline
cargo test -p gridcraft-ui-egui review_shows_compact_sign_verify_and_verify_v2_without_gpu --lib --locked --offline
```

Replace `gridcraft` with `wordcraft` or `gridcraft`. This QA branch must
not replace an installed application or be merged into the previously
accepted branch until all the commands are genuinely PASS and a clean
remote commit matches the local test worktree.

## Explicit residual risks

These checks do NOT guarantee that a simultaneously writable file could
never be changed and restored between observations. Windows file metadata
can be deliberately forged, and a positive result refers to the verified
saved bytes at a particular time, not to all future edits. Continuous
cryptographic freshness would require a separately designed verified
open-handle/snapshot protocol, with security review and stress tests.

No installed owner-approved Craft Signer module exists in this QA context;
there is no positive installed-module QES, B-Trust or EU DSS acceptance.
No live trust policy, production application or certificate store was
changed by preparing this GitHub-only source branch.
