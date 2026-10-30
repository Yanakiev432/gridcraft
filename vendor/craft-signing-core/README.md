# Craft Signing Core

**Status: v0.1 API contracts, not a standalone production signer.**

Shared Rust interfaces for digital-signature providers in PdfCraft, WordCraft and GridCraft.

- Providers: Windows CNG (including USB tokens), macOS Keychain, PKCS#11, PKCS#12, remote.
- Separates signing identity/key access from document format.
- Separates content integrity, chain trust, revocation and timestamp outcomes.
- Document adapters must implement PDF/PAdES and OOXML/XMLDSig separately.

## Proven working implementation

PdfCraft 0.8.0 currently implements B-Trust USB/CNG signing and PDF PAdES validation in its own source tree. The Oct 17 Windows-root trust fix was tested in PdfCraft, **not in this standalone crate**. The extraction/integration milestone is still open.

## Next milestones

1. Extract Windows CNG and PKCS#11 provider implementations and tests from the PdfCraft integration.
2. Implement PdfCraft PAdES adapter and end-to-end signing/verification tests.
3. Implement separate WordCraft/GridCraft OOXML/XMLDSig adapters.
4. Verify certificate policy, revocation and qualified-signature requirements; never claim eIDAS QES from chain trust alone.

No PINs, private keys, signed personal documents or certificates are committed.

## Cross-application format contracts (2026-10-19)

The tested `format_contract` module now provides one provider-facing namespace for three **different document formats**: PdfCraft (`.pdf` / PAdES), WordCraft (`.docx` / OOXML OPC XMLDSig), and GridCraft (`.xlsx` / OOXML OPC XMLDSig). It rejects crossing a PDF signature serializer with an OOXML package, distinguishes the PAdES B/T/LT/LTA profile names, and performs an initial container-magic check.

**Release boundary:** The OOXML preflight only verifies the ZIP container marker. It is **not** an OOXML package parser, not a signed package, not a document validator and not a QES claim. Production WordCraft/GridCraft integration must separately implement and test OPC signature origin/relationships, content types, digest transforms, XML canonicalization, package coverage, Office interoperability and independent validation. A detached PDF CMS signature must not be inserted into an Office ZIP and advertised as a valid Office signature.

On Ubuntu WSL with no GPU and no private keys: `cargo test --offline` gives 6 passed, 0 failed. The project does not process PIN and never exports a physical token key.

## Test

`cargo test`

## License and attribution

MIT OR Apache-2.0. Architecture inspired by [PdfCraft](https://github.com/storytold/pdfcraft) (MIT OR Apache-2.0). No upstream source files copied here.

## Windows trust-store adapter (experimental)

`windows_trust::trusted_root_der()` reads the existing Windows CurrentUser and LocalMachine Root stores without modifying them. This read-only extraction is useful to a PdfCraft-style verifier but does **not** replace Windows chain-policy evaluation, revocation/Disallowed checks or eIDAS qualification. Hardware signing remains in the original PdfCraft implementation; it is **not yet extracted** into this crate.

## Windows CNG RSA adapter

On Windows, `windows_cng::WindowsCngProvider` enumerates CurrentUser Personal certificates and selects an identity by the SHA-256 hash of its DER certificate. `sign` acquires the Windows key **interactively**, hashes the supplied message, and requests RSA PKCS#1 v1.5 signing from CNG. It does not export the key.

**Important:** This implementation is compile-checked, not hardware-validated. Do not treat its presence as a qualified-signature or interoperability certification. It does not implement RSA-PSS, ECDSA signature conversion, PKCS#11 or OOXML signatures. No signing operation was run during extraction.

## Native Windows OPC live acceptance (2026-10-22)

The in-process Windows OPC bridge was exercised with fresh minimal DOCX and XLSX
documents under target/ and an explicitly chosen self-signed development
code-signing identity from CurrentUser/My. No private key or PIN is exported.

- DOCX sign and reopen/verify: PASS (one signature).
- XLSX sign and reopen/verify: PASS (one signature).
- Mutated signed XLSX worksheet: verification correctly rejects (0x80096010).
- Signed DOCX with one extra, unsigned OPC part: verification rejects (0x80096010);
  each of the six original ZIP parts was independently SHA-256 checked unchanged.
- Run: pwsh -NoProfile -File tools/opc-live-smoke.ps1
- Run negative checks with tools/opc-tamper-smoke.ps1 -FixtureDirectory <test-fixture-directory>.

Root cause of first live failure: Windows OPC defaults to storing an X.509
certificate in a separate package part, which conflicted with the strict
package-wide part coverage check. New signatures explicitly use
OPC_CERTIFICATE_IN_SIGNATURE_PART; no weakening of content coverage checks.

These results do NOT prove B-Trust USB-token signing, eIDAS QES status, trust
chains/revocation, visible Office signature lines, or Microsoft Office
interoperability. Existing Office signatures with separate certificate parts
still require an explicit interoperability test before acceptance.
