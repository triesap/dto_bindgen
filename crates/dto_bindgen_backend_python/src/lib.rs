#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use dto_bindgen_core::{
    Backend, BackendError, BackendId, Config, DefaultKind, Diagnostic, DiagnosticCode, EnumDef,
    EnumRepr, FieldDef, GeneratedFile, GeneratedFileSet, IntRepr, Primitive, Registry,
    SerializePresence, StructDef, TypeDef, TypeId, TypeRef, VariantShape,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PythonBackend;

impl PythonBackend {
    pub const fn new() -> Self {
        Self
    }
}

impl Backend for PythonBackend {
    fn id(&self) -> BackendId {
        BackendId::Python
    }

    fn render(
        &self,
        registry: &Registry,
        config: &Config,
    ) -> Result<GeneratedFileSet, BackendError> {
        if !config.python.enabled {
            return Ok(GeneratedFileSet::empty());
        }

        let mut files = Vec::new();
        for (type_id, type_def) in &registry.types_by_id {
            let contents =
                render_type_file(*type_id, type_def, registry, config).map_err(|diagnostic| {
                    BackendError::from_diagnostic(BackendId::Python, diagnostic)
                })?;
            files.push(generated_file(type_file_path(type_def, config), contents)?);
        }

        if !registry.types_by_id.is_empty() {
            files.push(generated_file(
                package_file_path("__init__.py", config),
                render_init_file(registry),
            )?);
            files.push(generated_file(
                package_file_path("errors.py", config),
                render_errors_file(),
            )?);
            if config.python.emit_py_typed {
                files.push(generated_file(package_file_path("py.typed", config), "")?);
            }
        }

        GeneratedFileSet::try_from_files(files).map_err(|err| {
            BackendError::from_diagnostic(
                BackendId::Python,
                Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                    .with_backend(BackendId::Python),
            )
        })
    }
}

pub fn backend_name() -> &'static str {
    "python"
}

pub fn core_version() -> &'static str {
    dto_bindgen_core::VERSION
}

fn generated_file(
    path: String,
    contents: impl Into<String>,
) -> Result<GeneratedFile, BackendError> {
    GeneratedFile::new(BackendId::Python, path, contents).map_err(|err| {
        BackendError::from_diagnostic(
            BackendId::Python,
            Diagnostic::error(DiagnosticCode::new(701), err.to_string())
                .with_backend(BackendId::Python),
        )
    })
}

fn render_type_file(
    type_id: TypeId,
    type_def: &TypeDef,
    registry: &Registry,
    config: &Config,
) -> Result<String, Diagnostic> {
    let mut output = String::new();
    output.push_str("from __future__ import annotations\n\n");

    match type_def {
        TypeDef::Struct(_) => output.push_str(
            "from dataclasses import dataclass, field\nfrom typing import Any\n\nfrom .errors import DtoParseError\n",
        ),
        TypeDef::Enum(def) if is_fieldless_enum(def) => {
            output.push_str("from enum import StrEnum\n");
        }
        TypeDef::Enum(def) if is_tagged_enum(def) => output.push_str(
            "from dataclasses import dataclass, field\nfrom typing import Any, TypeAlias\n\nfrom .errors import DtoParseError\n",
        ),
        TypeDef::Enum(def) => {
            return Err(Diagnostic::error(
                DiagnosticCode::new(601),
                "unsupported Python enum representation",
            )
            .with_type(def.export_name.clone())
            .with_backend(BackendId::Python));
        }
    }

    let imports = collect_imports(type_id, type_def, registry);
    for (dependency, import) in imports {
        let dependency_def = registry.type_def(dependency).ok_or_else(|| {
            Diagnostic::error(DiagnosticCode::new(102), "missing named dependency")
                .with_backend(BackendId::Python)
        })?;
        output.push_str("\nfrom .");
        output.push_str(&module_name(dependency_def));
        output.push_str(" import ");
        let mut names = vec![type_name(dependency_def).to_owned()];
        if import.parser {
            names.push(parser_name(dependency_def));
        }
        output.push_str(&names.join(", "));
        output.push('\n');
    }

    output.push('\n');

    match type_def {
        TypeDef::Struct(def) => render_struct(def, registry, config, &mut output)?,
        TypeDef::Enum(def) if is_fieldless_enum(def) => render_fieldless_enum(def, &mut output),
        TypeDef::Enum(def) => render_tagged_enum(def, registry, config, &mut output)?,
    }

    Ok(output)
}

