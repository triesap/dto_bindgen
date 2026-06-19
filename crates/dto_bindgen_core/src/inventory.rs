use core::fmt;
use std::collections::BTreeSet;

use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, ExprLit, Fields, GenericArgument, Item, ItemEnum, ItemStruct, Lit, Meta,
    PathArguments, Type, punctuated::Punctuated,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInventory {
    pub source_file: String,
    pub items: Vec<InventoryItem>,
    pub findings: Vec<InventoryFinding>,
}

impl SourceInventory {
    pub fn new(source_file: impl Into<String>) -> Self {
        Self {
            source_file: source_file.into(),
            items: Vec::new(),
            findings: Vec::new(),
        }
    }

    pub fn fields(&self) -> impl Iterator<Item = &InventoryField> {
        self.items.iter().flat_map(|item| {
            item.fields
                .iter()
                .chain(item.variants.iter().flat_map(|v| &v.fields))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub kind: InventoryItemKind,
    pub rust_name: String,
    pub derives: Vec<String>,
    pub generics: Vec<String>,
    pub attrs: Vec<InventoryAttribute>,
    pub fields: Vec<InventoryField>,
    pub variants: Vec<InventoryVariant>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryItemKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryVariant {
    pub rust_name: String,
    pub attrs: Vec<InventoryAttribute>,
    pub fields: Vec<InventoryField>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryField {
    pub rust_name: String,
    pub type_name: String,
    pub type_paths: Vec<String>,
    pub attrs: Vec<InventoryAttribute>,
    pub skipped: bool,
    pub classes: Vec<InventoryTypeClass>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryAttribute {
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
    pub supported: bool,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryTypeClass {
    LargeInteger { rust_type: String },
    ThirdParty { family: String, rust_type: String },
    CustomCandidate { rust_type: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryFinding {
    pub code: String,
    pub severity: InventorySeverity,
    pub message: String,
    pub location: InventoryLocation,
    pub type_name: Option<String>,
    pub field_name: Option<String>,
    pub variant_name: Option<String>,
    pub attribute: Option<String>,
    pub help: Option<String>,
}

impl InventoryFinding {
    fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        location: InventoryLocation,
    ) -> Self {
        Self {
            code: code.into(),
            severity: InventorySeverity::Warning,
            message: message.into(),
            location,
            type_name: None,
            field_name: None,
            variant_name: None,
            attribute: None,
            help: None,
        }
    }

    fn with_type(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = Some(type_name.into());
        self
    }

    fn with_field(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = Some(field_name.into());
        self
    }

    fn with_variant(mut self, variant_name: impl Into<String>) -> Self {
        self.variant_name = Some(variant_name.into());
        self
    }

    fn with_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.attribute = Some(attribute.into());
        self
    }

    fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySeverity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl InventoryLocation {
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryScanError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for InventoryScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse Rust source `{}` for inventory: {}",
            self.path, self.message
        )
    }
}

impl std::error::Error for InventoryScanError {}

pub fn scan_rust_source(
    path: impl Into<String>,
    input: &str,
) -> Result<SourceInventory, InventoryScanError> {
    let path = path.into();
    let file = syn::parse_file(input).map_err(|source| InventoryScanError {
        path: path.clone(),
        message: source.to_string(),
    })?;
    let known_items = collect_known_items(&file.items);
    let known_dto_items = collect_known_dto_items(&file.items);
    let mut scanner = InventoryScanner {
        path: path.clone(),
        known_items,
        known_dto_items,
        inventory: SourceInventory::new(path),
    };

    for item in &file.items {
        scanner.scan_item(item);
    }

    scanner.inventory.items.sort_by(|left, right| {
        left.location
            .line
            .cmp(&right.location.line)
            .then_with(|| left.rust_name.cmp(&right.rust_name))
    });
    scanner.inventory.findings.sort_by(|left, right| {
        left.location
            .file
            .cmp(&right.location.file)
            .then_with(|| left.location.line.cmp(&right.location.line))
            .then_with(|| left.location.column.cmp(&right.location.column))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.message.cmp(&right.message))
    });

    Ok(scanner.inventory)
}

struct InventoryScanner {
    path: String,
    known_items: BTreeSet<String>,
    known_dto_items: BTreeSet<String>,
    inventory: SourceInventory,
}

impl InventoryScanner {
    fn scan_item(&mut self, item: &Item) {
        match item {
            Item::Struct(item) => self.scan_struct(item),
            Item::Enum(item) => self.scan_enum(item),
            _ => {}
        }
    }

    fn scan_struct(&mut self, item: &ItemStruct) {
        let type_name = item.ident.to_string();
        let attrs = self.collect_attrs(&item.attrs, AttrScope::Container);
        let derives = derive_names(&item.attrs);
        let generics = generic_names(&item.generics);
        let fields = self.scan_fields(&type_name, None, &item.fields);

        if !generics.is_empty() && derives.iter().any(|name| name == "Dto") {
            self.push_finding(
                InventoryFinding::warning(
                    "INV1001",
                    "generic DTO declarations are not supported by `Dto` derive",
                    self.location(item),
                )
                .with_type(type_name.clone())
                .with_help("Keep this deferred unless the real SDK requires generic DTO support."),
            );
        }

        match &item.fields {
            Fields::Named(_) => {}
            Fields::Unnamed(_) => self.push_finding(
                InventoryFinding::warning(
                    "INV1002",
                    "tuple structs are unsupported DTO shapes",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            ),
            Fields::Unit => self.push_finding(
                InventoryFinding::warning(
                    "INV1003",
                    "unit structs are unsupported DTO shapes",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            ),
        }

        self.inventory.items.push(InventoryItem {
            kind: InventoryItemKind::Struct,
            rust_name: type_name,
            derives,
            generics,
            attrs,
            fields,
            variants: Vec::new(),
            location: self.location(item),
        });
    }

    fn scan_enum(&mut self, item: &ItemEnum) {
        let type_name = item.ident.to_string();
        let attrs = self.collect_attrs(&item.attrs, AttrScope::Container);
        let derives = derive_names(&item.attrs);
        let generics = generic_names(&item.generics);
        let tagged = attrs.iter().any(|attr| {
            attr.namespace == "serde" && (attr.name == "tag" || attr.name == "content")
        });
        let mut variants = Vec::new();

        if !generics.is_empty() && derives.iter().any(|name| name == "Dto") {
            self.push_finding(
                InventoryFinding::warning(
                    "INV1001",
                    "generic DTO declarations are not supported by `Dto` derive",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            );
        }

        for variant in &item.variants {
            let variant_name = variant.ident.to_string();
            let variant_attrs = self.collect_attrs(&variant.attrs, AttrScope::Variant);
            let fields = self.scan_fields(&type_name, Some(&variant_name), &variant.fields);

            match &variant.fields {
                Fields::Unit => {}
                Fields::Named(_) if tagged => {}
                Fields::Named(_) => self.push_finding(
                    InventoryFinding::warning(
                        "INV1004",
                        "externally tagged data enum variants are deferred",
                        self.location(variant),
                    )
                    .with_type(type_name.clone())
                    .with_variant(variant_name.clone())
                    .with_help("Add explicit enum tagging or defer until Python representation is specified."),
                ),
                Fields::Unnamed(_) => self.push_finding(
                    InventoryFinding::warning(
                        "INV1005",
                        "tuple enum variants are unsupported DTO shapes",
                        self.location(variant),
                    )
                    .with_type(type_name.clone())
                    .with_variant(variant_name.clone()),
                ),
            }

            variants.push(InventoryVariant {
                rust_name: variant_name,
                attrs: variant_attrs,
                fields,
                location: self.location(variant),
            });
        }

        self.inventory.items.push(InventoryItem {
            kind: InventoryItemKind::Enum,
            rust_name: type_name,
            derives,
            generics,
            attrs,
            fields: Vec::new(),
            variants,
            location: self.location(item),
        });
    }

    fn scan_fields(
        &mut self,
        type_name: &str,
        variant_name: Option<&str>,
        fields: &Fields,
    ) -> Vec<InventoryField> {
        let Fields::Named(fields) = fields else {
            return Vec::new();
        };

        fields
            .named
            .iter()
            .filter_map(|field| {
                let ident = field.ident.as_ref()?;
                let field_name = ident.to_string();
                let attrs = self.collect_attrs(&field.attrs, AttrScope::Field);
                let skipped = attrs.iter().any(|attr| {
                    (attr.namespace == "serde" || attr.namespace == "dto") && attr.name == "skip"
                });
                let type_paths = type_paths(&field.ty);
                let mut classes =
                    classify_field_type(&type_paths, &self.known_items, &self.known_dto_items);
                classes.sort();
                classes.dedup();

                for class in &classes {
                    self.push_type_class_finding(
                        type_name,
                        variant_name,
                        &field_name,
                        field,
                        class,
                    );
                }

                Some(InventoryField {
                    rust_name: field_name,
                    type_name: field.ty.to_token_stream().to_string(),
                    type_paths,
                    attrs,
                    skipped,
                    classes,
                    location: self.location(field),
                })
            })
            .collect()
    }

    fn collect_attrs(&mut self, attrs: &[Attribute], scope: AttrScope) -> Vec<InventoryAttribute> {
        let mut output = Vec::new();
        for attr in attrs {
            let Some(namespace) = attr_namespace(attr) else {
                continue;
            };
            let location = self.location(attr);

            match meta_children(&attr.meta) {
                Ok(children) if !children.is_empty() => {
                    for meta in children {
                        let name = meta_path(&meta);
                        let value = meta_value(&meta);
                        let supported = attr_supported(namespace, scope, &meta);
                        let inventory_attr = InventoryAttribute {
                            namespace: namespace.to_owned(),
                            name,
                            value,
                            supported,
                            location: location.clone(),
                        };
                        self.push_attr_finding(scope, &inventory_attr);
                        output.push(inventory_attr);
                    }
                }
                _ => {
                    let name = attr
                        .path()
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string())
                        .unwrap_or_else(|| namespace.to_owned());
                    let inventory_attr = InventoryAttribute {
                        namespace: namespace.to_owned(),
                        name,
                        value: None,
                        supported: false,
                        location,
                    };
                    self.push_attr_finding(scope, &inventory_attr);
                    output.push(inventory_attr);
                }
            }
        }
        output.sort_by(|left, right| {
            left.namespace
                .cmp(&right.namespace)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.value.cmp(&right.value))
        });
        output
    }

    fn push_attr_finding(&mut self, scope: AttrScope, attr: &InventoryAttribute) {
        if attr.supported {
            return;
        }

        let namespace = match attr.namespace.as_str() {
            "serde" => "Serde",
            "dto" => "dto",
            _ => attr.namespace.as_str(),
        };
        let scope_name = match scope {
            AttrScope::Container => "container",
            AttrScope::Field => "field",
            AttrScope::Variant => "variant",
        };
        self.push_finding(
            InventoryFinding::warning(
                "INV0300",
                format!(
                    "unsupported {namespace} {scope_name} attribute `{}`",
                    attr.name
                ),
                attr.location.clone(),
            )
            .with_attribute(format!("{}::{}", attr.namespace, attr.name))
            .with_help("Keep this deferred unless SDK inventory proves it is required."),
        );
    }

    fn push_type_class_finding(
        &mut self,
        type_name: &str,
        variant_name: Option<&str>,
        field_name: &str,
        field: &syn::Field,
        class: &InventoryTypeClass,
    ) {
        let finding = match class {
            InventoryTypeClass::LargeInteger { rust_type } => InventoryFinding::warning(
                "INV0400",
                format!("large integer field uses `{rust_type}`"),
                self.location(field),
            )
            .with_help("Add an explicit numeric policy before generated TypeScript is adopted."),
            InventoryTypeClass::ThirdParty { family, rust_type } => InventoryFinding::warning(
                "INV1100",
                format!(
                    "third-party field type `{rust_type}` from `{family}` requires inventory review"
                ),
                self.location(field),
            )
            .with_help(
                "Do not add a mapping until the pilot report proves this field is required.",
            ),
            InventoryTypeClass::CustomCandidate { rust_type } => InventoryFinding::warning(
                "INV1101",
                format!("custom field type `{rust_type}` may require a `Dto` descriptor"),
                self.location(field),
            )
            .with_help(
                "Confirm the referenced type is a DTO dependency or add an explicit override.",
            ),
        };

        let finding = finding
            .with_type(type_name.to_owned())
            .with_field(field_name.to_owned());
        let finding = match variant_name {
            Some(variant_name) => finding.with_variant(variant_name.to_owned()),
            None => finding,
        };
        self.push_finding(finding);
    }

    fn push_finding(&mut self, finding: InventoryFinding) {
        self.inventory.findings.push(finding);
    }

    fn location<T>(&self, node: &T) -> InventoryLocation
    where
        T: Spanned,
    {
        let start = node.span().start();
        InventoryLocation::new(&self.path, start.line as u32, start.column as u32)
    }
}

#[derive(Debug, Clone, Copy)]
enum AttrScope {
    Container,
    Field,
    Variant,
}

fn collect_known_items(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some(item.ident.to_string()),
            Item::Enum(item) => Some(item.ident.to_string()),
            _ => None,
        })
        .collect()
}

fn collect_known_dto_items(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) if derive_names(&item.attrs).iter().any(|name| name == "Dto") => {
                Some(item.ident.to_string())
            }
            Item::Enum(item) if derive_names(&item.attrs).iter().any(|name| name == "Dto") => {
                Some(item.ident.to_string())
            }
            _ => None,
        })
        .collect()
}

fn attr_namespace(attr: &Attribute) -> Option<&'static str> {
    if attr.path().is_ident("serde") {
        Some("serde")
    } else if attr.path().is_ident("dto") {
        Some("dto")
    } else {
        None
    }
}

