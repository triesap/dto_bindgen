#![forbid(unsafe_code)]

mod backend;
mod diagnostics;
mod source;

pub use backend::{BackendId, ParseBackendIdError};
pub use diagnostics::{Diagnostic, DiagnosticCode, ParseDiagnosticCodeError, Severity};
pub use source::{SourceFile, SourcePosition, SourceSpan};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_package_version() {
        assert_eq!(crate::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
