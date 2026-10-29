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


    fn scalar(formula: &str, cache: f64) -> Cell {
        let mut c=Cell::formula(Formula::new(formula));
        c.value=Value::Number(cache);
        c
    }

    #[test]
    fn v46_sum_average_min_max_count_and_arithmetic_roundtrip() {
        let mut wb=sample(Some(162913));
        let sheet=wb.sheet_mut(0).unwrap();
        sheet.cells.set(CellRef::parse("D1").unwrap(),Cell::value(Value::Number(20.0)));
        sheet.cells.set(CellRef::parse("E1").unwrap(),scalar("=SUM(B1:D1)",35.0));
        sheet.cells.set(CellRef::parse("F1").unwrap(),scalar("=AVERAGE(B1:D1)",35.0/3.0));
        sheet.cells.set(CellRef::parse("G1").unwrap(),scalar("=MAX(B1:D1)-MIN(B1:D1)",15.0));
        sheet.cells.set(CellRef::parse("H1").unwrap(),scalar("=COUNT(B1:D1)",3.0));
        sheet.cells.set(CellRef::parse("I1").unwrap(),scalar("=SUM(E1,H1,5)",43.0));
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        let loc=chain.kids("c").filter_map(|c|c.attr("r")).collect::<Vec<_>>();
        assert_eq!(loc,vec!["C1","E1","F1","G1","H1","I1"]);
        let (again,report)=read_xlsx(&bytes).unwrap();
        assert!(report.warnings.is_empty(),"{:?}",report.warnings);
        assert_eq!(again.sheet(0).unwrap().cell(CellRef::parse("I1").unwrap()).unwrap().value,Value::Number(43.0));
    }

    #[test]
    fn v46_explicit_cross_sheet_formula_dependency_topological_order() {
        let mut wb=sample(Some(162913));
        wb.sheets.push(std::sync::Arc::new(gridcraft_model::Sheet::new("Budget")));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),scalar("=Budget!C1+5",26.0));
        let other=wb.sheet_mut(1).unwrap();
        other.cells.set(CellRef::parse("B1").unwrap(),Cell::value(Value::Number(7.0)));
        other.cells.set(CellRef::parse("C1").unwrap(),scalar("=B1*3",21.0));
        let bytes=write_xlsx(&wb).unwrap();
        native_auto_preflight(&bytes).unwrap();
        let mut pkg=Package::open(&bytes).unwrap();
        let chain=pkg.read_xml("xl/calcChain.xml").unwrap().unwrap();
        let loc=chain.kids("c").map(|c|(c.attr("r").unwrap(),c.attr("i").unwrap())).collect::<Vec<_>>();
        assert_eq!(loc,vec![("C1","1"),("C1","2"),("D1","1")]);
    }

    #[test]
    fn v46_range_missing_values_or_unsupported_volatile_fail_closed() {
        let mut wb=sample(Some(162913));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),scalar("=SUM(B1:E1)",15.0));
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
        let mut wb=sample(Some(162913));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),scalar("=NOW()",46323.0));
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
        let mut wb=sample(Some(162913));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),scalar("=SUM(B1:B2000)",20.0));
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
    }

    #[test]
    fn v46_cross_sheet_cycle_and_stale_aggregate_are_blocked() {
        let mut wb=sample(Some(162913));
        wb.sheets.push(std::sync::Arc::new(gridcraft_model::Sheet::new("Budget")));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),scalar("=Budget!C1+1",12.0));
        wb.sheet_mut(1).unwrap().cells.set(CellRef::parse("C1").unwrap(),scalar("=Sheet1!D1-1",11.0));
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
        let mut wb=sample(Some(162913));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("D1").unwrap(),Cell::value(Value::Number(10.0)));
        wb.sheet_mut(0).unwrap().cells.set(CellRef::parse("E1").unwrap(),scalar("=SUM(B1:D1)",999.0));
        assert!(native_auto_preflight(&write_xlsx(&wb).unwrap()).is_err());
    }
    /// Opt-in private V46 fixture from a real Excel-authored workbook.
    /// Input is read-only, and output uses create_new (no overwrite).
    #[test]
    #[ignore = "requires CRAFT_V46_EXCEL_SOURCE and CRAFT_V46_QA_OUTPUT"]
    fn v46_generate_excel_authored_aggregates_cross_sheet_unsigned_qa() {
        let source=std::env::var("CRAFT_V46_EXCEL_SOURCE").expect("QA source path");
        let dest=std::env::var("CRAFT_V46_QA_OUTPUT").expect("QA output path");
        let before=std::fs::read(&source).unwrap();
        native_auto_preflight(&before).unwrap();
        let (mut book, _report)=read_xlsx(&before).unwrap();
        assert_eq!(book.calc.engine_id,Some(162913));
        {
            let sheet=book.sheet_mut(0).unwrap();
            sheet.cells.set(CellRef::parse("D1").unwrap(),Cell::value(Value::Number(20.0)));
            sheet.cells.set(CellRef::parse("E1").unwrap(),scalar("=SUM(B1:D1)",35.0));
            sheet.cells.set(CellRef::parse("F1").unwrap(),scalar("=AVERAGE(B1:D1)",35.0/3.0));
            sheet.cells.set(CellRef::parse("G1").unwrap(),scalar("=MAX(B1:D1)-MIN(B1:D1)",15.0));
            sheet.cells.set(CellRef::parse("H1").unwrap(),scalar("=COUNT(B1:D1)",3.0));
            sheet.cells.set(CellRef::parse("I1").unwrap(),scalar("=Budget!C1+E1",56.0));
            sheet.cells.set(CellRef::parse("J1").unwrap(),scalar("=SUM(Budget!B1:C1)",28.0));
            sheet.cells.set(CellRef::parse("K1").unwrap(),scalar("=I1+J1",84.0));
        }
        book.sheets.push(std::sync::Arc::new(gridcraft_model::Sheet::new("Budget")));
        {
            let budget=book.sheet_mut(1).unwrap();
            budget.cells.set(CellRef::parse("B1").unwrap(),Cell::value(Value::Number(7.0)));
            budget.cells.set(CellRef::parse("C1").unwrap(),scalar("=B1*3",21.0));
        }
        let data=write_xlsx(&book).unwrap();
        native_auto_preflight(&data).unwrap();
        let mut out=std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&dest).unwrap();
        std::io::Write::write_all(&mut out,&data).unwrap();
        out.sync_all().unwrap();
        assert_eq!(std::fs::read(&source).unwrap(),before);
        let (again,_)=read_xlsx(&data).unwrap();
        assert_eq!(again.sheets.len(),2);
        assert_eq!(again.sheet(0).unwrap().cell(CellRef::parse("K1").unwrap()).unwrap().value,Value::Number(84.0));
        assert_eq!(again.sheet(1).unwrap().cell(CellRef::parse("C1").unwrap()).unwrap().value,Value::Number(21.0));
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