fn derive_names(attrs: &[Attribute]) -> Vec<String> {
    let mut names = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }
        if let Ok(children) = meta_children(&attr.meta) {
            for child in children {
                names.push(meta_path(&child));
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn generic_names(generics: &syn::Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Type(param) => param.ident.to_string(),
            syn::GenericParam::Lifetime(param) => param.lifetime.ident.to_string(),
            syn::GenericParam::Const(param) => param.ident.to_string(),
        })
        .collect()
}

fn meta_children(meta: &Meta) -> syn::Result<Vec<Meta>> {
    match meta {
        Meta::List(list) => Ok(list
            .parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?
            .into_iter()
            .collect()),
        Meta::Path(_) | Meta::NameValue(_) => Ok(Vec::new()),
    }
}

fn meta_path(meta: &Meta) -> String {
    match meta {
        Meta::Path(path)
        | Meta::List(syn::MetaList { path, .. })
        | Meta::NameValue(syn::MetaNameValue { path, .. }) => path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
    }
}

fn meta_value(meta: &Meta) -> Option<String> {
    let Meta::NameValue(name_value) = meta else {
        return None;
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &name_value.value
    else {
        return Some(name_value.value.to_token_stream().to_string());
    };
    Some(value.value())
}

fn attr_supported(namespace: &str, scope: AttrScope, meta: &Meta) -> bool {
    let name = meta_path(meta);
    match (namespace, scope, name.as_str()) {
        (
            "serde",
            AttrScope::Container,
            "rename"
            | "rename_all"
            | "rename_all_fields"
            | "tag"
            | "content"
            | "deny_unknown_fields",
        ) => !matches!(meta, Meta::List(_)),
        ("serde", AttrScope::Container, "default") => matches!(meta, Meta::Path(_)),
        ("serde", AttrScope::Field, "rename" | "skip") => !matches!(meta, Meta::List(_)),
        ("serde", AttrScope::Field, "default") => matches!(meta, Meta::Path(_)),
        ("serde", AttrScope::Variant, "rename") => !matches!(meta, Meta::List(_)),
        ("dto", AttrScope::Field, "skip" | "int_repr") => !matches!(meta, Meta::List(_)),
        _ => false,
    }
}

fn type_paths(ty: &Type) -> Vec<String> {
    let mut paths = Vec::new();
    collect_type_paths(ty, &mut paths);
    paths.sort();
    paths.dedup();
    paths
}

fn collect_type_paths(ty: &Type, paths: &mut Vec<String>) {
    match ty {
        Type::Array(array) => collect_type_paths(&array.elem, paths),
        Type::Group(group) => collect_type_paths(&group.elem, paths),
        Type::Paren(paren) => collect_type_paths(&paren.elem, paths),
        Type::Path(path) => collect_path(&path.path, paths),
        Type::Reference(reference) => collect_type_paths(&reference.elem, paths),
        Type::Slice(slice) => collect_type_paths(&slice.elem, paths),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_type_paths(elem, paths);
            }
        }
        _ => {}
    }
}

