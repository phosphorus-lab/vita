//! Debug-oriented standard I/O surface.
//!
//! `log` is the forward-compatible debug output function. `print` remains
//! registered in `functions.rs` for compatibility, but can be retired later.
//! `inp` is the future standard input hook.

use crate::semantics::types::{Type, TypeEnv};

pub fn register(env: &mut TypeEnv) {
    for ty in [
        Type::Str,
        Type::Bool,
        Type::Char,
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::I128,
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::U128,
        Type::F16,
        Type::F32,
        Type::F64,
        Type::F128,
    ] {
        env.define_function("log", vec![("value".to_string(), ty)], Type::Unit);
    }

    env.define_function("inp", Vec::new(), Type::Str);
}
