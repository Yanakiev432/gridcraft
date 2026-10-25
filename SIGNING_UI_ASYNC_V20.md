# Craft Office signing UI — asynchronous provider work (V20)

The 25 October 2026 Windows Application log captured a WordCraft Application Hang (event 1002). The installed desktop shortcut points to Craft Suite 1.0.4, which contains the older synchronous GUI implementation. This isolated source patch does not change any installed executables.

## Changes

- Enumerate Windows CurrentUser/My certificates outside the egui render/update thread. Loading and signing phases repaint their own progress window.
- Execute OPC signing, optional embedded appearance and independent reopen/verification outside the egui render/update thread. The GUI polls the completion channel and reports success or a specific failure.
- Only one request is allowed per UI state. The signing state cannot be dismissed while Windows/CNG or a hardware vendor PIN dialog is pending. This avoids a second signature request racing with the first.
- Make **Allow additional signatures** configurable. Checked means OPC multiple signatures are allowed. Unchecked signs an internal **Craft-only** restriction using the existing `opc_windows::sign_office_document_with_policy`; this does **not** impose universal Office document protection and does **not** prevent a non-Craft application from adding a signature.
- Craft-only restrictive policy and a custom visible Office appearance cannot be combined with this integration; the UI refuses this unsupported combination instead of silently misrepresenting it.
- Keep original input untouched and select a new output filename; never request PIN in Craft, store PIN, copy private keys, alter trust stores or claim eIDAS qualification.

## Verification and non-goals

`cargo check --offline --locked -p <app>-ui-egui` and focused `office_sign_ui::tests` are the intended smoke gates. Negative tests use no real signing certificates. Actual B-Trust signing and whether its provider PIN dialog appears on the intended Windows desktop **remain physical acceptance gates**.

Important: background threading improves the GUI responsiveness but is NOT a timeout of the native smart-card operation. If the vendor middleware blocks indefinitely, the worker may remain pending; the signing UI intentionally prevents duplicate requests. Do not mark hardware signing PASS until a real synthetic DOCX/XLSX completes and is independently verified.

This patch is for review branches. It is not yet deployed to Craft Suite, signed as a release, or published.
