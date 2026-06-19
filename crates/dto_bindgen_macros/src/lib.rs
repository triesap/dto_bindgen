#![forbid(unsafe_code)]

use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DataEnum, DeriveInput, Fields, Ident, LitStr, Token, Type, parse_macro_input,
};

#[proc_macro_derive(Dto, attributes(dto, serde))]
pub fn derive_dto(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_dto(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_dto(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            input.generics,
            "`Dto` derive does not support generic DTOs yet",
        ));
    }

    let attrs = input.attrs;
    let ident = input.ident;
    match input.data {
        Data::Struct(data) => {
            expand_struct(ident, data.fields, StructContainerAttrs::parse(&attrs)?)
        }
        Data::Enum(data) => expand_enum(ident, data, EnumContainerAttrs::parse(&attrs)?),
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "`Dto` derive does not support unions",
        )),
    }
}

fn expand_enum(
    ident: Ident,
    data: DataEnum,
    container_attrs: EnumContainerAttrs,
) -> syn::Result<proc_macro2::TokenStream> {
    let enum_repr = enum_repr_tokens(&container_attrs, &data)?;
    let mut variant_tokens = Vec::new();

    for (variant_index, variant) in data.variants.into_iter().enumerate() {
        let variant_attrs = VariantAttrs::parse(&variant.attrs)?;
        let rust_name = variant.ident.to_string();
        let wire_name = variant_attrs
            .rename
            .unwrap_or_else(|| container_attrs.rename_variant(&rust_name));

        match variant.fields {
            Fields::Unit => {
                variant_tokens.push(quote! {
                    __dto_bindgen_def = __dto_bindgen_def.with_variant(
                        ::dto_bindgen::__private::VariantDef::new(
                            #rust_name,
                            #wire_name,
                            ::dto_bindgen::__private::VariantShape::Unit,
                            ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!()),
                        ),
                    );
                });
            }
            Fields::Named(fields) => {
                if container_attrs.tag.is_none() {
                    return Err(syn::Error::new_spanned(
                        fields,
                        "externally tagged data enums are not supported in the MVP",
                    ));
                }

                let variant_fields_var =
                    format_ident!("__dto_bindgen_variant_fields_{variant_index}");
                let mut field_tokens = Vec::new();

                for (field_index, field) in fields.named.into_iter().enumerate() {
                    let Some(field_ident) = field.ident else {
                        return Err(syn::Error::new_spanned(
                            field,
                            "`Dto` derive requires named variant fields",
                        ));
                    };
                    let field_attrs = FieldAttrs::parse(&field.attrs)?;
                    if field_attrs.skip {
                        continue;
                    }

                    let field_var = format_ident!(
                        "__dto_bindgen_variant_{variant_index}_field_ty_{field_index}"
                    );
                    let rust_name = clean_ident(&field_ident);
                    let wire_name = field_attrs
                        .rename
                        .unwrap_or_else(|| container_attrs.rename_variant_field(&rust_name));
                    let ty = field.ty;
                    let field_expr = field_def_tokens(
                        &field_var,
                        &rust_name,
                        &wire_name,
                        &ty,
                        field_attrs.int_repr,
                        field_attrs.default,
                    )?;

                    field_tokens.push(quote! {
                        let #field_var = <#ty as ::dto_bindgen::Dto>::describe(ctx);
                        #variant_fields_var.push(#field_expr);
                    });
                }

                variant_tokens.push(quote! {
                    let mut #variant_fields_var = ::std::vec::Vec::new();
                    #(#field_tokens)*
                    __dto_bindgen_def = __dto_bindgen_def.with_variant(
                        ::dto_bindgen::__private::VariantDef::new(
                            #rust_name,
                            #wire_name,
                            ::dto_bindgen::__private::VariantShape::Struct(#variant_fields_var),
                            ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!()),
                        ),
                    );
                });
            }
            Fields::Unnamed(fields) => {
                return Err(syn::Error::new_spanned(
                    fields,
                    "`Dto` derive does not support tuple enum variants",
                ));
            }
        }
    }

    let export_name = container_attrs
        .rename
        .clone()
        .unwrap_or_else(|| ident.to_string());

    Ok(quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                let __dto_bindgen_source =
                    ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!());
                let mut __dto_bindgen_def =
                    ::dto_bindgen::__private::EnumDef::new(
                        stringify!(#ident),
                        #export_name,
                        #enum_repr,
                        __dto_bindgen_source,
                    );

                #(#variant_tokens)*

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
                    ::dto_bindgen::__private::TypeDef::Enum(__dto_bindgen_def),
                )
            }
        }
    })
}

