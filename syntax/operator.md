Vita Operator Concept

Vita supports basic operators for primitive types and built-in collections.

Operators follow Vita's immutable value model.
Assignment-like operators do not mutate the old value from the developer's point of view.
They create a new value semantically, while the compiler may optimize storage in-place when safe.

---

Arithmetic operators

+   add
-   subtract
*   multiply
/   divide
%   modulo

Example:

let a = 10 + 5;
let b = 10 - 5;
let c = 10 * 5;
let d = 10 / 5;
let e = 10 % 3;

Arithmetic works on numeric primitive types.

---

Comparison operators

==  equal
!=  not equal
<   less than
<=  less than or equal
>   greater than
>=  greater than or equal

Example:

let ok = age >= 18;

Comparison returns bool.

---

Boolean operators

&&  and
||  or
!   not

Example:

let can_enter = age >= 18 && has_ticket;
let blocked = !can_enter;

---

Bitwise operators

&   bitwise and
|   bitwise or
^   bitwise xor
<<  shift left
>>  shift right

Bitwise operators work on integer primitive types.

Example:

let flags = a | b;
let masked = flags & mask;

---

Assignment / update operators

=    bind to new value
+=   add and rebind
-=   subtract and rebind
*=   multiply and rebind
/=   divide and rebind
%=   modulo and rebind

Example:

let x = 10;
x += 5;

Semantically:

x = x + 5;

This does not mutate the old value.
It creates a new value and rebinds x to it.

The compiler may optimize this into an in-place update if the old value cannot be observed anymore.

---

Indexing

Arrays can be indexed with:

arr[index]

Example:

let nums = [1, 2, 3];
let first = nums[0];

Array index uses normal integer syntax.
The expected index type is u32.

Map can also use indexing if the key exists or if the operation is defined by Map.

Example:

let value = map[key];

For safe access, Map should provide get:

map.get(key) // Option<V>

---

Index update

Array update:

nums[1] = 99;

Map update:

map[key] = value;

Set update should use methods:

set.add(value);
set.remove(value);

All updates follow Vita immutable semantics.

---

Operator overload

Operator overload is not part of the core v1 model.

Primitive types and built-in collections support operators directly.

Custom types should use normal methods first.

Example:

impl Vec2 {
    pub fn add(self, other: Vec2) {
        Vec2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

let c = a.add(b);

Operator overload may be added later through impl if needed.

---

Summary

1. Primitive numeric types support arithmetic operators.
2. Comparison operators return bool.
3. Boolean operators work on bool.
4. Bitwise operators work on integers.
5. Assignment/update operators create new values semantically.
6. Compiler may optimize updates in-place when safe.
7. Arrays and maps support indexing.
8. Custom operator overload is not required in core v1.
