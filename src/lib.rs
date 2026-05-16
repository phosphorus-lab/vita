//! Vita Compiler
//!
//! A compiler for the Vita programming language.
//! Pipeline: Source → Lexer → Parser → Type Checker → Codegen → LLVM IR

pub mod backend;
pub mod diagnostics;
pub mod semantics;
pub mod syntax;

// Backwards-compatible public module aliases for existing users of the crate.
pub use backend::codegen;
pub use diagnostics::error;
pub use semantics::{checker, types};
pub use syntax::{ast, lexer, parser, token};
