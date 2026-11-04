# V51 — Real B-Trust XLSX signature accepted by Microsoft Excel

Date: 2026-11-04. Scope: **manual external Excel acceptance**, not automated CI.

## Result: PASS (Office interoperability)

A user-controlled Windows computer with Microsoft Excel opened the XLSX file signed through the Windows certificate provider on the separate Lenovo machine. A user-provided screenshot of Excel showed:

- **Signatures:** `Valid signatures`
- **Signature Details:** `Valid Signature - The signed content has not changed and the signer's certificate is valid.`
- **Signature type:** `XML-DSig`
- **Issuer:** `B-Trust Operational Qualified CA`

The signer was a real personal certificate rather than the V46 ephemeral test certificate. The local Windows OPC verifier independently reported `signature_integrity=Success`, `signature_count=1`, `signed_parts_count=9`, `unsigned_content_parts=[]`, `uncovered_relationships=[]`.

The signed artifact's SHA-256 was `86313254FEF7D0E27B17763B4BDEDCBC3857867B210F1CBCA61DE95CFC917156`. It remains **local-only** on the Lenovo; the private XLSX and personal certificate identity are intentionally excluded from the repository. The evidence screenshot is in the private conversation, not checked into source control. A separate local JSON receipt records the external Excel acceptance.

## What this proves

- The Windows-provider signing path can create an Office-compatible, cryptographically intact XLSX signature using the user's real B-Trust personal signing certificate.
- Microsoft Excel independently recognizes the document signature as **valid**, rather than recoverable/untrusted as in V46.
- The signing workflow did not require extracting the private key or sharing the PIN.

## What this does NOT prove

- A qualified electronic signature under eIDAS (qualified certificate and qualified signature creation device must be evaluated separately).
- Qualified or trusted timestamp; long-term LTV validation; online revocation state at the signing time.
- Authenticode trust of the Windows MSI (a different signature type).
- Hosted Windows MSI lifecycle CI acceptance (still pending).
- That every arbitrary XLSX feature is compatible or that the workbook was recalculated in Excel.

## QA observation

The original launching PowerShell wrapper threw an error because it checked `$LASTEXITCODE` after calling a script which returned a structured result but no process exit code. The actual signed file was created and separately verified successfully. This is a **wrapper exit-status reporting defect**, not a failed signature. Address it independently; do not rerun or re-prompt the user for their PIN just to reproduce a known-successful signature.

## Privacy and release boundary

Never commit personal signed documents, certificate thumbprints, identity details, PINs, or screenshot containing personal identity. This report is deliberately redacted. No production installer was signed or published.
