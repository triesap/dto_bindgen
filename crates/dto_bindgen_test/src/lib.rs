#![forbid(unsafe_code)]

pub fn fixture_workspace_ready() -> bool {
    !dto_bindgen::version().is_empty()
        && !dto_bindgen_backend_python::backend_name().is_empty()
        && !dto_bindgen_backend_ts::backend_name().is_empty()
        && !dto_bindgen_core::VERSION.is_empty()
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_workspace_is_ready() {
        assert!(crate::fixture_workspace_ready());
    }
}
