#![forbid(unsafe_code)]

use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DataEnum, DeriveInput, Fields, GenericArgument, Ident, LitStr, PathArguments,
    Token, Type, parse_macro_input, spanned::Spanned,
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
    if container_attrs.dto_as == Some(DtoAsAttr::String) {
        return Err(syn::Error::new_spanned(
            ident,
            "dto(as = \"string\") is supported only for structs and fields",
        ));
    }
    let enum_repr = enum_repr_tokens(&container_attrs, &data)?;
    let enum_source = source_span_tokens(&ident);
    let mut variant_tokens = Vec::new();

    for (variant_index, variant) in data.variants.into_iter().enumerate() {
        let variant_attrs = VariantAttrs::parse(&variant.attrs)?;
        let rust_name = variant.ident.to_string();
        let variant_source = source_span_tokens(&variant.ident);
        if container_attrs.dto_as == Some(DtoAsAttr::StringEnum) && variant_attrs.rename.is_none() {
            return Err(syn::Error::new_spanned(
                variant.ident,
                "dto(as = \"string_enum\") requires #[dto(rename = \"...\")] on every variant",
            ));
        }
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
                            #variant_source,
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
                    let field_source = source_span_tokens(&field_ident);
                    let wire_name = field_attrs
                        .rename
                        .unwrap_or_else(|| container_attrs.rename_variant_field(&rust_name));
                    let ty = field.ty;
                    let field_ty_expr = type_ref_expr_tokens(
                        &ty,
                        field_attrs.dto_as,
                        field_attrs.bytes_repr,
                        field_attrs.ts_type,
                    )?;
                    let field_expr = field_def_tokens(FieldDefTokens {
                        field_var: &field_var,
                        rust_name: &rust_name,
                        wire_name: &wire_name,
                        ty: &ty,
                        int_repr: field_attrs.int_repr,
                        serde_default: field_attrs.default,
                        skip_serializing_if: field_attrs.skip_serializing_if,
                        source: field_source,
                    })?;

                    field_tokens.push(quote! {
                        let #field_var = #field_ty_expr;
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
                            #variant_source,
                        ),
                    );
                });
            }
            Fields::Unnamed(fields) => {
                if container_attrs.content.is_none() || fields.unnamed.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        fields,
                        "`Dto` derive supports only one-field adjacent tagged tuple variants",
                    ));
                }
                let field = fields
                    .unnamed
                    .into_iter()
                    .next()
                    .expect("length already checked");
                let field_attrs = FieldAttrs::parse(&field.attrs)?;
                if field_attrs.skip {
                    return Err(syn::Error::new_spanned(
                        field,
                        "tuple variant payloads cannot be skipped",
                    ));
                }
                let ty = field.ty;
                let ty_expr = type_ref_expr_tokens(
                    &ty,
                    field_attrs.dto_as,
                    field_attrs.bytes_repr,
                    field_attrs.ts_type,
                )?;

                variant_tokens.push(quote! {
                    let __dto_bindgen_variant_ty = #ty_expr;
                    __dto_bindgen_def = __dto_bindgen_def.with_variant(
                        ::dto_bindgen::__private::VariantDef::new(
                            #rust_name,
                            #wire_name,
                            ::dto_bindgen::__private::VariantShape::Newtype(__dto_bindgen_variant_ty),
                            #variant_source,
                        ),
                    );
                });
            }
        }
    }

    let export_name = container_attrs
        .rename
        .clone()
        .unwrap_or_else(|| ident.to_string());
    let ts_name_attr = option_string_tokens(container_attrs.ts_name.as_deref());

    Ok(quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                let __dto_bindgen_source =
                    #enum_source;
                let mut __dto_bindgen_def =
                    ::dto_bindgen::__private::EnumDef::new(
                        stringify!(#ident),
                        #export_name,
                        #enum_repr,
                        __dto_bindgen_source,
                    );
                __dto_bindgen_def.attrs.ts_name = #ts_name_attr;

                #(#variant_tokens)*

                let __dto_bindgen_module_path = module_path!()
                    .split("::")
                    .skip(1)
                    .map(::std::string::ToString::to_string)
                    .collect::<::std::vec::Vec<_>>();
                let __dto_bindgen_rust_id =
                    ::dto_bindgen::__private::RustTypeId::new(
                        env!("CARGO_PKG_NAME"),
                        env!("CARGO_CRATE_NAME"),
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

    if attrs.dto_as == Some(DtoAsAttr::StringEnum) {
        if attrs.tag.is_some() || attrs.content.is_some() || has_struct || has_tuple {
            return Err(syn::Error::new_spanned(
                data.enum_token,
                "dto(as = \"string_enum\") requires an untagged fieldless enum",
            ));
        }
        return Ok(quote!(::dto_bindgen::__private::EnumRepr::External));
    }

    if has_tuple && attrs.content.is_none() {
        return Err(syn::Error::new_spanned(
            data.enum_token,
            "`Dto` derive supports tuple variants only for adjacent tagged enums",
        ));
    }

    if has_unit && has_struct && attrs.content.is_none() {
        return Err(syn::Error::new_spanned(
            data.enum_token,
            "`Dto` derive supports mixed unit and data enum variants only for adjacent tagged enums",
        ));
    }

    match (&attrs.tag, &attrs.content, has_struct || has_tuple) {
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
    if container_attrs.dto_as == Some(DtoAsAttr::String) {
        return Ok(expand_string_mapped_type(ident));
    }

    let fields = match fields {
        Fields::Named(fields) => fields,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "`Dto` derive currently supports only structs with named fields",
            ));
        }
    };

    if container_attrs.dto_as == Some(DtoAsAttr::StringEnum) {
        return Err(syn::Error::new_spanned(
            ident,
            "dto(as = \"string_enum\") is supported only for enums",
        ));
    }

    if container_attrs.transparent {
        return expand_transparent_struct(ident, fields);
    }

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
        let field_source = source_span_tokens(&field_ident);
        let wire_name = field_attrs
            .rename
            .unwrap_or_else(|| container_attrs.rename_field(&rust_name));
        let ty = field.ty;
        let dto_as = field_attrs.dto_as;
        let bytes_repr = field_attrs.bytes_repr;
        let ts_type = field_attrs.ts_type;

        let field_expr = field_def_tokens(FieldDefTokens {
            field_var: &field_var,
            rust_name: &rust_name,
            wire_name: &wire_name,
            ty: &ty,
            int_repr: field_attrs.int_repr,
            serde_default: field_attrs.default || container_attrs.default,
            skip_serializing_if: field_attrs.skip_serializing_if,
            source: field_source,
        })?;
        let field_ty_expr = type_ref_expr_tokens(&ty, dto_as, bytes_repr, ts_type)?;
        field_tokens.push(quote! {
            let #field_var = #field_ty_expr;
            __dto_bindgen_def = __dto_bindgen_def.with_field(#field_expr);
        });
    }

    let export_name = container_attrs
        .rename
        .clone()
        .unwrap_or_else(|| ident.to_string());
    let rename_attr = option_string_tokens(container_attrs.rename.as_deref());
    let rename_all_attr = option_string_tokens(container_attrs.rename_all.as_deref());
    let ts_name_attr = option_string_tokens(container_attrs.ts_name.as_deref());
    let deny_unknown_fields = container_attrs.deny_unknown_fields;
    let struct_source = source_span_tokens(&ident);

    Ok(quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                let __dto_bindgen_source =
                    #struct_source;
                let mut __dto_bindgen_def =
                    ::dto_bindgen::__private::StructDef::new(
                        stringify!(#ident),
                        #export_name,
                        __dto_bindgen_source.clone(),
                );
                __dto_bindgen_def.attrs.rename = #rename_attr;
                __dto_bindgen_def.attrs.rename_all = #rename_all_attr;
                __dto_bindgen_def.attrs.ts_name = #ts_name_attr;
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
                        env!("CARGO_CRATE_NAME"),
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

fn expand_string_mapped_type(ident: Ident) -> proc_macro2::TokenStream {
    quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                _ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                ::dto_bindgen::__private::TypeRef::String
            }
        }
    }
}

