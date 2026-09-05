use super::*;

fn assert_error(expand: fn(&DeriveInput) -> Result<TokenStream2>, source: &str, expected: &str) {
    let input = syn::parse_str(source).unwrap();
    let error = expand(&input).unwrap_err().to_string();
    assert!(
        error.contains(expected),
        "{error:?} should contain {expected:?}"
    );
}

#[test]
fn record_rejects_invalid_definitions() {
    for (source, expected) in [
        ("enum Record { A }", "named fields"),
        ("struct Record(u32);", "named fields"),
        ("struct Record { value: u32 }", "tag and decode"),
        (
            r#"struct Record { #[tes(tag = b"NAME")] value: u32 }"#,
            "expected tes decode",
        ),
        (
            r#"struct Record { #[tes(tag = b"BAD", decode = read)] value: u32 }"#,
            "exactly four bytes",
        ),
        (
            r#"struct Record {
                #[tes(tag = b"NAME", decode = read)] a: u32,
                #[tes(tag = b"NAME", decode = read)] b: u32,
            }"#,
            "duplicate subrecord tag",
        ),
        (
            r#"struct Record {
                #[tes(tag = b"NAME", decode = read, skip)] value: u32,
            }"#,
            "skip cannot be combined",
        ),
        (
            r#"struct Record { #[tes(skip, decode = read)] value: u32 }"#,
            "skip cannot be combined",
        ),
        (
            r#"struct Record {
                #[tes(tag = b"NAME", decode = read)]
                #[tes(tag = b"FNAM")]
                value: u32,
            }"#,
            "duplicate tes option",
        ),
        (
            r#"struct Record { #[tes(skip, skip)] value: u32 }"#,
            "duplicate tes option",
        ),
        (
            r#"struct Record { #[tes(read = read)] value: u32 }"#,
            "expected one of: tag, decode, skip",
        ),
        (
            r#"#[tes(parser = read)] struct Record {}"#,
            "expected one of: unmapped",
        ),
        (
            r#"struct Record<'a, 'b> { #[tes(skip)] value: &'a &'b str }"#,
            "at most one lifetime",
        ),
    ] {
        assert_error(expand_record, source, expected);
    }
}

#[test]
fn payload_rejects_invalid_definitions() {
    for (source, expected) in [
        ("union Payload { value: u32 }", "named fields"),
        ("struct Payload { value: u32 }", "expected tes parser"),
        (
            "#[tes(parser = read)] struct Payload { value: u32 }",
            "expected tes read",
        ),
        (
            "#[tes(parser = read)] struct Payload { #[tes(skip)] value: u32 }",
            "expected one of: read",
        ),
        (
            "#[tes(parser = read, parser = again)] struct Payload {}",
            "duplicate tes option",
        ),
        (
            "#[tes(unmapped = handler)] struct Payload {}",
            "expected one of: parser",
        ),
    ] {
        assert_error(expand_payload, source, expected);
    }
}

#[test]
fn expansions_are_valid_rust_with_closures_and_generics() {
    let record = parse_quote! {
        #[tes(unmapped = Self::other)]
        struct Record<'data, T = u32> where T: Default {
            #[tes(tag = b"NAME", decode = |bytes| Some(l1(bytes)))]
            text: Option<&'data str>,
            #[tes(skip)]
            extra: T,
        }
    };
    syn::parse2::<syn::ItemImpl>(expand_record(&record).unwrap()).unwrap();

    let payload = parse_quote! {
        #[tes(parser = payload)]
        struct Payload<T = u32> where T: Default {
            #[tes(read = read_value)]
            value: T,
        }
    };
    syn::parse2::<syn::ItemFn>(expand_payload(&payload).unwrap()).unwrap();
}
