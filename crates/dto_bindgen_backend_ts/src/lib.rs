#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

use std::collections::BTreeSet;

use dto_bindgen_core::{
    Backend, BackendError, BackendId, Config, Diagnostic, DiagnosticCode, EnumDef, EnumRepr,
    FieldDef, GeneratedFile, GeneratedFileSet, IntRepr, Primitive, Registry, SerializePresence,
    StructDef, TsEmit, TypeDef, TypeId, TypeRef, VariantDef, VariantShape,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TypeScriptBackend;

impl TypeScriptBackend {
    pub const fn new() -> Self {
        Self
    }
}

impl Backend for TypeScriptBackend {
    fn id(&self) -> BackendId {
        BackendId::TypeScript
    }

    fn render(
        &self,
        registry: &Registry,
        config: &Config,
    ) -> Result<GeneratedFileSet, BackendError> {
        if !config.typescript.enabled {
            return Ok(GeneratedFileSet::empty());
        }

        let mut files = Vec::new();
        for (type_id, type_def) in &registry.types_by_id {
            let contents =
                render_type_file(*type_id, type_def, registry, config).map_err(|diagnostic| {
                    BackendError::from_diagnostic(BackendId::TypeScript, diagnostic)
                })?;
            let path = type_file_path(type_def, config);
            let file =
                GeneratedFile::new(BackendId::TypeScript, path, contents).map_err(|err| {
                    BackendError::from_diagnostic(
                        BackendId::TypeScript,
                        Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                            .with_backend(BackendId::TypeScript),
                    )
                })?;
            files.push(file);
        }

        if !registry.types_by_id.is_empty() {
            let index = GeneratedFile::new(
                BackendId::TypeScript,
                index_file_path(config),
                render_index_file(registry, config),
            )
            .map_err(|err| {
                BackendError::from_diagnostic(
                    BackendId::TypeScript,
                    Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                        .with_backend(BackendId::TypeScript),
                )
            })?;
            files.push(index);
        }

        GeneratedFileSet::try_from_files(files).map_err(|err| {
            BackendError::from_diagnostic(
                BackendId::TypeScript,
                Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                    .with_backend(BackendId::TypeScript),
            )
        })
    }
}

pub fn backend_name() -> &'static str {
    "typescript"
}

pub fn core_version() -> &'static str {
    dto_bindgen_core::VERSION
}

fn render_type_file(
    type_id: TypeId,
    type_def: &TypeDef,
    registry: &Registry,
    config: &Config,
) -> Result<String, Diagnostic> {
    let mut output = String::new();
    let imports = collect_imports(type_id, type_def);

    for dependency in imports {
        let dependency_def = registry.type_def(dependency).ok_or_else(|| {
            Diagnostic::error(DiagnosticCode::new(102), "missing named dependency")
                .with_backend(BackendId::TypeScript)
        })?;
        output.push_str("import type { ");
        output.push_str(type_name(dependency_def));
        output.push_str(" } from \"");
        output.push_str(&module_specifier(dependency_def, config));
        output.push_str("\";\n");
    }

    if !output.is_empty() {
        output.push('\n');
    }

    match type_def {
        TypeDef::Struct(def) => render_struct(def, registry, config, &mut output)?,
        TypeDef::Enum(def) => render_enum(def, registry, config, &mut output)?,
    }

    Ok(output)
}

fn render_struct(
    def: &StructDef,
    registry: &Registry,
    config: &Config,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("export type ");
    output.push_str(&def.export_name);
    output.push_str(" = {\n");

    for field in def.fields.iter().filter(|field| field_is_emitted(field)) {
        render_object_field(field, registry, config, 2, output)?;
    }

    output.push_str("};\n");
    Ok(())
}

fn render_enum(
    def: &EnumDef,
    registry: &Registry,
    config: &Config,
    output: &mut String,
) -> Result<(), Diagnostic> {
    match &def.repr {
        EnumRepr::External
            if def
                .variants
                .iter()
                .all(|variant| matches!(variant.shape, VariantShape::Unit)) =>
        {
            render_fieldless_enum(def, output);
            Ok(())
        }
        EnumRepr::Internal { tag } => render_tagged_enum(def, registry, config, tag, None, output),
        EnumRepr::Adjacent { tag, content } => {
            render_tagged_enum(def, registry, config, tag, Some(content.as_str()), output)
        }
        EnumRepr::External | EnumRepr::Untagged => Err(Diagnostic::error(
            DiagnosticCode::new(501),
            "unsupported enum representation",
        )
        .with_type(def.export_name.clone())
        .with_backend(BackendId::TypeScript)),
    }
}

