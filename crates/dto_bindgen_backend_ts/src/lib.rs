#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, BTreeSet};

use dto_bindgen_core::{
    Backend, BackendCapabilities, BackendError, BackendId, BytesRepr, Config, Diagnostic,
    DiagnosticCode, EnumDef, EnumRepr, FieldDef, GeneratedFile, GeneratedFileSet, IntRepr,
    Primitive, Registry, StructDef, TsEmit, TypeDef, TypeId, TypeRef, TypeScriptLayout, VariantDef,
    VariantShape, validate_registry_for_backend,
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

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::typescript()
    }

    fn validate(&self, registry: &Registry, config: &Config) -> Vec<Diagnostic> {
        if !config.typescript.enabled {
            return Vec::new();
        }
        let mut diagnostics = validate_registry_for_backend(registry, config, &self.capabilities());
        diagnostics.extend(validate_type_names(registry));
        diagnostics
    }

    fn render(
        &self,
        registry: &Registry,
        config: &Config,
    ) -> Result<GeneratedFileSet, BackendError> {
        if !config.typescript.enabled {
            return Ok(GeneratedFileSet::empty());
        }

        let files = match config.typescript.layout {
            TypeScriptLayout::Bundle => render_bundle_files(registry, config)?,
            TypeScriptLayout::PerType => render_per_type_files(registry, config)?,
        };

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

fn render_per_type_files(
    registry: &Registry,
    config: &Config,
) -> Result<Vec<GeneratedFile>, BackendError> {
    let mut files = Vec::new();
    for (type_id, type_def) in sorted_registry_types(registry) {
        let contents =
            render_type_file(type_id, type_def, registry, config).map_err(|diagnostic| {
                BackendError::from_diagnostic(BackendId::TypeScript, diagnostic)
            })?;
        files.push(generated_file(type_file_path(type_def, config), contents)?);
    }

    if !registry.types_by_id.is_empty() {
        files.push(generated_file(
            index_file_path(config),
            render_index_file(registry, config),
        )?);
    }

    Ok(files)
}

fn render_bundle_files(
    registry: &Registry,
    config: &Config,
) -> Result<Vec<GeneratedFile>, BackendError> {
    if registry.types_by_id.is_empty() {
        return Ok(Vec::new());
    }

    let contents = render_bundle_file(registry, config)
        .map_err(|diagnostic| BackendError::from_diagnostic(BackendId::TypeScript, diagnostic))?;

    Ok(vec![
        generated_file(bundle_file_path(config), contents)?,
        generated_file(index_file_path(config), render_index_file(registry, config))?,
    ])
}

fn generated_file(path: String, contents: String) -> Result<GeneratedFile, BackendError> {
    GeneratedFile::new(BackendId::TypeScript, path, contents).map_err(|err| {
        BackendError::from_diagnostic(
            BackendId::TypeScript,
            Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                .with_backend(BackendId::TypeScript),
        )
    })
}

fn sorted_registry_types(registry: &Registry) -> Vec<(TypeId, &TypeDef)> {
    let mut types = registry
        .types_by_id
        .iter()
        .map(|(type_id, type_def)| (*type_id, type_def))
        .collect::<Vec<_>>();
    types.sort_by(|(left_id, left), (right_id, right)| {
        type_name(left)
            .cmp(type_name(right))
            .then_with(|| diagnostic_type_name(left).cmp(diagnostic_type_name(right)))
            .then_with(|| left_id.cmp(right_id))
    });
    types
}

fn render_bundle_file(registry: &Registry, config: &Config) -> Result<String, Diagnostic> {
    let mut output = String::new();

    for (index, (_, type_def)) in sorted_registry_types(registry).into_iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }

        match type_def {
            TypeDef::Struct(def) => render_struct(def, registry, config, &mut output)?,
            TypeDef::Enum(def) => render_enum(def, registry, config, &mut output)?,
        }
    }

    Ok(output)
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
    output.push_str(struct_type_name(def));
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
    output.push_str(enum_type_name(def));
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
    output.push_str(enum_type_name(def));
    output.push_str(" =\n");

    for variant in &def.variants {
        output.push_str("  | {\n");
        push_indent(output, 6);
        push_object_key(output, tag);
        output.push_str(": \"");
        output.push_str(&escape_string_literal(&variant.wire_name));
        output.push_str("\";\n");

        let VariantShape::Struct(fields) = &variant.shape else {
            return Err(unsupported_variant(def, variant));
        };

        if let Some(content) = content {
            push_indent(output, 6);
            push_object_key(output, content);
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
    let contract = field.contract();
    push_indent(output, indent);
    push_object_key(output, &field.wire.serialize_name);
    if !contract.required {
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
        TypeRef::Bytes(BytesRepr::Base64String) => Ok("string".to_owned()),
        TypeRef::Bytes(BytesRepr::Bytes) => Err(Diagnostic::error(
            DiagnosticCode::new(402),
            "raw bytes are unsupported for JSON DTO exchange",
        )
        .with_field(field.rust_name.to_string())
        .with_backend(BackendId::TypeScript)),
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
            Some(IntRepr::JsonNumber) => Ok("number".to_owned()),
            None => match config.numeric.large_int_policy {
                dto_bindgen_core::LargeIntPolicy::RequireExplicit => Err(Diagnostic::error(
                    DiagnosticCode::new(401),
                    "large integer field requires explicit numeric policy",
                )
                .with_field(field.rust_name.to_string())
                .with_backend(BackendId::TypeScript)),
                dto_bindgen_core::LargeIntPolicy::JsonString => Ok("string".to_owned()),
                dto_bindgen_core::LargeIntPolicy::JsonNumber => Ok("number".to_owned()),
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
    field.contract().serialized
}

fn unsupported_variant(def: &EnumDef, variant: &VariantDef) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(501), "unsupported enum variant shape")
        .with_type(def.export_name.clone())
        .with_variant(variant.rust_name.clone())
        .with_backend(BackendId::TypeScript)
}

fn validate_type_names(registry: &Registry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeMap::<String, String>::new();

    for type_def in registry.types_by_id.values() {
        let name = type_name(type_def);
        let diagnostic_name = diagnostic_type_name(type_def);
        if !is_valid_type_identifier(name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::new(502),
                    format!("invalid TypeScript type name `{name}`"),
                )
                .with_help(
                    "Use #[dto(ts(name = \"ValidIdentifier\"))] to provide a valid TypeScript type name.",
                )
                .with_type(diagnostic_name.to_owned())
                .with_backend(BackendId::TypeScript),
            );
        }
        if let Some(first_type) = seen.insert(name.to_owned(), diagnostic_name.to_owned()) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::new(503),
                    format!("duplicate TypeScript type name `{name}`"),
                )
                .with_help(format!(
                    "Types `{first_type}` and `{diagnostic_name}` resolve to the same TypeScript name."
                ))
                .with_type(diagnostic_name.to_owned())
                .with_backend(BackendId::TypeScript),
            );
        }
    }

    diagnostics
}

