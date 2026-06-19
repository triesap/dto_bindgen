use crate::{
    DefaultKind, DocString, FieldPresence, FlattenMode, IdentName, IntRepr, SourceSpan,
    TargetFieldNames, TypeRef, WireFieldNames,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericParam {
    pub name: String,
}

impl GenericParam {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerAttrs {
    pub rename: Option<String>,
    pub rename_all: Option<String>,
    pub rename_all_fields: Option<String>,
    pub ts_name: Option<String>,
    pub tag: Option<String>,
    pub content: Option<String>,
    pub deny_unknown_fields: bool,
    pub default: Option<DefaultKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub rust_name: IdentName,
    pub wire: WireFieldNames,
    pub target: TargetFieldNames,
    pub ty: TypeRef,
    pub presence: FieldPresence,
    pub int_repr: Option<IntRepr>,
    pub flatten: FlattenMode,
    pub docs: Option<DocString>,
    pub source: SourceSpan,
}

impl FieldDef {
    pub fn new(
        rust_name: IdentName,
        wire: WireFieldNames,
        target: TargetFieldNames,
        ty: TypeRef,
        source: SourceSpan,
    ) -> Self {
        Self {
            rust_name,
            wire,
            target,
            ty,
            presence: FieldPresence::required(),
            int_repr: None,
            flatten: FlattenMode::None,
            docs: None,
            source,
        }
    }

    pub fn with_presence(mut self, presence: FieldPresence) -> Self {
        self.presence = presence;
        self
    }

    pub fn with_docs(mut self, docs: DocString) -> Self {
        self.docs = Some(docs);
        self
    }

    pub const fn with_int_repr(mut self, int_repr: IntRepr) -> Self {
        self.int_repr = Some(int_repr);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    pub rust_name: String,
    pub export_name: String,
    pub docs: Option<String>,
    pub fields: Vec<FieldDef>,
    pub generics: Vec<GenericParam>,
    pub attrs: ContainerAttrs,
    pub source: SourceSpan,
}

impl StructDef {
    pub fn new(
        rust_name: impl Into<String>,
        export_name: impl Into<String>,
        source: SourceSpan,
    ) -> Self {
        Self {
            rust_name: rust_name.into(),
            export_name: export_name.into(),
            docs: None,
            fields: Vec::new(),
            generics: Vec::new(),
            attrs: ContainerAttrs::default(),
            source,
        }
    }

    pub fn with_field(mut self, field: FieldDef) -> Self {
        self.fields.push(field);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnumRepr {
    External,
    Internal { tag: String },
    Adjacent { tag: String, content: String },
    Untagged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantShape {
    Unit,
    Newtype(TypeRef),
    Tuple(Vec<TypeRef>),
    Struct(Vec<FieldDef>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub rust_name: String,
    pub wire_name: String,
    pub shape: VariantShape,
    pub docs: Option<DocString>,
    pub source: SourceSpan,
}

impl VariantDef {
    pub fn new(
        rust_name: impl Into<String>,
        wire_name: impl Into<String>,
        shape: VariantShape,
        source: SourceSpan,
    ) -> Self {
        Self {
            rust_name: rust_name.into(),
            wire_name: wire_name.into(),
            shape,
            docs: None,
            source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub rust_name: String,
    pub export_name: String,
    pub attrs: ContainerAttrs,
    pub repr: EnumRepr,
    pub variants: Vec<VariantDef>,
    pub source: SourceSpan,
}

impl EnumDef {
    pub fn new(
        rust_name: impl Into<String>,
        export_name: impl Into<String>,
        repr: EnumRepr,
        source: SourceSpan,
    ) -> Self {
        Self {
            rust_name: rust_name.into(),
            export_name: export_name.into(),
            attrs: ContainerAttrs::default(),
            repr,
            variants: Vec::new(),
            source,
        }
    }

    pub fn with_variant(mut self, variant: VariantDef) -> Self {
        self.variants.push(variant);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeDef {
    Struct(StructDef),
    Enum(EnumDef),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Primitive;

    fn span() -> SourceSpan {
        SourceSpan::new("src/types.rs", 1, 1)
    }

    #[test]
    fn field_def_defaults_to_required_and_unflattened() {
        let field = FieldDef::new(
            IdentName::new("user_id"),
            WireFieldNames::same("userId"),
            TargetFieldNames::new("userId", "user_id"),
            TypeRef::String,
            span(),
        );

        assert!(field.presence.required_on_deserialize);
        assert_eq!(field.flatten, FlattenMode::None);
        assert_eq!(field.int_repr, None);
        assert_eq!(field.wire.serialize_name, "userId");
    }

    #[test]
    fn struct_def_preserves_field_order() {
        let first = FieldDef::new(
            IdentName::new("user_id"),
            WireFieldNames::same("userId"),
            TargetFieldNames::new("userId", "user_id"),
            TypeRef::String,
            span(),
        );
        let second = FieldDef::new(
            IdentName::new("active"),
            WireFieldNames::same("active"),
            TargetFieldNames::new("active", "active"),
            TypeRef::Primitive(Primitive::Bool),
            span(),
        );

        let def = StructDef::new("UserProfile", "UserProfile", span())
            .with_field(first)
            .with_field(second);

        assert_eq!(def.fields[0].rust_name.as_str(), "user_id");
        assert_eq!(def.fields[1].rust_name.as_str(), "active");
    }

    #[test]
    fn enum_def_can_represent_supported_tagging() {
        let def = EnumDef::new(
            "SdkEvent",
            "SdkEvent",
            EnumRepr::Adjacent {
                tag: "type".to_owned(),
                content: "payload".to_owned(),
            },
            span(),
        )
        .with_variant(VariantDef::new(
            "UserCreated",
            "userCreated",
            VariantShape::Struct(Vec::new()),
            span(),
        ));

        assert_eq!(def.variants.len(), 1);
        assert!(matches!(def.repr, EnumRepr::Adjacent { .. }));
    }

    #[test]
    fn enum_def_can_represent_deferred_shapes_for_validation() {
        let def = EnumDef::new("Value", "Value", EnumRepr::Untagged, span()).with_variant(
            VariantDef::new(
                "StringValue",
                "stringValue",
                VariantShape::Newtype(TypeRef::String),
                span(),
            ),
        );

        assert!(matches!(def.repr, EnumRepr::Untagged));
        assert!(matches!(def.variants[0].shape, VariantShape::Newtype(_)));
    }
}
