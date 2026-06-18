#![forbid(unsafe_code)]

use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, Type, parse_macro_input};

#[proc_macro_derive(Dto, attributes(dto, serde))]
pub fn derive_dto(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_dto(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_dto(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    reject_attrs(&input.attrs)?;

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            input.generics,
            "`Dto` derive does not support generic DTOs yet",
        ));
    }

    let ident = input.ident;
    match input.data {
        Data::Struct(data) => expand_struct(ident, data.fields),
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

fn expand_struct(ident: Ident, fields: Fields) -> syn::Result<proc_macro2::TokenStream> {
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
        reject_attrs(&field.attrs)?;

        let Some(field_ident) = field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "`Dto` derive requires named fields",
            ));
        };
        let field_var = format_ident!("__dto_bindgen_field_ty_{index}");
        let rust_name = clean_ident(&field_ident);
        let wire_name = rust_name.clone();
        let ty = field.ty;

        field_tokens.push(field_descriptor_tokens(field_var, ty, rust_name, wire_name));
    }

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
                        stringify!(#ident),
                        __dto_bindgen_source.clone(),
                    );

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

fn reject_attrs(attrs: &[Attribute]) -> syn::Result<()> {
    for attr in attrs {
        if attr.path().is_ident("serde") || attr.path().is_ident("dto") {
            return Err(syn::Error::new_spanned(
                attr,
                "`Dto` derive does not support dto/serde attributes in this slice yet",
            ));
        }
    }
    Ok(())
}

fn clean_ident(ident: &Ident) -> String {
    let raw = ident.to_string();
    raw.strip_prefix("r#").unwrap_or(&raw).to_owned()
}
