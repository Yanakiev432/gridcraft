//! Windows Office OPC/XMLDSig signer backed by the OS Packaging API.
//!
//! A private key is never read here. X509Certificate2 selects an explicit
//! CurrentUser/My certificate and Windows CSP/CNG performs the operation.
//! Verification is package integrity, NOT certificate qualification or eIDAS QES.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SCRIPT: &str = include_str!("../tools/opc_signature_windows.ps1");

struct StagedScript {
    dir: PathBuf,
    script: PathBuf,
}
impl StagedScript {
    fn create() -> Result<Self, String> {
        for attempt in 0..12 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "craft-opc-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            if fs::create_dir(&dir).is_err() {
                continue;
            }
            let script = dir.join("opc_signature_windows.ps1");
            let write = (|| -> std::io::Result<()> {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&script)?;
                file.write_all(SCRIPT.as_bytes())?;
                file.sync_all()?;
                Ok(())
            })();
            if let Err(e) = write {
                let _ = fs::remove_dir_all(&dir);
                return Err(format!("could not stage OPC signing helper: {e}"));
            }
            return Ok(Self { dir, script });
        }
        Err("could not create an isolated OPC helper directory".into())
    }
}
impl Drop for StagedScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.script);
        let _ = fs::remove_dir(&self.dir);
    }
}

fn powershell() -> PathBuf {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

fn execute(
    mode: &str,
    input: &Path,
    output: Option<&Path>,
    thumbprint: Option<&str>,
    allow_additional_signatures: Option<bool>,
) -> Result<String, String> {
    let guard = StagedScript::create()?;
    let mut cmd = Command::new(powershell());
    cmd.arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-File")
        .arg(&guard.script)
        .arg("-Mode")
        .arg(mode)
        .arg("-InputFile")
        .arg(input);
    if let Some(output) = output {
        cmd.arg("-OutputFile").arg(output);
    }
    if let Some(thumbprint) = thumbprint {
        cmd.arg("-CertificateThumbprint").arg(thumbprint);
    }
    if let Some(allow) = allow_additional_signatures {
        cmd.arg("-AdditionalSignatures")
            .arg(if allow { "allow" } else { "restrict" });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let result = cmd
        .output()
        .map_err(|e| format!("Windows OPC helper could not start: {e}"))?;
    let output = String::from_utf8(result.stdout)
        .map_err(|_| "Windows OPC helper stdout was not valid UTF-8".to_owned())?;
    if result.status.success() {
        return Ok(output.trim().to_owned());
    }
    let err = String::from_utf8_lossy(&result.stderr);
    Err(format!(
        "OPC {} rejected (exit {}): {}",
        mode,
        result.status,
        err.chars().take(1500).collect::<String>()
    ))
}

/// Sign one unsigned DOCX or XLSX document with an explicit Windows Personal
/// certificate. The target must not exist; original bytes are never replaced.
/// The Windows vendor may show an interactive smart-card PIN prompt.
pub fn sign_office_document(
    input: &Path,
    output: &Path,
    cert_thumbprint: &str,
) -> Result<String, String> {
    if cert_thumbprint.len() != 40 || !cert_thumbprint.as_bytes().iter().all(u8::is_ascii_hexdigit)
    {
        return Err("certificate thumbprint must be exactly 40 hexadecimal characters".into());
    }
    if input == output {
        return Err("source and destination must differ".into());
    }
    sign_office_document_with_policy(input, output, cert_thumbprint, true)
}

/// Sign a DOCX/XLSX document. When true (recommended), other people may
/// subsequently add independent OPC signatures without invalidating earlier
/// signatures. When false, Craft embeds a signed application-specific policy
/// that blocks future signing in Craft only; this is not a universal Microsoft
/// Office read-only/document-protection mechanism.
pub fn sign_office_document_with_policy(
    input: &Path,
    output: &Path,
    cert_thumbprint: &str,
    allow_additional_signatures: bool,
) -> Result<String, String> {
    if cert_thumbprint.len() != 40 || !cert_thumbprint.as_bytes().iter().all(u8::is_ascii_hexdigit)
    {
        return Err("certificate thumbprint must be exactly 40 hexadecimal characters".into());
    }
    if input == output {
        return Err("source and destination must differ".into());
    }
    execute(
        "sign",
        input,
        Some(output),
        Some(cert_thumbprint),
        Some(allow_additional_signatures),
    )
}

/// Independently reopen the document through Windows OPC/XMLDSig and reject
/// invalid signatures or uncovered content parts. This does not imply QES,
/// trusted timestamping, revocation validation or Microsoft Office UI parity.
pub fn verify_office_document(input: &Path) -> Result<String, String> {
    execute("verify", input, None, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_implicit_identity() {
        for invalid in ["", "abcd", "nothexnothexnothexnothexnothexnothexnothex00"] {
            assert!(
                sign_office_document(Path::new("a.docx"), Path::new("b.docx"), invalid).is_err()
            );
        }
    }
    #[test]
    fn staging_lifetime_is_bounded() {
        let s = StagedScript::create().expect("stage helper");
        assert!(s.script.is_file());
        let dir = s.dir.clone();
        drop(s);
        assert!(!dir.exists());
    }
}
