use super::controls::{act, anchor_keytip, control_keytip, menu_items, sep};
use super::{Icon, SheetApp, Stroke, StrokeKind, Tokens, Ui, big_button, json, small_button};
use crate::widgets::small_button_accent;

pub(super) fn review(app: &mut SheetApp, ui: &mut Ui) {
    let __k = big_button(ui, Icon::Spell, "Spelling", "Spelling (F7)", false);
    control_keytip(app, ui, &__k, "dialog:spelling");
    if __k.clicked() {
        app.open_dialog("spelling", json!({}));
    }
    let __k = big_button(ui, Icon::Calc, "Workbook\nStatistics", "Workbook Statistics", false);
    control_keytip(app, ui, &__k, "statistics");
    if __k.clicked() {
        app.open_dialog("statistics", json!({}));
    }
    let __k = big_button(ui, Icon::Validation, "Check\nAccessibility", "Check Accessibility", false);
    control_keytip(app, ui, &__k, "dialog:accessibility");
    if __k.clicked() {
        app.open_dialog("accessibility", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Comment, "New\nComment", "New Comment", false);
    control_keytip(app, ui, &__k, "dialog:comment");
    if __k.clicked() {
        app.open_dialog("comment", json!({"threaded": true}));
    }
    let __k = big_button(ui, Icon::Delete, "Delete", "Delete Comment", false);
    control_keytip(app, ui, &__k, "review.deleteComment");
    if __k.clicked() {
        act(app, "review.deleteComment", json!({}));
    }
    ui.vertical(|ui| {
        for (icon, label, tip, id) in
            [(Icon::Left, "Previous", "Previous Comment", "review.previousComment"), (Icon::Right, "Next", "Next Comment", "review.nextComment")]
        {
            if small_button(ui, icon, label, tip, false).clicked() {
                act(app, id, json!({}));
                app.grid.ensure_visible = true;
            }
        }
    });
    let shown = app.session.comments_pane;
    let r = big_button(ui, Icon::Comment, "Show\nComments", "Comments pane", false);
    if shown {
        ui.painter().rect_stroke(r.rect, 5.0, Stroke::new(2.0, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
    }
    if r.clicked() {
        act(app, "review.showComments", json!({"on": !shown}));
        if app.session.comments_pane {
            app.grid.pane = None;
        }
    }
    let notes = big_button(ui, Icon::Note, "Notes", "Notes", true);
    anchor_keytip(app, ui, &notes, &["review.nextComment"]);
    egui::Popup::menu(&notes).show(|ui| {
        menu_items(
            app,
            ui,
            &[
                ("New Note", "dialog:comment", json!({"threaded": false})),
                ("Show/Hide Note", "review.showNote", json!({})),
                ("Show All Notes", "review.showAllNotes", json!({})),
                ("-", "", json!({})),
                ("Convert to Comments", "review.convertToComments", json!({})),
            ],
        );
    });
    sep(ui);
    #[cfg(target_os = "windows")]
    {
        // Match GridCraft's 23px ribbon controls: two quiet icon-and-text rows instead
        // of unrelated oversized 72px tiles. The sign/verify actions remain unchanged.
        let current = app.session.active().map(|d| (d.path.clone(), d.is_dirty()));
        // Never verify the on-disk XLSX while a different unsaved
        // workbook state is visible to the current user.
        let can_verify = current.as_ref().and_then(|(path, _)| path.as_deref())
            .is_some_and(|p| {
                std::path::Path::new(p).extension().and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"))
            }) && current.as_ref().is_some_and(|(_, dirty)| !dirty)
                && !app.office_sign.busy();
        let can_sign = can_verify;
        let accent = Tokens::get(ui.ctx()).accent;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(7.0);
            let sign = ui.add_enabled_ui(can_sign, |ui| {
                small_button_accent(
                    ui, Icon::Pen, Some(accent), "Sign",
                    "Sign the saved XLSX as a new copy", false,
                )
            }).inner;
            if can_sign && sign.clicked() {
                let result = match current.as_ref() {
                    Some((Some(path), false)) => app.office_sign.begin(std::path::Path::new(path)),
                    _ => Err("Save workbook changes before signing.".to_owned()),
                };
                if let Err(error) = result {
                    app.message = Some(("Digital signature".into(), error));
                }
            }
            let verify = ui.add_enabled_ui(can_verify, |ui| {
                small_button(
                    ui, Icon::Validation, "Verify",
                    "Verify the saved XLSX signature integrity", false,
                )
            }).inner;
            if can_verify && verify.clicked() {
                let result = match current.as_ref() {
                    Some((Some(path), dirty)) => app.office_sign.verify(std::path::Path::new(path), *dirty),
                    _ => Err("Save the XLSX before verifying signatures.".to_owned()),
                };
                if let Err(error) = result {
                    app.message = Some(("Digital signature".into(), error));
                }
            }
            let verify_v2 = ui.add_enabled_ui(can_verify, |ui| {
                small_button(
                    ui, Icon::Validation, "Verify v2",
                    "Check saved XLSX via owner-approved signed Craft Signer v2 (OPC integrity only)", false,
                )
            }).inner;
            if can_verify && verify_v2.clicked() {
                let result = match current.as_ref() {
                    Some((Some(path), dirty)) => app.office_sign.verify_v2(std::path::Path::new(path), *dirty),
                    _ => Err("Save the XLSX before signed-module verification.".to_owned()),
                };
                if let Err(error) = result {
                    app.message = Some(("Digital signature".into(), error));
                }
            }
        });
        sep(ui);
    }
    let protected = app.active_sheet_protected();
    let __k = big_button(ui, Icon::Lock, if protected { "Unprotect\nSheet" } else { "Protect\nSheet" }, "Protect Sheet", false);
    control_keytip(app, ui, &__k, "dialog:protectSheet");
    if __k.clicked() {
        app.open_dialog("protectSheet", json!({}));
    }
    let __k = big_button(ui, Icon::Book, "Protect\nWorkbook", "Protect Workbook structure", false);
    control_keytip(app, ui, &__k, "review.protectWorkbook");
    if __k.clicked() {
        crate::backstage::protect_workbook(app);
    }
    if big_button(ui, Icon::Validation, "Allow Edit\nRanges", "Allow Edit Ranges", false).clicked() {
        app.open_dialog("allowEditRanges", json!({}));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod signed_v2_ui_qa {
    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn review_shows_compact_sign_verify_and_verify_v2_without_gpu() {
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        let mut app = SheetApp::new(session, crate::Services::default());
        app.ui.ribbon_tab = "Review".into();
        app.use_prepared_fonts();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(|ctx| {
                ctx.set_fonts(crate::theme::pinned_font_definitions());
                crate::theme::apply(ctx, false);
            })
            .renderer(egui_kittest::LazyRenderer::Uninitialized { textures_delta: Default::default(), builder: None })
            .build_ui_state(
                |ui, app: &mut SheetApp| {
                    app.logic(ui.ctx());
                    app.ui(ui);
                },
                app,
            );
        harness.run_steps(5);
        for label in ["Sign", "Verify", "Verify v2"] {
            let control = harness.query_by_role_and_label(Role::Button, label);
            assert!(control.is_some(), "Review contains accessible {label} control");
            let rect = control.unwrap().rect();
            assert!(rect.width() > 30.0 && rect.height() <= 34.0, "Review {label} uses a compact small_button: {rect:?}");
            assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0, "Review {label} remains inside the viewport");
        }
        assert!(!harness.state().office_sign.busy(), "headless accessibility check does not start a signer");
    }
}
