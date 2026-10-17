//! A deliberately small derive: generated code implements the caller's KindName trait.
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};
#[proc_macro_derive(KindName)]
pub fn kind_name(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    quote! {
        impl #impl_generics crate::KindName for #name #type_generics #where_clause {
            fn kind_name() -> &'static str { stringify!(#name) }
        }
    }
    .into()
}