fn expand_transparent_struct(
    ident: Ident,
    fields: syn::FieldsNamed,
) -> syn::Result<proc_macro2::TokenStream> {
    if fields.named.len() != 1 {
        return Err(syn::Error::new_spanned(
            fields,
            "serde(transparent) DTO structs must have exactly one field",
        ));
    }
    let field = fields
        .named
        .into_iter()
        .next()
        .expect("length already checked");
    let field_attrs = FieldAttrs::parse(&field.attrs)?;
    if field_attrs.skip {
        return Err(syn::Error::new_spanned(
            field,
            "serde(transparent) DTO field cannot be skipped",
        ));
    }
    let ty = field.ty;
    let ty_expr = type_ref_expr_tokens(
        &ty,
        field_attrs.dto_as,
        field_attrs.bytes_repr,
        field_attrs.ts_type,
    )?;

    Ok(quote! {
        impl ::dto_bindgen::Dto for #ident {
            fn describe(
                ctx: &mut ::dto_bindgen::__private::DescribeCtx,
            ) -> ::dto_bindgen::__private::TypeRef {
                #ty_expr
            }
        }
    })
}

struct FieldDefTokens<'a> {
    field_var: &'a Ident,
    rust_name: &'a str,
    wire_name: &'a str,
    ty: &'a Type,
    int_repr: Option<IntReprAttr>,
    serde_default: bool,
    skip_serializing_if: Option<String>,
    source: proc_macro2::TokenStream,
}

