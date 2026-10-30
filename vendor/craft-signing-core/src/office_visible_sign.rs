//! Certificate-bound end-to-end visible OPC signing prototype.
//! Uses the Windows certificate inventory, system-font SVG→PNG renderer,
//! safe unsigned OOXML asset writer and existing in-process OPC signer.
//! The displayed "device time" is NOT a trusted timestamp or eIDAS proof.
//!
//! Safe default: refuses to mutate already-signed documents, preserving their
//! existing XMLDSig references. Caller should offer invisible multi-signing.
#![cfg(windows)]
use crate::{
    office_appearance::OfficeSignatureAppearance,
    office_appearance_render::render_png,
    office_certificates::list_office_certificates,
    office_visible_opc::{add_unsigned_visual_appearance, OfficeImagePlacement},
    opc_native::{sign_opc_inprocess_with_policy_cancelable, verify_opc_inprocess},
};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub fn sign_with_visible_appearance(
    source: &Path,
    output: &Path,
    certificate_thumbprint: &str,
    appearance: &OfficeSignatureAppearance,
    placement: OfficeImagePlacement,
) -> Result<(), String> {
    sign_with_visible_appearance_with_policy(
        source, output, certificate_thumbprint, appearance, placement, true,
    )
}

/// Visible signing with the signed Craft-only final-signature policy when
/// allow_additional=false. External Office apps may ignore the Craft policy.
pub fn sign_with_visible_appearance_with_policy(
    source: &Path,
    output: &Path,
    certificate_thumbprint: &str,
    appearance: &OfficeSignatureAppearance,
    placement: OfficeImagePlacement,
    allow_additional: bool,
) -> Result<(), String> {
    let never=std::sync::atomic::AtomicBool::new(false);
    sign_with_visible_appearance_with_policy_cancelable(
        source,output,certificate_thumbprint,appearance,placement,allow_additional,&never)
}
pub fn sign_with_visible_appearance_with_policy_cancelable(
    source: &Path,
    output: &Path,
    certificate_thumbprint: &str,
    appearance: &OfficeSignatureAppearance,
    placement: OfficeImagePlacement,
    allow_additional: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("Visible signature cancelled before rendering".into());
    }
    if source == output || output.exists() {
        return Err("Visible signing destination already exists or aliases the source".into());
    }
    if certificate_thumbprint.len() != 40
        || !certificate_thumbprint.bytes().all(|x| x.is_ascii_hexdigit())
    {
        return Err("Explicit 40-hex certificate thumbprint required".into());
    }
    let certificate = list_office_certificates()?
        .into_iter()
        .find(|c| c.thumbprint.eq_ignore_ascii_case(certificate_thumbprint))
        .ok_or("The selected certificate was not found in Windows CurrentUser/My")?;
    if !certificate.has_private_key || !certificate.valid_now {
        return Err("Certificate lacks a usable private key or is outside its validity period".into());
    }
    // Never accept a caller-supplied arbitrary signer's name for a visible
    // cryptographic signature. Derive the displayed identity from the
    // same certificate selected for the *actual* private-key operation.
    let signer = OfficeSignatureAppearance::subject_display_name(&certificate.subject);
    let local_clock = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();
    let graphic = render_png(appearance, signer, &local_clock)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let ext = source.extension().and_then(|x| x.to_str())
        .ok_or("Source extension missing")?;
    if !matches!(ext.to_ascii_lowercase().as_str(), "docx" | "xlsx") {
        return Err("Visible Office signing accepts DOCX or XLSX".into());
    }
    let name = format!(".craft-visual-{}-{stamp}-{sequence}.{ext}", std::process::id());
    let staged = output.with_file_name(name);
    if staged.exists() {
        return Err("Unsigned staging file already exists".into());
    }
    let result = (|| {
        add_unsigned_visual_appearance(source, &staged, &graphic, signer, placement)?;
        sign_opc_inprocess_with_policy_cancelable(&staged, output, certificate_thumbprint, allow_additional, cancel)?;
        let count = verify_opc_inprocess(output)?;
        if count != 1 {
            return Err("Expected one valid new OPC signature".into());
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&staged);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_implicit_identity_without_touching_output() {
        let a = OfficeSignatureAppearance::default();
        assert!(sign_with_visible_appearance(
            Path::new("missing.docx"), Path::new("missing-signed.docx"), "", &a,
            OfficeImagePlacement::WordDocumentEnd
        ).is_err());
    }
}
