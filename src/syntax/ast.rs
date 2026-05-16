//! Abstract Syntax Tree definitions for the Vita language.

use crate::syntax::token::TokenKind;
use std::fmt;

/// A top-level item in a Vita source file.
#[derive(Debug, Clone)]
pub enum Item {
    Def(DefItem),
    Enum(EnumItem),
    Spec(SpecItem),
    Impl(ImplItem),
    Fn(FnItem),
    Use(UseItem),
}

/// A `def` data type definition.
#[derive(Debug, Clone)]
pub struct DefItem {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<FieldDef>,
}

/// A single field in a `def`.
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_ann: TypeExpr,
}

/// An `enum` variant type definition.
#[derive(Debug, Clone)]
pub struct EnumItem {
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<VariantDef>,
}

/// A single variant in an `enum`.
#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub payload: Option<TypeExpr>,
}

/// A `spec` specification definition.
#[derive(Debug, Clone)]
pub struct SpecItem {
    pub name: String,
    pub members: Vec<SpecMember>,
}

/// A member of a spec (field requirement or function signature).
#[derive(Debug, Clone)]
pub enum SpecMember {
    Field {
        name: String,
        type_ann: TypeExpr,
    },
    Fn {
        is_pub: bool,
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
    },
}

/// An `impl` block.
#[derive(Debug, Clone)]
pub struct ImplItem {
    pub target_type: String,
    pub target_generics: Vec<String>,
    pub spec_name: Option<String>, // Some for `impl Type: Spec`
    pub methods: Vec<FnItem>,
}

/// A standalone function definition.
#[derive(Debug, Clone)]
pub struct FnItem {
    pub is_pub: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: TypeExpr,
    pub default: Option<Expr>,
}

/// A `use` import statement.
#[derive(Debug, Clone)]
pub struct UseItem {
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub symbols: Vec<UseSymbol>,
}

#[derive(Debug, Clone)]
pub struct UseSymbol {
    pub name: String,
    pub alias: Option<String>,
}

/// A type expression.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Named(String),
    Generic {
        name: String,
        args: Vec<TypeExpr>,
    },
    Array {
        element: Box<TypeExpr>,
        size: Option<usize>,
    },
    Tuple(Vec<TypeExpr>),
    Fn {
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
    },
    SelfType,
    Unit,
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TypeExpr::Named(n) => write!(f, "{}", n),
            TypeExpr::Generic { name, args } => {
                write!(f, "{}<", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            TypeExpr::Array { element, size } => match size {
                Some(s) => write!(f, "[{}; {}]", element, s),
                None => write!(f, "[{}]", element),
            },
            TypeExpr::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            TypeExpr::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            TypeExpr::SelfType => write!(f, "Self"),
            TypeExpr::Unit => write!(f, "()"),
        }
    }
}

/// A block of statements with an optional trailing expression.
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
}

/// A statement (does not produce a value).
#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        type_ann: Option<TypeExpr>,
        value: Expr,
    },
    Expr(Expr),
    SemiExpr(Expr), // expression with trailing semicolon
    Break,
    Continue,
    Return(Option<Expr>),
}

/// An expression (produces a value).
#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Char(char),
    Unit,

    // Variables
    Ident(String),

    // Binary operations
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    // Unary operations
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },

    // Assignment / compound assignment
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    // Function call
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },

    // Method call
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },

    // Field access
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },

    // Tuple access
    TupleAccess {
        tuple: Box<Expr>,
        index: usize,
    },

    // Index access
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    // If expression: ? { } !? { } ! { }
    If {
        condition: Box<Expr>,
        then_block: Box<Block>,
        else_if_clauses: Vec<(Expr, Box<Block>)>,
        else_block: Option<Box<Block>>,
    },

    // Match expression: $ expr { ... }
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },

    // Loop: * { }
    Loop(Box<Block>),

    // While loop: * condition { }
    While {
        condition: Box<Expr>,
        body: Box<Block>,
    },

    // For-each: *? item: items { }
    ForEach {
        var: String,
        iterable: Box<Expr>,
        body: Box<Block>,
    },

    // Fallible block: ?? { }
    Fallible {
        block: Box<Block>,
        handler: Option<FallibleHandler>,
    },

    // Struct/Def literal: TypeName { field: value, ... }
    StructLiteral {
        type_name: String,
        fields: Vec<(String, Expr)>,
    },

    // Enum variant construction: Type::Variant(value)
    EnumVariant {
        type_name: String,
        variant: String,
        value: Option<Box<Expr>>,
    },

    // Array literal: [1, 2, 3]
    ArrayLiteral(Vec<Expr>),

    // Tuple literal: (a, b, c)
    TupleLiteral(Vec<Expr>),

    // Map literal: { key: value, ... }
    MapLiteral(Vec<(Expr, Expr)>),

    // Set literal: { value, ... }
    SetLiteral(Vec<Expr>),

    // Lambda: (args) => body
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },

    // Grouped expression: (expr)
    Grouped(Box<Expr>),
}

/// A match arm: Pattern => body
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
}

/// A pattern for match expressions.
#[derive(Debug, Clone)]
pub enum Pattern {
    // Wildcard: _
    Wildcard,
    // Identifier binding: name
    Ident(String),
    // Enum variant destructuring: Variant(name) or Type::Variant(name)
    Variant {
        type_name: Option<String>,
        variant: String,
        binding: Option<String>,
    },
    // Literal pattern
    Int(i64),
    Bool(bool),
    String(String),
}

/// Handler for fallible blocks.
#[derive(Debug, Clone)]
pub enum FallibleHandler {
    // !! err { body }
    Catch {
        err_name: String,
        body: Box<Block>,
    },
    // !$ err { arms }
    CatchMatch {
        err_name: String,
        arms: Vec<MatchArm>,
    },
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn from_token(kind: TokenKind) -> Option<BinOp> {
        match kind {
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star | TokenKind::StarMul => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            TokenKind::EqEq => Some(BinOp::Eq),
            TokenKind::BangEq => Some(BinOp::Neq),
            TokenKind::Lt => Some(BinOp::Lt),
            TokenKind::Gt => Some(BinOp::Gt),
            TokenKind::LtEq => Some(BinOp::LtEq),
            TokenKind::GtEq => Some(BinOp::GtEq),
            TokenKind::AmpAmp => Some(BinOp::And),
            TokenKind::PipePipe => Some(BinOp::Or),
            TokenKind::Amp => Some(BinOp::BitAnd),
            TokenKind::Pipe => Some(BinOp::BitOr),
            TokenKind::Caret => Some(BinOp::BitXor),
            TokenKind::LtLt => Some(BinOp::Shl),
            TokenKind::GtGt => Some(BinOp::Shr),
            _ => None,
        }
    }

    /// Precedence level (higher = binds tighter).
    pub fn precedence(&self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::BitOr => 3,
            BinOp::BitXor => 4,
            BinOp::BitAnd => 5,
            BinOp::Eq | BinOp::Neq => 6,
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => 7,
            BinOp::Shl | BinOp::Shr => 8,
            BinOp::Add | BinOp::Sub => 9,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 10,
        }
    }

    pub fn is_associative(&self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
        )
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg, // -x
    Not, // !x
}

/// Compound assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    AddEq,
    SubEq,
    MulEq,
    DivEq,
    ModEq,
}
