//! Shared *visual appearance preferences* for Craft Office signature previews.
//! Based on PdfCraft's Appearance/AppearanceLabels pattern (no PDF object reuse).
//!
//! The preview is not a signature. The Office DOCX/XLSX appearance must be written
//! into an unsigned document before OPC/XMLDSig signing, and then verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficeSignatureAppearance {
    pub name: bool,
    pub date: bool,
    pub reason: bool,
    pub location: bool,
    pub labels: bool,
    pub reason_text: String,
    pub location_text: String,
}

impl Default for OfficeSignatureAppearance {
    fn default() -> Self {
        Self {
            name: true,
            date: true,
            reason: false,
            location: false,
            labels: true,
            reason_text: String::new(),
            location_text: String::new(),
        }
    }
}

impl OfficeSignatureAppearance {
    /// Human-readable preview; never interpreted as provider attestation.
    pub fn details(&self, signer: &str) -> Vec<String> {
        let mut result = Vec::new();
        let signer = signer.trim();
        if self.name && !signer.is_empty() {
            result.push(if self.labels {
                format!("Digitally signed by: {signer}")
            } else {
                signer.to_owned()
            });
        }
        if self.reason {
            let v = self.reason_text.trim();
            if !v.is_empty() {
                result.push(if self.labels {
                    format!("Reason: {v}")
                } else {
                    v.to_owned()
                });
            }
        }
        if self.location {
            let v = self.location_text.trim();
            if !v.is_empty() {
                result.push(if self.labels {
                    format!("Location: {v}")
                } else {
                    v.to_owned()
                });
            }
        }
        if self.date {
            result.push(if self.labels {
                "Date: supplied by signing workflow".into()
            } else {
                "Signing time not yet established".into()
            });
        }
        result
    }

    /// Extract display-friendly CN while retaining full certificate subject
    /// separately in the signing dialog for unambiguous identity selection.
    pub fn subject_display_name(subject: &str) -> &str {
        subject
            .split(',')
            .map(str::trim)
            .find_map(|part| part.strip_prefix("CN=").or_else(|| part.strip_prefix("cn=")))
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(subject)
            .trim()
    }
}

#[cfg(test)]
mod tests {
    use super::OfficeSignatureAppearance;
    #[test]
    fn pdfcraft_defaults_have_name_and_date_only() {
        let a = OfficeSignatureAppearance::default();
        assert!(a.name && a.date && a.labels);
        assert!(!a.reason && !a.location);
        assert_eq!(
            a.details("Test Signer"),
            ["Digitally signed by: Test Signer", "Date: supplied by signing workflow"]
        );
    }

    #[test]
    fn details_hide_empty_or_disabled_fields() {
        let a = OfficeSignatureAppearance {
            reason: true, location: true,
            reason_text: " ".into(),
            location_text: " Sofia ".into(),
            ..Default::default()
        };
        let all = a.details("  Test Signer ");
        assert_eq!(all.len(), 3);
        assert!(all.contains(&"Location: Sofia".to_string()));
        assert!(!all.iter().any(|line| line.contains("Reason:")));
    }

    #[test]
    fn certificate_subject_display_has_fallback() {
        assert_eq!(
            OfficeSignatureAppearance::subject_display_name("CN=Craft Test, O=Craft"),
            "Craft Test"
        );
        assert_eq!(
            OfficeSignatureAppearance::subject_display_name("OU=Test, O=Craft"),
            "OU=Test, O=Craft"
        );
    }

    #[test]
    fn turning_all_fields_off_makes_no_preview_claim() {
        let a = OfficeSignatureAppearance {
            name: false, date: false, reason: false, location: false,
            ..Default::default()
        };
        assert!(a.details("Test Signer").is_empty());
    }
}
