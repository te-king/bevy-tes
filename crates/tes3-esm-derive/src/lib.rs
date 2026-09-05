//! Internal code generation for `tes3-esm`, using its `crate::common::Subrecord` and
//! `nom` dependency. These derives generate ordinary parsers, not runtime schemas.
//!
//! Both derives accept named structs with at most one lifetime (the input lifetime).
//! Field types, visibility, documentation, and other derives are left untouched.

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::meta::ParseNestedMeta;
use syn::parse::Parse;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Fields, FieldsNamed, Generics, Ident, Lifetime,
    LitByteStr, Result, parse_macro_input, parse_quote,
};

/// Generate `from_subrecords`, starting from `Default` and processing tags in order.
///
/// Fields require `#[tes(tag = b"NAME", decode = expression)]` or `#[tes(skip)]`.
/// A decoder consumes the raw payload and returns the field's value; assignment is
/// unconditional, so duplicate tags use the last decoded value. Recovery policy
/// belongs in the decoder (e.g. `parse_or_default`), not in this derive.
///
/// `#[tes(unmapped = Self::handler)]` on the struct forwards unmapped subrecords to
/// `fn handler(&mut self, sub: Subrecord<'a>)`. Without it, unmapped tags are ignored.
#[proc_macro_derive(TesRecord, attributes(tes))]
pub fn tes_record(input: TokenStream) -> TokenStream {
    expand_record(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Generate a private sequential `nom` parser named by `#[tes(parser = function_name)]`.
///
/// Every field requires `#[tes(read = expression)]`. Parsers run in field declaration
/// order, propagating errors and returning the unconsumed input. No endianness,
/// optionality, or defaults are inferred from field types.
#[proc_macro_derive(TesPayload, attributes(tes))]
pub fn tes_payload(input: TokenStream) -> TokenStream {
    expand_payload(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[derive(Default)]
struct Options {
    tag: Option<LitByteStr>,
    decode: Option<Expr>,
    read: Option<Expr>,
    parser: Option<Ident>,
    unmapped: Option<Expr>,
    skip: bool,
}

fn parse_once<T: Parse>(meta: &ParseNestedMeta<'_>, slot: &mut Option<T>) -> Result<()> {
    if slot.is_some() {
        return Err(meta.error("duplicate tes option"));
    }
    *slot = Some(meta.value()?.parse()?);
    Ok(())
}

fn options(attrs: &[Attribute], allowed: &[&str]) -> Result<Options> {
    let mut out = Options::default();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("tes")) {
        attr.parse_nested_meta(|meta| {
            if !allowed.iter().any(|name| meta.path.is_ident(name)) {
                return Err(meta.error(format!("expected one of: {}", allowed.join(", "))));
            }
            if meta.path.is_ident("tag") {
                parse_once(&meta, &mut out.tag)
            } else if meta.path.is_ident("decode") {
                parse_once(&meta, &mut out.decode)
            } else if meta.path.is_ident("read") {
                parse_once(&meta, &mut out.read)
            } else if meta.path.is_ident("parser") {
                parse_once(&meta, &mut out.parser)
            } else if meta.path.is_ident("unmapped") {
                parse_once(&meta, &mut out.unmapped)
            } else {
                if out.skip {
                    return Err(meta.error("duplicate tes option"));
                }
                out.skip = true;
                Ok(())
            }
        })?;
    }
    Ok(out)
}

fn named_fields(input: &DeriveInput) -> Result<&FieldsNamed> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(fields),
            _ => Err(Error::new_spanned(
                input,
                "expected a struct with named fields",
            )),
        },
        _ => Err(Error::new_spanned(
            input,
            "expected a struct with named fields",
        )),
    }
}

fn input_lifetime(generics: &Generics) -> Result<Lifetime> {
    let mut lifetimes = generics.lifetimes();
    let first = lifetimes.next();
    if let Some(extra) = lifetimes.next() {
        return Err(Error::new_spanned(
            extra,
            "expected at most one lifetime for the parser input",
        ));
    }
    Ok(first
        .map(|param| param.lifetime.clone())
        .unwrap_or_else(|| parse_quote!('__tes_input)))
}

fn expand_record(input: &DeriveInput) -> Result<TokenStream2> {
    let fields = named_fields(input)?;
    let opts = options(&input.attrs, &["unmapped"])?;
    let lifetime = input_lifetime(&input.generics)?;
    let method_generics = input
        .generics
        .lifetimes()
        .next()
        .is_none()
        .then(|| quote!(<#lifetime>));
    let mut tags = HashSet::new();
    let mut arms = Vec::new();
    for field in &fields.named {
        let opts = options(&field.attrs, &["tag", "decode", "skip"])?;
        if opts.skip {
            if opts.tag.is_some() || opts.decode.is_some() {
                return Err(Error::new_spanned(
                    field,
                    "skip cannot be combined with tag or decode",
                ));
            }
            continue;
        }
        let tag = opts
            .tag
            .ok_or_else(|| Error::new_spanned(field, "expected tes tag and decode, or skip"))?;
        if tag.value().len() != 4 {
            return Err(Error::new_spanned(
                tag,
                "subrecord tags must contain exactly four bytes",
            ));
        }
        if !tags.insert(tag.value()) {
            return Err(Error::new_spanned(tag, "duplicate subrecord tag"));
        }
        let decode = opts
            .decode
            .ok_or_else(|| Error::new_spanned(field, "expected tes decode"))?;
        let name = &field.ident;
        arms.push(quote! {
            #tag => __tes_out.#name = (#decode)(__tes_sub.data),
        });
    }
    let unmapped = opts
        .unmapped
        .map(|handler| quote!((#handler)(&mut __tes_out, __tes_sub)));
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn from_subrecords #method_generics (
                subs: impl ::core::iter::Iterator<Item = crate::common::Subrecord<#lifetime>>,
            ) -> Self
            where
                Self: ::core::default::Default,
            {
                let mut __tes_out = Self::default();
                for __tes_sub in subs {
                    match &__tes_sub.tag.0 {
                        #(#arms)*
                        _ => { #unmapped }
                    }
                }
                __tes_out
            }
        }
    })
}

fn expand_payload(input: &DeriveInput) -> Result<TokenStream2> {
    let fields = named_fields(input)?;
    let opts = options(&input.attrs, &["parser"])?;
    let parser = opts
        .parser
        .ok_or_else(|| Error::new_spanned(input, "expected tes parser = function_name"))?;
    let lifetime = input_lifetime(&input.generics)?;
    let mut generics = input.generics.clone();
    if generics.lifetimes().next().is_none() {
        generics.params.insert(0, parse_quote!(#lifetime));
    }
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let mut reads = Vec::new();
    let mut assignments = Vec::new();
    for (index, field) in fields.named.iter().enumerate() {
        let read = options(&field.attrs, &["read"])?
            .read
            .ok_or_else(|| Error::new_spanned(field, "expected tes read"))?;
        let name = &field.ident;
        let value = format_ident!("__tes_field_{index}");
        reads.push(quote! { let (__tes_input, #value) = (#read)(__tes_input)?; });
        assignments.push(quote! { #name: #value });
    }
    let name = &input.ident;
    Ok(quote! {
        fn #parser #fn_generics (
            __tes_input: &#lifetime [u8],
        ) -> ::nom::IResult<&#lifetime [u8], #name #ty_generics>
        #where_clause
        {
            #(#reads)*
            Ok((__tes_input, #name { #(#assignments),* }))
        }
    })
}

#[cfg(test)]
mod tests;
