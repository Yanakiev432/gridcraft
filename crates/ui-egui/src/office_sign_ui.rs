//! Adapted from PdfCraft's Digital ID choose/draw/confirm interaction.
//! Office documents use OPC/XMLDSig via the common Craft signing core,
//! NOT PDF PAdES. A drawn on-screen rectangle is only a placement selection;
//! it must not be misrepresented as an embedded Office signature-line object.
use std::path::{Path, PathBuf};

use craft_signing_core::office_certificates::{OfficeCertificate, list_office_certificates};
use craft_signing_core::opc_native::{sign_opc_inprocess, verify_opc_inprocess};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Closed,
    Choose,
    Draw,
    Confirm,
}
#[derive(Default)]
pub struct OfficeSignUi {
    phase: Phase,
    certificates: Vec<OfficeCertificate>,
    selected: Option<usize>,
    allow_additional_signatures: bool,
    drag_start: Option<egui::Pos2>,
    rect: Option<egui::Rect>,
    source: Option<PathBuf>,
    pub notice: Option<String>,
}
pub enum Action {
    Sign,
}
impl OfficeSignUi {
    pub fn begin(&mut self, path: &Path) -> Result<(), String> {
        if !path.is_file() {
            return Err("Save the document before choosing a certificate.".into());
        }
        let ids = list_office_certificates()?;
        let first = ids.iter().position(|c| c.has_private_key && c.valid_now);
        self.certificates = ids;
        self.selected = first;
        self.allow_additional_signatures = true;
        self.drag_start = None;
        self.rect = None;
        self.source = Some(path.to_path_buf());
        self.phase = Phase::Choose;
        self.notice = None;
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.phase != Phase::Closed
    }
    pub fn cancel(&mut self) {
        self.phase = Phase::Closed;
        self.drag_start = None;
        self.source = None;
    }
    pub fn screen_rectangle(&self) -> Option<egui::Rect> {
        self.rect
    }