fn render_struct(
    def: &StructDef,
    registry: &Registry,
    config: &Config,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("@dataclass(");
    output.push_str(&format!(
        "frozen={}, slots={}, kw_only={}",
        py_bool(config.python.frozen),
        py_bool(config.python.slots),
        py_bool(config.python.kw_only)
    ));
    output.push_str(")\nclass ");
    output.push_str(&def.export_name);
    output.push_str(":\n");

    let fields = def
        .fields
        .iter()
        .filter(|field| field_is_emitted(field))
        .collect::<Vec<_>>();

    if fields.is_empty() {
        output.push_str("    pass\n");
    } else {
        for field in &fields {
            render_dataclass_field(field, registry, output)?;
        }
    }

    if config.python.emit_from_dict {
        render_from_dict(def, &fields, registry, output)?;
    }
    if config.python.emit_to_dict {
        render_to_dict(def, &fields, registry, output)?;
    }

    Ok(())
}

fn render_from_dict(
    def: &StructDef,
    fields: &[&FieldDef],
    registry: &Registry,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("\n    @classmethod\n");
    output.push_str("    def from_dict(cls, data: dict[str, Any]) -> \"");
    output.push_str(&def.export_name);
    output.push_str("\":\n");
    output.push_str("        try:\n");

    if def.attrs.deny_unknown_fields {
        output.push_str("            unknown = set(data) - {");
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push('"');
            output.push_str(&escape_py_string(&field.wire.deserialize_name));
            output.push('"');
        }
        output.push_str("}\n");
        output.push_str("            if unknown:\n");
        output
            .push_str("                raise ValueError(f\"unknown fields: {sorted(unknown)}\")\n");
    }

    output.push_str("            return cls(\n");
    for field in fields {
        output.push_str("                ");
        output.push_str(&field.target.python);
        output.push('=');
        output.push_str(&parse_expr(
            field,
            registry,
            &field_access_expr(field, "data"),
        )?);
        output.push_str(",\n");
    }
    output.push_str("            )\n");
    output.push_str("        except Exception as exc:\n");
    output.push_str("            raise DtoParseError(path=\"");
    output.push_str(&escape_py_string(&def.export_name));
    output.push_str("\", cause=exc) from exc\n");
    Ok(())
}

fn render_to_dict(
    _def: &StructDef,
    fields: &[&FieldDef],
    registry: &Registry,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("\n    def to_dict(self) -> dict[str, Any]:\n");
    output.push_str("        output = {}\n");
    render_dict_field_assignments("output", fields, registry, "self", "        ", output)?;
    output.push_str("        return output\n");
    Ok(())
}

fn render_dict_field_assignments(
    target: &str,
    fields: &[&FieldDef],
    registry: &Registry,
    access_prefix: &str,
    indent: &str,
    output: &mut String,
) -> Result<(), Diagnostic> {
    for field in fields {
        let access = format!("{access_prefix}.{}", field.target.python);
        if field.presence.serialize_presence == SerializePresence::SkipIfNone {
            output.push_str(indent);
            output.push_str("if ");
            output.push_str(&access);
            output.push_str(" is not None:\n");
            output.push_str(indent);
            output.push_str("    ");
        } else {
            output.push_str(indent);
        }
        output.push_str(target);
        output.push_str("[\"");
        output.push_str(&escape_py_string(&field.wire.serialize_name));
        output.push_str("\"] = ");
        output.push_str(&serialize_expr(field, registry, &access)?);
        output.push('\n');
    }
    Ok(())
}

fn render_fieldless_enum(def: &EnumDef, output: &mut String) {
    output.push_str("class ");
    output.push_str(&def.export_name);
    output.push_str("(StrEnum):\n");

    if def.variants.is_empty() {
        output.push_str("    pass\n");
        return;
    }

    for variant in &def.variants {
        output.push_str("    ");
        output.push_str(&enum_member_name(&variant.rust_name));
        output.push_str(" = \"");
        output.push_str(&escape_py_string(&variant.wire_name));
        output.push_str("\"\n");
    }
}