fn field_def_tokens(args: FieldDefTokens<'_>) -> syn::Result<proc_macro2::TokenStream> {
    let FieldDefTokens {
        field_var,
        rust_name,
        wire_name,
        ty,
        int_repr,
        serde_default,
        skip_serializing_if,
        source,
    } = args;
    let int_repr_tokens = int_repr
        .map(|value| {
            let value = value.tokens();
            quote!(.with_int_repr(#value))
        })
        .unwrap_or_default();
    let presence_tokens = field_presence_tokens(ty, serde_default, skip_serializing_if.as_deref())?;

    Ok(quote! {
        ::dto_bindgen::__private::FieldDef::new(
            ::dto_bindgen::__private::IdentName::new(#rust_name),
            ::dto_bindgen::__private::WireFieldNames::same(#wire_name),
            ::dto_bindgen::__private::TargetFieldNames::new(#wire_name, #rust_name),
            #field_var,
            #source,
        )#presence_tokens #int_repr_tokens
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StructContainerAttrs {
    rename: Option<String>,
    rename_all: Option<String>,
    ts_name: Option<String>,
    dto_as: Option<DtoAsAttr>,
    export: bool,
    deny_unknown_fields: bool,
    default: bool,
    transparent: bool,
}

impl StructContainerAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                parse_dto_container_attr(
                    attr,
                    &mut parsed.ts_name,
                    &mut parsed.dto_as,
                    &mut parsed.export,
                )?;
                continue;
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_serde_rename_value(&meta)?);
                    Ok(())
                } else if meta.path.is_ident("rename_all") {
                    let rule = parse_string_value(&meta)?;
                    validate_rename_rule(&rule, &meta)?;
                    parsed.rename_all = Some(rule);
                    Ok(())
                } else if meta.path.is_ident("deny_unknown_fields") {
                    parsed.deny_unknown_fields = true;
                    Ok(())
                } else if meta.path.is_ident("transparent") {
                    parsed.transparent = true;
                    Ok(())
                } else if meta.path.is_ident("default") {
                    if meta.input.peek(Token![=]) {
                        let _ = parse_string_value(&meta)?;
                        Err(meta.error(
                            "custom serde container default paths are unsupported for `Dto` derive",
                        ))
                    } else {
                        Err(meta.error("serde container default is unsupported for `Dto` derive"))
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
    skip_serializing_if: Option<String>,
    int_repr: Option<IntReprAttr>,
    dto_as: Option<DtoAsAttr>,
    bytes_repr: Option<BytesReprAttr>,
    ts_type: Option<String>,
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
                    } else if meta.path.is_ident("as") {
                        let dto_as = DtoAsAttr::parse(&parse_string_value(&meta)?, &meta)?;
                        if dto_as != DtoAsAttr::String {
                            return Err(meta
                                .error("dto field as mapping supports only dto(as = \"string\")"));
                        }
                        parsed.dto_as = Some(dto_as);
                        Ok(())
                    } else if meta.path.is_ident("int") || meta.path.is_ident("int_repr") {
                        parsed.int_repr =
                            Some(IntReprAttr::parse(&parse_string_value(&meta)?, &meta)?);
                        Ok(())
                    } else if meta.path.is_ident("bytes") {
                        parsed.bytes_repr =
                            Some(BytesReprAttr::parse(&parse_string_value(&meta)?, &meta)?);
                        Ok(())
                    } else if meta.path.is_ident("ts") {
                        meta.parse_nested_meta(|meta| {
                            if meta.path.is_ident("type") {
                                parsed.ts_type = Some(parse_string_value(&meta)?);
                                Ok(())
                            } else {
                                Err(meta.error(
                                    "dto field ts mapping supports only dto(ts(type = \"...\"))",
                                ))
                            }
                        })
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
                    parsed.rename = Some(parse_serde_rename_value(&meta)?);
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
                } else if meta.path.is_ident("skip_serializing_if") {
                    let value = parse_string_value(&meta)?;
                    if value == "Option::is_none" {
                        parsed.skip_serializing_if = Some(value);
                        Ok(())
                    } else {
                        Err(meta.error(
                            "only serde(skip_serializing_if = \"Option::is_none\") is supported for `Dto` derive",
                        ))
                    }
                } else if meta.path.is_ident("alias") {
                    Err(meta.error("unsupported serde field attribute `alias` for `Dto` derive"))
                } else if meta.path.is_ident("skip_serializing")
                    || meta.path.is_ident("skip_deserializing")
                {
                    Err(meta.error(
                        "split-direction serde skip attributes are unsupported for `Dto` derive",
                    ))
                } else {
                    Err(meta.error("unsupported serde field attribute for `Dto` derive"))
                }
            })?;
        }

        if parsed.ts_type.is_some()
            && (parsed.dto_as.is_some() || parsed.bytes_repr.is_some() || parsed.int_repr.is_some())
        {
            return Err(syn::Error::new_spanned(
                attrs
                    .first()
                    .expect("ts_type is parsed only when attrs are present"),
                "dto(ts(type = \"...\")) cannot be combined with dto(as), dto(bytes), dto(int), or dto(int_repr)",
            ));
        }

        Ok(parsed)
    }
}

fn type_ref_expr_tokens(
    ty: &Type,
    dto_as: Option<DtoAsAttr>,
    bytes_repr: Option<BytesReprAttr>,
    ts_type: Option<String>,
) -> syn::Result<proc_macro2::TokenStream> {
    if dto_as.is_some() && bytes_repr.is_some() {
        return Err(syn::Error::new_spanned(
            ty,
            "dto(as = \"...\") and dto(bytes = \"...\") cannot be combined",
        ));
    }

    if let Some(target_type) = ts_type {
        return Ok(quote!(
            ::dto_bindgen::__private::TypeRef::Override(
                ::dto_bindgen::__private::TargetOverride::new(
                    ::dto_bindgen::__private::BackendId::TypeScript,
                    #target_type,
                )
            )
        ));
    }

    if let Some(bytes_repr) = bytes_repr {
        let bytes_repr = bytes_repr.tokens();
        let bytes_type = quote!(
            ::dto_bindgen::__private::TypeRef::Bytes(#bytes_repr)
        );
        if is_vec_u8_type(ty) {
            return Ok(bytes_type);
        }
        if option_inner_type(ty).is_some_and(is_vec_u8_type) {
            return Ok(quote!(
                ::dto_bindgen::__private::TypeRef::option(#bytes_type)
            ));
        }
        return Err(syn::Error::new_spanned(
            ty,
            "dto(bytes = \"base64\") is supported only for Vec<u8> or Option<Vec<u8>> fields",
        ));
    }

    match dto_as {
        Some(DtoAsAttr::String) => Ok(quote!(::dto_bindgen::__private::TypeRef::String)),
        Some(DtoAsAttr::StringEnum) => Err(syn::Error::new_spanned(
            ty,
            "dto(as = \"string_enum\") is supported only for enum containers",
        )),
        None => Ok(quote!(<#ty as ::dto_bindgen::Dto>::describe(ctx))),
    }
}

fn is_vec_u8_type(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Vec" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    let Some(GenericArgument::Type(Type::Path(item))) = args.args.first() else {
        return false;
    };
    item.path
        .segments
        .last()
        .map(|segment| segment.ident == "u8")
        .unwrap_or(false)
}

fn field_presence_tokens(
    ty: &Type,
    serde_default: bool,
    skip_serializing_if: Option<&str>,
) -> syn::Result<proc_macro2::TokenStream> {
    if is_option_type(ty) {
        if skip_serializing_if == Some("Option::is_none") {
            return Ok(quote!(
                .with_presence(::dto_bindgen::__private::FieldPresence::optional_nullable_skip_if_none())
            ));
        }
        return Ok(quote!(
            .with_presence(::dto_bindgen::__private::FieldPresence::optional_nullable())
        ));
    }

    if skip_serializing_if == Some("Option::is_none") {
        return Err(syn::Error::new_spanned(
            ty,
            "serde(skip_serializing_if = \"Option::is_none\") is supported only for Option fields",
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
    option_inner_type(ty).is_some()
}

fn last_type_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };

    path.path.segments.last().map(|segment| &segment.ident)
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let Some(GenericArgument::Type(inner)) = args.args.first() else {
        return None;
    };
    Some(inner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntReprAttr {
    JsonString,
    JsonNumber,
}

impl IntReprAttr {
    fn parse(value: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Self> {
        match value {
            "json_string" => Ok(Self::JsonString),
            "json_number" => Ok(Self::JsonNumber),
            _ => Err(meta.error("unsupported dto int_repr value")),
        }
    }

    fn tokens(self) -> proc_macro2::TokenStream {
        match self {
            Self::JsonString => quote!(::dto_bindgen::__private::IntRepr::JsonString),
            Self::JsonNumber => quote!(::dto_bindgen::__private::IntRepr::JsonNumber),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BytesReprAttr {
    Base64String,
}

impl BytesReprAttr {
    fn parse(value: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Self> {
        match value {
            "base64" => Ok(Self::Base64String),
            _ => Err(meta.error("unsupported dto bytes value")),
        }
    }

    fn tokens(self) -> proc_macro2::TokenStream {
        match self {
            Self::Base64String => quote!(::dto_bindgen::__private::BytesRepr::Base64String),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EnumContainerAttrs {
    rename: Option<String>,
    rename_all: Option<String>,
    rename_all_fields: Option<String>,
    ts_name: Option<String>,
    dto_as: Option<DtoAsAttr>,
    export: bool,
    tag: Option<String>,
    content: Option<String>,
}

impl EnumContainerAttrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut parsed = Self::default();

        for attr in attrs {
            if attr.path().is_ident("dto") {
                parse_dto_container_attr(
                    attr,
                    &mut parsed.ts_name,
                    &mut parsed.dto_as,
                    &mut parsed.export,
                )?;
                continue;
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_serde_rename_value(&meta)?);
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
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        parsed.rename = Some(parse_string_value(&meta)?);
                        Ok(())
                    } else {
                        Err(meta.error("unsupported dto variant attribute for `Dto` derive"))
                    }
                })?;
                continue;
            }

            if !attr.path().is_ident("serde") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(parse_serde_rename_value(&meta)?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported serde variant attribute for `Dto` derive"))
                }
            })?;
        }

        Ok(parsed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DtoAsAttr {
    String,
    StringEnum,
}

impl DtoAsAttr {
    fn parse(value: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Self> {
        match value {
            "string" => Ok(Self::String),
            "string_enum" => Ok(Self::StringEnum),
            _ => Err(meta.error("unsupported dto as mapping")),
        }
    }
}

fn parse_dto_container_attr(
    attr: &Attribute,
    ts_name: &mut Option<String>,
    dto_as: &mut Option<DtoAsAttr>,
    export: &mut bool,
) -> syn::Result<()> {
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("ts") {
            meta.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    *ts_name = Some(parse_string_value(&meta)?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported dto ts container attribute for `Dto` derive"))
                }
            })
        } else if meta.path.is_ident("as") {
            *dto_as = Some(DtoAsAttr::parse(&parse_string_value(&meta)?, &meta)?);
            Ok(())
        } else if meta.path.is_ident("export") {
            if !meta.input.is_empty() {
                return Err(meta.error("dto(export) does not accept arguments"));
            }
            *export = true;
            Ok(())
        } else {
            Err(meta.error("unsupported dto container attribute for `Dto` derive"))
        }
    })
}

fn parse_string_value(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    let value = meta.value()?;
    let literal: LitStr = value.parse()?;
    Ok(literal.value())
}

fn parse_serde_rename_value(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    if meta.input.peek(Token![=]) {
        parse_string_value(meta)
    } else {
        Err(meta.error("split serialize/deserialize rename is unsupported for `Dto` derive"))
    }
}

fn validate_rename_rule(rule: &str, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
    match rule {
        "camelCase"
        | "snake_case"
        | "PascalCase"
        | "SCREAMING_SNAKE_CASE"
        | "lowercase"
        | "kebab-case" => Ok(()),
        _ => Err(meta.error("unsupported rename_all rule for `Dto` derive")),
    }
}

fn apply_rename_rule(rule: &str, rust_name: &str) -> String {
    match rule {
        "camelCase" => to_camel_case(rust_name),
        "PascalCase" => to_pascal_case(rust_name),
        "SCREAMING_SNAKE_CASE" => to_screaming_snake_case(rust_name),
        "lowercase" => rust_name.to_lowercase(),
        "kebab-case" => to_kebab_case(rust_name),
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

fn to_kebab_case(value: &str) -> String {
    to_screaming_snake_case(value)
        .to_lowercase()
        .replace('_', "-")
}

fn option_string_tokens(value: Option<&str>) -> proc_macro2::TokenStream {
    match value {
        Some(value) => quote!(::std::option::Option::Some(#value.to_owned())),
        None => quote!(::std::option::Option::None),
    }
}

fn source_span_tokens<T>(node: &T) -> proc_macro2::TokenStream
where
    T: Spanned,
{
    let span = node.span();
    let start = span.start();
    let end = span.end();
    let file = span.file();
    let start_line = start.line as u32;
    let start_column = start.column as u32;
    let end_line = end.line as u32;
    let end_column = end.column as u32;

    quote!(
        ::dto_bindgen::__private::SourceSpan::new(#file, #start_line, #start_column)
            .with_end(#end_line, #end_column)
    )
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
        assert_eq!(apply_rename_rule("lowercase", "DateTime"), "datetime");
        assert_eq!(apply_rename_rule("kebab-case", "StaticJson"), "static-json");
        assert_eq!(
            apply_rename_rule("kebab-case", "static_json"),
            "static-json"
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
    fn accepts_container_typescript_name_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(ts(name = "Mf2WebManifest"))]
            struct Manifest {
                schema: u32,
            }
        };

        let tokens = expand_dto(input).expect("expand");

        assert!(tokens.to_string().contains("Mf2WebManifest"));
        assert!(tokens.to_string().contains("CARGO_PKG_NAME"));
        assert!(tokens.to_string().contains("CARGO_CRATE_NAME"));
    }

    #[test]
    fn rejects_container_typescript_type_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(ts(type = "Mf2WebManifest"))]
            struct Manifest {
                schema: u32,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("container attribute"));
    }

    #[test]
    fn accepts_enum_typescript_name_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(ts(name = "Mf2ArgType"))]
            enum ArgType {
                String,
                Number,
            }
        };

        let tokens = expand_dto(input).expect("expand");

        assert!(tokens.to_string().contains("Mf2ArgType"));
    }

    #[test]
    fn rejects_unsupported_container_dto_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(py(name = "ManifestDto"))]
            struct Manifest {
                schema: u32,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported dto container attribute")
        );
    }

    #[test]
    fn accepts_dto_export_container_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(export)]
            struct UserProfile {
                id: String,
            }
        };

        let tokens = expand_dto(input).expect("expand");

        assert!(tokens.to_string().contains("UserProfile"));
    }

    #[test]
    fn emits_concrete_source_spans() {
        let input: DeriveInput = syn::parse_quote! {
            struct UserProfile {
                id: String,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("SourceSpan :: new"));
        assert!(tokens.contains("with_end"));
        assert!(!tokens.contains("file !"));
        assert!(!tokens.contains("line !"));
        assert!(!tokens.contains("column !"));
    }

    #[test]
    fn rejects_dto_export_arguments() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(export = true)]
            struct UserProfile {
                id: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("dto(export) does not accept"));
    }

    #[test]
    fn supports_type_level_string_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(as = "string")]
            struct Decimal(String);
        };

        let tokens = expand_dto(input).expect("expand");
        let tokens = tokens.to_string();

        assert!(tokens.contains("TypeRef :: String"));
        assert!(!tokens.contains("StructDef"));
    }

    #[test]
    fn supports_field_level_string_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Price {
                #[dto(as = "string")]
                amount: Decimal,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("TypeRef :: String"));
        assert!(!tokens.contains("< Decimal as :: dto_bindgen :: Dto >"));
    }

    #[test]
    fn supports_field_level_typescript_type_override() {
        let input: DeriveInput = syn::parse_quote! {
            struct Manifest {
                #[dto(ts(type = "ReadonlyArray<string>"))]
                tags: Vec<String>,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("TypeRef :: Override"));
        assert!(tokens.contains("BackendId :: TypeScript"));
        assert!(tokens.contains("ReadonlyArray<string>"));
        assert!(!tokens.contains("< Vec < String > as :: dto_bindgen :: Dto >"));
    }

    #[test]
    fn rejects_field_level_typescript_name_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            struct Manifest {
                #[dto(ts(name = "Tags"))]
                tags: Vec<String>,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("field ts mapping"));
    }

    #[test]
    fn rejects_mixed_field_typescript_type_override_mappings() {
        let input: DeriveInput = syn::parse_quote! {
            struct Manifest {
                #[dto(ts(type = "string"), as = "string")]
                id: Uuid,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("cannot be combined"));
    }

    #[test]
    fn supports_field_level_base64_bytes_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Attachment {
                #[dto(bytes = "base64")]
                payload: Vec<u8>,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("TypeRef :: Bytes"));
        assert!(tokens.contains("BytesRepr :: Base64String"));
        assert!(!tokens.contains("< Vec < u8 > as :: dto_bindgen :: Dto >"));
    }

    #[test]
    fn supports_optional_field_level_base64_bytes_mapping() {
        let input: DeriveInput = syn::parse_quote! {
            struct Attachment {
                #[dto(bytes = "base64")]
                payload: Option<Vec<u8>>,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("TypeRef :: option"));
        assert!(tokens.contains("TypeRef :: Bytes"));
        assert!(tokens.contains("BytesRepr :: Base64String"));
        assert!(!tokens.contains("< Option < Vec < u8 > > as :: dto_bindgen :: Dto >"));
    }

    #[test]
    fn rejects_bytes_mapping_for_non_bytes_fields() {
        let input: DeriveInput = syn::parse_quote! {
            struct Attachment {
                #[dto(bytes = "base64")]
                payload: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("Option<Vec<u8>>"));
    }

    #[test]
    fn supports_string_enum_mapping_with_dto_variant_renames() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(as = "string_enum")]
            enum Unit {
                #[dto(rename = "kg")]
                Kilogram,
                #[dto(rename = "lb")]
                Pound,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("\"kg\""));
        assert!(tokens.contains("\"lb\""));
        assert!(tokens.contains("EnumRepr :: External"));
    }

    #[test]
    fn rejects_string_enum_variants_without_explicit_renames() {
        let input: DeriveInput = syn::parse_quote! {
            #[dto(as = "string_enum")]
            enum Unit {
                Kilogram,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("requires #[dto(rename"));
    }

    #[test]
    fn supports_transparent_one_field_structs() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(transparent)]
            struct UserId {
                value: String,
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("< String as :: dto_bindgen :: Dto >"));
        assert!(!tokens.contains("StructDef"));
    }

    #[test]
    fn rejects_invalid_transparent_structs() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(transparent)]
            struct UserId {
                value: String,
                other: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("exactly one field"));
    }

    #[test]
    fn derives_adjacent_newtype_variants() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(tag = "kind", content = "amount")]
            enum DiscountValue {
                Fixed(Decimal),
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("VariantShape :: Newtype"));
        assert!(tokens.contains("EnumRepr :: Adjacent"));
    }

    #[test]
    fn derives_mixed_adjacent_unit_and_struct_variants() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(tag = "kind", content = "payload")]
            enum Delivery {
                Pickup,
                Shipping { address: String },
            }
        };

        let tokens = expand_dto(input).expect("expand").to_string();

        assert!(tokens.contains("VariantShape :: Unit"));
        assert!(tokens.contains("VariantShape :: Struct"));
    }

    #[test]
    fn rejects_serde_with_for_large_integer_strings() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[serde(with = "my_u128_string_serde")]
                #[dto(int = "json_string")]
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
    fn rejects_unsupported_skip_serializing_if_predicates() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(skip_serializing_if = "Vec::is_empty")]
                values: Vec<String>,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("only serde(skip_serializing_if"));
    }

    #[test]
    fn rejects_option_is_none_for_non_option_fields() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(skip_serializing_if = "Option::is_none")]
                value: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("supported only for Option fields"));
    }

    #[test]
    fn rejects_split_rename_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(rename(serialize = "publicName", deserialize = "public_name"))]
                public_name: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("split serialize/deserialize rename")
        );
    }

    #[test]
    fn rejects_container_split_rename_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(rename(serialize = "PublicMetadata", deserialize = "MetadataIn"))]
            struct Metadata {
                public_name: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("split serialize/deserialize rename")
        );
    }

    #[test]
    fn rejects_alias_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(alias = "oldName")]
                public_name: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported serde field attribute `alias`")
        );
    }

    #[test]
    fn rejects_split_direction_skip_attrs() {
        let input: DeriveInput = syn::parse_quote! {
            struct Metadata {
                #[serde(skip_serializing)]
                public_name: String,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(
            err.to_string()
                .contains("split-direction serde skip attributes")
        );
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
    fn rejects_container_defaults() {
        let input: DeriveInput = syn::parse_quote! {
            #[serde(default)]
            struct Metadata {
                values: Vec<String>,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("serde container default"));
    }

    #[test]
    fn accepts_json_number_int_values() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[dto(int = "json_number")]
                sequence: u64,
            }
        };

        let tokens = expand_dto(input).expect("expand");

        assert!(tokens.to_string().contains("JsonNumber"));
    }

    #[test]
    fn accepts_legacy_json_number_int_repr_values() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[dto(int_repr = "json_number")]
                sequence: u64,
            }
        };

        let tokens = expand_dto(input).expect("expand");

        assert!(tokens.to_string().contains("JsonNumber"));
    }

    #[test]
    fn rejects_legacy_non_json_int_repr_values() {
        let input: DeriveInput = syn::parse_quote! {
            struct LedgerEntry {
                #[dto(int_repr = "non_json_bigint")]
                sequence: u64,
            }
        };

        let err = expand_dto(input).unwrap_err();

        assert!(err.to_string().contains("unsupported dto int_repr"));
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