fn type_name(type_def: &TypeDef) -> &str {
    match type_def {
        TypeDef::Struct(def) => struct_type_name(def),
        TypeDef::Enum(def) => enum_type_name(def),
    }
}

fn struct_type_name(def: &StructDef) -> &str {
    def.attrs.ts_name.as_deref().unwrap_or(&def.export_name)
}

fn enum_type_name(def: &EnumDef) -> &str {
    def.attrs.ts_name.as_deref().unwrap_or(&def.export_name)
}

fn diagnostic_type_name(type_def: &TypeDef) -> &str {
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

fn bundle_file_path(config: &Config) -> String {
    format!(
        "{}/{}",
        config.typescript.out_dir.trim_end_matches('/'),
        config.typescript.bundle_file
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

fn bundle_module_specifier(config: &Config) -> String {
    let file = config
        .typescript
        .bundle_file
        .strip_suffix(".d.ts")
        .or_else(|| config.typescript.bundle_file.strip_suffix(".ts"))
        .unwrap_or(config.typescript.bundle_file.as_str());
    let mut specifier = format!("./{file}");
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

    match config.typescript.layout {
        TypeScriptLayout::Bundle => {
            output.push_str("export type { ");
            for (index, (_, type_def)) in sorted_registry_types(registry).into_iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(type_name(type_def));
            }
            output.push_str(" } from \"");
            output.push_str(&bundle_module_specifier(config));
            output.push_str("\";\n");
        }
        TypeScriptLayout::PerType => {
            for (_, type_def) in sorted_registry_types(registry) {
                output.push_str("export type { ");
                output.push_str(type_name(type_def));
                output.push_str(" } from \"");
                output.push_str(&module_specifier(type_def, config));
                output.push_str("\";\n");
            }
        }
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

fn push_object_key(output: &mut String, value: &str) {
    if is_valid_property_identifier(value) {
        output.push_str(value);
    } else {
        output.push('"');
        output.push_str(&escape_string_literal(value));
        output.push('"');
    }
}

fn is_valid_type_identifier(value: &str) -> bool {
    is_valid_identifier(value) && !is_reserved_type_identifier(value)
}

fn is_valid_property_identifier(value: &str) -> bool {
    is_valid_identifier(value) && !is_reserved_type_identifier(value)
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start(first) {
        return false;
    }
    chars.all(is_identifier_part)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_part(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

fn is_reserved_type_identifier(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "any"
            | "as"
            | "asserts"
            | "async"
            | "await"
            | "bigint"
            | "boolean"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constructor"
            | "continue"
            | "debugger"
            | "declare"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "from"
            | "function"
            | "get"
            | "global"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "infer"
            | "instanceof"
            | "interface"
            | "is"
            | "keyof"
            | "let"
            | "module"
            | "namespace"
            | "never"
            | "new"
            | "null"
            | "number"
            | "object"
            | "of"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "require"
            | "return"
            | "set"
            | "static"
            | "string"
            | "super"
            | "switch"
            | "symbol"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "undefined"
            | "unique"
            | "unknown"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

fn escape_string_literal(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\u{2028}' => output.push_str("\\u2028"),
            '\u{2029}' => output.push_str("\\u2029"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                write!(&mut output, "\\u{:04x}", ch as u32)
                    .expect("writing to a String cannot fail");
            }
            ch => output.push(ch),
        }
    }
    output
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

    fn per_type_config() -> Config {
        let mut config = Config::default();
        config.typescript.layout = TypeScriptLayout::PerType;
        config
    }

    #[test]
    fn identifies_backend() {
        assert_eq!(crate::backend_name(), "typescript");
        assert!(!crate::core_version().is_empty());
        assert_eq!(TypeScriptBackend::new().id(), BackendId::TypeScript);
    }

    #[test]
    fn renders_default_bundle_and_type_only_index() {
        let user = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("UserProfile", "UserProfile", span())
                .with_field(field("user_id", "userId", TypeRef::String)),
        );
        let role = TypeDef::Enum(
            EnumDef::new("UserRole", "UserRole", EnumRepr::External, span()).with_variant(
                VariantDef::new("Admin", "admin", VariantShape::Unit, span()),
            ),
        );
        let registry = registry_with_types([
            (RustTypeId::new("sdk", "sdk", "UserRole"), role),
            (RustTypeId::new("sdk", "sdk", "UserProfile"), user),
        ]);

        let files = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap();

        assert_eq!(files.len(), 2);
        let bundle = find_file(&files, "types.ts");
        let index = find_file(&files, "index.ts");
        assert!(bundle.contents().contains("export type UserProfile = {"));
        assert!(bundle.contents().contains("userId: string;"));
        assert!(
            bundle
                .contents()
                .contains("export type UserRole = \"admin\";")
        );
        assert!(
            index
                .contents()
                .contains("export type { UserProfile, UserRole } from \"./types\";")
        );
        assert!(
            !files
                .files()
                .iter()
                .any(|file| file.relative_path().as_str().ends_with("user_profile.ts"))
        );
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
            (RustTypeId::new("sdk", "sdk", "UserProfile"), user),
            (RustTypeId::new("sdk", "sdk", "UserRole"), role),
        ]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
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
            RustTypeId::new("sdk", "sdk", "UserProfile"),
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
        let event_id = registry.register_type(RustTypeId::new("sdk", "sdk", "SdkEvent"), event);
        registry.add_dependency(event_id, user_id);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
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
        assert!(event_file.contents().contains("\"type\": \"userCreated\";"));
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
        let json_number = field("sequence", "sequence", TypeRef::Primitive(Primitive::U64))
            .with_int_repr(IntRepr::JsonNumber);
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("LedgerEntry", "LedgerEntry", span())
                .with_field(amount)
                .with_field(json_number),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "LedgerEntry"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let contents = find_file(&files, "ledger_entry.ts").contents();

        assert!(contents.contains("amountMinorUnits: string;"));
        assert!(contents.contains("sequence: number;"));
    }

    #[test]
    fn renders_base64_bytes_as_strings() {
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("Attachment", "Attachment", span()).with_field(field(
                "payload",
                "payload",
                TypeRef::Bytes(BytesRepr::Base64String),
            )),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "Attachment"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let contents = find_file(&files, "attachment.ts").contents();

        assert!(contents.contains("payload: string;"));
    }

    #[test]
    fn rejects_raw_bytes_for_json_exchange() {
        let def =
            TypeDef::Struct(
                dto_bindgen_core::StructDef::new("Attachment", "Attachment", span()).with_field(
                    field("payload", "payload", TypeRef::Bytes(BytesRepr::Bytes)),
                ),
            );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "Attachment"), def)]);

        let err = TypeScriptBackend::new()
            .render(&registry, &Config::default())
            .unwrap_err();

        assert_eq!(err.diagnostics()[0].code, DiagnosticCode::new(402));
        assert!(
            err.diagnostics()[0]
                .message
                .contains("raw bytes are unsupported")
        );
    }

    #[test]
    fn renders_json_exchange_field_contracts() {
        let display_name = field(
            "display_name",
            "displayName",
            TypeRef::option(TypeRef::String),
        )
        .with_presence(FieldPresence::optional_nullable());
        let nickname = field("nickname", "nickname", TypeRef::option(TypeRef::String))
            .with_presence(FieldPresence::optional_nullable_skip_if_none());
        let tags = field("tags", "tags", TypeRef::vec(TypeRef::String)).with_presence(
            FieldPresence::defaulted(dto_bindgen_core::DefaultKind::EmptyVec),
        );
        let internal_note = field("internal_note", "internalNote", TypeRef::String)
            .with_presence(FieldPresence::skipped());
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("ProfilePatch", "ProfilePatch", span())
                .with_field(display_name)
                .with_field(nickname)
                .with_field(tags)
                .with_field(internal_note),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "ProfilePatch"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let contents = find_file(&files, "profile_patch.ts").contents();

        assert!(contents.contains("displayName?: string | null;"));
        assert!(contents.contains("nickname?: string | null;"));
        assert!(contents.contains("tags?: Array<string>;"));
        assert!(!contents.contains("internalNote"));
    }

    #[test]
    fn quotes_unsafe_object_property_names() {
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("AssetEntry", "AssetEntry", span())
                .with_field(field("content_type", "content-type", TypeRef::String))
                .with_field(field("dot_key", "metadata.hash", TypeRef::String))
                .with_field(field("numeric_key", "123value", TypeRef::String))
                .with_field(field("class_name", "class", TypeRef::String))
                .with_field(field("line_break", "line\nbreak", TypeRef::String)),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "AssetEntry"), def)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let contents = find_file(&files, "asset_entry.ts").contents();

        assert!(contents.contains("\"content-type\": string;"));
        assert!(contents.contains("\"metadata.hash\": string;"));
        assert!(contents.contains("\"123value\": string;"));
        assert!(contents.contains("\"class\": string;"));
        assert!(contents.contains("\"line\\nbreak\": string;"));
    }

    #[test]
    fn escapes_typescript_string_literals() {
        let role = TypeDef::Enum(
            EnumDef::new("UserRole", "UserRole", EnumRepr::External, span())
                .with_variant(VariantDef::new(
                    "Quoted",
                    "admin\"root",
                    VariantShape::Unit,
                    span(),
                ))
                .with_variant(VariantDef::new(
                    "Tabbed",
                    "guest\tuser",
                    VariantShape::Unit,
                    span(),
                ))
                .with_variant(VariantDef::new(
                    "LineSeparated",
                    "line\u{2028}sep",
                    VariantShape::Unit,
                    span(),
                )),
        );
        let registry = registry_with_types([(RustTypeId::new("sdk", "sdk", "UserRole"), role)]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let contents = find_file(&files, "user_role.ts").contents();

        assert!(contents.contains("\"admin\\\"root\""));
        assert!(contents.contains("\"guest\\tuser\""));
        assert!(contents.contains("\"line\\u2028sep\""));
    }

    #[test]
    fn honors_typescript_target_name_overrides() {
        let mut def = dto_bindgen_core::StructDef::new("Manifest", "Manifest", span());
        def.attrs.ts_name = Some("Mf2WebManifest".to_owned());
        let registry = registry_with_types([(
            RustTypeId::new("sdk", "sdk", "Manifest"),
            TypeDef::Struct(def),
        )]);

        let files = TypeScriptBackend::new()
            .render(&registry, &per_type_config())
            .unwrap();
        let manifest = find_file(&files, "mf2_web_manifest.ts");
        let index = find_file(&files, "index.ts");

        assert!(manifest.contents().contains("export type Mf2WebManifest"));
        assert!(
            index
                .contents()
                .contains("export type { Mf2WebManifest } from \"./mf2_web_manifest\";")
        );
    }

    #[test]
    fn rejects_invalid_and_duplicate_typescript_target_names() {
        let mut first = dto_bindgen_core::StructDef::new("BuildManifest", "BuildManifest", span());
        first.attrs.ts_name = Some("Mf2WebManifest".to_owned());
        let mut duplicate =
            dto_bindgen_core::StructDef::new("RuntimeManifest", "RuntimeManifest", span());
        duplicate.attrs.ts_name = Some("Mf2WebManifest".to_owned());
        let mut invalid = dto_bindgen_core::StructDef::new("BadName", "BadName", span());
        invalid.attrs.ts_name = Some("bad-name".to_owned());
        let mut reserved = dto_bindgen_core::StructDef::new("ClassName", "ClassName", span());
        reserved.attrs.ts_name = Some("class".to_owned());
        let registry = registry_with_types([
            (
                RustTypeId::new("sdk", "sdk", "BuildManifest"),
                TypeDef::Struct(first),
            ),
            (
                RustTypeId::new("sdk", "sdk", "RuntimeManifest"),
                TypeDef::Struct(duplicate),
            ),
            (
                RustTypeId::new("sdk", "sdk", "BadName"),
                TypeDef::Struct(invalid),
            ),
            (
                RustTypeId::new("sdk", "sdk", "ClassName"),
                TypeDef::Struct(reserved),
            ),
        ]);

        let diagnostics = TypeScriptBackend::new().validate(&registry, &Config::default());
        let codes = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&DiagnosticCode::new(502)));
        assert!(codes.contains(&DiagnosticCode::new(503)));
    }

    #[test]
    fn honors_js_import_extension_in_imports_and_index() {
        let mut config = per_type_config();
        config.typescript.import_extension = dto_bindgen_core::ImportExtension::Js;

        let mut registry = Registry::new();
        let user_id = registry.register_type(
            RustTypeId::new("sdk", "sdk", "UserProfile"),
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
        registry.register_type(RustTypeId::new("sdk", "sdk", "UserEvent"), event);

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
