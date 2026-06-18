use std::collections::BTreeMap;

use crate::{
    Config, DefaultKind, Diagnostic, DiagnosticCode, EnumDef, EnumRepr, FieldDef, FlattenMode,
    LargeIntPolicy, Primitive, Registry, SerializePresence, StructDef, TypeDef, TypeRef,
    VariantDef, VariantShape,
};

pub fn validate_registry(registry: &Registry, config: &Config) -> Vec<Diagnostic> {
    let mut diagnostics = registry.diagnostics.clone();

    for type_def in registry.types_by_id.values() {
        match type_def {
            TypeDef::Struct(def) => validate_struct(def, config, &mut diagnostics),
            TypeDef::Enum(def) => validate_enum(def, config, &mut diagnostics),
        }
    }

    diagnostics
}

fn validate_struct(def: &StructDef, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    validate_fields(&def.export_name, None, &def.fields, config, diagnostics);
}

fn validate_enum(def: &EnumDef, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    validate_variant_discriminants(def, diagnostics);

    match &def.repr {
        EnumRepr::Untagged => diagnostics.push(
            Diagnostic::error(DiagnosticCode::new(304), "unsupported untagged enum")
                .with_help("`untagged` is not supported in the MVP.")
                .with_type(def.export_name.clone())
                .with_source(def.source.clone()),
        ),
        EnumRepr::External => validate_external_enum(def, config, diagnostics),
        EnumRepr::Internal { .. } | EnumRepr::Adjacent { .. } => {
            validate_tagged_enum(def, config, diagnostics);
        }
    }
}

fn validate_external_enum(def: &EnumDef, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    for variant in &def.variants {
        match &variant.shape {
            VariantShape::Unit => {}
            VariantShape::Struct(fields) => {
                diagnostics.push(unsupported_variant_shape(
                    def,
                    variant,
                    "externally tagged data enum",
                ));
                validate_fields(
                    &def.export_name,
                    Some(variant.rust_name.as_str()),
                    fields,
                    config,
                    diagnostics,
                );
            }
            VariantShape::Newtype(_) | VariantShape::Tuple(_) => {
                diagnostics.push(unsupported_variant_shape(
                    def,
                    variant,
                    "tuple/newtype variant",
                ));
            }
        }
    }
}

fn validate_tagged_enum(def: &EnumDef, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    for variant in &def.variants {
        match &variant.shape {
            VariantShape::Struct(fields) => validate_fields(
                &def.export_name,
                Some(variant.rust_name.as_str()),
                fields,
                config,
                diagnostics,
            ),
            VariantShape::Unit | VariantShape::Newtype(_) | VariantShape::Tuple(_) => {
                diagnostics.push(unsupported_variant_shape(
                    def,
                    variant,
                    "tagged enum non-struct variant",
                ));
            }
        }
    }
}

fn validate_variant_discriminants(def: &EnumDef, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = BTreeMap::<String, String>::new();

    for variant in &def.variants {
        if let Some(first_variant) =
            seen.insert(variant.wire_name.clone(), variant.rust_name.clone())
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::new(202),
                    format!("duplicate enum discriminant `{}`", variant.wire_name),
                )
                .with_help(format!(
                    "Variants `{first_variant}` and `{}` resolve to the same wire name.",
                    variant.rust_name
                ))
                .with_type(def.export_name.clone())
                .with_variant(variant.rust_name.clone())
                .with_source(variant.source.clone()),
            );
        }
    }
}

fn validate_fields(
    type_name: &str,
    variant_name: Option<&str>,
    fields: &[FieldDef],
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = BTreeMap::<String, String>::new();

    for field in fields {
        if !field.presence.is_serialized() {
            continue;
        }

        if field.flatten == FlattenMode::Flattened {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(303),
                    "unsupported Serde attribute `flatten`",
                    type_name,
                    variant_name,
                    field,
                )
                .with_help("`flatten` is not supported in the MVP. Use an explicit nested field."),
            );
        }

        if field.wire.diverges() {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(305),
                    "divergent serialize/deserialize names are unsupported",
                    type_name,
                    variant_name,
                    field,
                )
                .with_help("Use one shared `rename` value or split the DTO shape."),
            );
        }

        if !field.wire.aliases.is_empty() {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(307),
                    "unsupported Serde attribute `alias`",
                    type_name,
                    variant_name,
                    field,
                )
                .with_help("Aliases are deferred for the MVP."),
            );
        }

        if matches!(field.presence.default, Some(DefaultKind::CustomPath(_))) {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(306),
                    "custom default paths are unsupported",
                    type_name,
                    variant_name,
                    field,
                )
                .with_help("Use a supported built-in default kind."),
            );
        }

        if matches!(
            field.presence.serialize_presence,
            SerializePresence::SkipIfNone | SerializePresence::SkipIfDefault
        ) {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(308),
                    "conditional serialization is unsupported",
                    type_name,
                    variant_name,
                    field,
                )
                .with_help("`skip_serializing_if` is deferred for the MVP."),
            );
        }

        if let Some(first_field) = seen.insert(
            field.wire.serialize_name.clone(),
            field.rust_name.to_string(),
        ) {
            diagnostics.push(
                field_diagnostic(
                    DiagnosticCode::new(201),
                    format!("duplicate wire field name `{}`", field.wire.serialize_name),
                    type_name,
                    variant_name,
                    field,
                )
                .with_help(format!(
                    "Fields `{first_field}` and `{}` resolve to the same wire name.",
                    field.rust_name
                )),
            );
        }

        validate_type_ref(
            type_name,
            variant_name,
            field,
            &field.ty,
            config,
            diagnostics,
        );
    }
}

