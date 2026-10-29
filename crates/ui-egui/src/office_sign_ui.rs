//! Adapted from PdfCraft's Digital ID choose/draw/confirm interaction.
//! Office documents use OPC/XMLDSig via the common Craft signing core,
//! NOT PDF PAdES. A drawn rectangle maps to an XLSX drawing picture anchor.
//! It is not a native Excel signature-line control.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::fs::{File,OpenOptions};

use craft_signing_core::office_certificates::{OfficeCertificate, list_office_certificates};
use craft_signing_core::office_appearance::OfficeSignatureAppearance;
use craft_signing_core::office_visible_opc::OfficeImagePlacement;
use craft_signing_core::office_visible_sign::sign_with_visible_appearance_with_policy_cancelable;
use craft_signing_core::opc_native::{sign_opc_inprocess_with_policy_cancelable, verify_opc_inprocess};
use craft_signing_core::office_excel_calc::prepare_xlsx_for_excel_signing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Closed,
    Loading,
    Choose,
    Draw,
    Confirm,
    Signing,
}
#[derive(Default)]
pub struct OfficeSignUi {
    signature_mode: XlsxSignatureMode,
    phase: Phase,
    certificates: Vec<OfficeCertificate>,
    selected: Option<usize>,
    allow_additional_signatures: bool,
    excel_manual_calc_consent: bool,
    appearance: OfficeSignatureAppearance,
    embed_visible_at_default_location: bool,
    excel_anchor_cell: String,
    drag_start: Option<egui::Pos2>,
    rect: Option<egui::Rect>,
    excel_region: Option<ExcelRegion>,
    source: Option<PathBuf>,
    pub notice: Option<String>,
    inventory_rx: Option<Receiver<Result<Vec<OfficeCertificate>, String>>>,
    sign_rx: Option<Receiver<Result<String, String>>>,
    verify_rx: Option<Receiver<Result<String, String>>>,
    sign_started: Option<Instant>,
    sign_cancel: Option<Arc<AtomicBool>>,
}
pub enum Action {
    Sign,
}
#[derive(Clone, Copy, Debug)]
struct ExcelRegion {
    column: u32, row: u32, col_offset: u32, row_offset: u32,
    width: u32, height: u32,
}

#[derive(Clone,Copy,Debug,PartialEq,Eq,Default)]
enum XlsxSignatureMode {
    #[default]
    DetachedAutomatic,
    NativeOfficeAutomatic,
    NativeOfficeManual,
}

