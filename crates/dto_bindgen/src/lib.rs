#![forbid(unsafe_code)]
#![doc = include_str!("../../../README.md")]

pub use dto_bindgen_core::Dto;
pub use dto_bindgen_macros::Dto;

pub mod config {
    pub use dto_bindgen_core::{
        Config, ConfigError, ExportConfig, ImportExtension, LargeIntPolicy, ModuleResolution,
        NumericConfig, PythonConfig, PythonMode, TsEmit, TypeScriptConfig, TypeScriptStyle,
        UnknownFieldsPolicy,
    };
}

pub mod diagnostics {
    pub use dto_bindgen_core::{
        BackendId, Diagnostic, DiagnosticCode, ParseBackendIdError, ParseDiagnosticCodeError,
        Severity, SourceFile, SourcePosition, SourceSpan, VERSION,
    };
}

pub mod export {
    pub use dto_bindgen_core::{
        DescribeCtx, Registry, RootDescriptor, TypeRef, VERSION, build_registry,
    };
}

pub mod prelude {
    pub use crate::Dto;
}

#[doc(hidden)]
pub mod __private {
    pub use dto_bindgen_core::{
        DescribeCtx, FieldDef, IdentName, RustTypeId, SourceSpan, StructDef, TargetFieldNames,
        TypeDef, TypeRef, WireFieldNames,
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
