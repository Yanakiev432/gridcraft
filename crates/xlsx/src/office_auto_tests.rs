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
    fn multi_formula_dependency_graph_has_valid_excel_calculation_chain() {
        let mut wb=sample(Some(162913));
        let mut d=Cell::formula(Formula::new("=C1*3"));
        d.value=Value::Number(30.0);
        let mut e=Cell::formula(Formula::new("=D1+4"));
        e.value=Value::Number(34.0);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),d);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("E1").unwrap(),e);
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        let cells=chain.kids("c").map(|c|c.attr("r").unwrap()).collect::<Vec<_>>();
        assert_eq!(cells,vec!["C1","D1","E1"]);
        assert!(chain.kids("c").all(|c|c.attr("i")==Some("1")));
        let (reopened,_)=read_xlsx(&bytes).unwrap();
        assert_eq!(reopened.calc.engine_id,Some(162913));
        assert_eq!(reopened.sheet(0).unwrap().cell(CellRef::parse("E1").unwrap()).unwrap().value,Value::Number(34.0));
    }

    #[test]
    fn independent_branches_and_dependent_aggregation_are_ordered() {
        let mut wb=sample(Some(162913));
        let mut d=Cell::formula(Formula::new("=B1+1"));
        d.value=Value::Number(6.0);
        let mut e=Cell::formula(Formula::new("=C1+D1"));
        e.value=Value::Number(16.0);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),d);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("E1").unwrap(),e);
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        let ordered=chain.kids("c").map(|c|c.attr("r").unwrap()).collect::<Vec<_>>();
        assert_eq!(ordered,vec!["C1","D1","E1"]);
    }

    #[test]
    fn independent_formulas_on_two_worksheets_preserve_excel_sheet_ids() {
        let mut wb=sample(Some(162913));
        wb.sheets.push(std::sync::Arc::new(gridcraft_model::Sheet::new("Budget")));
        let sheet=wb.sheet_mut(1).unwrap();
        sheet.cells.set(CellRef::parse("B1").unwrap(),Cell::value(Value::Number(7.0)));
        let mut f=Cell::formula(Formula::new("=B1*3"));
        f.value=Value::Number(21.0);
        sheet.cells.set(CellRef::parse("C1").unwrap(),f);
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        let cells=chain.kids("c").map(|c|(c.attr("r").unwrap(),c.attr("i").unwrap())).collect::<Vec<_>>();
        assert_eq!(cells,vec![("C1","1"),("C1","2")]);
    }

    #[test]
    fn stale_cached_result_refuses_native_auto_signature() {
        let mut wb=sample(Some(162913));
        let mut invalid=Cell::formula(Formula::new("=C1+1"));
        invalid.value=Value::Number(42.0); // should be 11.0
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),invalid);
        let bytes=write_xlsx(&wb).unwrap();
        assert!(native_auto_preflight(&bytes).is_err());
        assert!(!Package::open(&bytes).unwrap().has("xl/calcChain.xml"));
    }

    #[test]
    fn circular_references_refuse_native_auto_signature() {
        let mut wb=sample(Some(162913));
        let mut c=Cell::formula(Formula::new("=D1+1"));
        c.value=Value::Number(2.0);
        let mut d=Cell::formula(Formula::new("=C1-1"));
        d.value=Value::Number(1.0);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("C1").unwrap(),c);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),d);
        let bytes=write_xlsx(&wb).unwrap();
        assert!(native_auto_preflight(&bytes).is_err());
        assert!(!Package::open(&bytes).unwrap().has("xl/calcChain.xml"));
    }

    #[test]
    fn volatile_and_cross_sheet_formulas_are_not_silently_assumed_safe() {
        let mut wb=sample(Some(162913));
        let mut unsafe_cell=Cell::formula(Formula::new("=RAND()"));
        unsafe_cell.value=Value::Number(0.4);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("C1").unwrap(),unsafe_cell);
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
        let mut wb=sample(Some(162913));
        let mut cross=Cell::formula(Formula::new("=Sheet2!B1*2"));
        cross.value=Value::Number(10.0);
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("C1").unwrap(),cross);
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
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
    /// Synthetic Office acceptance fixture. It is generated from an
    /// Excel-authored UNSIGNED workbook, not from a fabricated Excel calcId.
    /// Explicit environment variables are required to write the fixture.
    #[test]
    #[ignore = "requires CRAFT_V45_EXCEL_SOURCE and CRAFT_V45_QA_OUTPUT"]
    fn v45_generate_excel_authored_multiformula_unsigned_qa() {
        let source=std::env::var("CRAFT_V45_EXCEL_SOURCE").expect("QA input");
        let dest=std::env::var("CRAFT_V45_QA_OUTPUT").expect("QA output");
        let before=std::fs::read(&source).unwrap();
        native_auto_preflight(&before).unwrap();
        let (mut book,_report)=read_xlsx(&before).unwrap();
        assert_eq!(book.calc.engine_id,Some(162913));
        let add = |formula:&str,number:f64| -> Cell {
            let mut c=Cell::formula(Formula::new(formula));
            c.value=Value::Number(number);
            c
        };
        let sheet=book.sheet_mut(0).unwrap();
        sheet.cells.set(CellRef::parse("D1").unwrap(),add("=C1*3",30.0));
        sheet.cells.set(CellRef::parse("E1").unwrap(),add("=D1+4",34.0));
        sheet.cells.set(CellRef::parse("F1").unwrap(),add("=B1-1",4.0));
        sheet.cells.set(CellRef::parse("G1").unwrap(),add("=E1+F1",38.0));
        let bytes=write_xlsx(&book).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut out=std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&dest).unwrap();
        std::io::Write::write_all(&mut out,&bytes).unwrap();
        out.sync_all().unwrap();
        assert_eq!(std::fs::read(&source).unwrap(),before,"Source must be unchanged");
        let (roundtrip,_)=read_xlsx(&bytes).unwrap();
        assert_eq!(roundtrip.sheet(0).unwrap().cell(CellRef::parse("G1").unwrap()).unwrap().value,Value::Number(38.0));
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
