use crate::tvf::expr::*;
use core::{fmt::Display, iter::Peekable};
use proc_macro2::{
    Delimiter, Literal, Punct, Spacing, Span, TokenStream, TokenTree, token_stream::IntoIter,
};
use quote::TokenStreamExt;
use syn::{Ident, parse_quote, spanned::Spanned};

/// Store context to consume tokens and produce expressions
pub(crate) struct TvfParser {
    /// Tokens to iterate over
    pub tokens: Peekable<IntoIter>,

    /// Whole span of the block of tokens
    pub whole_span: Span,

    /// Span of the last successful token parsing
    pub last_span: Span,

    /// Are we iterating over a list or a map?
    pub is_map: bool,

    /// Number of fields that have been parsed until now
    pub field_count: usize,
}

impl TvfParser {
    /// Wrap the iterable tokenstream into the parser
    #[inline]
    pub(crate) fn new(tokens: TokenStream, is_map: bool) -> Self {
        let whole_span = tokens.span();
        Self {
            tokens: tokens.into_iter().peekable(),
            whole_span,
            last_span: whole_span,
            is_map,
            field_count: 0,
        }
    }

    /// Grab the next token from the stream, if none found,
    /// return an error using the last successful span.
    pub(crate) fn next<Err, Msg>(&mut self, on_error: Err) -> Result<TokenTree, syn::Error>
    where
        Err: FnOnce() -> Msg,
        Msg: Display,
    {
        if let Some(tt) = self.tokens.next() {
            self.last_span = tt.span();
            Ok(tt)
        } else {
            Err(syn::Error::new(self.last_span, on_error()))
        }
    }

    /// Peek at the next token in the stream
    /// Use `consume` afterward to actually advance the iterator
    #[inline]
    pub(crate) fn peek(&mut self) -> Option<&TokenTree> {
        self.tokens.peek()
    }

    /// After peeking at the next token in the stream,
    /// actually move to the next token.
    /// Return true if there was a token to consume or false otherwise.
    #[inline]
    pub(crate) fn consume(&mut self) -> bool {
        if let Some(tt) = self.tokens.next() {
            self.last_span = tt.span();
            true
        } else {
            false
        }
    }
}

impl TvfParser {
    /// Iterate over the tokens to collect expressions
    pub(crate) fn collect_expr(&mut self) -> Result<Vec<TvfExpr>, syn::Error> {
        // We don't really know in advance how many fields we will build
        let mut expressions = Vec::new();

        if self.is_map {
            // Read the sub-buffer as a key-value map
            while self.peek().is_some() {
                let expr = TvfExpr::parse_from_map(self)?;
                expressions.push(expr);
                self.field_count += 1;
                self.check_for_comma()?;
            }
        } else {
            // Read the sub-buffer as a list of values
            while self.peek().is_some() {
                let expr = TvfExpr::parse_from_list(self)?;
                expressions.push(expr);
                self.field_count += 1;
                self.check_for_comma()?;
            }
        }

        Ok(expressions)
    }

    /// Check for fat-arrow '=>'
    fn check_for_fat_arrow(&mut self) -> Result<Option<TokenTree>, syn::Error> {
        // Extract the first token to peek at the next one
        if let Some(tt1) = self.tokens.next() {
            // We expect a fat-arrow "=>" separator between identifiers and values
            // Fat-arrow is composed of two punctuation tokens: '=' and '>'
            if let TokenTree::Punct(p1) = &tt1
                && p1.as_char() == '='
                && p1.spacing() == Spacing::Joint
                && let Some(TokenTree::Punct(p2)) = self.peek()
                && p2.as_char() == '>'
            {
                self.consume();
                Ok(None)
            } else {
                Ok(Some(tt1))
            }
        } else {
            Err(syn::Error::new(
                Span::call_site(),
                "Missing fat-arrow '=>' between field identifier and field value.",
            ))
        }
    }

    /// Check if the next token is a comma ','
    /// If so, move on to the next token to read the next expression
    fn check_for_comma(&mut self) -> Result<(), syn::Error> {
        if let Some(tt) = self.peek() {
            if let TokenTree::Punct(comma) = tt
                && comma.as_char() == ','
            {
                // consume the comma
                self.consume();
                Ok(())
            } else {
                Err(syn::Error::new(
                    self.last_span,
                    "Next token is not a comma ','",
                ))
            }
        } else {
            // No more token is fine too
            Ok(())
        }
    }
}

impl TvfExpr {
    /// Parse an expression from a list buffer
    /// `<value>`
    fn parse_from_list(parser: &mut TvfParser) -> Result<Self, syn::Error> {
        let id = TvfId::Int(parser.field_count + 1);
        Self::parse_value(parser, id)
    }

    /// Parse an expression from a map buffer
    /// `<id> => <value>`
    fn parse_from_map(parser: &mut TvfParser) -> Result<Self, syn::Error> {
        let id = TvfId::parse_id(parser)?;

        // Parse the remaining tokens as the value
        Self::parse_value(parser, id)
    }

