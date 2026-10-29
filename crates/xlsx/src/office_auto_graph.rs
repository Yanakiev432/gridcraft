//! Conservative dependency-validated Excel calculation chain for native XMLDSig.
//!
//! Only a small, nonvolatile scalar-arithmetic subset is accepted. Any
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
            let (SheetSel::Current, RefKind::Cell(anchor)) = (&r.sheet, &r.kind)
                else { return None; };
            let cell_ref = anchor.cell();
            let cell = wb.sheet(sheet)?.cell(cell_ref)?;
            if cell.formula.is_some() {
                deps.insert((sheet, cell_ref));
            }
            match cell.value {
                Value::Number(number) => number,
                _ => return None,
            }
        }
        // Names, structured references, range references, functions (including
        // volatile calls), dynamic arrays, and cross-sheet dependencies are NOT
        // supported by this constrained native Automatic calculation path.
        _ => return None,
    };
    n.is_finite().then_some(n)
}
