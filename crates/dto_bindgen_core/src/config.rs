use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

use crate::RustTypeId;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub export: ExportConfig,
    pub numeric: NumericConfig,
    pub typescript: TypeScriptConfig,
    pub python: PythonConfig,
    #[serde(rename = "package")]
    pub packages: Vec<PackageConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            export: ExportConfig::default(),
            numeric: NumericConfig::default(),
            typescript: TypeScriptConfig::default(),
            python: PythonConfig::default(),
            packages: Vec::new(),
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

        validate_external_imports(&self.typescript.external_types)?;
        validate_packages(&self.packages)?;
        validate_package_graph(&self.packages)?;

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
    #[serde(rename = "external_type")]
    pub external_types: Vec<ExternalTypeImportConfig>,
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
            external_types: Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    pub key: String,
    pub rust_package: String,
    pub rust_crate: String,
    #[serde(rename = "npm")]
    pub npm_name: String,
    pub out_dir: String,
    #[serde(default, deserialize_with = "deserialize_rust_type_ids")]
    pub roots: Vec<RustTypeId>,
    #[serde(default, rename = "external_type")]
    pub external_types: Vec<ExternalTypeImportConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalTypeImportConfig {
    #[serde(deserialize_with = "deserialize_rust_type_id")]
    pub rust: RustTypeId,
    pub typescript: String,
    pub from: String,
}

fn deserialize_rust_type_id<'de, D>(deserializer: D) -> Result<RustTypeId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
}

fn deserialize_rust_type_ids<'de, D>(deserializer: D) -> Result<Vec<RustTypeId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    values
        .into_iter()
        .map(|value| value.parse().map_err(serde::de::Error::custom))
        .collect()
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
    Validation(String),
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
            Self::Validation(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Toml(source) => Some(source),
            Self::UnsupportedSchemaVersion { .. } => None,
            Self::Validation(_) => None,
        }
    }
}

fn validate_external_imports(imports: &[ExternalTypeImportConfig]) -> Result<(), ConfigError> {
    let mut seen_rust = BTreeSet::<RustTypeId>::new();
    let mut seen_bindings = BTreeSet::<(String, String)>::new();

    for import in imports {
        if !seen_rust.insert(import.rust.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate external type import for `{}`",
                import.rust
            )));
        }
        if !is_valid_typescript_module_specifier(&import.from) {
            return Err(ConfigError::Validation(format!(
                "invalid TypeScript module specifier `{}`",
                import.from
            )));
        }
        if import.typescript.is_empty() {
            return Err(ConfigError::Validation(
                "external type import has empty TypeScript name".to_owned(),
            ));
        }
        if !seen_bindings.insert((import.from.clone(), import.typescript.clone())) {
            return Err(ConfigError::Validation(format!(
                "duplicate external TypeScript import binding `{}` from `{}`",
                import.typescript, import.from
            )));
        }
    }

    Ok(())
}

fn validate_packages(packages: &[PackageConfig]) -> Result<(), ConfigError> {
    let mut keys = BTreeSet::<String>::new();
    let mut npm_names = BTreeSet::<String>::new();

    for package in packages {
        if package.key.is_empty() {
            return Err(ConfigError::Validation(
                "package key cannot be empty".to_owned(),
            ));
        }
        if !keys.insert(package.key.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate package key `{}`",
                package.key
            )));
        }
        if package.rust_package.is_empty() {
            return Err(ConfigError::Validation(format!(
                "package `{}` has empty rust_package",
                package.key
            )));
        }
        if package.rust_crate.is_empty() {
            return Err(ConfigError::Validation(format!(
                "package `{}` has empty rust_crate",
                package.key
            )));
        }
        if !is_valid_typescript_module_specifier(&package.npm_name) {
            return Err(ConfigError::Validation(format!(
                "package `{}` has invalid npm module specifier `{}`",
                package.key, package.npm_name
            )));
        }
        if !npm_names.insert(package.npm_name.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate package npm module specifier `{}`",
                package.npm_name
            )));
        }
        if package.out_dir.is_empty() {
            return Err(ConfigError::Validation(format!(
                "package `{}` has empty out_dir",
                package.key
            )));
        }
        validate_external_imports(&package.external_types)?;
    }

    Ok(())
}

