Vita Type System Concept

Vita has four core type-level constructs:

- def
- enum
- spec
- impl

These are not classes.
Vita does not use inheritance as the core model.
Vita separates data, variants, specification, and behavior clearly.

---

def

`def` defines a data type.

A `def` is a record-like type that describes the fields of a value.
It only contains data fields.
It does not contain methods.

Example:

def Vec2 {
    x: f32,
    y: f32
}

This defines a new type named `Vec2`.
A `Vec2` value has two fields: `x` and `y`.

Values of a `def` type are immutable from the developer's point of view.
Updating a field creates a new value semantically.
The compiler may optimize this internally when it is safe.

---

enum

`enum` defines a variant type.

An enum is a type whose value can be one of several variants.
It is used when one type needs to represent multiple possible forms.

Example:

def Button {
    text: str
}

def TextBox {
    value: str
}

enum Widget {
    Button,
    TextBox
}

This defines a new type named `Widget`.
A `Widget` can contain either a `Button` or a `TextBox`.

Enums are used for closed sets of possibilities.
If all possible variants are known, enum is preferred over inheritance.

---

spec

`spec` defines an optional specification/category for types.

A spec is not a class.
A spec is not inherited.
A spec does not automatically apply to any type.

A spec describes what a type must have if that type explicitly chooses to satisfy the spec.

Example:

spec Animal {
    name: str
    fn speak(self) -> str
}

This means any type that claims to satisfy `Animal` must have:

- a field `name: str`
- a function `speak(self) -> str`

A type is not forced to implement a spec.
But if it does, the compiler checks that the type satisfies the spec.

Example:

def Dog {
    name: str
}

impl Dog: Animal {
    fn speak(self) -> str {
        "woof"
    }
}

Here, `Dog` explicitly satisfies the `Animal` spec.
The compiler checks that `Dog` has `name: str` and that its `impl Dog: Animal` provides `speak`.

If something is missing, it is a compile error.

Example error:

def Cat {
    age: i32
}

impl Cat: Animal {
    fn speak(self) -> str {
        "meow"
    }
}

This is invalid because `Cat` does not have `name: str`.

---

impl

`impl` defines behavior for a type.

An impl block can only contain functions.
It cannot contain fields.

Fields belong to `def`.
Variants belong to `enum`.
Requirements belong to `spec`.
Functions belong to `impl`.

Example:

impl Vec2 {
    fn len(self) -> f32 {
        sqrt(self.x * self.x + self.y * self.y)
    }

    pub fn add(self, other: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + other.x,
            y: self.y + other.y
        }
    }
}

This adds methods to `Vec2`.

Calling a method:

let a = Vec2 { x: 1.0, y: 2.0 };
let b = Vec2 { x: 3.0, y: 4.0 };

let c = a.add(b);

Method calls are syntax sugar for calling functions associated with a type.
They do not imply class inheritance or object mutation.

---

impl Type

`impl Type` defines normal methods for a type.

Example:

impl Dog {
    pub fn bark(self) -> str {
        "woof"
    }
}

These methods belong to `Dog`.
They are available on Dog values.

Example:

let dog = Dog { name: "Pochi" };
dog.bark();

---

impl Type: Spec

`impl Type: Spec` defines the functions needed for a type to satisfy a spec.

Example:

spec Renderable {
    fn render(self);
}

def Button {
    text: str
}

impl Button: Renderable {
    fn render(self) {
        print(self.text);
    }
}

This means `Button` satisfies the `Renderable` spec.

The compiler checks that every required function from `Renderable` is implemented.
The compiler also checks that every required field from `Renderable` exists in `Button`.

---

Rules

1. `def` defines data.
2. `enum` defines variants.
3. `spec` defines optional requirements.
4. `impl` defines functions.
5. `impl` can only contain `fn`.
6. `impl Type` adds normal methods to a type.
7. `impl Type: Spec` makes a type satisfy a spec.
8. A type does not satisfy a spec unless it explicitly declares `impl Type: Spec`.
9. Specs are for organization and compiler checking, not inheritance.
10. Vita does not use classes as its core model.

---

Example

spec Animal {
    name: str;
    fn speak(self) -> str;
}

def Dog {
    name: str,
}

def Cat {
    name: str,
}

impl Dog {
    fn wag(self) {
        print(self.name + " is wagging");
    }
}

impl Dog: Animal {
    fn speak(self) -> str {
        "woof"
    }
}

impl Cat: Animal {
    fn speak(self) -> str {
        "meow"
    }
}

enum Pet {
    Dog,
    Cat,
}

impl Pet {
    pub fn speak(self) -> str {
        $ self {
            Dog(d) => d.speak(),
            Cat(c) => c.speak(),
        }
    }
}

fn main() {
    let dog = Dog { name: "Pochi" };
    let cat = Cat { name: "Mimi" };

    print(dog.speak());
    print(cat.speak());

    let pet = Pet::Dog(dog);
    print(pet.speak());
}

enum Pet {
    Dog,
    Cat,
}

This defines a new variant type named `Pet`.

Each variant in this enum refers to a type with the same name.

So this:

enum Pet {
    Dog,
    Cat,
}

is shorthand for:

enum Pet {
    Dog(Dog),
    Cat(Cat),
}

That means a `Pet` value can be either:

- `Dog`, containing a `Dog` value
- `Cat`, containing a `Cat` value

Example:

let dog = Dog { name: "Pochi" };
let pet = Pet::Dog(dog);

Here, `pet` has type `Pet`.
Inside it, there is a `Dog` value.

To use the value inside an enum, match it:

$ pet {
    Dog(d) => d.speak(),
    Cat(c) => c.speak(),
}

In this match:

Dog(d)

means:

If `pet` is the `Dog` variant, unwrap the Dog value inside it
and bind that value to the name `d`.

Inside that branch:

d: Dog

So this is valid:

d.speak()

Because `d` is a `Dog`.

Likewise:

Cat(c)

unwraps the Cat value inside the Cat variant.

Inside that branch:

c: Cat

So:

c.speak()

calls the `Cat` implementation.

An enum is useful when one value can be one of several known types.

`Pet` is not a base class.
`Dog` and `Cat` do not inherit from `Pet`.

`Pet` is a closed variant type.
It explicitly lists every possible form it can contain.
