use std::fs;
use std::path::{Path, PathBuf};

use dto_bindgen_backend_python::PythonBackend;
use dto_bindgen_backend_ts::TypeScriptBackend;
use dto_bindgen_core::{
    Backend, Config, ConfigError, Diagnostic, DiagnosticCode, GeneratedFile, GeneratedFileSet,
    GeneratedManifest, OutputWriter,
};
use sha2::{Digest, Sha256};

pub use dto_bindgen_core::{
    DescribeCtx, ExportError, ExportOptions, ExportReport, Registry, RootDescriptor, TypeRef,
    VERSION, build_registry,
};

pub fn export_with_roots(
    options: ExportOptions,
    roots: impl IntoIterator<Item = RootDescriptor>,
) -> Result<ExportReport, ExportError> {
    let (config, config_input) = load_config(&options)?;
    let registry = build_registry(roots);
    let mut diagnostics = registry.validate(&config);
    diagnostics.extend(validate_enabled_backends(&registry, &config));

    if diagnostics.iter().any(Diagnostic::blocks_export) {
        return Err(ExportError::Diagnostics(diagnostics));
    }

    let generated_files = render_enabled_backends(&registry, &config)?;
    let writer_files = strip_output_root(&generated_files, &config)?;
    let manifest = GeneratedManifest::from_file_set(
        VERSION,
        sha256_hex(format!("{registry:#?}").as_bytes()),
        sha256_hex(config_input.as_bytes()),
        &generated_files,
    );
    let writer = OutputWriter::new(output_root(&options.config_path, &config))
        .map_err(ExportError::Output)?;
    let output = if options.check {
        writer.check(&writer_files, &manifest)
    } else {
        writer.write(&writer_files, &manifest)
    }
    .map_err(ExportError::Output)?;

    Ok(ExportReport {
        registry,
        files: output.files,
        diagnostics,
    })
}

fn load_config(options: &ExportOptions) -> Result<(Config, String), ExportError> {
    let input = fs::read_to_string(&options.config_path).map_err(|source| {
        ExportError::Config(ConfigError::Read {
            path: options.config_path.clone(),
            source,
        })
    })?;
    let config = Config::from_toml_str(&input).map_err(ExportError::Config)?;
    Ok((config, input))
}

fn validate_enabled_backends(registry: &Registry, config: &Config) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if config.typescript.enabled {
        diagnostics.extend(TypeScriptBackend::new().validate(registry, config));
    }
    if config.python.enabled {
        diagnostics.extend(PythonBackend::new().validate(registry, config));
    }
    diagnostics
}

fn render_enabled_backends(
    registry: &Registry,
    config: &Config,
) -> Result<GeneratedFileSet, ExportError> {
    let mut files = Vec::new();

    if config.typescript.enabled {
        files.extend(
            TypeScriptBackend::new()
                .render(registry, config)
                .map_err(ExportError::Backend)?
                .into_files(),
        );
    }

    if config.python.enabled {
        files.extend(
            PythonBackend::new()
                .render(registry, config)
                .map_err(ExportError::Backend)?
                .into_files(),
        );
    }

    GeneratedFileSet::try_from_files(files).map_err(ExportError::GeneratedFiles)
}

fn strip_output_root(
    file_set: &GeneratedFileSet,
    config: &Config,
) -> Result<GeneratedFileSet, ExportError> {
    let output_root = normalized_output_root(&config.export.out_dir)?;
    let mut files = Vec::new();

    for file in file_set.files() {
        let stripped_path = strip_relative_prefix(file.relative_path().as_str(), &output_root)
            .ok_or_else(|| {
                ExportError::Diagnostics(vec![
                    Diagnostic::error(
                        DiagnosticCode::new(701),
                        format!(
                            "generated path `{}` is outside output root `{}`",
                            file.relative_path(),
                            config.export.out_dir
                        ),
                    )
                    .with_backend(file.backend().clone()),
                ])
            })?;
        files.push(
            GeneratedFile::new(file.backend().clone(), stripped_path, file.contents()).map_err(
                |err| {
                    ExportError::Diagnostics(vec![
                        Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                            .with_backend(file.backend().clone()),
                    ])
                },
            )?,
        );
    }

    GeneratedFileSet::try_from_files(files).map_err(ExportError::GeneratedFiles)
}

fn output_root(config_path: &Path, config: &Config) -> PathBuf {
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(&config.export.out_dir)
}

fn normalized_output_root(output_root: &str) -> Result<String, ExportError> {
    dto_bindgen_core::GeneratedRelativePath::new(output_root)
        .map(|path| path.as_str().trim_end_matches('/').to_owned())
        .map_err(|err| {
            ExportError::Diagnostics(vec![Diagnostic::error(
                DiagnosticCode::new(701),
                format!("invalid output root `{output_root}`: {err}"),
            )])
        })
}

fn strip_relative_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
        .and_then(|stripped| stripped.strip_prefix('/'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use dto_bindgen_core::{
        Dto, FieldDef, IdentName, RustTypeId, SourceSpan, StructDef, TargetFieldNames, TypeDef,
        WireFieldNames,
    };

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct SimpleDto;

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

    fn span() -> SourceSpan {
        SourceSpan::new("src/dto.rs", 1, 1)
    }

    fn temp_project() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "dto_bindgen_facade_export_test_{}_{}",
            std::process::id(),
            counter
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_config(root: &Path) -> PathBuf {
        let path = root.join("dto_bindgen.toml");
        fs::write(
            &path,
            r#"
[export]
out_dir = "generated"

[typescript]
enabled = true
out_dir = "generated/ts"

[python]
enabled = false
"#,
        )
        .unwrap();
        path
    }

    #[test]
    fn export_types_macro_writes_enabled_backend_files() {
        let root = temp_project();
        let config_path = write_config(&root);

        let report =
            crate::export_types!(config = config_path.clone(), roots = [SimpleDto]).unwrap();

        assert_eq!(report.registry.roots.len(), 1);
        assert!(
            root.join("generated/ts/simple_dto.ts").is_file(),
            "expected TypeScript DTO file"
        );
        assert!(
            root.join("generated/ts/index.ts").is_file(),
            "expected TypeScript index file"
        );
        assert!(
            root.join("generated/dto_bindgen.generated.json").is_file(),
            "expected generated manifest"
        );
        assert_eq!(report.files.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn check_mode_passes_after_export_writes_files() {
        let root = temp_project();
        let config_path = write_config(&root);

        export_with_roots(
            ExportOptions::new(config_path.clone()),
            [RootDescriptor::new::<SimpleDto>()],
        )
        .unwrap();
        let report = export_with_roots(
            ExportOptions::new(config_path.clone()).check(true),
            [RootDescriptor::new::<SimpleDto>()],
        )
        .unwrap();

        assert_eq!(report.files.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }
}