fn validate_package_graph(packages: &[PackageConfig]) -> Result<(), ConfigError> {
    let package_by_npm = packages
        .iter()
        .map(|package| (package.npm_name.clone(), package.key.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut graph = packages
        .iter()
        .map(|package| (package.key.clone(), BTreeSet::<String>::new()))
        .collect::<BTreeMap<_, _>>();

    for package in packages {
        for import in &package.external_types {
            if let Some(dependency) = package_by_npm.get(&import.from) {
                graph
                    .entry(package.key.clone())
                    .or_default()
                    .insert(dependency.clone());
            }
        }
    }

    if let Some(cycle) = first_package_cycle(&graph) {
        return Err(ConfigError::Validation(format!(
            "package dependency cycle detected: {}",
            cycle.join(" -> ")
        )));
    }

    Ok(())
}

fn first_package_cycle(graph: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    let mut visiting = BTreeSet::<String>::new();
    let mut visited = BTreeSet::<String>::new();
    let mut stack = Vec::<String>::new();

    for package in graph.keys() {
        if let Some(cycle) = visit_package(package, graph, &mut visiting, &mut visited, &mut stack)
        {
            return Some(cycle);
        }
    }

    None
}

fn visit_package(
    package: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Option<Vec<String>> {
    if let Some(start) = stack.iter().position(|value| value == package) {
        let mut cycle = stack[start..].to_vec();
        cycle.push(package.to_owned());
        return Some(cycle);
    }
    if visited.contains(package) {
        return None;
    }

    visiting.insert(package.to_owned());
    stack.push(package.to_owned());

    if let Some(dependencies) = graph.get(package) {
        for dependency in dependencies {
            if visiting.contains(dependency) || !visited.contains(dependency) {
                if let Some(cycle) = visit_package(dependency, graph, visiting, visited, stack) {
                    return Some(cycle);
                }
            }
        }
    }

    stack.pop();
    visiting.remove(package);
    visited.insert(package.to_owned());
    None
}

fn is_valid_typescript_module_specifier(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && !value.contains('"')
        && !value.contains('\'')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
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
    fn parses_typescript_external_type_imports() {
        let config = Config::from_toml_str(
            r#"
[[typescript.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#,
        )
        .unwrap();

        let external = &config.typescript.external_types[0];
        assert_eq!(external.rust.package_name, "radroots-core");
        assert_eq!(external.rust.crate_name, "radroots_core");
        assert_eq!(external.rust.module_path, ["money"]);
        assert_eq!(external.rust.rust_ident, "RadrootsCoreMoney");
        assert_eq!(external.typescript, "RadrootsCoreMoney");
        assert_eq!(external.from, "@radroots/core-bindings");
    }

    #[test]
    fn rejects_duplicate_typescript_external_type_imports() {
        let err = Config::from_toml_str(
            r#"
[[typescript.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"

[[typescript.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("duplicate external type import"));
    }

    #[test]
    fn rejects_invalid_external_type_rust_identity() {
        let err = Config::from_toml_str(
            r#"
[[typescript.external_type]]
rust = "radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("invalid Rust type identity"));
    }

    #[test]
    fn rejects_invalid_external_type_module_specifier() {
        let err = Config::from_toml_str(
            r#"
[[typescript.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = ""
"#,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid TypeScript module specifier")
        );
    }

    #[test]
    fn parses_package_configs_with_external_imports() {
        let config = Config::from_toml_str(
            r#"
[[package]]
key = "events"
rust_package = "radroots-events"
rust_crate = "radroots_events"
npm = "@radroots/event-bindings"
out_dir = "packages/event-bindings/src/generated"
roots = ["radroots-events:radroots_events::EventEnvelope"]

[[package.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#,
        )
        .unwrap();

        let package = &config.packages[0];
        assert_eq!(package.key, "events");
        assert_eq!(package.npm_name, "@radroots/event-bindings");
        assert_eq!(package.roots[0].rust_ident, "EventEnvelope");
        assert_eq!(package.external_types[0].typescript, "RadrootsCoreMoney");
    }

    #[test]
    fn accepts_acyclic_package_graphs() {
        let config = Config::from_toml_str(package_graph_config(false)).unwrap();

        assert_eq!(config.packages.len(), 2);
    }

    #[test]
    fn rejects_package_graph_cycles() {
        let err = Config::from_toml_str(package_graph_config(true)).unwrap_err();

        assert!(err.to_string().contains("package dependency cycle"));
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

    fn package_graph_config(with_cycle: bool) -> &'static str {
        if with_cycle {
            r#"
[[package]]
key = "core"
rust_package = "radroots-core"
rust_crate = "radroots_core"
npm = "@radroots/core-bindings"
out_dir = "packages/core-bindings/src/generated"

[[package.external_type]]
rust = "radroots-events:radroots_events::EventEnvelope"
typescript = "EventEnvelope"
from = "@radroots/event-bindings"

[[package]]
key = "events"
rust_package = "radroots-events"
rust_crate = "radroots_events"
npm = "@radroots/event-bindings"
out_dir = "packages/event-bindings/src/generated"

[[package.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#
        } else {
            r#"
[[package]]
key = "core"
rust_package = "radroots-core"
rust_crate = "radroots_core"
npm = "@radroots/core-bindings"
out_dir = "packages/core-bindings/src/generated"

[[package]]
key = "events"
rust_package = "radroots-events"
rust_crate = "radroots_events"
npm = "@radroots/event-bindings"
out_dir = "packages/event-bindings/src/generated"

[[package.external_type]]
rust = "radroots-core:radroots_core::money::RadrootsCoreMoney"
typescript = "RadrootsCoreMoney"
from = "@radroots/core-bindings"
"#
        }
    }
}
