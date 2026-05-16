//! Token definitions for the Vita lexer.

use std::fmt;

/// A single token with its kind, text, and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, text: &str, line: usize, col: usize) -> Self {
        Token {
            kind,
            text: text.to_string(),
            line,
            col,
        }
    }

    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// All token kinds in the Vita language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // --- Literals ---
    IntLiteral,
    FloatLiteral,
    StringLiteral,
    CharLiteral,
    BoolLiteral, // true, false

    // --- Identifiers & Keywords ---
    Ident,
    // Keywords
    KwDef,
    KwEnum,
    KwSpec,
    KwImpl,
    KwFn,
    KwLet,
    KwConst,
    KwPub,
    KwUse,
    KwAs,
    KwSelf,
    KwTrue,
    KwFalse,
    KwBreak,
    KwContinue,
    KwReturn,

    // --- Symbols for control flow ---
    Question,       // ?   if
    BangQuestion,   // !?  else-if
    Bang,           // !   else
    Dollar,         // $   match
    Star,           // *   loop / while
    StarQuestion,   // *?  for-each
    DoubleQuestion, // ??  fallible
    BangDollar,     // !$  catch-match
    DoubleBang,     // !!  catch

    // --- Delimiters ---
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]

    // --- Punctuation ---
    Comma,       // ,
    Semicolon,   // ;
    Colon,       // :
    Dot,         // .
    DotDot,      // ..
    Arrow,       // ->
    FatArrow,    // =>
    DoubleColon, // ::
    Underscore,  // _
    Hash,        // #

    // --- Operators ---
    Plus,      // +
    Minus,     // -
    StarMul,   // * (when used as multiply)
    Slash,     // /
    Percent,   // %
    Amp,       // &
    Pipe,      // |
    Caret,     // ^
    LtLt,      // <<
    GtGt,      // >>
    Eq,        // =
    EqEq,      // ==
    BangEq,    // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    AmpAmp,    // &&
    PipePipe,  // ||
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=
    PercentEq, // %=

    // --- Special ---
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            TokenKind::IntLiteral => "integer literal",
            TokenKind::FloatLiteral => "float literal",
            TokenKind::StringLiteral => "string literal",
            TokenKind::CharLiteral => "character literal",
            TokenKind::BoolLiteral => "boolean literal",
            TokenKind::Ident => "identifier",
            TokenKind::KwDef => "def",
            TokenKind::KwEnum => "enum",
            TokenKind::KwSpec => "spec",
            TokenKind::KwImpl => "impl",
            TokenKind::KwFn => "fn",
            TokenKind::KwLet => "let",
            TokenKind::KwConst => "const",
            TokenKind::KwPub => "pub",
            TokenKind::KwUse => "use",
            TokenKind::KwAs => "as",
            TokenKind::KwSelf => "self",
            TokenKind::KwTrue => "true",
            TokenKind::KwFalse => "false",
            TokenKind::KwBreak => "break",
            TokenKind::KwContinue => "continue",
            TokenKind::KwReturn => "return",
            TokenKind::Question => "?",
            TokenKind::BangQuestion => "!?",
            TokenKind::Bang => "!",
            TokenKind::Dollar => "$",
            TokenKind::Star => "*",
            TokenKind::StarQuestion => "*?",
            TokenKind::DoubleQuestion => "??",
            TokenKind::BangDollar => "!$",
            TokenKind::DoubleBang => "!!",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Comma => ",",
            TokenKind::Semicolon => ";",
            TokenKind::Colon => ":",
            TokenKind::Dot => ".",
            TokenKind::DotDot => "..",
            TokenKind::Arrow => "->",
            TokenKind::FatArrow => "=>",
            TokenKind::DoubleColon => "::",
            TokenKind::Underscore => "_",
            TokenKind::Hash => "#",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::StarMul => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Amp => "&",
            TokenKind::Pipe => "|",
            TokenKind::Caret => "^",
            TokenKind::LtLt => "<<",
            TokenKind::GtGt => ">>",
            TokenKind::Eq => "=",
            TokenKind::EqEq => "==",
            TokenKind::BangEq => "!=",
            TokenKind::Lt => "<",
            TokenKind::Gt => ">",
            TokenKind::LtEq => "<=",
            TokenKind::GtEq => ">=",
            TokenKind::AmpAmp => "&&",
            TokenKind::PipePipe => "||",
            TokenKind::PlusEq => "+=",
            TokenKind::MinusEq => "-=",
            TokenKind::StarEq => "*=",
            TokenKind::SlashEq => "/=",
            TokenKind::PercentEq => "%=",
            TokenKind::Eof => "end of file",
        };
        write!(f, "{}", s)
    }
}

/// Map keyword strings to their token kinds.
pub fn lookup_keyword(ident: &str) -> Option<TokenKind> {
    match ident {
        "def" => Some(TokenKind::KwDef),
        "enum" => Some(TokenKind::KwEnum),
        "spec" => Some(TokenKind::KwSpec),
        "impl" => Some(TokenKind::KwImpl),
        "fn" => Some(TokenKind::KwFn),
        "let" => Some(TokenKind::KwLet),
        "const" => Some(TokenKind::KwConst),
        "pub" => Some(TokenKind::KwPub),
        "use" => Some(TokenKind::KwUse),
        "as" => Some(TokenKind::KwAs),
        "self" => Some(TokenKind::KwSelf),
        "true" => Some(TokenKind::KwTrue),
        "false" => Some(TokenKind::KwFalse),
        "break" => Some(TokenKind::KwBreak),
        "continue" => Some(TokenKind::KwContinue),
        "return" => Some(TokenKind::KwReturn),
        _ => None,
    }
}
