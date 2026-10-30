//! In-process Windows OPC signature bridge. No PowerShell, CMD or subprocess.
//! Integrity validation is not certificate trust or eIDAS QES qualification.
#![cfg(windows)]
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT_OPC_STAGE: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn craft_opc_verify_inprocess(source: *const u16, count: *mut i32) -> i32;
    fn craft_opc_sign_with_policy_inprocess(source: *const u16, dest: *const u16, sha1: *const u16, deny_additional: i32) -> i32;
}
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}
fn error(stage: &str, hr: i32) -> String {
    format!(
        "{stage} rejected by Windows OPC (HRESULT=0x{:08X}); trust, revocation and QES not assessed",
        hr as u32
    )
}

/// Validates signed XML and covered OPC references without starting a shell.
/// Package-wide OPC part coverage is enforced by the native implementation.
/// A successful result does not establish certificate trust or eIDAS QES.
pub fn verify_opc_inprocess(path: &Path) -> Result<usize, String> {
    let absolute = path
        .canonicalize()
        .map_err(|_| "source file does not exist".to_string())?;
    if !absolute.is_file() {
        return Err("source is not a regular file".into());
    }
    let source = wide(absolute.as_os_str());
    let mut count = 0i32;
    let hr = unsafe { craft_opc_verify_inprocess(source.as_ptr(), &mut count) };
    if hr < 0 || count <= 0 {
        return Err(error("native OPC verification", hr));
    }
    Ok(count as usize)
}

/// Signs a new DOCX/XLSX with explicit CurrentUser/My SHA1 certificate thumbprint.
/// Hardware private key remains with its Windows provider and official PIN UI.
/// No existing output is replaced. Verification is performed before final rename.
pub fn sign_opc_inprocess(
    source: &Path,
    output: &Path,
    sha1_thumbprint: &str,
) -> Result<(), String> {
    sign_opc_inprocess_with_policy(source, output, sha1_thumbprint, true)
}

/// When false, embeds a cryptographically signed Craft-only no-further-signing
/// marker. Craft refuses a second signature; other Office applications are
/// not obliged to enforce this application-specific restriction.
fn await_bounded_child(
    child: &mut std::process::Child,
    cancel: &std::sync::atomic::AtomicBool,
    max_duration: std::time::Duration,
) -> Result<std::process::ExitStatus,String> {
    use std::{sync::atomic::Ordering as Mem,thread,time::{Duration,Instant}};
    let started=Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if cancel.load(Mem::SeqCst) => {
                let _=child.kill();
                let _=child.wait();
                return Err("Isolated signer cancelled; no copy published".into());
            }
            Ok(None) if started.elapsed()<max_duration =>
                thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _=child.kill();
                let _=child.wait();
                return Err("Isolated signer timed out and was terminated; no copy published".into());
            }
            Err(e) => {
                let _=child.kill();
                let _=child.wait();
                return Err(format!("Could not monitor isolated signer, terminated: {e}"));
            }
        }
    }
}
fn cleanup_staged_worker_outputs(stage: &Path) {
    // A terminated worker may have left its own *.raw or *.normalized files.
    // Remove only files bearing this request's unpredictable stage prefix.
    let Some(parent)=stage.parent() else {return};
    let Some(name)=stage.file_name().and_then(|n|n.to_str()) else {return};
    let prefix=format!("{name}.craft-opc-");
    if let Ok(entries)=std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let filename=entry.file_name();
            let Some(filename)=filename.to_str() else {continue};
            if filename.starts_with(&prefix)
                && (filename.ends_with(".raw") || filename.ends_with(".normalized")) {
                let _=std::fs::remove_file(entry.path());
            }
        }
    }
}
/// Run the private-key operation in a separate, killable process.
/// The worker sees only a randomized staging path, never the user's final path.
/// No file is published if the child fails, times out or disappears.
/// Timeout is deliberately fixed; environment variables cannot extend it.
pub fn sign_opc_inprocess_with_policy(
    source: &Path, output: &Path, sha1_thumbprint: &str, allow_additional: bool,
) -> Result<(), String> {
    let never=std::sync::atomic::AtomicBool::new(false);
    sign_opc_inprocess_with_policy_cancelable(source,output,sha1_thumbprint,allow_additional,&never)
}

