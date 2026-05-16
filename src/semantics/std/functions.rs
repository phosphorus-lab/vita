//! Free builtin functions available without importing a module.

use crate::semantics::types::{Type, TypeEnv};

pub fn register(env: &mut TypeEnv) {
    env.define_function(
        "read_file",
        vec![("path".to_string(), Type::Str)],
        Type::Str,
    );
    env.define_function(
        "write_file",
        vec![
            ("path".to_string(), Type::Str),
            ("contents".to_string(), Type::Str),
        ],
        Type::I32,
    );

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
        env.define_function("print", vec![("value".to_string(), ty)], Type::Unit);
    }
}
