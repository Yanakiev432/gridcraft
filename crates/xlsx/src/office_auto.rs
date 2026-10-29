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
    if chain.name!="calcChain" || !chain.kids("c").any(|c|c.attr("r").is_some()) {
        return Err(IoError::Format("Empty or malformed Excel calculation chain".into()));
    }
    Ok(())
}