fn render_fieldless_enum(def: &EnumDef, output: &mut String) {
    output.push_str("export type ");
    output.push_str(&def.export_name);
    output.push_str(" = ");

    for (index, variant) in def.variants.iter().enumerate() {
        if index > 0 {
            output.push_str(" | ");
        }
        output.push('"');
        output.push_str(&escape_string_literal(&variant.wire_name));
        output.push('"');
    }

    output.push_str(";\n");
}

fn render_tagged_enum(
    def: &EnumDef,
    registry: &Registry,
    config: &Config,
    tag: &str,
    content: Option<&str>,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("export type ");
    output.push_str(&def.export_name);
    output.push_str(" =\n");

    for variant in &def.variants {
        output.push_str("  | {\n");
        push_indent(output, 6);
        output.push_str(tag);
        output.push_str(": \"");
        output.push_str(&escape_string_literal(&variant.wire_name));
        output.push_str("\";\n");

        let VariantShape::Struct(fields) = &variant.shape else {
            return Err(unsupported_variant(def, variant));
        };

        if let Some(content) = content {
            push_indent(output, 6);
            output.push_str(content);
            output.push_str(": {\n");
            for field in fields.iter().filter(|field| field_is_emitted(field)) {
                render_object_field(field, registry, config, 8, output)?;
            }
            push_indent(output, 6);
            output.push_str("};\n");
        } else {
            for field in fields.iter().filter(|field| field_is_emitted(field)) {
                render_object_field(field, registry, config, 6, output)?;
            }
        }

        output.push_str("    }\n");
    }

    output.push_str(";\n");
    Ok(())
}

fn render_object_field(
    field: &FieldDef,
    registry: &Registry,
    config: &Config,
    indent: usize,
    output: &mut String,
) -> Result<(), Diagnostic> {
    push_indent(output, indent);
    output.push_str(&field.wire.serialize_name);
    if !field.presence.required_on_deserialize {
        output.push('?');
    }
    output.push_str(": ");
    output.push_str(&render_type_ref(
        &field.ty,
        field.int_repr,
        registry,
        config,
        field,
    )?);
    output.push_str(";\n");
    Ok(())
}

fn render_type_ref(
    ty: &TypeRef,
    int_repr: Option<IntRepr>,
    registry: &Registry,
    config: &Config,
    field: &FieldDef,
) -> Result<String, Diagnostic> {
    match ty {
        TypeRef::Primitive(primitive) => render_primitive(*primitive, int_repr, config, field),
        TypeRef::String => Ok("string".to_owned()),
        TypeRef::Bytes(_) => Ok("Uint8Array".to_owned()),
        TypeRef::Option(inner) => Ok(format!(
            "{} | null",
            render_type_ref(inner, int_repr, registry, config, field)?
        )),
        TypeRef::Vec(inner) | TypeRef::Array { item: inner, .. } => Ok(format!(
            "Array<{}>",
            render_type_ref(inner, int_repr, registry, config, field)?
        )),
        TypeRef::Map { key, value } => {
            if !matches!(key.as_ref(), TypeRef::String) {
                return Err(Diagnostic::error(
                    DiagnosticCode::new(509),
                    "non-string map keys are unsupported",
                )
                .with_field(field.rust_name.to_string())
                .with_backend(BackendId::TypeScript));
            }
            Ok(format!(
                "Record<string, {}>",
                render_type_ref(value, int_repr, registry, config, field)?
            ))
        }
        TypeRef::Named(type_id) => {
            let def = registry.type_def(*type_id).ok_or_else(|| {
                Diagnostic::error(DiagnosticCode::new(102), "missing named type reference")
                    .with_field(field.rust_name.to_string())
                    .with_backend(BackendId::TypeScript)
            })?;
            Ok(type_name(def).to_owned())
        }
        TypeRef::GenericParam(name) => Ok(name.clone()),
        TypeRef::Override(override_type) if override_type.backend == BackendId::TypeScript => {
            Ok(override_type.target_type.clone())
        }
        TypeRef::Override(_) => Err(Diagnostic::error(
            DiagnosticCode::new(501),
            "target override is for a different backend",
        )
        .with_field(field.rust_name.to_string())
        .with_backend(BackendId::TypeScript)),
    }
}