    /// Parse a field expression value
    /// Samples of expected token sequences:
    /// - `10u64`
    /// - `-10 as Signed`
    /// - `!false`
    /// - `MY_CONST as String`
    /// - `(10 - 2) as Unsigned`
    /// - `"2026-09-10" as Date`
    /// - `0x1A2B3C as Bytes`
    /// - `{ 1 => 10 }` sub-buffer expressed as map
    /// - `[ 1, 2, 3 ]` sub-buffer expressed as list
    fn parse_value(parser: &mut TvfParser, id: TvfId) -> Result<Self, syn::Error> {
        // First token might be a modifier (unary operator)
        let modifier = if let Some(TokenTree::Punct(punct)) = parser.peek() {
            let modifier = Modifier::from_punct(punct)?;
            parser.consume(); // move to next token
            modifier
        } else {
            Modifier::None
        };

        // Next we necessarily expect the value
        let value = parser.next(|| "Expected a value, none found.")?;

        // Check if the next token is `format`
        let value = if let TokenTree::Ident(ident) = &value
            && ident == "format"
        {
            // Check if the next token is `!`
            if let Some(TokenTree::Punct(punct)) = parser.peek()
                && punct.as_char() == '!'
            {
                parser.consume();

                // Check if we have a group
                let group = parser.next(|| "Expected (), [] or {}, got none")?;
                if let TokenTree::Group(group) = group {
                    TvfValue::Format(group.stream())
                } else {
                    return Err(syn::Error::new_spanned(
                        group,
                        "Expected (), [] or {}, got something else",
                    ));
                }
            } else {
                // `format` is not followed by `!` treat it as a regular identifier
                TvfValue::Ident(ident.clone())
            }
        } else {
            TvfValue::from_tokens(&value)?
        };

        // Next we might have a `as` keyword to indicate the expected type
        let explicit_type = if let Some(TokenTree::Ident(word)) = parser.peek()
            && word == "as"
        {
            // move past the "as" and look at the next token
            parser.consume();
            let cast_to = parser.next(|| "Expected type, none found.")?;
            let cast_span = cast_to.span();

            // Following keyword must be a type name
            let explicit_type = if let TokenTree::Ident(type_name) = cast_to {
                TvfType::from_as_cast(type_name)?
            } else {
                return Err(syn::Error::new(cast_span, "Expected type."));
            };
            Some(explicit_type)
        } else {
            // No explicitly type is specified,
            None
        };

        // Complete the expression
        Ok(Self {
            id,
            modifier,
            value,
            explicit_type,
        })
    }
}

impl TvfId {
    /// Identify the field identifier from a token-tree
    fn parse_id(parser: &mut TvfParser) -> Result<Self, syn::Error> {
        let mut tokens = TokenStream::new();

        // Pull tokens out of the parser until we find a fat-arrow '=>'
        while let Some(tt) = parser.check_for_fat_arrow()? {
            tokens.append(tt);
        }
        Ok(Self::Expr(tokens))
    }
}

impl TvfValue {
    /// Identify the value from a token-tree
    fn from_tokens(tt: &TokenTree) -> Result<Self, syn::Error> {
        let span = tt.span();
        match tt {
            TokenTree::Ident(ident) => Ok(Self::Ident(ident.clone())),
            TokenTree::Literal(literal) => Ok(Self::Lit(convert_lit(literal)?)),
            TokenTree::Group(group) => match group.delimiter() {
                Delimiter::Parenthesis | Delimiter::None => Ok(Self::Expr(group.stream())),
                Delimiter::Bracket => {
                    let mut parser = TvfParser::new(group.stream(), false);
                    let exprs = parser.collect_expr()?;
                    Ok(Self::Buffer(exprs, parser.whole_span))
                }
                Delimiter::Brace => {
                    let mut parser = TvfParser::new(group.stream(), true);
                    let exprs = parser.collect_expr()?;
                    Ok(Self::Buffer(exprs, parser.whole_span))
                }
            },
            TokenTree::Punct(punct) => Err(syn::Error::new(
                span,
                format!("Punctuation '{}' cannot be used as value", punct),
            )),
        }
    }
}

impl TvfType {
    /// Deduce the value type from the type provided in a `as` cast
    #[rustfmt::skip]
    fn from_as_cast(ident: Ident) -> Result<Self, syn::Error> {
        match ident.to_string().to_ascii_lowercase().as_str() {
            "byte"     | "u8"                                    => Ok(Self::Byte),
            "signed"   | "i8"  | "i16" | "i32" | "i64" | "isize" => Ok(Self::Signed),
            "unsigned" |         "u16" | "u32" | "u64" | "usize" => Ok(Self::Unsigned),
            "float"    |                 "f32" | "f64"           => Ok(Self::Float),
            "string"   | "str"                                   => Ok(Self::String),
            "bytes"                                              => Ok(Self::Bytes),
            "date"     | "naivedate"                             => Ok(Self::Date),
            "datetime" | "naivedatetime"                         => Ok(Self::DateTime),
            "buffer"   | "tvf"                                   => Ok(Self::Buffer),
            _ => Err(syn::Error::new_spanned(ident, "Invalid value type")),
        }
    }
}

impl Modifier {
    /// Identify unary operation from punctuation mark
    fn from_punct(punct: &Punct) -> Result<Self, syn::Error> {
        match punct.as_char() {
            '+' => Ok(Self::Positive),
            '-' => Ok(Self::Negative),
            '!' => Ok(Self::LogicalNot),
            '*' => Ok(Self::Dereference),
            '&' => Ok(Self::Borrow),
            _ => Err(syn::Error::new_spanned(punct, "Unsupported punctuation")),
        }
    }
}

/// Convert a `proc_macro2::Literal` into a `syn::Lit`
fn convert_lit(literal: &Literal) -> Result<syn::Lit, syn::Error> {
    if let syn::Expr::Lit(literal) = parse_quote! [ #literal ] {
        Ok(literal.lit)
    } else {
        Err(syn::Error::new_spanned(literal, "Invalid literal"))
    }
}