/// Cancels a running isolated Windows OPC worker and prevents publication
/// of any new signed document after cancellation has been observed.
pub fn sign_opc_inprocess_with_policy_cancelable(
    source: &Path,
    output: &Path,
    sha1_thumbprint: &str,
    allow_additional: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    use std::{process::{Command, Stdio}, time::Duration};
    const MAX_SIGNING_TIME: Duration = Duration::from_secs(180);
    if cancel.load(Ordering::SeqCst) {return Err("Signing cancelled before provider launch".into());}
    if source == output || output.exists() {
        return Err("signing output must be a new and different file".into());
    }
    if !source.is_file() {
        return Err("source file does not exist".into());
    }
    let source = source.canonicalize().map_err(|e|e.to_string())?;
    let output = if output.is_absolute() { output.to_path_buf() }
        else { std::env::current_dir().map_err(|e|e.to_string())?.join(output) };
    if !output.parent().is_some_and(|d|d.is_dir()) {
        return Err("signed output folder missing".into());
    }
    // Important: never accept an arbitrary uploaded worker path or shell command.
    let worker = std::env::current_exe().map_err(|e|e.to_string())?
        .with_file_name("craft-opc-worker.exe");
    if !worker.is_file() {
        return Err("Signing disabled: isolated craft-opc-worker.exe is missing.".into());
    }
    let sequence=NEXT_OPC_STAGE.fetch_add(1,Ordering::Relaxed);
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e|e.to_string())?.as_nanos();
    let ext=source.extension().and_then(|x|x.to_str()).ok_or("Source extension required")?;
    let stage=output.parent().unwrap().join(format!(
        ".craft-worker-stage-{}-{stamp}-{sequence}.{ext}",std::process::id()));
    if stage.exists(){return Err("Worker stage path collision".into());}
    let normalized=stage.with_file_name(format!(
        "{}.office-ct-normalized.{}",stage.file_stem().and_then(|s|s.to_str()).ok_or("Invalid stage name")?,ext));
    if normalized.exists(){return Err("Office normalized path collision".into());}
    let result=(||{
        let mut child=Command::new(&worker)
            .arg("--craft-worker-sign-v1")
            .arg(sha1_thumbprint)
            .arg(if allow_additional {"allow"} else {"craft-final"})
            .arg(&source)
            .arg(&stage)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().map_err(|e|format!("Unable to start isolated OPC worker: {e}"))?;
        let status=await_bounded_child(&mut child,cancel,MAX_SIGNING_TIME)?;
        if !status.success() {
            return Err(format!("Isolated OPC worker failed (exit status {status}); no signed copy published"));
        }
        if cancel.load(Ordering::SeqCst) {return Err("Signing cancelled before publication".into());}
        let count=verify_opc_inprocess(&stage)?;
        if count==0 {return Err("Isolated signing produced no signatures".into());}
        crate::office_ct::normalize_office_content_types(&stage,&normalized)?;
        let normalized_count=verify_opc_inprocess(&normalized)?;
        if normalized_count!=count {return Err("Office MIME normalization invalidated signature".into());}
        // Final publication is atomic and refuses existing files, even in a race.
        if cancel.load(Ordering::SeqCst) {return Err("Signing cancelled before publication".into());}
        std::fs::hard_link(&normalized,&output)
            .map_err(|e|format!("Signed copy was not published: {e}"))?;
        Ok(())
    })();
    let _=std::fs::remove_file(&stage);
    let _=std::fs::remove_file(&normalized);
    cleanup_staged_worker_outputs(&stage);
    result
}