fn render_tagged_enum(
    def: &EnumDef,
    registry: &Registry,
    config: &Config,
    output: &mut String,
) -> Result<(), Diagnostic> {
    let (tag, content) = match &def.repr {
        EnumRepr::Internal { tag } => (tag.as_str(), None),
        EnumRepr::Adjacent { tag, content } => (tag.as_str(), Some(content.as_str())),
        EnumRepr::External | EnumRepr::Untagged => {
            return Err(Diagnostic::error(
                DiagnosticCode::new(601),
                "unsupported Python enum representation",
            )
            .with_type(def.export_name.clone())
            .with_backend(BackendId::Python));
        }
    };

    let mut variant_class_names = Vec::new();
    for variant in &def.variants {
        let VariantShape::Struct(fields) = &variant.shape else {
            return Err(Diagnostic::error(
                DiagnosticCode::new(601),
                "unsupported Python enum variant shape",
            )
            .with_type(def.export_name.clone())
            .with_variant(variant.rust_name.clone())
            .with_backend(BackendId::Python));
        };

        let class_name = format!("{}{}", def.export_name, variant.rust_name);
        variant_class_names.push(class_name.clone());
        let fields = fields
            .iter()
            .filter(|field| field_is_emitted(field))
            .collect::<Vec<_>>();

        output.push_str("@dataclass(");
        output.push_str(&format!(
            "frozen={}, slots={}, kw_only={}",
            py_bool(config.python.frozen),
            py_bool(config.python.slots),
            py_bool(config.python.kw_only)
        ));
        output.push_str(")\nclass ");
        output.push_str(&class_name);
        output.push_str(":\n");

        if fields.is_empty() {
            output.push_str("    pass\n");
        } else {
            for field in &fields {
                render_dataclass_field(field, registry, output)?;
            }
        }

        output.push_str("\n    def to_dict(self) -> dict[str, Any]:\n");
        output.push_str("        output = {\n");
        output.push_str("            \"");
        output.push_str(&escape_py_string(tag));
        output.push_str("\": \"");
        output.push_str(&escape_py_string(&variant.wire_name));
        output.push_str("\",\n");
        output.push_str("        }\n");
        if let Some(content) = content {
            output.push_str("        payload = {}\n");
            render_dict_field_assignments(
                "payload", &fields, registry, "self", "        ", output,
            )?;
            output.push_str("        output[\"");
            output.push_str(&escape_py_string(content));
            output.push_str("\"] = payload\n");
        } else {
            render_dict_field_assignments("output", &fields, registry, "self", "        ", output)?;
        }
        output.push_str("        return output\n\n");
    }

    output.push_str(def.export_name.as_str());
    output.push_str(": TypeAlias = ");
    output.push_str(&variant_class_names.join(" | "));
    output.push_str("\n\n");

    output.push_str("def ");
    output.push_str(&parser_name(&TypeDef::Enum(def.clone())));
    output.push_str("(data: dict[str, Any]) -> ");
    output.push_str(&def.export_name);
    output.push_str(":\n");
    output.push_str("    try:\n");
    output.push_str("        tag = data[\"");
    output.push_str(&escape_py_string(tag));
    output.push_str("\"]\n");

    for variant in &def.variants {
        let VariantShape::Struct(fields) = &variant.shape else {
            continue;
        };
        let class_name = format!("{}{}", def.export_name, variant.rust_name);
        let payload_name = if content.is_some() { "payload" } else { "data" };
        output.push_str("        if tag == \"");
        output.push_str(&escape_py_string(&variant.wire_name));
        output.push_str("\":\n");
        if let Some(content) = content {
            output.push_str("            payload = data[\"");
            output.push_str(&escape_py_string(content));
            output.push_str("\"]\n");
        }
        output.push_str("            return ");
        output.push_str(&class_name);
        output.push_str("(\n");
        for field in fields.iter().filter(|field| field_is_emitted(field)) {
            output.push_str("                ");
            output.push_str(&field.target.python);
            output.push('=');
            output.push_str(&parse_expr(
                field,
                registry,
                &field_access_expr(field, payload_name),
            )?);
            output.push_str(",\n");
        }
        output.push_str("            )\n");
    }

    output.push_str("        raise ValueError(f\"unknown tag: {tag}\")\n");
    output.push_str("    except Exception as exc:\n");
    output.push_str("        raise DtoParseError(path=\"");
    output.push_str(&escape_py_string(&def.export_name));
    output.push_str("\", cause=exc) from exc\n");
    Ok(())
}

