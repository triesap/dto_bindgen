#![forbid(unsafe_code)]

use proc_macro::TokenStream;

#[proc_macro_derive(Dto, attributes(dto, serde))]
pub fn derive_dto(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
