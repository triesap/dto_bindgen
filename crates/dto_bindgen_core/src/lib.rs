#![forbid(unsafe_code)]

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_package_version() {
        assert_eq!(crate::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
