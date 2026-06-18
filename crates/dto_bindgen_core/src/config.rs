use std::fmt;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub export: ExportConfig,
    pub numeric: NumericConfig,
    pub typescript: TypeScriptConfig,
    pub python: PythonConfig,
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        toml::from_str(input).map_err(ConfigError::Toml)
    }

    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let input = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_toml_str(&input)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportConfig {
    pub out_dir: String,
    pub emit_docs: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            out_dir: "generated".to_owned(),
            emit_docs: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NumericConfig {
    pub large_int_policy: LargeIntPolicy,
}

impl Default for NumericConfig {
    fn default() -> Self {
        Self {
            large_int_policy: LargeIntPolicy::RequireExplicit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LargeIntPolicy {
    RequireExplicit,
    JsonString,
    JsonNumberUnsafe,
    NonJsonBigint,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeScriptConfig {
    pub enabled: bool,
    pub out_dir: String,
    pub emit: TsEmit,
    pub module_resolution: ModuleResolution,
    pub import_extension: ImportExtension,
    pub type_only_imports: bool,
    pub strict_null_checks_required: bool,
    pub style: TypeScriptStyle,
}

impl Default for TypeScriptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            out_dir: "generated/ts".to_owned(),
            emit: TsEmit::Ts,
            module_resolution: ModuleResolution::Bundler,
            import_extension: ImportExtension::None,
            type_only_imports: true,
            strict_null_checks_required: true,
            style: TypeScriptStyle::DtoBindgen,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TsEmit {
    #[serde(rename = "ts")]
    Ts,
    #[serde(rename = "d.ts")]
    Dts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleResolution {
    Bundler,
    Node16,
    Nodenext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportExtension {
    None,
    Js,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeScriptStyle {
    DtoBindgen,
    TsRsCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PythonConfig {
    pub enabled: bool,
    pub out_dir: String,
    pub package: String,
    pub mode: PythonMode,
    pub python_version: String,
    pub frozen: bool,
    pub slots: bool,
    pub kw_only: bool,
    pub emit_from_dict: bool,
    pub emit_to_dict: bool,
    pub emit_py_typed: bool,
    pub unknown_fields: UnknownFieldsPolicy,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            out_dir: "generated/python/my_sdk_dto".to_owned(),
            package: "my_sdk_dto".to_owned(),
            mode: PythonMode::Dataclass,
            python_version: "3.11".to_owned(),
            frozen: true,
            slots: true,
            kw_only: true,
            emit_from_dict: true,
            emit_to_dict: true,
            emit_py_typed: true,
            unknown_fields: UnknownFieldsPolicy::Ignore,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonMode {
    Dataclass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownFieldsPolicy {
    Ignore,
    Deny,
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Toml(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            Self::Toml(source) => write!(f, "failed to parse config: {source}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Toml(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn defaults_match_handoff_example() {
        let config = Config::default();
        assert_eq!(config.export.out_dir, "generated");
        assert_eq!(
            config.numeric.large_int_policy,
            LargeIntPolicy::RequireExplicit
        );
        assert_eq!(config.typescript.emit, TsEmit::Ts);
        assert_eq!(
            config.typescript.module_resolution,
            ModuleResolution::Bundler
        );
        assert_eq!(config.python.mode, PythonMode::Dataclass);
        assert_eq!(config.python.unknown_fields, UnknownFieldsPolicy::Ignore);
    }

    #[test]
    fn parses_checked_in_example() {
        let input = include_str!(
            "../../../docs/handoff/dto_bindgen_handoff/repo/DTO_BINDGEN_TOML_EXAMPLE.toml"
        );
        let config = Config::from_toml_str(input).unwrap();

        assert!(config.typescript.enabled);
        assert_eq!(config.typescript.out_dir, "generated/ts");
        assert_eq!(config.python.package, "my_sdk_dto");
        assert!(config.python.emit_py_typed);
    }

    #[test]
    fn uses_defaults_for_missing_sections() {
        let config = Config::from_toml_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = Config::from_toml_str("[typescript]\nmagic = true\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn loads_config_from_path() {
        let path = std::env::temp_dir().join(format!(
            "dto_bindgen_config_test_{}.toml",
            std::process::id()
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "[python]\npackage = \"sdk_dto\"").unwrap();
        drop(file);

        let config = Config::from_toml_path(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(config.python.package, "sdk_dto");
    }
}
