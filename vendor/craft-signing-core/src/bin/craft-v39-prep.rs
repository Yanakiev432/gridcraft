//! Private QA helper only. Does not sign, contact token providers or read secrets.
//! Requires an unsigned XLSX and an unused output path.
#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("usage: craft-v39-prep.exe <unsigned-input.xlsx> <unused-output.xlsx>");
        std::process::exit(2);
    }
    let source = PathBuf::from(&args[1]);
    let dest = PathBuf::from(&args[2]);
    match craft_signing_core::office_excel_calc::prepare_xlsx_for_excel_signing(&source, &dest) {
        Ok(()) => println!("V39_UNSIGNED_MANUAL_CALC_PREP=PASS; source preserved"),
        Err(e) => { eprintln!("V39_UNSIGNED_MANUAL_CALC_PREP=FAIL: {e}"); std::process::exit(1) }
    }
}
#[cfg(not(windows))]
fn main() { eprintln!("Only supported on Windows"); std::process::exit(1); }