fn render_dataclass_field(
    field: &FieldDef,
    registry: &Registry,
    output: &mut String,
) -> Result<(), Diagnostic> {
    output.push_str("    ");
    output.push_str(&field.target.python);
    output.push_str(": ");
    output.push_str(&render_type_ref(&field.ty, registry, field)?);
    output.push_str(" = ");
    output.push_str(&field_call(field));
    output.push('\n');
    Ok(())
}

fn field_call(field: &FieldDef) -> String {
    let metadata = format!(
        "metadata={{\"wire_name\": \"{}\"}}",
        escape_py_string(&field.wire.serialize_name)
    );

    match field.presence.default.as_ref() {
        None => format!("field({metadata})"),
        Some(DefaultKind::NoneValue) => format!("field(default=None, {metadata})"),
        Some(DefaultKind::EmptyString) => format!("field(default=\"\", {metadata})"),
        Some(DefaultKind::EmptyVec) => format!("field(default_factory=list, {metadata})"),
        Some(DefaultKind::EmptyMap) => format!("field(default_factory=dict, {metadata})"),
        Some(DefaultKind::BoolFalse) => format!("field(default=False, {metadata})"),
        Some(DefaultKind::NumericZero) => format!("field(default=0, {metadata})"),
        Some(DefaultKind::CustomPath(_)) => format!("field({metadata})"),
    }
}

fn field_access_expr(field: &FieldDef, source: &str) -> String {
    let wire_name = escape_py_string(&field.wire.deserialize_name);
    if field.presence.required_on_deserialize {
        return format!("{source}[\"{wire_name}\"]");
    }

    match field.presence.default.as_ref() {
        None | Some(DefaultKind::NoneValue) => format!("{source}.get(\"{wire_name}\")"),
        Some(DefaultKind::EmptyString) => format!("{source}.get(\"{wire_name}\", \"\")"),
        Some(DefaultKind::EmptyVec) => format!("{source}.get(\"{wire_name}\", [])"),
        Some(DefaultKind::EmptyMap) => format!("{source}.get(\"{wire_name}\", {{}})"),
        Some(DefaultKind::BoolFalse) => format!("{source}.get(\"{wire_name}\", False)"),
        Some(DefaultKind::NumericZero) => format!("{source}.get(\"{wire_name}\", 0)"),
        Some(DefaultKind::CustomPath(_)) => format!("{source}.get(\"{wire_name}\")"),
    }
}

fn parse_expr(field: &FieldDef, registry: &Registry, access: &str) -> Result<String, Diagnostic> {
    parse_type_expr(&field.ty, field.int_repr, registry, access, field)
}

fn serialize_expr(
    field: &FieldDef,
    registry: &Registry,
    access: &str,
) -> Result<String, Diagnostic> {
    serialize_type_expr(&field.ty, field.int_repr, registry, access, field)
}

