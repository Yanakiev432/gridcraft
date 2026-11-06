# Trusted Windows code signing plan (2026-11-07)

**Status: proposed route only. No signing service has been approved or connected. No signed MSI release exists for this QA branch.**

## Current certificate findings

- Windows CurrentUser/My contains two currently valid B-Trust Operational Qualified CA personal certificates with private keys; their Enhanced Key Usage entries are Client Authentication and Secure Email. **They are not Code Signing certificates.**
- The local Craft Open Source Local Code Signing certificate has Code Signing EKU, but is a local development/test certificate, not a publicly trusted release publisher.
- The installed upstream WordCraft/GridCraft releases remain independently vendor-signed; those signatures cannot be transferred to modified binaries.

## Candidate for free public OSS releases

Microsoft's Windows code-signing documentation identifies **SignPath Foundation** as a free signing option for qualifying open-source projects. Official references:
- https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
- https://signpath.org/
- https://signpath.org/terms.html
- https://signpath.org/apply

Requirements to verify before applying: own maintained public project/repository, OSI-approved licensing, no proprietary incompatible components, released and documented software, trusted reproducible/public build provenance and a written code-signing policy. SignPath Foundation signs qualifying builds using its managed HSM and identifies **SignPath Foundation** as the certificate publisher.

The official upstream WiX manifests currently state `Manufacturer="Learning Machines LLC"`. A user-maintained fork should **not** represent itself as the upstream company in release metadata without authorization. Confirm fork naming/trademark permissions and reconcile publisher / manufacturer presentation before release.

## Proposed gated pipeline

1. Keep isolated `qa/signing-*` branches; no release from development branches.
2. Build both GUI and matching CLI from pinned public source in GitHub Actions.
3. Require cargo-check, exact source/version provenance, unit tests, executable startup tests, synthetic OPC sign/verify/tamper tests, and B-Trust user-present interactive acceptance.
4. Produce .exe and .msi artifacts with SHA-256 and build provenance.
5. Only after SignPath application approval: submit eligible CI artifacts to its signing service, review and manually approve each signing request.
6. Verify every signed .exe and .msi with Windows Authenticode and inspect the resulting publisher and timestamp.
7. Only then publish tagged releases and consider an explicit production MSI upgrade with rollback.

No PINs, private keys or secret API credentials in source control or logs. No bypass of Windows Smart App Control or installer signing enforcement.

## Independent gate

A cryptographically valid DOCX/XLSX OPC signature is not automatically an eIDAS-qualified signature. Hardware token/private-key status, certificate qualification at signing time, trusted certificate chain, timestamps and revocation must be validated independently.
