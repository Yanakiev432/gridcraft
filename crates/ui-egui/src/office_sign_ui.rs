//! Windows XLSX OPC signing workflow for the Review ribbon.
//! Cryptographic integrity does not establish certificate trust or eIDAS QES.
//! The GUI launches the matching pinned CLI in a separate, bounded process; Windows controls the key and PIN.
//! Never write into the source file or replace an existing signed destination.
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, mpsc::{self, Receiver, TryRecvError},
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use craft_signing_core::office_certificates::{OfficeCertificate, list_office_certificates};
use craft_signing_core::opc_native::verify_opc_inprocess;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Listing,
    Choosing,
    Signing,
    Verifying,
    Finished,
}
pub struct OfficeSignUi {
    extension: &'static str,
    phase: Phase,
    source: Option<PathBuf>,
    output: String,
    certificates: Vec<OfficeCertificate>,
    selected: Option<usize>,
    allow_additional: bool,
    inventory: Option<Receiver<Result<Vec<OfficeCertificate>, String>>>,
    worker: Option<Receiver<Result<String, String>>>,
    cancel: Arc<AtomicBool>,
    started: Option<Instant>,
    notice: String,
    success: bool,
}
impl OfficeSignUi {
    pub fn new(extension: &'static str) -> Self {
        assert!(matches!(extension, "docx" | "xlsx"));
        Self {
            extension,
            phase: Phase::Idle,
            source: None,
            output: String::new(),
            certificates: Vec::new(),
            selected: None,
            allow_additional: true,
            inventory: None,
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
            started: None,
            notice: String::new(),
            success: false,
        }
    }
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Listing | Phase::Choosing | Phase::Signing | Phase::Verifying)
    }
    fn ensure_source(&self, file: &Path) -> Result<(), String> {
        if !file.is_file() {
            return Err(format!("Save the workbook/document as .{} before using digital signatures.", self.extension));
        }
        if !file.extension().and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case(self.extension)) {
            return Err(format!("Only saved .{} files are supported for this signature workflow.", self.extension));
        }
        Ok(())
    }
    fn proposed_output(&self, src: &Path) -> Result<PathBuf, String> {
        let parent = src.parent().ok_or("Source folder is missing")?;
        let stem = src.file_stem().and_then(|s| s.to_str()).ok_or("Invalid source filename")?;
        for n in 1..=10_000 {
            let suffix = if n == 1 { String::new() } else { format!("-{n}") };
            let candidate = parent.join(format!("{stem}-signed{suffix}.{}", self.extension));
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        Err("Cannot choose a non-existing signed-copy filename".into())
    }
    pub fn begin(&mut self, src: &Path) -> Result<(), String> {
        if self.busy() { return Err("Another certificate/signature operation is active.".into()); }
        self.ensure_source(src)?;
        let absolute = src.canonicalize().map_err(|e| format!("Cannot access saved source: {e}"))?;
        self.output = self.proposed_output(&absolute)?.to_string_lossy().to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("craft-cert-inventory".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(list_office_certificates)
                    .unwrap_or_else(|_| Err("Windows certificate enumeration failed".into()));
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Cannot start certificate enumeration: {e}"))?;
        self.source = Some(absolute);
        self.selected = None; // Never silently pick a software certificate instead of the user's QES.
        self.certificates.clear();
        self.allow_additional = true;
        self.cancel.store(false, Ordering::SeqCst);
        self.inventory = Some(rx);
        self.worker = None;
        self.notice.clear();
        self.phase = Phase::Listing;
        Ok(())
    }
    pub fn verify(&mut self, src: &Path) -> Result<(), String> {
        if self.busy() { return Err("Complete the current signature operation first.".into()); }
        self.ensure_source(src)?;
        let src = src.canonicalize().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("craft-opc-verify".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    verify_opc_inprocess(&src).map(|n| format!(
                        "Integrity PASS: {n} native OPC signature(s) verified in {}. Certificate trust, qualified status and eIDAS QES were NOT assessed.",
                        src.display(),
                    ))
                }).unwrap_or_else(|_| Err("Signature verification worker failed".into()));
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Cannot start signature verification: {e}"))?;
        self.worker = Some(rx);
        self.started = Some(Instant::now());
        self.notice.clear();
        self.success = false;
        self.phase = Phase::Verifying;
        Ok(())
    }
    pub fn validate_destination(&self, destination: &Path) -> Result<(), String> {
        let source = self.source.as_deref().ok_or("No saved document selected")?;
        if !destination.is_absolute() { return Err("Enter an absolute destination path.".into()); }
        if destination == source || destination.exists() {
            return Err("The signed copy must be a NEW file; existing files will never be overwritten.".into());
        }
        if destination.extension().and_then(|s| s.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case(self.extension)) {
            return Err(format!("The signed copy must have a .{} extension.", self.extension));
        }
        if !destination.parent().is_some_and(|p| p.is_dir()) {
            return Err("The destination folder does not exist.".into());
        }
        Ok(())
    }
    fn start_sign(&mut self, current: Option<&Path>, dirty: bool) -> Result<(), String> {
        if self.phase != Phase::Choosing { return Err("Choose a certificate first.".into()); }
        if dirty { return Err("Save document changes before signing. Unsaved edits are not signed.".into()); }
        let source = self.source.clone().ok_or("Source document missing")?;
        if current.and_then(|p| p.canonicalize().ok()).as_deref() != Some(source.as_path()) {
            return Err("The active document changed. Reopen the signing dialog for that document.".into());
        }
        self.ensure_source(&source)?;
        let index = self.selected.ok_or("Select a signing certificate explicitly.")?;
        let cert = self.certificates.get(index).ok_or("Selected certificate no longer exists")?;
        if !cert.has_private_key || !cert.valid_now {
            return Err("Selected certificate has no usable private key or is not currently valid.".into());
        }
        let target = PathBuf::from(self.output.trim());
        self.validate_destination(&target)?;
        let thumbprint = cert.thumbprint.clone();
        let allow_additional = self.allow_additional;
        self.cancel.store(false, Ordering::SeqCst);
        let cancel = Arc::clone(&self.cancel);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("craft-opc-sign".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // The pinned CLI calls the native Windows OPC provider inside an isolated child.
                // Kill that process tree on cancellation or after the 180-second deadline.
                // No PowerShell signer or environment-supplied helper path.
                // Stable first-party extension slot: the versioned signer
                // worker lives outside the host installation. No arbitrary
                // executable paths from JSON or the search PATH are executed.
                let cli = crate::signer_extension::resolve_worker()?;
                let mut child = Command::new(cli)
                    .arg("opc-sign").arg(&source).arg(&target)
                    .arg("--thumbprint").arg(&thumbprint)
                    .arg(if allow_additional {
                        "--allow-additional-signatures"
                    } else {
                        "--restrict-additional-signatures"
                    })
                    .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                    .spawn().map_err(|e| format!("Cannot start the isolated signing companion: {e}"))?;
                let started = Instant::now();
                loop {
                    let cancelled = cancel.load(Ordering::SeqCst);
                    let timed_out = started.elapsed() >= Duration::from_secs(180);
                    if cancelled || timed_out {
                        // Windows TASKKILL /T also stops any provider helpers launched by the CLI.
                        let status = Command::new(r"C:\Windows\System32\taskkill.exe")
                            .arg("/F").arg("/T").arg("/PID").arg(child.id().to_string())
                            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                            .status().map_err(|e| format!("Cannot terminate signer process tree: {e}"))?;
                        if !status.success() { let _ = child.kill(); }
                        let _ = child.wait();
                        return Err(if cancelled {
                            "Signing cancelled. Check destination before retrying; any completed new copy is not overwritten."
                        } else {
                            "Signing exceeded 180 seconds and was terminated. Verify destination before retrying."
                        }.into());
                    }
                    match child.try_wait().map_err(|e| e.to_string())? {
                        Some(status) if status.success() => break,
                        Some(status) => return Err(format!(
                            "Isolated signer exited with {status}. No signature accepted; check token/PIN and file rights."
                        )),
                        None => std::thread::sleep(Duration::from_millis(100)),
                    }
                }
                let count = verify_opc_inprocess(&target)?;
                Ok(format!(
                    "Signature integrity PASS: {count} signature(s). Created NEW copy: {}. Certificate trust and eIDAS QES still require an independent validation.",
                    target.display(),
                ))
            })).unwrap_or_else(|_| Err("Windows signing worker failed; inspect output before retrying".into()));
            let _ = tx.send(result);
        }).map_err(|e| format!("Cannot start isolated signing: {e}"))?;
        self.worker = Some(rx);
        self.started = Some(Instant::now());
        self.phase = Phase::Signing;
        self.notice.clear();
        Ok(())
    }
    fn finish(&mut self, result: Result<String, String>) {
        self.success = result.is_ok();
        self.notice = match result {
            Ok(msg) => msg,
            Err(error) => format!("Signature operation failed: {error}"),
        };
        self.phase = Phase::Finished;
        self.started = None;
        self.worker = None;
        self.inventory = None;
    }
    fn poll(&mut self) {
        if self.phase == Phase::Listing {
            let done = self.inventory.as_ref().and_then(|r| match r.try_recv() {
                Ok(value) => Some(value),
                Err(TryRecvError::Disconnected) => Some(Err("Certificate enumeration disconnected".into())),
                Err(TryRecvError::Empty) => None,
            });
            if let Some(result) = done {
                self.inventory = None;
                match result {
                    Ok(certs) => { self.certificates = certs; self.phase = Phase::Choosing; }
                    Err(error) => self.finish(Err(error)),
                }
            }
        }
        if matches!(self.phase, Phase::Signing | Phase::Verifying) {
            let done = self.worker.as_ref().and_then(|r| match r.try_recv() {
                Ok(value) => Some(value),
                Err(TryRecvError::Disconnected) => Some(Err("Signing or verification worker disconnected".into())),
                Err(TryRecvError::Empty) => None,
            });
            if let Some(result) = done { self.finish(result); }
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, current: Option<&Path>, dirty: bool) {
        self.poll();
        if self.phase == Phase::Idle { return; }
        if matches!(self.phase, Phase::Listing | Phase::Signing | Phase::Verifying) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        let mut close = false;
        egui::Window::new(format!("{} digital signatures", self.extension.to_ascii_uppercase()))
            .id(egui::Id::new(format!("craft-opc-sign-{}", self.extension)))
            .collapsible(false)
            .resizable(true)
            .min_width(430.0)
            .show(ctx, |ui| {
                match self.phase {
                    Phase::Idle => {}
                    Phase::Listing => {
                        ui.spinner();
                        ui.label("Reading personal signing certificates from Windows. Smart-card middleware runs outside the UI thread.");
                        if ui.button("Cancel").clicked() { close = true; }
                    }
                    Phase::Choosing => {
                        ui.heading("Choose a digital signing certificate");
                        if let Some(p) = self.source.as_deref() { ui.label(format!("Saved source: {}", p.display())); }
                        ui.label("Only native Office OPC/XML digital signatures are created; this does not add a visible signature line.");
                        ui.separator();
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            for (idx, cert) in self.certificates.iter().enumerate() {
                                let eligible = cert.has_private_key && cert.valid_now;
                                let label = format!(
                                    "{}\nIssuer: {}\nValid until: {}{}",
                                    cert.subject, cert.issuer, cert.expires,
                                    if eligible { "" } else { "\nUnavailable for signing" },
                                );
                                ui.add_enabled_ui(eligible, |ui| { ui.radio_value(&mut self.selected, Some(idx), label); });
                                ui.separator();
                            }
                        });
                        if self.certificates.is_empty() { ui.label("No Windows signing certificates were found."); }
                        ui.separator();
                        ui.label("New signed-copy destination (must not already exist):");
                        ui.text_edit_singleline(&mut self.output);
                        ui.small("Native OPC permits additional independent signatures. Restricting later signatures is not available in this backend.");
                        ui.small("The token/provider opens its OWN PIN prompt. Do not enter a PIN into Craft.");
                        ui.small("A successful integrity check does not by itself certify the signer as qualified under eIDAS.");
                        if !self.notice.is_empty() { ui.colored_label(egui::Color32::YELLOW, &self.notice); }
                        ui.horizontal(|ui| {
                            let eligible = self.selected.and_then(|i| self.certificates.get(i))
                                .is_some_and(|cert| cert.has_private_key && cert.valid_now);
                            if ui.add_enabled(eligible, egui::Button::new("Sign new copy")).clicked() {
                                if let Err(err) = self.start_sign(current, dirty) { self.notice = err; }
                            }
                            if ui.button("Cancel").clicked() { close = true; }
                        });
                    }
                    Phase::Signing => {
                        ui.spinner();
                        ui.label("The isolated Windows signing worker is running. Check any B-Trust PIN dialog.");
                        if let Some(t) = self.started { ui.label(format!("Elapsed: {} seconds (worker limit: 180 seconds)", t.elapsed().as_secs())); }
                        if ui.button("Cancel signing").clicked() {
                            self.cancel.store(true, Ordering::SeqCst);
                            ui.label("Cancellation requested; waiting for worker confirmation.");
                        }
                        ui.label("Do not start another signing operation until this one completes.");
                    }
                    Phase::Verifying => {
                        ui.spinner();
                        ui.label("Checking native OPC signature integrity in a background worker...");
                    }
                    Phase::Finished => {
                        if self.success { ui.strong("Integrity check completed"); }
                        else { ui.colored_label(egui::Color32::RED, "Signature operation failed"); }
                        ui.label(&self.notice);
                        if ui.button("Close").clicked() { close = true; }
                    }
                }
            });
        if close && !matches!(self.phase, Phase::Signing | Phase::Verifying) {
            self.phase = Phase::Idle;
            self.inventory = None;
            self.worker = None;
            self.source = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refusal_to_overwrite_and_wrong_extension() {
        let mut dialog = OfficeSignUi::new("docx");
        let parent = std::env::temp_dir();
        let source = parent.join(format!("craft-doc-source-{}.docx", std::process::id()));
        std::fs::write(&source, b"sample").unwrap();
        dialog.source = Some(source.clone());
        assert!(dialog.validate_destination(&source).is_err());
        assert!(dialog.validate_destination(&source.with_extension("xlsx")).is_err());
        let unused = parent.join(format!("craft-doc-signed-not-created-{}.docx", std::process::id()));
        assert!(dialog.validate_destination(&unused).is_ok());
        std::fs::remove_file(source).unwrap();
    }
    #[test]
    fn no_invalid_source_can_open_signing_dialog() {
        let mut dialog = OfficeSignUi::new("xlsx");
        assert!(dialog.begin(Path::new("not-a-saved-document.xlsx")).is_err());
        assert!(!dialog.busy());
    }
}
