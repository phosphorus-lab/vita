//! CLI entry point for the Vita compiler.

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use vita::backend::codegen::CodeGen;
use vita::semantics::checker::TypeChecker;
use vita::syntax::lexer::Lexer;
use vita::syntax::parser::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Vita Compiler v0.0.1");
        eprintln!();
        eprintln!("USAGE:");
        eprintln!("  vita <source.vita>                    Compile to LLVM IR (.ll)");
        eprintln!("  vita <source.vita> -o <output>        Compile to a native executable");
        eprintln!("  vita <source.vita> --run              Compile and run executable");
        eprintln!("  vita <source.vita> --run -o <output>  Compile, save executable, and run it");
        eprintln!();
        eprintln!("OPTIONS:");
        eprintln!("  --check           Type check only (no code generation)");
        eprintln!("  --ast             Dump the AST to stdout");
        eprintln!("  --emit-llvm       Emit LLVM IR text");
        eprintln!("  --emit-asm        Emit assembly via llc (if available)");
        eprintln!("  --emit-obj        Emit object file via llc (if available)");
        eprintln!("  --run             Compile and run via clang (if available)");
        eprintln!("  -o <path>         Output path; defaults to executable output if no emit mode is selected");
        eprintln!("  -h, --help        Show this help message");
        process::exit(1);
    }

    let mut source_path = String::new();
    let mut output_path = String::new();
    let mut check_only = false;
    let mut dump_ast = false;
    let mut emit_llvm = true;
    let mut emit_asm = false;
    let mut emit_obj = false;
    let mut emit_exe = false;
    let mut run = false;
    let mut explicit_emit_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => check_only = true,
            "--ast" => dump_ast = true,
            "--emit-llvm" => {
                explicit_emit_mode = true;
                emit_llvm = true;
                emit_asm = false;
                emit_obj = false;
                emit_exe = false;
                run = false;
            }
            "--emit-asm" => {
                explicit_emit_mode = true;
                emit_llvm = false;
                emit_asm = true;
                emit_obj = false;
                emit_exe = false;
                run = false;
            }
            "--emit-obj" => {
                explicit_emit_mode = true;
                emit_llvm = false;
                emit_asm = false;
                emit_obj = true;
                emit_exe = false;
                run = false;
            }
            "--run" => {
                explicit_emit_mode = true;
                emit_llvm = false;
                emit_asm = false;
                emit_obj = false;
                emit_exe = false;
                run = true;
            }
            "-o" => {
                i += 1;
                if i < args.len() {
                    output_path = args[i].clone();
                }
            }
            "-h" | "--help" => {
                eprintln!("Vita Compiler v0.0.1");
                eprintln!("A compiler for the Vita programming language.");
                eprintln!(
                    "Compiles Vita source to LLVM IR, assembly, object files, or executables."
                );
                process::exit(0);
            }
            _ => source_path = args[i].clone(),
        }
        i += 1;
    }

    if source_path.is_empty() {
        eprintln!("Error: no source file specified");
        process::exit(1);
    }

    // Make `vita input.vita -o output` behave like users expect from a compiler:
    // produce a native executable. Use `--emit-llvm -o output.ll` for LLVM IR.
    if !explicit_emit_mode && !output_path.is_empty() {
        emit_llvm = false;
        emit_exe = true;
    }

    let source = match fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", source_path, e);
            process::exit(1);
        }
    };

    // Phase 1: Lexing
    let tokens = match Lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(e) => {
            eprintln!("Lexer error: {}", e);
            process::exit(1);
        }
    };

    // Phase 2: Parsing
    let mut parser = Parser::new(tokens);
    let items = match parser.parse() {
        Ok(items) => items,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    if dump_ast {
        for item in &items {
            println!("{:#?}", item);
        }
        return;
    }

    // Phase 3: Type checking
    let mut checker = TypeChecker::new();
    let env = match checker.check(&items) {
        Ok(env) => env,
        Err(e) => {
            eprintln!("Type error: {}", e);
            process::exit(1);
        }
    };

    if check_only {
        println!("Type check passed.");
        return;
    }

    // Phase 4: Code generation
    let mut codegen = CodeGen::new(env);
    let llvm_ir = codegen.generate(&items);
    let requested_output = if output_path.is_empty() {
        None
    } else {
        Some(output_path)
    };

    if emit_llvm {
        let ll_path = requested_output.unwrap_or_else(|| with_extension(&source_path, "ll"));
        write_file_or_exit(&ll_path, &llvm_ir);
        println!("Compiled to LLVM IR: {}", ll_path);
        if !path_has_extension(&ll_path, "ll") {
            println!(
                "Note: this file contains LLVM IR text, not an executable. Use --run to build and run an executable."
            );
        }
        return;
    }

    // Find available tools for post-processing LLVM IR.
    let llc_path = find_tool(&["llc"]);
    let clang_path = find_tool(&["clang"]);
    let cc_path = find_tool(&["cc", "gcc"]);

    if emit_asm {
        let asm_path = requested_output
            .clone()
            .unwrap_or_else(|| with_extension(&source_path, "s"));
        let ll_path = with_extension(&asm_path, "ll");
        write_file_or_exit(&ll_path, &llvm_ir);
        let mut compiled = false;

        // Try llc first
        if let Some(ref llc) = llc_path {
            if run_command(llc, &["-o", &asm_path, &ll_path]).is_ok() {
                println!("Compiled to assembly: {}", asm_path);
                compiled = true;
            }
        }

        // Try clang as fallback
        if !compiled {
            if let Some(ref clang) = clang_path {
                if run_command(clang, &["-S", "-o", &asm_path, &ll_path]).is_ok() {
                    println!("Compiled to assembly: {}", asm_path);
                    compiled = true;
                }
            }
        }

        if !compiled {
            eprintln!(
                "Warning: could not compile to assembly. LLVM IR saved to: {}",
                ll_path
            );
            eprintln!("Install llc or clang to compile to assembly.");
        }
    } else if emit_obj {
        let obj_path = requested_output
            .clone()
            .unwrap_or_else(|| with_extension(&source_path, "o"));
        let ll_path = with_extension(&obj_path, "ll");
        write_file_or_exit(&ll_path, &llvm_ir);
        let mut compiled = false;

        // Try llc first
        if let Some(ref llc) = llc_path {
            if run_command(
                llc,
                &[
                    "-filetype=obj",
                    "-relocation-model=pic",
                    "-o",
                    &obj_path,
                    &ll_path,
                ],
            )
            .is_ok()
            {
                println!("Compiled to object file: {}", obj_path);
                compiled = true;
            }
        }

        // Try clang as fallback
        if !compiled {
            if let Some(ref clang) = clang_path {
                if run_command(clang, &["-c", "-o", &obj_path, &ll_path]).is_ok() {
                    println!("Compiled to object file: {}", obj_path);
                    compiled = true;
                }
            }
        }

        if !compiled {
            eprintln!(
                "Warning: could not compile to object file. LLVM IR saved to: {}",
                ll_path
            );
            eprintln!("Install llc or clang to compile to object files.");
        }
    } else if emit_exe || run {
        let exe_path = requested_output
            .clone()
            .unwrap_or_else(|| without_extension(&source_path));
        let ll_path = with_extension(&exe_path, "ll");
        let obj_path = with_extension(&exe_path, "o");
        write_file_or_exit(&ll_path, &llvm_ir);
        let mut compiled = false;

        // Strategy 1: clang directly (simplest)
        if let Some(ref clang) = clang_path {
            if run_command(clang, &["-o", &exe_path, &ll_path]).is_ok() {
                compiled = true;
            }
        }

        // Strategy 2: llc + cc
        if !compiled {
            if let Some(ref llc) = llc_path {
                if run_command(
                    llc,
                    &[
                        "-filetype=obj",
                        "-relocation-model=pic",
                        "-o",
                        &obj_path,
                        &ll_path,
                    ],
                )
                .is_ok()
                {
                    if let Some(ref cc) = cc_path {
                        if run_command(cc, &["-o", &exe_path, &obj_path]).is_ok() {
                            compiled = true;
                        }
                    }
                }
            }
        }

        if compiled {
            println!("Compiled to executable: {}", exe_path);
            if run {
                let run_path = executable_run_path(&exe_path);
                match run_command(&run_path, &[]) {
                    Ok(_) => {}
                    Err(e) => eprintln!("Runtime error: {}", e),
                }
            }
            // Clean up temp files
            let _ = fs::remove_file(&obj_path);
        } else {
            eprintln!(
                "Warning: could not compile executable. LLVM IR saved to: {}",
                ll_path
            );
            eprintln!("Install clang, or llc+cc to compile executable programs.");
        }
    }
}

