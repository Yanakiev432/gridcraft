//! Experimental **unsigned** DOCX/XLSX visible picture writer.
//! Images are written into the OPC package *before* XMLDSig signing.
//! WordCraft/GridCraft call this writer through an explicit opt-in path.
//! Document placement is currently fixed, not mapped from the screen rectangle;
//! native Office interactive Sign behavior needs separate acceptance.
//!
//! Fail closed on already signed inputs, active drawings and unusual XML.
//! No subprocess, no private-key access and no modification of source documents.
#![cfg(windows)]
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_IMAGE: usize = 2 * 1024 * 1024;
const MAX_XML: usize = 8 * 1024 * 1024;
const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug)]
pub enum OfficeImagePlacement {
    /// Appended as a Word paragraph before the final section properties.
    /// Not yet bound to the WordCraft screen rectangle.
    WordDocumentEnd,
    /// Floating appearance anchored to a Word body paragraph, with page-relative
    /// distances and size in document points. The caller maps screen geometry
    /// from WordCraft's actual layout, not an arbitrary viewport rectangle.
    WordPageRectangle {
        block_index: usize,
        left_pt: f32,
        top_pt: f32,
        width_pt: f32,
        height_pt: f32,
    },
    /// Picture anchored at the given zero-based column and row.
    /// Not yet bound to the GridCraft viewport rectangle.
    ExcelCell { column: u32, row: u32 },
    ExcelRectangle { column: u32, row: u32, col_offset: u32, row_offset: u32, width: u32, height: u32 },
}

/// Parse a user-entered Excel cell (A1 notation) into a zero-based anchor.
/// Rejects absolute references, ranges, formulas and out-of-sheet positions.
/// This does not infer coordinates from a screen rectangle or viewport zoom.
pub fn parse_excel_cell_anchor(value: &str) -> Result<OfficeImagePlacement, String> {
    let text = value.trim();
    if text.is_empty() || !text.is_ascii() || text.len() > 10 {
        return Err("Enter one Excel cell, for example E2".into());
    }
    let letters = text.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
    if !(1..=3).contains(&letters) {
        return Err("Excel column must be A through XFD".into());
    }
    let (column_letters, row_digits) = text.split_at(letters);
    if row_digits.is_empty() || row_digits.starts_with('0')
        || !row_digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("Excel row must be 1 through 1048576".into());
    }
    let mut col = 0_u32;
    for b in column_letters.bytes() {
        let letter = (b.to_ascii_uppercase() - b'A' + 1) as u32;
        col = col.checked_mul(26).and_then(|v| v.checked_add(letter))
            .ok_or("Excel column outside safe bounds")?;
    }
    let row = row_digits.parse::<u32>().map_err(|_| "Excel row is too large")?;
    if !(1..=16384).contains(&col) || !(1..=1048576).contains(&row) {
        return Err("Excel cell outside supported sheet bounds".into());
    }
    Ok(OfficeImagePlacement::ExcelCell { column: col - 1, row: row - 1 })
}

