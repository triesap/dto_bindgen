use dto_bindgen::{Dto, export};

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

    let dto_bindgen::export::TypeRef::Named(address_id) = first_named_field(root_def).unwrap()
    else {
        panic!("expected named field ref");
    };

    assert!(registry.dependencies_of(root).any(|dep| dep == address_id));
}

fn wire_field<'a>(def: &'a dto_bindgen::__private::StructDef, name: &str) -> Option<&'a str> {
    def.fields
        .iter()
        .find(|field| field.rust_name.as_str() == name)
        .map(|field| field.wire.serialize_name.as_str())
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
