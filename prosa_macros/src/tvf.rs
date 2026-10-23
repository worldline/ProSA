/// Structures to properly identify the TVF fields to generate
pub(crate) mod expr;

/// Consume tvf! macro tokens and generate corresponding tree structure of expressions
pub(crate) mod parser;

/// Convert the expressions into tokens to be passed to the compiler
pub(crate) mod tokens;

use crate::tvf::{parser::TvfParser, tokens::buffer_to_tokens};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::quote;
use syn::spanned::Spanned;

pub(crate) fn gen_tvf_impl(input: TokenStream) -> Result<TokenStream, syn::Error> {
    // Process the token tree
    let span = input.span();
    let mut tokens = input.into_iter();

    // The first token must be the buffer type used
    let buffer_type = if let Some(TokenTree::Ident(buffer_type)) = tokens.next() {
        buffer_type
    } else {
        return Err(syn::Error::new(
            span,
            "First argument must be a type identifier",
        ));
    };

    // The second token must be the content enclosed in {} or []
    let output = if let Some(TokenTree::Group(group)) = tokens.next() {
        match group.delimiter() {
            Delimiter::Brace => {
                let mut parser = TvfParser::new(group.stream(), true);
                let exprs = parser.collect_expr()?;
                buffer_to_tokens(&buffer_type, &exprs)?
            }
            Delimiter::Bracket => {
                let mut parser = TvfParser::new(group.stream(), false);
                let exprs = parser.collect_expr()?;
                buffer_to_tokens(&buffer_type, &exprs)?
            }
            _ => {
                return Err(syn::Error::new(
                    span,
                    "Invalid delimiter, expected {} or []",
                ));
            }
        }
    } else {
        return Err(syn::Error::new(
            span,
            "Second argument must be the content enclosed in {} or []",
        ));
    };

    // Raise an error on any extra tokens
    if let Some(token) = tokens.next() {
        return Err(syn::Error::new_spanned(token, "Unexpected token"));
    }

    Ok(quote![
        {
            use prosa_utils::msg::tvf as __tvf;
            use prosa_utils::msg::bytes as __bytes;
            use prosa_utils::msg::chrono as __chrono;
            #output
        }
    ])
}

/// Negating a variable cast to an unsigned type must be rejected, like `-7 as Unsigned` is
///
/// ```compile_fail
/// use prosa_macros::tvf;
/// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
///
/// let amount: i64 = 7;
/// let _ = tvf!(SimpleStringTvf { 1 => -amount as Unsigned });
/// ```
#[cfg(doctest)]
struct NegatedVariableAsUnsigned;

#[cfg(test)]
mod tests {
    use super::gen_tvf_impl;
    use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
    use quote::quote;

    /// Expand `tvf!(SimpleStringTvf { 1 => <value> })` for each value
    /// and return the ones whose expansion success differs from `expect_ok`
    fn mismatches(expect_ok: bool, values: impl IntoIterator<Item = TokenStream>) -> Vec<String> {
        values
            .into_iter()
            .filter(|value| {
                gen_tvf_impl(quote!(SimpleStringTvf { 1 => #value })).is_ok() != expect_ok
            })
            .map(|value| value.to_string())
            .collect()
    }

    #[test]
    fn negated_byte_overflow_as_string() {
        // Panics with overflow checks, wraps to "128" without
        let tokens = gen_tvf_impl(quote!(SimpleStringTvf { 1 => -b'\x80' as String }))
            .expect("should expand");
        assert!(
            tokens.to_string().contains("\"-128\""),
            "should store \"-128\" like `-b'\\x80' as String`, got: {tokens}"
        );
    }

    #[test]
    fn numeric_modifier_on_non_numeric_type() {
        let accepted = mismatches(
            false,
            [
                quote!(-"1995-01-10" as Date),
                quote!(-0x0102 as Bytes),
                quote!(-format!("{}", 1)),
                quote!(-[1, 2]),
                quote!(![1, 2]),
                quote!(-{ 1 => 2 }),
            ],
        );
        assert!(accepted.is_empty(), "should be rejected: {accepted:?}");
    }

    #[test]
    fn explicit_type_on_sub_buffer() {
        let accepted = mismatches(
            false,
            [quote!({ 1 => 2 } as String), quote!([1, 2] as Unsigned)],
        );
        assert!(accepted.is_empty(), "should be rejected: {accepted:?}");
    }

    #[test]
    fn signed_min_literal() {
        let rejected = mismatches(
            true,
            [
                quote!(-9223372036854775808),
                quote!(-9223372036854775808 as Signed),
                quote!(-9223372036854775808 as String),
            ],
        );
        assert!(rejected.is_empty(), "should be accepted: {rejected:?}");
    }

    #[test]
    fn literal_in_none_delimited_group() {
        // `macro_rules!` wraps `$v:literal` fragments in a `Delimiter::None` group
        let none_group = |value: TokenStream| TokenTree::Group(Group::new(Delimiter::None, value));

        let expr1 = none_group(quote!(-7));
        let expr2 = none_group(quote!("s"));
        let rejected = mismatches(
            true,
            [quote!( #expr1 as Signed ), quote!( #expr2 as String )],
        );
        assert!(rejected.is_empty(), "should be accepted: {rejected:?}");
    }

    #[test]
    fn subnormal_float_literal() {
        let rejected = mismatches(true, [quote!(1e-310), quote!(1e-310 as Float)]);
        assert!(rejected.is_empty(), "should be accepted: {rejected:?}");
    }

    #[test]
    fn missing_fat_arrow_error() {
        let err = gen_tvf_impl(quote!(SimpleStringTvf { 1 2 }))
            .expect_err("a map entry without `=>` should be rejected");
        assert!(
            err.to_string().contains("=>"),
            "error should mention the missing `=>`, got: {err}"
        );
    }
}