/// For **unsigned** source only, create a distinct unsigned file with a PNG
/// appearance embedded as an Office image. The returned document MUST then
/// be signed by the already verified native OPC implementation.
pub fn add_unsigned_visual_appearance(
    source: &Path,
    output: &Path,
    png: &[u8],
    signer_display_name: &str,
    placement: OfficeImagePlacement,
) -> Result<(), String> {
    if source == output || output.exists() {
        return Err("Visual output must be new and distinct from input".into());
    }
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
    let expected = match placement {
        OfficeImagePlacement::WordDocumentEnd | OfficeImagePlacement::WordPageRectangle { .. } => "docx",
        OfficeImagePlacement::ExcelCell { .. } | OfficeImagePlacement::ExcelRectangle { .. } => "xlsx",
    };
    if !extension.eq_ignore_ascii_case(expected) || output.extension() != source.extension() {
        return Err("Visual placement does not match DOCX/XLSX extension".into());
    }
    if !(1..=120).contains(&signer_display_name.trim().chars().count()) {
        return Err("Signer name length outside accepted range".into());
    }
    if signer_display_name.chars().any(char::is_control) {
        return Err("Signer contains control characters".into());
    }
    if png.len() > MAX_IMAGE || png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Only bounded real PNG bytes accepted".into());
    }
    let w = u32::from_be_bytes(png[16..20].try_into().map_err(|_| "Bad PNG width")?);
    let h = u32::from_be_bytes(png[20..24].try_into().map_err(|_| "Bad PNG height")?);
    if !(100..=2000).contains(&w) || !(40..=2000).contains(&h) {
        return Err("PNG width or height is outside appearance limits".into());
    }
    if let OfficeImagePlacement::ExcelCell { column, row } = placement
        && (column > 16383 || row > 1048575)
    {
        return Err("Excel anchor outside worksheet bounds".into());
    }
    if let OfficeImagePlacement::ExcelRectangle { column, row, col_offset, row_offset, width, height } = placement {
        if column > 16383 || row > 1048575 || col_offset > 20_000_000 || row_offset > 20_000_000
            || !(380_000..=20_000_000).contains(&width) || !(190_000..=12_000_000).contains(&height) {
            return Err("Excel drawn signature region is outside supported bounds".into());
        }
    }
    if let OfficeImagePlacement::WordPageRectangle { block_index, left_pt, top_pt, width_pt, height_pt } = placement {
        if block_index > 50_000 || ![left_pt, top_pt, width_pt, height_pt].iter().all(|v| v.is_finite())
            || left_pt < 0.0 || top_pt < 0.0 || left_pt > 2000.0 || top_pt > 3000.0
            || !(48.0..=1000.0).contains(&width_pt) || !(24.0..=550.0).contains(&height_pt)
            || left_pt + width_pt > 2300.0 || top_pt + height_pt > 3300.0
        {
            return Err("Word signature rectangle outside safe page bounds".into());
        }
    }

    let mut input = File::open(source).map_err(|e| e.to_string())?;
    // The zip crate indexes entries by name and can silently collapse exact
    // duplicates. Validate the raw central-directory entry count before
    // trusting archive.len() or any by_index() lookup.
    let declared_entries = zip32_central_directory_entries(&mut input)?;
    input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(input).map_err(|e| e.to_string())?;
    if archive.len() != declared_entries {
        return Err("Repeated ZIP central-directory names or collapsed entries".into());
    }
    if archive.is_empty() || archive.len() > 65520 {
        return Err("Office ZIP entry count outside accepted limit".into());
    }
    let mut bytes = std::collections::BTreeMap::<String, (Vec<u8>, CompressionMethod)>::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        let lower = name.to_ascii_lowercase();
        if name.is_empty() || name.starts_with('/') || name.contains('\\')
            || name.split('/').any(|segment| segment == ".." || segment == ".")
            || !seen.insert(lower.clone())
        {
            return Err("Unsafe or repeated ZIP entry name".into());
        }
        if lower.contains("_xmlsignatures")
            || lower.contains("package/services/digital-signature")
            || lower.contains("digital-signature/origin")
        {
            return Err("Already signed document: adding a visual would invalidate existing signatures".into());
        }
        if !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            return Err("Unsupported source compression".into());
        }
        total = total.checked_add(entry.size()).ok_or("Overflow")?;
        if total > MAX_TOTAL {
            return Err("Document exceeds 128 MiB experimental safety bound".into());
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        bytes.insert(name, (data, entry.compression()));
    }
    // ZIP filenames alone are insufficient to establish unsigned OPC input.
    // A relationship may point to a signature origin under an unexpected
    // part name. Decode XML attribute entities before classifying the type.
    reject_signature_relationships_and_content_types(&bytes)?;

    let stamp = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let image_filename = format!("CraftSignature-{stamp}.png");
    let rid = format!("rIdCraftSignature{stamp}");
    let base = match placement {
        OfficeImagePlacement::WordDocumentEnd | OfficeImagePlacement::WordPageRectangle { .. } => "word",
        OfficeImagePlacement::ExcelCell { .. } | OfficeImagePlacement::ExcelRectangle { .. } => "xl",
    };
    let image_part = format!("{base}/media/{image_filename}");
    if bytes.keys().any(|k| k.eq_ignore_ascii_case(&image_part)) {
        return Err("Image part collision".into());
    }
    match placement {
        OfficeImagePlacement::WordDocumentEnd => {
            inject_word(&mut bytes, &rid, &image_filename, signer_display_name, stamp, None)?
        }
        OfficeImagePlacement::WordPageRectangle { block_index, left_pt, top_pt, width_pt, height_pt } => {
            inject_word(&mut bytes, &rid, &image_filename, signer_display_name, stamp,
                Some((block_index, left_pt, top_pt, width_pt, height_pt)))?
        }
        OfficeImagePlacement::ExcelCell { column, row } => {
            inject_excel(&mut bytes, &rid, &image_filename, stamp, column, row, 0, 0, 4_876_800, 1_414_275)?
        }
        OfficeImagePlacement::ExcelRectangle { column, row, col_offset, row_offset, width, height } => {
            inject_excel(&mut bytes, &rid, &image_filename, stamp, column, row, col_offset, row_offset, width, height)?
        }
    }
    bytes.insert(image_part, (png.to_vec(), CompressionMethod::Deflated));
    let file = OpenOptions::new().write(true).create_new(true).open(output)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut writer = ZipWriter::new(file);
        for (name, (data, method)) in bytes {
            writer.start_file(name, SimpleFileOptions::default()
                .compression_method(method).large_file(false))
                .map_err(|e| e.to_string())?;
            writer.write_all(&data).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?.sync_all()
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

/// Parse ZIP32 EOCD directly. A ZIP archive can contain repeated central
/// directory names that ZipArchive silently collapses into one entry.
fn zip32_central_directory_entries(file: &mut File) -> Result<usize, String> {
    let total_bytes = file.metadata().map_err(|e| e.to_string())?.len();
    if total_bytes < 22 {
        return Err("ZIP32 end-of-central-directory is missing".into());
    }
    let tail_len = total_bytes.min(22 + 65535) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|e| e.to_string())?;
    let mut tail = vec![0u8; tail_len];
    file.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let eocd = (0..=tail_len - 22)
        .rev()
        .find(|&i| {
            &tail[i..i + 4] == b"PK\x05\x06"
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail_len
        })
        .ok_or("ZIP32 EOCD not found or archive has trailing data")?;
    let u16_at = |n: usize| u16::from_le_bytes([tail[eocd + n], tail[eocd + n + 1]]);
    let u32_at = |n: usize| u32::from_le_bytes(
        tail[eocd + n..eocd + n + 4]
            .try_into()
            .expect("validated 22-byte EOCD"),
    );
    let entries = u16_at(10);
    if u16_at(4) != 0
        || u16_at(6) != 0
        || u16_at(8) != entries
        || entries == u16::MAX
        || u32_at(12) == u32::MAX
        || u32_at(16) == u32::MAX
    {
        return Err("Multi-disk or ZIP64 package is outside safe Office ZIP32 profile".into());
    }
    let offset = u32_at(16) as u64;
    let cd_size = u32_at(12) as u64;
    let eocd_file_offset = total_bytes - tail_len as u64 + eocd as u64;
    if offset.checked_add(cd_size) != Some(eocd_file_offset) {
        return Err("ZIP32 central directory length or offset mismatch".into());
    }
    Ok(entries as usize)
}