fn same_bytes(first: &Path, second: &Path) -> Result<bool,String> {
    let mut a=File::open(first).map_err(|e|e.to_string())?;
    let mut b=File::open(second).map_err(|e|e.to_string())?;
    if a.metadata().map_err(|e|e.to_string())?.len()!=b.metadata().map_err(|e|e.to_string())?.len(){
        return Ok(false);
    }
    let (mut a_buf,mut b_buf)=([0u8;65536],[0u8;65536]);
    loop {
        let n=a.read(&mut a_buf).map_err(|e|e.to_string())?;
        if n==0 {return Ok(b.read(&mut b_buf).map_err(|e|e.to_string())?==0);}
        b.read_exact(&mut b_buf[..n]).map_err(|e|e.to_string())?;
        if a_buf[..n]!=b_buf[..n] {return Ok(false);}
    }
}
fn cms_sidecar_path(output:&Path) -> Result<PathBuf,String> {
    let name=output.file_name().and_then(|x|x.to_str()).ok_or("Invalid XLSX file name")?;
    Ok(output.with_file_name(format!("{name}.p7s")))
}
fn bounded_cms_worker(exe:&Path,args:&[&std::ffi::OsStr],cancel:&AtomicBool) -> Result<(),String>{
    let mut child=Command::new(exe).args(args).stdin(Stdio::null())
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        .map_err(|e|format!("Cannot start independent CMS helper: {e}"))?;
    let start=Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) || start.elapsed()>Duration::from_secs(180){
            let _=child.kill();let _=child.wait();
            return Err("CMS signing cancelled or 180s deadline reached".into());
        }
        match child.try_wait().map_err(|e|e.to_string())?{
            Some(status) if status.success()=>return Ok(()),
            Some(status)=>return Err(format!("CMS sign/verify exited {status}; no accepted signature")),
            None=>std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
fn sign_detached_automatic(original:&Path,output:&Path,thumbprint:&str,cancel:&AtomicBool)
    -> Result<String,String>{
    if cancel.load(Ordering::SeqCst){return Err("Cancelled before signing".into());}
    let helper=std::env::current_exe().map_err(|e|e.to_string())?
        .with_file_name("craft-xlsx-cms.exe");
    if !helper.is_file(){return Err("Automatic XLSX detached CMS signer missing from the GridCraft installation".into());}
    let sidecar=cms_sidecar_path(output)?;
    if sidecar.exists(){return Err("Existing .p7s sidecar: refuse overwrite".into());}
    let mut created_xlsx=false;
    let result=(||{
        let mut input=File::open(original).map_err(|e|e.to_string())?;
        let mut result=OpenOptions::new().write(true).create_new(true).open(output)
            .map_err(|e|format!("Create new signed XLSX copy: {e}"))?;
        created_xlsx=true;
        std::io::copy(&mut input,&mut result).map_err(|e|e.to_string())?;
        result.sync_all().map_err(|e|e.to_string())?;
        drop(result);
        if !same_bytes(original,output)?{return Err("XLSX source changed while being copied".into());}
        if cancel.load(Ordering::SeqCst){return Err("Cancelled after copying XLSX".into());}
        let sign_args=[std::ffi::OsStr::new("sign"),output.as_os_str(),sidecar.as_os_str(),
            std::ffi::OsStr::new(thumbprint)];
        bounded_cms_worker(&helper,&sign_args,cancel)?;
        if !same_bytes(original,output)?{return Err("Input XLSX changed during CMS signing".into());}
        let verify_args=[std::ffi::OsStr::new("verify"),output.as_os_str(),sidecar.as_os_str()];
        bounded_cms_worker(&helper,&verify_args,cancel)?;
        Ok::<(),String>(())
    })();
    if let Err(e)=result{
        if sidecar.exists() { let _=std::fs::remove_file(&sidecar); }
        if created_xlsx {let _=std::fs::remove_file(output);}
        return Err(e);
    }
    Ok(format!("Automatic XLSX preserved: {}. Independent detached CMS signature: {}. Verification PASS. This external signature is NOT shown in Excel's Signatures pane. Trust and QES not assessed.",
        output.display(),sidecar.display()))
}

/// Local QA breadcrumbs. Only event labels and UTC millisecond timestamps are recorded.
fn trace_ui_stage(stage: &str) {
    let Some(folder) = std::env::var_os("CRAFT_OPC_TRACE_DIR").map(PathBuf::from) else { return };
    if !folder.is_dir() { return; }
    let name = format!("office-ui-stage-{}.log", std::process::id());
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(folder.join(name)) else { return };
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let _ = writeln!(file, "utc_ms={millis} stage={stage}");
}
impl OfficeSignUi {
    pub fn begin(&mut self, path: &Path) -> Result<(), String> {
        if self.active() {
            return Err("A certificate selection or signature operation is already active.".into());
        }
        if !path.is_file() {
            return Err("Save the workbook before choosing a certificate.".into());
        }
        if !path.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("xlsx")) {
            return Err("GridCraft can sign only saved XLSX workbooks. Use Save As > XLSX; CSV cannot carry an OPC digital signature.".into());
        }
        // Provider enumeration may call smart-card middleware: never block the GUI thread.
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("craft-office-certificates".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(list_office_certificates)
                    .unwrap_or_else(|_| Err("Certificate inventory worker failed".into()));
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Cannot launch certificate inventory: {e}"))?;
        self.certificates.clear();
        self.selected = None;
        self.allow_additional_signatures = false;
        self.excel_manual_calc_consent = false;
        self.signature_mode = XlsxSignatureMode::DetachedAutomatic;
        self.appearance = OfficeSignatureAppearance::default();
        self.embed_visible_at_default_location = true; // XLSX fixed-cell visible appearance, not screen rectangle.
        self.excel_anchor_cell = "E2".to_owned();
        self.drag_start = None;
        self.rect = None;
        self.excel_region = None;
        self.source = Some(path.to_path_buf());
        self.phase = Phase::Loading;
        self.inventory_rx = Some(rx);
        self.notice = None;
        self.verify_rx = None;
        Ok(())
    }
    pub fn detached_mode(&self)->bool {self.signature_mode==XlsxSignatureMode::DetachedAutomatic}
    fn verify_cms_sidecar_in_background(&mut self) -> Result<(),String> {
        if self.verify_rx.is_some(){return Err("Verification is already running".into());}
        let original=self.source.clone().ok_or("No saved workbook selected")?;
        let paired=cms_sidecar_path(&original)?;
        if !paired.is_file(){return Err(format!("No paired signature exists: {}",paired.display()));}
        let helper=std::env::current_exe().map_err(|e|e.to_string())?
            .with_file_name("craft-xlsx-cms.exe");
        if !helper.is_file(){return Err("Missing CMS verifier helper".into());}
        let (tx,rx)=mpsc::channel();
        std::thread::Builder::new().name("craft-detached-cms-verify".into()).spawn(move||{
            let result=Command::new(helper).arg("verify").arg(&original).arg(&paired)
                .stdin(Stdio::null()).stderr(Stdio::null()).output()
                .map_err(|e|e.to_string()).and_then(|r|{
                    if !r.status.success(){return Err("Detached CMS signature invalid, missing or no longer matches XLSX".into());}
                    let info=String::from_utf8_lossy(&r.stdout);
                    if !info.contains("CMS_DETACHED_VERIFY=PASS"){return Err("Unexpected CMS verifier result".into());}
                    Ok(format!("Detached CMS verification PASS for {}; trust and eIDAS QES not evaluated.",original.display()))
                });
            let _=tx.send(result);
        }).map_err(|e|e.to_string())?;
        self.verify_rx=Some(rx);
        self.notice=Some("Verifying paired .xlsx.p7s against workbook bytes...".into());
        Ok(())
    }
    fn lock_after_signing(&self) -> bool { !self.allow_additional_signatures }
    fn set_lock_after_signing(&mut self, value: bool) { self.allow_additional_signatures = !value; }

    /// Screen-space drag is mapped through the current GridCraft geometry.
    /// The OPC writer currently supports worksheet 1 only.
    fn resolve_grid_rectangle(
        shape: egui::Rect, geo: &crate::grid::Geo,
        sheet: &gridcraft_engine::model::Sheet,
    ) -> Result<ExcelRegion, String> {
        if shape.width() < 40.0 || shape.height() < 20.0
            || !geo.cells.contains(shape.min) || !geo.cells.contains(shape.max) {
            return Err("Draw a rectangle entirely inside visible worksheet cells.".into());
        }
        let logical = geo.mirror_pos(shape.min);
        let cell = geo.cell_at(sheet, logical);
        let anchor = geo.cell_rect(sheet, cell);
        let factor = 9525.0 / geo.z;
        let col_offset = ((logical.x - anchor.left()).max(0.0) * factor).round();
        let row_offset = ((logical.y - anchor.top()).max(0.0) * factor).round();
        let width = (shape.width() * factor).round();
        let height = (shape.height() * factor).round();
        if ![col_offset,row_offset,width,height].iter().all(|x| x.is_finite())
            || width < 380_000.0 || height < 190_000.0
            || width > 20_000_000.0 || height > 12_000_000.0 {
            return Err("Signature drawing dimensions outside supported bounds.".into());
        }
        Ok(ExcelRegion {
            column: cell.col, row: cell.row,
            col_offset: col_offset as u32, row_offset: row_offset as u32,
            width: width as u32, height: height as u32,
        })
    }

    pub fn active(&self) -> bool {
        self.phase != Phase::Closed
    }
    pub fn request_cancel_sign(&mut self) {
        if self.phase == Phase::Signing {
            if let Some(flag)=&self.sign_cancel {flag.store(true,Ordering::SeqCst);}
            self.notice=Some("Cancellation requested: terminating isolated signer; no new copy should be published.".into());
        }
    }
    pub fn cancel(&mut self) {
        // Do not dismiss the dialog until isolated worker shutdown completes.
        if self.phase == Phase::Signing {
            self.request_cancel_sign();
            return;
        }
        self.phase = Phase::Closed;
        self.drag_start = None;
        self.excel_region = None;
        self.source = None;
        self.inventory_rx = None;
        self.sign_rx = None;
        self.sign_started = None;
        self.sign_cancel = None;
    }
    /// Poll the CNG/provider workers without blocking an egui frame.
    /// A result is returned once and must be surfaced by the host application.
    pub fn poll(&mut self) -> Option<Result<String, String>> {
        match self.inventory_rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(Ok(ids))) => {
                self.selected = ids.iter().position(|c| c.has_private_key && c.valid_now);
                self.certificates = ids;
                self.inventory_rx = None;
                self.phase = Phase::Choose;
            }
            Some(Ok(Err(error))) => {
                self.cancel();
                return Some(Err(format!("Certificate selection failed: {error}")));
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.cancel();
                return Some(Err("Certificate inventory worker disconnected".into()));
            }
            _ => {}
        }
        match self.sign_rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(result)) => {
                self.sign_rx = None;
                self.sign_started = None;
        self.sign_cancel = None;
                self.phase = Phase::Closed;
                self.source = None;
                return Some(result);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.sign_rx = None;
                self.sign_started = None;
        self.sign_cancel = None;
                self.phase = Phase::Closed;
                self.source = None;
                return Some(Err("Hardware signing worker disconnected; inspect the output folder before retrying".into()));
            }
            _ => {}
        }
        None
    }
    pub fn check_ready_to_sign(&self, current: &Path) -> Result<(), String> {
        if self.signature_mode==XlsxSignatureMode::NativeOfficeManual && !self.excel_manual_calc_consent {
            return Err("Native Office signing requires explicit manual-calculation acceptance".into())
        }
        if self.phase != Phase::Confirm || self.source.as_deref() != Some(current) {
            return Err("Select a certificate for the current saved XLSX file.".into());
        }
        let cert = self.selected.and_then(|n| self.certificates.get(n))
            .ok_or("Choose a signing certificate.")?;
        if !cert.has_private_key || !cert.valid_now {
            return Err("The selected certificate is not available for signing.".into());
        }
        if self.signature_mode!=XlsxSignatureMode::DetachedAutomatic && self.embed_visible_at_default_location && self.excel_region.is_none() {
            return Err("Draw the signature rectangle on worksheet 1 first.".into());
        }
        Ok(())
    }
    fn check_destination(source: &Path, output: &Path) -> Result<(), String> {
        if source == output || output.exists() {
            return Err("Choose a new XLSX name; existing files are never overwritten.".into());
        }
        if !output.is_absolute() || !output.extension().and_then(|x|x.to_str())
            .is_some_and(|x|x.eq_ignore_ascii_case("xlsx"))
            || !output.parent().is_some_and(|p| p.is_dir()) {
            return Err("Choose an absolute .xlsx path in an existing folder.".into());
        }
        Ok(())
    }
    pub fn start_sign(&mut self, current: &Path, output: &Path) -> Result<(), String> {
        if self.verify_rx.is_some(){return Err("Finish CMS verification before starting another signature".into());}
        self.check_ready_to_sign(current)?;
        Self::check_destination(current,output)?;
        trace_ui_stage("save_as_destination_confirmed");
        trace_ui_stage(if self.allow_additional_signatures { "multiple_signature_mode" } else { "craft_final_signature_mode" });
        let cancel=Arc::new(AtomicBool::new(false));
        let worker_cancel=Arc::clone(&cancel);
        let mut request = std::mem::take(self);
        self.signature_mode=request.signature_mode; // preserve progress-dialog label
        let src = current.to_path_buf();
        let dst = output.to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| request.sign_cancelable(&src,&dst,&worker_cancel)))
                .unwrap_or_else(|_| Err("Signature worker failed; inspect the output before retry.".into()));
            let _ = tx.send(result);
        });
        self.phase = Phase::Signing;
        self.sign_rx = Some(rx);
        self.sign_started = Some(Instant::now());
        self.sign_cancel=Some(cancel);
        Ok(())
    }
    pub fn screen_rectangle(&self) -> Option<egui::Rect> {
        self.rect
    }

    /// Interactive selection mapped to real GridCraft worksheet geometry.
    pub fn draw(&mut self, ctx: &egui::Context, viewport: Option<egui::Rect>,
            geo: Option<&crate::grid::Geo>, sheet: Option<&gridcraft_engine::model::Sheet>,
            active_sheet_index: Option<usize>) {
        if self.phase != Phase::Draw { return; }
        let Some(viewport) = viewport.filter(|r|r.width()>30.0 && r.height()>30.0) else { return };
        let (pressed,released,pointer)=ctx.input(|i| (
            i.pointer.primary_pressed(),i.pointer.primary_released(),
            i.pointer.interact_pos().or_else(||i.pointer.latest_pos())));
        if pressed {
            self.drag_start=pointer.filter(|p|viewport.contains(*p));
            if self.drag_start.is_none(){self.notice=Some("Start inside worksheet cells.".into());}
        }
        let mut preview=None;
        if let (Some(start),Some(point))=(self.drag_start,pointer) {
            let end=egui::pos2(point.x.clamp(viewport.left(),viewport.right()),
                point.y.clamp(viewport.top(),viewport.bottom()));
            let shape=egui::Rect::from_two_pos(start,end);
            preview=Some(shape);
            if released {
                let result=match (geo,sheet,active_sheet_index) {
                    (Some(g),Some(sh),Some(0)) => Self::resolve_grid_rectangle(shape,g,sh),
                    (_,_,Some(_)) => Err("Visible XLSX signing currently supports worksheet 1 only.".into()),
                    _ => Err("Worksheet geometry unavailable. Try normal worksheet view.".into()),
                };
                match result {
                    Ok(region)=>{
                        self.rect=Some(shape);
                        self.excel_region=Some(region);
                        self.excel_anchor_cell=format!("{}{}",gridcraft_engine::core::col_to_letters(region.column),region.row+1);
                        self.phase=Phase::Confirm;self.notice=None;
                        trace_ui_stage("excel_rectangle_confirmed");
                    },
                    Err(reason)=>{self.notice=Some(reason);trace_ui_stage("excel_rectangle_rejected");}
                }
                self.drag_start=None;
            } else {ctx.request_repaint();}
        }
        egui::Area::new(egui::Id::new("craft_excel_sign_rectangle"))
            .order(egui::Order::Foreground).fixed_pos(viewport.min)
            .movable(false).show(ctx,|ui|{
                let response=ui.allocate_response(viewport.size(),egui::Sense::click_and_drag());
                if response.hovered(){ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);}
                if let Some(shape)=preview {
                    ui.painter().rect_filled(shape,0.0,egui::Color32::from_rgba_unmultiplied(20,115,230,28));
                    ui.painter().rect_stroke(shape,0.0,egui::Stroke::new(2.0,egui::Color32::from_rgb(20,115,230)),egui::StrokeKind::Inside);
                }
            });
        egui::Window::new("Draw XLSX signature rectangle").collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_TOP,[0.0,80.0])
            .show(ctx,|ui|{
                ui.label("Draw on worksheet 1. This rectangle determines the XLSX image anchor and size.");
                if let Some(reason)=&self.notice {ui.colored_label(egui::Color32::YELLOW,reason);}
                if ui.button("Back").clicked(){self.phase=Phase::Confirm;self.drag_start=None;}
            });
    }
    /// Choose a Digital ID and confirm its provider-controlled signature.
    /// A visible Excel signature requires a real worksheet rectangle.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if let Some(rx)=&self.verify_rx {
            match rx.try_recv(){
                Ok(result)=>{
                    self.notice=Some(match result {Ok(msg)=>msg,Err(err)=>format!("CMS verification FAIL: {err}")});
                    self.verify_rx=None;
                },
                Err(TryRecvError::Disconnected)=>{
                    self.notice=Some("CMS verifier worker disconnected".into());
                    self.verify_rx=None;
                },
                Err(TryRecvError::Empty)=>ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
        if self.phase == Phase::Loading {
            ctx.request_repaint_after(Duration::from_millis(120));
            egui::Window::new("Reading Windows certificates")
                .collapsible(false).resizable(false).show(ctx, |ui| {
                    ui.spinner();
                    ui.label("Checking available certificates. The application remains responsive.");
                    if ui.button("Cancel certificate selection").clicked() { self.cancel(); }
                });
            return None;
        }
        if self.phase == Phase::Signing {
            ctx.request_repaint_after(Duration::from_millis(120));
            egui::Window::new("Digital signature in progress")
                .collapsible(false).resizable(false).show(ctx, |ui| {
                    ui.spinner();
                    ui.label(if self.signature_mode==XlsxSignatureMode::DetachedAutomatic {
                       "GridCraft — automatic XLSX with external .p7s CMS signature"
                   } else {"GridCraft — native Office XML-DSig signing"});
                    if let Some(started) = self.sign_started {
                        let elapsed = started.elapsed().as_secs();
                        ui.label(format!("Elapsed: {elapsed} s"));
                        if elapsed >= 30 {
                            ui.colored_label(egui::Color32::YELLOW, "No response from signing provider; do not start a second signature. Inspect the QA stage log.");
                        }
                    }
                    ui.label("Windows/B-Trust may be waiting for its own PIN dialog.");
                    ui.label("The document is being signed and verified in a background worker.");
                    ui.label("The isolated signer will be terminated after 180 seconds at most.");
                    if ui.button("Cancel signing").clicked() {self.request_cancel_sign();}
                    if let Some(message)=&self.notice {ui.colored_label(egui::Color32::YELLOW,message);}
                });
            return None;
        }
        if !matches!(self.phase, Phase::Choose | Phase::Confirm) {
            return None;
        }
        let mut action = None;
        let mut opened = true;
        egui::Window::new(if self.phase == Phase::Choose {"Sign with a Digital ID"} else {"Sign as selected Digital ID"})
            .id(egui::Id::new("craft_office_sign_choose"))
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .open(&mut opened)
            .show(ctx, |ui| {
                if self.phase == Phase::Choose {
                    ui.heading("Choose a certificate");
                    ui.label("To check an automatic XLSX, open the matching .xlsx and click Verify paired .p7s.");
                    if ui.button("Verify paired .xlsx.p7s (no certificate or PIN needed)").clicked(){
                        if let Err(e)=self.verify_cms_sidecar_in_background(){
                            self.notice=Some(format!("Verification unavailable: {e}"));
                        }
                    }
                    if let Some(message)=&self.notice {ui.colored_label(egui::Color32::YELLOW,message);}

                    let mut lock=self.lock_after_signing();
                    if ui.checkbox(&mut lock,"Native Office only: Lock document after signing").changed() {
                        self.set_lock_after_signing(lock);
                    }
                    ui.small("With lock: Craft refuses further signatures; other Office programs may ignore this rule.");
                    ui.small("Without lock: later OPC signatures are allowed.");
                    ui.label("Windows Current User / Personal. The private key stays in its hardware or software provider.");
                    egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                        for (i, c) in self.certificates.iter().enumerate() {
                            let eligible = c.has_private_key && c.valid_now;
                            let label = format!("{}\nIssuer: {} | Until: {}\n{}", c.subject, c.issuer, c.expires, c.thumbprint);
                            ui.add_enabled_ui(eligible, |ui| {
                                ui.radio_value(&mut self.selected, Some(i), label);
                            });
                            if !eligible {
                                ui.weak("Unavailable: no linked private key or outside validity period.");
                            }
                            ui.separator();
                        }
                    });
                    if self.certificates.is_empty() {
                        ui.colored_label(egui::Color32::RED, "No personal certificates were found.");
                    }
                    ui.horizontal(|ui| {
                        let can_draw = self.selected.is_some_and(|i| self.certificates.get(i).is_some_and(|c| c.has_private_key && c.valid_now));
                        if ui.add_enabled(can_draw, egui::Button::new("Continue to signature settings")).clicked() {
                            self.rect = None;
                            self.phase = Phase::Confirm;
                        }
                        if ui.add_enabled(can_draw && self.signature_mode!=XlsxSignatureMode::DetachedAutomatic,
                            egui::Button::new("Draw native signature rectangle on worksheet")).clicked() {
                            self.phase=Phase::Draw;self.notice=None;self.drag_start=None;
                            trace_ui_stage("excel_rectangle_mode");
                        }
                        if ui.button("Cancel").clicked() { self.cancel() }
                    });
                } else {
                    let c = self.selected.and_then(|i| self.certificates.get(i));
                    ui.label(format!("Certificate: {}",c.map_or("Missing",|c|c.subject.as_str())));
                    ui.separator();
                    ui.label("Signature format:");
                    ui.radio_value(&mut self.signature_mode,XlsxSignatureMode::DetachedAutomatic,
                        "Automatic (recommended): preserve XLSX exactly + independent CMS .p7s");
                    ui.radio_value(&mut self.signature_mode,XlsxSignatureMode::NativeOfficeAutomatic,
                        "Native Excel XML-DSig (Automatic): verified arithmetic graph + Excel chain");
                    ui.radio_value(&mut self.signature_mode,XlsxSignatureMode::NativeOfficeManual,
                        "Native Office XML-DSig (Manual): fallback for other workbooks");
                    if self.signature_mode==XlsxSignatureMode::DetachedAutomatic {
                        ui.colored_label(egui::Color32::YELLOW,
                            "The XLSX remains automatic and unchanged. A companion .xlsx.p7s contains the detached cryptographic signature.");
                        ui.small("Excel's Signatures pane will NOT show this external signature. Verify both files together with GridCraft/CMS verifier. Never separate them.");
                    } else {
                    if let Some(region)=self.excel_region {
                        ui.label(format!("Worksheet 1: anchor {}, {:.0} x {:.0} px at 96 dpi",
                            self.excel_anchor_cell,region.width as f32/9525.0,region.height as f32/9525.0));
                    } else if self.embed_visible_at_default_location {
                        ui.colored_label(egui::Color32::YELLOW,"Draw the signature rectangle before signing.");
                    }
                    // Mirrors PdfCraft's appearance preview without promising that an
                    // editable Office line has already been embedded in the package.
                    let signer = c.map(|c| OfficeSignatureAppearance::subject_display_name(&c.subject))
                        .unwrap_or("Unknown signer").to_owned();
                    let lines = self.appearance.details(&signer);
                    ui.label(egui::RichText::new("Digital signature appearance").strong());
                    egui::Frame::new()
                        .fill(egui::Color32::WHITE)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(178, 186, 197)))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_min_height(96.0);
                            ui.columns(2, |cols| {
                                cols[0].label(egui::RichText::new("DIGITAL SIGNATURE")
                                    .color(egui::Color32::from_gray(92)).small());
                                if self.appearance.name {
                                    cols[0].label(egui::RichText::new(&signer).size(19.0).strong()
                                        .color(egui::Color32::from_rgb(32, 42, 60)));
                                }
                                for line in &lines {
                                    cols[1].label(egui::RichText::new(line)
                                        .color(egui::Color32::from_rgb(45, 55, 70)));
                                }
                            });
                        });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.appearance.name, "Name");
                        ui.checkbox(&mut self.appearance.date, "Date");
                        ui.checkbox(&mut self.appearance.reason, "Reason");
                        ui.checkbox(&mut self.appearance.location, "Location");
                        ui.checkbox(&mut self.appearance.labels, "Labels");
                    });
                    if self.appearance.reason {
                        ui.horizontal(|ui| {
                            ui.label("Reason");
                            ui.add(egui::TextEdit::singleline(&mut self.appearance.reason_text)
                                .hint_text("Optional").desired_width(280.0));
                        });
                    }
                    if self.appearance.location {
                        ui.horizontal(|ui| {
                            ui.label("Location");
                            ui.add(egui::TextEdit::singleline(&mut self.appearance.location_text)
                                .hint_text("Optional").desired_width(280.0));
                        });
                    }
                    // Own the extension so later UI callbacks may mutate self.
                    let destination="worksheet 1, at the drawn rectangle";
                    ui.checkbox(&mut self.embed_visible_at_default_location,
                        format!("Include a visible signed appearance at {destination}"));
                    if self.embed_visible_at_default_location {
                        if ui.button("Draw / change rectangle on worksheet").clicked() {
                            self.phase=Phase::Draw;self.drag_start=None;self.notice=None;
                            trace_ui_stage("excel_rectangle_mode");
                        }
                        if self.excel_region.is_none() {
                            ui.colored_label(egui::Color32::YELLOW,"Required: drag a rectangle over worksheet 1.");
                        }
                    }
                    ui.small("The image will be inserted at the drawn XLSX position, BEFORE the package is signed.");
                    let mut lock=self.lock_after_signing();
                    if ui.checkbox(&mut lock,"Native Office only: Lock document after signing").changed(){
                        self.set_lock_after_signing(lock);
                    }
                    if lock {
                        ui.small("Final in Craft only. Microsoft Excel may allow other signing methods.");
                    } else {
                        ui.small("Additional signatures may be added.");
                    }
                    if self.signature_mode==XlsxSignatureMode::NativeOfficeAutomatic {
                        ui.label("Native Automatic requires a real Excel calcId, dependency-ordered calcChain, and verified arithmetic caches (up to 256 formulas). Unsupported expressions fail closed.");
                        ui.small("Visible in Excel Signatures. Tested in one Excel environment with Book1 already open. Other versions and workbooks require QA.");
                    } else {
                    ui.colored_label(egui::Color32::YELLOW,
                        "Excel compatibility: automatic recalculation made the V37 XML signature invalid in Office. Manual mode was accepted in V38.");
                    ui.checkbox(&mut self.excel_manual_calc_consent,
                        "I accept MANUAL calculation in the new signed XLSX (original remains unchanged)");
                    ui.small("Formula results in the signed copy may become stale. Recalculate the UNSIGNED source first, then sign. This choice does not change the source or every Excel workbook.");
                    ui.small("Important: Excel uses one calculation mode across all open workbooks. To verify: launch Excel with no other workbooks, select Formulas > Calculation Options > Manual, then open the signed XLSX. Opening an automatic workbook first can invalidate the signature.");
                    }
                    }
                    ui.label("The Windows certificate provider will request your PIN if necessary. Never enter your PIN into Craft.");
                    ui.horizontal(|ui| {
                        if ui.button("Back").clicked() { self.phase = Phase::Choose }
                        let ready=self.selected.is_some() && (self.signature_mode==XlsxSignatureMode::DetachedAutomatic ||
                            (self.signature_mode==XlsxSignatureMode::NativeOfficeAutomatic &&
                              (!self.embed_visible_at_default_location || self.excel_region.is_some())) ||
                            (self.signature_mode==XlsxSignatureMode::NativeOfficeManual &&
                              self.excel_manual_calc_consent &&
                              (!self.embed_visible_at_default_location || self.excel_region.is_some())));
                        if ui.add_enabled(ready, egui::Button::new(
                            if self.signature_mode==XlsxSignatureMode::DetachedAutomatic {"Choose new XLSX + .p7s location..."}
                            else {"Choose new native-signed XLSX location..."})).clicked() {
                            trace_ui_stage("save_as_button_clicked");
                            action=Some(Action::Sign);
                        }
                        if ui.button("Cancel").clicked() { self.cancel() }
                    });
                }
            });
        if !opened {
            self.cancel()
        }
        action
    }
    pub fn sign(&mut self, current: &Path, output: &Path) -> Result<String,String> {
        let never=AtomicBool::new(false);
        self.sign_cancelable(current,output,&never)
    }
    fn sign_cancelable(&mut self,current:&Path,output:&Path,cancel:&AtomicBool) -> Result<String,String> {
        if cancel.load(Ordering::SeqCst){return Err("Signature cancelled before starting".into());}
        if self.phase != Phase::Confirm { return Err("Confirm the certificate and signing choices.".into()); }
        let original=self.source.as_ref().ok_or("No source workbook")?;
        if original != current { return Err("The opened XLSX workbook changed.".into()); }
        Self::check_destination(original,output)?;
        let cert=self.selected.and_then(|i|self.certificates.get(i))
            .ok_or("No eligible certificate selected.")?;
        if !cert.has_private_key || !cert.valid_now {
            return Err("The chosen certificate is not eligible.".into());
        }
        if self.signature_mode==XlsxSignatureMode::DetachedAutomatic {
            return sign_detached_automatic(original,output,&cert.thumbprint,cancel);
        }
        if self.signature_mode==XlsxSignatureMode::NativeOfficeAutomatic {
            let bytes=std::fs::read(original).map_err(|e|e.to_string())?;
            gridcraft_xlsx::office_auto::native_auto_preflight(&bytes)
                .map_err(|e|format!("V43B native Automatic preflight: {e}"))?;
            if cancel.load(Ordering::SeqCst){return Err("Signature cancelled before provider invocation".into());}
            let result=if self.embed_visible_at_default_location {
                let region=self.excel_region.ok_or("Draw signature rectangle first.")?;
                sign_with_visible_appearance_with_policy_cancelable(
                    original,output,&cert.thumbprint,&self.appearance,
                    OfficeImagePlacement::ExcelRectangle{
                        column:region.column,row:region.row,
                        col_offset:region.col_offset,row_offset:region.row_offset,
                        width:region.width,height:region.height,
                    },self.allow_additional_signatures,cancel
                )
            } else {
                sign_opc_inprocess_with_policy_cancelable(
                    original,output,&cert.thumbprint,self.allow_additional_signatures,cancel)
            };
            result?;
            let count=verify_opc_inprocess(output)?;
            if count==0{return Err("No native OPC digital signature after signing".into());}
            self.cancel();
            return Ok(format!("Native Office XML-DSig: {}. Conservative native Automatic formula graph validated. Windows OPC PASS; Microsoft Excel and QES status not assessed for THIS output.",
                output.display()));
        }
        if !self.excel_manual_calc_consent {
            return Err("Native Office signing requires explicit manual calculation consent.".into());
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?.as_nanos();
        let prepared = output.with_file_name(format!(
            ".craft-v39-calc-{}-{stamp}.xlsx",std::process::id()));
        if prepared.exists() { return Err("V39 unsigned staging collision".into()); }
        trace_ui_stage("excel_manual_prepare_enter");
        let result=(|| {
            prepare_xlsx_for_excel_signing(original,&prepared)?;
            trace_ui_stage("excel_manual_prepare_return");
            if cancel.load(Ordering::SeqCst){
                return Err("V39 signing cancelled after unsigned preparation".into());
            }
            if self.embed_visible_at_default_location {
                let region=self.excel_region.ok_or("Draw a signature rectangle on worksheet 1 first.")?;
                trace_ui_stage("appearance_prepare_enter");
                sign_with_visible_appearance_with_policy_cancelable(
                    &prepared,output,&cert.thumbprint,&self.appearance,
                    OfficeImagePlacement::ExcelRectangle {
                        column:region.column,row:region.row,
                        col_offset:region.col_offset,row_offset:region.row_offset,
                        width:region.width,height:region.height,
                    },
                    self.allow_additional_signatures,cancel,
                )?;
                trace_ui_stage("appearance_sign_return");
            } else {
                trace_ui_stage("invisible_sign_enter");
                sign_opc_inprocess_with_policy_cancelable(&prepared,output,&cert.thumbprint,self.allow_additional_signatures,cancel)?;
                trace_ui_stage("invisible_sign_return");
            }
            trace_ui_stage("verify_opc_enter");
            let count=verify_opc_inprocess(output)?;
            trace_ui_stage("verify_opc_return");
            if count == 0 { return Err("No OPC signature present after signing.".into()); }
            Ok::<(),String>(())
        })();
        let _=std::fs::remove_file(&prepared);
        result?;
        let msg=format!("Signed copy: {}. V39 consented manual calculation; original XLSX preserved. OPC verification PASS. To inspect in Excel, first start Excel with no workbook open, select Formulas > Calculation Options > Manual, then open this signed copy. Excel uses the first workbook calculation mode for all others. eIDAS QES and broad Excel compatibility NOT assessed.",output.display());
        self.cancel();
        Ok(msg)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_button_requests_hard_worker_termination_without_closing_dialog() {
        let mut dialog=OfficeSignUi::default();
        dialog.phase=Phase::Signing;
        let marker=Arc::new(AtomicBool::new(false));
        dialog.sign_cancel=Some(Arc::clone(&marker));
        dialog.cancel();
        assert!(marker.load(Ordering::SeqCst));
        assert_eq!(dialog.phase,Phase::Signing);
    }

    #[test]
    fn pointer_drag_maps_to_real_xlsx_cell_and_dimensions() {
        let sheet=gridcraft_engine::model::Sheet::new("Sheet1");
        let viewport=egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(960.0,750.0));
        let geo=crate::grid::Geo::new(&sheet,viewport,egui::Vec2::ZERO);
        let rect=geo.cells;
        let begin=egui::pos2(rect.left()+78.0,rect.top()+58.0);
        let finish=egui::pos2(rect.left()+428.0,rect.top()+188.0);
        let mut dialog=OfficeSignUi::default();
        dialog.phase=Phase::Draw;
        let ctx=egui::Context::default();
        let send=|dialog:&mut OfficeSignUi,events|{
            let mut raw=egui::RawInput::default();
            raw.screen_rect=Some(viewport);
            raw.events=events;
            let mut frame=ctx.run_ui(raw,|ctx|dialog.draw(ctx,Some(rect),Some(&geo),Some(&sheet),Some(0)));
            frame.textures_delta.clear();
        };
        send(&mut dialog,vec![]);
        send(&mut dialog,vec![egui::Event::PointerMoved(begin),egui::Event::PointerButton{
            pos:begin,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE,
        }]);
        send(&mut dialog,vec![egui::Event::PointerMoved(finish)]);
        send(&mut dialog,vec![egui::Event::PointerMoved(finish),egui::Event::PointerButton{
            pos:finish,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE,
        }]);
        assert_eq!(dialog.phase,Phase::Confirm);
        let shape=dialog.excel_region.expect("missing sheet-relative rectangle");
        assert!(shape.column>0 && shape.row>0);
        assert!(shape.width>2_000_000 && shape.height>900_000);
        assert!(dialog.excel_anchor_cell.chars().any(|c|c.is_ascii_digit()));
    }

    #[test]
    fn csv_is_rejected_before_certificate_provider_access() {
        let path = std::env::temp_dir().join(format!("craft-sign-csv-{}.csv", std::process::id()));
        std::fs::write(&path, b"header\nvalue\n").unwrap();
        let mut dialog = OfficeSignUi::default();
        let result = dialog.begin(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("XLSX"));
        assert!(!dialog.active());
        assert!(dialog.inventory_rx.is_none());
    }

    #[test]
    fn certificate_discovery_requires_an_existing_saved_document() {
        let mut s = OfficeSignUi::default();
        assert!(s.begin(Path::new("not-a-real-file-for-craft-signing.docx")).is_err());
        assert!(!s.active());
    }

    #[test]
    fn signing_cannot_start_without_confirmed_certificate() {
        let mut s = OfficeSignUi::default();
        assert!(s.start_sign(Path::new("any.xlsx"), Path::new("out.xlsx")).is_err());
        assert!(!s.active());
    }

    #[test]
    fn cancel_does_not_abandon_a_hardware_signing_operation() {
        let mut s = OfficeSignUi::default();
        s.phase = Phase::Signing;
        s.cancel();
        assert_eq!(s.phase, Phase::Signing);
    }

    #[test]
    fn craft_final_mode_is_default_and_can_be_reversed() {
        let mut ui=OfficeSignUi::default();
        assert!(ui.lock_after_signing());
        ui.set_lock_after_signing(false);
        assert!(ui.allow_additional_signatures);
        ui.set_lock_after_signing(true);
        assert!(!ui.allow_additional_signatures);
    }
    #[test]
    fn manual_calculation_requires_explicit_consent() {
        let mut dialog=OfficeSignUi::default();
        assert!(!dialog.excel_manual_calc_consent);
        assert!(dialog.check_ready_to_sign(Path::new("demo.xlsx")).is_err());
        dialog.excel_manual_calc_consent=true;
        assert!(dialog.check_ready_to_sign(Path::new("demo.xlsx")).is_err()); // no certificate
    }
    #[test]
    fn save_as_will_never_overwrite_a_workbook() {
        let root=std::env::temp_dir().join(format!("grid-sign-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let original=root.join("original.xlsx");
        std::fs::write(&original,b"test").unwrap();
        let output=root.join("new.xlsx");
        assert!(OfficeSignUi::check_destination(&original,&original).is_err());
        assert!(OfficeSignUi::check_destination(&original,&output).is_ok());
        std::fs::write(&output,b"exists").unwrap();
        assert!(OfficeSignUi::check_destination(&original,&output).is_err());
        assert_eq!(std::fs::read(&original).unwrap(),b"test");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Explicitly invoked only by isolated synthetic-cert QA orchestration.
    /// Never uses production token, credential or confidential document.
    #[test]
    #[ignore = "requires CRAFT_V44_QA_SOURCE, CRAFT_V44_QA_DEST, CRAFT_V44_QA_THUMBPRINT"]
    fn v44_real_native_automatic_visible_signature_qa() {
        let source=PathBuf::from(std::env::var_os("CRAFT_V44_QA_SOURCE").expect("QA source"));
        let dest=PathBuf::from(std::env::var_os("CRAFT_V44_QA_DEST").expect("QA destination"));
        let thumb=std::env::var("CRAFT_V44_QA_THUMBPRINT").expect("QA certificate thumbprint");
        assert!(!dest.exists(), "No overwrite in synthetic signing QA");
        gridcraft_xlsx::office_auto::native_auto_preflight(&std::fs::read(&source).unwrap()).unwrap();
        let mut dialog=OfficeSignUi::default();
        dialog.source=Some(source.clone());
        dialog.phase=Phase::Confirm;
        dialog.signature_mode=XlsxSignatureMode::NativeOfficeAutomatic;
        dialog.certificates.push(OfficeCertificate{
            thumbprint:thumb,
            subject:"CN=CRAFT-V44-EPHEMERAL-QA-UNTRUSTED".into(),
            issuer:"CN=CRAFT-V44-EPHEMERAL-QA-UNTRUSTED".into(),
            expires:"test-only".into(),
            has_private_key:true,
            valid_now:true,
        });
        dialog.selected=Some(0);
        dialog.embed_visible_at_default_location=true;
        dialog.excel_region=Some(ExcelRegion{
            column:4,row:2,col_offset:0,row_offset:0,
            width:180*9525,height:65*9525,
        });
        let result=dialog.sign_cancelable(&source,&dest,&AtomicBool::new(false));
        assert!(result.is_ok(), "Synthetic V44 native signature failed: {result:?}");
        let signed=std::fs::read(&dest).unwrap();
        assert!(!signed.is_empty());
        assert!(verify_opc_inprocess(&dest).unwrap()>0);
    }

    #[test]
    fn automatic_signature_helpers_check_sidecar_and_byte_integrity() {
        let mut ui=OfficeSignUi::default();
        assert!(ui.detached_mode());
        ui.signature_mode=XlsxSignatureMode::NativeOfficeManual;
        assert!(!ui.detached_mode());
        assert_eq!(cms_sidecar_path(Path::new("a.xlsx")).unwrap(),PathBuf::from("a.xlsx.p7s"));
        let root=std::env::temp_dir().join(format!("craft-cms-test-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source=root.join("original.xlsx");
        let copy=root.join("copy.xlsx");
        std::fs::write(&source,b"synthetic").unwrap();
        std::fs::write(&copy,b"synthetic").unwrap();
        assert!(same_bytes(&source,&copy).unwrap());
        std::fs::write(&copy,b"tampered!").unwrap();
        assert!(!same_bytes(&source,&copy).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]    fn closed_state_will_not_sign() {
        let mut s = OfficeSignUi::default();
        assert!(!s.active());
        assert!(s.sign(Path::new("nonexistent.xlsx"),Path::new("out.xlsx")).is_err());
    }


}
