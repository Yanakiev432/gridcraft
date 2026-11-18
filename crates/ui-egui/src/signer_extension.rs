//! First-party Craft Signer extension slot, protocol v1 (Windows QA).
//! The UI and its Review ribbon are part of the maintained host executable.
//! A separate, versioned worker survives updates of that host's own files.
//!
//! SECURITY: until trusted publisher signatures are available, worker SHA256 is
//! anchored in this host binary. The editable manifest is NOT itself trusted.
//! It must never supply an executable path or an arbitrary trust anchor.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const SCHEMA: &str = "craft.signer/1";
const EXTENSION_ID: &str = "org.craftopensource.signer";
const PROTOCOL_MAJOR: u32 = 1;
const HOST: &str = "gridcraft";
const FORMAT: &str = "xlsx";
const WORKER: &str = "gridcraft-cli.exe";
const TRUSTED_WORKER_SHA256: &str = "E86AD51AED75D54C22B6847921EE805CB1889DB3CA96D467D59AA7616B8E4CA2";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema: String,
    id: String,
    host: String,
    protocol_major: u32,
    document_type: String,
    worker_executable: String,
    worker_sha256: String,
    actions: Vec<String>,
}

fn expected_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != SCHEMA
        || manifest.id != EXTENSION_ID
        || manifest.host != HOST
        || manifest.protocol_major != PROTOCOL_MAJOR
        || manifest.document_type != FORMAT
        || manifest.worker_executable != WORKER
        || !manifest.worker_sha256.eq_ignore_ascii_case(TRUSTED_WORKER_SHA256)
        || manifest.actions.len() != 2
        || !manifest.actions.iter().any(|x| x == "SignDocument")
        || !manifest.actions.iter().any(|x| x == "VerifyDocument")
    {
        return Err("Signing extension manifest does not match this host's trusted v1 protocol.".into());
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("Cannot inspect signer worker: {e}"))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > 200_000_000 {
        return Err("Signing worker must be a regular file (max. 200 MB).".into());
    }
    let mut file = fs::File::open(path).map_err(|e| format!("Cannot open signer worker: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("Cannot read signer worker: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02X}")).collect())
}

fn verify_worker(path: PathBuf) -> Result<PathBuf, String> {
    let actual = sha256_file(&path)?;
    if !actual.eq_ignore_ascii_case(TRUSTED_WORKER_SHA256) {
        return Err("Signing worker digest does not match this host's pinned trusted release.".into());
    }
    Ok(path)
}

fn resolve_from_root(root: &Path) -> Result<Option<PathBuf>, String> {
    let slot = root.join("CraftSignerNativeUI").join("extensions").join(HOST);
    let manifest_path = slot.join("manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let md = fs::symlink_metadata(&manifest_path).map_err(|e| format!("Cannot inspect signing extension manifest: {e}"))?;
    if !md.file_type().is_file() || md.len() > 8192 {
        return Err("Invalid signing extension manifest file.".into());
    }
    // The manifest is a capability declaration, NOT a source of trusted paths.
    let json = fs::read(&manifest_path).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_slice(&json).map_err(|e| format!("Invalid signer v1 manifest: {e}"))?;
    expected_manifest(&manifest)?;
    // DO NOT read paths from JSON. Only the fixed host-specific worker is allowed.
    verify_worker(slot.join(WORKER)).map(Some)
}

pub fn resolve_worker() -> Result<PathBuf, String> {
    // Windows sets ProgramData to its system-wide data directory. Even if that
    // environment value is changed, an attacker cannot substitute an arbitrary
    // worker: the expected executable name and SHA256 are in the host binary.
    if let Some(program_data) = std::env::var_os("ProgramData") {
        if let Some(candidate) = resolve_from_root(Path::new(&program_data))? {
            return Ok(candidate);
        }
    }
    // Backward-compatible QA overlay: previous installer shipped the pinned
    // companion beside the GUI. It is also digest-verified; never use PATH.
    let sibling = std::env::current_exe().map_err(|e| format!("Cannot locate Craft application: {e}"))?.with_file_name(WORKER);
    verify_worker(sibling).map_err(|e| format!("{e} Reinstall the compatible Craft Signer module."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_wrong_schema_actions_and_untrusted_digest() {
        let mut m = Manifest {
            schema: SCHEMA.into(),
            id: EXTENSION_ID.into(),
            host: HOST.into(),
            protocol_major: 1,
            document_type: FORMAT.into(),
            worker_executable: WORKER.into(),
            worker_sha256: TRUSTED_WORKER_SHA256.into(),
            actions: vec!["SignDocument".into(), "VerifyDocument".into()],
        };
        assert!(expected_manifest(&m).is_ok());
        m.worker_executable = "../cmd.exe".into();
        assert!(expected_manifest(&m).is_err());
        m.worker_executable = WORKER.into();
        m.worker_sha256 = "0".repeat(64);
        assert!(expected_manifest(&m).is_err());
        m.worker_sha256 = TRUSTED_WORKER_SHA256.into();
        m.actions = vec!["SignDocument".into(), "SignDocument".into()];
        assert!(expected_manifest(&m).is_err());
    }
    #[test]
    fn missing_slot_returns_none_without_executing_code() {
        let root = std::env::temp_dir().join("craft-no-signer-slot-qa");
        assert!(resolve_from_root(&root).unwrap().is_none());
    }
}