/// Recognize drawing elements regardless of XML namespace prefix. Existing
/// workbook drawing relationships must never be silently overridden.
fn has_drawing_element(xml: &str) -> bool {
    xml.split('<').skip(1).filter_map(|part| {
        let tag = part.split(|c: char| c.is_whitespace() || c == '>').next()?;
        if tag.starts_with('/') || tag.starts_with('!') || tag.starts_with('?') {
            return None;
        }
        Some(tag.trim_end_matches('/'))
    }).any(|tag| tag == "drawing" || tag.ends_with(":drawing"))
}

/// Refuse any package which *advertises* signatures, even if the signature
/// parts are renamed or absent. A signed source must never be rewritten merely
/// because its ZIP filenames do not match the conventional _xmlsignatures path.
/// XML is parsed with namespace awareness; attribute character references are
/// decoded before checking relationship/content-type values.
fn reject_signature_relationships_and_content_types(
    bytes: &std::collections::BTreeMap<String, (Vec<u8>, CompressionMethod)>,
) -> Result<(), String> {
    for (name, (content, _)) in bytes {
        if !(name.ends_with(".rels") || name == "[Content_Types].xml") {
            continue;
        }
        if content.len() > MAX_XML {
            return Err(format!("OPC metadata too large: {name}"));
        }
        let xml = std::str::from_utf8(content)
            .map_err(|_| format!("Non-UTF8 OPC metadata in {name}"))?;
        if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
            return Err("OPC metadata DTD/entities are not supported".into());
        }
        let document = roxmltree::Document::parse(xml)
            .map_err(|e| format!("Malformed OPC relationship/content types XML in {name}: {e}"))?;
        let root = document.root_element();
        if name.ends_with(".rels") {
            if root.tag_name().namespace() != Some(REL_NS)
                || root.tag_name().name() != "Relationships"
            {
                return Err("Unexpected OPC relationship root namespace".into());
            }
            for entry in root.children().filter(|n| n.is_element()) {
                if entry.tag_name().namespace() != Some(REL_NS)
                    || entry.tag_name().name() != "Relationship"
                {
                    return Err("Unexpected relationship element".into());
                }
                let relationship_type = entry.attribute("Type")
                    .ok_or("Relationship without a Type attribute")?;
                if relationship_type.to_ascii_lowercase().contains("digital-signature")
                    || relationship_type.to_ascii_lowercase().contains("digitalsignature")
                {
                    return Err("Already signed or signature-marked OPC relationships".into());
                }
            }
        } else {
            if root.tag_name().namespace() != Some(CT_NS)
                || root.tag_name().name() != "Types"
            {
                return Err("Unexpected OPC content-types root namespace".into());
            }
            for entry in root.children().filter(|n| n.is_element()) {
                let ct = entry.attribute("ContentType").unwrap_or("");
                if ct.to_ascii_lowercase().contains("digital-signature")
                    || ct.to_ascii_lowercase().contains("digitalsignature")
                {
                    return Err("Existing OPC signature content type forbids visual rewrite".into());
                }
            }
        }
    }
    Ok(())
}