fn collect_path(path: &syn::Path, paths: &mut Vec<String>) {
    let path_name = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    paths.push(path_name);

    for segment in &path.segments {
        let PathArguments::AngleBracketed(args) = &segment.arguments else {
            continue;
        };
        for arg in &args.args {
            if let GenericArgument::Type(ty) = arg {
                collect_type_paths(ty, paths);
            }
        }
    }
}

fn classify_field_type(
    paths: &[String],
    known_items: &BTreeSet<String>,
    known_dto_items: &BTreeSet<String>,
) -> Vec<InventoryTypeClass> {
    let mut classes = Vec::new();
    for path in paths {
        let Some(last) = path.rsplit("::").next() else {
            continue;
        };
        if is_large_integer(last) {
            classes.push(InventoryTypeClass::LargeInteger {
                rust_type: path.clone(),
            });
            continue;
        }
        if let Some(family) = third_party_family(path, last) {
            classes.push(InventoryTypeClass::ThirdParty {
                family,
                rust_type: path.clone(),
            });
            continue;
        }
        if is_builtin_or_container(last) {
            continue;
        }
        if known_dto_items.contains(last) {
            continue;
        }
        if known_items.contains(last) || likely_custom_type(last) {
            classes.push(InventoryTypeClass::CustomCandidate {
                rust_type: path.clone(),
            });
        }
    }
    classes
}

