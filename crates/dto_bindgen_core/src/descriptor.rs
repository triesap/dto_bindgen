use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{
    Diagnostic, DiagnosticCode, Primitive, Registry, RustTypeId, TypeDef, TypeId, TypeRef,
};

pub trait Dto {
    fn describe(ctx: &mut DescribeCtx) -> TypeRef;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescribeCtx {
    registry: Registry,
}

impl DescribeCtx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    pub fn into_registry(self) -> Registry {
        self.registry
    }

    pub fn register_type(&mut self, rust_id: RustTypeId, type_def: TypeDef) -> TypeRef {
        let dependencies = type_def_dependencies(&type_def);
        let type_id = self.registry.register_type(rust_id, type_def);

        for dependency in dependencies {
            if dependency != type_id {
                self.registry.add_dependency(type_id, dependency);
            }
        }

        TypeRef::named(type_id)
    }

    pub fn describe_root<T>(&mut self) -> TypeRef
    where
        T: Dto + 'static,
    {
        self.describe_root_descriptor(&RootDescriptor::new::<T>())
    }

    pub fn describe_root_descriptor(&mut self, root: &RootDescriptor) -> TypeRef {
        let type_ref = root.describe(self);
        match type_ref {
            TypeRef::Named(type_id) => self.registry.mark_root(type_id),
            _ => self.registry.add_diagnostic(
                Diagnostic::error(
                    DiagnosticCode::new(101),
                    "export root must describe a named DTO type",
                )
                .with_help("Only structs and enums implementing `Dto` can be export roots.")
                .with_type(root.rust_type_name),
            ),
        }
        type_ref
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RootDescriptor {
    pub rust_type_name: &'static str,
    describe: fn(&mut DescribeCtx) -> TypeRef,
}

impl RootDescriptor {
    pub fn new<T>() -> Self
    where
        T: Dto + 'static,
    {
        Self {
            rust_type_name: type_name::<T>(),
            describe: describe_type::<T>,
        }
    }

    pub fn describe(&self, ctx: &mut DescribeCtx) -> TypeRef {
        (self.describe)(ctx)
    }
}

pub fn build_registry(roots: impl IntoIterator<Item = RootDescriptor>) -> Registry {
    let mut ctx = DescribeCtx::new();

    for root in roots {
        ctx.describe_root_descriptor(&root);
    }

    ctx.into_registry()
}

fn describe_type<T>(ctx: &mut DescribeCtx) -> TypeRef
where
    T: Dto + 'static,
{
    T::describe(ctx)
}

fn type_def_dependencies(type_def: &TypeDef) -> BTreeSet<TypeId> {
    let mut dependencies = BTreeSet::new();

    match type_def {
        TypeDef::Struct(def) => {
            for field in &def.fields {
                collect_type_ref_dependencies(&field.ty, &mut dependencies);
            }
        }
        TypeDef::Enum(def) => {
            for variant in &def.variants {
                match &variant.shape {
                    crate::VariantShape::Unit => {}
                    crate::VariantShape::Newtype(ty) => {
                        collect_type_ref_dependencies(ty, &mut dependencies);
                    }
                    crate::VariantShape::Tuple(items) => {
                        for item in items {
                            collect_type_ref_dependencies(item, &mut dependencies);
                        }
                    }
                    crate::VariantShape::Struct(fields) => {
                        for field in fields {
                            collect_type_ref_dependencies(&field.ty, &mut dependencies);
                        }
                    }
                }
            }
        }
    }

    dependencies
}

fn collect_type_ref_dependencies(ty: &TypeRef, dependencies: &mut BTreeSet<TypeId>) {
    match ty {
        TypeRef::Named(type_id) => {
            dependencies.insert(*type_id);
        }
        TypeRef::Option(inner) | TypeRef::Vec(inner) => {
            collect_type_ref_dependencies(inner, dependencies);
        }
        TypeRef::Array { item, .. } => {
            collect_type_ref_dependencies(item, dependencies);
        }
        TypeRef::Map { key, value } => {
            collect_type_ref_dependencies(key, dependencies);
            collect_type_ref_dependencies(value, dependencies);
        }
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes(_)
        | TypeRef::GenericParam(_)
        | TypeRef::Override(_) => {}
    }
}

macro_rules! primitive_dto {
    ($ty:ty, $primitive:expr) => {
        impl Dto for $ty {
            fn describe(_ctx: &mut DescribeCtx) -> TypeRef {
                TypeRef::Primitive($primitive)
            }
        }
    };
}

impl Dto for String {
    fn describe(_ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::String
    }
}

primitive_dto!(bool, Primitive::Bool);
primitive_dto!(i8, Primitive::I8);
primitive_dto!(u8, Primitive::U8);
primitive_dto!(i16, Primitive::I16);
primitive_dto!(u16, Primitive::U16);
primitive_dto!(i32, Primitive::I32);
primitive_dto!(u32, Primitive::U32);
primitive_dto!(i64, Primitive::I64);
primitive_dto!(u64, Primitive::U64);
primitive_dto!(i128, Primitive::I128);
primitive_dto!(u128, Primitive::U128);
primitive_dto!(isize, Primitive::Isize);
primitive_dto!(usize, Primitive::Usize);
primitive_dto!(f32, Primitive::F32);
primitive_dto!(f64, Primitive::F64);

impl<T> Dto for Option<T>
where
    T: Dto,
{
    fn describe(ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::option(T::describe(ctx))
    }
}

impl<T> Dto for Vec<T>
where
    T: Dto,
{
    fn describe(ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::vec(T::describe(ctx))
    }
}

impl<T, const N: usize> Dto for [T; N]
where
    T: Dto,
{
    fn describe(ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::array(T::describe(ctx), N)
    }
}

impl<T> Dto for BTreeMap<String, T>
where
    T: Dto,
{
    fn describe(ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::string_keyed_map(T::describe(ctx))
    }
}

impl<T> Dto for HashMap<String, T>
where
    T: Dto,
{
    fn describe(ctx: &mut DescribeCtx) -> TypeRef {
        TypeRef::string_keyed_map(T::describe(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldDef, IdentName, SourceSpan, StructDef, TargetFieldNames, WireFieldNames};

    struct PostalAddress;
    struct UserProfile;

    fn span(line: u32) -> SourceSpan {
        SourceSpan::new("src/dto.rs", line, 1)
    }

    fn string_field(name: &str, wire: &str, source_line: u32) -> FieldDef {
        FieldDef::new(
            IdentName::new(name),
            WireFieldNames::same(wire),
            TargetFieldNames::new(wire, name),
            TypeRef::String,
            span(source_line),
        )
    }

    impl Dto for PostalAddress {
        fn describe(ctx: &mut DescribeCtx) -> TypeRef {
            let def = StructDef::new("PostalAddress", "PostalAddress", span(1))
                .with_field(string_field("line_1", "line1", 2));
            ctx.register_type(
                RustTypeId::new("sdk", "PostalAddress"),
                TypeDef::Struct(def),
            )
        }
    }

    impl Dto for UserProfile {
        fn describe(ctx: &mut DescribeCtx) -> TypeRef {
            let address_ty = PostalAddress::describe(ctx);
            let def = StructDef::new("UserProfile", "UserProfile", span(10))
                .with_field(string_field("user_id", "userId", 11))
                .with_field(FieldDef::new(
                    IdentName::new("address"),
                    WireFieldNames::same("address"),
                    TargetFieldNames::new("address", "address"),
                    address_ty,
                    span(12),
                ));
            ctx.register_type(RustTypeId::new("sdk", "UserProfile"), TypeDef::Struct(def))
        }
    }

    #[test]
    fn builtin_descriptors_return_neutral_type_refs() {
        let mut ctx = DescribeCtx::new();

        assert_eq!(String::describe(&mut ctx), TypeRef::String);
        assert_eq!(u32::describe(&mut ctx), TypeRef::Primitive(Primitive::U32));
        assert_eq!(
            <Vec<Option<String>> as Dto>::describe(&mut ctx),
            TypeRef::vec(TypeRef::option(TypeRef::String))
        );
        assert_eq!(
            <[bool; 4] as Dto>::describe(&mut ctx),
            TypeRef::array(TypeRef::Primitive(Primitive::Bool), 4)
        );
        assert!(ctx.registry().types_by_id.is_empty());
    }

    #[test]
    fn root_descriptors_build_registry_and_mark_roots() {
        let registry = build_registry([RootDescriptor::new::<UserProfile>()]);

        assert_eq!(registry.types_by_id.len(), 2);
        assert_eq!(registry.roots.len(), 1);

        let root = *registry.roots.iter().next().unwrap();
        let dependencies = registry.dependencies_of(root).collect::<Vec<_>>();
        assert_eq!(dependencies.len(), 1);
    }

    #[test]
    fn primitive_roots_are_diagnostic_errors() {
        let registry = build_registry([RootDescriptor::new::<String>()]);

        assert!(registry.roots.is_empty());
        assert_eq!(registry.diagnostics[0].code, DiagnosticCode::new(101));
        assert_eq!(
            registry.diagnostics[0].type_name.as_deref(),
            Some(std::any::type_name::<String>())
        );
    }

    #[test]
    fn describe_ctx_reuses_registered_rust_id() {
        let mut ctx = DescribeCtx::new();
        let first = PostalAddress::describe(&mut ctx);
        let second = PostalAddress::describe(&mut ctx);

        assert_eq!(first, second);
        assert_eq!(ctx.registry().types_by_id.len(), 1);
    }
}
