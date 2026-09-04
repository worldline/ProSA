use proc_macro2::TokenStream;

/// Define a field expression which includes the following elements
/// - a target field identifier
/// - a modifier (unary operator)
/// - a literal / variable / rust-expression
/// - a implicit or explicit target type
#[derive(Debug, Clone)]
pub(crate) struct TvfExpr {
    /// Field identifier to use
    pub id: TvfId,

    /// Modifier for the value
    pub modifier: Modifier,

    /// Value to use for the field
    pub value: TvfValue,

    /// Output type to insert the field in the buffer
    pub out_type: TvfType,
}

/// Identifier of a field
#[derive(Debug, Clone)]
pub(crate) enum TvfId {
    /// We directly have an integer value
    Int(usize),

    /// Identifier of a variable
    Ident(syn::Ident),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),
}

/// Define a value to insert into the buffer
#[derive(Debug, Clone)]
pub(crate) enum TvfValue {
    /// Simple literal which can be used to implicitely identify the type of the value
    Lit(syn::Lit),

    /// Identifier of a variable
    Ident(syn::Ident),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),

    /// Sub-buffer
    Buffer(Vec<TvfExpr>),
}

/// The types that can be added to a TVF buffer
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TvfType {
    Byte,
    Signed,
    Unsigned,
    Float,
    String,
    Bytes,
    Date,
    DateTime,
    Buffer,
}

/// Modifier for a value
/// Usually a not for boolean or a minus sign for numbers, etc..
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modifier {
    /// No operator
    #[default]
    None,

    /// + sign, results in no operator being added
    Positive,

    /// - sign, negate the following value
    Negative,

    /// logical not for boolean values
    LogicalNot,

    /// * dereference
    Dereference,

    /// & borrow
    Borrow,
}

/// Simple sequence of bytes to serialize
#[derive(Debug, Default, Clone)]
pub(crate) struct Bytes(pub Vec<u8>);