fn write_file_or_exit(path: &str, contents: &str) {
    match fs::write(path, contents) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error writing '{}': {}", path, e);
            process::exit(1);
        }
    }
}

fn with_extension(path: &str, extension: &str) -> String {
    let mut path = Path::new(path).to_path_buf();
    path.set_extension(extension);
    path.to_string_lossy().into_owned()
}

fn without_extension(path: &str) -> String {
    let mut path = Path::new(path).to_path_buf();
    path.set_extension("");
    path.to_string_lossy().into_owned()
}

fn path_has_extension(path: &str, extension: &str) -> bool {
    Path::new(path).extension().and_then(|ext| ext.to_str()) == Some(extension)
}

fn executable_run_path(path: &str) -> String {
    if Path::new(path).is_absolute() || path.contains('/') {
        path.to_string()
    } else {
        format!("./{}", path)
    }
}

/// Find the first available tool from a list of candidates.
fn find_tool(candidates: &[&str]) -> Option<String> {
    for tool in candidates {
        // Check PATH
        if let Ok(output) = process::Command::new("which").arg(tool).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
        // Also check Rust toolchain's llc
        if *tool == "llc" {
            if let Ok(home) = std::env::var("HOME") {
                let rust_llc = format!("{}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llc", home);
                if std::path::Path::new(&rust_llc).exists() {
                    return Some(rust_llc);
                }
            }
        }
    }
    None
}

fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", program, e))?;

    // Forward stdout and stderr from the child process
    if !output.stdout.is_empty() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{}", stdout);
    }
    if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{}", stderr);
    }

    if !output.status.success() {
        return Err(format!(
            "{} exited with code {:?}",
            program,
            output.status.code()
        ));
    }

    Ok(())
}
