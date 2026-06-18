#![forbid(unsafe_code)]

use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitStr, Type, parse_macro_input};

#[proc_macro_derive(Dto, attributes(dto, serde))]
pub fn derive_dto(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_dto(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_dto(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let container_attrs = StructContainerAttrs::parse(&input.attrs)?;

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            input.generics,
            "`Dto` derive does not support generic DTOs yet",
        ));
    }

    let ident = input.ident;
    match input.data {
        Data::Struct(data) => expand_struct(ident, data.fields, container_attrs),
        Data::Enum(_) => Err(syn::Error::new_spanned(
            ident,
            "`Dto` derive for enums is not implemented yet",
        )),
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "`Dto` derive does not support unions",
        )),
    }
}

fn expand_struct(
    ident: Ident,
    fields: Fields,
    container_attrs: StructContainerAttrs,
) -> syn::Result<proc_macro2::TokenStream> {
    let fields = match fields {
        Fields::Named(fields) => fields,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "`Dto` derive currently supports only structs with named fields",
            ));
        }
    };

    let mut field_tokens = Vec::new();

    for (index, field) in fields.named.into_iter().enumerate() {
        let Some(field_ident) = field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "`Dto` derive requires named fields",
            ));
        };
        let field_attrs = FieldAttrs::parse(&field.attrs)?;
        if field_attrs.skip {
            continue;
        }

        let field_var = format_ident!("__dto_bindgen_field_ty_{index}");
        let rust_name = clean_ident(&field_ident);
        let wire_name = field_attrs
            .rename
            .unwrap_or_else(|| container_attrs.rename_field(&rust_name));
        let ty = field.ty;

        field_tokens.push(field_descriptor_tokens(field_var, ty, rust_name, wire_name));
    }

    let export_name = container_attrs
        .rename
        .clone()
        .unwrap_or_else(|| ident.to_string());
    let rename_attr = option_string_tokens(container_attrs.rename.as_deref());
    let rename_all_attr = option_string_tokens(container_attrs.rename_all.as_deref());
    let deny_unknown_fields = container_attrs.deny_unknown_fields;

    Ok(quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                let __dto_bindgen_source =
                    ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!());
                let mut __dto_bindgen_def =
                    ::dto_bindgen::__private::StructDef::new(
                        stringify!(#ident),
                        #export_name,
                        __dto_bindgen_source.clone(),
                    );
                __dto_bindgen_def.attrs.rename = #rename_attr;
                __dto_bindgen_def.attrs.rename_all = #rename_all_attr;
                __dto_bindgen_def.attrs.deny_unknown_fields = #deny_unknown_fields;

                #(#field_tokens)*

                let __dto_bindgen_module_path = module_path!()
                    .split("::")
                    .skip(1)
                    .map(::std::string::ToString::to_string)
                    .collect::<::std::vec::Vec<_>>();
                let __dto_bindgen_rust_id =
                    ::dto_bindgen::__private::RustTypeId::new(
                        env!("CARGO_PKG_NAME"),
                        stringify!(#ident),
                    )
                    .with_module_path(__dto_bindgen_module_path);

                ctx.register_type(
                    __dto_bindgen_rust_id,
                    ::dto_bindgen::__private::TypeDef::Struct(__dto_bindgen_def),
                )
            }
        }
    })
}

fn field_descriptor_tokens(
    field_var: Ident,
    ty: Type,
    rust_name: String,
    wire_name: String,
) -> proc_macro2::TokenStream {
    quote! {
        let #field_var = <#ty as ::dto_bindgen::Dto>::describe(ctx);
        __dto_bindgen_def = __dto_bindgen_def.with_field(
            ::dto_bindgen::__private::FieldDef::new(
                ::dto_bindgen::__private::IdentName::new(#rust_name),
                ::dto_bindgen::__private::WireFieldNames::same(#wire_name),
                ::dto_bindgen::__private::TargetFieldNames::new(#wire_name, #rust_name),
                #field_var,
                ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!()),
            ),
        );
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StructContainerAttrs {
    rename: Option<String>,
    rename_all: Option<String>,
    deny_unknown_fields: bool,
}

impl StructContainerAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`Dto` derive does not support dto container attributes in this slice yet",
                ));
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_string_value(&meta)?);
                    Ok(())
                } else if meta.path.is_ident("rename_all") {
                    let rule = parse_string_value(&meta)?;
                    validate_rename_rule(&rule, &meta)?;
                    parsed.rename_all = Some(rule);
                    Ok(())
                } else if meta.path.is_ident("deny_unknown_fields") {
                    parsed.deny_unknown_fields = true;
                    Ok(())
                } else {
                    Err(meta.error("unsupported serde container attribute for `Dto` derive"))
                }
            })?;
        }

        Ok(parsed)
    }

    fn rename_field(&self, rust_name: &str) -> String {
        self.rename_all
            .as_deref()
            .map(|rule| apply_rename_rule(rule, rust_name))
            .unwrap_or_else(|| rust_name.to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FieldAttrs {
    rename: Option<String>,
    skip: bool,
}

impl FieldAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        parsed.skip = true;
                        Ok(())
                    } else {
                        Err(meta.error("unsupported dto field attribute for `Dto` derive"))
                    }
                })?;
                continue;
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_string_value(&meta)?);
                    Ok(())
                } else if meta.path.is_ident("skip") {
                    parsed.skip = true;
                    Ok(())
                } else {
                    Err(meta.error("unsupported serde field attribute for `Dto` derive"))
                }
            })?;
        }

        Ok(parsed)
    }
}

fn parse_string_value(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    let value = meta.value()?;
    let literal: LitStr = value.parse()?;
    Ok(literal.value())
}

fn validate_rename_rule(rule: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
    match rule {
        "camelCase" | "snake_case" | "PascalCase" | "SCREAMING_SNAKE_CASE" => Ok(()),
        _ => Err(meta.error("unsupported rename_all rule for `Dto` derive")),
    }
}

fn apply_rename_rule(rule: &str, rust_name: &str) -> String {
    match rule {
        "camelCase" => to_camel_case(rust_name),
        "PascalCase" => {
            let mut value = to_camel_case(rust_name);
            if let Some(first) = value.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            value
        }
        "SCREAMING_SNAKE_CASE" => rust_name.to_ascii_uppercase(),
        "snake_case" => rust_name.to_owned(),
        _ => rust_name.to_owned(),
    }
}

fn to_camel_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = false;

    for ch in value.chars() {
        if ch == '_' {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            output.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(ch);
        }
    }

    output
}

fn option_string_tokens(value: Option<&str>) -> proc_macro2::TokenStream {
    match value {
        Some(value) => quote!(::std::option::Option::Some(#value.to_owned())),
        None => quote!(::std::option::Option::None),
    }
}

fn clean_ident(ident: &Ident) -> String {
    let raw = ident.to_string();
    raw.strip_prefix("r#").unwrap_or(&raw).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_supported_rename_rules() {
        assert_eq!(apply_rename_rule("camelCase", "user_id"), "userId");
        assert_eq!(apply_rename_rule("PascalCase", "user_id"), "UserId");
        assert_eq!(
            apply_rename_rule("SCREAMING_SNAKE_CASE", "user_id"),
            "USER_ID"
        );
        assert_eq!(apply_rename_rule("snake_case", "user_id"), "user_id");
    }

    #[test]
    fn rejects_unsupported_field_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(flatten)]
                values: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported serde field attribute")
        );
    }
}
