use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    BackendId, BytesRepr, CONFIG_SCHEMA_VERSION, ContainerAttrs, DefaultKind, Diagnostic, EnumDef,
    EnumRepr, FieldContract, FieldDef, FieldWireContract, FlattenMode, GenericParam, IntRepr,
    Primitive, Registry, RustTypeId, SerializePresence, SourcePosition, SourceSpan, StructDef,
    TypeDef, TypeId, TypeRef, VariantDef, VariantShape,
};

pub fn canonical_registry_json_bytes(registry: &Registry) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&CanonicalRegistry::from_registry(registry))
}

pub fn canonical_registry_sha256(registry: &Registry) -> Result<String, serde_json::Error> {
    let bytes = canonical_registry_json_bytes(registry)?;
    Ok(sha256_hex(&bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanonicalRegistry {
    pub schema_version: u32,
    pub types: Vec<CanonicalType>,
    pub roots: Vec<CanonicalNamedTypeRef>,
    pub dependencies: Vec<CanonicalDependency>,
    pub target_names: Vec<CanonicalTargetName>,
    pub output_paths: Vec<CanonicalOutputPath>,
    pub diagnostics: Vec<CanonicalDiagnostic>,
}

impl CanonicalRegistry {
    pub fn from_registry(registry: &Registry) -> Self {
        let type_id_to_rust_id = type_id_to_rust_id(registry);

        let types = registry
            .rust_id_to_type_id
            .iter()
            .filter_map(|(rust_id, type_id)| {
                registry
                    .type_def(*type_id)
                    .map(|def| CanonicalType::from_type_def(rust_id, def, &type_id_to_rust_id))
            })
            .collect();

        let mut roots = registry
            .roots
            .iter()
            .map(|type_id| canonical_named_ref(*type_id, &type_id_to_rust_id))
            .collect::<Vec<_>>();
        roots.sort();

        let mut dependencies = Vec::new();
        for (from, targets) in &registry.dependencies {
            for to in targets {
                dependencies.push(CanonicalDependency {
                    from: canonical_named_ref(*from, &type_id_to_rust_id),
                    to: canonical_named_ref(*to, &type_id_to_rust_id),
                });
            }
        }
        dependencies.sort();

        let target_names = registry
            .target_names
            .iter()
            .map(
                |((backend, namespace, name), type_id)| CanonicalTargetName {
                    backend: backend_name(backend).to_owned(),
                    namespace: namespace.as_str().to_owned(),
                    name: name.clone(),
                    rust_id: canonical_named_ref(*type_id, &type_id_to_rust_id),
                },
            )
            .collect();

        let output_paths = registry
            .output_paths
            .iter()
            .map(|(file_id, type_id)| CanonicalOutputPath {
                backend: backend_name(&file_id.backend).to_owned(),
                normalized_relative_path: file_id.normalized_relative_path.clone(),
                rust_id: canonical_named_ref(*type_id, &type_id_to_rust_id),
            })
            .collect();

        let mut diagnostics = registry
            .diagnostics
            .iter()
            .map(CanonicalDiagnostic::from_diagnostic)
            .collect::<Vec<_>>();
        diagnostics.sort();

        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            types,
            roots,
            dependencies,
            target_names,
            output_paths,
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalType {
    pub rust_id: CanonicalRustTypeId,
    pub def: CanonicalTypeDef,
}

impl CanonicalType {
    fn from_type_def(
        rust_id: &RustTypeId,
        def: &TypeDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        Self {
            rust_id: CanonicalRustTypeId::from_rust_type_id(rust_id),
            def: CanonicalTypeDef::from_type_def(def, type_id_to_rust_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalRustTypeId {
    pub package_name: String,
    pub crate_name: String,
    pub module_path: Vec<String>,
    pub rust_ident: String,
    pub generic_parameters: Vec<String>,
}

impl CanonicalRustTypeId {
    fn from_rust_type_id(rust_id: &RustTypeId) -> Self {
        Self {
            package_name: rust_id.package_name.clone(),
            crate_name: rust_id.crate_name.clone(),
            module_path: rust_id.module_path.clone(),
            rust_ident: rust_id.rust_ident.clone(),
            generic_parameters: rust_id.generic_parameters.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalTypeDef {
    Struct {
        rust_name: String,
        export_name: String,
        docs: Option<String>,
        fields: Vec<CanonicalField>,
        generics: Vec<CanonicalGenericParam>,
        attrs: CanonicalContainerAttrs,
        source: CanonicalSourceSpan,
    },
    Enum {
        rust_name: String,
        export_name: String,
        attrs: CanonicalContainerAttrs,
        repr: CanonicalEnumRepr,
        variants: Vec<CanonicalVariant>,
        source: CanonicalSourceSpan,
    },
}

impl CanonicalTypeDef {
    fn from_type_def(
        def: &TypeDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        match def {
            TypeDef::Struct(def) => Self::from_struct_def(def, type_id_to_rust_id),
            TypeDef::Enum(def) => Self::from_enum_def(def, type_id_to_rust_id),
        }
    }

    fn from_struct_def(
        def: &StructDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        Self::Struct {
            rust_name: def.rust_name.clone(),
            export_name: def.export_name.clone(),
            docs: def.docs.clone(),
            fields: def
                .fields
                .iter()
                .map(|field| CanonicalField::from_field_def(field, type_id_to_rust_id))
                .collect(),
            generics: def
                .generics
                .iter()
                .map(CanonicalGenericParam::from_generic_param)
                .collect(),
            attrs: CanonicalContainerAttrs::from_container_attrs(&def.attrs),
            source: CanonicalSourceSpan::from_source_span(&def.source),
        }
    }

    fn from_enum_def(
        def: &EnumDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        Self::Enum {
            rust_name: def.rust_name.clone(),
            export_name: def.export_name.clone(),
            attrs: CanonicalContainerAttrs::from_container_attrs(&def.attrs),
            repr: CanonicalEnumRepr::from_enum_repr(&def.repr),
            variants: def
                .variants
                .iter()
                .map(|variant| CanonicalVariant::from_variant_def(variant, type_id_to_rust_id))
                .collect(),
            source: CanonicalSourceSpan::from_source_span(&def.source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalContainerAttrs {
    pub rename: Option<String>,
    pub rename_all: Option<String>,
    pub rename_all_fields: Option<String>,
    pub ts_name: Option<String>,
    pub tag: Option<String>,
    pub content: Option<String>,
    pub deny_unknown_fields: bool,
    pub default: Option<CanonicalDefaultKind>,
}

impl CanonicalContainerAttrs {
    fn from_container_attrs(attrs: &ContainerAttrs) -> Self {
        Self {
            rename: attrs.rename.clone(),
            rename_all: attrs.rename_all.clone(),
            rename_all_fields: attrs.rename_all_fields.clone(),
            ts_name: attrs.ts_name.clone(),
            tag: attrs.tag.clone(),
            content: attrs.content.clone(),
            deny_unknown_fields: attrs.deny_unknown_fields,
            default: attrs
                .default
                .as_ref()
                .map(CanonicalDefaultKind::from_default_kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalGenericParam {
    pub name: String,
}

impl CanonicalGenericParam {
    fn from_generic_param(param: &GenericParam) -> Self {
        Self {
            name: param.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalField {
    pub rust_name: String,
    pub wire: CanonicalWireFieldNames,
    pub target: CanonicalTargetFieldNames,
    pub ty: CanonicalTypeRef,
    pub wire_contract: CanonicalFieldWireContract,
    pub contract: CanonicalFieldContract,
    pub int_repr: Option<CanonicalIntRepr>,
    pub flatten: CanonicalFlattenMode,
    pub docs: Option<String>,
    pub source: CanonicalSourceSpan,
}

impl CanonicalField {
    fn from_field_def(
        field: &FieldDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        Self {
            rust_name: field.rust_name.as_str().to_owned(),
            wire: CanonicalWireFieldNames {
                serialize_name: field.wire.serialize_name.clone(),
                deserialize_name: field.wire.deserialize_name.clone(),
                aliases: field.wire.aliases.clone(),
            },
            target: CanonicalTargetFieldNames {
                typescript: field.target.typescript.clone(),
                python: field.target.python.clone(),
            },
            ty: CanonicalTypeRef::from_type_ref(&field.ty, type_id_to_rust_id),
            wire_contract: CanonicalFieldWireContract::from_field_wire_contract(
                &field.wire_contract(),
            ),
            contract: CanonicalFieldContract::from_field_contract(&field.contract()),
            int_repr: field.int_repr.map(CanonicalIntRepr::from_int_repr),
            flatten: CanonicalFlattenMode::from_flatten_mode(field.flatten),
            docs: field.docs.as_ref().map(|docs| docs.as_str().to_owned()),
            source: CanonicalSourceSpan::from_source_span(&field.source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalWireFieldNames {
    pub serialize_name: String,
    pub deserialize_name: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalTargetFieldNames {
    pub typescript: String,
    pub python: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalFieldWireContract {
    pub nullable: bool,
    pub required_on_deserialize: bool,
    pub default: Option<CanonicalDefaultKind>,
    pub serialize_presence: CanonicalSerializePresence,
}

impl CanonicalFieldWireContract {
    fn from_field_wire_contract(wire: &FieldWireContract) -> Self {
        Self {
            nullable: wire.nullable,
            required_on_deserialize: wire.required_on_deserialize,
            default: wire
                .default
                .as_ref()
                .map(CanonicalDefaultKind::from_default_kind),
            serialize_presence: CanonicalSerializePresence::from_serialize_presence(
                wire.serialize_presence,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalFieldContract {
    pub serialized: bool,
    pub required: bool,
    pub nullable: bool,
    pub default: Option<CanonicalDefaultKind>,
    pub omit_when_none: bool,
}

impl CanonicalFieldContract {
    fn from_field_contract(contract: &FieldContract) -> Self {
        Self {
            serialized: contract.serialized,
            required: contract.required,
            nullable: contract.nullable,
            default: contract
                .default
                .as_ref()
                .map(CanonicalDefaultKind::from_default_kind),
            omit_when_none: contract.omit_when_none,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CanonicalDefaultKind {
    NoneValue,
    DefaultValue,
    EmptyString,
    EmptyVec,
    EmptyMap,
    BoolFalse,
    NumericZero,
    CustomPath(String),
}

impl CanonicalDefaultKind {
    fn from_default_kind(default: &DefaultKind) -> Self {
        match default {
            DefaultKind::NoneValue => Self::NoneValue,
            DefaultKind::DefaultValue => Self::DefaultValue,
            DefaultKind::EmptyString => Self::EmptyString,
            DefaultKind::EmptyVec => Self::EmptyVec,
            DefaultKind::EmptyMap => Self::EmptyMap,
            DefaultKind::BoolFalse => Self::BoolFalse,
            DefaultKind::NumericZero => Self::NumericZero,
            DefaultKind::CustomPath(path) => Self::CustomPath(path.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalIntRepr {
    JsonString,
    JsonNumber,
}

impl CanonicalIntRepr {
    fn from_int_repr(repr: IntRepr) -> Self {
        match repr {
            IntRepr::JsonString => Self::JsonString,
            IntRepr::JsonNumber => Self::JsonNumber,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalSerializePresence {
    Always,
    SkipIfNone,
    SkipIfDefault,
    Never,
}

impl CanonicalSerializePresence {
    fn from_serialize_presence(presence: SerializePresence) -> Self {
        match presence {
            SerializePresence::Always => Self::Always,
            SerializePresence::SkipIfNone => Self::SkipIfNone,
            SerializePresence::SkipIfDefault => Self::SkipIfDefault,
            SerializePresence::Never => Self::Never,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalFlattenMode {
    None,
    Flattened,
}

impl CanonicalFlattenMode {
    fn from_flatten_mode(flatten: FlattenMode) -> Self {
        match flatten {
            FlattenMode::None => Self::None,
            FlattenMode::Flattened => Self::Flattened,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalTypeRef {
    Primitive {
        primitive: &'static str,
    },
    String,
    Bytes {
        repr: CanonicalBytesRepr,
    },
    Option {
        inner: Box<CanonicalTypeRef>,
    },
    Vec {
        inner: Box<CanonicalTypeRef>,
    },
    Array {
        item: Box<CanonicalTypeRef>,
        len: usize,
    },
    Map {
        key: Box<CanonicalTypeRef>,
        value: Box<CanonicalTypeRef>,
    },
    Named {
        rust_id: CanonicalNamedTypeRef,
    },
    GenericParam {
        name: String,
    },
    Override {
        backend: String,
        target_type: String,
    },
}

impl CanonicalTypeRef {
    fn from_type_ref(
        ty: &TypeRef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        match ty {
            TypeRef::Primitive(primitive) => Self::Primitive {
                primitive: primitive_name(*primitive),
            },
            TypeRef::String => Self::String,
            TypeRef::Bytes(repr) => Self::Bytes {
                repr: CanonicalBytesRepr::from_bytes_repr(*repr),
            },
            TypeRef::Option(inner) => Self::Option {
                inner: Box::new(Self::from_type_ref(inner, type_id_to_rust_id)),
            },
            TypeRef::Vec(inner) => Self::Vec {
                inner: Box::new(Self::from_type_ref(inner, type_id_to_rust_id)),
            },
            TypeRef::Array { item, len } => Self::Array {
                item: Box::new(Self::from_type_ref(item, type_id_to_rust_id)),
                len: *len,
            },
            TypeRef::Map { key, value } => Self::Map {
                key: Box::new(Self::from_type_ref(key, type_id_to_rust_id)),
                value: Box::new(Self::from_type_ref(value, type_id_to_rust_id)),
            },
            TypeRef::Named(type_id) => Self::Named {
                rust_id: canonical_named_ref(*type_id, type_id_to_rust_id),
            },
            TypeRef::GenericParam(name) => Self::GenericParam { name: name.clone() },
            TypeRef::Override(override_type) => Self::Override {
                backend: backend_name(&override_type.backend).to_owned(),
                target_type: override_type.target_type.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalBytesRepr {
    Bytes,
    Base64String,
}

impl CanonicalBytesRepr {
    fn from_bytes_repr(repr: BytesRepr) -> Self {
        match repr {
            BytesRepr::Bytes => Self::Bytes,
            BytesRepr::Base64String => Self::Base64String,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalEnumRepr {
    External,
    Internal { tag: String },
    Adjacent { tag: String, content: String },
    Untagged,
}

impl CanonicalEnumRepr {
    fn from_enum_repr(repr: &EnumRepr) -> Self {
        match repr {
            EnumRepr::External => Self::External,
            EnumRepr::Internal { tag } => Self::Internal { tag: tag.clone() },
            EnumRepr::Adjacent { tag, content } => Self::Adjacent {
                tag: tag.clone(),
                content: content.clone(),
            },
            EnumRepr::Untagged => Self::Untagged,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalVariant {
    pub rust_name: String,
    pub wire_name: String,
    pub shape: CanonicalVariantShape,
    pub docs: Option<String>,
    pub source: CanonicalSourceSpan,
}

impl CanonicalVariant {
    fn from_variant_def(
        variant: &VariantDef,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        Self {
            rust_name: variant.rust_name.clone(),
            wire_name: variant.wire_name.clone(),
            shape: CanonicalVariantShape::from_variant_shape(&variant.shape, type_id_to_rust_id),
            docs: variant.docs.as_ref().map(|docs| docs.as_str().to_owned()),
            source: CanonicalSourceSpan::from_source_span(&variant.source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalVariantShape {
    Unit,
    Newtype { ty: CanonicalTypeRef },
    Tuple { items: Vec<CanonicalTypeRef> },
    Struct { fields: Vec<CanonicalField> },
}

impl CanonicalVariantShape {
    fn from_variant_shape(
        shape: &VariantShape,
        type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
    ) -> Self {
        match shape {
            VariantShape::Unit => Self::Unit,
            VariantShape::Newtype(ty) => Self::Newtype {
                ty: CanonicalTypeRef::from_type_ref(ty, type_id_to_rust_id),
            },
            VariantShape::Tuple(items) => Self::Tuple {
                items: items
                    .iter()
                    .map(|item| CanonicalTypeRef::from_type_ref(item, type_id_to_rust_id))
                    .collect(),
            },
            VariantShape::Struct(fields) => Self::Struct {
                fields: fields
                    .iter()
                    .map(|field| CanonicalField::from_field_def(field, type_id_to_rust_id))
                    .collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalNamedTypeRef {
    pub rust_id: Option<CanonicalRustTypeId>,
    pub missing_type_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalDependency {
    pub from: CanonicalNamedTypeRef,
    pub to: CanonicalNamedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalTargetName {
    pub backend: String,
    pub namespace: String,
    pub name: String,
    pub rust_id: CanonicalNamedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalOutputPath {
    pub backend: String,
    pub normalized_relative_path: String,
    pub rust_id: CanonicalNamedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalDiagnostic {
    pub code: u16,
    pub severity: String,
    pub message: String,
    pub help: Option<String>,
    pub source: Option<CanonicalSourceSpan>,
    pub type_name: Option<String>,
    pub field_name: Option<String>,
    pub variant_name: Option<String>,
    pub backend: Option<String>,
}

impl CanonicalDiagnostic {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            code: diagnostic.code.number(),
            severity: diagnostic.severity.as_str().to_owned(),
            message: diagnostic.message.clone(),
            help: diagnostic.help.clone(),
            source: diagnostic
                .source
                .as_ref()
                .map(CanonicalSourceSpan::from_source_span),
            type_name: diagnostic.type_name.clone(),
            field_name: diagnostic.field_name.clone(),
            variant_name: diagnostic.variant_name.clone(),
            backend: diagnostic
                .backend
                .as_ref()
                .map(backend_name)
                .map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalSourceSpan {
    pub file: String,
    pub start: CanonicalSourcePosition,
    pub end: Option<CanonicalSourcePosition>,
}

impl CanonicalSourceSpan {
    fn from_source_span(span: &SourceSpan) -> Self {
        Self {
            file: span.file.path().to_owned(),
            start: CanonicalSourcePosition::from_source_position(span.start),
            end: span.end.map(CanonicalSourcePosition::from_source_position),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CanonicalSourcePosition {
    pub line: u32,
    pub column: u32,
}

impl CanonicalSourcePosition {
    const fn from_source_position(position: SourcePosition) -> Self {
        Self {
            line: position.line,
            column: position.column,
        }
    }
}

fn type_id_to_rust_id(registry: &Registry) -> BTreeMap<TypeId, CanonicalRustTypeId> {
    registry
        .rust_id_to_type_id
        .iter()
        .map(|(rust_id, type_id)| (*type_id, CanonicalRustTypeId::from_rust_type_id(rust_id)))
        .collect()
}

fn canonical_named_ref(
    type_id: TypeId,
    type_id_to_rust_id: &BTreeMap<TypeId, CanonicalRustTypeId>,
) -> CanonicalNamedTypeRef {
    let rust_id = type_id_to_rust_id.get(&type_id).cloned();
    CanonicalNamedTypeRef {
        missing_type_id: rust_id.is_none().then_some(type_id.value()),
        rust_id,
    }
}

fn backend_name(backend: &BackendId) -> &str {
    backend.as_str()
}

fn primitive_name(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Bool => "bool",
        Primitive::I8 => "i8",
        Primitive::U8 => "u8",
        Primitive::I16 => "i16",
        Primitive::U16 => "u16",
        Primitive::I32 => "i32",
        Primitive::U32 => "u32",
        Primitive::I64 => "i64",
        Primitive::U64 => "u64",
        Primitive::I128 => "i128",
        Primitive::U128 => "u128",
        Primitive::Isize => "isize",
        Primitive::Usize => "usize",
        Primitive::F32 => "f32",
        Primitive::F64 => "f64",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdentName, SourceSpan, StructDef, TargetFieldNames, WireFieldNames};

    fn span() -> SourceSpan {
        SourceSpan::new("src/dto.rs", 1, 1)
    }

    fn string_struct(name: &str) -> TypeDef {
        TypeDef::Struct(StructDef::new(name, name, span()).with_field(FieldDef::new(
            IdentName::new("name"),
            WireFieldNames::same("name"),
            TargetFieldNames::new("name", "name"),
            TypeRef::String,
            span(),
        )))
    }

    fn registry_with_roots(root_order: [&str; 2]) -> Registry {
        let mut registry = Registry::new();
        for root in root_order {
            let type_id =
                registry.register_type(RustTypeId::new("sdk", "sdk", root), string_struct(root));
            registry.mark_root(type_id);
        }
        registry
    }

    #[test]
    fn canonical_registry_is_stable_across_root_ordering() {
        let first = registry_with_roots(["UserProfile", "SdkEvent"]);
        let second = registry_with_roots(["SdkEvent", "UserProfile"]);

        assert_ne!(first.rust_id_to_type_id, second.rust_id_to_type_id);
        assert_eq!(
            canonical_registry_json_bytes(&first).unwrap(),
            canonical_registry_json_bytes(&second).unwrap()
        );
        assert_eq!(
            canonical_registry_sha256(&first).unwrap(),
            canonical_registry_sha256(&second).unwrap()
        );
    }
}