fn render_primitive(
    primitive: Primitive,
    int_repr: Option<IntRepr>,
    config: &Config,
    field: &FieldDef,
) -> Result<String, Diagnostic> {
    if primitive.requires_explicit_integer_policy() {
        return match int_repr {
            Some(IntRepr::JsonString) => Ok("string".to_owned()),
            Some(IntRepr::JsonNumberUnsafe) => Ok("number".to_owned()),
            Some(IntRepr::NonJsonBigint) => Ok("bigint".to_owned()),
            None => match config.numeric.large_int_policy {
                dto_bindgen_core::LargeIntPolicy::RequireExplicit => Err(Diagnostic::error(
                    DiagnosticCode::new(401),
                    "large integer field requires explicit numeric policy",
                )
                .with_field(field.rust_name.to_string())
                .with_backend(BackendId::TypeScript)),
                dto_bindgen_core::LargeIntPolicy::JsonString => Ok("string".to_owned()),
                dto_bindgen_core::LargeIntPolicy::JsonNumberUnsafe => Ok("number".to_owned()),
                dto_bindgen_core::LargeIntPolicy::NonJsonBigint => Ok("bigint".to_owned()),
            },
        };
    }

    match primitive {
        Primitive::Bool => Ok("boolean".to_owned()),
        primitive if primitive.is_integer() || primitive.is_float() => Ok("number".to_owned()),
        _ => unreachable!("all primitive variants are covered by bool, integer, or float"),
    }
}

fn collect_imports(type_id: TypeId, type_def: &TypeDef) -> BTreeSet<TypeId> {
    let mut imports = BTreeSet::new();
    collect_type_def_named_refs(type_def, &mut imports);
    imports.remove(&type_id);
    imports
}

fn collect_type_def_named_refs(type_def: &TypeDef, imports: &mut BTreeSet<TypeId>) {
    match type_def {
        TypeDef::Struct(def) => {
            for field in &def.fields {
                collect_type_ref_named_refs(&field.ty, imports);
            }
        }
        TypeDef::Enum(def) => {
            for variant in &def.variants {
                match &variant.shape {
                    VariantShape::Unit => {}
                    VariantShape::Newtype(ty) => collect_type_ref_named_refs(ty, imports),
                    VariantShape::Tuple(items) => {
                        for item in items {
                            collect_type_ref_named_refs(item, imports);
                        }
                    }
                    VariantShape::Struct(fields) => {
                        for field in fields {
                            collect_type_ref_named_refs(&field.ty, imports);
                        }
                    }
                }
            }
        }
    }
}

fn collect_type_ref_named_refs(ty: &TypeRef, imports: &mut BTreeSet<TypeId>) {
    match ty {
        TypeRef::Named(type_id) => {
            imports.insert(*type_id);
        }
        TypeRef::Option(inner) | TypeRef::Vec(inner) => collect_type_ref_named_refs(inner, imports),
        TypeRef::Array { item, .. } => collect_type_ref_named_refs(item, imports),
        TypeRef::Map { key, value } => {
            collect_type_ref_named_refs(key, imports);
            collect_type_ref_named_refs(value, imports);
        }
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes(_)
        | TypeRef::GenericParam(_)
        | TypeRef::Override(_) => {}
    }
}

fn field_is_emitted(field: &FieldDef) -> bool {
    !matches!(field.presence.serialize_presence, SerializePresence::Never)
}

fn unsupported_variant(def: &EnumDef, variant: &VariantDef) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(501), "unsupported enum variant shape")
        .with_type(def.export_name.clone())
        .with_variant(variant.rust_name.clone())
        .with_backend(BackendId::TypeScript)
}

