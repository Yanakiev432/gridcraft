//! Explicit, unsigned XLSX preparation for Microsoft Excel signature acceptance.
//! Changes only a NEW, unsigned staging copy; it never edits the source.
//! Switching to manual calculation can leave formulas stale until recalculated:
//! the caller MUST obtain specific user consent before invoking this API.
#![cfg(windows)]
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 65535;

fn escaped(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;")
        .replace('>', "&gt;").replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn manual_calc_xml(xml: &str) -> Result<String, String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Invalid workbook.xml: {e}"))?;
    let root = document.root_element();
    if root.tag_name().name() != "workbook" || root.tag_name().namespace() != Some(NS) {
        return Err("Not a standard SpreadsheetML workbook".into());
    }
    let nodes: Vec<_> = root.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "calcPr"
            && n.tag_name().namespace() == Some(NS)).collect();
    if nodes.len() != 1 {
        return Err("Excel-compatible V39 signing requires exactly one calcPr element; save an unsigned copy from Excel first".into());
    }
    let node = nodes[0];
    if node.children().any(|n| n.is_element() || n.is_text() && n.text().is_some_and(|t| !t.trim().is_empty())) {
        return Err("Unexpected calcPr content".into());
    }
    let mut replacement = String::from("<calcPr");
    for a in node.attributes() {
        if a.namespace().is_some() {
            return Err("Namespaced calcPr attributes are not supported by the conservative signing preparation".into());
        }
        if matches!(a.name(), "calcMode" | "fullCalcOnLoad" | "forceFullCalc") { continue; }
        replacement.push_str(&format!(" {}=\"{}\"", a.name(), escaped(a.value())));
    }
    replacement.push_str(" calcMode=\"manual\" fullCalcOnLoad=\"0\" forceFullCalc=\"0\"/>");
    let span = node.range();
    let mut result = String::with_capacity(xml.len() + 75);
    result.push_str(&xml[..span.start]);
    result.push_str(&replacement);
    result.push_str(&xml[span.end..]);
    // Parsing the output also catches incorrect replacements and namespace loss.
    roxmltree::Document::parse(&result).map_err(|e| format!("Prepared workbook.xml invalid: {e}"))?;
    Ok(result)
}