    /// PdfCraft's visible-rectangle interaction, adapted to the current
    /// Word/Sheet viewport. No document write occurs while dragging.
    pub fn draw(&mut self, ctx: &egui::Context, viewport: Option<egui::Rect>) {
        if self.phase != Phase::Draw {
            return;
        }
        let Some(viewport) = viewport.filter(|r| r.width() > 30.0 && r.height() > 30.0) else { return };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.phase = Phase::Choose;
            self.drag_start = None;
            return;
        }
        egui::Area::new(egui::Id::new("craft_office_sign_rectangle")).order(egui::Order::Foreground).fixed_pos(viewport.min).movable(false).show(
            ctx,
            |ui| {
                let response = ui.allocate_response(viewport.size(), egui::Sense::click_and_drag());
                if response.hovered() || response.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if response.drag_started() {
                    self.drag_start = response.interact_pointer_pos();
                }
                if let (Some(start), Some(end)) = (self.drag_start, response.interact_pointer_pos()) {
                    let end = egui::pos2(end.x.clamp(viewport.left(), viewport.right()), end.y.clamp(viewport.top(), viewport.bottom()));
                    let shape = egui::Rect::from_two_pos(start, end).intersect(viewport);
                    ui.painter().rect_filled(shape, 0.0, egui::Color32::from_rgba_unmultiplied(20, 115, 230, 28));
                    ui.painter().rect_stroke(shape, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(20, 115, 230)), egui::StrokeKind::Inside);
                    if response.drag_stopped() {
                        self.drag_start = None;
                        if shape.width() >= 24.0 && shape.height() >= 18.0 {
                            self.rect = Some(shape);
                            self.phase = Phase::Confirm;
                        }
                    }
                }
            },
        );
        egui::Window::new("Draw the signature rectangle")
            .id(egui::Id::new("office_sign_draw_hint"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .show(ctx, |ui| {
                ui.label("Drag a rectangle over the document, then confirm the certificate signature.");
                if ui.button("Back to certificates").clicked() {
                    self.phase = Phase::Choose;
                    self.drag_start = None;
                }
            });
    }
    /// Dialog flow is structurally identical to PdfCraft: choose a Digital ID,
    /// place its rectangle, confirm the provider-controlled signature.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
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
                    ui.checkbox(&mut self.allow_additional_signatures, "Allow additional signatures");
                    ui.small("When enabled, another person can sign this document later without invalidating existing OPC signatures.");
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
                        if ui.add_enabled(can_draw, egui::Button::new("Draw signature rectangle")).clicked() {
                            self.phase = Phase::Draw;
                        }
                        if ui.button("Cancel").clicked() { self.cancel() }
                    });
                } else {
                    let c = self.selected.and_then(|i| self.certificates.get(i));
                    ui.label(format!("Certificate: {}",c.map_or("Missing",|c|c.subject.as_str())));
                    if let Some(r) = self.rect {
                        ui.label(format!("Chosen rectangle: {:.0} x {:.0} screen points",r.width(),r.height()));
                    }
                    ui.colored_label(egui::Color32::YELLOW, "Office OPC signature protects document bytes. A visible Office signature-line drawing is a separate OOXML object, not a PDF stamp.");
                    ui.checkbox(&mut self.allow_additional_signatures, "Allow additional signatures");
                    if !self.allow_additional_signatures {
                        ui.small("Disabling additional signatures applies a Craft-only signed policy; other Office programs may not honor this restriction.");
                    }
                    ui.label("The Windows certificate provider will request your PIN if necessary. Never enter your PIN into Craft.");
                    ui.horizontal(|ui| {
                        if ui.button("Back").clicked() { self.phase = Phase::Draw }
                        if ui.button("Sign a separate copy").clicked() { action = Some(Action::Sign) }
                        if ui.button("Cancel").clicked() { self.cancel() }
                    });
                }
            });
        if !opened {
            self.cancel()
        }
        action
    }
    pub fn sign(&mut self, current: &Path) -> Result<String, String> {
        if self.phase != Phase::Confirm || self.rect.is_none() {
            return Err("Choose a certificate and draw a rectangle before signing.".into());
        }
        let original = self.source.as_ref().ok_or("No original document")?;
        if original != current {
            return Err("The open document changed: start a new signature.".into());
        }
        let i = self.selected.ok_or("No certificate chosen")?;
        let cert = self.certificates.get(i).ok_or("Selected certificate no longer exists")?;
        if !cert.has_private_key || !cert.valid_now {
            return Err("Selected certificate is not eligible for signing.".into());
        }
        let ext = original.extension().and_then(|s| s.to_str()).ok_or("Document extension missing")?;
        if !matches!(ext.to_ascii_lowercase().as_str(), "docx" | "xlsx") {
            return Err("Native OPC signature supports only DOCX and XLSX.".into());
        }
        let stem = original.file_stem().and_then(|s| s.to_str()).ok_or("Document name invalid")?;
        let dir = original.parent().ok_or("Document directory missing")?;
        let output = (1..=100)
            .map(|n| {
                let name = if n == 1 { format!("{stem}-signed.{ext}") } else { format!("{stem}-signed-{n}.{ext}") };
                dir.join(name)
            })
            .find(|p| !p.exists())
            .ok_or("No free signed-copy filename")?;
        if !self.allow_additional_signatures { return Err("Native OPC policy-lock is not implemented; no signature was created.".into()); }
        sign_opc_inprocess(original, &output, &cert.thumbprint)?;
        // Fail closed: reopened independently; cryptographic integrity is NOT QTSA/QES.
        let _signatures = verify_opc_inprocess(&output)?;
        self.cancel();
        Ok(format!("Signed and independently checked: {}. eIDAS QES and visible Office signature-line NOT evaluated.", output.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_state_will_not_sign() {
        let mut s = OfficeSignUi::default();
        assert!(!s.active());
        assert!(s.sign(Path::new("nonexistent.docx")).is_err());
    }
}
