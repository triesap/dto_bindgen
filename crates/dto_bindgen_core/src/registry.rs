use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BackendId, Diagnostic, GeneratedFileId, Namespace, RustTypeId, TargetTypeName, TypeDef, TypeId,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    next_type_id: u32,
    pub types_by_id: BTreeMap<TypeId, TypeDef>,
    pub rust_id_to_type_id: BTreeMap<RustTypeId, TypeId>,
    pub target_names: BTreeMap<(BackendId, Namespace, String), TypeId>,
    pub output_paths: BTreeMap<GeneratedFileId, TypeId>,
    pub dependencies: BTreeMap<TypeId, BTreeSet<TypeId>>,
    pub reverse_dependencies: BTreeMap<TypeId, BTreeSet<TypeId>>,
    pub roots: BTreeSet<TypeId>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_type(&mut self, rust_id: RustTypeId, type_def: TypeDef) -> TypeId {
        if let Some(existing) = self.rust_id_to_type_id.get(&rust_id) {
            return *existing;
        }

        let type_id = self.allocate_type_id();
        self.types_by_id.insert(type_id, type_def);
        self.rust_id_to_type_id.insert(rust_id, type_id);
        self.dependencies.entry(type_id).or_default();
        self.reverse_dependencies.entry(type_id).or_default();
        type_id
    }

    pub fn type_def(&self, type_id: TypeId) -> Option<&TypeDef> {
        self.types_by_id.get(&type_id)
    }

    pub fn mark_root(&mut self, type_id: TypeId) {
        self.roots.insert(type_id);
    }

    pub fn add_dependency(&mut self, from: TypeId, to: TypeId) {
        self.dependencies.entry(from).or_default().insert(to);
        self.reverse_dependencies
            .entry(to)
            .or_default()
            .insert(from);
        self.dependencies.entry(to).or_default();
        self.reverse_dependencies.entry(from).or_default();
    }

    pub fn dependencies_of(&self, type_id: TypeId) -> impl Iterator<Item = TypeId> + '_ {
        self.dependencies
            .get(&type_id)
            .into_iter()
            .flat_map(|deps| deps.iter().copied())
    }

    pub fn transitive_dependencies_of(&self, root: TypeId) -> BTreeSet<TypeId> {
        let mut visited = BTreeSet::new();
        let mut stack = Vec::from_iter(self.dependencies_of(root));

        while let Some(type_id) = stack.pop() {
            if visited.insert(type_id) {
                stack.extend(self.dependencies_of(type_id));
            }
        }

        visited
    }

    pub fn assign_target_name(
        &mut self,
        target: TargetTypeName,
        type_id: TypeId,
    ) -> Option<TypeId> {
        self.target_names
            .insert((target.backend, target.namespace, target.name), type_id)
    }

    pub fn assign_output_path(
        &mut self,
        file_id: GeneratedFileId,
        type_id: TypeId,
    ) -> Option<TypeId> {
        self.output_paths.insert(file_id, type_id)
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::blocks_export)
    }

    pub fn validate(&self, config: &crate::Config) -> Vec<Diagnostic> {
        crate::validation::validate_registry(self, config)
    }

    fn allocate_type_id(&mut self) -> TypeId {
        self.next_type_id += 1;
        TypeId::new(self.next_type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticCode, EnumDef, EnumRepr, Severity, SourceSpan, StructDef};

    fn span() -> SourceSpan {
        SourceSpan::new("src/types.rs", 1, 1)
    }

    fn struct_type(name: &str) -> TypeDef {
        TypeDef::Struct(StructDef::new(name, name, span()))
    }

    fn enum_type(name: &str) -> TypeDef {
        TypeDef::Enum(EnumDef::new(name, name, EnumRepr::External, span()))
    }

    #[test]
    fn registers_types_deterministically() {
        let mut registry = Registry::new();
        let user =
            registry.register_type(RustTypeId::new("sdk", "sdk", "User"), struct_type("User"));
        let event =
            registry.register_type(RustTypeId::new("sdk", "sdk", "Event"), enum_type("Event"));

        assert_eq!(user, TypeId::new(1));
        assert_eq!(event, TypeId::new(2));
        assert_eq!(registry.types_by_id.len(), 2);
    }

    #[test]
    fn reuses_existing_rust_identity() {
        let mut registry = Registry::new();
        let first =
            registry.register_type(RustTypeId::new("sdk", "sdk", "User"), struct_type("User"));
        let second = registry.register_type(
            RustTypeId::new("sdk", "sdk", "User"),
            struct_type("UserAgain"),
        );

        assert_eq!(first, second);
        assert_eq!(registry.types_by_id.len(), 1);
    }

    #[test]
    fn stores_dependency_edges_in_both_directions() {
        let mut registry = Registry::new();
        let user =
            registry.register_type(RustTypeId::new("sdk", "sdk", "User"), struct_type("User"));
        let address = registry.register_type(
            RustTypeId::new("sdk", "sdk", "Address"),
            struct_type("Address"),
        );

        registry.add_dependency(user, address);

        assert_eq!(
            registry.dependencies_of(user).collect::<Vec<_>>(),
            vec![address]
        );
        assert!(
            registry
                .reverse_dependencies
                .get(&address)
                .unwrap()
                .contains(&user)
        );
    }

    #[test]
    fn traverses_transitive_dependencies() {
        let mut registry = Registry::new();
        let event =
            registry.register_type(RustTypeId::new("sdk", "sdk", "Event"), enum_type("Event"));
        let user =
            registry.register_type(RustTypeId::new("sdk", "sdk", "User"), struct_type("User"));
        let address = registry.register_type(
            RustTypeId::new("sdk", "sdk", "Address"),
            struct_type("Address"),
        );

        registry.add_dependency(event, user);
        registry.add_dependency(user, address);

        assert_eq!(
            registry.transitive_dependencies_of(event),
            BTreeSet::from([user, address])
        );
    }

    #[test]
    fn records_roots_names_paths_and_diagnostics() {
        let mut registry = Registry::new();
        let user =
            registry.register_type(RustTypeId::new("sdk", "sdk", "User"), struct_type("User"));

        registry.mark_root(user);
        registry.assign_target_name(
            TargetTypeName::new(BackendId::TypeScript, Namespace::root(), "User"),
            user,
        );
        registry.assign_output_path(GeneratedFileId::new(BackendId::TypeScript, "user.ts"), user);
        registry.add_diagnostic(crate::Diagnostic::new(
            DiagnosticCode::new(201),
            Severity::Error,
            "duplicate wire field name",
        ));

        assert!(registry.roots.contains(&user));
        assert_eq!(registry.target_names.len(), 1);
        assert_eq!(registry.output_paths.len(), 1);
        assert!(registry.has_errors());
    }
}
