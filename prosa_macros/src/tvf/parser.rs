use crate::tvf::expr::*;
use chrono::{NaiveDate, NaiveDateTime};
use num_traits::FromPrimitive;
use proc_macro2::{
    Delimiter, Literal, Punct, Spacing, Span, TokenStream, TokenTree, token_stream::IntoIter,
};
use std::{fmt::Display, iter::Peekable, str::FromStr};
use syn::{Ident, parse_quote, spanned::Spanned};

/// Store context to consume tokens and produce expressions
pub(crate) struct TvfParser {
    /// Tokens to iterate over
    pub tokens: Peekable<IntoIter>,

    /// Whole span of the block of tokens
    pub whole_span: Span,

    /// Span of the last successfull token parsing
    pub last_span: Span,

    /// Are we iterating over a list of a map?
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

    /// Grap the next token from the stream, if none found,
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
        let id = parser.next(|| "Expected field identifier, none found.")?;
        let id = TvfId::from_tokens(&id)?;

        // We expect a fat-arrow "=>" separator between identifiers and values
        // Fat-arrow is composed of two punctuation tokens: '=' and '>'
        let sep1 = parser.next(|| "Expected fat-arrow, none found.")?;
        let sep2 = parser.next(|| "Expected fat-arrow, none found.")?;
        let span = sep1.span();
        if let TokenTree::Punct(sep1) = sep1
            && sep1.as_char() == '='
            && sep1.spacing() == Spacing::Joint
            && let TokenTree::Punct(sep2) = sep2
            && sep2.as_char() == '>'
        {
            // successfully identified "=>"
            /* do nothing */
        } else {
            return Err(syn::Error::new(span, "Expected fat-arrow"));
        }

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
    /// - `0x1A2B3C` as Bytes`
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

        // Next we necessarely expect the value
        let value = parser.next(|| "Expected a value, none found.")?;
        let value = TvfValue::from_tokens(&value)?;

