use dto_bindgen::{Dto, export};

#[allow(dead_code)]
#[derive(Dto)]
struct PostalAddress {
    line_1: String,
}

#[allow(dead_code)]
#[derive(Dto)]
struct UserProfile {
    user_id: String,
    active: bool,
    address: PostalAddress,
    tags: Vec<String>,
}

#[test]
fn derives_named_struct_descriptors() {
    let registry = export::build_registry([export::RootDescriptor::new::<UserProfile>()]);

    assert!(registry.diagnostics.is_empty());
    assert_eq!(registry.types_by_id.len(), 2);
    assert_eq!(registry.roots.len(), 1);

    let root = *registry.roots.iter().next().unwrap();
    let dto_bindgen::export::TypeRef::Named(address_id) =
        registry.type_def(root).and_then(first_named_field).unwrap()
    else {
        panic!("expected named field ref");
    };

    assert!(registry.dependencies_of(root).any(|dep| dep == address_id));
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
