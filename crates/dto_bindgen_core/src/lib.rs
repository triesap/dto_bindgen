#![forbid(unsafe_code)]

mod backend;
mod config;
mod definitions;
mod diagnostics;
mod fields;
mod identity;
mod registry;
mod source;
mod types;

pub use backend::{BackendId, ParseBackendIdError};
pub use config::{
    Config, ConfigError, ExportConfig, ImportExtension, LargeIntPolicy, ModuleResolution,
    NumericConfig, PythonConfig, PythonMode, TsEmit, TypeScriptConfig, TypeScriptStyle,
    UnknownFieldsPolicy,
};
pub use definitions::{
    ContainerAttrs, EnumDef, EnumRepr, FieldDef, GenericParam, StructDef, TypeDef, VariantDef,
    VariantShape,
};
pub use diagnostics::{Diagnostic, DiagnosticCode, ParseDiagnosticCodeError, Severity};
pub use fields::{
    DefaultKind, DocString, FieldPresence, FlattenMode, IdentName, SerializePresence,
    TargetFieldNames, WireFieldNames,
};
pub use identity::{GeneratedFileId, Namespace, RustTypeId, TargetTypeName, TypeId};
pub use registry::Registry;
pub use source::{SourceFile, SourcePosition, SourceSpan};
pub use types::{BytesRepr, Primitive, TargetOverride, TypeRef};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_package_version() {
        assert_eq!(crate::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