fn validate_type_ref(
    type_name: &str,
    variant_name: Option<&str>,
    field: &FieldDef,
    ty: &TypeRef,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match ty {
        TypeRef::Primitive(primitive) => {
            validate_primitive(
                type_name,
                variant_name,
                field,
                *primitive,
                config,
                diagnostics,
            );
        }
        TypeRef::String | TypeRef::Bytes(_) | TypeRef::Named(_) | TypeRef::GenericParam(_) => {}
        TypeRef::Option(inner) | TypeRef::Vec(inner) => {
            validate_type_ref(type_name, variant_name, field, inner, config, diagnostics);
        }
        TypeRef::Array { item, .. } => {
            validate_type_ref(type_name, variant_name, field, item, config, diagnostics);
        }
        TypeRef::Map { key, value } => {
            if !matches!(key.as_ref(), TypeRef::String) {
                diagnostics.push(
                    field_diagnostic(
                        DiagnosticCode::new(309),
                        "non-string map keys are unsupported",
                        type_name,
                        variant_name,
                        field,
                    )
                    .with_help("Use `HashMap<String, T>` or `BTreeMap<String, T>`."),
                );
            }
            validate_type_ref(type_name, variant_name, field, value, config, diagnostics);
        }
        TypeRef::Override(_) => {}
    }
}

fn validate_primitive(
    type_name: &str,
    variant_name: Option<&str>,
    field: &FieldDef,
    primitive: Primitive,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if primitive.requires_explicit_integer_policy()
        && config.numeric.large_int_policy == LargeIntPolicy::RequireExplicit
    {
        diagnostics.push(
            field_diagnostic(
                DiagnosticCode::new(401),
                "large integer field requires explicit numeric policy",
                type_name,
                variant_name,
                field,
            )
            .with_help("Add a supported field override or configure `[numeric].large_int_policy`."),
        );
    }
}

fn unsupported_variant_shape(def: &EnumDef, variant: &VariantDef, shape: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(304),
        format!("unsupported enum shape `{shape}`"),
    )
    .with_help("MVP enum support is fieldless enums plus tagged enums with struct variants.")
    .with_type(def.export_name.clone())
    .with_variant(variant.rust_name.clone())
    .with_source(variant.source.clone())
}

