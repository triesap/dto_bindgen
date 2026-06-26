use std::fmt;
use std::path::Path;

use serde::Deserialize;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub export: ExportConfig,
    pub numeric: NumericConfig,
    pub typescript: TypeScriptConfig,
    pub python: PythonConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            export: ExportConfig::default(),
            numeric: NumericConfig::default(),
            typescript: TypeScriptConfig::default(),
            python: PythonConfig::default(),
        }
    }
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(input).map_err(ConfigError::Toml)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let input = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_toml_str(&input)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: CONFIG_SCHEMA_VERSION,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportConfig {
    pub out_dir: String,
    pub emit_docs: bool,
    pub wire_format: WireFormat,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            out_dir: "generated".to_owned(),
            emit_docs: false,
            wire_format: WireFormat::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireFormat {
    Json,
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
    JsonNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeScriptConfig {
    pub enabled: bool,
    pub out_dir: String,
    pub wire_contract: TypeScriptWireContract,
    pub layout: TypeScriptLayout,
    pub bundle_file: String,
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
            wire_contract: TypeScriptWireContract::JsonExchange,
            layout: TypeScriptLayout::Bundle,
            bundle_file: "types.ts".to_owned(),
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
#[serde(rename_all = "snake_case")]
pub enum TypeScriptWireContract {
    JsonExchange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeScriptLayout {
    Bundle,
    PerType,
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
    UnsupportedSchemaVersion {
        found: u32,
        supported: u32,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            Self::Toml(source) => write!(f, "failed to parse config: {source}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "unsupported config schema_version {found}; supported schema_version is {supported}"
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Toml(source) => Some(source),
            Self::UnsupportedSchemaVersion { .. } => None,
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
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config.export.out_dir, "generated");
        assert_eq!(config.export.wire_format, WireFormat::Json);
        assert_eq!(
            config.numeric.large_int_policy,
            LargeIntPolicy::RequireExplicit
        );
        assert_eq!(
            config.typescript.wire_contract,
            TypeScriptWireContract::JsonExchange
        );
        assert_eq!(config.typescript.layout, TypeScriptLayout::Bundle);
        assert_eq!(config.typescript.bundle_file, "types.ts");
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
    fn parses_explicit_schema_version_and_wire_format() {
        let config =
            Config::from_toml_str("schema_version = 1\n\n[export]\nwire_format = \"json\"\n")
                .unwrap();

        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config.export.wire_format, WireFormat::Json);
    }

    #[test]
    fn rejects_unknown_schema_versions() {
        let err = Config::from_toml_str("schema_version = 2\n").unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported config schema_version")
        );
    }

    #[test]
    fn rejects_unknown_wire_formats() {
        let err = Config::from_toml_str("[export]\nwire_format = \"message_pack\"\n").unwrap_err();

        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn parses_typescript_json_exchange_contract() {
        let config =
            Config::from_toml_str("[typescript]\nwire_contract = \"json_exchange\"\n").unwrap();

        assert_eq!(
            config.typescript.wire_contract,
            TypeScriptWireContract::JsonExchange
        );
    }

    #[test]
    fn rejects_unsupported_typescript_wire_contracts() {
        let err = Config::from_toml_str("[typescript]\nwire_contract = \"runtime_validator\"\n")
            .unwrap_err();

        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn parses_typescript_layout_and_bundle_file() {
        let config = Config::from_toml_str(
            "[typescript]\nlayout = \"per_type\"\nbundle_file = \"dto-types.ts\"\n",
        )
        .unwrap();

        assert_eq!(config.typescript.layout, TypeScriptLayout::PerType);
        assert_eq!(config.typescript.bundle_file, "dto-types.ts");
    }

    #[test]
    fn rejects_unknown_typescript_layouts() {
        let err = Config::from_toml_str("[typescript]\nlayout = \"single_file\"\n").unwrap_err();

        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn parses_json_safe_large_integer_policy() {
        let config =
            Config::from_toml_str("[numeric]\nlarge_int_policy = \"json_number\"\n").unwrap();

        assert_eq!(config.numeric.large_int_policy, LargeIntPolicy::JsonNumber);
    }

    #[test]
    fn rejects_legacy_non_json_large_integer_policy() {
        let err = Config::from_toml_str("[numeric]\nlarge_int_policy = \"non_json_bigint\"\n")
            .unwrap_err();

        assert!(err.to_string().contains("unknown variant"));
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
