# GridCraft / Office Excel native signature compatibility — 2026-10-28

## Required acceptance

1. The finished XLSX contains a **native OPC XML-DSig** signature that Office Excel
   recognizes under File > Info > View Signatures.
2. Formula calculation is Automatic and the signature remains recognized when the
   workbook is opened (A) as the first workbook and (B) after an existing
   Automatic workbook is already open.
3. The source file remains untouched, the signing key stays with the Windows
   provider, and a test certificate is never represented as B-Trust QES.

## Microsoft behavior and limitation

Excel itself can invalidate digital signatures by recalculating previously
signed workbooks, even with unchanged visible formula results. Microsoft confirms
this as a product problem and officially recommends Manual calculation **before**
signing or before opening the signed workbook:
https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/document-contains-invalid-signatures-error

Excel Desktop calculation settings affect all open workbooks:
https://support.microsoft.com/en-us/excel/change-formula-recalculation-iteration-or-precision-in-excel

Office signing workflow (visible and invisible signatures):
https://support.microsoft.com/en-us/office/security-privacy/add-or-remove-a-digital-signature-for-microsoft-365-files

Package signatures include a signed manifest and OPC parts:
https://learn.microsoft.com/en-us/previous-versions/windows/desktop/opc/digital-signatures-overview

Do not claim universal compatibility with Automatic mode. Do not attempt to
silently omit xl/workbook.xml from signed package coverage as a workaround.

## Actual controlled evidence

- V36 Word native XMLDSig: Recoverable signature with intentionally untrusted
  software-only QA certificate.
- V38-A Excel with Manual calculation and formula: Recoverable (controlled).
- V39 Excel with Manual calculation and same formula:
  * directly opened in fresh Excel: Recoverable;
  * opened after new workbook with Automatic calculation: Invalid signature;
  * source package Windows OPC verification: PASS.
- The user confirmed that after Edit Anyway removed the V39 signature, Excel
  showed workbook's saved calculation option as **Manual**.
- V41 automatic calculation + **external .xlsx.p7s**: Windows CMS and OpenSSL
  verify PASS; modified XLSX correctly fails. This is NOT a native Excel
  signature and will NOT appear in Excel's Signatures pane.
- V42 native OPC XMLDSig experiment is signed with:
  calcId=191029, calcMode=auto, calcCompleted=1,
  fullCalcOnLoad=0, forceFullCalc=0.
  Windows OPC verification PASS. **Microsoft Excel result has NOT been received.**

Comparison on Lenovo found that both V39 and V42 sign xl/workbook.xml and
its relationship part (Office package Manifest 11 entries).
The native signature properly covers the calculation configuration. Hash
invalidation from Excel recalculation may be a client-side behavior, not
evidence of a malformed signature.

## Technical direction

- Keep the existing manually-configured native Office signature mode for
  Office Signatures pane compatibility. Clearly label the manual calculation
  limitation and never force system-wide Excel settings.
- Keep the detached CMS option for preserving Automatic calculation and
  independent integrity validation. Explain it is a separate format, NOT
  interchangeable with a native Excel signature.
- Test V42 with **actual** Excel in BOTH sequences; do not infer acceptance
  from Windows OPC verification.
- If V42 still fails, obtain an Excel-signed **synthetic** XLSX baseline,
  compare manifest references, SignatureInfoV1, relationships, pre-sign
  calculation metadata and signer behavior. Perform an A/B opening-order
  test with that native Excel-produced signature to separate Craft defects
  from Microsoft's documented limitation.
- Never re-sign signed content, alter worksheet values or weaken package
  signature coverage merely to suppress an invalid-signature warning.

## Release decision

**Native XMLDSig + Automatic, independent of Excel opening order: NOT ACCEPTED.**
Private development is allowed; public release / qualified signature claims
remain NO-GO pending independent validation.
