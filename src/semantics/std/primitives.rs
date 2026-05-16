//! Methods on primitive built-in types such as `str`, `char`, and integers.

use crate::semantics::types::{Type, TypeEnv};

pub fn register_methods(env: &mut TypeEnv) {
    env.define_method(Type::Str, "len", Vec::new(), Type::I64);
    env.define_method(Type::Str, "is_empty", Vec::new(), Type::Bool);
    env.define_method(
        Type::Str,
        "slice",
        vec![
            ("start".to_string(), Type::I64),
            ("end".to_string(), Type::I64),
        ],
        Type::Str,
    );

    env.define_method(Type::Char, "is_digit", Vec::new(), Type::Bool);
    env.define_method(Type::Char, "is_alpha", Vec::new(), Type::Bool);
    env.define_method(Type::Char, "is_alnum", Vec::new(), Type::Bool);
    env.define_method(Type::Char, "is_whitespace", Vec::new(), Type::Bool);

    for ty in [Type::I8, Type::I16, Type::I32, Type::I64, Type::I128] {
        env.define_method(ty.clone(), "abs", Vec::new(), ty);
    }
}