fn parse_type_expr(
    ty: &TypeRef,
    int_repr: Option<IntRepr>,
    registry: &Registry,
    access: &str,
    field: &FieldDef,
) -> Result<String, Diagnostic> {
    match ty {
        TypeRef::Primitive(primitive) => {
            if primitive.requires_explicit_integer_policy() && int_repr == Some(IntRepr::JsonString)
            {
                Ok(format!("int({access})"))
            } else {
                Ok(access.to_owned())
            }
        }
        TypeRef::String | TypeRef::Bytes(_) | TypeRef::GenericParam(_) | TypeRef::Override(_) => {
            Ok(access.to_owned())
        }
        TypeRef::Option(inner) => Ok(format!(
            "None if {access} is None else {}",
            parse_type_expr(inner, int_repr, registry, access, field)?
        )),
        TypeRef::Vec(inner) | TypeRef::Array { item: inner, .. } => Ok(format!(
            "[{} for item in {access}]",
            parse_type_expr(inner, int_repr, registry, "item", field)?
        )),
        TypeRef::Map { key, value } => {
            if !matches!(key.as_ref(), TypeRef::String) {
                return Err(non_string_key(field));
            }
            Ok(format!(
                "{{key: {} for key, value in {access}.items()}}",
                parse_type_expr(value, int_repr, registry, "value", field)?
            ))
        }
        TypeRef::Named(type_id) => {
            let def = registry
                .type_def(*type_id)
                .ok_or_else(|| missing_named(field))?;
            match def {
                TypeDef::Struct(_) => Ok(format!("{}.from_dict({access})", type_name(def))),
                TypeDef::Enum(def) if is_fieldless_enum(def) => {
                    Ok(format!("{}({access})", def.export_name))
                }
                TypeDef::Enum(def) if is_tagged_enum(def) => Ok(format!(
                    "{}({access})",
                    parser_name(&TypeDef::Enum(def.clone()))
                )),
                TypeDef::Enum(_) => Ok(format!("{}.from_dict({access})", type_name(def))),
            }
        }
    }
}

fn serialize_type_expr(
    ty: &TypeRef,
    int_repr: Option<IntRepr>,
    registry: &Registry,
    access: &str,
    field: &FieldDef,
) -> Result<String, Diagnostic> {
    match ty {
        TypeRef::Primitive(primitive) => {
            if primitive.requires_explicit_integer_policy() && int_repr == Some(IntRepr::JsonString)
            {
                Ok(format!("str({access})"))
            } else {
                Ok(access.to_owned())
            }
        }
        TypeRef::String | TypeRef::Bytes(_) | TypeRef::GenericParam(_) | TypeRef::Override(_) => {
            Ok(access.to_owned())
        }
        TypeRef::Option(inner) => Ok(format!(
            "None if {access} is None else {}",
            serialize_type_expr(inner, int_repr, registry, access, field)?
        )),
        TypeRef::Vec(inner) | TypeRef::Array { item: inner, .. } => Ok(format!(
            "[{} for item in {access}]",
            serialize_type_expr(inner, int_repr, registry, "item", field)?
        )),
        TypeRef::Map { key, value } => {
            if !matches!(key.as_ref(), TypeRef::String) {
                return Err(non_string_key(field));
            }
            Ok(format!(
                "{{key: {} for key, value in {access}.items()}}",
                serialize_type_expr(value, int_repr, registry, "value", field)?
            ))
        }
        TypeRef::Named(type_id) => {
            let def = registry
                .type_def(*type_id)
                .ok_or_else(|| missing_named(field))?;
            match def {
                TypeDef::Struct(_) => Ok(format!("{access}.to_dict()")),
                TypeDef::Enum(def) if is_fieldless_enum(def) => Ok(format!("{access}.value")),
                TypeDef::Enum(_) => Ok(format!("{access}.to_dict()")),
            }
        }
    }
}

fn render_type_ref(
    ty: &TypeRef,
    registry: &Registry,
    field: &FieldDef,
) -> Result<String, Diagnostic> {
    match ty {
        TypeRef::Primitive(primitive) => Ok(match primitive {
            Primitive::Bool => "bool".to_owned(),
            primitive if primitive.is_integer() => "int".to_owned(),
            primitive if primitive.is_float() => "float".to_owned(),
            _ => unreachable!("all primitive variants are bool, integer, or float"),
        }),
        TypeRef::String => Ok("str".to_owned()),
        TypeRef::Bytes(_) => Ok("bytes".to_owned()),
        TypeRef::Option(inner) => Ok(format!(
            "{} | None",
            render_type_ref(inner, registry, field)?
        )),
        TypeRef::Vec(inner) | TypeRef::Array { item: inner, .. } => Ok(format!(
            "list[{}]",
            render_type_ref(inner, registry, field)?
        )),
        TypeRef::Map { key, value } => {
            if !matches!(key.as_ref(), TypeRef::String) {
                return Err(non_string_key(field));
            }
            Ok(format!(
                "dict[str, {}]",
                render_type_ref(value, registry, field)?
            ))
        }
        TypeRef::Named(type_id) => {
            let def = registry
                .type_def(*type_id)
                .ok_or_else(|| missing_named(field))?;
            Ok(type_name(def).to_owned())
        }
        TypeRef::GenericParam(name) => Ok(name.clone()),
        TypeRef::Override(override_type) if override_type.backend == BackendId::Python => {
            Ok(override_type.target_type.clone())
        }
        TypeRef::Override(_) => Err(Diagnostic::error(
            DiagnosticCode::new(601),
            "target override is for a different backend",
        )
        .with_field(field.rust_name.to_string())
        .with_backend(BackendId::Python)),
    }
}

