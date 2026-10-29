//! Conservative gate for native XML-DSig + Excel Automatic mode (V43B proof).
//! This does NOT guarantee acceptance by every Excel version or recalculation.
//! It accepts only an unsigned, Excel-authored-like workbook with a complete
//! calculation-chain relationship and matching cached calculations.
use crate::{IoError, package::Package};

pub fn native_auto_preflight(bytes: &[u8]) -> Result<(), IoError> {
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(IoError::TooLarge("Native Excel signing input >128 MiB".into()));
    }
    let mut pkg=Package::open(bytes)?;
    if pkg.has("_xmlsignatures/sig1.xml") || pkg.has("_xmlsignatures/origin.sigs") {
        return Err(IoError::Format("Input already signed; save an unsigned copy first".into()));
    }
    let book=pkg.read_xml("xl/workbook.xml")?
        .ok_or_else(||IoError::Format("Missing Excel workbook XML".into()))?;
    let calc=book.child("calcPr").ok_or_else(||IoError::Format(
        "Native Automatic Office signing requires a saved Excel calcPr; export/recalculate in Excel first".into()))?;
    let id=calc.attr_u32("calcId").unwrap_or(0);
    if id==0 {
        return Err(IoError::Format("Workbook has no valid Excel calculation engine ID; cannot guarantee Automatic native signature".into()));
    }
    if !matches!(calc.attr("calcMode"), None | Some("auto")) {
        return Err(IoError::Format("Workbook does not specify Automatic calculation".into()));
    }
    if calc.flag("fullCalcOnLoad",false) || calc.flag("forceFullCalc",false) {
        return Err(IoError::Format("Workbook requests full recalculation on opening; native Office signature may be invalidated".into()));
    }
    let rels=pkg.rels("xl/workbook.xml")?;
    if !rels.iter().any(|r|r.kind.eq_ignore_ascii_case("calcChain") &&
        !r.external && crate::package::normalize(&r.target)=="xl/calcchain.xml") {
        return Err(IoError::Format("Workbook is missing the Excel calculation-chain relationship; native Automatic signing is not certified for this XLSX".into()));
    }
    let chain=pkg.read_xml("xl/calcChain.xml")?.ok_or_else(||IoError::Format(
        "Missing Excel calculation chain; native Automatic signing unavailable".into()))?;
    if chain.name != "calcChain" {
        return Err(IoError::Format("Malformed Excel calculation chain".into()));
    }
    let (workbook, _warnings) = crate::read_xlsx(bytes)?;
    let verified = crate::office_auto_graph::validated_chain(&workbook).ok_or_else(|| {
        IoError::Format("Native Automatic mode supports up to 256 nonvolatile scalar arithmetic formulas with consistent cached results and no cycles; this workbook does not qualify".into())
    })?;
    let mut sheet_ids = std::collections::BTreeMap::new();
    let sheets = book.path(&["sheets"]).ok_or_else(||
        IoError::Format("Workbook sheet metadata missing".into()))?;
    for item in sheets.kids("sheet") {
        if let (Some(id), Some(name)) = (item.attr_u32("sheetId"), item.attr("name")) {
            if let Some(index) = workbook.sheet_index(name) {
                if sheet_ids.insert(id, index).is_some() {
                    return Err(IoError::Format("Duplicate Excel sheetId".into()));
                }
            }
        }
    }
    let mut last_id = None;
    let mut order = Vec::new();
    for entry in chain.kids("c") {
        if let Some(id) = entry.attr_u32("i") { last_id = Some(id); }
        let sheet_index = last_id.and_then(|id|sheet_ids.get(&id).copied())
            .ok_or_else(||IoError::Format("Calculation chain refers to unknown sheet".into()))?;
        let cell = entry.attr("r").and_then(gridcraft_core::CellRef::parse)
            .ok_or_else(||IoError::Format("Calculation chain contains invalid cell address".into()))?;
        order.push((sheet_index,cell));
        if order.len()>256 {
            return Err(IoError::Format("Calculation chain has too many entries".into()));
        }
    }
    if !verified.accepts_order(&order) {
        return Err(IoError::Format("Excel calculation chain does not cover all formulas in dependency order, or has duplicates".into()));
    }
    Ok(())
}
