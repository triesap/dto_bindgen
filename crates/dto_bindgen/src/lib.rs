#![forbid(unsafe_code)]
#![doc = include_str!("../../../README.md")]

pub use dto_bindgen_macros::Dto;

pub mod config {
    pub use dto_bindgen_core::{
        Config, ConfigError, ExportConfig, ImportExtension, LargeIntPolicy, ModuleResolution,
        NumericConfig, PythonConfig, PythonMode, TsEmit, TypeScriptConfig, TypeScriptStyle,
        UnknownFieldsPolicy,
    };
}

pub mod diagnostics {
    pub use dto_bindgen_core::VERSION;
}

pub mod export {
    pub use dto_bindgen_core::VERSION;
}

pub mod prelude {
    pub use crate::Dto;
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
}