fn render_init_file(registry: &Registry) -> String {
    let mut output = String::new();
    output.push_str("from .errors import DtoParseError\n");

    for type_def in registry.types_by_id.values() {
        output.push_str("from .");
        output.push_str(&module_name(type_def));
        output.push_str(" import ");
        output.push_str(type_name(type_def));
        output.push('\n');
    }

    output.push_str("\n__all__ = [\n    \"DtoParseError\",\n");
    for type_def in registry.types_by_id.values() {
        output.push_str("    \"");
        output.push_str(type_name(type_def));
        output.push_str("\",\n");
    }
    output.push_str("]\n");
    output
}

fn render_errors_file() -> String {
    "from __future__ import annotations\n\n\nclass DtoParseError(ValueError):\n    def __init__(self, path: str, cause: Exception | None = None) -> None:\n        self.path = path\n        self.cause = cause\n        message = f\"failed to parse DTO at {path}\"\n        if cause is not None:\n            message = f\"{message}: {cause}\"\n        super().__init__(message)\n".to_owned()
}

#[derive(Debug, Default)]
struct PythonImport {
    parser: bool,
}

fn collect_imports(
    type_id: TypeId,
    type_def: &TypeDef,
    registry: &Registry,
) -> BTreeMap<TypeId, PythonImport> {
    let mut imports = BTreeMap::new();
    collect_type_def_named_refs(type_def, registry, &mut imports);
    imports.remove(&type_id);
    imports
}

fn collect_type_def_named_refs(
    type_def: &TypeDef,
    registry: &Registry,
    imports: &mut BTreeMap<TypeId, PythonImport>,
) {
    match type_def {
        TypeDef::Struct(def) => {
            for field in &def.fields {
                collect_type_ref_named_refs(&field.ty, registry, imports);
            }
        }
        TypeDef::Enum(def) => {
            for variant in &def.variants {
                if let VariantShape::Struct(fields) = &variant.shape {
                    for field in fields {
                        collect_type_ref_named_refs(&field.ty, registry, imports);
                    }
                }
            }
        }
    }
}

fn is_fieldless_enum(def: &EnumDef) -> bool {
    matches!(def.repr, EnumRepr::External)
        && def
            .variants
            .iter()
            .all(|variant| matches!(variant.shape, VariantShape::Unit))
}

fn is_tagged_enum(def: &EnumDef) -> bool {
    matches!(
        def.repr,
        EnumRepr::Internal { .. } | EnumRepr::Adjacent { .. }
    ) && def
        .variants
        .iter()
        .all(|variant| matches!(variant.shape, VariantShape::Struct(_)))
}

fn collect_type_ref_named_refs(
    ty: &TypeRef,
    registry: &Registry,
    imports: &mut BTreeMap<TypeId, PythonImport>,
) {
    match ty {
        TypeRef::Named(type_id) => {
            let import = imports.entry(*type_id).or_default();
            if matches!(
                registry.type_def(*type_id),
                Some(TypeDef::Enum(def)) if is_tagged_enum(def)
            ) {
                import.parser = true;
            }
        }
        TypeRef::Option(inner) | TypeRef::Vec(inner) => {
            collect_type_ref_named_refs(inner, registry, imports);
        }
        TypeRef::Array { item, .. } => collect_type_ref_named_refs(item, registry, imports),
        TypeRef::Map { key, value } => {
            collect_type_ref_named_refs(key, registry, imports);
            collect_type_ref_named_refs(value, registry, imports);
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

fn non_string_key(field: &FieldDef) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(609),
        "non-string map keys are unsupported",
    )
    .with_field(field.rust_name.to_string())
    .with_backend(BackendId::Python)
}

fn missing_named(field: &FieldDef) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(102), "missing named type reference")
        .with_field(field.rust_name.to_string())
        .with_backend(BackendId::Python)
}