fn is_large_integer(name: &str) -> bool {
    matches!(name, "i64" | "u64" | "i128" | "u128" | "isize" | "usize")
}

fn is_builtin_or_container(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "str"
            | "bool"
            | "i8"
            | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "f32"
            | "f64"
            | "Option"
            | "Vec"
            | "HashMap"
            | "BTreeMap"
            | "Box"
    )
}

fn third_party_family(path: &str, last: &str) -> Option<String> {
    let mut segments = path.split("::");
    let first = segments.next().unwrap_or(path);
    match first {
        "uuid" => Some("uuid".to_owned()),
        "chrono" => Some("chrono".to_owned()),
        "time" => Some("time".to_owned()),
        "serde_json" => Some("serde_json".to_owned()),
        "url" => Some("url".to_owned()),
        "bytes" => Some("bytes".to_owned()),
        "rust_decimal" => Some("rust_decimal".to_owned()),
        "indexmap" => Some("indexmap".to_owned()),
        "Cow" => Some("cow".to_owned()),
        "std" | "core" if path.contains("borrow::Cow") => Some("cow".to_owned()),
        "NonZeroI8" | "NonZeroU8" | "NonZeroI16" | "NonZeroU16" | "NonZeroI32" | "NonZeroU32"
        | "NonZeroI64" | "NonZeroU64" | "NonZeroI128" | "NonZeroU128" | "NonZeroIsize"
        | "NonZeroUsize" => Some("nonzero".to_owned()),
        "std" | "core" if path.contains("num::NonZero") => Some("nonzero".to_owned()),
        _ if last.starts_with("NonZero") => Some("nonzero".to_owned()),
        _ => None,
    }
}