/// Copy an unsigned XLSX to a NEW destination with explicit manual calculation.
/// Call only after informed user consent. Existing signatures are rejected,
/// never silently invalidated. All other package-part bytes remain identical.
pub fn prepare_xlsx_for_excel_signing(source: &Path, output: &Path) -> Result<(), String> {
    if source == output || output.exists() {
        return Err("V39 needs a new unsigned staging file; refusing overwrite".into());
    }
    if !source.extension().is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
        || !output.extension().is_some_and(|e| e.eq_ignore_ascii_case("xlsx")) {
        return Err("V39 supports only unsigned XLSX packages".into());
    }
    let input = File::open(source).map_err(|e| format!("Cannot read source XLSX: {e}"))?;
    let mut archive = ZipArchive::new(input).map_err(|e| format!("Invalid XLSX ZIP: {e}"))?;
    if archive.len() == 0 || archive.len() > MAX_ENTRIES {
        return Err("XLSX ZIP entry count outside safe limits".into());
    }
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut workbook_count = 0;
    let mut types_count = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name();
        if name.is_empty() || name.starts_with('/') || name.contains('\\')
            || name.split('/').any(|s| s == ".." || s == ".")
            || !seen.insert(name.to_owned()) {
            return Err("Unsafe, duplicated or ambiguous XLSX ZIP member".into());
        }
        if name.starts_with("_xmlsignatures/") || name.contains("/_xmlsignatures/") {
            return Err("V39 refuses to modify a previously signed XLSX".into());
        }
        if !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            return Err("Unsupported XLSX ZIP compression".into());
        }
        total = total.checked_add(entry.size()).ok_or("XLSX size overflow")?;
        if total > MAX_EXPANDED_BYTES {
            return Err("XLSX expanded size exceeds V39 safety bound".into());
        }
        if name == "xl/workbook.xml" { workbook_count += 1; }
        if name == "[Content_Types].xml" { types_count += 1; }
    }
    if workbook_count != 1 || types_count != 1 {
        return Err("Missing workbook.xml or OPC [Content_Types].xml".into());
    }
    let mut workbook = String::new();
    archive.by_name("xl/workbook.xml").map_err(|e| e.to_string())?
        .read_to_string(&mut workbook).map_err(|e| format!("Workbook is not UTF-8 XML: {e}"))?;
    let prepared = manual_calc_xml(&workbook)?.into_bytes();
    let file = OpenOptions::new().write(true).create_new(true).open(output)
        .map_err(|e| format!("Cannot create new unsigned staging XLSX: {e}"))?;
    let result = (|| {
        let mut writer = ZipWriter::new(file);
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let options = SimpleFileOptions::default().compression_method(entry.compression());
            if entry.is_dir() {
                writer.add_directory(entry.name(), options).map_err(|e| e.to_string())?;
            } else {
                writer.start_file(entry.name(), options).map_err(|e| e.to_string())?;
                if entry.name() == "xl/workbook.xml" {
                    writer.write_all(&prepared).map_err(|e| e.to_string())?;
                } else {
                    std::io::copy(&mut entry, &mut writer).map_err(|e| e.to_string())?;
                }
            }
        }
        writer.finish().map_err(|e| e.to_string())?.sync_all().map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(output); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn preserves_formula_and_other_workbook_settings() {
        let original = format!(r#"<?xml version="1.0"?><workbook xmlns="{NS}"><sheets/><calcPr calcId="0" calcMode="auto" iterate="1" fullCalcOnLoad="1" forceFullCalc="1"/></workbook>"#);
        let result = manual_calc_xml(&original).unwrap();
        assert!(result.contains("calcId=\"0\""));
        assert!(result.contains("iterate=\"1\""));
        assert!(result.contains("calcMode=\"manual\""));
        assert!(result.contains("fullCalcOnLoad=\"0\" forceFullCalc=\"0\""));
        assert!(!result.contains("calcMode=\"auto\""));
        assert_eq!(original.matches("calcPr").count(), 1);
    }
    #[test]
    fn rejects_missing_calc_pr_and_wrong_root() {
        assert!(manual_calc_xml(&format!("<workbook xmlns=\"{NS}\"><sheets/></workbook>")).is_err());
        assert!(manual_calc_xml("<workbook><calcPr/></workbook>").is_err());
    }
    #[test]
    fn prepares_new_zip_and_does_not_touch_input_or_formulas() {
        let dir = std::env::temp_dir().join(format!("craft-v39-{}-{:?}",std::process::id(),std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("unsigned.xlsx");
        let output = dir.join("prepared.xlsx");
        let source = File::create(&input).unwrap();
        let mut writer = ZipWriter::new(source);
        writer.start_file("[Content_Types].xml",SimpleFileOptions::default()).unwrap();
        writer.write_all(b"<Types/>").unwrap();
        writer.start_file("xl/workbook.xml",SimpleFileOptions::default()).unwrap();
        writer.write_all(format!("<workbook xmlns=\"{NS}\"><sheets/><calcPr calcId=\"0\"/></workbook>").as_bytes()).unwrap();
        writer.start_file("xl/worksheets/sheet1.xml",SimpleFileOptions::default()).unwrap();
        let formula = b"<worksheet><f>B1*2</f><v>10</v></worksheet>";
        writer.write_all(formula).unwrap();
        writer.finish().unwrap();
        let before = std::fs::read(&input).unwrap();
        prepare_xlsx_for_excel_signing(&input,&output).unwrap();
        assert_eq!(before,std::fs::read(&input).unwrap());
        let mut out = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        let mut workbook=String::new();
        out.by_name("xl/workbook.xml").unwrap().read_to_string(&mut workbook).unwrap();
        assert!(workbook.contains("calcMode=\"manual\""));
        let mut sheet=Vec::new();
        out.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_end(&mut sheet).unwrap();
        assert_eq!(sheet,formula);
        drop(out);
        assert!(prepare_xlsx_for_excel_signing(&input,&output).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_xlsx_fails_without_creating_output() {
        let dir=std::env::temp_dir();
        let input=dir.join(format!("v39-invalid-{}.xlsx",std::process::id()));
        let output=dir.join(format!("v39-invalid-output-{}.xlsx",std::process::id()));
        std::fs::write(&input,b"not ZIP").unwrap();
        assert!(prepare_xlsx_for_excel_signing(&input,&output).is_err());
        assert!(!output.exists());
        let _=std::fs::remove_file(input);
        let _: Cursor<Vec<u8>>=Cursor::new(Vec::new());
    }
}
