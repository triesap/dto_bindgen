use dto_bindgen::{Dto, export};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
#[derive(Dto)]
#[serde(rename_all = "camelCase")]
struct PostalAddress {
    line_1: String,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(
    rename = "PublicUserProfile",
    rename_all = "camelCase",
    deny_unknown_fields
)]
struct UserProfile {
    user_id: String,
    #[serde(rename = "enabled")]
    active: bool,
    address: PostalAddress,
    tags: Vec<String>,
    #[serde(skip)]
    internal_note: String,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(rename_all = "camelCase")]
enum UserRole {
    Admin,
    GuestUser,
    #[serde(rename = "owner")]
    OwnerRole,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SdkEvent {
    UserCreated { user: UserProfile, event_id: String },
    UserDeleted { user_id: String },
}

#[allow(dead_code)]
#[derive(Dto)]
struct LedgerEntry {
    #[dto(int_repr = "json_string")]
    amount_minor_units: u128,
}

#[allow(dead_code)]
#[derive(Dto)]
struct UnannotatedLedgerEntry {
    amount_minor_units: u128,
}

#[allow(dead_code)]
#[derive(Dto)]
struct PresenceDefaults {
    display_name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    retry_count: u32,
    #[serde(default)]
    note: String,
}

#[allow(dead_code)]
#[derive(Dto)]
struct SkipNonePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

#[test]
fn derives_named_struct_descriptors() {
    let registry = export::build_registry([export::RootDescriptor::new::<UserProfile>()]);

    assert!(registry.diagnostics.is_empty());
    assert_eq!(registry.types_by_id.len(), 2);
    assert_eq!(registry.roots.len(), 1);

    let root = *registry.roots.iter().next().unwrap();
    let root_def = registry.type_def(root).unwrap();
    let dto_bindgen::__private::TypeDef::Struct(root_struct) = root_def else {
        panic!("expected root struct");
    };

    assert_eq!(root_struct.export_name, "PublicUserProfile");
    assert_eq!(
        root_struct.attrs.rename.as_deref(),
        Some("PublicUserProfile")
    );
    assert_eq!(root_struct.attrs.rename_all.as_deref(), Some("camelCase"));
    assert!(root_struct.attrs.deny_unknown_fields);
    assert!(
        root_struct
            .fields
            .iter()
            .all(|field| field.rust_name.as_str() != "internal_note")
    );
    assert_eq!(wire_field(root_struct, "user_id"), Some("userId"));
    assert_eq!(wire_field(root_struct, "active"), Some("enabled"));

    let (root_rust_id, _) = registry
        .rust_id_to_type_id
        .iter()
        .find(|(_, type_id)| **type_id == root)
        .expect("root should have canonical Rust identity");
    assert_eq!(root_rust_id.package_name, env!("CARGO_PKG_NAME"));
    assert_eq!(root_rust_id.crate_name, env!("CARGO_CRATE_NAME"));
    assert_eq!(root_rust_id.rust_ident, "UserProfile");

    let dto_bindgen::export::TypeRef::Named(address_id) = first_named_field(root_def).unwrap()
    else {
        panic!("expected named field ref");
    };

    assert!(registry.dependencies_of(root).any(|dep| dep == address_id));
}

#[test]
fn derives_fieldless_enum_descriptors() {
    let registry = export::build_registry([export::RootDescriptor::new::<UserRole>()]);

    assert!(registry.diagnostics.is_empty());
    assert_eq!(registry.types_by_id.len(), 1);
    assert_eq!(registry.roots.len(), 1);

    let root = *registry.roots.iter().next().unwrap();
    let dto_bindgen::__private::TypeDef::Enum(def) = registry.type_def(root).unwrap() else {
        panic!("expected enum root");
    };

    assert_eq!(def.export_name, "UserRole");
    assert!(matches!(
        def.repr,
        dto_bindgen::__private::EnumRepr::External
    ));
    assert_eq!(variant_wire_name(def, "Admin"), Some("admin"));
    assert_eq!(variant_wire_name(def, "GuestUser"), Some("guestUser"));
    assert_eq!(variant_wire_name(def, "OwnerRole"), Some("owner"));
}

#[test]
fn derives_adjacently_tagged_enum_descriptors() {
    let registry = export::build_registry([export::RootDescriptor::new::<SdkEvent>()]);

    assert!(registry.diagnostics.is_empty());
    assert_eq!(registry.types_by_id.len(), 3);
    assert_eq!(registry.roots.len(), 1);

    let root = *registry.roots.iter().next().unwrap();
    let dto_bindgen::__private::TypeDef::Enum(def) = registry.type_def(root).unwrap() else {
        panic!("expected enum root");
    };

    let dto_bindgen::__private::EnumRepr::Adjacent { tag, content } = &def.repr else {
        panic!("expected adjacent enum repr");
    };
    assert_eq!(tag, "type");
    assert_eq!(content, "payload");
    assert_eq!(variant_wire_name(def, "UserCreated"), Some("userCreated"));
    assert_eq!(variant_wire_name(def, "UserDeleted"), Some("userDeleted"));
    assert_eq!(
        variant_field_wire_name(def, "UserCreated", "event_id"),
        Some("eventId")
    );
    assert_eq!(
        variant_field_wire_name(def, "UserDeleted", "user_id"),
        Some("userId")
    );

    let dto_bindgen::export::TypeRef::Named(user_profile_id) =
        variant_named_field(def, "UserCreated", "user").unwrap()
    else {
        panic!("expected named user profile field");
    };
    assert!(
        registry
            .dependencies_of(root)
            .any(|dep| dep == user_profile_id)
    );
}

#[test]
fn export_types_macro_builds_and_validates_roots() {
    let config_path = temp_config("");

    let report = dto_bindgen::export_types!(
        config = config_path.as_path(),
        roots = [UserProfile, SdkEvent],
    )
    .unwrap();

    assert_eq!(report.registry.roots.len(), 2);
    assert!(!report.files.is_empty());
    assert!(report.diagnostics.is_empty());

    cleanup_config(&config_path);
}

#[test]
fn inventory_preserves_skipped_fields_while_export_omits_them() {
    let inventory = dto_bindgen_core::scan_rust_source(
        "src/sdk.rs",
        r#"
        #[derive(Dto)]
        #[serde(rename = "PublicUserProfile", rename_all = "camelCase")]
        struct UserProfile {
            user_id: String,
            #[serde(skip)]
            internal_note: String,
        }
        "#,
    )
    .unwrap();

    let skipped = inventory
        .fields()
        .find(|field| field.rust_name == "internal_note")
        .expect("inventory should preserve skipped field");
    assert!(skipped.skipped);

    let config_path = temp_config("");
    dto_bindgen::export_types!(config = config_path.as_path(), roots = [UserProfile],).unwrap();
    let generated = config_path
        .parent()
        .unwrap()
        .join("generated/ts/public_user_profile.ts");
    let contents = std::fs::read_to_string(generated).unwrap();

    assert!(!contents.contains("internalNote"));
    assert!(!contents.contains("internal_note"));

    cleanup_config(&config_path);
}

#[test]
fn dto_int_repr_satisfies_large_integer_policy() {
    let config_path = temp_config("");

    let report =
        dto_bindgen::export_types!(config = config_path.as_path(), roots = [LedgerEntry],).unwrap();

    let root = *report.registry.roots.iter().next().unwrap();
    let dto_bindgen::__private::TypeDef::Struct(def) = report.registry.type_def(root).unwrap()
    else {
        panic!("expected ledger struct");
    };
    assert_eq!(
        def.fields[0].int_repr,
        Some(dto_bindgen::__private::IntRepr::JsonString)
    );

    cleanup_config(&config_path);
}

#[test]
fn derives_option_and_builtin_default_presence() {
    let registry = export::build_registry([export::RootDescriptor::new::<PresenceDefaults>()]);

    assert!(registry.diagnostics.is_empty());
    let root = *registry.roots.iter().next().unwrap();
    let dto_bindgen::__private::TypeDef::Struct(def) = registry.type_def(root).unwrap() else {
        panic!("expected presence struct");
    };

    let display_name = struct_field(def, "display_name").unwrap();
    assert!(display_name.presence.nullable);
    assert!(!display_name.presence.required_on_deserialize);
    assert_eq!(
        display_name.presence.default,
        Some(dto_bindgen::__private::DefaultKind::NoneValue)
    );

    let tags = struct_field(def, "tags").unwrap();
    assert!(!tags.presence.required_on_deserialize);
    assert_eq!(
        tags.presence.default,
        Some(dto_bindgen::__private::DefaultKind::EmptyVec)
    );

    let active = struct_field(def, "active").unwrap();
    assert_eq!(
        active.presence.default,
        Some(dto_bindgen::__private::DefaultKind::BoolFalse)
    );

    let retry_count = struct_field(def, "retry_count").unwrap();
    assert_eq!(
        retry_count.presence.default,
        Some(dto_bindgen::__private::DefaultKind::NumericZero)
    );

    let note = struct_field(def, "note").unwrap();
    assert_eq!(
        note.presence.default,
        Some(dto_bindgen::__private::DefaultKind::EmptyString)
    );
}

#[test]
fn derives_skip_if_none_presence_for_option_fields() {
    let registry = export::build_registry([export::RootDescriptor::new::<SkipNonePatch>()]);

    assert!(registry.diagnostics.is_empty());
    let root = *registry.roots.iter().next().unwrap();
    let dto_bindgen::__private::TypeDef::Struct(def) = registry.type_def(root).unwrap() else {
        panic!("expected skip-none struct");
    };

    let display_name = struct_field(def, "display_name").unwrap();
    assert_eq!(
        display_name.presence.serialize_presence,
        dto_bindgen_core::SerializePresence::SkipIfNone
    );
    assert!(!display_name.presence.required_on_deserialize);
    assert_eq!(
        display_name.presence.default,
        Some(dto_bindgen::__private::DefaultKind::NoneValue)
    );
}

#[test]
fn export_types_macro_returns_blocking_diagnostics() {
    let config_path = temp_config("");

    let err = dto_bindgen::export_types!(
        config = config_path.as_path(),
        roots = [UnannotatedLedgerEntry],
    )
    .unwrap_err();

    let dto_bindgen::export::ExportError::Diagnostics(diagnostics) = err else {
        panic!("expected diagnostics error");
    };
    assert_eq!(
        diagnostics[0].code,
        dto_bindgen::diagnostics::DiagnosticCode::new(401)
    );

    cleanup_config(&config_path);
}

fn temp_config(contents: &str) -> std::path::PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "dto_bindgen_facade_export_test_{}_{}",
        std::process::id(),
        counter
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("dto_bindgen.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn cleanup_config(path: &std::path::Path) {
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

fn wire_field<'a>(def: &'a dto_bindgen::__private::StructDef, name: &str) -> Option<&'a str> {
    struct_field(def, name).map(|field| field.wire.serialize_name.as_str())
}

