Vita Generic Concept

Generics in Vita are type parameters.

They are written with angle brackets:

<T>
<A, B>
<T, E>

Generics can be used on:

- def
- enum
- fn
- impl

Generics are not tied to spec.
A spec is only used for type/category checking through impl Type: Spec.

---

Generic def

A def can have type parameters.

def Pair<A, B> {
    first: A,
    second: B,
}

This defines a type Pair with two generic fields.

Example:

let pair = Pair<i32, str> {
    first: 10,
    second: "hello",
};

---

Generic enum

An enum can have type parameters.

enum Result<T, E> {
    Ok(T),
    Err(E),
}

This defines a result type that can contain either:

- Ok(T)
- Err(E)

Example:

let value = Result<i32, str>::Ok(10);

---

Generic function

A function can have type parameters.

fn id<T>(x: T) {
    x
}

fn pair<A, B>(a: A, b: B) {
    Pair<A, B> {
        first: a,
        second: b,
    }
}

Function arguments still need explicit types.
Generic type parameters count as explicit types.

---

Generic impl

An impl block can use type parameters.

impl Pair<A, B> {
    fn swap(self) {
        Pair<B, A> {
            first: self.second,
            second: self.first,
        }
    }
}

For generic types, the impl applies to every valid instantiation of that type.

---

Spec is separate

Spec does not control generic functions.

This is not part of the core model:

fn show<T: SomeSpec>(x: T) { ... }

Instead, spec is only used with impl Type: Spec.

Example:

spec Animal {
    name: str;
    pub fn speak(self) -> str;
}

def Dog {
    name: str,
}

impl Dog: Animal {
    fn speak(self) {
        "woof"
    }
}

---

Summary

1. Generics are type parameters.
2. Generics use `<T>`, `<A, B>`, etc.
3. def, enum, fn, and impl can be generic.
4. Function arguments must still have explicit types.
5. Generic parameters are valid explicit types.
6. Spec is separate from generics.
7. Spec is only for checking impl Type: Spec.
