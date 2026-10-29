//! Conservative dependency-validated Excel calculation chain for native XMLDSig.
//!
//! Only a bounded, nonvolatile numeric subset is accepted. Any
//! unrecognized expression, unsupported reference, circular graph, missing
//! cached result, or mismatched cache FAILS CLOSED. A calcChain is never
//! invented for a workbook that claims to need calculation.
use std::collections::{BTreeMap, BTreeSet};

use gridcraft_core::{CellRef, Value};
use gridcraft_formula::{BinOp, Expr, RefKind, SheetSel, UnOp};
use gridcraft_model::{CalcMode, Workbook};

pub(crate) type FormulaKey = (usize, CellRef);

#[derive(Debug)]
pub(crate) struct ValidatedChain {
    pub ordered: Vec<FormulaKey>,
    dependencies: BTreeMap<FormulaKey, BTreeSet<FormulaKey>>,
}

impl ValidatedChain {
    /// Excel may order independent formulas differently. Accept any complete
    /// dependency-respecting chain, rather than requiring identical XML order.
    pub(crate) fn accepts_order(&self, seen: &[FormulaKey]) -> bool {
        if self.ordered.len() != seen.len() {
            return false;
        }
        let mut locations = BTreeMap::new();
        for (position, key) in seen.iter().enumerate() {
            if locations.insert(*key, position).is_some() {
                return false; // duplicate cell
            }
        }
        if locations.len() != self.dependencies.len() {
            return false;
        }
        self.dependencies.iter().all(|(key, deps)| {
            let Some(&position) = locations.get(key) else { return false; };
            deps.iter().all(|dep| locations.get(dep).is_some_and(|&p| p < position))
        })
    }
}

const MAX_FORMULAS: usize = 256;
const MAX_DEPTH: usize = 64;

/// Returns no plan for unsupported workbooks. Only the positive result may
/// be used to emit a trustworthy calcChain for native Office Automatic mode.
pub(crate) fn validated_chain(wb: &Workbook) -> Option<ValidatedChain> {
    if wb.calc.engine_id.filter(|&id| id > 0).is_none()
        || wb.calc.mode != CalcMode::Automatic
        || wb.calc.iterative
    {
        return None;
    }

    let mut formulas = BTreeMap::new();
    for (sheet_index, sheet) in wb.sheets.iter().enumerate() {
        if !sheet.spill_ranges.is_empty() {
            return None;
        }
        for (coordinate, cell) in sheet.cells.iter() {
            let Some(formula) = cell.formula.as_deref() else { continue; };
            if formula.array.is_some() || !matches!(&cell.value, Value::Number(n) if n.is_finite()) {
                return None;
            }
            let expression = formula.expr()?;
            formulas.insert((sheet_index, coordinate), expression);
            if formulas.len() > MAX_FORMULAS {
                return None;
            }
        }
    }
    if formulas.is_empty() {
        return None;
    }

    let mut deps = BTreeMap::new();
    for (&key, expression) in &formulas {
        let mut needed = BTreeSet::new();
        let result = evaluate(expression, wb, key.0, 0, &mut needed)?;
        let cache = match wb.sheet(key.0)?.cell(key.1)?.value {
            Value::Number(n) if n.is_finite() => n,
            _ => return None,
        };
        // Cached values must agree with the supported arithmetic calculation,
        // not merely be present. Relative tolerance covers floating rounding.
        if (result - cache).abs() > 1e-9_f64.max(1e-10 * result.abs().max(cache.abs())) {
            return None;
        }
        for &ref_key in &needed {
            if !formulas.contains_key(&ref_key) {
                return None; // dependency marked as formula but not in graph
            }
        }
        deps.insert(key, needed);
    }

    // DFS topological ordering and cycle detection.
    let mut ordered = Vec::with_capacity(formulas.len());
    let mut completed = BTreeSet::new();
    let mut active = BTreeSet::new();
    fn visit(
        key: FormulaKey,
        deps: &BTreeMap<FormulaKey, BTreeSet<FormulaKey>>,
        completed: &mut BTreeSet<FormulaKey>,
        active: &mut BTreeSet<FormulaKey>,
        ordered: &mut Vec<FormulaKey>,
    ) -> Option<()> {
        if completed.contains(&key) { return Some(()); }
        if !active.insert(key) { return None; } // circular formula reference
        for dep in deps.get(&key)? {
            visit(*dep, deps, completed, active, ordered)?;
        }
        active.remove(&key);
        completed.insert(key);
        ordered.push(key);
        Some(())
    }
    for &key in formulas.keys() {
        visit(key, &deps, &mut completed, &mut active, &mut ordered)?;
    }
    Some(ValidatedChain { ordered, dependencies: deps })
}