fn enum_repr_tokens(
    attrs: &EnumContainerAttrs,
    data: &DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let has_unit = data
        .variants
        .iter()
        .any(|variant| matches!(variant.fields, Fields::Unit));
    let has_struct = data
        .variants
        .iter()
        .any(|variant| matches!(variant.fields, Fields::Named(_)));
    let has_tuple = data
        .variants
        .iter()
        .any(|variant| matches!(variant.fields, Fields::Unnamed(_)));

    if has_tuple {
        return Err(syn::Error::new_spanned(
            data.enum_token,
            "`Dto` derive does not support tuple enum variants",
        ));
    }

    if has_unit && has_struct {
        return Err(syn::Error::new_spanned(
            data.enum_token,
            "`Dto` derive does not support mixed unit and data enum variants",
        ));
    }

    match (&attrs.tag, &attrs.content, has_struct) {
        (None, None, false) => Ok(quote!(::dto_bindgen::__private::EnumRepr::External)),
        (None, Some(_), _) => Err(syn::Error::new_spanned(
            data.enum_token,
            "serde content requires serde tag for `Dto` derive",
        )),
        (Some(_), _, false) => Err(syn::Error::new_spanned(
            data.enum_token,
            "tagged fieldless enums are not supported in the MVP",
        )),
        (None, None, true) => Err(syn::Error::new_spanned(
            data.enum_token,
            "externally tagged data enums are not supported in the MVP",
        )),
        (Some(tag), None, true) => Ok(quote!(::dto_bindgen::__private::EnumRepr::Internal {
            tag: #tag.to_owned(),
        })),
        (Some(tag), Some(content), true) => {
            Ok(quote!(::dto_bindgen::__private::EnumRepr::Adjacent {
                tag: #tag.to_owned(),
                content: #content.to_owned(),
            }))
        }
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

        let field_expr = field_def_tokens(
            &field_var,
            &rust_name,
            &wire_name,
            &ty,
            field_attrs.int_repr,
            field_attrs.default || container_attrs.default,
        )?;
        field_tokens.push(quote! {
            let #field_var = <#ty as ::dto_bindgen::Dto>::describe(ctx);
            __dto_bindgen_def = __dto_bindgen_def.with_field(#field_expr);
        });
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

fn field_def_tokens(
    field_var: &Ident,
    rust_name: &str,
    wire_name: &str,
    ty: &Type,
    int_repr: Option<IntReprAttr>,
    serde_default: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    let int_repr_tokens = int_repr
        .map(|value| {
            let value = value.tokens();
            quote!(.with_int_repr(#value))
        })
        .unwrap_or_default();
    let presence_tokens = field_presence_tokens(ty, serde_default)?;

    Ok(quote! {
        ::dto_bindgen::__private::FieldDef::new(
            ::dto_bindgen::__private::IdentName::new(#rust_name),
            ::dto_bindgen::__private::WireFieldNames::same(#wire_name),
            ::dto_bindgen::__private::TargetFieldNames::new(#wire_name, #rust_name),
            #field_var,
            ::dto_bindgen::__private::SourceSpan::new(file!(), line!(), column!()),
        )#presence_tokens #int_repr_tokens
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StructContainerAttrs {
    rename: Option<String>,
    rename_all: Option<String>,
    deny_unknown_fields: bool,
    default: bool,
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
                } else if meta.path.is_ident("default") {
                    if meta.input.peek(Token![=]) {
                        let _ = parse_string_value(&meta)?;
                        Err(meta.error(
                            "custom serde container default paths are unsupported for `Dto` derive",
                        ))
                    } else {
                        parsed.default = true;
                        Ok(())
                    }
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
    default: bool,
    int_repr: Option<IntReprAttr>,
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
                    } else if meta.path.is_ident("int_repr") {
                        parsed.int_repr =
                            Some(IntReprAttr::parse(&parse_string_value(&meta)?, &meta)?);
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
                } else if meta.path.is_ident("default") {
                    if meta.input.peek(Token![=]) {
                        let _ = parse_string_value(&meta)?;
                        Err(meta
                            .error("custom serde default paths are unsupported for `Dto` derive"))
                    } else {
                        parsed.default = true;
                        Ok(())
                    }
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

fn field_presence_tokens(ty: &Type, serde_default: bool) -> syn::Result<proc_macro2::TokenStream> {
    if is_option_type(ty) {
        return Ok(quote!(
            .with_presence(::dto_bindgen::__private::FieldPresence::optional_nullable())
        ));
    }

    if !serde_default {
        return Ok(proc_macro2::TokenStream::new());
    }

    let default_kind = default_kind_tokens(ty)?;
    Ok(quote!(
        .with_presence(::dto_bindgen::__private::FieldPresence::defaulted(#default_kind))
    ))
}

fn default_kind_tokens(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    let Some(ident) = last_type_ident(ty) else {
        return Err(syn::Error::new_spanned(
            ty,
            "serde(default) is supported only for built-in DTO field types",
        ));
    };
    let ident = ident.to_string();

    match ident.as_str() {
        "String" => Ok(quote!(::dto_bindgen::__private::DefaultKind::EmptyString)),
        "Vec" => Ok(quote!(::dto_bindgen::__private::DefaultKind::EmptyVec)),
        "HashMap" | "BTreeMap" => Ok(quote!(::dto_bindgen::__private::DefaultKind::EmptyMap)),
        "bool" => Ok(quote!(::dto_bindgen::__private::DefaultKind::BoolFalse)),
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "i128" | "u128" | "isize"
        | "usize" | "f32" | "f64" => Ok(quote!(::dto_bindgen::__private::DefaultKind::NumericZero)),
        _ => Err(syn::Error::new_spanned(
            ty,
            "serde(default) is supported only for Option, String, bool, numeric, Vec, and string-keyed map fields",
        )),
    }
}

fn is_option_type(ty: &Type) -> bool {
    last_type_ident(ty)
        .map(|ident| ident == "Option")
        .unwrap_or(false)
}

fn last_type_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };

    path.path.segments.last().map(|segment| &segment.ident)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntReprAttr {
    JsonString,
    JsonNumberUnsafe,
    NonJsonBigint,
}

impl IntReprAttr {
    fn parse(value: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Self> {
        match value {
            "json_string" => Ok(Self::JsonString),
            "json_number_unsafe" => Ok(Self::JsonNumberUnsafe),
            "non_json_bigint" => Ok(Self::NonJsonBigint),
            _ => Err(meta.error("unsupported dto int_repr value")),
        }
    }

    fn tokens(self) -> proc_macro2::TokenStream {
        match self {
            Self::JsonString => quote!(::dto_bindgen::__private::IntRepr::JsonString),
            Self::JsonNumberUnsafe => {
                quote!(::dto_bindgen::__private::IntRepr::JsonNumberUnsafe)
            }
            Self::NonJsonBigint => quote!(::dto_bindgen::__private::IntRepr::NonJsonBigint),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EnumContainerAttrs {
    rename: Option<String>,
    rename_all: Option<String>,
    rename_all_fields: Option<String>,
    tag: Option<String>,
    content: Option<String>,
}

impl EnumContainerAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`Dto` derive does not support dto enum attributes in this slice yet",
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
                } else if meta.path.is_ident("rename_all_fields") {
                    let rule = parse_string_value(&meta)?;
                    validate_rename_rule(&rule, &meta)?;
                    parsed.rename_all_fields = Some(rule);
                    Ok(())
                } else if meta.path.is_ident("tag") {
                    parsed.tag = Some(parse_string_value(&meta)?);
                    Ok(())
                } else if meta.path.is_ident("content") {
                    parsed.content = Some(parse_string_value(&meta)?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported serde enum attribute for `Dto` derive"))
                }
            })?;
        }

        Ok(parsed)
    }

    fn rename_variant(&self, rust_name: &str) -> String {
        self.rename_all
            .as_deref()
            .map(|rule| apply_rename_rule(rule, rust_name))
            .unwrap_or_else(|| rust_name.to_owned())
    }

    fn rename_variant_field(&self, rust_name: &str) -> String {
        self.rename_all_fields
            .as_deref()
            .map(|rule| apply_rename_rule(rule, rust_name))
            .unwrap_or_else(|| rust_name.to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct VariantAttrs {
    rename: Option<String>,
}

impl VariantAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`Dto` derive does not support dto variant attributes in this slice yet",
                ));
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_string_value(&meta)?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported serde variant attribute for `Dto` derive"))
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
        "PascalCase" => to_pascal_case(rust_name),
        "SCREAMING_SNAKE_CASE" => to_screaming_snake_case(rust_name),
        "snake_case" => rust_name.to_owned(),
        _ => rust_name.to_owned(),
    }
}

fn to_camel_case(value: &str) -> String {
    if !value.contains('_') {
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return String::new();
        };
        return first.to_lowercase().chain(chars).collect();
    }

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

fn to_pascal_case(value: &str) -> String {
    let mut value = to_camel_case(value);
    if let Some(first) = value.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    value
}

fn to_screaming_snake_case(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;

    for ch in value.chars() {
        if ch == '_' {
            if !output.ends_with('_') {
                output.push('_');
            }
            previous_was_separator = true;
            continue;
        }

        if ch.is_ascii_uppercase() && !previous_was_separator && !output.ends_with('_') {
            output.push('_');
        }

        output.extend(ch.to_uppercase());
        previous_was_separator = false;
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
        assert_eq!(apply_rename_rule("camelCase", "GuestUser"), "guestUser");
        assert_eq!(apply_rename_rule("PascalCase", "user_id"), "UserId");
        assert_eq!(
            apply_rename_rule("SCREAMING_SNAKE_CASE", "user_id"),
            "USER_ID"
        );
        assert_eq!(
            apply_rename_rule("SCREAMING_SNAKE_CASE", "GuestUser"),
            "GUEST_USER"
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

    #[test]
    fn rejects_serde_with_for_large_integer_strings() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[serde(with = "my_u128_string_serde")]
                #[dto(int_repr = "json_string")]
                amount: u128,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported serde field attribute")
        );
    }

    #[test]
    fn rejects_custom_default_paths() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(default = "fallback")]
                values: Vec<String>,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("custom serde default paths"));
    }

    #[test]
    fn rejects_custom_container_default_paths() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(default = "fallback")]
            struct Metadata {
                values: Vec<String>,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("custom serde container default paths")
        );
    }

    #[test]
    fn rejects_default_for_unmapped_field_types() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(default)]
                nested: PostalAddress,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("serde(default) is supported only"));
    }

    #[test]
    fn rejects_container_default_for_unmapped_field_types() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(default)]
            struct Metadata {
                nested: PostalAddress,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("serde(default) is supported only"));
    }

    #[test]
    fn rejects_unknown_int_repr_values() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[dto(int_repr = "magic")]
                amount: u128,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("unsupported dto int_repr"));
    }

    #[test]
    fn rejects_enum_data_variants() {
        let input: DeriveInput = syn::parse_quote! {
            enum Event {
                UserCreated { user_id: String },
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("externally tagged data enums"));
    }
}
