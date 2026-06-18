use core::fmt;
use core::str::FromStr;

use crate::{BackendId, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    pub const fn blocks_export(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticCode {
    number: u16,
}

impl DiagnosticCode {
    pub const fn new(number: u16) -> Self {
        Self { number }
    }

    pub const fn number(self) -> u16 {
        self.number
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DTO{:04}", self.number)
    }
}

impl FromStr for DiagnosticCode {
    type Err = ParseDiagnosticCodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = value
            .strip_prefix("DTO")
            .ok_or(ParseDiagnosticCodeError)?
            .parse::<u16>()
            .map_err(|_| ParseDiagnosticCodeError)?;

        Ok(Self::new(number))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseDiagnosticCodeError;

impl fmt::Display for ParseDiagnosticCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("diagnostic codes must use the DTO0000 format")
    }
}

impl std::error::Error for ParseDiagnosticCodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub help: Option<String>,
    pub source: Option<SourceSpan>,
    pub type_name: Option<String>,
    pub field_name: Option<String>,
    pub variant_name: Option<String>,
    pub backend: Option<BackendId>,
}

impl Diagnostic {
    pub fn new(code: DiagnosticCode, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            help: None,
            source: None,
            type_name: None,
            field_name: None,
            variant_name: None,
            backend: None,
        }
    }

    pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    pub fn note(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Note, message)
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_source(mut self, source: SourceSpan) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_type(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = Some(type_name.into());
        self
    }

    pub fn with_field(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = Some(field_name.into());
        self
    }

    pub fn with_variant(mut self, variant_name: impl Into<String>) -> Self {
        self.variant_name = Some(variant_name.into());
        self
    }

    pub fn with_backend(mut self, backend: BackendId) -> Self {
        self.backend = Some(backend);
        self
    }

    pub const fn blocks_export(&self) -> bool {
        self.severity.blocks_export()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_stable_codes() {
        assert_eq!(DiagnosticCode::new(303).to_string(), "DTO0303");
    }

    #[test]
    fn parses_stable_codes() {
        assert_eq!("DTO0401".parse::<DiagnosticCode>().unwrap().number(), 401);
        assert!("401".parse::<DiagnosticCode>().is_err());
    }

    #[test]
    fn severity_reports_blocking_behavior() {
        assert!(Severity::Error.blocks_export());
        assert!(!Severity::Warning.blocks_export());
        assert!(!Severity::Note.blocks_export());
    }

    #[test]
    fn diagnostic_carries_context() {
        let diagnostic = Diagnostic::error(
            DiagnosticCode::new(303),
            "unsupported Serde attribute `flatten`",
        )
        .with_help("Use an explicit nested field.")
        .with_type("UserProfile")
        .with_field("metadata")
        .with_backend(BackendId::TypeScript);

        assert!(diagnostic.blocks_export());
        assert_eq!(diagnostic.code.to_string(), "DTO0303");
        assert_eq!(diagnostic.type_name.as_deref(), Some("UserProfile"));
        assert_eq!(diagnostic.field_name.as_deref(), Some("metadata"));
        assert_eq!(diagnostic.backend, Some(BackendId::TypeScript));
    }
}
