#![forbid(unsafe_code)]
#![doc = include_str!("../../../README.md")]

pub use dto_bindgen_core::Dto;
pub use dto_bindgen_macros::Dto;

pub mod config {
    pub use dto_bindgen_core::{
        CONFIG_SCHEMA_VERSION, Config, ConfigError, ExportConfig, ImportExtension, LargeIntPolicy,
        ModuleResolution, NumericConfig, PackageRootDiscoveryConfig, PythonConfig, PythonMode,
        RootDiscoveryConfig, RootDiscoveryMode, TsEmit, TypeScriptConfig, TypeScriptStyle,
        TypeScriptWireContract, UnknownFieldsPolicy, WireFormat,
    };
}

pub mod diagnostics {
    pub use dto_bindgen_core::{
        BackendId, Diagnostic, DiagnosticCode, ParseBackendIdError, ParseDiagnosticCodeError,
        Severity, SourceFile, SourcePosition, SourceSpan, VERSION,
    };
}

pub mod export;

#[macro_export]
macro_rules! export_types {
    (config = $config:expr, roots = [$($root:ty),* $(,)?] $(,)?) => {{
        $crate::export::export_with_roots(
            $crate::export::ExportOptions::new($config),
            [$($crate::export::RootDescriptor::new::<$root>()),*],
        )
    }};
}

pub mod prelude {
    pub use crate::Dto;
}

#[doc(hidden)]
pub mod __private {
    pub use dto_bindgen_core::{
        DefaultKind, DescribeCtx, EnumDef, EnumRepr, FieldContract, FieldDef, FieldPresence,
        FieldWireContract, IdentName, IntRepr, RustTypeId, SourceSpan, StructDef, TargetFieldNames,
        TypeDef, TypeRef, VariantDef, VariantShape, WireFieldNames,
    };
}

pub fn version() -> &'static str {
    dto_bindgen_core::VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_version() {
        assert!(!crate::version().is_empty());
    }

    #[test]
    fn exposes_descriptor_api() {
        let registry =
            crate::export::build_registry([crate::export::RootDescriptor::new::<String>()]);

        assert!(registry.has_errors());
    }
}
