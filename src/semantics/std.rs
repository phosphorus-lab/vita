//! Standard library type and builtin declarations.
//!
//! This module is the semantic registry for Vita's built-in surface area:
//! primitive type methods, standard generic type constructors, and builtin
//! functions. Keeping these declarations out of the checker/type environment
//! makes it easier to add methods like `str.len()` or `i32.abs()` in one place.

use crate::semantics::types::{Type, TypeEnv, TypeInfo};

/// Register all standard library declarations into a type environment.
pub fn register(env: &mut TypeEnv) {
    register_functions(env);
    register_type_constructors(env);
    register_primitive_methods(env);
}

/// Resolve a generic standard type constructor such as `Array<T>` or
/// `Result<T, E>`.
pub fn resolve_generic_type(
    name: &str,
    args: &[Type],
) -> Option<::std::result::Result<Type, usize>> {
    let resolved = match name {
        "Array" => expect_arity(args, 1).map(|_| Type::DynArray(Box::new(args[0].clone()))),
        "Option" => expect_arity(args, 1).map(|_| Type::Option(Box::new(args[0].clone()))),
        "Result" => expect_arity(args, 2).map(|_| Type::Result {
            ok: Box::new(args[0].clone()),
            err: Box::new(args[1].clone()),
        }),
        "Map" => expect_arity(args, 2).map(|_| Type::Map {
            key: Box::new(args[0].clone()),
            value: Box::new(args[1].clone()),
        }),
        "Set" => expect_arity(args, 1).map(|_| Type::Set(Box::new(args[0].clone()))),
        _ => return None,
    };

    Some(resolved)
}

fn expect_arity(args: &[Type], expected: usize) -> ::std::result::Result<(), usize> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(expected)
    }
}

fn register_functions(env: &mut TypeEnv) {
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

fn register_type_constructors(env: &mut TypeEnv) {
    env.define_type(
        "Array".to_string(),
        TypeInfo::Def {
            name: "Array".to_string(),
            generics: vec!["T".to_string()],
            fields: Vec::new(),
        },
    );

    env.define_type(
        "Map".to_string(),
        TypeInfo::Def {
            name: "Map".to_string(),
            generics: vec!["K".to_string(), "V".to_string()],
            fields: Vec::new(),
        },
    );

    env.define_type(
        "Set".to_string(),
        TypeInfo::Def {
            name: "Set".to_string(),
            generics: vec!["T".to_string()],
            fields: Vec::new(),
        },
    );

    env.define_type(
        "Option".to_string(),
        TypeInfo::Enum {
            name: "Option".to_string(),
            generics: vec!["T".to_string()],
            variants: vec![
                ("Some".to_string(), Some(Type::Var("T".to_string()))),
                ("None".to_string(), None),
            ],
        },
    );

    env.define_type(
        "Result".to_string(),
        TypeInfo::Enum {
            name: "Result".to_string(),
            generics: vec!["T".to_string(), "E".to_string()],
            variants: vec![
                ("Ok".to_string(), Some(Type::Var("T".to_string()))),
                ("Err".to_string(), Some(Type::Var("E".to_string()))),
            ],
        },
    );
}

fn register_primitive_methods(env: &mut TypeEnv) {
    let generic_array = Type::DynArray(Box::new(Type::Var("T".to_string())));
    env.define_method(generic_array.clone(), "len", Vec::new(), Type::I64);
    env.define_method(
        generic_array.clone(),
        "push",
        vec![("value".to_string(), Type::Var("T".to_string()))],
        generic_array,
    );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_standard_type_constructors() {
        let env = TypeEnv::new();

        assert!(env.lookup_type("Array").is_some());
        assert!(env.lookup_type("Map").is_some());
        assert!(env.lookup_type("Set").is_some());
        assert!(env.lookup_type("Option").is_some());
        assert!(env.lookup_type("Result").is_some());
    }

    #[test]
    fn resolves_standard_generic_types() {
        assert_eq!(
            resolve_generic_type("Array", &[Type::I32]).and_then(|r| r.ok()),
            Some(Type::DynArray(Box::new(Type::I32)))
        );
        assert_eq!(
            resolve_generic_type("Map", &[Type::Str, Type::I32]).and_then(|r| r.ok()),
            Some(Type::Map {
                key: Box::new(Type::Str),
                value: Box::new(Type::I32),
            })
        );
        assert_eq!(resolve_generic_type("Set", &[]), Some(Err(1)));
    }

    #[test]
    fn registers_primitive_methods() {
        let env = TypeEnv::new();

        let len = env
            .lookup_fn("len")
            .and_then(|methods| {
                methods
                    .iter()
                    .find(|method| method.receiver_type == Some(Type::Str))
            })
            .expect("str.len should be registered");
        assert_eq!(len.return_type, Type::I64);

        let abs = env
            .lookup_fn("abs")
            .and_then(|methods| {
                methods
                    .iter()
                    .find(|method| method.receiver_type == Some(Type::I32))
            })
            .expect("i32.abs should be registered");
        assert_eq!(abs.return_type, Type::I32);
    }
}
