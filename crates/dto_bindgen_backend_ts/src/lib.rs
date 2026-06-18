#![forbid(unsafe_code)]

pub fn backend_name() -> &'static str {
    "typescript"
}

pub fn core_version() -> &'static str {
    dto_bindgen_core::VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifies_backend() {
        assert_eq!(crate::backend_name(), "typescript");
        assert!(!crate::core_version().is_empty());
    }
}
