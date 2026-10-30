//! Reusable contracts; production cryptography remains in format/provider adapters.
//! OOXML signatures use OPC/XMLDSig, not PDF PAdES/CMS serialization.
pub mod format_contract;
pub mod office_appearance;
#[cfg(windows)]
pub mod office_visible_opc;
#[cfg(windows)]
pub mod office_appearance_render;
#[cfg(windows)]
pub mod office_visible_sign;
#[cfg(windows)]
pub mod opc_native;
#[cfg(windows)]
mod office_ct;
#[cfg(windows)]
pub mod office_excel_calc;

#[cfg(windows)]
mod opc_zip;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    WindowsCng,
    MacosKeychain,
    Pkcs11,
    Pkcs12,
    Remote,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityRef {
    pub provider: Provider,
    pub id: String,
}
impl IdentityRef {
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        let (p, id) = s.split_once(':').ok_or("missing namespace")?;
        if id.is_empty() || id.trim() != id || id.chars().any(char::is_control) {
            return Err("invalid identifier");
        }
        let provider = match p {
            "windows" => Provider::WindowsCng,
            "keychain" => Provider::MacosKeychain,
            "pkcs11" => Provider::Pkcs11,
            "pkcs12" => Provider::Pkcs12,
            "remote" => Provider::Remote,
            _ => return Err("unknown provider"),
        };
        Ok(Self {
            provider,
            id: id.to_owned(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Digest {
    Sha256,
    Sha384,
    Sha512,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Valid,
    Invalid,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    pub integrity: Verdict,
    pub chain_trust: Verdict,
    pub revocation: Verdict,
    pub timestamp: Verdict,
    pub details: Vec<String>,
}
/// The provider hashes the message exactly once and applies algorithm-specific
/// padding and encoding. Do not supply a prehashed digest.
pub struct SigningRequest<'a> {
    pub identity: &'a IdentityRef,
    pub digest: Digest,
    pub message: &'a [u8],
}
pub trait SigningProvider: Send + Sync {
    fn kind(&self) -> Provider;
    fn list_identities(&self) -> Result<Vec<IdentityRef>, String>;
    fn sign(&self, request: SigningRequest<'_>) -> Result<Vec<u8>, String>;
}
pub trait DocumentSignatureAdapter {
    fn format(&self) -> &'static str;
    fn sign(&self, original: &[u8], id: &IdentityRef) -> Result<Vec<u8>, String>;
    fn verify(&self, document: &[u8]) -> Result<Verification, String>;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_validation() {
        assert_eq!(
            IdentityRef::parse("windows:abc").unwrap().provider,
            Provider::WindowsCng
        );
        for bad in [
            "",
            "windows:",
            "unknown:abc",
            "windows: abc",
            "windows:a\nb",
        ] {
            assert!(IdentityRef::parse(bad).is_err());
        }
    }
    struct MockProvider;
    impl SigningProvider for MockProvider {
        fn kind(&self) -> Provider {
            Provider::WindowsCng
        }
        fn list_identities(&self) -> Result<Vec<IdentityRef>, String> {
            Ok(vec![
                IdentityRef::parse("windows:test").map_err(str::to_owned)?,
            ])
        }
        fn sign(&self, request: SigningRequest<'_>) -> Result<Vec<u8>, String> {
            if request.identity.provider != self.kind() {
                return Err("provider mismatch".into());
            }
            if request.identity.id != "test" {
                return Err("unknown identity".into());
            }
            if request.message != b"message" {
                return Err("unexpected message".into());
            }
            Ok(vec![1, 2, 3])
        }
    }
    #[test]
    fn mock_provider_routing() {
        let provider = MockProvider;
        let identity = provider.list_identities().unwrap().remove(0);
        assert_eq!(
            provider
                .sign(SigningRequest {
                    identity: &identity,
                    digest: Digest::Sha256,
                    message: b"message"
                })
                .unwrap(),
            vec![1, 2, 3]
        );
        let wrong = IdentityRef::parse("pkcs11:test").unwrap();
        assert!(
            provider
                .sign(SigningRequest {
                    identity: &wrong,
                    digest: Digest::Sha256,
                    message: b"message"
                })
                .is_err()
        );
    }
    #[test]
    fn trust_is_independent() {
        let result = Verification {
            integrity: Verdict::Valid,
            chain_trust: Verdict::Unknown,
            revocation: Verdict::Unknown,
            timestamp: Verdict::Unknown,
            details: vec![],
        };
        assert_ne!(result.integrity, result.chain_trust);
    }
}
#[cfg(target_os = "windows")]
pub mod office_certificates;
#[cfg(target_os = "windows")]
pub mod opc_windows;
#[cfg(target_os = "windows")]
pub mod windows_cng;
#[cfg(target_os = "windows")]
pub mod windows_trust;
