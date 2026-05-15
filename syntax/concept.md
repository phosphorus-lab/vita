Vita is immutable for developers, but mutable for the compiler.

Every value in Vita is immutable.
A variable is a binding to a compiler-tracked value handle.

let a = 10;

Semantically, `a` points to an immutable i32 value.
The compiler may store that value however it wants: register, stack, inline, or heap.

a = 15;

This does not mutate the old value.
It creates a new immutable value and rebinds `a` to it.

If the old value is never observed again, the compiler may remove it entirely.
So this:

let a = 10;
a = 15;

can be compiled as:

let a = 15;

---

let a = 5;
let b = a;

Both `a` and `b` point to the same immutable value.

b = b + 1;

This creates a new value for `b`.
It does not affect `a`.

a -> 5
b -> 6

The old value can only be freed when no live binding or value graph can reach it anymore.

---

fn SomeFunc(arg: i32) -> i32 {
    let a = 123;
    let b = 456;

    arg + b
}

fn main() {
    let num = 55;
    let get_from_func = SomeFunc(num);
}

Calling `SomeFunc(num)` passes the value handle of `num`.
Inside the function, `arg` is a local binding to the same immutable value.

`a` is never used, so it can be removed.
`b` is used only to compute the return value, so it can be freed or optimized away after its last use.
`arg` disappears when the function ends, but the value it points to is still tracked from `num`.
`arg + b` creates a new immutable value.
Because that value is returned, it escapes the function and becomes bound to `get_from_func`.