fn struct_field<'a>(
    def: &'a dto_bindgen::__private::StructDef,
    name: &str,
) -> Option<&'a dto_bindgen::__private::FieldDef> {
    def.fields
        .iter()
        .find(|field| field.rust_name.as_str() == name)
}

fn variant_wire_name<'a>(def: &'a dto_bindgen::__private::EnumDef, name: &str) -> Option<&'a str> {
    def.variants
        .iter()
        .find(|variant| variant.rust_name == name)
        .map(|variant| variant.wire_name.as_str())
}

fn variant_field_wire_name<'a>(
    def: &'a dto_bindgen::__private::EnumDef,
    variant_name: &str,
    field_name: &str,
) -> Option<&'a str> {
    variant_fields(def, variant_name)?
        .iter()
        .find(|field| field.rust_name.as_str() == field_name)
        .map(|field| field.wire.serialize_name.as_str())
}

fn variant_named_field(
    def: &dto_bindgen::__private::EnumDef,
    variant_name: &str,
    field_name: &str,
) -> Option<dto_bindgen::export::TypeRef> {
    variant_fields(def, variant_name)?
        .iter()
        .find(|field| field.rust_name.as_str() == field_name)
        .map(|field| field.ty.clone())
}

fn variant_fields<'a>(
    def: &'a dto_bindgen::__private::EnumDef,
    variant_name: &str,
) -> Option<&'a [dto_bindgen::__private::FieldDef]> {
    def.variants
        .iter()
        .find(|variant| variant.rust_name == variant_name)
        .and_then(|variant| match &variant.shape {
            dto_bindgen::__private::VariantShape::Struct(fields) => Some(fields.as_slice()),
            _ => None,
        })
}

fn first_named_field(
    type_def: &dto_bindgen::__private::TypeDef,
) -> Option<dto_bindgen::export::TypeRef> {
    match type_def {
        dto_bindgen::__private::TypeDef::Struct(def) => def
            .fields
            .iter()
            .find(|field| field.rust_name.as_str() == "address")
            .map(|field| field.ty.clone()),
        dto_bindgen::__private::TypeDef::Enum(_) => None,
    }
}
