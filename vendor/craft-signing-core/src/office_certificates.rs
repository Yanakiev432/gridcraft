//! Native Windows CurrentUser/My certificate metadata. No subprocesses or PIN prompts.
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
pub struct OfficeCertificate {
    pub thumbprint: String,
    pub subject: String,
    pub issuer: String,
    pub expires: String,
    pub has_private_key: bool,
    pub valid_now: bool,
}
unsafe extern "C" {
    fn craft_windows_cert_inventory(output: *mut u8, capacity: i32) -> i32;
}
pub fn list_office_certificates() -> Result<Vec<OfficeCertificate>, String> {
    let mut buffer = vec![0u8; 1024 * 1024];
    let count = unsafe { craft_windows_cert_inventory(buffer.as_mut_ptr(), buffer.len() as i32) };
    if count < 0 { return Err(format!("Windows certificate inventory failed ({count})")); }
    let mut certs: Vec<OfficeCertificate> = serde_json::from_slice(&buffer[..count as usize]).map_err(|e| e.to_string())?;
    certs.sort_by(|a,b| a.subject.cmp(&b.subject).then(a.thumbprint.cmp(&b.thumbprint)));
    Ok(certs)
}