fn field_diagnostic(
    code: DiagnosticCode,
    message: impl Into<String>,
    type_name: &str,
    variant_name: Option<&str>,
    field: &FieldDef,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(code, message)
        .with_type(type_name.to_owned())
        .with_field(field.rust_name.to_string())
        .with_source(field.source.clone());

    if let Some(variant_name) = variant_name {
        diagnostic.with_variant(variant_name.to_owned())
    } else {
        diagnostic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FieldPresence, IdentName, RustTypeId, SourceSpan, TargetFieldNames, WireFieldNames,
    };

    fn span(line: u32) -> SourceSpan {
        SourceSpan::new("src/dto.rs", line, 1)
    }

    fn field(name: &str, wire: &str, ty: TypeRef) -> FieldDef {
        FieldDef::new(
            IdentName::new(name),
            WireFieldNames::same(wire),
            TargetFieldNames::new(wire, name),
            ty,
            span(10),
        )
    }

    fn registry_with_type(type_def: TypeDef) -> Registry {
        let mut registry = Registry::new();
        registry.register_type(RustTypeId::new("sdk", "Example"), type_def);
        registry
    }

    #[test]
    fn accepts_supported_struct_fields() {
        let def = StructDef::new("UserProfile", "UserProfile", span(1))
            .with_field(field("user_id", "userId", TypeRef::String))
            .with_field(field(
                "active",
                "active",
                TypeRef::Primitive(Primitive::Bool),
            ))
            .with_field(field("tags", "tags", TypeRef::vec(TypeRef::String)));
        let registry = registry_with_type(TypeDef::Struct(def));

        assert!(registry.validate(&Config::default()).is_empty());
    }

    #[test]
    fn rejects_duplicate_wire_fields() {
        let def = StructDef::new("UserProfile", "UserProfile", span(1))
            .with_field(field("user_id", "userId", TypeRef::String))
            .with_field(field("account_id", "userId", TypeRef::String));
        let registry = registry_with_type(TypeDef::Struct(def));

        let diagnostics = registry.validate(&Config::default());

        assert_eq!(diagnostics[0].code, DiagnosticCode::new(201));
        assert_eq!(diagnostics[0].field_name.as_deref(), Some("account_id"));
    }

    #[test]
    fn rejects_flatten_divergent_names_aliases_and_custom_defaults() {
        let mut metadata = field("metadata", "metadata", TypeRef::String);
        metadata.flatten = FlattenMode::Flattened;
        metadata.wire.deserialize_name = "metadata_in".to_owned();
        metadata.wire.aliases.push("meta".to_owned());
        metadata.presence =
            FieldPresence::defaulted(DefaultKind::CustomPath("fallback".to_owned()));

        let registry = registry_with_type(TypeDef::Struct(
            StructDef::new("UserProfile", "UserProfile", span(1)).with_field(metadata),
        ));

        let codes = registry
            .validate(&Config::default())
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&DiagnosticCode::new(303)));
        assert!(codes.contains(&DiagnosticCode::new(305)));
        assert!(codes.contains(&DiagnosticCode::new(306)));
        assert!(codes.contains(&DiagnosticCode::new(307)));
    }

    #[test]
    fn rejects_large_integers_when_policy_requires_explicit() {
        let registry = registry_with_type(TypeDef::Struct(
            StructDef::new("LedgerEntry", "LedgerEntry", span(1)).with_field(field(
                "amount",
                "amount",
                TypeRef::Primitive(Primitive::U128),
            )),
        ));

        let diagnostics = registry.validate(&Config::default());

        assert_eq!(diagnostics[0].code, DiagnosticCode::new(401));
        assert_eq!(diagnostics[0].field_name.as_deref(), Some("amount"));
    }

    #[test]
    fn allows_large_integers_when_global_policy_is_explicit() {
        let registry = registry_with_type(TypeDef::Struct(
            StructDef::new("LedgerEntry", "LedgerEntry", span(1)).with_field(field(
                "amount",
                "amount",
                TypeRef::Primitive(Primitive::U128),
            )),
        ));
        let mut config = Config::default();
        config.numeric.large_int_policy = LargeIntPolicy::JsonString;

        assert!(registry.validate(&config).is_empty());
    }

    #[test]
    fn rejects_non_string_map_keys() {
        let registry = registry_with_type(TypeDef::Struct(
            StructDef::new("Index", "Index", span(1)).with_field(field(
                "items",
                "items",
                TypeRef::Map {
                    key: Box::new(TypeRef::Primitive(Primitive::U32)),
                    value: Box::new(TypeRef::String),
                },
            )),
        ));

        let diagnostics = registry.validate(&Config::default());

        assert_eq!(diagnostics[0].code, DiagnosticCode::new(309));
    }

    #[test]
    fn rejects_unsupported_enum_shapes() {
        let registry = registry_with_type(TypeDef::Enum(
            EnumDef::new("Value", "Value", EnumRepr::Untagged, span(1)).with_variant(
                VariantDef::new(
                    "StringValue",
                    "stringValue",
                    VariantShape::Newtype(TypeRef::String),
                    span(2),
                ),
            ),
        ));

        let codes = registry
            .validate(&Config::default())
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&DiagnosticCode::new(304)));
    }

    #[test]
    fn accepts_tagged_enum_struct_variants() {
        let registry = registry_with_type(TypeDef::Enum(
            EnumDef::new(
                "SdkEvent",
                "SdkEvent",
                EnumRepr::Adjacent {
                    tag: "type".to_owned(),
                    content: "payload".to_owned(),
                },
                span(1),
            )
            .with_variant(VariantDef::new(
                "UserDeleted",
                "userDeleted",
                VariantShape::Struct(vec![field("user_id", "userId", TypeRef::String)]),
                span(2),
            )),
        ));

        assert!(registry.validate(&Config::default()).is_empty());
    }

    #[test]
    fn rejects_duplicate_enum_discriminants() {
        let registry = registry_with_type(TypeDef::Enum(
            EnumDef::new("Role", "Role", EnumRepr::External, span(1))
                .with_variant(VariantDef::new(
                    "Admin",
                    "ADMIN",
                    VariantShape::Unit,
                    span(2),
                ))
                .with_variant(VariantDef::new(
                    "Root",
                    "ADMIN",
                    VariantShape::Unit,
                    span(3),
                )),
        ));

        let diagnostics = registry.validate(&Config::default());

        assert_eq!(diagnostics[0].code, DiagnosticCode::new(202));
        assert_eq!(diagnostics[0].variant_name.as_deref(), Some("Root"));
    }
}