fn type_name(type_def: &TypeDef) -> &str {
    match type_def {
        TypeDef::Struct(def) => &def.export_name,
        TypeDef::Enum(def) => &def.export_name,
    }
}

fn type_file_path(type_def: &TypeDef, config: &Config) -> String {
    let extension = match config.typescript.emit {
        TsEmit::Ts => "ts",
        TsEmit::Dts => "d.ts",
    };
    format!(
        "{}/{}.{}",
        config.typescript.out_dir.trim_end_matches('/'),
        to_snake_case(type_name(type_def)),
        extension
    )
}

fn module_specifier(type_def: &TypeDef, config: &Config) -> String {
    let mut specifier = format!("./{}", to_snake_case(type_name(type_def)));
    if matches!(
        config.typescript.import_extension,
        dto_bindgen_core::ImportExtension::Js
    ) {
        specifier.push_str(".js");
    }
    specifier
}

fn index_file_path(config: &Config) -> String {
    let extension = match config.typescript.emit {
        TsEmit::Ts => "ts",
        TsEmit::Dts => "d.ts",
    };
    format!(
        "{}/index.{}",
        config.typescript.out_dir.trim_end_matches('/'),
        extension
    )
}

fn render_index_file(registry: &Registry, config: &Config) -> String {
    let mut output = String::new();

    for type_def in registry.types_by_id.values() {
        output.push_str("export type { ");
        output.push_str(type_name(type_def));
        output.push_str(" } from \"");
        output.push_str(&module_specifier(type_def, config));
        output.push_str("\";\n");
    }

    output
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();

    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.extend(ch.to_lowercase());
        } else {
            output.push(ch);
        }
    }

    output
}

fn push_indent(output: &mut String, count: usize) {
    for _ in 0..count {
        output.push(' ');
    }
}