const MAX_RANGE_CELLS: u64 = 1024;

/// Resolve a local or single explicitly named sheet reference. No 3D ranges,
/// external books, implicit sheet fallbacks or ambiguous sheet names.
fn resolve_sheet(sel: &SheetSel, wb: &Workbook, current: usize) -> Option<usize> {
    match sel {
        SheetSel::Current => Some(current),
        SheetSel::Named(name) => wb.sheet_index(name),
        SheetSel::Span(_, _) => None,
    }
}

fn value_from_cell(
    wb: &Workbook, sheet: usize, pos: CellRef,
    deps: &mut BTreeSet<FormulaKey>,
) -> Option<f64> {
    let cell = wb.sheet(sheet)?.cell(pos)?;
    if cell.formula.is_some() {
        deps.insert((sheet, pos));
    }
    match cell.value {
        Value::Number(n) if n.is_finite() => Some(n),
        // Excel SUM() ignores blank or text in a cell reference, but V46
        // deliberately refuses those cases unless we prove exact semantics.
        _ => None,
    }
}

fn supported_range(
    r: &gridcraft_formula::Reference,
    wb: &Workbook,
    current: usize,
    deps: &mut BTreeSet<FormulaKey>,
) -> Option<Vec<f64>> {
    let sheet = resolve_sheet(&r.sheet, wb, current)?;
    let RefKind::Range(a, b) = &r.kind else { return None; };
    let rows = u64::from(a.row.max(b.row) - a.row.min(b.row)) + 1;
    let cols = u64::from(a.col.max(b.col) - a.col.min(b.col)) + 1;
    if rows.checked_mul(cols)? > MAX_RANGE_CELLS {
        return None;
    }
    let mut values = Vec::with_capacity(rows as usize * cols as usize);
    for row in a.row.min(b.row)..=a.row.max(b.row) {
        for col in a.col.min(b.col)..=a.col.max(b.col) {
            values.push(value_from_cell(wb, sheet, CellRef::new(row, col), deps)?);
        }
    }
    Some(values)
}

fn evaluate_aggregate(
    name: &str, args: &[Expr], wb: &Workbook, current: usize, depth: usize,
    deps: &mut BTreeSet<FormulaKey>,
) -> Option<f64> {
    if args.is_empty() || args.len() > 64 { return None; }
    if !matches!(name, "SUM" | "AVERAGE" | "MIN" | "MAX" | "COUNT") {
        return None;
    }
    let mut values = Vec::new();
    for expr in args {
        match expr {
            Expr::Ref(r) if matches!(&r.kind, RefKind::Range(..)) =>
                values.extend(supported_range(r, wb, current, deps)?),
            _ => values.push(evaluate(expr, wb, current, depth + 1, deps)?),
        }
        if values.len() > MAX_RANGE_CELLS as usize { return None; }
    }
    if values.is_empty() { return None; }
    let n = match name {
        "SUM" => values.iter().copied().sum(),
        "AVERAGE" => values.iter().copied().sum::<f64>() / values.len() as f64,
        "COUNT" => values.len() as f64,
        "MIN" => values.iter().copied().reduce(f64::min)?,
        "MAX" => values.iter().copied().reduce(f64::max)?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn evaluate(
    expr: &Expr, wb: &Workbook, sheet: usize, depth: usize,
    deps: &mut BTreeSet<FormulaKey>,
) -> Option<f64> {
    if depth >= MAX_DEPTH {
        return None;
    }
    let nested = depth + 1;
    let n = match expr {
        Expr::Number(n) => *n,
        Expr::Paren(inner) => evaluate(inner, wb, sheet, nested, deps)?,
        Expr::Unary(UnOp::Neg, inner) => -evaluate(inner, wb, sheet, nested, deps)?,
        Expr::Unary(UnOp::Plus, inner) => evaluate(inner, wb, sheet, nested, deps)?,
        Expr::Binary(op, lhs, rhs) => {
            let a = evaluate(lhs, wb, sheet, nested, deps)?;
            let b = evaluate(rhs, wb, sheet, nested, deps)?;
            match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div if b != 0.0 => a / b,
                _ => return None,
            }
        }
        Expr::Ref(r) => {
            let RefKind::Cell(anchor) = &r.kind else { return None; };
            let target = resolve_sheet(&r.sheet, wb, sheet)?;
            value_from_cell(wb, target, anchor.cell(), deps)?
        }
        Expr::Call(name, args) =>
            evaluate_aggregate(name, args, wb, sheet, depth, deps)?,
        // Dynamic array, external, structured, name references, implicit
        // intersection, volatile functions and non-arithmetic operators are
        // never assumed to be compatible with a native Excel signature.
        _ => return None,
    };
    n.is_finite().then_some(n)
}
