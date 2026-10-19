# gridcraft — OPC/XMLDSig Windows CLI preview

Date: 2026-10-19. Branch: feature/opc-xml-dsig-cli-20261019.

Shared dependency: https://github.com/Yanakiev432/craft-signing-core at immutable rev 764d09c5937e0730df4f1a9aba106087e31c4fe7.
Microsoft Office OPC/XMLDSig implementation uses the WindowsBase PackageDigitalSignatureManager reference API, never the PDF PAdES signature serializer. This is a separate package signature standard.

## Actual CLI commands (Windows only)

- gridcraft-cli opc-sign original.xlsx new-signed.xlsx --thumbprint 0123456789ABCDEF0123456789ABCDEF01234567
- gridcraft-cli opc-verify new-signed.xlsx

Replace the example thumbprint with the explicitly selected *real* CurrentUser/My certificate's thumbprint. The actual PIN is entered only in the Windows token-provider dialog; do not enter it in CLI parameters or ChatGPT. The output must not exist. An existing signed package is refused pending separate counter-signature handling.

A JSON integrity result does NOT mean eIDAS QES, trusted timestamps, OCSP/CRL, proof of qualified signing time or verified Microsoft Office product interoperability.

## Verified evidence

- Shared core directly signed a disposable XLSX document using an in-memory software test identity.
- Shared core independently reopened and verified the signed OPC package. Modifying signed content and adding a new unsigned part were both rejected.
- The application CLI builds successfully in a Linux-to-Windows GNU cross-target check (2026-10-19).
- No real hardware B-Trust token was invoked for XLSX yet; CI workflow will separately exercise the CLI on an ephemeral Windows runner.
- GUI menu integration and independent Microsoft Word/Excel interoperability have NOT been accepted yet.
- The fork and binary preview are not officially distributed, not trusted Windows Code Signed, and must not be described as finished before complete acceptance.

## Next acceptance gate

Run GitHub Actions workflow .github/workflows/opc-signature-ci.yml on the fork branch. It performs real CLI sign/verify with a temporary, disposable Windows software signing identity and validates generated DOCX/XLSX. For production acceptance, a dedicated interactive B-Trust run and independent Office interoperability remain required. Never substitute a test leaf for a qualified signing certificate.
