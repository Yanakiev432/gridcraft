//! Acceptance-focused unit tests for the constrained, single-formula Excel path.
#[cfg(test)]
mod tests {
    use gridcraft_core::{CellRef,Value};
    use gridcraft_model::{Cell,Formula,Workbook,CalcMode};
    use crate::{read_xlsx,write_xlsx,package::Package};
    use crate::office_auto::native_auto_preflight;

    fn sample(excel_id:Option<u32>) -> Workbook {
        let mut wb=Workbook::new();
        wb.calc.engine_id=excel_id;
        let sheet=wb.sheet_mut(0).unwrap();
        sheet.cells.set(CellRef::parse("B1").unwrap(),Cell::value(Value::Number(5.0)));
        let mut f=Cell::formula(Formula::new("=B1*2"));
        f.value=Value::Number(10.0);
        sheet.cells.set(CellRef::parse("C1").unwrap(),f);
        wb
    }

    #[test]
    fn excel_source_engine_id_and_formula_chain_survive_gridcraft_save() {
        let wb=sample(Some(162913));
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let book=pkg.read_xml("xl/workbook.xml").unwrap().unwrap();
        assert_eq!(book.child("calcPr").unwrap().attr("calcId"),Some("162913"));
        assert_eq!(book.child("calcPr").unwrap().attr("calcMode"),None);
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        assert_eq!(chain.name,"calcChain");
        let c=chain.child("c").unwrap();
        assert_eq!(c.attr("r"),Some("C1"));
        assert_eq!(c.attr("i"),Some("1"));
        let (reopened, warnings)=read_xlsx(&bytes).unwrap();
        assert!(warnings.warnings.is_empty(),"{warnings:?}");
        assert_eq!(reopened.calc.engine_id,Some(162913));
        assert_eq!(reopened.sheet(0).unwrap().cell(CellRef::parse("C1").unwrap()).unwrap().value,Value::Number(10.0));
    }

    #[test]
    fn brand_new_gridcraft_book_must_not_forge_excel_calc_id() {
        let wb=sample(None);
        let bytes=write_xlsx(&wb).unwrap();
        assert!(native_auto_preflight(&bytes).is_err());
        let mut pkg=Package::open(&bytes).unwrap();
        let book=pkg.read_xml("xl/workbook.xml").unwrap().unwrap();
        assert_eq!(book.child("calcPr").unwrap().attr("calcId"),None);
        assert!(!pkg.has("xl/calcChain.xml"));
    }

    #[test]
    fn formula_dependency_graph_not_overclaimed() {
        let mut wb=sample(Some(162913));
        let mut f=Cell::formula(Formula::new("=C1*3"));
        f.value=Value::Number(30.0);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),f);
        let bytes=write_xlsx(&wb).unwrap();
        assert!(native_auto_preflight(&bytes).is_err());
        assert!(!Package::open(&bytes).unwrap().has("xl/calcChain.xml"));
    }

    #[test]
    fn imported_excel_reference_can_be_read_saved_and_checked_for_native_auto() {
        let Ok(path)=std::env::var("CRAFT_V44_EXCEL_AUTHORED_SOURCE") else {
            return; // optional private QA fixture, never checked into the repository
        };
        let bytes=std::fs::read(path).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let (book,_report)=read_xlsx(&bytes).unwrap();
        assert_eq!(book.calc.engine_id,Some(162913));
        assert_eq!(book.calc.mode,CalcMode::Automatic);
        let rewritten=write_xlsx(&book).unwrap();
        native_auto_preflight(&rewritten).unwrap();
        let (again,_)=read_xlsx(&rewritten).unwrap();
        assert_eq!(again.calc.engine_id,Some(162913));
        assert_eq!(again.sheet(0).unwrap().cell(CellRef::parse("C1").unwrap()).unwrap().value,Value::Number(10.0));
    }
    #[test]
    fn manual_or_recalc_flags_cannot_be_misrepresented_as_excel_auto() {
        let mut wb=sample(Some(162913));
        wb.calc.mode=CalcMode::Manual;
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
        let mut wb=sample(Some(162913));
        let f=Cell::formula(Formula::new("=B1*2"));
        // No cached result: Excel needs to recalculate, so the signed Automatic
        // promise must not be accepted by the conservative V44 gate.
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("C1").unwrap(),f);
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
    }
}
