//! Shared, fail-closed Office OPC/XMLDSig command wrapper.
//! The signing key stays in the Windows-selected CSP/CNG provider.
//! A valid OPC integrity check is not an eIDAS QES qualification.

#[cfg(target_os = "windows")]
pub(crate) fn execute(cmd: &str, args: &[String], extension: &str) -> Result<String, String> {
    use craft_signing_core::format_contract::{DocumentFamily, FormatBinding, SignatureStandard};
    use craft_signing_core::opc_windows::{sign_office_document_with_policy, verify_office_document};
    use std::io::Read;
    use std::path::Path;

    let family = match extension {
        "docx" => DocumentFamily::WordOoxml,
        "xlsx" => DocumentFamily::SpreadsheetOoxml,
        _ => return Err("unsupported Office document family".into()),
    };
    let binding = FormatBinding::new(family, SignatureStandard::OoxmlOpcXmldsig).map_err(str::to_owned)?;

    let source = args.first().ok_or_else(|| format!("{cmd}: source .{extension} document required"))?;
    let input = Path::new(source);
    if input.extension().and_then(|x| x.to_str()) != Some(extension) {
        return Err(format!("{cmd}: the source must be .{extension}"));
    }
    let mut file = std::fs::File::open(input).map_err(|e| format!("cannot open document: {e}"))?;
    let mut header = [0u8; 4];
    file.read_exact(&mut header).map_err(|e| format!("cannot read OPC package header: {e}"))?;
    binding.preflight_container(&header).map_err(str::to_owned)?;

    match cmd {
        "opc-verify" => {
            if args.len() != 1 {
                return Err("opc-verify syntax: opc-verify <document>".into());
            }
            verify_office_document(input)
        }
        "opc-sign" => {
            let [_, output, flag, thumbprint, remaining @ ..] = args else {
                return Err("opc-sign syntax: opc-sign <document> <new-output> --thumbprint <40-hex> [--restrict-additional-signatures]".into());
            };
            let allow_additional = match remaining {
                [] => true,
                [option] if option == "--allow-additional-signatures" => true,
                [option] if option == "--restrict-additional-signatures" => false,
                _ => return Err("Use --allow-additional-signatures or --restrict-additional-signatures (exactly one)".into()),
            };
            if flag != "--thumbprint" {
                return Err("opc-sign requires explicit --thumbprint".into());
            }
            let output_path = Path::new(output);
            if output_path.extension().and_then(|x| x.to_str()) != Some(extension) {
                return Err(format!("opc-sign: output must be .{extension}"));
            }
            sign_office_document_with_policy(input, output_path, thumbprint, allow_additional)
        }
        _ => Err("unsupported Office OPC command".into()),
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn execute(_: &str, _: &[String], _: &str) -> Result<String, String> {
    Err("Office OPC/XMLDSig signing and validation require Windows; no unsigned fallback".into())
}
