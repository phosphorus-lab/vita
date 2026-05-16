//! Standard composite/container type constructors and their methods.
//!
//! `Array<T>`, `Map<K, V>`, `Set<T>`, `Option<T>`, and `Result<T, E>` are
//! named standard type constructors. Tuple types are structural syntax in Vita
//! today, so they are represented directly as `Type::Tuple` rather than through
//! a named constructor.

use crate::semantics::types::{Type, TypeEnv, TypeInfo};

pub fn register_type_constructors(env: &mut TypeEnv) {
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

pub fn register_methods(env: &mut TypeEnv) {
    let generic_array = Type::DynArray(Box::new(Type::Var("T".to_string())));
    env.define_method(generic_array.clone(), "len", Vec::new(), Type::I64);
    env.define_method(
        generic_array.clone(),
        "push",
        vec![("value".to_string(), Type::Var("T".to_string()))],
        generic_array,
    );
}

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
