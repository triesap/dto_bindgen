use std::fmt;
use std::path::PathBuf;

use crate::{
    BackendError, Config, ConfigError, Diagnostic, GeneratedFileSetError, OutputWriterError,
    Registry, RootDescriptor, build_registry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub config_path: PathBuf,
    pub check: bool,
}

impl ExportOptions {
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
            check: false,
        }
    }

    pub const fn check(mut self, check: bool) -> Self {
        self.check = check;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub registry: Registry,
    pub files: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug)]
pub enum ExportError {
    Config(ConfigError),
    CanonicalRegistry(serde_json::Error),
    Diagnostics(Vec<Diagnostic>),
    Backend(BackendError),
    GeneratedFiles(GeneratedFileSetError),
    Output(OutputWriterError),
}

/// Build and validate a registry for explicit roots without rendering or writing files.
///
/// The user-facing export path lives in the `dto_bindgen` facade crate because it
/// wires concrete backends and output writing around these shared core types.
pub fn validate_roots(
    options: ExportOptions,
    roots: impl IntoIterator<Item = RootDescriptor>,
) -> Result<ExportReport, ExportError> {
    let config = Config::from_toml_path(&options.config_path).map_err(ExportError::Config)?;
    let registry = build_registry(roots);
    let diagnostics = registry.validate(&config);

    if diagnostics.iter().any(Diagnostic::blocks_export) {
        return Err(ExportError::Diagnostics(diagnostics));
    }

    Ok(ExportReport {
        registry,
        files: Vec::new(),
        diagnostics,
    })
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(source) => write!(f, "{source}"),
            Self::CanonicalRegistry(source) => {
                write!(f, "failed to serialize canonical registry: {source}")
            }
            Self::Diagnostics(diagnostics) => {
                write!(f, "export failed with {} diagnostic(s)", diagnostics.len())
            }
            Self::Backend(source) => write!(f, "{source}"),
            Self::GeneratedFiles(source) => write!(f, "{source}"),
            Self::Output(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(source) => Some(source),
            Self::CanonicalRegistry(source) => Some(source),
            Self::Diagnostics(_) => None,
            Self::Backend(source) => Some(source),
            Self::GeneratedFiles(source) => Some(source),
            Self::Output(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::{
        DescribeCtx, Dto, FieldDef, IdentName, Primitive, RustTypeId, SourceSpan, StructDef,
        TargetFieldNames, TypeDef, TypeRef, WireFieldNames,
    };

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct SimpleDto;
    struct LargeIntDto;

    impl Dto for SimpleDto {
        fn describe(ctx: &mut DescribeCtx) -> TypeRef {
            let def = StructDef::new("SimpleDto", "SimpleDto", span()).with_field(FieldDef::new(
                IdentName::new("name"),
                WireFieldNames::same("name"),
                TargetFieldNames::new("name", "name"),
                TypeRef::String,
                span(),
            ));
            ctx.register_type(
                RustTypeId::new("sdk", "sdk", "SimpleDto"),
                TypeDef::Struct(def),
            )
        }
    }

    impl Dto for LargeIntDto {
        fn describe(ctx: &mut DescribeCtx) -> TypeRef {
            let def =
                StructDef::new("LargeIntDto", "LargeIntDto", span()).with_field(FieldDef::new(
                    IdentName::new("amount"),
                    WireFieldNames::same("amount"),
                    TargetFieldNames::new("amount", "amount"),
                    TypeRef::Primitive(Primitive::U128),
                    span(),
                ));
            ctx.register_type(
                RustTypeId::new("sdk", "sdk", "LargeIntDto"),
                TypeDef::Struct(def),
            )
        }
    }

    fn span() -> SourceSpan {
        SourceSpan::new("src/dto.rs", 1, 1)
    }

    fn temp_config(contents: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "dto_bindgen_export_test_{}_{}.toml",
            std::process::id(),
            counter
        ));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn validates_roots_without_rendering_files() {
        let path = temp_config("");
        let report = validate_roots(
            ExportOptions::new(path.clone()),
            [RootDescriptor::new::<SimpleDto>()],
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(report.registry.roots.len(), 1);
        assert!(report.files.is_empty());
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn returns_blocking_diagnostics() {
        let path = temp_config("");
        let err = validate_roots(
            ExportOptions::new(path.clone()),
            [RootDescriptor::new::<LargeIntDto>()],
        )
        .unwrap_err();
        std::fs::remove_file(path).unwrap();

        let ExportError::Diagnostics(diagnostics) = err else {
            panic!("expected diagnostics error");
        };
        assert_eq!(diagnostics[0].code, crate::DiagnosticCode::new(401));
    }
}
