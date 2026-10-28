# GridCraft: automatic XLSX detached CMS signature

This helper signs exact bytes of a saved XLSX using detached CMS/PKCS#7 and SHA-256.
It writes a companion .xlsx.p7s file. GridCraft copies the input XLSX without editing
formulas or calculation settings; both files must remain paired.

Automatic mode (default): file-signed.xlsx plus file-signed.xlsx.p7s.
Excel automatically recalculates cells without altering XLSX bytes on disk until saved.
Excel does NOT show an external signature in File > Info > Signatures.
Use the GridCraft Verify paired .p7s function.

Native Office XMLDSig (opt-in): visible in Excel; Excel's calculation engine
can invalidate the in-package signature in Automatic mode, as Microsoft documents.
This format requires manual-mode compatibility precautions.

Saving a changed XLSX after signing invalidates the external .p7s. This is correct.

Security: Windows CurrentUser/My certificate, Windows provider-controlled PIN.
No key export, no trust-store changes, no source overwrite. 128 MiB size limit.
Verification checks cryptographic signature only: certificate trust, revocation,
trusted timestamp and qualified eIDAS status are not established.
.NET Framework 4.x Windows runtime is needed. No network calls or paid APIs.

Build: run Build.ps1 in this folder. Place the generated craft-xlsx-cms.exe
beside gridcraft.exe. The existing craft-opc-worker.exe remains necessary for
the native Office mode. QA-only: not a public release or an Authenticode signature.
