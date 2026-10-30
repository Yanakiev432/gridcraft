//! Windows CurrentUser Personal store signing provider.
//! Private keys stay with Windows CNG (including hardware tokens).
use crate::{Digest, IdentityRef, Provider, SigningProvider, SigningRequest};
use rustls_cng::key::{AlgorithmGroup, SignaturePadding};
use rustls_cng::store::{CertStore, CertStoreType};
use sha2::{Digest as _, Sha256, Sha384, Sha512};

pub struct WindowsCngProvider;

fn reference(der: &[u8]) -> String {
    let hash = Sha256::digest(der);
    let mut id = String::from("windows:");
    for b in hash {
        use std::fmt::Write;
        let _ = write!(id, "{b:02x}");
    }
    id
}

impl SigningProvider for WindowsCngProvider {
    fn kind(&self) -> Provider {
        Provider::WindowsCng
    }

    /// Lists certificates with a CNG key that opens without prompting, plus
    /// certificates whose key needs interactive UI. No PIN prompt on listing.
    fn list_identities(&self) -> Result<Vec<IdentityRef>, String> {
        let store = CertStore::open(CertStoreType::CurrentUser, "My")
            .map_err(|e| format!("Windows Personal store open failed: {e:?}"))?;
        let mut result = Vec::new();
        for context in store
            .find_all()
            .map_err(|e| format!("Windows Personal store read failed: {e:?}"))?
        {
            // 0x80090022 NTE_SILENT_CONTEXT; 0x8009002E NTE_UI_REQUIRED
            let usable = match context.acquire_key(true) {
                Ok(key) => matches!(key.algorithm_group(), Ok(AlgorithmGroup::Rsa)),
                Err(rustls_cng::error::CngError::WindowsError(0x80090022 | 0x8009002E)) => true,
                Err(_) => false,
            };
            if usable {
                result
                    .push(IdentityRef::parse(&reference(context.as_der())).map_err(str::to_owned)?);
            }
        }
        Ok(result)
    }

    /// Signs message bytes, not an already hashed digest. RSA PKCS#1 v1.5
    /// only for this first adapter; ECDSA is rejected until DER conversion
    /// and curve checks are implemented. May trigger the Windows PIN UI.
    fn sign(&self, request: SigningRequest<'_>) -> Result<Vec<u8>, String> {
        if request.identity.provider != self.kind() {
            return Err("provider mismatch".into());
        }
        let store = CertStore::open(CertStoreType::CurrentUser, "My")
            .map_err(|e| format!("Windows Personal store open failed: {e:?}"))?;
        for context in store
            .find_all()
            .map_err(|e| format!("Windows Personal store read failed: {e:?}"))?
        {
            if reference(context.as_der()).strip_prefix("windows:")
                != Some(request.identity.id.as_str())
            {
                continue;
            }
            // Explicit interactive acquisition, never silently bypass the token's PIN.
            let key = context
                .acquire_key(false)
                .map_err(|e| format!("Windows signing key unavailable or cancelled: {e:?}"))?;
            if key.algorithm_group().map_err(|e| format!("{e:?}"))? != AlgorithmGroup::Rsa {
                return Err("unsupported key type: only RSA PKCS#1 v1.5 implemented".into());
            }
            let digest = match request.digest {
                Digest::Sha256 => Sha256::digest(request.message).to_vec(),
                Digest::Sha384 => Sha384::digest(request.message).to_vec(),
                Digest::Sha512 => Sha512::digest(request.message).to_vec(),
            };
            return key
                .sign(&digest, SignaturePadding::Pkcs1)
                .map_err(|e| format!("Windows signing failed or cancelled: {e:?}"));
        }
        Err("signing identity not found in CurrentUser Personal store".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_is_stable_and_namespaced() {
        assert_eq!(
            reference(b"abc"),
            "windows:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
