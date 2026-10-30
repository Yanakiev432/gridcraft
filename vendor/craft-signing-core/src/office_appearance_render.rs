//! Real in-process PNG signature appearance, following PdfCraft's
//! 1000x290 name-left / details-right layout with Segoe UI system fonts.
//! This is a graphical representation, not a legal trust statement.
#![cfg(windows)]
use crate::office_appearance::OfficeSignatureAppearance;
use resvg::{tiny_skia, usvg};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 290;

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

fn safe_label(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.chars().any(char::is_control) || t.chars().count() > 120 {
        return Err("Signature appearance label contains a control or is too long".into());
    }
    Ok(escape_xml(t))
}

fn svg_string(
    appearance: &OfficeSignatureAppearance,
    signer: &str,
    clock_label: &str,
) -> Result<String, String> {
    let signer = safe_label(signer)?;
    if signer.is_empty() {
        return Err("Visible signer identity cannot be empty".into());
    }
    let clock = safe_label(clock_label)?;
    let mut right: Vec<String> = Vec::new();
    if appearance.name {
        if appearance.labels { right.push("Digitally signed by".to_owned()); }
        right.push(signer.clone());
    }
    if appearance.reason && !appearance.reason_text.trim().is_empty() {
        right.push(format!("{}{}", if appearance.labels { "Reason: " } else { "" },
                           safe_label(&appearance.reason_text)?));
    }
    if appearance.location && !appearance.location_text.trim().is_empty() {
        right.push(format!("{}{}", if appearance.labels { "Location: " } else { "" },
                           safe_label(&appearance.location_text)?));
    }
    if appearance.date {
        right.push(format!("{}{}", if appearance.labels { "Device time: " } else { "" },clock));
    }
    let displayed_left = if appearance.name { signer.as_str() } else { "Digital signature" };
    let signer_len = displayed_left.chars().count();
    let signer_size = if signer_len > 48 { 21 } else if signer_len > 32 { 27 } else { 37 };
    let line_count = right.len().max(1);
    let line_spacing = if line_count > 5 { 30 } else { 38 };
    let total_height = (line_count - 1) * line_spacing;
    let start_y = 136usize.saturating_sub(total_height / 2);
    let mut spans = String::new();
    for (index, line) in right.iter().enumerate() {
        let y = start_y + index * line_spacing;
        let fs = if line.chars().count() > 44 { 18 } else { 22 };
        spans.push_str(&format!(
            "<text x=\"530\" y=\"{y}\" font-size=\"{fs}\" fill=\"#2D3746\">{line}</text>"
        ));
    }
    Ok(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
      <rect x="2" y="2" width="996" height="286" fill="white" stroke="#ADB7C3" stroke-width="2"/>
      <line x1="500" y1="25" x2="500" y2="265" stroke="#DAE0E6" stroke-width="2"/>
      <g font-family="Segoe UI, Arial">
        <text x="28" y="48" font-size="24" fill="#505B6D">DIGITAL SIGNATURE</text>
        <text x="28" y="170" font-size="{signer_size}" font-weight="bold" fill="#202A3C">{displayed_left}</text>
        {spans}
        <text x="530" y="263" font-size="18" fill="#677588">OPC / XML-DSig</text>
      </g>
    </svg>"##))
}

/// Generate a PDFCraft-like *visual appearance* from the certificate name and
/// untrusted local system clock. No font files are embedded in the artifact.
pub fn render_png(
    appearance: &OfficeSignatureAppearance,
    signer: &str,
    clock_label: &str,
) -> Result<Vec<u8>, String> {
    let svg = svg_string(appearance, signer, clock_label)?;
    let mut opts = usvg::Options::default();
    let font_regular = r"C:\Windows\Fonts\segoeui.ttf";
    let font_bold = r"C:\Windows\Fonts\segoeuib.ttf";
    opts.fontdb_mut().load_font_file(font_regular)
        .map_err(|e| format!("Windows Segoe UI font unavailable: {e}"))?;
    let _ = opts.fontdb_mut().load_font_file(font_bold);
    let tree = usvg::Tree::from_str(&svg, &opts)
        .map_err(|e| format!("SVG appearance layout rejected: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT)
        .ok_or("Image buffer allocation failure")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let png = pixmap.encode_png().map_err(|e| format!("PNG encoding failed: {e}"))?;
    if png.len() > 2 * 1024 * 1024 {
        return Err("Generated signature appearance PNG too large".into());
    }
    Ok(png)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_escapes_untrusted_subjects() {
        let p=OfficeSignatureAppearance::default();
        let svg=svg_string(&p,"<Craft & Co>","2026-10-23").unwrap();
        assert!(svg.contains("&lt;Craft &amp; Co&gt;"));
        assert!(!svg.contains("<Craft & Co>"));
    }
    #[test]
    fn labels_can_be_absent() {
        let p = OfficeSignatureAppearance {
            labels: false,
            date: false,
            ..Default::default()
        };
        assert!(!svg_string(&p,"Test","UNTRUSTED").unwrap().contains("UNTRUSTED"));
    }
    #[test]
    fn control_characters_fail_closed() {
        assert!(render_png(&OfficeSignatureAppearance::default(),"Test\nSigner","time").is_err());
    }
}
