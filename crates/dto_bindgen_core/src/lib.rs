#![forbid(unsafe_code)]

mod backend;
mod config;
mod definitions;
mod descriptor;
mod diagnostics;
mod export;
mod fields;
mod generated;
mod identity;
mod registry;
mod source;
mod types;
mod validation;
mod writer;

pub use backend::{Backend, BackendError, BackendId, ParseBackendIdError};
pub use config::{
    Config, ConfigError, ExportConfig, ImportExtension, LargeIntPolicy, ModuleResolution,
    NumericConfig, PythonConfig, PythonMode, TsEmit, TypeScriptConfig, TypeScriptStyle,
    UnknownFieldsPolicy,
};
pub use definitions::{
    ContainerAttrs, EnumDef, EnumRepr, FieldDef, GenericParam, StructDef, TypeDef, VariantDef,
    VariantShape,
};
pub use descriptor::{DescribeCtx, Dto, RootDescriptor, build_registry};
pub use diagnostics::{Diagnostic, DiagnosticCode, ParseDiagnosticCodeError, Severity};
pub use export::{ExportError, ExportOptions, ExportReport, validate_roots};
pub use fields::{
    DefaultKind, DocString, FieldPresence, FlattenMode, IdentName, IntRepr, SerializePresence,
    TargetFieldNames, WireFieldNames,
};
pub use generated::{
    GeneratedFile, GeneratedFileSet, GeneratedFileSetError, GeneratedManifest,
    GeneratedManifestFile, GeneratedPathError, GeneratedRelativePath,
};
pub use identity::{GeneratedFileId, Namespace, RustTypeId, TargetTypeName, TypeId};
pub use registry::Registry;
pub use source::{SourceFile, SourcePosition, SourceSpan};
pub use types::{BytesRepr, Primitive, TargetOverride, TypeRef};
pub use validation::validate_registry;
pub use writer::{CheckMismatch, CheckMismatchKind, OutputReport, OutputWriter, OutputWriterError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_package_version() {
        assert_eq!(crate::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
