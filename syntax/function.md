Vita Function Concept

Functions in Vita use explicit argument types.

Argument types are not inferred.
A function boundary should be clear.

Example:

fn add(a: i32, b: i32) {
    a + b
}

Here:

a: i32
b: i32

The return type is inferred as i32 because the last expression is:

a + b

---

Return type

A function may write its return type explicitly:

fn add(a: i32, b: i32) -> i32 {
    a + b
}

Or omit it when the compiler can infer it:

fn add(a: i32, b: i32) {
    a + b
}

If a function has no return value, it returns `()`.

These are equivalent:

fn log(msg: str) {
    print(msg);
}

fn log(msg: str) -> () {
    print(msg);
}

---

Last expression return

If the last expression in a function has no semicolon, it becomes the return value.

fn is_zero(x: i32) {
    x == 0
}

This returns bool.

If the last line is a statement with semicolon, the function returns `()` unless an explicit return is used.

---

Default arguments

Functions may have default arguments.

fn greet(name: str, prefix: str = "hello") {
    print(prefix + " " + name);
}

greet("Rin");
greet("Rin", "hi");

Arguments with default values should come after required arguments.

---

No function overload in core

Vita does not use multiple functions with the same name and different argument types as a core feature.

This is not preferred:

fn show(x: i32) { ... }
fn show(x: str) { ... }
fn show(x: Vec2) { ... }

If a function needs to work with many types, use generics or specs instead.

! can be written as arrow function like... let a = () => {...};
lambda argument must has type
return type infer from body
last expression without ; = return

---

Summary

1. Function arguments must have explicit types.
2. Return type can be inferred when clear.
3. No written return type and no returned expression means `()`.
4. Last expression without semicolon returns.
5. Default arguments are allowed.
6. Function overload is not part of the core model.
