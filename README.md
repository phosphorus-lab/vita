# Vita

Vita is an experimental programming language and compiler written in Rust. The compiler currently parses Vita source code, performs semantic/type checking, emits LLVM IR, and can compile Vita programs into native executables through `clang` or `llc` plus a C compiler.

> Status: early-stage compiler. The language and compiler internals are still evolving.

## Features

The current compiler includes support for:

- Lexing and recursive-descent parsing
- AST generation
- Type checking and semantic analysis
- LLVM IR code generation
- Primitive numeric, boolean, character, and string types
- Functions and methods
- `def` data types / struct-like records
- `enum` types with variants
- `spec` and `impl` declarations
- Control flow with `if`, `else if`, `else`, and `match`
- Loop constructs including `loop`, `while`, and `for`-style iteration
- Arrays, tuples, maps, sets, and literals
- Range literals such as `[0..n]` for end-exclusive iteration
- Repeat literals such as `[0; n]` for `n` copies of a value
- Standard type constructors such as `Array<T>`, `Map<K, V>`, `Set<T>`, `Option<T>`, and `Result<T, E>`
- Basic primitive methods such as `str.len()`, `str.is_empty()`, and signed integer `.abs()`
- Basic builtin output via `print`

## Requirements

Required:

- Rust toolchain, edition 2021
- Cargo

Optional, for compiling LLVM IR into native artifacts such as executables, object files, or assembly:

- `clang`, or
- `llc` plus `cc`/`gcc`

You can still run lexer/parser/type-checking and emit LLVM IR without the optional native toolchain. Native executable output requires one of the optional toolchains above.

## Project layout

```text
src/
├── backend/
│   ├── codegen.rs        # LLVM IR generation
│   └── mod.rs
├── diagnostics/
│   ├── error.rs          # compiler errors and spans
│   └── mod.rs
├── modules.rs            # source loading and local use/import resolution
├── semantics/
│   ├── checker.rs        # type checker / semantic analysis
│   ├── std.rs            # standard types, builtin functions, and primitive methods
│   ├── types.rs          # type system and type environment
│   └── mod.rs
├── syntax/
│   ├── ast.rs            # AST definitions
│   ├── lexer.rs          # lexer
│   ├── parser.rs         # parser
│   ├── token.rs          # token definitions
│   └── mod.rs
├── lib.rs                # library module exports
└── main.rs               # CLI entry point
```

Other directories:

```text
examples/                 # example Vita programs and generated outputs
test/                     # test Vita source and local test artifacts
```

## Building

```sh
cargo build
```

For an optimized compiler binary:

```sh
cargo build --release
```

## Usage

The compiler command is:

```sh
vita <source.vita> [options]
```

During development, you can run it through Cargo:

```sh
cargo run -- <source.vita> [options]
```

Local `use` imports are loaded recursively from sibling `.vita` files or
`mod.vita` files. For example, `use helper` checks for `helper.vita` and
`helper/mod.vita` beside the importing file.

## Output mode quick reference

| Command | Output |
| --- | --- |
| `vita app.vita` | LLVM IR at `app.ll` |
| `vita app.vita -o app` | Native executable at `app` |
| `vita app.vita --run` | Native executable, then run it immediately |
| `vita app.vita --run -o app` | Native executable at `app`, then run it immediately |
| `vita app.vita --emit-llvm -o app.ll` | LLVM IR at `app.ll` |
| `vita app.vita --emit-asm -o app.s` | Assembly at `app.s` |
| `vita app.vita --emit-obj -o app.o` | Object file at `app.o` |

### Type check only

```sh
cargo run -- examples/hello.vita --check
```

Expected output:

```text
Type check passed.
```

### Dump the AST

```sh
cargo run -- examples/hello.vita --ast
```

### Emit LLVM IR

LLVM IR is the default output mode when no explicit output path is provided.

```sh
cargo run -- examples/hello.vita
```

This writes:

```text
examples/hello.ll
```

To write LLVM IR to a specific path, use `--emit-llvm` explicitly:

```sh
cargo run -- examples/hello.vita --emit-llvm -o build/hello.ll
```

### Compile an executable

When you provide `-o <output>` without selecting another emit mode, Vita compiles a native executable:

```sh
cargo run -- test/test-vita.vita -o test/test
```

This produces an executable at `test/test`, using `test/test.ll` as the intermediate LLVM IR file. You do not need to run `chmod +x` manually when Vita successfully compiles a native executable. If native compilation fails because `clang`, `llc`, or `cc` is unavailable, the intermediate `.ll` file is kept so you can inspect or compile it manually.

You can run the compiled binary yourself:

```sh
./test/test
```

### Compile and run

Use `--run` to compile the generated LLVM IR into a native executable and run it immediately:

```sh
cargo run -- examples/hello.vita --run
```

If you do not pass `-o`, the executable path is derived from the source file path. For example, `examples/hello.vita` compiles to `examples/hello`.

With an explicit executable output path:

```sh
cargo run -- test/test-vita.vita --run -o test/test
```

### Emit assembly

```sh
cargo run -- examples/hello.vita --emit-asm -o build/hello.s
```

### Emit object file

```sh
cargo run -- examples/hello.vita --emit-obj -o build/hello.o
```

## CLI options

| Option | Description |
| --- | --- |
| `--check` | Type check only; do not generate LLVM IR |
| `--ast` | Print the parsed AST |
| `--emit-llvm` | Emit LLVM IR text |
| `--emit-asm` | Emit assembly using `llc` or `clang` |
| `--emit-obj` | Emit an object file using `llc` or `clang` |
| `--run` | Compile to a native executable and run it |
| `-o <path>` | Output path. If no emit mode is selected, this produces a native executable instead of LLVM IR |
| `-h`, `--help` | Show help |

## Example Vita program

```vita
fn main() -> i32 {
    let text = "Hello";
    *? let i: [0..text.len()] {
        print(text[i]);
    };
    0
}
```

Run it with:

```sh
cargo run -- examples/hello.vita --run
```

## Development commands

Format the code:

```sh
cargo fmt
```

Check formatting:

```sh
cargo fmt --check
```

Check that the crate builds:

```sh
cargo check
```

Run tests:

```sh
cargo test
```

Run Clippy with warnings denied:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```

## Current compiler pipeline

```text
Vita source
    ↓
Lexer
    ↓
Parser
    ↓
AST
    ↓
Type checker / semantic analyzer
    ↓
LLVM IR code generator
    ↓
Optional native compilation via clang or llc + cc
```

## Notes

- LLVM IR output files are text files and are not directly executable.
- `vita <source.vita>` keeps the compiler lightweight by emitting LLVM IR only.
- `vita <source.vita> -o <output>` produces a native executable without running it.
- `--run` produces a native executable and immediately runs it.
- `--emit-llvm -o <output.ll>` is the explicit way to choose a custom LLVM IR output path.
- The compiler currently depends only on the Rust standard library.
- The generated LLVM IR is intended to be consumed by modern LLVM-compatible tools such as `clang`.

## License

No license has been specified yet.
