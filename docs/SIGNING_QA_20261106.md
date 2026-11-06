# GridCraft 0.16.1 — Windows Office signatures (QA only)

This branch starts from the official `storytold/gridcraft` v0.16.1 tag. It ports the existing Craft OPC/XMLDSig command-line bridge from an older development fork into the current upstream CLI, without replacing installed official applications.

## Windows CLI

```powershell
gridcraft-cli opc-sign original.xlsx new-signed.xlsx --thumbprint <40-hex-certificate-thumbprint>
gridcraft-cli opc-verify new-signed.xlsx
```

The output must be a separate, new file. Certificate selection is explicit, and the signing key remains with the Windows certificate provider. Hardware-token PIN entry, if required, belongs solely to the provider's interactive dialog. Never put PINs, passwords, private keys or personal signed documents in commits or logs.

This is Microsoft Office OPC/XMLDSig, **not PDF PAdES**. The signatures are not proven qualified under eIDAS by an integrity check. Certificate chain, revocation, timestamp, qualified status, interoperability with Office, and real B-Trust USB/token acceptance require independent tests.

## Evidence and release gates (2026-11-06)

- Source port made on the exact upstream v0.16.1 commit, shared `craft-signing-core` pinned to revision `7c8dd0f0cee6068094520a43fca309fb8ff48bc1`.
- `cargo check -p gridcraft-cli` on Windows: **PASS**.
- Full executable runtime signing, synthetic tamper rejection and multi-signature behavior: **not accepted until completed and recorded**.
- Application GUI ribbon integration: **not present on this branch**.
- Actual hardware B-Trust QES and independent eIDAS verification: **not accepted**.
- Trustworthy Authenticode code signing and clean installer QA: **not accepted**.

**Release decision: NO-GO.** This QA source branch is not a signed production binary, MSI release, or upstream official release. Do not replace the current trusted installations with an unsigned QA executable.
