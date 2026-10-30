//! Read-only access to Windows trusted root stores.
//! Certificate presence is NOT proof of revocation status or eIDAS QES.
use rustls_cng::store::{CertStore, CertStoreType};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootCertificate {
    pub der: Vec<u8>,
}

/// Read CurrentUser and LocalMachine Root stores, deduplicated by DER bytes.
/// This does not implement Windows chain-policy evaluation or the Disallowed store.
pub fn trusted_root_der() -> Result<Vec<RootCertificate>, String> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    for kind in [CertStoreType::CurrentUser, CertStoreType::LocalMachine] {
        let store = CertStore::open(kind, "Root")
            .map_err(|e| format!("Windows Root store open failed: {e:?}"))?;
        let contexts = store
            .find_all()
            .map_err(|e| format!("Windows Root store read failed: {e:?}"))?;
        for context in contexts {
            let der = context.as_der().to_vec();
            if seen.insert(der.clone()) {
                roots.push(RootCertificate { der });
            }
        }
    }
    Ok(roots)
}
