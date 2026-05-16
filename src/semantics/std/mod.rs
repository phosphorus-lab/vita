//! Standard library type and builtin declarations.
//!
//! This module is the semantic registry for Vita's built-in surface area.
//! Keep builtins organized by domain so primitive methods, composite types,
//! and free functions can grow without turning the checker into the registry.

mod composites;
mod functions;
mod io;
mod primitives;

use crate::semantics::types::{Type, TypeEnv};

/// Register all standard library declarations into a type environment.
pub fn register(env: &mut TypeEnv) {
    functions::register(env);
    io::register(env);
    composites::register_type_constructors(env);
    composites::register_methods(env);
    primitives::register_methods(env);
}

/// Resolve a generic standard type constructor such as `Array<T>` or
/// `Result<T, E>`.
pub fn resolve_generic_type(
    name: &str,
    args: &[Type],
) -> Option<::std::result::Result<Type, usize>> {
    composites::resolve_generic_type(name, args)
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
