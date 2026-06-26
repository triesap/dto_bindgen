use core::fmt;

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
