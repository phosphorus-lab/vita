Vita Module and Visibility Concept

Vita keeps visibility simple.

Top-level declarations are importable by default.
There is no `pub def`, `pub enum`, `pub spec`, or `pub impl`.

The `pub` keyword is used only for functions/methods that should be accessible from outside the module.

---

Top-level declarations

These are top-level declarations:

def
enum
spec
fn

They can be imported from other files/modules.

Example:

def Dog {
    name: str,
}

enum Pet {
    Dog,
    Cat,
}

spec Animal {
    name: str;
    pub fn speak(self) -> str;
}

fn helper() {
    ...
}

Other files can import these declarations by using `use`.

---

def visibility

Fields inside `def` are accessible by default.

Example:

def Dog {
    name: str,
    age: u32,
}

Another module can access:

dog.name
dog.age

Vita does not require:

pub def Dog {
    pub name: str,
}

Instead, the type and its fields are treated as directly usable once imported.

---

impl visibility

An `impl` block does not have visibility.

There is no `pub impl`.

Visibility is controlled by each function inside the impl block.

Example:

def Dog {
    name: str,
}

impl Dog {
    pub fn bark(self) -> str {
        "woof"
    }

    fn secret(self) -> str {
        "hidden"
    }
}

Another module can call:

dog.bark()

But cannot call:

dog.secret()

Because `secret` is private to the module.

---

spec visibility

A `spec` can declare required fields and required functions.

Fields in a spec describe required data shape.

Functions in a spec can be public or private depending on whether they are marked with `pub`.

Example:

spec Animal {
    name: str;
    pub fn speak(self) -> str;
}

This means any type that implements `Animal` must have:

- field `name: str`
- function `speak(self) -> str`

Because `speak` is marked as `pub`, the implementation of `speak` becomes public when a type implements this spec.

---

impl Type: Spec visibility

When implementing a spec, methods required by the spec inherit their visibility from the spec.

Example:

spec Animal {
    name: str;
    pub fn speak(self) -> str;
}

def Dog {
    name: str,
}

impl Dog: Animal {
    fn speak(self) -> str {
        "woof"
    }
}

Even though `speak` is not written as `pub` inside the impl block, it is public because the spec declares:

pub fn speak(self) -> str;

So this is valid from another module:

dog.speak()

If an impl contains extra functions that are not required by the spec, those functions use normal impl visibility rules.

Example:

impl Dog: Animal {
    fn speak(self) -> str {
        "woof"
    }

    pub fn wag(self) {
        print(self.name + " is wagging");
    }

    fn internal_id(self) -> str {
        "dog-internal"
    }
}

Here:

- `speak` is public because it comes from `Animal`
- `wag` is public because it is explicitly marked `pub`
- `internal_id` is private because it is not marked `pub`

---

use

Vita uses `use` to import modules or symbols.

Imports are resolved relative to the file that contains the `use` statement.

Example project structure:

src/
  app/
    pages/
      home.vita
      helper.vita
  ui/
    widget.vita
  core/
    result.vita

Inside:

src/app/pages/home.vita

This import:

use helper

resolves to:

src/app/pages/helper.vita

This import:

use ..layout

resolves to:

src/app/layout.vita

This import:

use ...ui.widget

resolves to:

src/ui/widget.vita

Leading dots mean moving up from the current file location.

Rules:

use name       = module next to or below the current file location
use ..name     = go up 1 level, then import name
use ...name    = go up 2 levels, then import name
use ....name   = go up 3 levels, then import name

---

Module namespace import

By default, importing a module keeps it as a namespace.

Example:

use ui.widget

Then use declarations through the module path:

let button = ui.widget.Button { text: "OK" };

This avoids name collisions.

If two modules both have a function named `render`, they do not collide:

use ui.button
use ui.textbox

ui.button.render(...)
ui.textbox.render(...)

---

Module alias

A module can be imported with an alias.

Example:

use ui.widget as widget

Then use:

let button = widget.Button { text: "OK" };

This is useful when the full module path is too long.

---

Symbol import

A symbol can be imported directly with braces.

Example:

use ui.widget.{ Button }

Then use:

let button = Button { text: "OK" };

This imports `Button` directly into the current scope.

If a name already exists, it is a compile error.

Example:

use ui.button.{ Button }
use game.button.{ Button }

This is invalid because `Button` is imported twice into the same scope.

Use an alias instead:

use ui.button.{ Button as UiButton }
use game.button.{ Button as GameButton }

---

Combined import examples

use ui.widget
use ui.widget as widget
use ui.widget.{ Button }
use ui.widget.{ Button as UiButton }

From src/app/pages/home.vita:

use helper
use ..layout
use ...ui.widget
use ...ui.widget.{ Button }
use ...core.result.{ Result }

---

Visibility summary

1. Top-level declarations are importable by default.
2. There is no `pub def`.
3. There is no `pub enum`.
4. There is no `pub spec`.
5. There is no `pub impl`.
6. Fields in `def` are accessible by default.
7. Normal impl methods are private by default.
8. Use `pub fn` inside `impl Type` to expose a method.
9. In `impl Type: Spec`, methods required by the spec inherit visibility from the spec.
10. Extra methods inside `impl Type: Spec` follow normal impl rules.
11. `use` imports relative to the current file location.
12. Module imports stay namespaced by default.
13. Symbol imports using `{ Name }` bring the symbol directly into scope.
14. Aliases can be used to avoid name collisions.