fn type_file_path(type_def: &TypeDef, config: &Config) -> String {
    package_file_path(&format!("{}.py", module_name(type_def)), config)
}

fn package_file_path(file_name: &str, config: &Config) -> String {
    format!(
        "{}/{}",
        config.python.out_dir.trim_end_matches('/'),
        file_name
    )
}

fn type_name(type_def: &TypeDef) -> &str {
    match type_def {
        TypeDef::Struct(def) => &def.export_name,
        TypeDef::Enum(def) => &def.export_name,
    }
}

fn module_name(type_def: &TypeDef) -> String {
    to_snake_case(type_name(type_def))
}

fn parser_name(type_def: &TypeDef) -> String {
    format!("parse_{}", module_name(type_def))
}

fn enum_member_name(value: &str) -> String {
    to_snake_case(value).to_ascii_uppercase()
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

fn py_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn escape_py_string(value: &str) -> String {
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

    fn find_file<'a>(files: &'a GeneratedFileSet, suffix: &str) -> &'a GeneratedFile {
        files
            .files()
            .iter()
            .find(|file| file.relative_path().as_str().ends_with(suffix))
            .unwrap()
    }

    #[test]
    fn identifies_backend() {
        assert_eq!(crate::backend_name(), "python");
        assert!(!crate::core_version().is_empty());
        assert_eq!(PythonBackend::new().id(), BackendId::Python);
    }

    #[test]
    fn renders_struct_package_files() {
        let def = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("UserProfile", "UserProfile", span())
                .with_field(field("user_id", "userId", TypeRef::String))
                .with_field(
                    field(
                        "display_name",
                        "displayName",
                        TypeRef::option(TypeRef::String),
                    )
                    .with_presence(FieldPresence::defaulted(
                        dto_bindgen_core::DefaultKind::NoneValue,
                    )),
                )
                .with_field(
                    field("tags", "tags", TypeRef::vec(TypeRef::String))
                        .with_presence(FieldPresence::defaulted(DefaultKind::EmptyVec)),
                ),
        );
        let mut registry = Registry::new();
        registry.register_type(RustTypeId::new("sdk", "sdk", "UserProfile"), def);

        let files = PythonBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let user = find_file(&files, "user_profile.py");
        let init = find_file(&files, "__init__.py");
        let errors = find_file(&files, "errors.py");

        assert!(
            user.contents()
                .contains("@dataclass(frozen=True, slots=True, kw_only=True)")
        );
        assert!(user.contents().contains("user_id: str"));
        assert!(user.contents().contains("display_name: str | None"));
        assert!(
            user.contents()
                .contains("display_name: str | None = field(default=None")
        );
        assert!(
            user.contents()
                .contains("tags: list[str] = field(default_factory=list")
        );
        assert!(user.contents().contains("data.get(\"displayName\")"));
        assert!(user.contents().contains("data.get(\"tags\", [])"));
        assert!(user.contents().contains("def from_dict"));
        assert!(user.contents().contains("def to_dict"));
        assert!(
            init.contents()
                .contains("from .user_profile import UserProfile")
        );
        assert!(errors.contents().contains("class DtoParseError"));
        assert!(find_file(&files, "py.typed").contents().is_empty());
    }

    #[test]
    fn renders_fieldless_enum() {
        let def = TypeDef::Enum(
            EnumDef::new("UserRole", "UserRole", EnumRepr::External, span())
                .with_variant(dto_bindgen_core::VariantDef::new(
                    "Admin",
                    "admin",
                    VariantShape::Unit,
                    span(),
                ))
                .with_variant(dto_bindgen_core::VariantDef::new(
                    "GuestUser",
                    "guestUser",
                    VariantShape::Unit,
                    span(),
                )),
        );
        let mut registry = Registry::new();
        registry.register_type(RustTypeId::new("sdk", "sdk", "UserRole"), def);

        let files = PythonBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let role = find_file(&files, "user_role.py");

        assert!(role.contents().contains("from enum import StrEnum"));
        assert!(role.contents().contains("class UserRole(StrEnum):"));
        assert!(role.contents().contains("ADMIN = \"admin\""));
        assert!(role.contents().contains("GUEST_USER = \"guestUser\""));
    }

    #[test]
    fn renders_named_imports_and_json_string_integers() {
        let mut registry = Registry::new();
        let user_id = registry.register_type(
            RustTypeId::new("sdk", "sdk", "UserProfile"),
            TypeDef::Struct(dto_bindgen_core::StructDef::new(
                "UserProfile",
                "UserProfile",
                span(),
            )),
        );
        let role_id = registry.register_type(
            RustTypeId::new("sdk", "sdk", "UserRole"),
            TypeDef::Enum(
                EnumDef::new("UserRole", "UserRole", EnumRepr::External, span()).with_variant(
                    dto_bindgen_core::VariantDef::new(
                        "GuestUser",
                        "guestUser",
                        VariantShape::Unit,
                        span(),
                    ),
                ),
            ),
        );
        let amount = field(
            "amount_minor_units",
            "amountMinorUnits",
            TypeRef::Primitive(Primitive::U128),
        )
        .with_int_repr(IntRepr::JsonString);
        let entry = TypeDef::Struct(
            dto_bindgen_core::StructDef::new("LedgerEntry", "LedgerEntry", span())
                .with_field(field("user", "user", TypeRef::named(user_id)))
                .with_field(field("role", "role", TypeRef::named(role_id)))
                .with_field(amount),
        );
        registry.register_type(RustTypeId::new("sdk", "sdk", "LedgerEntry"), entry);

        let files = PythonBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let ledger = find_file(&files, "ledger_entry.py");

        assert!(
            ledger
                .contents()
                .contains("from .user_profile import UserProfile")
        );
        assert!(
            ledger
                .contents()
                .contains("from .user_role import UserRole")
        );
        assert!(ledger.contents().contains("user: UserProfile"));
        assert!(ledger.contents().contains("role: UserRole"));
        assert!(ledger.contents().contains("UserRole(data[\"role\"])"));
        assert!(
            ledger
                .contents()
                .contains("output[\"role\"] = self.role.value")
        );
        assert!(ledger.contents().contains("amount_minor_units: int"));
        assert!(
            ledger
                .contents()
                .contains("int(data[\"amountMinorUnits\"])")
        );
        assert!(ledger.contents().contains("str(self.amount_minor_units)"));
    }

    #[test]
    fn renders_adjacent_tagged_enum_helpers() {
        let mut registry = Registry::new();
        let user_id = registry.register_type(
            RustTypeId::new("sdk", "sdk", "UserProfile"),
            TypeDef::Struct(dto_bindgen_core::StructDef::new(
                "UserProfile",
                "UserProfile",
                span(),
            )),
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
            .with_variant(dto_bindgen_core::VariantDef::new(
                "UserCreated",
                "userCreated",
                VariantShape::Struct(vec![
                    field("user", "user", TypeRef::named(user_id)),
                    field("event_id", "eventId", TypeRef::String),
                ]),
                span(),
            )),
        );
        registry.register_type(RustTypeId::new("sdk", "sdk", "SdkEvent"), event);

        let files = PythonBackend::new()
            .render(&registry, &Config::default())
            .unwrap();
        let event = find_file(&files, "sdk_event.py");

        assert!(
            event
                .contents()
                .contains("from .user_profile import UserProfile")
        );
        assert!(event.contents().contains("class SdkEventUserCreated"));
        assert!(
            event
                .contents()
                .contains("SdkEvent: TypeAlias = SdkEventUserCreated")
        );
        assert!(
            event
                .contents()
                .contains("def parse_sdk_event(data: dict[str, Any]) -> SdkEvent:")
        );
        assert!(event.contents().contains("payload = data[\"payload\"]"));
        assert!(
            event
                .contents()
                .contains("UserProfile.from_dict(payload[\"user\"])")
        );
        assert!(event.contents().contains("\"type\": \"userCreated\""));
        assert!(event.contents().contains("output[\"payload\"] = payload"));
    }
}