fn escape_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dto_bindgen_core::{
        FieldPresence, IdentName, RustTypeId, SourceSpan, TargetFieldNames, WireFieldNames,
    };

    fn span() -> SourceSpan {
        SourceSpan::new("src/dto.rs", 1, 1)
    }

    fn field(name: &str, wire: &str, ty: TypeRef) -> FieldDef {
        FieldDef::new(
            IdentName::new(name),
            WireFieldNames::same(wire),
            TargetFieldNames::new(wire, name),
            ty,
            span(),
        )
    }

    fn registry_with_types(types: impl IntoIterator<Item = (RustTypeId, TypeDef)>) -> Registry {
        let mut registry = Registry::new();
        for (rust_id, type_def) in types {
            registry.register_type(rust_id, type_def);
        }
        registry
    }

    #[test]
    fn identifies_backend() {
        assert_eq!(crate::backend_name(), "typescript");
        assert!(!crate::core_version().is_empty());
        assert_eq!(TypeScriptBackend::new().id(), BackendId::TypeScript);
    }

    #[test]
    fn renders_structs_and_fieldless_enums() {
        let user = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("UserProfile", "UserProfile", span())
                .with_field(field("user_id", "userId", TypeRef::String))
                .with_field(field(
                    "active",
                    "active",
                    TypeRef::Primitive(Primitive::Bool),
                )),
        );
        let role = TypeDef::Enum(
            EnumDef::new("UserRole", "UserRole", EnumRepr::External, span())
                .with_variant(VariantDef::new(
                    "Admin",
                    "admin",
                    VariantShape::Unit,
                    span(),
                ))
                .with_variant(VariantDef::new(
                    "GuestUser",
                    "guestUser",
                    VariantShape::Unit,
                    span(),
                )),
        );
        let registry = registry_with_types([
            (RustTypeId::new("sdk", "UserProfile"), user),
            (RustTypeId::new("sdk", "UserRole"), role),
        ]);

        let files = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap();

        assert_eq!(files.len(), 3);
        let user = find_file(&files, "user_profile.ts");
        let role = find_file(&files, "user_role.ts");
        let index = find_file(&files, "index.ts");
        assert!(user.contents().contains("export type UserProfile"));
        assert!(user.contents().contains("userId: string;"));
        assert!(
            role.contents()
                .contains("export type UserRole = \"admin\" | \"guestUser\";")
        );
        assert!(
            index
                .contents()
                .contains("export type { UserProfile } from \"./user_profile\";")
        );
        assert!(
            index
                .contents()
                .contains("export type { UserRole } from \"./user_role\";")
        );
    }

    #[test]
    fn renders_adjacent_tagged_enum_with_imports() {
        let mut registry = Registry::new();
        let user_id = registry.register_type(
            RustTypeId::new("sdk", "UserProfile"),
            TypeDef::Struct(
                dto_bindgen_core::StructDef::new("UserProfile", "UserProfile", span())
                    .with_field(field("user_id", "userId", TypeRef::String)),
            ),
        );
        let event = TypeDef::Enum(
            EnumDef::new(
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
                VariantShape::Struct(vec![
                    field("user", "user", TypeRef::named(user_id)),
                    field("event_id", "eventId", TypeRef::String),
                ]),
                span(),
            )),
        );
        let event_id = registry.register_type(RustTypeId::new("sdk", "SdkEvent"), event);
        registry.add_dependency(event_id, user_id);

        let files = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let event_file = files
            .files()
            .iter()
            .find(|file| file.relative_path().as_str().ends_with("sdk_event.ts"))
            .unwrap();

        assert!(
            event_file
                .contents()
                .contains("import type { UserProfile }")
        );
        assert!(event_file.contents().contains("type: \"userCreated\";"));
        assert!(event_file.contents().contains("payload: {"));
        assert!(event_file.contents().contains("user: UserProfile;"));
        assert!(event_file.contents().contains("eventId: string;"));
    }

    #[test]
    fn maps_large_integer_overrides() {
        let amount = field(
            "amount_minor_units",
            "amountMinorUnits",
            TypeRef::Primitive(Primitive::U128),
        )
        .with_int_repr(IntRepr::JsonString);
        let unsafe_number = field("sequence", "sequence", TypeRef::Primitive(Primitive::U64))
            .with_int_repr(IntRepr::JsonNumberUnsafe);
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("LedgerEntry", "LedgerEntry", span())
                .with_field(amount)
                .with_field(unsafe_number),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "LedgerEntry"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let contents = find_file(&files, "ledger_entry.ts").contents();

        assert!(contents.contains("amountMinorUnits: string;"));
        assert!(contents.contains("sequence: number;"));
    }

    #[test]
    fn renders_optional_nullable_fields() {
        let field = field(
            "display_name",
            "displayName",
            TypeRef::option(TypeRef::String),
        )
        .with_presence(FieldPresence::defaulted(
            dto_bindgen_core::DefaultKind::NoneValue,
        ));
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("ProfilePatch", "ProfilePatch", span())
                .with_field(field),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "ProfilePatch"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let contents = find_file(&files, "profile_patch.ts").contents();

        assert!(contents.contains("displayName?: string | null;"));
    }

    #[test]
    fn honors_js_import_extension_in_imports_and_index() {
        let mut config = Config::default();
        config.typescript.import_extension = dto_bindgen_core::ImportExtension::Js;

        let mut registry = Registry::new();
        let user_id = registry.register_type(
            RustTypeId::new("sdk", "UserProfile"),
            TypeDef::Struct(dto_bindgen_core::StructDef::new(
                "UserProfile",
                "UserProfile",
                span(),
            )),
        );
        let event =
            TypeDef::Struct(
                dto_bindgen_core::StructDef::new("UserEvent", "UserEvent", span())
                    .with_field(field("user", "user", TypeRef::named(user_id))),
            );
        registry.register_type(RustTypeId::new("sdk", "UserEvent"), event);

        let files = TypeScriptBackend::new().render(&registry, &config).unwrap();
        let event = find_file(&files, "user_event.ts");
        let index = find_file(&files, "index.ts");

        assert!(
            event
                .contents()
                .contains("import type { UserProfile } from \"./user_profile.js\";")
        );
        assert!(
            index
                .contents()
                .contains("export type { UserProfile } from \"./user_profile.js\";")
        );
    }

    fn find_file<'a>(
        files: &'a GeneratedFileSet,
        suffix: &str,
    ) -> &'a dto_bindgen_core::GeneratedFile {
        files
            .files()
            .iter()
            .find(|file| file.relative_path().as_str().ends_with(suffix))
            .unwrap()
    }
}
