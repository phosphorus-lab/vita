//! Error types for the Vita compiler.

use std::fmt;

/// A position in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub offset: usize,
}

impl Span {
    pub fn new(line: usize, col: usize, offset: usize) -> Self {
        Span { line, col, offset }
    }
    pub fn zero() -> Self {
        Span::new(1, 1, 0)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Compiler error kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    // Lexer
    UnexpectedChar(char),
    UnterminatedString,
    UnterminatedChar,
    InvalidNumber(String),

    // Parser
    UnexpectedToken {
        expected: String,
        got: String,
    },
    ExpectedEof(String),

    // Type checker
    UndefinedType(String),
    UndefinedName(String),
    TypeMismatch {
        expected: String,
        got: String,
    },
    DuplicateDefinition(String),
    MissingField {
        type_name: String,
        field: String,
    },
    MissingMethod {
        type_name: String,
        method: String,
        spec: String,
    },
    WrongNumberOfArguments {
        expected: usize,
        got: usize,
    },
    NotAFunction(String),
    AmbiguousMethod(String),
    UnknownSpec(String),
    CannotAssignConst(String),

    // Codegen
    UnsupportedFeature(String),
}

/// A compiler error with location information.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub kind: ErrorKind,
    pub span: Span,
    pub message: String,
}

impl CompileError {
    pub fn new(kind: ErrorKind, span: Span) -> Self {
        let message = format!("{}", kind);
        CompileError {
            kind,
            span,
            message,
        }
    }

    pub fn with_message(kind: ErrorKind, span: Span, message: String) -> Self {
        CompileError {
            kind,
            span,
            message,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ErrorKind::UnexpectedChar(c) => write!(f, "unexpected character '{}'", c),
            ErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            ErrorKind::UnterminatedChar => write!(f, "unterminated character literal"),
            ErrorKind::InvalidNumber(s) => write!(f, "invalid number literal '{}'", s),
            ErrorKind::UnexpectedToken { expected, got } => {
                write!(f, "expected {}, got {}", expected, got)
            }
            ErrorKind::ExpectedEof(got) => write!(f, "expected end of file, got {}", got),
            ErrorKind::UndefinedType(t) => write!(f, "undefined type '{}'", t),
            ErrorKind::UndefinedName(n) => write!(f, "undefined name '{}'", n),
            ErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {}, got {}", expected, got)
            }
            ErrorKind::DuplicateDefinition(n) => {
                write!(f, "duplicate definition of '{}'", n)
            }
            ErrorKind::MissingField { type_name, field } => {
                write!(f, "type '{}' missing required field '{}'", type_name, field)
            }
            ErrorKind::MissingMethod {
                type_name,
                method,
                spec,
            } => {
                write!(
                    f,
                    "type '{}' missing method '{}' required by spec '{}'",
                    type_name, method, spec
                )
            }
            ErrorKind::WrongNumberOfArguments { expected, got } => {
                write!(
                    f,
                    "wrong number of arguments: expected {}, got {}",
                    expected, got
                )
            }
            ErrorKind::NotAFunction(n) => write!(f, "'{}' is not a function", n),
            ErrorKind::AmbiguousMethod(n) => {
                write!(f, "ambiguous method '{}': multiple candidates", n)
            }
            ErrorKind::UnknownSpec(s) => write!(f, "unknown spec '{}'", s),
            ErrorKind::CannotAssignConst(name) => {
                write!(f, "cannot assign to constant '{}'", name)
            }
            ErrorKind::UnsupportedFeature(feat) => {
                write!(f, "unsupported feature: {}", feat)
            }
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error at {}: {}", self.span, self.kind)
    }
}

impl std::error::Error for CompileError {}

pub type Result<T> = std::result::Result<T, CompileError>;
