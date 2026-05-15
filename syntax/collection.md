Vita Collection Concept

Vita has four core collection forms:

- fixed array
- dynamic Array
- Map
- Set

There is no Vector type.
`Array<T>` is already the optimized dynamic array type.

---

Fixed array

Array literals with known size infer as fixed-size arrays.

let nums = [1, 2, 3];        // [i32; 3]
let names = ["Rin", "Me"];   // [str; 2]

A fixed array type is written as:

[T; N]

Example:

let nums: [i32; 3] = [1, 2, 3];

Fixed arrays are useful for low-level code because the size is known at compile time.

---

Dynamic Array

Dynamic arrays use:

Array<T>

Example:

let nums: Array<i32> = [1, 2, 3];

Use `Array<T>` when the collection needs dynamic behavior such as growing, resizing, or appending.

Array length returns u32.

let size = nums.len(); // u32

Indexing uses normal integer syntax.

let first = nums[0];

Updating an array follows Vita's immutable value model.

nums[1] = 99;

For developers, this creates a new array value semantically.
The old value is not changed.

The compiler may optimize this into an in-place update if the old array value cannot be observed anymore.

---

Map

Map is a built-in generic collection.

Map<K, V>

A Map stores key-value pairs.

Example:

let animals: Map<str, i32> = {
    "cat": 0,
    "dog": 1,
};

Map literal uses:

{
    key: value,
}

Map methods are defined through impl.

impl Map<K, V> {
    pub fn len(self) -> u32;
    pub fn get(self, key: K) -> Option<V>;
    pub fn has(self, key: K) -> bool;
}

Map is implemented by the compiler/runtime, such as with a hash table.

---

Set

Set is also a built-in generic collection.

Set<T>

A Set stores unique values.

Example:

let tags: Set<str> = {
    "compiler",
    "safe",
    "low-level",
};

Set literal uses:

{
    value,
    value,
}

Set methods are defined through impl.

impl Set<T> {
    pub fn len(self) -> u32;
    pub fn has(self, value: T) -> bool;
    pub fn add(self, value: T) -> Set<T>;
    pub fn remove(self, value: T) -> Set<T>;
}

Set can be implemented internally like Map, but without values.

---

Literal rules

[1, 2, 3]
= fixed array by default

let xs: Array<i32> = [1, 2, 3];
= dynamic array because the expected type is Array<i32>

{
    "cat": 0,
    "dog": 1,
}
= Map literal when the expected type is Map<K, V>

{
    "cat",
    "dog",
}
= Set literal when the expected type is Set<T>

Dog {
    name: "Pochi",
}
= def value literal

---

Summary

1. `[T; N]` is a fixed-size array.
2. Array literals infer fixed arrays by default.
3. `Array<T>` is the dynamic optimized array.
4. Vita does not need a separate Vector type.
5. `Map<K, V>` is a built-in key-value collection.
6. `Set<T>` is a built-in unique-value collection.
7. Map and Set use impl methods like normal types.
8. All collection updates follow Vita immutable semantics.
9. The compiler may mutate storage in-place when it is safe.

Vita Tuple Concept

Tuple is a fixed-size value that can contain multiple types.

Tuple uses parentheses.

Tuple type:

(i32, u32, str)

Tuple value:

(-5, 5u, "Me")

Example:

let tup: (i32, u32, str) = (-5, 5u, "Me");

Tuple is useful when a value needs to group multiple values together
without defining a named def type.

Tuple elements are ordered.

Access can use index syntax:

let a = tup.0;
let b = tup.1;
let c = tup.2;

Here:

a: i32
b: u32
c: str

Tuple is not an array.

Array:

[i32; 3]

means fixed-size array with one element type.

Tuple:

(i32, u32, str)

means fixed-size group with different element types.

Summary:

1. Tuple uses `(...)`.
2. Tuple type is `(T1, T2, T3)`.
3. Tuple value is `(v1, v2, v3)`.
4. Tuple can contain different types.
5. Tuple is fixed-size.
6. Tuple access uses `.0`, `.1`, `.2`.
7. Tuple is different from array.