/// Worker-only direct implementation, never call on the application's UI thread.
/// The child is invoked exclusively by the guarded parent wrapper above.
pub fn sign_opc_worker_direct(

    source: &Path,
    output: &Path,
    sha1_thumbprint: &str,
    allow_additional: bool,
) -> Result<(), String> {
    if sha1_thumbprint.len() != 40 || !sha1_thumbprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("explicit 40-hex certificate SHA1 thumbprint required".into());
    }
    if output.exists() || source == output {
        return Err("output must be nonexistent and distinct".into());
    }
    if source.extension().and_then(|x| x.to_str()).is_none_or(|x| {
        !["docx", "xlsx"]
            .iter()
            .any(|ext| x.eq_ignore_ascii_case(ext))
    }) {
        return Err("source must be a DOCX or XLSX document".into());
    }
    let absolute = source
        .canonicalize()
        .map_err(|_| "source document missing".to_string())?;
    if !absolute.is_file() {
        return Err("source must be a regular file".into());
    }
    let abs_output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(output)
    };
    // Never expose a half-normalized output as the user's signed file.
    // Native OPC signs into a unique sibling; ZIP is rewritten in-process,
    // both original and normalized signed archives are checked independently.
    let sequence = NEXT_OPC_STAGE.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stem = format!(".craft-opc-{}-{stamp}-{sequence}", std::process::id());
    let mut raw_name = abs_output.as_os_str().to_os_string();
    raw_name.push(format!("{stem}.raw"));
    let raw = PathBuf::from(raw_name);
    let mut stage_name = abs_output.as_os_str().to_os_string();
    stage_name.push(format!("{stem}.normalized"));
    let normalized = PathBuf::from(stage_name);
    if raw.exists() || normalized.exists() {
        return Err("temporary OPC output name collision".into());
    }
    let result = (|| {
        let src = wide(absolute.as_os_str());
        let dst = wide(raw.as_os_str());
        let identity = wide(OsStr::new(sha1_thumbprint));
        let hr = unsafe {
            craft_opc_sign_with_policy_inprocess(
                src.as_ptr(), dst.as_ptr(), identity.as_ptr(), i32::from(!allow_additional)
            )
        };
        if hr < 0 {
            return Err(error("native OPC signing", hr));
        }
        let before = verify_opc_inprocess(&raw)?;
        crate::opc_zip::normalize_container(&raw, &normalized)?;
        let after = verify_opc_inprocess(&normalized)?;
        if before != after {
            return Err("ZIP normalization changed the verified signature count".into());
        }
        // The final path must be created atomically without replacement.
        // A separate process might create it after our earlier exists() check.
        // Staging and destination are siblings on the same filesystem, so
        // hard_link refuses an existing name without changing validated bytes.
        std::fs::hard_link(&normalized, &abs_output)
            .map_err(|e| format!("atomic no-overwrite OPC publication failed: {e}"))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&raw);
    let _ = std::fs::remove_file(&normalized);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_prevents_worker_launch_or_document_publication() {
        use std::sync::atomic::AtomicBool;
        let canceled=AtomicBool::new(true);
        let source=Path::new("does-not-exist.docx");
        let output=Path::new("no-published-signature.docx");
        let error=sign_opc_inprocess_with_policy_cancelable(source,output,&"A".repeat(40),true,&canceled)
            .unwrap_err();
        assert!(error.contains("cancelled"));
        assert!(!output.exists());
    }
    #[test]
    fn isolated_worker_timeout_and_cancel_kill_the_actual_process() {
        use std::{process::Command,sync::atomic::AtomicBool,time::Duration};
        let command="C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
        for cancel_first in [false,true] {
            let mut child=Command::new(command).args(["-NoProfile","-NonInteractive",
                "-Command","Start-Sleep -Seconds 10"]).spawn().unwrap();
            let flag=AtomicBool::new(cancel_first);
            let result=await_bounded_child(&mut child,&flag,Duration::from_millis(250));
            assert!(result.is_err(), "mock signer must be cancelled or terminated");
            assert!(child.try_wait().unwrap().is_some(), "child process must not remain active");
        }
    }

    #[test]
    fn aborted_worker_cleanup_never_removes_unrelated_files() {
        use std::fs;
        let dir=std::env::temp_dir().join(format!("craft-stage-cleanup-test-{}",std::process::id()));
        let _=fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let stage=dir.join(".craft-worker-stage-secret.xlsx");
        let raw=dir.join(".craft-worker-stage-secret.xlsx.craft-opc-100-101-0.raw");
        let normalized=dir.join(".craft-worker-stage-secret.xlsx.craft-opc-100-101-0.normalized");
        let unrelated=dir.join("signed.xlsx");
        fs::write(&raw,b"raw").unwrap();
        fs::write(&normalized,b"normalized").unwrap();
        fs::write(&unrelated,b"keep").unwrap();
        cleanup_staged_worker_outputs(&stage);
        assert!(!raw.exists() && !normalized.exists());
        assert_eq!(fs::read(&unrelated).unwrap(),b"keep");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn native_signer_avoids_redundant_key_acquisition_preflight() {
        // Guard against reintroducing the observed B-Trust hang.
        let native = include_str!("../native/opc_native.cpp");
        assert!(!native.contains("CryptAcquireCertificatePrivateKey("));
        assert!(native.contains("provider_preacquire_disabled"));
        assert!(native.contains("manager->Sign(cert,options.Get(),&signature)"));
    }

    #[test]
    fn office_origin_is_declared_before_provider_sign_and_rejects_generic_legacy_origin() {
        let native=include_str!("../native/opc_native.cpp");
        assert!(native.contains("L\"/_xmlsignatures/origin.sigs\""));
        assert!(native.contains("manager->SetSignatureOriginPartName(officeOrigin.Get())"));
        assert!(native.contains("office_origin_assigned"));
        assert!(native.contains("office_origin_existing_validated"));
        assert!(native.contains("ERROR_NOT_SUPPORTED"));
        let origin=native.find("manager->SetSignatureOriginPartName(").unwrap();
        let provider=native.find("manager->Sign(cert,options.Get(),&signature)").unwrap();
        assert!(origin<provider, "Origin must be fixed before OPC signs any bytes");
    }

    #[test]
    fn office_object_is_signed_and_has_schema_required_fields() {
        let native = include_str!("../native/opc_native.cpp");
        // MS-OFFCRYPTO 2.5.2.5: these fields are mandatory in SignatureInfoV1.
        let mandatory = [
            "SetupID", "SignatureText", "SignatureImage", "SignatureComments",
            "WindowsVersion", "OfficeVersion", "ApplicationVersion", "Monitors",
            "HorizontalResolution", "VerticalResolution", "ColorDepth",
            "SignatureProviderId", "SignatureProviderUrl", "SignatureProviderDetails",
            "SignatureType"
        ];
        let mut last = native.find("<SignatureInfoV1").unwrap();
        for element in mandatory {
            let at=native[last..].find(&format!("<{element}"))
                .unwrap_or_else(||panic!("Office signature field missing: {element}"))+last;
            assert!(at>=last,"Office signature fields must follow schema sequence");
            last=at;
        }
        assert!(native.contains("Id=\\\"idOfficeObject\\\""));
        assert!(native.contains("Target=\\\"#idPackageSignature\\\""));
        assert!(native.contains("SetSignatureId(L\"idPackageSignature\")"));
        assert!(native.contains("CreateUri(L\"#idOfficeObject\""));
        assert!(native.contains("GetCustomObjectSet(&objects)"));
        assert!(native.contains("GetCustomReferenceSet(&refs)"));
        assert!(native.contains("refs->Create(officeRef.Get()"));
        assert!(native.contains("OPC_CANONICALIZATION_C14N"));
        assert!(native.contains("SetSignaturePartName(signaturePart.Get())"));
        assert!(native.contains("L\"/_xmlsignatures/sig%d.xml\""));
        assert!(native.contains("add_office_signature_object(options.Get())"));
        let object=native.find("add_office_signature_object(options.Get())").unwrap();
        let sign=native.find("manager->Sign(cert,options.Get(),&signature)").unwrap();
        assert!(object<sign, "Office object must be constructed before Sign");
        assert!(include_str!("../build.rs").contains("rustc-link-lib=urlmon"));
    }

    #[test]
    fn craft_final_policy_rejects_followup_in_both_modes_before_any_key_access() {
        use std::io::Write;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("craft-final-policy-test-{nonce}-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let doc = dir.join("source.docx");
        {
            let file = std::fs::OpenOptions::new().create_new(true).write(true).open(&doc).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            let entries = [
                ("[Content_Types].xml",
                 r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/craft/signature-policy.xml" ContentType="application/vnd.craft.signing-policy+xml"/></Types>"#),
                ("_rels/.rels",
                 r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#),
                ("word/document.xml",
                 r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p/></w:body></w:document>"#),
                ("craft/signature-policy.xml",
                 r#"<craft:signaturePolicy xmlns:craft="urn:craft:signing:policy:v1" allowAdditionalSignatures="false"/>"#),
            ];
            for (name, value) in entries {
                zip.start_file(name, options).unwrap();
                zip.write_all(value.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let identity = "A".repeat(40); // Synthetic ID: never matches a real certificate.
        for allow in [true, false] {
            let out = dir.join(if allow { "more.docx" } else { "final.docx" });
            let error = sign_opc_worker_direct(&doc, &out, &identity, allow).unwrap_err();
            assert!(error.contains("80070005"), "expected Craft policy denial, got {error}");
            assert!(!out.exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_implicit_identity_non_office_or_missing_file() {
        assert!(sign_opc_inprocess(Path::new("missing.docx"), Path::new("out.docx"), "").is_err());
        assert!(
            sign_opc_inprocess(Path::new("test.pdf"), Path::new("out.pdf"), &"a".repeat(40))
                .is_err()
        );
        assert!(verify_opc_inprocess(Path::new("missing.docx")).is_err());
    }
}
