# V50 — Authenticode metadata and isolated local signing QA

Date: 2026-10-31. Source commit: pending.

## Code-signing identity metadata

The Windows `sign.ps1` no longer claims that independent GridCraft builds are `GridCraft` published at the upstream `storytold/gridcraft` URL. Its explicit SignTool description is now `GridCraft (Craft Open Source)`, and its source URL is `https://github.com/Yanakiev432/gridcraft`.

**Important:** SignTool's display description is not the legal signer identity. The publisher displayed by Windows security prompts comes from the **certificate subject**. The project name `Craft Open Source` is not independently verified as a legal company or Authenticode issuing subject. Public distribution remains blocked without a trusted certificate and verified publisher.

## Actual local-only signing result on Lenovo

An exact copy of the independently packaged V48 QA MSI was signed with the installed self-issued code-signing certificate `CN=Craft Open Source Local Code Signing`, valid through 2028-10-22, by the Windows SDK x64 SignTool.

- Copied original MSI SHA-256 before signing: `FA08CB350F7E0CA2FC95ABDAA243085CC580EFAEE79489C14BA445F74F592A4D`
- Signed QA copy SHA-256: `19C0CEBCF65D2B13AB870150E149B35790BE228AF2A6A7329204C8D3B178AF69`
- SignTool reported `Successfully signed` (exit code 0).
- Embedded signer matched the selected ephemeral/local code-signing identity.
- Windows returned `UnknownError` with the diagnostic **root certificate is not trusted**.
- No RFC 3161 trusted timestamp was used. No certificate was added to Trusted Root.
- No real B-Trust hardware token was used; no signing key exported.
- The original unsigned QA MSI was not modified.
- No MSI was installed or released.
- This is *not* a public Authenticode acceptance or a qualified electronic signature.

QA result: **LOCAL SIGNATURE EMBEDDED / PUBLIC TRUST NO-GO**.

## Remaining release blockers

1. Validated legal publisher and publicly trusted Authenticode certificate, with appropriate timestamp service.
2. Hosted Windows MSI lifecycle CI with original-plus-independent side-by-side installation (V49 code added, actual CI outcome still unconfirmed).
3. Final runtime branding/assets/trademark and license review.
4. Existing separate external V46 Excel interoperability acceptance and B-Trust eIDAS/qualified signature workflow, if in scope for the release.

Never publish this self-signed installer or represent it as trusted.
