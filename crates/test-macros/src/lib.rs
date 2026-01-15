//! Test attribute to run async integration tests on the shared runtime.
//!
//! Apply `#[shared_runtime_test]` to an async test function. It will expand to
//! a synchronous `#[test]` that executes the body on `crate::common::test_db::run_test`.
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, ItemFn, Meta};

/// Marks an async function as a test that runs on the shared database runtime.
#[proc_macro_attribute]
pub fn shared_runtime_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    parse_macro_input!(attr as syn::parse::Nothing);

    let input_fn = parse_macro_input!(item as ItemFn);

    if input_fn.sig.asyncness.is_none() {
        return syn::Error::new(
            input_fn.sig.span(),
            "shared_runtime_test can only be applied to async functions",
        )
        .to_compile_error()
        .into();
    }

    if !input_fn.sig.inputs.is_empty() {
        return syn::Error::new(
            input_fn.sig.inputs.span(),
            "shared_runtime_test functions cannot accept arguments",
        )
        .to_compile_error()
        .into();
    }

    if !input_fn.sig.generics.params.is_empty() {
        return syn::Error::new(
            input_fn.sig.generics.span(),
            "shared_runtime_test does not support generic parameters",
        )
        .to_compile_error()
        .into();
    }

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input_fn;

    let name = sig.ident;
    let output = sig.output;

    let filtered_attrs = attrs.into_iter().filter(
        |attr| !matches!(attr.meta, Meta::Path(ref path) if path.is_ident("shared_runtime_test")),
    );

    TokenStream::from(quote! {
        #(#filtered_attrs)*
        #[test]
        #vis fn #name() #output {
            crate::common::test_db::run_test(async #block)
        }
    })
}
