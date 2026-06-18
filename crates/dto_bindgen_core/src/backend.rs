use core::fmt;
use core::str::FromStr;

use crate::{Config, Diagnostic, GeneratedFileSet, Registry};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackendId {
    TypeScript,
    Python,
    Custom(String),
}

impl BackendId {
    pub const fn typescript() -> Self {
        Self::TypeScript
    }

    pub const fn python() -> Self {
        Self::Python
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BackendId {
    type Err = ParseBackendIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "typescript" | "ts" => Ok(Self::TypeScript),
            "python" | "py" => Ok(Self::Python),
            "" => Err(ParseBackendIdError),
            other => Ok(Self::Custom(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseBackendIdError;

impl fmt::Display for ParseBackendIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend id cannot be empty")
    }
}

impl std::error::Error for ParseBackendIdError {}

pub trait Backend {
    fn id(&self) -> BackendId;

    fn validate(&self, _registry: &Registry, _config: &Config) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn render(
        &self,
        registry: &Registry,
        config: &Config,
    ) -> Result<GeneratedFileSet, BackendError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    pub backend: BackendId,
    pub diagnostics: Vec<Diagnostic>,
}

impl BackendError {
    pub fn new(backend: BackendId, diagnostics: impl Into<Vec<Diagnostic>>) -> Self {
        Self {
            backend,
            diagnostics: diagnostics.into(),
        }
    }

    pub fn from_diagnostic(backend: BackendId, diagnostic: Diagnostic) -> Self {
        Self::new(backend, vec![diagnostic])
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "backend {} failed with {} diagnostic(s)",
            self.backend,
            self.diagnostics.len()
        )
    }
}

impl std::error::Error for BackendError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, GeneratedFile, GeneratedFileSet, Registry};

    #[test]
    fn parses_builtin_backend_ids() {
        assert_eq!("typescript".parse::<BackendId>(), Ok(BackendId::TypeScript));
        assert_eq!("py".parse::<BackendId>(), Ok(BackendId::Python));
    }

    #[test]
    fn preserves_custom_backend_ids() {
        let backend = "json_schema".parse::<BackendId>().unwrap();
        assert_eq!(backend.to_string(), "json_schema");
    }

    #[test]
    fn rejects_empty_backend_id() {
        assert!("".parse::<BackendId>().is_err());
    }

    struct FakeBackend;

    impl Backend for FakeBackend {
        fn id(&self) -> BackendId {
            BackendId::Custom("fake".to_owned())
        }

        fn render(
            &self,
            _registry: &Registry,
            _config: &Config,
        ) -> Result<GeneratedFileSet, BackendError> {
            let file =
                GeneratedFile::new(self.id(), "fake/user.ts", "export type User = {};\n").unwrap();
            GeneratedFileSet::try_from_files([file]).map_err(|err| {
                BackendError::from_diagnostic(
                    self.id(),
                    crate::Diagnostic::error(crate::DiagnosticCode::new(701), err.to_string()),
                )
            })
        }
    }

    #[test]
    fn backend_trait_validates_and_renders_generated_files() {
        let backend = FakeBackend;
        let registry = Registry::new();
        let config = Config::default();

        assert!(backend.validate(&registry, &config).is_empty());

        let files = backend.render(&registry, &config).unwrap();
        assert_eq!(files.files()[0].relative_path().as_str(), "fake/user.ts");
    }

    #[test]
    fn backend_error_carries_diagnostics() {
        let error = BackendError::from_diagnostic(
            BackendId::Python,
            crate::Diagnostic::error(crate::DiagnosticCode::new(600), "python backend failed"),
        );

        assert_eq!(error.backend, BackendId::Python);
        assert_eq!(error.diagnostics().len(), 1);
        assert_eq!(
            error.to_string(),
            "backend python failed with 1 diagnostic(s)"
        );
    }
}
