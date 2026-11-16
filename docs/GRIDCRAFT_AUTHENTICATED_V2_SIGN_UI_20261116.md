# GridCraft — native authenticated Craft Signer v2 signing UI

Date: 2026-11-16. Status: **isolated source QA; not shipped upstream; no real QES acceptance**.

## New integration

The GridCraft native signing dialog presents two clearly labelled choices: the pre-existing legacy QA CLI path and an **authenticated v2** path. The new button is a direct user gesture, and never invokes the other path as a fallback.

The v2 path validates the active saved XLSX, unsaved/dirty edits, exact explicitly selected Windows certificate, new output destination and currently supported additional-signatures policy before dispatch. It invokes `craft_signing_core::office_host_sign::sign_office_via_installed_signer` on a dedicated thread and uses the existing cancellation UI to signal its Windows Job-bound broker and worker process family. It shows a different progress label and 340-second timeout for the v2 path.

The native Windows certificate provider, not the application, owns the private key and any B-Trust PIN dialog. The resulting verdict is **OPC integrity only**, not certificate-chain trust, qualified status or eIDAS QES.

## Fail-closed release boundary

The installed 0.2.2 signed module does not yet support the new explicit `allowAdditionalSignatures` request field and therefore **cannot** complete a v2 sign through this new path. The 0.2.2 signed catalog and executable must remain immutable.

The next implementation step is to build, independently authenticate and owner-approve a distinct signer package release with the matching .NET broker. Until then the new button will refuse the unsupported protocol. No real owner approval, Windows certificate access, hardware PIN, or production document signing is part of automated QA.

No official GridCraft distribution has been modified. The final upstream PR must be reviewed and accepted by the ArtCraft/Storytold maintainers before any public shipping claim.
