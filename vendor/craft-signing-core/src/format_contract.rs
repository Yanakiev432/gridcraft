//! Host-neutral format routing shared by PdfCraft, WordCraft and GridCraft.
//! Does not equate a generic CMS signature to a valid OOXML package signature.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFamily {
    Pdf,
    WordOoxml,
    SpreadsheetOoxml,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureStandard {
    Pades,
    OoxmlOpcXmldsig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatBinding {
    pub document: DocumentFamily,
    pub signature: SignatureStandard,
}

impl FormatBinding {
    pub fn new(
        document: DocumentFamily,
        signature: SignatureStandard,
    ) -> Result<Self, &'static str> {
        let valid = matches!(
            (document, signature),
            (DocumentFamily::Pdf, SignatureStandard::Pades)
                | (
                    DocumentFamily::WordOoxml,
                    SignatureStandard::OoxmlOpcXmldsig
                )
                | (
                    DocumentFamily::SpreadsheetOoxml,
                    SignatureStandard::OoxmlOpcXmldsig
                )
        );
        if !valid {
            return Err("signature standard is incompatible with this document format");
        }
        Ok(Self {
            document,
            signature,
        })
    }

    pub fn extension(&self) -> &'static str {
        match self.document {
            DocumentFamily::Pdf => "pdf",
            DocumentFamily::WordOoxml => "docx",
            DocumentFamily::SpreadsheetOoxml => "xlsx",
        }
    }

    /// Reject an obviously wrong container before calling an adapter.
    /// ZIP magic alone is insufficient to establish an OOXML package or DOCX/XLSX type.
    /// A real OOXML signer must inspect [Content_Types].xml, package relationships,
    /// canonical XML and the OPC signature parts.
    pub fn preflight_container(&self, bytes: &[u8]) -> Result<(), &'static str> {
        let valid = match self.document {
            DocumentFamily::Pdf => bytes.starts_with(b"%PDF-"),
            DocumentFamily::WordOoxml | DocumentFamily::SpreadsheetOoxml => {
                bytes.starts_with(b"PK\x03\x04")
            }
        };
        if !valid {
            return Err("document container magic does not match selected document family");
        }
        Ok(())
    }

    pub fn requires_opc_signature_parts(&self) -> bool {
        self.signature == SignatureStandard::OoxmlOpcXmldsig
    }
}

/// This is *not* a QES declaration. A document format profile is separate from
/// certificate qualification, trusted timestamping, revocation and national trust lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PadesBaseline {
    B,
    T,
    Lt,
    Lta,
}

impl PadesBaseline {
    pub fn independent_dss_name(self) -> &'static str {
        match self {
            Self::B => "PAdES-BASELINE-B",
            Self::T => "PAdES-BASELINE-T",
            Self::Lt => "PAdES-BASELINE-LT",
            Self::Lta => "PAdES-BASELINE-LTA",
        }
    }

    pub fn from_independent_dss_name(label: &str) -> Option<Self> {
        match label {
            "PAdES-BASELINE-B" => Some(Self::B),
            "PAdES-BASELINE-T" => Some(Self::T),
            "PAdES-BASELINE-LT" => Some(Self::Lt),
            "PAdES-BASELINE-LTA" => Some(Self::Lta),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cross_format_signatures() {
        assert!(
            FormatBinding::new(DocumentFamily::Pdf, SignatureStandard::OoxmlOpcXmldsig).is_err()
        );
        assert!(FormatBinding::new(DocumentFamily::WordOoxml, SignatureStandard::Pades).is_err());
        assert!(
            FormatBinding::new(DocumentFamily::SpreadsheetOoxml, SignatureStandard::Pades).is_err()
        );
    }

    #[test]
    fn accepts_three_apps_but_not_zip_as_document_proof() {
        let pdf = FormatBinding::new(DocumentFamily::Pdf, SignatureStandard::Pades).unwrap();
        let word = FormatBinding::new(
            DocumentFamily::WordOoxml,
            SignatureStandard::OoxmlOpcXmldsig,
        )
        .unwrap();
        let cell = FormatBinding::new(
            DocumentFamily::SpreadsheetOoxml,
            SignatureStandard::OoxmlOpcXmldsig,
        )
        .unwrap();
        assert_eq!(pdf.extension(), "pdf");
        assert_eq!(word.extension(), "docx");
        assert_eq!(cell.extension(), "xlsx");
        assert!(pdf.preflight_container(b"%PDF-1.7\n").is_ok());
        assert!(word.preflight_container(b"PK\x03\x04mock").is_ok());
        assert!(cell.preflight_container(b"PK\x03\x04mock").is_ok());
        assert!(pdf.preflight_container(b"PK\x03\x04").is_err());
        assert!(word.preflight_container(b"%PDF-1.7").is_err());
        assert!(word.requires_opc_signature_parts());
        assert!(!pdf.requires_opc_signature_parts());
    }

    #[test]
    fn profile_levels_are_monotonic_but_not_qualification() {
        assert!(PadesBaseline::B < PadesBaseline::T);
        assert!(PadesBaseline::T < PadesBaseline::Lt);
        assert!(PadesBaseline::Lt < PadesBaseline::Lta);
        assert_eq!(
            PadesBaseline::from_independent_dss_name("PAdES-BASELINE-LTA"),
            Some(PadesBaseline::Lta)
        );
        assert_eq!(
            PadesBaseline::from_independent_dss_name("PAdES-BASELINE-Z"),
            None
        );
    }
}
