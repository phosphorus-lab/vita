Vita Null / Unit Concept

Vita has no `null`.

If a value may be missing, use `Option<T>`.

enum Option<T> {
    Some(T),
    None,
}

Example:

fn find_user(id: u32) -> Option<User> {
    ...
}

---

Vita has no `void` keyword.

Use `()` for functions that return no meaningful value.

fn log(msg: str) -> () {
    print(msg);
}

This can also be written as:

fn log(msg: str) {
    print(msg);
}

No return type means the function returns `()`.

Summary:

1. No null.
2. No void keyword.
3. Use `()` for unit / no meaningful value.
4. Use `Option<T>` for maybe-missing values.