        // Next we might have a `as` keyword to indicate the expected type
        let explicit_type = if let Some(TokenTree::Ident(word)) = parser.peek()
            && word == "as"
        {
            // move past the "as" and look at the next token
            parser.consume();
            let cast_to = parser.next(|| "Expected type, none found.")?;

            // Following keyword must be a type name
            if let TokenTree::Ident(type_name) = cast_to {
                Some(TvfType::from_as_cast(type_name)?)
            } else {
                return Err(syn::Error::new_spanned(cast_to, "Expected type."));
            }
        } else {
            // No explicity type is specified,
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
    /// Identify the value form a token-tree
    fn from_tokens(tt: &TokenTree) -> Result<Self, syn::Error> {
        match tt {
            TokenTree::Ident(ident) => Ok(Self::Ident(ident.clone())),
            TokenTree::Literal(literal) => {
                let lit = convert_lit(literal)?;
                let int = parse_int(&lit)?;
                Ok(Self::Int(int))
            }
            TokenTree::Group(group) => {
                if group.delimiter() == Delimiter::Parenthesis {
                    Ok(Self::Expr(group.stream()))
                } else {
                    Err(syn::Error::new_spanned(tt, "Non-supported delimiters"))
                }
            }
            TokenTree::Punct(punct) => Err(syn::Error::new_spanned(
                tt,
                format!["Punctuation '{}' cannot be used as value", punct],
            )),
        }
    }
}

impl TvfValue {
    /// Identify the value form a token-tree
    fn from_tokens(tt: &TokenTree) -> Result<Self, syn::Error> {
        let span = tt.span();
        match tt {
            TokenTree::Ident(ident) => Ok(Self::Ident(ident.clone())),
            TokenTree::Literal(literal) => Ok(Self::Lit(convert_lit(literal)?)),
            TokenTree::Group(group) => match group.delimiter() {
                Delimiter::Parenthesis => Ok(Self::Expr(group.stream())),
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
                Delimiter::None => Err(syn::Error::new(span, "Missing expression delimiters")),
            },
            TokenTree::Punct(punct) => Err(syn::Error::new(
                span,
                format!["Punctuation '{}' cannot be used as value", punct],
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

impl<'e> Value<'e> {
    /// Given a literal, identify the corresponding TVF value and type
    pub(crate) fn from_literal(literal: &syn::Lit) -> Result<Self, syn::Error> {
        match literal {
            syn::Lit::Bool(lit) => Ok(Self::Bool(lit.value)),
            syn::Lit::Byte(lit) => Ok(Self::Byte(lit.value())),
            syn::Lit::Char(lit) => Ok(Self::Byte(lit.value() as u8)),
            syn::Lit::Int(int) => {
                let suffix = int.suffix();
                if suffix == "u8" {
                    Ok(Self::Byte(int.base10_parse()?))
                } else if suffix.starts_with('u') {
                    Ok(Self::Unsigned(int.base10_parse()?))
                } else {
                    Ok(Self::Signed(int.base10_parse()?))
                }
            }
            syn::Lit::Float(float) => Ok(Self::Float(float.base10_parse()?)),
            syn::Lit::Str(string) => Ok(Self::String(string.value())),
            syn::Lit::CStr(string) => {
                Ok(Self::String(string.value().to_string_lossy().to_string()))
            }
            syn::Lit::ByteStr(bytes) => Ok(Self::Bytes(Bytes(bytes.value()))),
            _ => Err(syn::Error::new_spanned(literal, "Invalid literal")),
        }
    }

    /// Given a literal and an explicity type, identify the corresponding TVF value
    pub(crate) fn from_literal_with_type(
        literal: &syn::Lit,
        explicit: TvfType,
        modifier: Modifier,
    ) -> Result<Self, syn::Error> {
        match explicit {
            TvfType::Byte => Ok(Self::Byte(parse_int(literal)?)),
            TvfType::Signed => Ok(Self::Signed(parse_int(literal)?)),
            TvfType::Unsigned => Ok(Self::Unsigned(parse_int(literal)?)),
            TvfType::Float => Ok(Self::Float(parse_float(literal)?)),
            TvfType::String => Ok(Self::String(parse_string(literal, modifier)?)),
            TvfType::Bytes => Ok(Self::Bytes(Bytes::from_literal(literal)?)),
            TvfType::Date => Ok(Self::Date(parse_date(literal)?)),
            TvfType::DateTime => Ok(Self::DateTime(parse_datetime(literal)?)),
            TvfType::Buffer => Err(syn::Error::new_spanned(
                literal,
                "Cannot build sub-buffer from literal",
            )),
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

/// Get an integer value from a literal
pub(crate) fn parse_int<I>(literal: &syn::Lit) -> Result<I, syn::Error>
where
    I: FromPrimitive + FromStr,
    <I as FromStr>::Err: Display,
{
    let span = literal.span();
    match literal {
        syn::Lit::Byte(byte) => I::from_u8(byte.value())
            .ok_or_else(|| syn::Error::new(span, "Could not deduce integer from byte")),
        syn::Lit::Char(chr) => I::from_u32(chr.value() as u32)
            .ok_or_else(|| syn::Error::new(span, "Could not deduce integer from character")),
        syn::Lit::Int(int) => Ok(int.base10_parse()?),
        _ => Err(syn::Error::new(span, "Invalid literal")),
    }
}

/// Get a float value from a literal
pub(crate) fn parse_float(literal: &syn::Lit) -> Result<f64, syn::Error> {
    let span = literal.span();
    match literal {
        syn::Lit::Byte(byte) => Ok(byte.value() as f64),
        syn::Lit::Char(chr) => Ok(chr.value() as u32 as f64),
        syn::Lit::Int(int) => Ok(int.base10_parse()?),
        syn::Lit::Float(float) => Ok(float.base10_parse()?),
        _ => Err(syn::Error::new(span, "Invalid literal")),
    }
}

/// Given a literal deduce a String
/// We pass an extra modifier argument for primitives before being converted into string literals
pub(crate) fn parse_string(literal: &syn::Lit, modifier: Modifier) -> Result<String, syn::Error> {
    let span = literal.span();

    // Try to convert the literal into a string
    let string = match literal {
        syn::Lit::Str(s) => s.value(),
        syn::Lit::ByteStr(s) => match String::from_utf8(s.value()) {
            Ok(s) => s,
            Err(err) => {
                return Err(syn::Error::new(
                    span,
                    format!["Failed to parse UTF-8 string: {}", err],
                ));
            }
        },
        syn::Lit::CStr(s) => s.value().to_string_lossy().to_string(),
        syn::Lit::Bool(s) => {
            let value = match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => s.value,
                Modifier::LogicalNot | Modifier::Negative => !s.value,
                _ => return Err(syn::Error::new(span, "Unsupported operator for boolean")),
            };
            if value { "true" } else { "false" }.to_string()
        }
        syn::Lit::Byte(s) => {
            let value = s.value();
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => value,
                Modifier::LogicalNot => !value,
                Modifier::Negative => -(value as i8) as u8,
                _ => {
                    return Err(syn::Error::new(span, "Unsupported operatory for byte"));
                }
            }
            .to_string()
        }
        syn::Lit::Char(s) => {
            let value = s.value();
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => value,
                _ => {
                    return Err(syn::Error::new(span, "Unsupported operatory for character"));
                }
            }
            .to_string()
        }
        syn::Lit::Int(s) => match modifier {
            Modifier::None | Modifier::Positive | Modifier::Dereference => s.to_string(),
            Modifier::LogicalNot => (!s.base10_parse::<u64>()?).to_string(),
            Modifier::Negative => (-s.base10_parse::<i64>()?).to_string(),
            _ => {
                return Err(syn::Error::new(span, "Unsupported operatory for integer"));
            }
        },
        syn::Lit::Float(s) => match modifier {
            Modifier::None | Modifier::Positive | Modifier::Dereference => s.to_string(),
            Modifier::Negative => (-s.base10_parse::<f64>()?).to_string(),
            _ => {
                return Err(syn::Error::new(span, "Unsupported operatory for integer"));
            }
        },
        _ => {
            return Err(syn::Error::new(span, "Unsupported literal"));
        }
    };
    Ok(string)
}

/// Given a literal deduce a Date
pub(crate) fn parse_date(literal: &syn::Lit) -> Result<NaiveDate, syn::Error> {
    const FORMAT: &str = "%Y-%m-%d";
    let span = literal.span();

    let string = parse_str(literal)?;
    NaiveDate::parse_from_str(&string, FORMAT)
        .map_err(|err| syn::Error::new(span, format!["Failed to parse date: {}", err]))
}

/// Given a literal deduce a DateTime
pub(crate) fn parse_datetime(literal: &syn::Lit) -> Result<NaiveDateTime, syn::Error> {
    const FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";
    let span = literal.span();

    let string = parse_str(literal)?;
    NaiveDateTime::parse_from_str(&string, FORMAT)
        .map_err(|err| syn::Error::new(span, format!["Failed to parse date: {}", err]))
}

/// Given a literal deduce a String
fn parse_str(literal: &syn::Lit) -> Result<String, syn::Error> {
    let span = literal.span();

    // Try to convert the literal into a string
    let string = match literal {
        syn::Lit::Str(s) => s.value(),
        syn::Lit::ByteStr(s) => match String::from_utf8(s.value()) {
            Ok(s) => s,
            Err(err) => {
                return Err(syn::Error::new(
                    span,
                    format!["Failed to parse UTF-8 string: {}", err],
                ));
            }
        },
        syn::Lit::CStr(s) => s.value().to_string_lossy().to_string(),
        _ => {
            return Err(syn::Error::new(span, "Unsupported literal"));
        }
    };
    Ok(string)
}

impl Bytes {
    /// Parse a integer literal an build a sequence of bytes from it
    pub(crate) fn from_literal(literal: &syn::Lit) -> Result<Self, syn::Error> {
        let span = literal.span();

        match literal {
            syn::Lit::Str(lit) => {
                // convert the string into bytes
                todo!()
            }
            syn::Lit::ByteStr(lit) => {
                // convert the string into bytes
                todo!()
            }
            syn::Lit::CStr(lit) => {
                // convert the string into bytes
                todo!()
            }
            syn::Lit::Byte(lit) => todo!(),
            syn::Lit::Char(lit) => todo!(),
            syn::Lit::Int(lit) => {
                // Remove underscores from the literal
                let digits = lit.to_string().replace('_', "");

                // convert the digits to a sequence of bytes
                if digits.starts_with("0x") {
                    // hexadecimal string
                    Self::digits_to_bytes(literal.span(), &digits, 16, 2)
                } else if digits.starts_with("0b") {
                    // binary string
                    Self::digits_to_bytes(literal.span(), &digits, 2, 8)
                } else {
                    Err(syn::Error::new_spanned(
                        literal,
                        "Cannot convert number to bytes, only hexadecimal and binary literals are supported.",
                    ))
                }
            }
            _ => Err(syn::Error::new(
                span,
                "Unsupported literal for sequence of bytes",
            )),
        }
    }

    /// Convert a string of digits to bytes
    fn digits_to_bytes(
        span: Span,
        digits: &str,
        radix: u32,
        group_by: usize,
    ) -> Result<Self, syn::Error> {
        // remove the prefix and the underscores
        let trimmed = digits.replace('_', "").split_off(2);

        // compose a sequence of bytes
        let mut result = Vec::<u8>::with_capacity(trimmed.len() / group_by);

        // group the digits in chunks,
        // start from the end to avoid padding
        trimmed
            .chars()
            .collect::<Vec<_>>()
            .rchunks(group_by)
            .try_for_each(|chunk| {
                let byte = chunk.iter().collect::<String>();
                match u8::from_str_radix(&byte, radix) {
                    Ok(byte) => {
                        result.push(byte);
                        Ok(())
                    }
                    Err(e) => Err(syn::Error::new(
                        span,
                        format!["Failed to parse integer literal: {}", e],
                    )),
                }
            })?;

        // return the result in the correct order
        result.reverse();
        Ok(Self(result))
    }
}
