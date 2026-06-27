use core::fmt;
use core::str::FromStr;

use crate::BackendId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(u32);

impl TypeId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type:{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RustTypeId {
    pub package_name: String,
    pub crate_name: String,
    pub module_path: Vec<String>,
    pub rust_ident: String,
    pub generic_parameters: Vec<String>,
}

impl RustTypeId {
    pub fn new(
        package_name: impl Into<String>,
        crate_name: impl Into<String>,
        rust_ident: impl Into<String>,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            crate_name: crate_name.into(),
            module_path: Vec::new(),
            rust_ident: rust_ident.into(),
            generic_parameters: Vec::new(),
        }
    }

    pub fn with_module_path(mut self, module_path: impl IntoIterator<Item = String>) -> Self {
        self.module_path = module_path.into_iter().collect();
        self
    }

    pub fn with_generic_parameters(
        mut self,
        generic_parameters: impl IntoIterator<Item = String>,
    ) -> Self {
        self.generic_parameters = generic_parameters.into_iter().collect();
        self
    }
}

impl fmt::Display for RustTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}::", self.package_name, self.crate_name)?;
        for module in &self.module_path {
            write!(f, "{module}::")?;
        }
        f.write_str(&self.rust_ident)?;

        if !self.generic_parameters.is_empty() {
            write!(f, "<{}>", self.generic_parameters.join(", "))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseRustTypeIdError {
    reason: &'static str,
}

impl ParseRustTypeIdError {
    const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

impl fmt::Display for ParseRustTypeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Rust type identity: {}", self.reason)
    }
}

impl std::error::Error for ParseRustTypeIdError {}

impl FromStr for RustTypeId {
    type Err = ParseRustTypeIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || value.trim() != value {
            return Err(ParseRustTypeIdError::new(
                "identity must be non-empty and unpadded",
            ));
        }

        let (package_name, rest) = value
            .split_once(':')
            .ok_or_else(|| ParseRustTypeIdError::new("expected `package:crate::path::Type`"))?;
        if package_name.is_empty() {
            return Err(ParseRustTypeIdError::new("package name is empty"));
        }

        let mut parts = rest.split("::").collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(ParseRustTypeIdError::new(
                "expected crate name and Rust type identifier",
            ));
        }

        let crate_name = parts.remove(0);
        if !is_rust_identifier(crate_name) {
            return Err(ParseRustTypeIdError::new(
                "crate name is not a Rust identifier",
            ));
        }

        let raw_ident = parts
            .pop()
            .ok_or_else(|| ParseRustTypeIdError::new("Rust type identifier is missing"))?;
        let (rust_ident, generic_parameters) = parse_ident_and_generics(raw_ident)?;
        if !is_rust_identifier(rust_ident) {
            return Err(ParseRustTypeIdError::new(
                "Rust type identifier is not a Rust identifier",
            ));
        }

        let mut module_path = Vec::new();
        for module in parts {
            if !is_rust_identifier(module) {
                return Err(ParseRustTypeIdError::new(
                    "module path contains a non-Rust identifier",
                ));
            }
            module_path.push(module.to_owned());
        }

        Ok(Self {
            package_name: package_name.to_owned(),
            crate_name: crate_name.to_owned(),
            module_path,
            rust_ident: rust_ident.to_owned(),
            generic_parameters,
        })
    }
}

fn parse_ident_and_generics(value: &str) -> Result<(&str, Vec<String>), ParseRustTypeIdError> {
    let Some(start) = value.find('<') else {
        return Ok((value, Vec::new()));
    };

    if !value.ends_with('>') {
        return Err(ParseRustTypeIdError::new(
            "generic parameter list is not closed",
        ));
    }

    let ident = &value[..start];
    let generic_list = &value[start + 1..value.len() - 1];
    if generic_list.is_empty() {
        return Err(ParseRustTypeIdError::new(
            "generic parameter list cannot be empty",
        ));
    }

    let mut generic_parameters = Vec::new();
    for generic in generic_list.split(',') {
        let generic = generic.trim();
        if !is_rust_identifier(generic) {
            return Err(ParseRustTypeIdError::new(
                "generic parameter is not a Rust identifier",
            ));
        }
        generic_parameters.push(generic.to_owned());
    }

    Ok((ident, generic_parameters))
}

fn is_rust_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Namespace(String);

impl Namespace {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTypeName {
    pub backend: BackendId,
    pub namespace: Namespace,
    pub name: String,
}

impl TargetTypeName {
    pub fn new(backend: BackendId, namespace: Namespace, name: impl Into<String>) -> Self {
        Self {
            backend,
            namespace,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratedFileId {
    pub backend: BackendId,
    pub normalized_relative_path: String,
}

impl GeneratedFileId {
    pub fn new(backend: BackendId, normalized_relative_path: impl Into<String>) -> Self {
        Self {
            backend,
            normalized_relative_path: normalized_relative_path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rust_type_identity() {
        let rust_id = RustTypeId::new("radroots-sdk", "radroots_sdk", "UserProfile")
            .with_module_path(["types".to_owned(), "identity".to_owned()])
            .with_generic_parameters(["T".to_owned()]);

        assert_eq!(
            rust_id.to_string(),
            "radroots-sdk:radroots_sdk::types::identity::UserProfile<T>"
        );
    }

    #[test]
    fn parses_rust_type_identity() {
        let rust_id = "radroots-sdk:radroots_sdk::types::identity::UserProfile<T, U>"
            .parse::<RustTypeId>()
            .unwrap();

        assert_eq!(rust_id.package_name, "radroots-sdk");
        assert_eq!(rust_id.crate_name, "radroots_sdk");
        assert_eq!(rust_id.module_path, ["types", "identity"]);
        assert_eq!(rust_id.rust_ident, "UserProfile");
        assert_eq!(rust_id.generic_parameters, ["T", "U"]);
    }

    #[test]
    fn rejects_invalid_rust_type_identity() {
        assert!("radroots_sdk::UserProfile".parse::<RustTypeId>().is_err());
        assert!(
            "radroots-sdk:radroots-sdk::UserProfile"
                .parse::<RustTypeId>()
                .is_err()
        );
        assert!(
            "radroots-sdk:radroots_sdk::9UserProfile"
                .parse::<RustTypeId>()
                .is_err()
        );
    }

    #[test]
    fn keeps_target_name_parts_separate() {
        let name = TargetTypeName::new(
            BackendId::TypeScript,
            Namespace::new("identity"),
            "UserProfile",
        );

        assert_eq!(name.backend, BackendId::TypeScript);
        assert_eq!(name.namespace.as_str(), "identity");
        assert_eq!(name.name, "UserProfile");
    }

    #[test]
    fn supports_root_namespace() {
        let namespace = Namespace::root();
        assert!(namespace.is_root());
        assert_eq!(namespace.as_str(), "");
    }
}
