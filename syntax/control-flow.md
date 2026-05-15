Vita Control Flow Concept

Vita uses symbolic control flow.

The core control flow constructs are:

?   if
!?  else-if
!   else

$   match

*   loop / while
*?  for-each

??  fallible block
!!  catch block
!$  catch-match block

break
continue

---

if / else

`?` is used for conditional branching.

Example:

? score >= 80 {
    print("A");
} ! {
    print("B");
}

`!` is used as else.

---

else-if

`!?` is used as else-if.

Example:

? score >= 80 {
    print("A");
} !? score >= 70 {
    print("B");
} !? score >= 60 {
    print("C");
} ! {
    print("F");
}

---

if as expression

An if block can be used as an expression.

Example:

let grade = ? score >= 80 {
    "A"
} !? score >= 70 {
    "B"
} ! {
    "C"
};

The final expression inside the selected branch becomes the value of the whole conditional expression.

If all branches return the same type, the conditional expression has that type.

If branches return different types, Vita may infer an enum/union-like result when possible, or require explicit annotation.

---

match

`$` is used for pattern matching.

Example:

$ pet {
    Dog(d) => d.speak(),
    Cat(c) => c.speak(),
}

If `pet` is an enum value, each branch can unwrap a variant.

Example:

Dog(d)

means:

If the value is the `Dog` variant, unwrap the value inside it and bind it to `d`.

Inside that branch:

d: Dog

Likewise:

Cat(c)

binds the inner Cat value to `c`.

The `_` pattern is used as fallback.

Example:

$ value {
    Some(x) => x,
    None => 0,
    _ => -1,
}

---

match as expression

A match block can return a value.

Example:

let sound = $ pet {
    Dog(d) => d.speak(),
    Cat(c) => c.speak(),
};

The selected branch expression becomes the value of the match expression.

---

infinite loop

`*` without a condition creates an infinite loop.

Example:

* {
    tick();

    ? should_stop {
        break;
    }
}

---

while loop

`* condition` creates a loop that continues while the condition is true.

Example:

* count < 10 {
    print(count);
    count = count + 1;
}

This means:

while count < 10 {
    ...
}

---

for-each loop

`*? item: items` iterates over a collection.

Example:

*? item: items {
    print(item);
}

This means each value in `items` is bound to `item` during the loop body.

Example with index can be added later.

---

break

`break` exits the nearest loop.

Example:

* {
    ? done {
        break;
    }
}

---

continue

`continue` skips to the next loop iteration.

Example:

*? item: items {
    ? item == 0 {
        continue;
    }

    print(item);
}

---

fallible block

`??` creates a fallible block.

A fallible block is used for computations that may return `Result<T, E>` or another error-aware value.

Important:

`?? { ... }` does not automatically unwrap the result.

If there is no handler after it, the value remains whatever the block returns.

Example:

let result = ?? {
    read_file("data.txt")
};

If `read_file` returns:

Result<str, FileError>

then `result` also has type:

Result<str, FileError>

---

catch block

`!!` handles the error side of a fallible block as a whole.

Example:

let text = ?? {
    read_file("data.txt")
} !! err {
    "failed to read file"
};

Here, `err` receives the error value.

The catch block can return any value allowed by type inference or by the surrounding context.

It may return the success type, the error type, another enum type, or any explicitly expected type.

Example:

let value = ?? {
    read_file("data.txt")
} !! err {
    err
};

In this case, the result may be an error value or an inferred enum/union-like value depending on context.

---

catch-match block

`!$` handles the error side of a fallible block by pattern matching the error value.

Example:

enum FileError {
    NotFound(str),
    PermissionDenied(str),
    InvalidEncoding,
}

let text = ?? {
    read_file("data.txt")
} !$ err {
    NotFound(path) => "file not found: " + path,
    PermissionDenied(path) => "permission denied: " + path,
    InvalidEncoding => "invalid encoding",
};

Here:

- `??` evaluates the fallible block
- `!$ err` matches the error value
- each branch handles one error variant

The error value can be unwrapped in each pattern.

Example:

NotFound(path)

means:

If the error is `NotFound`, unwrap its inner `str` value and bind it to `path`.

---

fallible block rules

1. `?? { ... }` evaluates a fallible block.
2. `?? { ... }` does not unwrap automatically.
3. Without a handler, the block result remains unchanged.
4. `!! err { ... }` handles the error as a whole.
5. `!$ err { ... }` handles the error through pattern matching.
6. Handlers can return the success type, the error type, another enum type, or any type allowed by context.
7. Vita uses enum types to represent errors.
8. Hidden exceptions are not part of the core model.

---

Examples

Basic if:

? ready {
    start();
} ! {
    wait();
}

If expression:

let label = ? active {
    "active"
} ! {
    "inactive"
};

Loop:

* running {
    update();
}

For-each:

*? user: users {
    print(user.name);
}

Match:

let sound = $ pet {
    Dog(d) => d.speak(),
    Cat(c) => c.speak(),
};

Fallible result without handling:

let result = ?? {
    read_file("data.txt")
};

Fallible result with catch:

let text = ?? {
    read_file("data.txt")
} !! err {
    "fallback text"
};

Fallible result with catch-match:

let text = ?? {
    read_file("data.txt")
} !$ err {
    NotFound(path) => "missing: " + path,
    PermissionDenied(path) => "denied: " + path,
    _ => "unknown error",
};

---

Summary

?   condition branch
!?  else-if branch
!   else branch

$   pattern match

*   infinite loop or while loop
*?  for-each loop

??  fallible block
!!  catch error block
!$  catch and match error block

break exits a loop
continue skips to the next loop iteration
