//! Adapted from PdfCraft's Digital ID choose/draw/confirm interaction.
//! Office documents use OPC/XMLDSig via the common Craft signing core,
//! NOT PDF PAdES. A drawn on-screen rectangle is an optional screen guide;
//! it must not be misrepresented as an embedded Office signature-line object.
use std::path::{Path, PathBuf};

use craft_signing_core::office_certificates::{OfficeCertificate, list_office_certificates};
use craft_signing_core::office_appearance::OfficeSignatureAppearance;
use craft_signing_core::office_visible_opc::{OfficeImagePlacement, parse_excel_cell_anchor};
use craft_signing_core::office_visible_sign::sign_with_visible_appearance;
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
    appearance: OfficeSignatureAppearance,
    embed_visible_at_default_location: bool,
    excel_anchor_cell: String,
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
        self.appearance = OfficeSignatureAppearance::default();
        self.embed_visible_at_default_location = false;
        self.excel_anchor_cell = "E2".to_owned();
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

    /// Optional on-screen guide. Actual document position comes from the
    /// Word document end or the explicitly entered Excel cell, not this guide.
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
                ui.label("Optional screen guide only. The rectangle is NOT mapped into DOCX/XLSX signing coordinates.");
                if ui.button("Back to certificates").clicked() {
                    self.phase = Phase::Choose;
                    self.drag_start = None;
                }
            });
    }
    /// Choose a Digital ID and confirm its provider-controlled signature.
    /// The optional screen guide is not required for a valid signing request.
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
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.allow_additional_signatures, "Allow additional signatures (required for this signing mode)"));
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
                        if ui.add_enabled(can_draw, egui::Button::new("Continue to signature settings")).clicked() {
                            self.rect = None;
                            self.phase = Phase::Confirm;
                        }
                        if ui.add_enabled(can_draw, egui::Button::new("Optional screen guide")).clicked() {
                            self.phase = Phase::Draw;
                        }
                        if ui.button("Cancel").clicked() { self.cancel() }
                    });
                } else {
                    let c = self.selected.and_then(|i| self.certificates.get(i));
                    ui.label(format!("Certificate: {}",c.map_or("Missing",|c|c.subject.as_str())));
                    if let Some(r) = self.rect {
                        ui.small(format!("Optional guide: {:.0} x {:.0} screen points (not embedded at this position)",r.width(),r.height()));
                    }
                    // Mirrors PdfCraft's appearance preview without promising that an
                    // editable Office line has already been embedded in the package.
                    let signer = c.map(|c| OfficeSignatureAppearance::subject_display_name(&c.subject))
                        .unwrap_or("Unknown signer").to_owned();
                    let lines = self.appearance.details(&signer);
                    ui.label(egui::RichText::new("Signature appearance — PdfCraft layout").strong());
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
                    let file_ext = self.source.as_deref().and_then(|p| p.extension())
                        .and_then(|s| s.to_str()).unwrap_or("").to_owned();
                    let destination = if file_ext.eq_ignore_ascii_case("docx") {
                        "the end of the Word document"
                    } else if file_ext.eq_ignore_ascii_case("xlsx") {
                        "Excel sheet 1, at the cell specified below"
                    } else {
                        "an unsupported document format"
                    };
                    ui.checkbox(&mut self.embed_visible_at_default_location,
                        format!("Embed this PDFCraft-style appearance at {destination}"));
                    if self.embed_visible_at_default_location {
                        if file_ext.eq_ignore_ascii_case("xlsx") {
                            ui.horizontal(|ui| {
                                ui.label("Excel anchor cell on first worksheet:");
                                ui.add(egui::TextEdit::singleline(&mut self.excel_anchor_cell)
                                    .hint_text("E2").desired_width(90.0));
                            });
                            if let Err(e) = parse_excel_cell_anchor(&self.excel_anchor_cell) {
                                ui.colored_label(egui::Color32::RED, e);
                            }
                        }
                        ui.colored_label(egui::Color32::YELLOW,
                            "The optional screen guide is not used as a document position. Signing creates a new copy and refuses already-signed input.");
                        ui.small("Date shown in the graphic comes from the local device clock, not from a trusted timestamp authority.");
                    } else {
                        ui.small("Preview only: no graphic will be embedded. The resulting Office OPC signature will be invisible.");
                    }
                    ui.small("Office-native Sign / Signature Setup on this image has not been independently verified.");
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.allow_additional_signatures, "Allow additional signatures (required for this signing mode)"));
                    if !self.allow_additional_signatures {
                        ui.small("Disabling additional signatures applies a Craft-only signed policy; other Office programs may not honor this restriction.");
                    }
                    ui.label("The Windows certificate provider will request your PIN if necessary. Never enter your PIN into Craft.");
                    ui.horizontal(|ui| {
                        if ui.button("Back").clicked() { self.phase = Phase::Choose }
                        let anchor_is_valid = !self.embed_visible_at_default_location
                            || !file_ext.eq_ignore_ascii_case("xlsx")
                            || parse_excel_cell_anchor(&self.excel_anchor_cell).is_ok();
                        if ui.add_enabled(anchor_is_valid, egui::Button::new("Sign a separate copy")).clicked() { action = Some(Action::Sign) }
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
        if self.phase != Phase::Confirm {
            return Err("Choose a certificate and confirm the signing options before signing.".into());
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
        let embed_visible = self.embed_visible_at_default_location;
        if embed_visible {
            let placement = if ext.eq_ignore_ascii_case("docx") {
                OfficeImagePlacement::WordDocumentEnd
            } else {
                parse_excel_cell_anchor(&self.excel_anchor_cell)?
            };
            sign_with_visible_appearance(
                original, &output, &cert.thumbprint, &self.appearance, placement,
            )?;
        } else {
            sign_opc_inprocess(original, &output, &cert.thumbprint)?;
        }
        // Fail closed: reopened independently; integrity is not eIDAS/QES.
        let _signatures = verify_opc_inprocess(&output)?;
        self.cancel();
        let appearance_status = if embed_visible {
            "PdfCraft-style visual asset embedded at the stated default Office location. Native Office Sign button NOT evaluated."
        } else {
            "Invisible OPC signature; graphical preview was not embedded."
        };
        Ok(format!("Signed and independently checked: {}. {} eIDAS QES not evaluated.",
                   output.display(), appearance_status))
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

    #[test]
    fn confirm_no_longer_requires_an_unmapped_screen_rectangle() {
        let mut s = OfficeSignUi::default();
        s.phase = Phase::Confirm;
        assert!(s.rect.is_none());
        assert_eq!(
            s.sign(Path::new("nonexistent.docx")).unwrap_err(),
            "No original document"
        );
    }
}