fn xml_part(bytes: &std::collections::BTreeMap<String,(Vec<u8>, CompressionMethod)>, name: &str) -> Result<String, String> {
    let raw = &bytes.get(name).ok_or_else(|| format!("Missing OPC XML part {name}"))?.0;
    if raw.len() > MAX_XML {
        return Err("XML part exceeds size bound".into());
    }
    let string = std::str::from_utf8(raw).map_err(|_| format!("Non-UTF8 OPC XML in {name}"))?;
    if string.contains("<!DOCTYPE") || string.contains("<!ENTITY") {
        return Err("XML DTD/entity not supported for signed visual assets".into());
    }
    Ok(string.to_owned())
}
fn put_xml(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>, name: &str, xml: String) {
    bytes.insert(name.to_owned(), (xml.into_bytes(), CompressionMethod::Deflated));
}
fn add_xml(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>, name: String, xml: String) -> Result<(),String> {
    if bytes.contains_key(&name) {
        return Err(format!("OPC part already exists: {name}"));
    }
    put_xml(bytes, &name, xml);
    Ok(())
}
fn before_last(xml: &mut String, marker: &str, addition: &str) -> Result<(),String> {
    let loc = xml.rfind(marker).ok_or_else(|| format!("Missing XML insertion point {marker}"))?;
    xml.insert_str(loc, addition);
    Ok(())
}
fn sanitize_xml(input: &str) -> String {
    input.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;")
         .replace('"',"&quot;").replace('\'',"&apos;")
}
fn add_ct(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>, drawing: bool) -> Result<(),String> {
    let mut ct = xml_part(bytes, "[Content_Types].xml")?;
    if !ct.contains(CT_NS) {
        return Err("Content Types namespace mismatch".into());
    }
    let mut inserts = String::new();
    if !ct.contains("Extension=\"png\"") && !ct.contains("Extension='png'") {
        inserts.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
    }
    if drawing {
        if ct.contains("PartName=\"/xl/drawings/craftSignatureDrawing.xml\"") {
            return Err("Drawing content-type collision".into());
        }
        inserts.push_str("<Override PartName=\"/xl/drawings/craftSignatureDrawing.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>");
    }
    before_last(&mut ct, "</Types>", &inserts)?;
    put_xml(bytes, "[Content_Types].xml", ct);
    Ok(())
}
fn add_rel(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>, part: &str, id: &str, relation_type: &str, target: &str) -> Result<(),String> {
    let addition = format!("<Relationship Id=\"{id}\" Type=\"{R_NS}/{relation_type}\" Target=\"{target}\"/>");
    let mut xml = if bytes.contains_key(part) {
        xml_part(bytes, part)?
    } else {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{REL_NS}\"></Relationships>")
    };
    if !xml.contains(REL_NS) || xml.contains(&format!("Id=\"{id}\"")) {
        return Err("Relationships namespace mismatch or ID collision".into());
    }
    before_last(&mut xml, "</Relationships>", &addition)?;
    put_xml(bytes, part, xml);
    Ok(())
}
fn inject_word(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>,
    rid: &str, file: &str, signer: &str, stamp: u64,
    page_rectangle: Option<(usize, f32, f32, f32, f32)>) -> Result<(),String> {
    let mut xml = xml_part(bytes,"word/document.xml")?;
    if !xml.contains("http://schemas.openxmlformats.org/wordprocessingml/2006/main")
        || !xml.contains("<w:body") || !xml.contains("</w:body>") {
        return Err("Unsupported Word DOCX main body structure".into());
    }
    let safe_signer = sanitize_xml(signer);
    // An absolute VML shape stays attached to a real Word body paragraph;
    // its margins are page-relative, as in native Word floating pictures.
    // Never treat a screen-only rectangle as a valid document coordinate.
    let style = if let Some((_, x, y, w, h)) = page_rectangle {
        format!("position:absolute;margin-left:{x:.2}pt;margin-top:{y:.2}pt;\
                 width:{w:.2}pt;height:{h:.2}pt;z-index:251658240;\
                 mso-position-horizontal-relative:page;\
                 mso-position-vertical-relative:page")
    } else {
        "width:330pt;height:95pt".to_owned()
    };
    let run = format!(
        "<w:r><w:pict><v:shape xmlns:v=\"urn:schemas-microsoft-com:vml\" \
         xmlns:o=\"urn:schemas-microsoft-com:office:office\" \
         xmlns:r=\"{R_NS}\" \
         id=\"_x0000_s{stamp}\" style=\"{style}\" filled=\"f\" stroked=\"f\">\
         <v:imagedata r:id=\"{rid}\" o:title=\"Craft signature appearance\"/>\
         <o:signatureline v:ext=\"edit\" id=\"{{00000000-0000-4000-8000-{stamp:012}}}\" \
         provid=\"{{00000000-0000-0000-0000-000000000000}}\" issignatureline=\"t\" \
         showsigndate=\"t\" suggestedsigner=\"{safe_signer}\"/>\
         </v:shape></w:pict></w:r>");
    if let Some((block_index, ..)) = page_rectangle {
        // The layout's first_block index addresses direct body blocks.
        // Only paragraph anchors are supported; never guess a page or insert
        // into a table and silently shift the signed appearance.
        let insertion = {
            let document = roxmltree::Document::parse(&xml)
                .map_err(|e| format!("Word main XML is invalid: {e}"))?;
            let body = document.descendants().find(|n| n.is_element()
                    && n.tag_name().name() == "body"
                    && n.tag_name().namespace() ==
                        Some("http://schemas.openxmlformats.org/wordprocessingml/2006/main"))
                .ok_or("Word body XML was not found")?;
            let chosen = body.children().filter(|n| n.is_element()
                && matches!(n.tag_name().name(), "p" | "tbl"))
                .nth(block_index)
                .ok_or("Word page anchor block is not present in the saved DOCX")?;
            if chosen.tag_name().name() != "p" {
                return Err("Cannot anchor a signature to a table. Select a text paragraph.".into());
            }
            let range = chosen.range();
            let tag = "</w:p>";
            if !xml[range.clone()].ends_with(tag) {
                return Err("Unsupported Word paragraph namespace/structure".into());
            }
            range.end - tag.len()
        };
        xml.insert_str(insertion, &run);
    } else {
        let paragraph = format!("<w:p>{run}</w:p>");
        let body_close = xml.rfind("</w:body>").ok_or("Word body closure missing")?;
        let before_section = xml[..body_close].rfind("<w:sectPr").unwrap_or(body_close);
        xml.insert_str(before_section, &paragraph);
    }
    put_xml(bytes,"word/document.xml",xml);
    add_rel(bytes,"word/_rels/document.xml.rels",rid,"image",&format!("media/{file}"))?;
    add_ct(bytes,false)
}
fn inject_excel(bytes: &mut std::collections::BTreeMap<String,(Vec<u8>,CompressionMethod)>, rid: &str, file: &str, stamp: u64, column: u32, row: u32, col_offset: u32, row_offset: u32, width: u32, height: u32) -> Result<(),String> {
    let mut xml = xml_part(bytes,"xl/worksheets/sheet1.xml")?;
    if !xml.contains("http://schemas.openxmlformats.org/spreadsheetml/2006/main")
        || !xml.contains("</worksheet>")
        || has_drawing_element(&xml) {
        return Err("Unsupported worksheet or existing drawing: no destructive replacement".into());
    }
    let node=format!("<drawing xmlns:r=\"{R_NS}\" r:id=\"{rid}\"/>");
    let pos=xml.find("<extLst").unwrap_or_else(||xml.rfind("</worksheet>").unwrap());
    xml.insert_str(pos,&node);
    put_xml(bytes,"xl/worksheets/sheet1.xml",xml);
    let drawing_name="xl/drawings/craftSignatureDrawing.xml".to_string();
    if bytes.contains_key(&drawing_name) {return Err("Existing Craft drawing collision".into());}
    add_rel(bytes,"xl/worksheets/_rels/sheet1.xml.rels",rid,"drawing","../drawings/craftSignatureDrawing.xml")?;
    let picture_rid=format!("rIdCraftSignaturePicture{stamp}");
    let drawing=format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"{R_NS}\">\
         <xdr:oneCellAnchor><xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>{col_offset}</xdr:colOff>\
         <xdr:row>{row}</xdr:row><xdr:rowOff>{row_offset}</xdr:rowOff></xdr:from>\
         <xdr:ext cx=\"{width}\" cy=\"{height}\"/>\
         <xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"1\" name=\"Craft digital signature\"/>\
         <xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr>\
         <xdr:blipFill><a:blip r:embed=\"{picture_rid}\"/><a:stretch><a:fillRect/>\
         </a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/>\
         <a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr>\
         </xdr:pic><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>");
    add_xml(bytes,drawing_name,drawing)?;
    add_xml(bytes,"xl/drawings/_rels/craftSignatureDrawing.xml.rels".to_string(),
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <Relationships xmlns=\"{REL_NS}\">\
            <Relationship Id=\"{picture_rid}\" Type=\"{R_NS}/image\" Target=\"../media/{file}\"/>\
            </Relationships>"))?;
    add_ct(bytes,true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excel_drawn_rectangle_writes_exact_opc_drawing_geometry() {
        let mut parts=std::collections::BTreeMap::new();
        let sheet=r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#;
        let ct=format!(r#"<Types xmlns="{CT_NS}"><Default Extension="xml" ContentType="application/xml"/></Types>"#);
        parts.insert("xl/worksheets/sheet1.xml".to_owned(),(sheet.as_bytes().to_vec(),CompressionMethod::Deflated));
        parts.insert("[Content_Types].xml".to_owned(),(ct.into_bytes(),CompressionMethod::Deflated));
        inject_excel(&mut parts,"rIdCraftX","stamp.png",42,3,7,95000,47500,3800000,1140000).unwrap();
        let raw=std::str::from_utf8(&parts["xl/drawings/craftSignatureDrawing.xml"].0).unwrap();
        assert!(raw.contains("<xdr:col>3</xdr:col><xdr:colOff>95000</xdr:colOff>"));
        assert!(raw.contains("<xdr:row>7</xdr:row><xdr:rowOff>47500</xdr:rowOff>"));
        assert!(raw.contains(r#"<xdr:ext cx="3800000" cy="1140000"/>"#));
        assert!(raw.contains("rIdCraftSignaturePicture42"));
        assert!(std::str::from_utf8(&parts["xl/worksheets/sheet1.xml"].0).unwrap().contains("rIdCraftX"));
    }

    #[test]
    fn word_page_rectangle_is_anchored_to_selected_body_paragraph() {
        let mut parts = std::collections::BTreeMap::new();
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>first</w:t></w:r></w:p><w:p><w:r><w:t>second</w:t></w:r></w:p></w:body></w:document>"#;
        parts.insert("word/document.xml".to_owned(), (body.as_bytes().to_vec(), CompressionMethod::Deflated));
        let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#;
        parts.insert("[Content_Types].xml".to_owned(), (types.as_bytes().to_vec(), CompressionMethod::Deflated));
        inject_word(&mut parts, "rIdCraft7", "CraftSignature-7.png", "QA signer", 7,
                    Some((1, 124.0, 80.0, 320.0, 96.0))).unwrap();
        let xml = std::str::from_utf8(&parts["word/document.xml"].0).unwrap();
        let a = xml.find("first").unwrap();
        let b = xml.find("position:absolute").unwrap();
        let c = xml.find("second").unwrap();
        assert!(a < c && c < b && b < xml.rfind("</w:p>").unwrap(), "appearance anchor must be in the selected second paragraph");
        assert!(xml.contains("margin-left:124.00pt"));
        assert!(xml.contains("margin-top:80.00pt"));
        assert!(xml.contains("width:320.00pt;height:96.00pt"));
        assert!(xml.contains("mso-position-vertical-relative:page"));
        assert!(xml.contains("rIdCraft7"));
    }

    #[test]
    fn escapes_untrusted_certificate_display_names() {
        assert_eq!(sanitize_xml("A < B & \"C\""), "A &lt; B &amp; &quot;C&quot;");
    }
    #[test]
    fn existing_excel_drawings_are_detected_even_with_namespace_prefixes() {
        assert!(has_drawing_element("<worksheet><drawing r:id=\"rId1\"/></worksheet>"));
        assert!(has_drawing_element("<worksheet><x15:drawing r:id=\"rId1\"/></worksheet>"));
        assert!(has_drawing_element("<worksheet><z:drawing/></worksheet>"));
        assert!(!has_drawing_element("<worksheet><c v=\"drawing\"/></worksheet>"));
    }
    #[test]
    fn signed_package_relations_are_rejected_even_with_entity_encoded_type() {
        let mut parts = std::collections::BTreeMap::new();
        let rels = format!(
            "<Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"s1\" \
            Type=\"http://schemas.openxmlformats.org/package/2006/relationships/digital&#45;signature/origin\" \
            Target=\"some-obscure-path.xml\"/></Relationships>"
        );
        parts.insert("_rels/.rels".into(), (rels.into_bytes(), CompressionMethod::Stored));
        assert!(reject_signature_relationships_and_content_types(&parts).is_err());
    }

    #[test]
    fn unsigned_office_package_relations_are_accepted() {
        let mut parts = std::collections::BTreeMap::new();
        let rels = format!(
            "<Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" \
             Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
             Target=\"word/document.xml\"/></Relationships>"
        );
        parts.insert("_rels/.rels".into(), (rels.into_bytes(), CompressionMethod::Stored));
        assert!(reject_signature_relationships_and_content_types(&parts).is_ok());
    }

    #[test]
    fn advertised_signature_content_type_fails_closed() {
        let mut parts = std::collections::BTreeMap::new();
        let types = format!(
            "<Types xmlns=\"{CT_NS}\"><Override PartName=\"/unusual/sign.xml\" \
            ContentType=\"application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml\"/></Types>"
        );
        parts.insert("[Content_Types].xml".into(), (types.into_bytes(), CompressionMethod::Stored));
        assert!(reject_signature_relationships_and_content_types(&parts).is_err());
    }

    #[test]
    fn excel_cell_accepts_case_and_sheet_boundaries() {
        assert!(matches!(parse_excel_cell_anchor(" E2 "), Ok(OfficeImagePlacement::ExcelCell { column: 4, row: 1 })));
        assert!(matches!(parse_excel_cell_anchor("a1"), Ok(OfficeImagePlacement::ExcelCell { column: 0, row: 0 })));
        assert!(matches!(parse_excel_cell_anchor("XFD1048576"), Ok(OfficeImagePlacement::ExcelCell { column: 16383, row: 1048575 })));
        assert!(matches!(parse_excel_cell_anchor("ZZ100"), Ok(OfficeImagePlacement::ExcelCell { column: 701, row: 99 })));
    }

    #[test]
    fn excel_cell_rejects_bad_or_ambiguous_references() {
        for invalid in ["", "0", "A0", "A01", "A1048577", "XFE1", "ZZZ1",
                        "AAAA2", "A-1", "$A$1", "A1:B2", "Sheet2!E2",
                        "1A", "=A1", "E 2", "А1", "A999999999999999"] {
            assert!(parse_excel_cell_anchor(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn no_missing_xml_close_accepted() {
        let mut xml="<Types />".to_owned();
        assert!(before_last(&mut xml,"</Types>","<Default/>").is_err());
    }
}
