#![forbid(unsafe_code)]

mod backend;
mod definitions;
mod diagnostics;
mod fields;
mod identity;
mod source;
mod types;

pub use backend::{BackendId, ParseBackendIdError};
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