fn likely_custom_type(name: &str) -> bool {
    name.chars().next().map(char::is_uppercase).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_sdk_like_source_without_requiring_dto_compile_success() {
        let source = r#"
            use serde::{Deserialize, Serialize};

            #[derive(Serialize, Deserialize, Dto)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct UserProfile {
                user_id: uuid::Uuid,
                #[serde(skip)]
                internal_note: String,
                #[serde(flatten)]
                metadata: serde_json::Value,
                balance: u128,
                nested: MissingDto,
            }

            #[derive(Serialize, Dto)]
            struct Wrapper<T> {
                value: T,
            }

            #[derive(Serialize, Deserialize)]
            #[serde(untagged)]
            enum SdkEvent {
                UserCreated { user: UserProfile },
                Other(String),
            }
        "#;

        let inventory = scan_rust_source("src/sdk.rs", source).unwrap();

        assert_eq!(inventory.items.len(), 3);
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV0300"
                    && finding.attribute.as_deref() == Some("serde::flatten"))
        );
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV1001"
                    && finding.type_name.as_deref() == Some("Wrapper"))
        );
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV1005"
                    && finding.variant_name.as_deref() == Some("Other"))
        );

        let internal_note = inventory
            .fields()
            .find(|field| field.rust_name == "internal_note")
            .unwrap();
        assert!(internal_note.skipped);

        let user_id = inventory
            .fields()
            .find(|field| field.rust_name == "user_id")
            .unwrap();
        assert!(user_id.classes.iter().any(|class| {
            matches!(
                class,
                InventoryTypeClass::ThirdParty { family, .. } if family == "uuid"
            )
        }));

        let balance = inventory
            .fields()
            .find(|field| field.rust_name == "balance")
            .unwrap();
        assert!(balance.classes.iter().any(|class| {
            matches!(
                class,
                InventoryTypeClass::LargeInteger { rust_type } if rust_type == "u128"
            )
        }));
    }

    #[test]
    fn recognizes_container_default_as_supported_inventory_usage() {
        let source = r#"
            #[derive(Dto)]
            #[serde(default)]
            struct Defaults {
                tags: Vec<String>,
            }
        "#;

        let inventory = scan_rust_source("src/defaults.rs", source).unwrap();
        let item = inventory.items.first().unwrap();

        assert!(
            item.attrs.iter().any(|attr| {
                attr.namespace == "serde" && attr.name == "default" && attr.supported
            })
        );
        assert!(
            !inventory
                .findings
                .iter()
                .any(|finding| { finding.attribute.as_deref() == Some("serde::default") })
        );
    }

    #[test]
    fn reports_custom_default_paths_as_unsupported() {
        let source = r#"
            #[derive(Dto)]
            struct Defaults {
                #[serde(default = "fallback")]
                tags: Vec<String>,
            }
        "#;

        let inventory = scan_rust_source("src/defaults.rs", source).unwrap();

        assert!(inventory.findings.iter().any(|finding| {
            finding.code == "INV0300" && finding.attribute.as_deref() == Some("serde::default")
        }));
    }
}
