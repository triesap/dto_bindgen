use std::fmt;
use std::path::PathBuf;

use crate::{Config, ConfigError, Diagnostic, Registry, RootDescriptor, build_registry};

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
    Diagnostics(Vec<Diagnostic>),
}

pub fn export_with_roots(
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
            Self::Diagnostics(diagnostics) => {
                write!(f, "export failed with {} diagnostic(s)", diagnostics.len())
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(source) => Some(source),
            Self::Diagnostics(_) => None,
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
            ctx.register_type(RustTypeId::new("sdk", "SimpleDto"), TypeDef::Struct(def))
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
            ctx.register_type(RustTypeId::new("sdk", "LargeIntDto"), TypeDef::Struct(def))
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
    fn exports_validated_registry_without_files_until_backends_exist() {
        let path = temp_config("");
        let report = export_with_roots(
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
        let err = export_with_roots(
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
