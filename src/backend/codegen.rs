//! LLVM IR code generator for the Vita language.
//!
//! Generates valid SSA-form LLVM IR text output from a checked AST.
//! The output can be compiled to object files using `llc` or `clang`.
//!
//! Design:
//! - Variables use alloca/load/store (mem2reg pass will promote to SSA)
//! - Each function has proper basic blocks with terminators
//! - String constants are emitted as global `[N x i8]` arrays
//! - Struct types are emitted as named LLVM struct types
//! - Enums use tagged union representation: { i32 tag, [max_payload x i8] }

use crate::backend::os;
use crate::semantics::std as vita_std;
use crate::semantics::types::{Type, TypeEnv, TypeInfo};
use crate::syntax::ast::*;
use crate::syntax::ast::{BinOp, UnOp};

/// An LLVM IR value reference (e.g., `%x`, `@global`, `42`).
#[derive(Debug, Clone)]
enum LlvmVal {
    /// A local register reference like `%name`.
    Local(String),
    /// A global reference like `@name`.
    #[allow(dead_code)]
    Global(String),
    /// A constant value embedded inline (integers, floats).
    Const(String),
}

impl LlvmVal {
    fn to_str(&self) -> String {
        match self {
            LlvmVal::Local(s) => format!("%{}", s),
            LlvmVal::Global(s) => format!("@{}", s),
            LlvmVal::Const(s) => s.clone(),
        }
    }
}

/// LLVM IR code generator producing valid SSA-form IR text.
pub struct CodeGen {
    env: TypeEnv,
    output: String,

    // Counters for unique names
    counter: usize,
    label_counter: usize,

    // Current function context
    current_fn: String,
    ret_type: Type,
    ret_label: String, // label for return block

    // String constants to emit as globals
    string_constants: Vec<(String, String, usize)>, // (name, value, len including null)

    // Struct type definitions emitted so far
    emitted_types: Vec<String>,

    // Variable allocas in current function
    var_allocas: Vec<String>, // alloca instructions to emit at entry

    // Whether we've emitted a terminator in the current block
    terminated: bool,

    // Loop context for break/continue
    loop_end_labels: Vec<String>,
    loop_continue_labels: Vec<String>,
}

impl CodeGen {
    pub fn new(env: TypeEnv) -> Self {
        CodeGen {
            env,
            output: String::new(),
            counter: 0,
            label_counter: 0,
            current_fn: String::new(),
            ret_type: Type::Unit,
            ret_label: String::new(),
            string_constants: Vec::new(),
            emitted_types: Vec::new(),
            var_allocas: Vec::new(),
            terminated: false,
            loop_end_labels: Vec::new(),
            loop_continue_labels: Vec::new(),
        }
    }

    /// Generate LLVM IR for the given items.
    pub fn generate(&mut self, items: &[Item]) -> String {
        self.emit_header();

        // First pass: emit all type definitions
        for item in items {
            self.emit_type_decl(item);
        }

        // Second pass: emit global bindings
        for item in items {
            self.emit_global(item);
        }

        // Third pass: collect all functions and emit them
        for item in items {
            self.emit_item(item);
        }

        // Emit string constants as globals
        self.emit_string_constants();

        self.output.clone()
    }

    // =========================================================================
    // Name generation
    // =========================================================================

    fn fresh(&mut self, prefix: &str) -> String {
        let name = format!("{}_{}", prefix, self.counter);
        self.counter += 1;
        name
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let name = format!("{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        name
    }

    /// Convert a bare local variable name to an LLVM IR local reference (%name).
    fn l(&self, name: &str) -> String {
        format!("%{}", name)
    }

    // =========================================================================
    // Output helpers
    // =========================================================================

    fn emit(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn emit_indent(&mut self, s: &str) {
        self.output.push_str("  ");
        self.output.push_str(s);
        self.output.push('\n');
    }

    // =========================================================================
    // Module header and declarations
    // =========================================================================

    fn emit_header(&mut self) {
        self.emit("; Vita Compiler v0.0.1 - LLVM IR Output");
        self.emit("target triple = \"x86_64-pc-linux-gnu\"");
        self.emit("target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"");
        self.emit("");
        self.emit("; --- External declarations ---");
        self.emit("declare i32 @printf(ptr, ...)");
        self.emit("declare i32 @puts(ptr)");
        self.emit("declare i32 @sprintf(ptr, ptr, ...)");
        self.emit("declare ptr @fopen(ptr, ptr)");
        self.emit("declare i32 @fclose(ptr)");
        self.emit("declare i32 @fseek(ptr, i64, i32)");
        self.emit("declare i64 @ftell(ptr)");
        self.emit("declare i64 @fread(ptr, i64, i64, ptr)");
        self.emit("declare i64 @fwrite(ptr, i64, i64, ptr)");
        self.emit("declare ptr @strcpy(ptr, ptr)");
        self.emit("declare ptr @strcat(ptr, ptr)");
        self.emit("declare i32 @strcmp(ptr, ptr)");
        self.emit("declare i64 @strlen(ptr)");
        self.emit("declare ptr @malloc(i64)");
        self.emit("declare void @free(ptr)");
        self.emit("declare void @llvm.memcpy.p0.p0.i64(ptr noalias nocapture writeonly, ptr noalias nocapture readonly, i64, i1 immarg)");
        self.emit("declare i32 @__cxa_atexit(ptr, ptr, ptr)");
        self.emit("");
    }

    fn emit_string_constants(&mut self) {
        if self.string_constants.is_empty() {
            return;
        }
        self.emit("; --- String constants ---");
        let constants: Vec<(String, String, usize)> = self.string_constants.clone();
        for (name, value, len) in &constants {
            // Emit as global constant [len x i8]
            let escaped: String = value
                .chars()
                .flat_map(|c| {
                    // LLVM IR c"..." strings use \HH hex escape sequences
                    let byte = c as u8;
                    match c {
                        '\\' => vec!['\\', '\\'],
                        '"' => vec!['\\', '"'],
                        '\n' => vec!['\\', '0', 'A'], // 0x0A = newline
                        '\t' => vec!['\\', '0', '9'], // 0x09 = tab
                        '\r' => vec!['\\', '0', 'D'], // 0x0D = carriage return
                        '\0' => vec!['\\', '0', '0'], // 0x00 = null
                        c if c.is_ascii_graphic() || c == ' ' => vec![c],
                        _ => {
                            // Use hex escape for all other characters
                            let hex = format!("\\{:02X}", byte);
                            hex.chars().collect()
                        }
                    }
                })
                .collect();
            self.emit(&format!(
                "@{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                name, len, escaped
            ));
        }
        self.emit("");
    }

    // =========================================================================
    // Type definitions
    // =========================================================================

    fn emit_type_decl(&mut self, item: &Item) {
        match item {
            Item::Def(def_item) => {
                if self.emitted_types.contains(&def_item.name) {
                    return;
                }
                self.emitted_types.push(def_item.name.clone());

                let field_types: Vec<String> = def_item
                    .fields
                    .iter()
                    .map(|f| self.type_to_llvm(&self.resolve_field_type(f)))
                    .collect();

                self.emit(&format!("; Type: {} (def/struct)", def_item.name));
                self.emit(&format!(
                    "%{}_struct = type {{ {} }}",
                    def_item.name,
                    field_types.join(", ")
                ));

                // If generic, note it
                if !def_item.generics.is_empty() {
                    self.emit(&format!(";   generics: {}", def_item.generics.join(", ")));
                }
                self.emit("");
            }
            Item::Enum(enum_item) => {
                if self.emitted_types.contains(&enum_item.name) {
                    return;
                }
                self.emitted_types.push(enum_item.name.clone());

                let max_payload = self.enum_max_payload_size(enum_item);
                self.emit(&format!("; Type: {} (enum/tagged union)", enum_item.name));
                self.emit(&format!(
                    "%{}_enum = type {{ i32, [{} x i8] }}",
                    enum_item.name, max_payload
                ));

                // Emit variant tag constants
                for (i, variant) in enum_item.variants.iter().enumerate() {
                    self.emit(&format!(
                        ";   {}::{} = tag {}",
                        enum_item.name, variant.name, i
                    ));
                }

                if !enum_item.generics.is_empty() {
                    self.emit(&format!(";   generics: {}", enum_item.generics.join(", ")));
                }
                self.emit("");
            }
            _ => {}
        }
    }

    fn enum_max_payload_size(&self, enum_item: &EnumItem) -> usize {
        let mut max: usize = 8; // minimum for alignment
        for variant in &enum_item.variants {
            if let Some(payload) = &variant.payload {
                let size = self.type_size(&self.resolve_type_expr_simple(payload));
                max = max.max(size as usize);
            }
        }
        // Round up to 8-byte alignment
        max.div_ceil(8) * 8
    }

    // =========================================================================
    // Function / item emission
    // =========================================================================

    fn emit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fn_item) => {
                self.emit_function(fn_item);
            }
            Item::Impl(impl_item) => {
                for method in &impl_item.methods {
                    self.emit_method(method, &impl_item.target_type);
                }
            }
            _ => {}
        }
    }

    fn emit_global(&mut self, item: &Item) {
        let Item::Global(global) = item else {
            return;
        };

        let ty = global
            .type_ann
            .as_ref()
            .map(|t| self.resolve_type_expr_simple(t))
            .unwrap_or_else(|| self.infer_type_of_expr(&global.value));
        let llvm_type = self.type_to_llvm(&ty);
        let linkage = if global.is_const { "constant" } else { "global" };
        let value = self.global_initializer(&global.value, &ty);
        self.emit(&format!(
            "@{} = {} {} {}",
            global.name, linkage, llvm_type, value
        ));
    }

    fn global_initializer(&mut self, expr: &Expr, ty: &Type) -> String {
        match ty {
            Type::Bool => self
                .eval_const_bool(expr)
                .map(|value| {
                    if value {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                })
                .unwrap_or_else(|| self.zero_initializer(ty)),
            Type::F16 | Type::F32 | Type::F64 | Type::F128 => self
                .eval_const_float(expr)
                .map(|value| value.to_string())
                .unwrap_or_else(|| self.zero_initializer(ty)),
            Type::Char => self
                .eval_const_int(expr)
                .map(|value| value.to_string())
                .unwrap_or_else(|| self.zero_initializer(ty)),
            Type::Str => match expr {
                Expr::String(value) => {
                    let const_name = self.fresh("str");
                    let len = value.len() + 1;
                    self.string_constants
                        .push((const_name.clone(), value.clone(), len));
                    format!("@{}", const_name)
                }
                _ => self.zero_initializer(ty),
            },
            _ => self
                .eval_const_int(expr)
                .map(|value| value.to_string())
                .unwrap_or_else(|| self.zero_initializer(ty)),
        }
    }

    fn eval_const_int(&self, expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Int(value) => Some(*value),
            Expr::Char(value) => Some(*value as i64),
            Expr::Grouped(inner) => self.eval_const_int(inner),
            Expr::Unary { op, operand } => match op {
                UnOp::Neg => self.eval_const_int(operand).map(|value| -value),
                UnOp::Not => None,
            },
            Expr::Binary { op, left, right } => {
                let l = self.eval_const_int(left)?;
                let r = self.eval_const_int(right)?;
                match op {
                    BinOp::Add => Some(l + r),
                    BinOp::Sub => Some(l - r),
                    BinOp::Mul => Some(l * r),
                    BinOp::Div if r != 0 => Some(l / r),
                    BinOp::Mod if r != 0 => Some(l % r),
                    BinOp::BitAnd => Some(l & r),
                    BinOp::BitOr => Some(l | r),
                    BinOp::BitXor => Some(l ^ r),
                    BinOp::Shl if r >= 0 => Some(l << r),
                    BinOp::Shr if r >= 0 => Some(l >> r),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn eval_const_float(&self, expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Float(value) => Some(*value),
            Expr::Int(value) => Some(*value as f64),
            Expr::Grouped(inner) => self.eval_const_float(inner),
            Expr::Unary { op, operand } => match op {
                UnOp::Neg => self.eval_const_float(operand).map(|value| -value),
                UnOp::Not => None,
            },
            Expr::Binary { op, left, right } => {
                let l = self.eval_const_float(left)?;
                let r = self.eval_const_float(right)?;
                match op {
                    BinOp::Add => Some(l + r),
                    BinOp::Sub => Some(l - r),
                    BinOp::Mul => Some(l * r),
                    BinOp::Div => Some(l / r),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn eval_const_bool(&self, expr: &Expr) -> Option<bool> {
        match expr {
            Expr::Bool(value) => Some(*value),
            Expr::Grouped(inner) => self.eval_const_bool(inner),
            Expr::Unary { op, operand } => match op {
                UnOp::Not => self.eval_const_bool(operand).map(|value| !value),
                UnOp::Neg => None,
            },
            Expr::Binary { op, left, right } => match op {
                BinOp::And => Some(self.eval_const_bool(left)? && self.eval_const_bool(right)?),
                BinOp::Or => Some(self.eval_const_bool(left)? || self.eval_const_bool(right)?),
                BinOp::Eq => {
                    if let (Some(l), Some(r)) =
                        (self.eval_const_int(left), self.eval_const_int(right))
                    {
                        Some(l == r)
                    } else {
                        Some(self.eval_const_bool(left)? == self.eval_const_bool(right)?)
                    }
                }
                BinOp::Neq => {
                    if let (Some(l), Some(r)) =
                        (self.eval_const_int(left), self.eval_const_int(right))
                    {
                        Some(l != r)
                    } else {
                        Some(self.eval_const_bool(left)? != self.eval_const_bool(right)?)
                    }
                }
                BinOp::Lt => Some(self.eval_const_int(left)? < self.eval_const_int(right)?),
                BinOp::Gt => Some(self.eval_const_int(left)? > self.eval_const_int(right)?),
                BinOp::LtEq => Some(self.eval_const_int(left)? <= self.eval_const_int(right)?),
                BinOp::GtEq => Some(self.eval_const_int(left)? >= self.eval_const_int(right)?),
                _ => None,
            },
            _ => None,
        }
    }

    fn zero_initializer(&self, ty: &Type) -> String {
        match ty {
            Type::Bool => "false".to_string(),
            Type::F16 | Type::F32 | Type::F64 | Type::F128 => "0.0".to_string(),
            Type::Str | Type::DynArray(_) | Type::Map { .. } | Type::Set(_) | Type::Fn { .. } => {
                "null".to_string()
            }
            _ => "0".to_string(),
        }
    }

    fn emit_function(&mut self, fn_item: &FnItem) {
        self.current_fn = fn_item.name.clone();
        let is_main = fn_item.name == "main";
        self.ret_type = fn_item
            .return_type
            .as_ref()
            .map(|t| self.resolve_type_expr_simple(t))
            .unwrap_or(if is_main { Type::I32 } else { Type::Unit });
        self.var_allocas.clear();
        self.terminated = false;
        self.label_counter = 0;

        // Re-register this function's parameters in the type env so that
        // lookup_var returns the correct types (the global env may have
        // overwritten them with other functions' identically-named params).
        for p in &fn_item.params {
            let pt = self.resolve_type_expr_simple(&p.type_ann);
            self.env.define_var(p.name.clone(), pt);
        }

        let ret_llvm = self.type_to_llvm(&self.ret_type);
        let ret_label = self.fresh_label("ret");

        // Build parameter list
        let params: Vec<String> = fn_item
            .params
            .iter()
            .map(|p| {
                let pt = self.resolve_type_expr_simple(&p.type_ann);
                format!("{} %{}", self.type_to_llvm(&pt), p.name)
            })
            .collect();

        self.emit(&format!(
            "define {} @{}({}) {{",
            ret_llvm,
            fn_item.name,
            params.join(", ")
        ));

        // Store function parameters to allocas
        for p in &fn_item.params {
            let pt = self.resolve_type_expr_simple(&p.type_ann);
            let alloca_name = format!("{}_addr", p.name);
            self.emit_indent(&format!(
                "{} = alloca {}",
                self.l(&alloca_name),
                self.type_to_llvm(&pt)
            ));
            self.emit_indent(&format!(
                "store {} %{}, ptr {}",
                self.type_to_llvm(&pt),
                p.name,
                self.l(&alloca_name)
            ));
            self.var_allocas.push(p.name.clone());
        }

        // Return value alloca (for non-void functions)
        if self.ret_type != Type::Unit {
            self.ret_label = ret_label.clone();
            self.emit_indent(&format!("%_retval = alloca {}", ret_llvm));
            // For main(), initialize return value to 0
            if is_main {
                self.emit_indent("store i32 0, ptr %_retval");
            }
        } else {
            self.ret_label = ret_label.clone();
        }

        // Emit body
        self.emit_block_as_return(&fn_item.body);

        // If not terminated, add implicit return
        if !self.terminated {
            if self.ret_type == Type::Unit {
                self.emit_indent("ret void");
            } else {
                self.emit_indent(&format!("br label %{}", self.ret_label));
            }
        }

        // Emit return block for non-void functions
        if self.ret_type != Type::Unit {
            self.emit(&format!("{}:", self.ret_label));
            self.emit_indent(&format!("%_ret_load = load {}, ptr %_retval", ret_llvm));
            self.emit_indent(&format!("ret {} %_ret_load", ret_llvm));
        } else {
            // For void, we need a return block too if branches jump to it
            self.emit(&format!("{}:", self.ret_label));
            if !self.terminated {
                self.emit_indent("ret void");
            }
        }

        self.emit("}");
        self.emit("");
    }

    fn emit_method(&mut self, fn_item: &FnItem, target_type: &str) {
        // Methods are emitted as functions with mangled names: TypeName_methodName
        let mangled_name = format!("{}_{}", target_type, fn_item.name);

        // Create a modified FnItem with the mangled name
        let mut method = fn_item.clone();
        method.name = mangled_name;

        // Ensure the self parameter type is correct
        for p in &mut method.params {
            if p.name == "self" {
                p.type_ann = TypeExpr::Named(target_type.to_string());
            }
        }

        self.emit_function(&method);
    }

    // =========================================================================
    // Block and statement emission
    // =========================================================================

    /// Emit a block's statements. Returns the tail expression value if present.
    fn emit_block(&mut self, block: &Block) -> Option<LlvmVal> {
        for stmt in &block.stmts {
            if self.terminated {
                break;
            }
            self.emit_stmt(stmt);
        }
        if !self.terminated {
            if let Some(tail) = &block.tail {
                let val = self.emit_expr(tail);
                return Some(val);
            }
        }
        None
    }

    /// Emit a block and store the tail value as the function's return value.
    fn emit_block_as_return(&mut self, block: &Block) {
        let tail_val = self.emit_block(block);
        if let Some(val) = tail_val {
            if !self.terminated {
                let expr_type = self.infer_type_of_expr(block.tail.as_ref().unwrap());
                if self.ret_type != Type::Unit && expr_type != Type::Unit {
                    self.emit_indent(&format!(
                        "store {} {}, ptr %_retval",
                        self.type_to_llvm(&expr_type),
                        val.to_str()
                    ));
                    self.emit_indent(&format!("br label %{}", self.ret_label));
                    self.terminated = true;
                }
            }
        }
    }

    /// Emit a block and store the tail value to a given pointer.
    fn emit_block_to_ptr(&mut self, block: &Block, ptr: &str, result_type: &Type) {
        let tail_val = self.emit_block(block);
        if let Some(val) = tail_val {
            if !self.terminated {
                self.emit_indent(&format!(
                    "store {} {}, ptr {}",
                    self.type_to_llvm(result_type),
                    val.to_str(),
                    self.l(ptr)
                ));
            }
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        if self.terminated {
            return;
        }
        match stmt {
            Stmt::Let {
                name,
                type_ann,
                value,
                ..
            } => {
                let ty = type_ann
                    .as_ref()
                    .map(|t| self.resolve_type_expr_simple(t))
                    .unwrap_or_else(|| self.infer_type_of_expr(value));
                let val = match (&ty, value) {
                    (Type::DynArray(elem), Expr::ArrayLiteral(elements)) => {
                        self.emit_dyn_array_literal(elem, elements)
                    }
                    (Type::DynArray(elem), Expr::RepeatLiteral { value, count }) => {
                        self.emit_dyn_array_repeat(elem, value, count)
                    }
                    _ => self.emit_expr(value),
                };

                // Register the variable type so subsequent lookups are correct
                self.env.define_var(name.clone(), ty.clone());

                let alloca_name = format!("{}_addr", name);
                let llvm_type = self.type_to_llvm(&ty);
                self.emit_indent(&format!("{} = alloca {}", self.l(&alloca_name), llvm_type));
                self.emit_indent(&format!(
                    "store {} {}, ptr {}",
                    llvm_type,
                    val.to_str(),
                    self.l(&alloca_name)
                ));
                self.var_allocas.push(name.clone());
            }
            Stmt::Expr(expr) => {
                self.emit_expr(expr);
            }
            Stmt::SemiExpr(expr) => {
                self.emit_expr(expr);
            }
            Stmt::Break => {
                if let Some(end_label) = self.loop_end_labels.last() {
                    self.emit_indent(&format!("br label %{}", end_label));
                    self.terminated = true;
                }
            }
            Stmt::Continue => {
                if let Some(cont_label) = self.loop_continue_labels.last() {
                    self.emit_indent(&format!("br label %{}", cont_label));
                    self.terminated = true;
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    let val = self.emit_expr(e);
                    let ty = self.infer_type_of_expr(e);
                    if self.ret_type != Type::Unit {
                        self.emit_indent(&format!(
                            "store {} {}, ptr %_retval",
                            self.type_to_llvm(&ty),
                            val.to_str()
                        ));
                        self.emit_indent(&format!("br label %{}", self.ret_label));
                    } else {
                        self.emit_indent("ret void");
                    }
                } else {
                    self.emit_indent("ret void");
                }
                self.terminated = true;
            }
        }
    }

    // =========================================================================
    // Expression emission
    // =========================================================================

    fn emit_expr(&mut self, expr: &Expr) -> LlvmVal {
        if self.terminated {
            return LlvmVal::Const("poison".to_string());
        }

        match expr {
            // --- Literals ---
            Expr::Int(val) => LlvmVal::Const(val.to_string()),
            Expr::Float(val) => {
                let s = format!("{}", val);
                // Ensure float constants have a dot
                let s = if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{}.0", s)
                };
                LlvmVal::Const(s)
            }
            Expr::Bool(true) => LlvmVal::Const("true".to_string()),
            Expr::Bool(false) => LlvmVal::Const("false".to_string()),
            Expr::String(s) => {
                let const_name = self.fresh("str");
                let len = s.len() + 1; // +1 for null terminator
                self.string_constants
                    .push((const_name.clone(), s.clone(), len));
                // Return a pointer to the string constant (getelementptr to decay to ptr)
                let ptr_name = self.fresh("str_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [{} x i8], ptr @{}, i64 0, i64 0",
                    self.l(&ptr_name),
                    len,
                    const_name
                ));
                LlvmVal::Local(ptr_name)
            }
            Expr::Char(c) => LlvmVal::Const((*c as u32).to_string()),
            Expr::Unit => LlvmVal::Const("void".to_string()),

            // --- Variable reference ---
            Expr::Ident(name) => {
                let var_type = self.env.lookup_var(name).unwrap_or(Type::I32);
                let llvm_type = self.type_to_llvm(&var_type);
                let load_name = self.fresh(name);
                let ptr = if self.var_allocas.iter().any(|local| local == name) {
                    self.l(&format!("{}_addr", name))
                } else {
                    format!("@{}", name)
                };
                self.emit_indent(&format!(
                    "{} = load {}, ptr {}",
                    self.l(&load_name),
                    llvm_type,
                    ptr
                ));
                LlvmVal::Local(load_name)
            }

            // --- Binary operations ---
            Expr::Binary { op, left, right } => self.emit_binary(op, left, right),

            // --- Unary operations ---
            Expr::Unary { op, operand } => self.emit_unary(op, operand),

            // --- Assignment ---
            Expr::Assign { target, value } => {
                let val = self.emit_expr(value);
                let val_type = self.infer_type_of_expr(value);
                let llvm_type = self.type_to_llvm(&val_type);

                if let Expr::Ident(name) = target.as_ref() {
                    let ptr = if self.var_allocas.iter().any(|local| local == name) {
                        self.l(&format!("{}_addr", name))
                    } else {
                        format!("@{}", name)
                    };
                    self.emit_indent(&format!(
                        "store {} {}, ptr {}",
                        llvm_type,
                        val.to_str(),
                        ptr
                    ));
                }
                val
            }

            // --- Function call ---
            Expr::Call { func, args } => self.emit_call(func, args),

            // --- Method call ---
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.emit_method_call(receiver, method, args),

            // --- Field access ---
            Expr::FieldAccess { object, field } => self.emit_field_access(object, field),

            // --- Tuple access ---
            Expr::TupleAccess { tuple, index } => {
                let val = self.emit_expr(tuple);
                let tuple_type = self.infer_type_of_expr(tuple);
                let ptr_name = self.fresh("tidx");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                    self.l(&ptr_name),
                    self.type_to_llvm(&tuple_type),
                    val.to_str(),
                    index
                ));
                let elem_type = match &tuple_type {
                    Type::Tuple(types) => types.get(*index).cloned().unwrap_or(Type::I32),
                    _ => Type::I32,
                };
                let load_name = self.fresh("tload");
                self.emit_indent(&format!(
                    "{} = load {}, ptr {}",
                    self.l(&load_name),
                    self.type_to_llvm(&elem_type),
                    self.l(&ptr_name)
                ));
                LlvmVal::Local(load_name)
            }

            // --- Index access ---
            Expr::Index { object, index } => {
                let obj_val = self.emit_expr(object);
                let idx_val = self.emit_expr(index);
                let obj_type = self.infer_type_of_expr(object);

                if obj_type == Type::Str {
                    let byte_ptr = self.fresh("str_idx");
                    self.emit_indent(&format!(
                        "{} = getelementptr inbounds i8, ptr {}, {} {}",
                        self.l(&byte_ptr),
                        obj_val.to_str(),
                        self.type_to_llvm(&self.infer_type_of_expr(index)),
                        idx_val.to_str()
                    ));
                    let byte_load = self.fresh("str_byte");
                    self.emit_indent(&format!(
                        "{} = load i8, ptr {}",
                        self.l(&byte_load),
                        self.l(&byte_ptr)
                    ));
                    let char_val = self.fresh("str_char");
                    self.emit_indent(&format!(
                        "{} = zext i8 {} to i32",
                        self.l(&char_val),
                        self.l(&byte_load)
                    ));
                    return LlvmVal::Local(char_val);
                }

                let elem_type = match &obj_type {
                    Type::Array { element, .. } => *element.clone(),
                    Type::DynArray(elem) => *elem.clone(),
                    _ => Type::I32,
                };
                let elem_llvm = self.type_to_llvm(&elem_type);
                let ptr_name = self.fresh("idx");
                if let Type::Array { size, .. } = obj_type {
                    self.emit_indent(&format!(
                        "{} = getelementptr inbounds [{} x {}], ptr {}, i32 0, {} {}",
                        self.l(&ptr_name),
                        size,
                        elem_llvm,
                        obj_val.to_str(),
                        self.type_to_llvm(&self.infer_type_of_expr(index)),
                        idx_val.to_str()
                    ));
                } else if matches!(obj_type, Type::DynArray(_)) {
                    let data_ptr = self.emit_dyn_array_data_ptr(&obj_val);
                    self.emit_indent(&format!(
                        "{} = getelementptr inbounds {}, ptr {}, {} {}",
                        self.l(&ptr_name),
                        elem_llvm,
                        data_ptr,
                        self.type_to_llvm(&self.infer_type_of_expr(index)),
                        idx_val.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = getelementptr inbounds {}, ptr {}, {} {}",
                        self.l(&ptr_name),
                        elem_llvm,
                        obj_val.to_str(),
                        self.type_to_llvm(&self.infer_type_of_expr(index)),
                        idx_val.to_str()
                    ));
                }
                let load_name = self.fresh("iload");
                self.emit_indent(&format!(
                    "{} = load {}, ptr {}",
                    self.l(&load_name),
                    elem_llvm,
                    self.l(&ptr_name)
                ));
                LlvmVal::Local(load_name)
            }

            // --- If expression ---
            Expr::If {
                condition,
                then_block,
                else_if_clauses,
                else_block,
            } => self.emit_if(condition, then_block, else_if_clauses, else_block),

            // --- Match expression ---
            Expr::Match { subject, arms } => self.emit_match(subject, arms),

            // --- Infinite loop ---
            Expr::Loop(body) => self.emit_loop(body),

            // --- While loop ---
            Expr::While { condition, body } => self.emit_while(condition, body),

            // --- For-each loop ---
            Expr::ForEach {
                var,
                type_ann,
                iterable,
                body,
            } => self.emit_for_each(var, type_ann.as_ref(), iterable, body),

            // --- Fallible block ---
            Expr::Fallible { block, handler } => self.emit_fallible(block, handler),

            // --- Struct literal ---
            Expr::StructLiteral { type_name, fields } => {
                self.emit_struct_literal(type_name, fields)
            }

            // --- Enum variant ---
            Expr::EnumVariant {
                type_name,
                variant,
                value,
            } => self.emit_enum_variant(type_name, variant, value),

            // --- Array literal ---
            Expr::ArrayLiteral(elements) => self.emit_array_literal(elements),

            // --- Range/repeat literals ---
            Expr::RangeLiteral { .. } => LlvmVal::Const("null".to_string()),
            Expr::RepeatLiteral { value, count } => self.emit_repeat_literal(value, count),

            // --- Tuple literal ---
            Expr::TupleLiteral(elements) => self.emit_tuple_literal(elements),

            // --- Map literal ---
            Expr::MapLiteral(_) => {
                // Maps require runtime support; emit as null for now
                LlvmVal::Const("null".to_string())
            }

            // --- Set literal ---
            Expr::SetLiteral(_) => LlvmVal::Const("null".to_string()),

            // --- Lambda ---
            Expr::Lambda { .. } => {
                // Lambdas require closure conversion; not yet implemented
                LlvmVal::Const("null".to_string())
            }

            // --- Grouped ---
            Expr::Grouped(expr) => self.emit_expr(expr),
        }
    }

    // =========================================================================
    // Binary operations
    // =========================================================================

    fn emit_binary(&mut self, op: &BinOp, left: &Expr, right: &Expr) -> LlvmVal {
        let lv = self.emit_expr(left);
        let rv = self.emit_expr(right);
        let lt = self.infer_type_of_expr(left);
        let rt = self.infer_type_of_expr(right);
        let result_name = self.fresh("binop");
        let llvm_type = self.type_to_llvm(&lt);

        // String concatenation: str + str -> str
        if *op == BinOp::Add && (lt == Type::Str || rt == Type::Str) {
            return self.emit_string_concat(&lv, &rv, &lt, &rt);
        }

        if (*op == BinOp::Eq || *op == BinOp::Neq) && lt == Type::Str && rt == Type::Str {
            let cmp_name = self.fresh("strcmp");
            self.emit_indent(&format!(
                "{} = call i32 @strcmp(ptr {}, ptr {})",
                self.l(&cmp_name),
                lv.to_str(),
                rv.to_str()
            ));
            let pred = if *op == BinOp::Eq { "eq" } else { "ne" };
            self.emit_indent(&format!(
                "{} = icmp {} i32 {}, 0",
                self.l(&result_name),
                pred,
                self.l(&cmp_name)
            ));
            return LlvmVal::Local(result_name);
        }

        let instr = match op {
            BinOp::Add => {
                if lt.is_float() {
                    "fadd"
                } else {
                    "add"
                }
            }
            BinOp::Sub => {
                if lt.is_float() {
                    "fsub"
                } else {
                    "sub"
                }
            }
            BinOp::Mul => {
                if lt.is_float() {
                    "fmul"
                } else {
                    "mul"
                }
            }
            BinOp::Div => {
                if lt.is_float() {
                    "fdiv"
                } else if lt.is_signed_int() {
                    "sdiv"
                } else {
                    "udiv"
                }
            }
            BinOp::Mod => {
                if lt.is_float() {
                    "frem"
                } else if lt.is_signed_int() {
                    "srem"
                } else {
                    "urem"
                }
            }
            BinOp::Eq => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp oeq {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                    return LlvmVal::Local(result_name);
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp eq {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                    return LlvmVal::Local(result_name);
                }
            }
            BinOp::Neq => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp one {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                    return LlvmVal::Local(result_name);
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp ne {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                    return LlvmVal::Local(result_name);
                }
            }
            BinOp::Lt => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp olt {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else if lt.is_signed_int() {
                    self.emit_indent(&format!(
                        "{} = icmp slt {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp ult {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                }
                return LlvmVal::Local(result_name);
            }
            BinOp::Gt => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp ogt {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else if lt.is_signed_int() {
                    self.emit_indent(&format!(
                        "{} = icmp sgt {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp ugt {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                }
                return LlvmVal::Local(result_name);
            }
            BinOp::LtEq => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp ole {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else if lt.is_signed_int() {
                    self.emit_indent(&format!(
                        "{} = icmp sle {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp ule {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                }
                return LlvmVal::Local(result_name);
            }
            BinOp::GtEq => {
                if lt.is_float() {
                    self.emit_indent(&format!(
                        "{} = fcmp oge {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else if lt.is_signed_int() {
                    self.emit_indent(&format!(
                        "{} = icmp sge {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = icmp uge {} {}, {}",
                        self.l(&result_name),
                        llvm_type,
                        lv.to_str(),
                        rv.to_str()
                    ));
                }
                return LlvmVal::Local(result_name);
            }
            BinOp::And => {
                // Short-circuit is handled at a higher level; here we do bitwise i1 and
                self.emit_indent(&format!(
                    "{} = and i1 {}, {}",
                    self.l(&result_name),
                    lv.to_str(),
                    rv.to_str()
                ));
                return LlvmVal::Local(result_name);
            }
            BinOp::Or => {
                self.emit_indent(&format!(
                    "{} = or i1 {}, {}",
                    self.l(&result_name),
                    lv.to_str(),
                    rv.to_str()
                ));
                return LlvmVal::Local(result_name);
            }
            BinOp::BitAnd => "and",
            BinOp::BitOr => "or",
            BinOp::BitXor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => {
                if lt.is_signed_int() {
                    "ashr"
                } else {
                    "lshr"
                }
            }
        };

        self.emit_indent(&format!(
            "{} = {} {} {}, {}",
            self.l(&result_name),
            instr,
            llvm_type,
            lv.to_str(),
            rv.to_str()
        ));
        LlvmVal::Local(result_name)
    }

    // =========================================================================
    // Unary operations
    // =========================================================================

    fn emit_unary(&mut self, op: &UnOp, operand: &Expr) -> LlvmVal {
        let val = self.emit_expr(operand);
        let ty = self.infer_type_of_expr(operand);
        let result_name = self.fresh("unary");

        match op {
            UnOp::Neg => {
                let llvm_type = self.type_to_llvm(&ty);
                if ty.is_float() {
                    self.emit_indent(&format!(
                        "{} = fneg {} {}",
                        self.l(&result_name),
                        llvm_type,
                        val.to_str()
                    ));
                } else {
                    self.emit_indent(&format!(
                        "{} = sub {} 0, {}",
                        self.l(&result_name),
                        llvm_type,
                        val.to_str()
                    ));
                }
            }
            UnOp::Not => {
                self.emit_indent(&format!(
                    "{} = xor i1 {}, true",
                    self.l(&result_name),
                    val.to_str()
                ));
            }
        }
        LlvmVal::Local(result_name)
    }

    // =========================================================================
    // Function and method calls
    // =========================================================================

    fn emit_call(&mut self, func: &Expr, args: &[Expr]) -> LlvmVal {
        if let Expr::Ident(name) = func {
            match name.as_str() {
                "print" => {
                    return self.emit_builtin_print(args);
                }
                "log" => {
                    return self.emit_builtin_log(args);
                }
                "inp" => {
                    return self.emit_builtin_inp(args);
                }
                "read_file" => {
                    return self.emit_builtin_read_file(args);
                }
                "write_file" => {
                    return self.emit_builtin_write_file(args);
                }
                "sqrt" | "abs" => {
                    return self.emit_builtin_math(name, args);
                }
                _ => {}
            }

            // Regular function call
            let arg_vals: Vec<(String, String)> = args
                .iter()
                .map(|a| {
                    let v = self.emit_expr(a);
                    let t = self.infer_type_of_expr(a);
                    (self.type_to_llvm(&t), v.to_str())
                })
                .collect();

            let args_str: Vec<String> = arg_vals
                .iter()
                .map(|(ty, val)| format!("{} {}", ty, val))
                .collect();

            let ret_type = self
                .env
                .lookup_fn(name)
                .and_then(|fns| fns.first())
                .map(|f| f.return_type.clone())
                .unwrap_or(Type::I32);

            if ret_type == Type::Unit {
                self.emit_indent(&format!("call void @{}({})", name, args_str.join(", ")));
                LlvmVal::Const("void".to_string())
            } else {
                let result_name = self.fresh("call");
                let llvm_ret = self.type_to_llvm(&ret_type);
                self.emit_indent(&format!(
                    "{} = call {} @{}({})",
                    self.l(&result_name),
                    llvm_ret,
                    name,
                    args_str.join(", ")
                ));
                LlvmVal::Local(result_name)
            }
        } else {
            LlvmVal::Const("null".to_string())
        }
    }

    fn emit_method_call(&mut self, receiver: &Expr, method: &str, args: &[Expr]) -> LlvmVal {
        let recv_val = self.emit_expr(receiver);
        let recv_type = self.infer_type_of_expr(receiver);

        if let Some(value) = self.emit_std_method_call(&recv_type, &recv_val, method, args) {
            return value;
        }

        // Mangled method name: TypeName_methodName
        let mangled = match &recv_type {
            Type::Named(type_name) => format!("{}_{}", type_name, method),
            _ => method.to_string(),
        };

        // Build args: self + explicit args
        let recv_llvm = self.type_to_llvm(&recv_type);
        let mut arg_strs = vec![format!("{} {}", recv_llvm, recv_val.to_str())];

        for a in args {
            let v = self.emit_expr(a);
            let t = self.infer_type_of_expr(a);
            arg_strs.push(format!("{} {}", self.type_to_llvm(&t), v.to_str()));
        }

        let ret_type = self
            .env
            .lookup_fn(method)
            .and_then(|fns| {
                fns.iter()
                    .find(|f| f.receiver_type.as_ref() == Some(&recv_type))
            })
            .map(|f| f.return_type.clone())
            .unwrap_or(Type::I32);

        if ret_type == Type::Unit {
            self.emit_indent(&format!("call void @{}({})", mangled, arg_strs.join(", ")));
            LlvmVal::Const("void".to_string())
        } else {
            let result_name = self.fresh("mcall");
            let llvm_ret = self.type_to_llvm(&ret_type);
            self.emit_indent(&format!(
                "{} = call {} @{}({})",
                self.l(&result_name),
                llvm_ret,
                mangled,
                arg_strs.join(", ")
            ));
            LlvmVal::Local(result_name)
        }
    }

    fn emit_std_method_call(
        &mut self,
        recv_type: &Type,
        recv_val: &LlvmVal,
        method: &str,
        args: &[Expr],
    ) -> Option<LlvmVal> {
        match (recv_type, method) {
            (Type::DynArray(_), "len") if args.is_empty() => {
                Some(self.emit_dyn_array_len(recv_val))
            }
            (Type::DynArray(elem), "push") if args.len() == 1 => {
                Some(self.emit_dyn_array_push(recv_val, elem, &args[0]))
            }
            (Type::Str, "slice") if args.len() == 2 => Some(self.emit_string_slice(recv_val, args)),
            _ if !args.is_empty() => None,
            (Type::Str, "len") => {
                let result_name = self.fresh("strlen");
                self.emit_indent(&format!(
                    "{} = call i64 @strlen(ptr {})",
                    self.l(&result_name),
                    recv_val.to_str()
                ));
                Some(LlvmVal::Local(result_name))
            }
            (Type::Str, "is_empty") => {
                let len_name = self.fresh("strlen");
                self.emit_indent(&format!(
                    "{} = call i64 @strlen(ptr {})",
                    self.l(&len_name),
                    recv_val.to_str()
                ));
                let result_name = self.fresh("str_empty");
                self.emit_indent(&format!(
                    "{} = icmp eq i64 {}, 0",
                    self.l(&result_name),
                    self.l(&len_name)
                ));
                Some(LlvmVal::Local(result_name))
            }
            (Type::Char, "is_digit") => {
                Some(self.emit_char_range_check(recv_val, "is_digit", &[('0' as u32, '9' as u32)]))
            }
            (Type::Char, "is_alpha") => Some(self.emit_char_range_check(
                recv_val,
                "is_alpha",
                &[('a' as u32, 'z' as u32), ('A' as u32, 'Z' as u32)],
            )),
            (Type::Char, "is_alnum") => Some(self.emit_char_range_check(
                recv_val,
                "is_alnum",
                &[
                    ('0' as u32, '9' as u32),
                    ('a' as u32, 'z' as u32),
                    ('A' as u32, 'Z' as u32),
                ],
            )),
            (Type::Char, "is_whitespace") => {
                Some(self.emit_char_any_check(recv_val, "is_space", &[9, 10, 13, 32]))
            }
            (ty, "abs") if ty.is_signed_int() => {
                let llvm_type = self.type_to_llvm(ty);
                let is_neg = self.fresh("is_neg");
                self.emit_indent(&format!(
                    "{} = icmp slt {} {}, 0",
                    self.l(&is_neg),
                    llvm_type,
                    recv_val.to_str()
                ));
                let neg = self.fresh("neg");
                self.emit_indent(&format!(
                    "{} = sub {} 0, {}",
                    self.l(&neg),
                    llvm_type,
                    recv_val.to_str()
                ));
                let result_name = self.fresh("abs");
                self.emit_indent(&format!(
                    "{} = select i1 {}, {} {}, {} {}",
                    self.l(&result_name),
                    self.l(&is_neg),
                    llvm_type,
                    self.l(&neg),
                    llvm_type,
                    recv_val.to_str()
                ));
                Some(LlvmVal::Local(result_name))
            }
            _ => None,
        }
    }

    fn emit_string_slice(&mut self, recv_val: &LlvmVal, args: &[Expr]) -> LlvmVal {
        let start_val = self.emit_expr(&args[0]);
        let end_val = self.emit_expr(&args[1]);
        let start_type = self.infer_type_of_expr(&args[0]);
        let end_type = self.infer_type_of_expr(&args[1]);
        let start = self.coerce_int_value(&start_val, &start_type, &Type::I64);
        let end = self.coerce_int_value(&end_val, &end_type, &Type::I64);

        let len = self.fresh("slice_len");
        self.emit_indent(&format!("{} = sub i64 {}, {}", self.l(&len), end, start));
        let alloc_size = self.fresh("slice_alloc_size");
        self.emit_indent(&format!(
            "{} = add i64 {}, 1",
            self.l(&alloc_size),
            self.l(&len)
        ));
        let buffer = self.fresh("slice_buf");
        self.emit_indent(&format!(
            "{} = call ptr @malloc(i64 {})",
            self.l(&buffer),
            self.l(&alloc_size)
        ));
        let source = self.fresh("slice_src");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds i8, ptr {}, i64 {}",
            self.l(&source),
            recv_val.to_str(),
            start
        ));
        self.emit_indent(&format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {}, ptr {}, i64 {}, i1 false)",
            self.l(&buffer),
            self.l(&source),
            self.l(&len)
        ));
        let null_ptr = self.fresh("slice_null");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds i8, ptr {}, i64 {}",
            self.l(&null_ptr),
            self.l(&buffer),
            self.l(&len)
        ));
        self.emit_indent(&format!("store i8 0, ptr {}", self.l(&null_ptr)));

        LlvmVal::Local(buffer)
    }

    fn emit_char_range_check(
        &mut self,
        recv_val: &LlvmVal,
        prefix: &str,
        ranges: &[(u32, u32)],
    ) -> LlvmVal {
        let mut result: Option<String> = None;
        for (start, end) in ranges {
            let ge = self.fresh(&format!("{}_ge", prefix));
            self.emit_indent(&format!(
                "{} = icmp uge i32 {}, {}",
                self.l(&ge),
                recv_val.to_str(),
                start
            ));
            let le = self.fresh(&format!("{}_le", prefix));
            self.emit_indent(&format!(
                "{} = icmp ule i32 {}, {}",
                self.l(&le),
                recv_val.to_str(),
                end
            ));
            let in_range = self.fresh(&format!("{}_range", prefix));
            self.emit_indent(&format!(
                "{} = and i1 {}, {}",
                self.l(&in_range),
                self.l(&ge),
                self.l(&le)
            ));
            result = Some(if let Some(prev) = result {
                let combined = self.fresh(prefix);
                self.emit_indent(&format!(
                    "{} = or i1 {}, {}",
                    self.l(&combined),
                    self.l(&prev),
                    self.l(&in_range)
                ));
                combined
            } else {
                in_range
            });
        }

        LlvmVal::Local(result.unwrap_or_else(|| "false".to_string()))
    }

    fn emit_char_any_check(&mut self, recv_val: &LlvmVal, prefix: &str, values: &[u32]) -> LlvmVal {
        let mut result: Option<String> = None;
        for value in values {
            let eq = self.fresh(&format!("{}_eq", prefix));
            self.emit_indent(&format!(
                "{} = icmp eq i32 {}, {}",
                self.l(&eq),
                recv_val.to_str(),
                value
            ));
            result = Some(if let Some(prev) = result {
                let combined = self.fresh(prefix);
                self.emit_indent(&format!(
                    "{} = or i1 {}, {}",
                    self.l(&combined),
                    self.l(&prev),
                    self.l(&eq)
                ));
                combined
            } else {
                eq
            });
        }

        LlvmVal::Local(result.unwrap_or_else(|| "false".to_string()))
    }

    fn emit_builtin_print(&mut self, args: &[Expr]) -> LlvmVal {
        if args.is_empty() {
            return LlvmVal::Const("void".to_string());
        }

        let val = self.emit_expr(&args[0]);
        let arg_type = self.infer_type_of_expr(&args[0]);

        match arg_type {
            Type::Str => {
                self.emit_indent(&format!("call i32 @puts(ptr {})", val.to_str()));
            }
            Type::I32 | Type::I64 | Type::U32 | Type::U64 => {
                // Call printf with "%d\n" format
                let fmt_name = self.fresh("fmt");
                self.string_constants
                    .push((fmt_name.clone(), "%d\n".to_string(), 4));
                let fmt_ptr = self.fresh("fmt_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [4 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&fmt_ptr),
                    fmt_name
                ));
                self.emit_indent(&format!(
                    "call i32 (ptr, ...) @printf(ptr {}, {} {})",
                    self.l(&fmt_ptr),
                    self.type_to_llvm(&arg_type),
                    val.to_str()
                ));
            }
            Type::F32 | Type::F64 => {
                let fmt_name = self.fresh("fmt");
                self.string_constants
                    .push((fmt_name.clone(), "%f\n".to_string(), 4));
                let fmt_ptr = self.fresh("fmt_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [4 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&fmt_ptr),
                    fmt_name
                ));
                // printf expects doubles for varargs
                if arg_type == Type::F32 {
                    let ext_name = self.fresh("fpext");
                    self.emit_indent(&format!(
                        "{} = fpext float {} to double",
                        self.l(&ext_name),
                        val.to_str()
                    ));
                    self.emit_indent(&format!(
                        "call i32 (ptr, ...) @printf(ptr {}, double {})",
                        self.l(&fmt_ptr),
                        self.l(&ext_name)
                    ));
                } else {
                    self.emit_indent(&format!(
                        "call i32 (ptr, ...) @printf(ptr {}, double {})",
                        self.l(&fmt_ptr),
                        val.to_str()
                    ));
                }
            }
            Type::Bool => {
                let fmt_name = self.fresh("fmt");
                self.string_constants
                    .push((fmt_name.clone(), "%s\n".to_string(), 4));
                let fmt_ptr = self.fresh("fmt_ptr");
                let true_str = self.fresh("true_s");
                let false_str = self.fresh("false_s");
                self.string_constants
                    .push((true_str.clone(), "true".to_string(), 5));
                self.string_constants
                    .push((false_str.clone(), "false".to_string(), 6));

                let true_ptr = self.fresh("true_ptr");
                let false_ptr = self.fresh("false_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [5 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&true_ptr),
                    true_str
                ));
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [6 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&false_ptr),
                    false_str
                ));
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [4 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&fmt_ptr),
                    fmt_name
                ));

                let result_name = self.fresh("sel");
                self.emit_indent(&format!(
                    "{} = select i1 {}, ptr {}, ptr {}",
                    self.l(&result_name),
                    val.to_str(),
                    self.l(&true_ptr),
                    self.l(&false_ptr)
                ));
                self.emit_indent(&format!(
                    "call i32 (ptr, ...) @printf(ptr {}, ptr {})",
                    self.l(&fmt_ptr),
                    self.l(&result_name)
                ));
            }
            Type::Char => {
                let fmt_name = self.fresh("fmt");
                self.string_constants
                    .push((fmt_name.clone(), "%c\n".to_string(), 4));
                let fmt_ptr = self.fresh("fmt_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds [4 x i8], ptr @{}, i64 0, i64 0",
                    self.l(&fmt_ptr),
                    fmt_name
                ));
                self.emit_indent(&format!(
                    "call i32 (ptr, ...) @printf(ptr {}, i32 {})",
                    self.l(&fmt_ptr),
                    val.to_str()
                ));
            }
            _ => {
                // Generic: just call puts
                self.emit_indent(&format!("call i32 @puts(ptr {})", val.to_str()));
            }
        }
        LlvmVal::Const("void".to_string())
    }

    fn emit_builtin_log(&mut self, args: &[Expr]) -> LlvmVal {
        match os::log_lowering(os::current()) {
            os::IoLowering::Libc => self.emit_builtin_print(args),
            os::IoLowering::LinuxSyscall
            | os::IoLowering::MacosSyscall
            | os::IoLowering::WindowsApi
            | os::IoLowering::Placeholder => {
                // Placeholder until direct syscall/API lowering is implemented.
                self.emit_builtin_print(args)
            }
        }
    }

    fn emit_builtin_inp(&mut self, args: &[Expr]) -> LlvmVal {
        if !args.is_empty() {
            return LlvmVal::Const("null".to_string());
        }

        match os::inp_lowering(os::current()) {
            os::IoLowering::LinuxSyscall
            | os::IoLowering::MacosSyscall
            | os::IoLowering::WindowsApi => {
                // Future: lower to OS-specific stdin syscall/API.
            }
            os::IoLowering::Libc | os::IoLowering::Placeholder => {
                // Placeholder below returns an empty string.
            }
        }

        let const_name = self.fresh("inp_empty");
        self.string_constants.push((const_name.clone(), "".to_string(), 1));
        let ptr_name = self.fresh("inp_empty_ptr");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds [1 x i8], ptr @{}, i64 0, i64 0",
            self.l(&ptr_name),
            const_name
        ));
        LlvmVal::Local(ptr_name)
    }

    fn emit_builtin_read_file(&mut self, args: &[Expr]) -> LlvmVal {
        if args.len() != 1 {
            return LlvmVal::Const("null".to_string());
        }

        let path = self.emit_expr(&args[0]);
        let mode_name = self.fresh("file_mode");
        self.string_constants
            .push((mode_name.clone(), "rb".to_string(), 3));
        let mode_ptr = self.fresh("file_mode_ptr");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds [3 x i8], ptr @{}, i64 0, i64 0",
            self.l(&mode_ptr),
            mode_name
        ));

        let file = self.fresh("file");
        self.emit_indent(&format!(
            "{} = call ptr @fopen(ptr {}, ptr {})",
            self.l(&file),
            path.to_str(),
            self.l(&mode_ptr)
        ));

        self.emit_indent(&format!(
            "call i32 @fseek(ptr {}, i64 0, i32 2)",
            self.l(&file)
        ));
        let size = self.fresh("file_size");
        self.emit_indent(&format!(
            "{} = call i64 @ftell(ptr {})",
            self.l(&size),
            self.l(&file)
        ));
        self.emit_indent(&format!(
            "call i32 @fseek(ptr {}, i64 0, i32 0)",
            self.l(&file)
        ));

        let alloc_size = self.fresh("file_alloc_size");
        self.emit_indent(&format!(
            "{} = add i64 {}, 1",
            self.l(&alloc_size),
            self.l(&size)
        ));
        let buffer = self.fresh("file_buffer");
        self.emit_indent(&format!(
            "{} = call ptr @malloc(i64 {})",
            self.l(&buffer),
            self.l(&alloc_size)
        ));
        self.emit_indent(&format!(
            "call i64 @fread(ptr {}, i64 1, i64 {}, ptr {})",
            self.l(&buffer),
            self.l(&size),
            self.l(&file)
        ));
        let null_ptr = self.fresh("file_null");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds i8, ptr {}, i64 {}",
            self.l(&null_ptr),
            self.l(&buffer),
            self.l(&size)
        ));
        self.emit_indent(&format!("store i8 0, ptr {}", self.l(&null_ptr)));
        self.emit_indent(&format!("call i32 @fclose(ptr {})", self.l(&file)));

        LlvmVal::Local(buffer)
    }

    fn emit_builtin_write_file(&mut self, args: &[Expr]) -> LlvmVal {
        if args.len() != 2 {
            return LlvmVal::Const("0".to_string());
        }

        let path = self.emit_expr(&args[0]);
        let contents = self.emit_expr(&args[1]);
        let mode_name = self.fresh("file_mode");
        self.string_constants
            .push((mode_name.clone(), "wb".to_string(), 3));
        let mode_ptr = self.fresh("file_mode_ptr");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds [3 x i8], ptr @{}, i64 0, i64 0",
            self.l(&mode_ptr),
            mode_name
        ));
        let file = self.fresh("file");
        self.emit_indent(&format!(
            "{} = call ptr @fopen(ptr {}, ptr {})",
            self.l(&file),
            path.to_str(),
            self.l(&mode_ptr)
        ));
        let len = self.fresh("write_len");
        self.emit_indent(&format!(
            "{} = call i64 @strlen(ptr {})",
            self.l(&len),
            contents.to_str()
        ));
        let written = self.fresh("written");
        self.emit_indent(&format!(
            "{} = call i64 @fwrite(ptr {}, i64 1, i64 {}, ptr {})",
            self.l(&written),
            contents.to_str(),
            self.l(&len),
            self.l(&file)
        ));
        self.emit_indent(&format!("call i32 @fclose(ptr {})", self.l(&file)));
        let ok = self.fresh("write_ok");
        self.emit_indent(&format!(
            "{} = icmp eq i64 {}, {}",
            self.l(&ok),
            self.l(&written),
            self.l(&len)
        ));
        let result = self.fresh("write_result");
        self.emit_indent(&format!(
            "{} = zext i1 {} to i32",
            self.l(&result),
            self.l(&ok)
        ));
        LlvmVal::Local(result)
    }

    fn emit_string_concat(&mut self, lv: &LlvmVal, rv: &LlvmVal, lt: &Type, rt: &Type) -> LlvmVal {
        // When both operands are already strings (ptr), just concatenate directly
        let left_ptr = if *lt == Type::Str {
            lv.to_str()
        } else {
            // Convert non-string to string representation via sprintf
            let fmt_name = self.fresh("fmt");
            self.string_constants
                .push((fmt_name.clone(), "%d".to_string(), 3));
            let fmt_ptr = self.fresh("fmt_ptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds [3 x i8], ptr @{}, i64 0, i64 0",
                self.l(&fmt_ptr),
                fmt_name
            ));

            // Allocate buffer for the string representation
            let buf_name = self.fresh("buf");
            self.emit_indent(&format!("{} = alloca [32 x i8]", self.l(&buf_name)));
            self.emit_indent(&format!(
                "call i32 (ptr, ...) @sprintf(ptr {}, ptr {}, {} {})",
                self.l(&buf_name),
                self.l(&fmt_ptr),
                self.type_to_llvm(lt),
                lv.to_str()
            ));
            self.l(&buf_name)
        };

        let right_ptr = if *rt == Type::Str {
            rv.to_str()
        } else {
            let fmt_name = self.fresh("fmt");
            self.string_constants
                .push((fmt_name.clone(), "%d".to_string(), 3));
            let fmt_ptr = self.fresh("fmt_ptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds [3 x i8], ptr @{}, i64 0, i64 0",
                self.l(&fmt_ptr),
                fmt_name
            ));
            let buf_name = self.fresh("buf");
            self.emit_indent(&format!("{} = alloca [32 x i8]", self.l(&buf_name)));
            self.emit_indent(&format!(
                "call i32 (ptr, ...) @sprintf(ptr {}, ptr {}, {} {})",
                self.l(&buf_name),
                self.l(&fmt_ptr),
                self.type_to_llvm(rt),
                rv.to_str()
            ));
            self.l(&buf_name)
        };

        // Calculate total length: strlen(left) + strlen(right) + 1
        let len1 = self.fresh("slen1");
        self.emit_indent(&format!(
            "{} = call i64 @strlen(ptr {})",
            self.l(&len1),
            left_ptr
        ));
        let len2 = self.fresh("slen2");
        self.emit_indent(&format!(
            "{} = call i64 @strlen(ptr {})",
            self.l(&len2),
            right_ptr
        ));
        let total_len = self.fresh("total_len");
        self.emit_indent(&format!(
            "{} = add i64 {}, {}",
            self.l(&total_len),
            self.l(&len1),
            self.l(&len2)
        ));
        let alloc_size = self.fresh("alloc_size");
        self.emit_indent(&format!(
            "{} = add i64 {}, 1",
            self.l(&alloc_size),
            self.l(&total_len)
        ));

        // Allocate result buffer
        let result_buf = self.fresh("str_result");
        self.emit_indent(&format!(
            "{} = call ptr @malloc(i64 {})",
            self.l(&result_buf),
            self.l(&alloc_size)
        ));

        // Copy left string
        self.emit_indent(&format!(
            "call ptr @strcpy(ptr {}, ptr {})",
            self.l(&result_buf),
            left_ptr
        ));
        // Concatenate right string
        self.emit_indent(&format!(
            "call ptr @strcat(ptr {}, ptr {})",
            self.l(&result_buf),
            right_ptr
        ));

        LlvmVal::Local(result_buf)
    }

    fn emit_builtin_math(&mut self, name: &str, args: &[Expr]) -> LlvmVal {
        if args.is_empty() {
            return LlvmVal::Const("0".to_string());
        }
        let val = self.emit_expr(&args[0]);
        let ty = self.infer_type_of_expr(&args[0]);
        let result_name = self.fresh(name);
        let llvm_type = self.type_to_llvm(&ty);

        match name {
            "sqrt" => {
                self.emit_indent(&format!(
                    "{} = call {} @llvm.sqrt.{}({} {})",
                    self.l(&result_name),
                    llvm_type,
                    if ty == Type::F32 { "f32" } else { "f64" },
                    llvm_type,
                    val.to_str()
                ));
            }
            "abs" => {
                if ty.is_float() {
                    self.emit_indent(&format!(
                        "{} = call {} @llvm.fabs.{}({} {})",
                        self.l(&result_name),
                        llvm_type,
                        if ty == Type::F32 { "f32" } else { "f64" },
                        llvm_type,
                        val.to_str()
                    ));
                } else {
                    // For integers, use: x < 0 ? -x : x
                    let is_neg = self.fresh("isneg");
                    self.emit_indent(&format!(
                        "{} = icmp slt {} {}, 0",
                        self.l(&is_neg),
                        llvm_type,
                        val.to_str()
                    ));
                    let neg_val = self.fresh("neg");
                    self.emit_indent(&format!(
                        "{} = sub {} 0, {}",
                        self.l(&neg_val),
                        llvm_type,
                        val.to_str()
                    ));
                    self.emit_indent(&format!(
                        "{} = select i1 {}, {} {}, {} {}",
                        self.l(&result_name),
                        self.l(&is_neg),
                        llvm_type,
                        self.l(&neg_val),
                        llvm_type,
                        val.to_str()
                    ));
                }
            }
            _ => {}
        }
        LlvmVal::Local(result_name)
    }

    // =========================================================================
    // Field access
    // =========================================================================

    fn emit_field_access(&mut self, object: &Expr, field: &str) -> LlvmVal {
        let obj_val = self.emit_expr(object);
        let obj_type = self.infer_type_of_expr(object);
        let field_idx = self.get_field_index(&obj_type, field);
        let field_type = self.get_field_type(&obj_type, field);

        let struct_type_name = match &obj_type {
            Type::Named(name) => format!("%{}_struct", name),
            _ => self.type_to_llvm(&obj_type),
        };

        // The object value may be a first-class struct value (from load) or a pointer.
        // If it's a named struct type, we need to alloca+store it to get a pointer for GEP.
        let obj_ptr = if matches!(&obj_type, Type::Named(_)) {
            let alloca_name = self.fresh("obj");
            self.emit_indent(&format!(
                "{} = alloca {}",
                self.l(&alloca_name),
                struct_type_name
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                struct_type_name,
                obj_val.to_str(),
                self.l(&alloca_name)
            ));
            self.l(&alloca_name)
        } else {
            // Already a pointer
            match &obj_val {
                LlvmVal::Local(s) => self.l(s),
                _ => {
                    let alloca_name = self.fresh("obj");
                    self.emit_indent(&format!(
                        "{} = alloca {}",
                        self.l(&alloca_name),
                        struct_type_name
                    ));
                    self.emit_indent(&format!(
                        "store {} {}, ptr {}",
                        struct_type_name,
                        obj_val.to_str(),
                        self.l(&alloca_name)
                    ));
                    self.l(&alloca_name)
                }
            }
        };

        let ptr_name = self.fresh("fptr");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
            self.l(&ptr_name),
            struct_type_name,
            obj_ptr,
            field_idx
        ));

        let load_name = self.fresh("fload");
        self.emit_indent(&format!(
            "{} = load {}, ptr {}",
            self.l(&load_name),
            self.type_to_llvm(&field_type),
            self.l(&ptr_name)
        ));
        LlvmVal::Local(load_name)
    }

    // =========================================================================
    // Control flow
    // =========================================================================

    fn emit_if(
        &mut self,
        condition: &Expr,
        then_block: &Block,
        else_if_clauses: &[(Expr, Box<Block>)],
        else_block: &Option<Box<Block>>,
    ) -> LlvmVal {
        let cond_val = self.emit_expr(condition);

        // Determine the result type of the if expression
        let result_type = if let Some(tail) = &then_block.tail {
            self.infer_type_of_expr(tail)
        } else {
            Type::Unit
        };

        let then_label = self.fresh_label("then");
        let else_label = self.fresh_label("else");
        let end_label = self.fresh_label("endif");

        // Result alloca (for if-as-expression) - must come BEFORE the branch terminator
        let result_ptr = if result_type != Type::Unit {
            let ptr_name = self.fresh("if_result");
            self.emit_indent(&format!(
                "{} = alloca {}",
                self.l(&ptr_name),
                self.type_to_llvm(&result_type)
            ));
            Some(ptr_name)
        } else {
            None
        };

        // Branch
        self.emit_indent(&format!(
            "br i1 {}, label %{}, label %{}",
            cond_val.to_str(),
            then_label,
            else_label
        ));

        // Then block
        self.terminated = false;
        self.emit(&format!("{}:", then_label));
        if let Some(ptr) = &result_ptr {
            self.emit_block_to_ptr(then_block, ptr, &result_type);
        } else {
            self.emit_block(then_block);
        }
        if !self.terminated {
            self.emit_indent(&format!("br label %{}", end_label));
        }

        // Else-if chains
        let mut current_else_label = else_label;
        for (ei_cond, ei_block) in else_if_clauses.iter() {
            self.terminated = false;
            self.emit(&format!("{}:", current_else_label));
            let ei_cond_val = self.emit_expr(ei_cond);
            let ei_then_label = self.fresh_label("elseif_then");
            let next_else_label = self.fresh_label("else");

            self.emit_indent(&format!(
                "br i1 {}, label %{}, label %{}",
                ei_cond_val.to_str(),
                ei_then_label,
                next_else_label
            ));

            self.terminated = false;
            self.emit(&format!("{}:", ei_then_label));
            if let Some(ptr) = &result_ptr {
                self.emit_block_to_ptr(ei_block, ptr, &result_type);
            } else {
                self.emit_block(ei_block);
            }
            if !self.terminated {
                self.emit_indent(&format!("br label %{}", end_label));
            }
            current_else_label = next_else_label;
        }

        // Else block (or empty else for Unit result)
        self.terminated = false;
        self.emit(&format!("{}:", current_else_label));
        if let Some(else_b) = else_block {
            if let Some(ptr) = &result_ptr {
                self.emit_block_to_ptr(else_b, ptr, &result_type);
            } else {
                self.emit_block(else_b);
            }
        }
        if !self.terminated {
            self.emit_indent(&format!("br label %{}", end_label));
        }

        // End block
        self.terminated = false;
        self.emit(&format!("{}:", end_label));

        if let Some(ptr) = &result_ptr {
            let load_name = self.fresh("if_val");
            self.emit_indent(&format!(
                "{} = load {}, ptr {}",
                self.l(&load_name),
                self.type_to_llvm(&result_type),
                self.l(ptr)
            ));
            LlvmVal::Local(load_name)
        } else {
            LlvmVal::Const("void".to_string())
        }
    }

    fn emit_match(&mut self, subject: &Expr, arms: &[MatchArm]) -> LlvmVal {
        let subj_val = self.emit_expr(subject);
        let subj_type = self.infer_type_of_expr(subject);

        // Determine result type from first arm
        let result_type = arms
            .first()
            .map(|arm| self.infer_type_of_expr(&arm.body))
            .unwrap_or(Type::Unit);

        // Result alloca
        let result_ptr = if result_type != Type::Unit {
            let ptr_name = self.fresh("match_result");
            self.emit_indent(&format!(
                "{} = alloca {}",
                self.l(&ptr_name),
                self.type_to_llvm(&result_type)
            ));
            Some(ptr_name)
        } else {
            None
        };

        let end_label = self.fresh_label("match_end");

        // For enum match: extract the tag
        if let Type::Named(type_name) = &subj_type {
            if let Some(TypeInfo::Enum { variants, .. }) = self.env.lookup_type(type_name) {
                let tag_name = self.fresh("tag");
                let tag_ptr = self.fresh("tag_ptr");
                self.emit_indent(&format!(
                    "{} = getelementptr inbounds %{}_enum, ptr {}, i32 0, i32 0",
                    self.l(&tag_ptr),
                    type_name,
                    subj_val.to_str()
                ));
                self.emit_indent(&format!(
                    "{} = load i32, ptr {}",
                    self.l(&tag_name),
                    self.l(&tag_ptr)
                ));

                // Switch on the tag value
                let mut cases = Vec::new();
                let mut arm_labels = Vec::new();

                for (i, arm) in arms.iter().enumerate() {
                    let arm_label = self.fresh_label(&format!("arm_{}", i));
                    arm_labels.push(arm_label.clone());

                    // Determine which variant(s) this arm matches
                    match &arm.pattern {
                        Pattern::Variant { variant, .. } => {
                            for (vi, v) in variants.iter().enumerate() {
                                if v.0 == *variant {
                                    cases.push((vi as i64, arm_label.clone()));
                                }
                            }
                        }
                        Pattern::Wildcard | Pattern::Ident(_) => {
                            // Default case
                            cases.push((-1, arm_label.clone()));
                        }
                        _ => {}
                    }
                }

                let default_label = self.fresh_label("match_default");
                let default_i64: Vec<(i64, String)> = cases
                    .iter()
                    .filter(|(v, _)| *v >= 0)
                    .map(|(v, l)| (*v, l.clone()))
                    .collect();
                let has_default = cases.iter().any(|(v, _)| *v < 0);
                let default_target = if has_default {
                    cases
                        .iter()
                        .find(|(v, _)| *v < 0)
                        .map(|(_, l)| l.clone())
                        .unwrap_or(default_label.clone())
                } else {
                    default_label.clone()
                };

                self.emit_indent(&format!(
                    "switch i32 {}, label %{} [{}]",
                    self.l(&tag_name),
                    default_target,
                    default_i64
                        .iter()
                        .map(|(v, l)| format!("i32 {}, label %{}", v, l))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));

                // Emit each arm
                for (i, arm) in arms.iter().enumerate() {
                    self.terminated = false;
                    self.emit(&format!("{}:", arm_labels[i]));

                    // If variant has a binding, extract the payload
                    if let Pattern::Variant {
                        binding: Some(bind_name),
                        ..
                    } = &arm.pattern
                    {
                        // Extract payload pointer from enum
                        let payload_ptr = self.fresh("payload_ptr");
                        self.emit_indent(&format!(
                            "{} = getelementptr inbounds %{}_enum, ptr {}, i32 0, i32 1",
                            self.l(&payload_ptr),
                            type_name,
                            subj_val.to_str()
                        ));
                        // Bitcast to the correct type
                        let payload_type = self.infer_type_of_expr(&arm.body); // approximation
                        let alloca_name = format!("{}_addr", bind_name);
                        self.emit_indent(&format!(
                            "{} = alloca {}",
                            self.l(&alloca_name),
                            self.type_to_llvm(&payload_type)
                        ));
                        // Copy from payload
                        let payload_size = self.type_size(&payload_type);
                        self.emit_indent(&format!(
                            "call void @llvm.memcpy.p0.p0.i64(ptr {}, ptr {}, i64 {}, i1 false)",
                            self.l(&alloca_name),
                            self.l(&payload_ptr),
                            payload_size
                        ));
                        self.var_allocas.push(bind_name.clone());
                    }

                    let arm_val = self.emit_expr(&arm.body);
                    let arm_type = self.infer_type_of_expr(&arm.body);
                    if let Some(ptr) = &result_ptr {
                        self.emit_indent(&format!(
                            "store {} {}, ptr {}",
                            self.type_to_llvm(&arm_type),
                            arm_val.to_str(),
                            self.l(ptr)
                        ));
                    }
                    if !self.terminated {
                        self.emit_indent(&format!("br label %{}", end_label));
                    }
                }

                // Default case (for wildcard patterns or unmatched)
                if !has_default {
                    self.terminated = false;
                    self.emit(&format!("{}:", default_label));
                    self.emit_indent(&format!("br label %{}", end_label));
                }

                self.terminated = false;
                self.emit(&format!("{}:", end_label));

                if let Some(ptr) = &result_ptr {
                    let load_name = self.fresh("match.val");
                    self.emit_indent(&format!(
                        "{} = load {}, ptr {}",
                        self.l(&load_name),
                        self.type_to_llvm(&result_type),
                        self.l(ptr)
                    ));
                    return LlvmVal::Local(load_name);
                }
                return LlvmVal::Const("void".to_string());
            }
        }

        // Simple match (non-enum) - emit as if-else chain
        for arm in arms.iter() {
            match &arm.pattern {
                Pattern::Wildcard => {
                    let arm_val = self.emit_expr(&arm.body);
                    if let Some(ptr) = &result_ptr {
                        let arm_type = self.infer_type_of_expr(&arm.body);
                        self.emit_indent(&format!(
                            "store {} {}, ptr {}",
                            self.type_to_llvm(&arm_type),
                            arm_val.to_str(),
                            self.l(ptr)
                        ));
                    }
                    if !self.terminated {
                        self.emit_indent(&format!("br label %{}", end_label));
                    }
                }
                Pattern::Bool(b) => {
                    let cond = LlvmVal::Const(if *b { "true" } else { "false" }.to_string());
                    let arm_label = self.fresh_label("match_arm");
                    let next_label = self.fresh_label("match_next");
                    self.emit_indent(&format!(
                        "br i1 {}, label %{}, label %{}",
                        cond.to_str(),
                        arm_label,
                        next_label
                    ));
                    self.terminated = false;
                    self.emit(&format!("{}:", arm_label));
                    let arm_val = self.emit_expr(&arm.body);
                    if let Some(ptr) = &result_ptr {
                        let arm_type = self.infer_type_of_expr(&arm.body);
                        self.emit_indent(&format!(
                            "store {} {}, ptr {}",
                            self.type_to_llvm(&arm_type),
                            arm_val.to_str(),
                            self.l(ptr)
                        ));
                    }
                    if !self.terminated {
                        self.emit_indent(&format!("br label %{}", end_label));
                    }
                    self.terminated = false;
                    self.emit(&format!("{}:", next_label));
                }
                Pattern::Int(val) => {
                    // Compare subject with the integer constant
                    let cmp_name = self.fresh("match_cmp");
                    self.emit_indent(&format!(
                        "{} = icmp eq {} {}, {}",
                        self.l(&cmp_name),
                        self.type_to_llvm(&subj_type),
                        subj_val.to_str(),
                        val
                    ));
                    let arm_label = self.fresh_label("match_arm");
                    let next_label = self.fresh_label("match_next");
                    self.emit_indent(&format!(
                        "br i1 {}, label %{}, label %{}",
                        self.l(&cmp_name),
                        arm_label,
                        next_label
                    ));
                    self.terminated = false;
                    self.emit(&format!("{}:", arm_label));
                    let arm_val = self.emit_expr(&arm.body);
                    if let Some(ptr) = &result_ptr {
                        let arm_type = self.infer_type_of_expr(&arm.body);
                        self.emit_indent(&format!(
                            "store {} {}, ptr {}",
                            self.type_to_llvm(&arm_type),
                            arm_val.to_str(),
                            self.l(ptr)
                        ));
                    }
                    if !self.terminated {
                        self.emit_indent(&format!("br label %{}", end_label));
                    }
                    self.terminated = false;
                    self.emit(&format!("{}:", next_label));
                }
                _ => {
                    // For other patterns, just evaluate the body
                    let arm_val = self.emit_expr(&arm.body);
                    if let Some(ptr) = &result_ptr {
                        let arm_type = self.infer_type_of_expr(&arm.body);
                        self.emit_indent(&format!(
                            "store {} {}, ptr {}",
                            self.type_to_llvm(&arm_type),
                            arm_val.to_str(),
                            self.l(ptr)
                        ));
                    }
                    if !self.terminated {
                        self.emit_indent(&format!("br label %{}", end_label));
                    }
                }
            }
        }

        self.terminated = false;
        self.emit(&format!("{}:", end_label));

        if let Some(ptr) = &result_ptr {
            let load_name = self.fresh("match.val");
            self.emit_indent(&format!(
                "{} = load {}, ptr {}",
                load_name,
                self.type_to_llvm(&result_type),
                ptr
            ));
            LlvmVal::Local(load_name)
        } else {
            LlvmVal::Const("void".to_string())
        }
    }

    // =========================================================================
    // Loops
    // =========================================================================

    fn emit_loop(&mut self, body: &Block) -> LlvmVal {
        let start_label = self.fresh_label("loop_start");
        let end_label = self.fresh_label("loop_end");
        let continue_label = start_label.clone();

        self.emit_indent(&format!("br label %{}", start_label));

        self.terminated = false;
        self.emit(&format!("{}:", start_label));

        self.loop_end_labels.push(end_label.clone());
        self.loop_continue_labels.push(continue_label.clone());

        self.emit_block(body);

        self.loop_end_labels.pop();
        self.loop_continue_labels.pop();

        // Loop back
        if !self.terminated {
            self.emit_indent(&format!("br label %{}", start_label));
        }

        self.terminated = false;
        self.emit(&format!("{}:", end_label));
        LlvmVal::Const("void".to_string())
    }

    fn emit_while(&mut self, condition: &Expr, body: &Block) -> LlvmVal {
        let cond_label = self.fresh_label("while_cond");
        let body_label = self.fresh_label("while_body");
        let end_label = self.fresh_label("while_end");
        let continue_label = cond_label.clone();

        self.emit_indent(&format!("br label %{}", cond_label));

        // Condition block
        self.terminated = false;
        self.emit(&format!("{}:", cond_label));
        let cond_val = self.emit_expr(condition);
        self.emit_indent(&format!(
            "br i1 {}, label %{}, label %{}",
            cond_val.to_str(),
            body_label,
            end_label
        ));

        // Body block
        self.terminated = false;
        self.emit(&format!("{}:", body_label));

        self.loop_end_labels.push(end_label.clone());
        self.loop_continue_labels.push(continue_label.clone());

        self.emit_block(body);

        self.loop_end_labels.pop();
        self.loop_continue_labels.pop();

        if !self.terminated {
            self.emit_indent(&format!("br label %{}", cond_label));
        }

        self.terminated = false;
        self.emit(&format!("{}:", end_label));
        LlvmVal::Const("void".to_string())
    }

    fn emit_for_each(
        &mut self,
        var: &str,
        type_ann: Option<&TypeExpr>,
        iterable: &Expr,
        body: &Block,
    ) -> LlvmVal {
        if let Expr::RangeLiteral { start, end } = iterable {
            return self.emit_range_for_each(var, type_ann, start, end, body);
        }

        // For-each is lowered to:
        //   let iter = iterable;
        //   let idx = 0;
        //   let len = iter.len();
        //   while idx < len {
        //     let var = iter[idx];
        //     body
        //     idx = idx + 1;
        //   }

        let iter_val = self.emit_expr(iterable);
        let iter_type = self.infer_type_of_expr(iterable);

        let elem_type = match &iter_type {
            Type::Array { element, .. } => *element.clone(),
            Type::DynArray(elem) => *elem.clone(),
            _ => Type::I32,
        };

        // Allocate index variable
        let idx_alloca = format!("{}_addr", self.fresh("idx"));
        self.emit_indent(&format!("{} = alloca i32", self.l(&idx_alloca)));
        self.emit_indent(&format!("store i32 0, ptr {}", self.l(&idx_alloca)));

        // Allocate element variable
        let elem_alloca = format!("{}_addr", var);
        self.emit_indent(&format!(
            "{} = alloca {}",
            self.l(&elem_alloca),
            self.type_to_llvm(&elem_type)
        ));
        self.var_allocas.push(var.to_string());

        // Get length (assume 0 for now; proper impl would call .len())
        let len_val = match &iter_type {
            Type::Array { size, .. } => LlvmVal::Const(size.to_string()),
            _ => LlvmVal::Const("0".to_string()),
        };

        let len_alloca = self.fresh("len_addr");
        self.emit_indent(&format!("{} = alloca i32", self.l(&len_alloca)));
        self.emit_indent(&format!(
            "store i32 {}, ptr {}",
            len_val.to_str(),
            self.l(&len_alloca)
        ));

        let cond_label = self.fresh_label("foreach_cond");
        let body_label = self.fresh_label("foreach_body");
        let end_label = self.fresh_label("foreach_end");
        let continue_label = cond_label.clone();

        self.emit_indent(&format!("br label %{}", cond_label));

        // Condition: idx < len
        self.terminated = false;
        self.emit(&format!("{}:", cond_label));
        let idx_load = self.fresh("idx_load");
        self.emit_indent(&format!(
            "{} = load i32, ptr {}",
            self.l(&idx_load),
            self.l(&idx_alloca)
        ));
        let len_load = self.fresh("len_load");
        self.emit_indent(&format!(
            "{} = load i32, ptr {}",
            self.l(&len_load),
            self.l(&len_alloca)
        ));
        let cmp_name = self.fresh("foreach_cmp");
        self.emit_indent(&format!(
            "{} = icmp slt i32 {}, {}",
            self.l(&cmp_name),
            self.l(&idx_load),
            self.l(&len_load)
        ));
        self.emit_indent(&format!(
            "br i1 {}, label %{}, label %{}",
            self.l(&cmp_name),
            body_label,
            end_label
        ));

        // Body
        self.terminated = false;
        self.emit(&format!("{}:", body_label));

        // Load current element: iter[idx]
        let idx_load2 = self.fresh("idx2");
        self.emit_indent(&format!(
            "{} = load i32, ptr {}",
            self.l(&idx_load2),
            self.l(&idx_alloca)
        ));
        let elem_ptr = self.fresh("elem_ptr");
        let elem_llvm = self.type_to_llvm(&elem_type);
        self.emit_indent(&format!(
            "{} = getelementptr inbounds {}, ptr {}, i32 {}",
            self.l(&elem_ptr),
            elem_llvm,
            iter_val.to_str(),
            self.l(&idx_load2)
        ));
        let elem_load = self.fresh("elem_load");
        self.emit_indent(&format!(
            "{} = load {}, ptr {}",
            self.l(&elem_load),
            elem_llvm,
            self.l(&elem_ptr)
        ));
        self.emit_indent(&format!(
            "store {} {}, ptr {}",
            elem_llvm,
            self.l(&elem_load),
            self.l(&elem_alloca)
        ));

        self.loop_end_labels.push(end_label.clone());
        self.loop_continue_labels.push(continue_label.clone());

        self.emit_block(body);

        self.loop_end_labels.pop();
        self.loop_continue_labels.pop();

        // Increment index
        if !self.terminated {
            let idx_load3 = self.fresh("idx3");
            self.emit_indent(&format!(
                "{} = load i32, ptr {}",
                self.l(&idx_load3),
                self.l(&idx_alloca)
            ));
            let inc_name = self.fresh("idx.inc");
            self.emit_indent(&format!(
                "{} = add i32 {}, 1",
                self.l(&inc_name),
                self.l(&idx_load3)
            ));
            self.emit_indent(&format!(
                "store i32 {}, ptr {}",
                self.l(&inc_name),
                self.l(&idx_alloca)
            ));
            self.emit_indent(&format!("br label %{}", cond_label));
        }

        self.terminated = false;
        self.emit(&format!("{}:", end_label));
        LlvmVal::Const("void".to_string())
    }

    fn emit_range_for_each(
        &mut self,
        var: &str,
        type_ann: Option<&TypeExpr>,
        start: &Expr,
        end: &Expr,
        body: &Block,
    ) -> LlvmVal {
        let start_val = self.emit_expr(start);
        let end_val = self.emit_expr(end);
        let idx_type = type_ann
            .map(|type_ann| self.resolve_type_expr_simple(type_ann))
            .unwrap_or_else(|| {
                self.common_int_type(
                    &self.infer_type_of_expr(start),
                    &self.infer_type_of_expr(end),
                )
                .unwrap_or(Type::I64)
            });
        let idx_llvm = self.type_to_llvm(&idx_type);

        let idx_alloca = format!("{}_addr", var);
        self.emit_indent(&format!("{} = alloca {}", self.l(&idx_alloca), idx_llvm));
        let coerced_start =
            self.coerce_int_value(&start_val, &self.infer_type_of_expr(start), &idx_type);
        self.emit_indent(&format!(
            "store {} {}, ptr {}",
            idx_llvm,
            coerced_start,
            self.l(&idx_alloca)
        ));
        self.env.define_var(var.to_string(), idx_type.clone());
        self.var_allocas.push(var.to_string());

        let end_alloca = self.fresh("range_end_addr");
        self.emit_indent(&format!("{} = alloca {}", self.l(&end_alloca), idx_llvm));
        let coerced_end = self.coerce_int_value(&end_val, &self.infer_type_of_expr(end), &idx_type);
        self.emit_indent(&format!(
            "store {} {}, ptr {}",
            idx_llvm,
            coerced_end,
            self.l(&end_alloca)
        ));

        let cond_label = self.fresh_label("range_cond");
        let body_label = self.fresh_label("range_body");
        let end_label = self.fresh_label("range_end");
        let continue_label = cond_label.clone();

        self.emit_indent(&format!("br label %{}", cond_label));

        self.terminated = false;
        self.emit(&format!("{}:", cond_label));
        let idx_load = self.fresh("range_idx");
        self.emit_indent(&format!(
            "{} = load {}, ptr {}",
            self.l(&idx_load),
            idx_llvm,
            self.l(&idx_alloca)
        ));
        let end_load = self.fresh("range_end");
        self.emit_indent(&format!(
            "{} = load {}, ptr {}",
            self.l(&end_load),
            idx_llvm,
            self.l(&end_alloca)
        ));
        let cmp_name = self.fresh("range_cmp");
        let cmp_op = if idx_type.is_unsigned_int() {
            "ult"
        } else {
            "slt"
        };
        self.emit_indent(&format!(
            "{} = icmp {} {} {}, {}",
            self.l(&cmp_name),
            cmp_op,
            idx_llvm,
            self.l(&idx_load),
            self.l(&end_load)
        ));
        self.emit_indent(&format!(
            "br i1 {}, label %{}, label %{}",
            self.l(&cmp_name),
            body_label,
            end_label
        ));

        self.terminated = false;
        self.emit(&format!("{}:", body_label));

        self.loop_end_labels.push(end_label.clone());
        self.loop_continue_labels.push(continue_label.clone());
        self.emit_block(body);
        self.loop_end_labels.pop();
        self.loop_continue_labels.pop();

        if !self.terminated {
            let idx_load = self.fresh("range_idx_next");
            self.emit_indent(&format!(
                "{} = load {}, ptr {}",
                self.l(&idx_load),
                idx_llvm,
                self.l(&idx_alloca)
            ));
            let inc = self.fresh("range_inc");
            self.emit_indent(&format!(
                "{} = add {} {}, 1",
                self.l(&inc),
                idx_llvm,
                self.l(&idx_load)
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                idx_llvm,
                self.l(&inc),
                self.l(&idx_alloca)
            ));
            self.emit_indent(&format!("br label %{}", cond_label));
        }

        self.terminated = false;
        self.emit(&format!("{}:", end_label));
        LlvmVal::Const("void".to_string())
    }

    // =========================================================================
    // Fallible blocks
    // =========================================================================

    fn emit_fallible(&mut self, block: &Block, handler: &Option<FallibleHandler>) -> LlvmVal {
        // Emit the fallible block
        let block_val = self.emit_block_as_expr(block);

        if let Some(handler) = handler {
            match handler {
                FallibleHandler::Catch { err_name, body } => {
                    // The err_name binds the error value
                    let err_alloca = format!("{}_addr", err_name);
                    self.emit_indent(&format!("{} = alloca ptr", self.l(&err_alloca)));
                    // For now, store the block result as the error (simplified model)
                    self.emit_indent(&format!(
                        "store ptr {}, ptr {}",
                        block_val.to_str(),
                        self.l(&err_alloca)
                    ));
                    self.var_allocas.push(err_name.clone());

                    self.emit_block(body);
                    if let Some(tail) = &body.tail {
                        return self.emit_expr(tail);
                    }
                }
                FallibleHandler::CatchMatch { err_name, arms } => {
                    let err_alloca = format!("{}_addr", err_name);
                    self.emit_indent(&format!("{} = alloca ptr", self.l(&err_alloca)));
                    self.emit_indent(&format!(
                        "store ptr {}, ptr {}",
                        block_val.to_str(),
                        self.l(&err_alloca)
                    ));
                    self.var_allocas.push(err_name.clone());

                    // Emit match on error value
                    if !arms.is_empty() {
                        let last_arm_val = self.emit_expr(&arms.last().unwrap().body);
                        return last_arm_val;
                    }
                }
            }
        }

        block_val
    }

    fn emit_block_as_expr(&mut self, block: &Block) -> LlvmVal {
        for stmt in &block.stmts {
            if self.terminated {
                break;
            }
            self.emit_stmt(stmt);
        }
        if !self.terminated {
            if let Some(tail) = &block.tail {
                return self.emit_expr(tail);
            }
        }
        LlvmVal::Const("null".to_string())
    }

    // =========================================================================
    // Struct, enum, array, tuple literals
    // =========================================================================

    fn dyn_array_header_type(&self) -> &'static str {
        "{ i64, i64, ptr }"
    }

    fn emit_alloc_dyn_array_header(&mut self) -> String {
        let header = self.fresh("array_header");
        self.emit_indent(&format!("{} = call ptr @malloc(i64 24)", self.l(&header)));
        header
    }

    fn emit_dyn_array_field_ptr(&mut self, header: &LlvmVal, index: usize, prefix: &str) -> String {
        let ptr = self.fresh(prefix);
        self.emit_indent(&format!(
            "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
            self.l(&ptr),
            self.dyn_array_header_type(),
            header.to_str(),
            index
        ));
        ptr
    }

    fn emit_dyn_array_data_ptr(&mut self, header: &LlvmVal) -> String {
        let data_field = self.emit_dyn_array_field_ptr(header, 2, "array_data_field");
        let data = self.fresh("array_data");
        self.emit_indent(&format!(
            "{} = load ptr, ptr {}",
            self.l(&data),
            self.l(&data_field)
        ));
        self.l(&data)
    }

    fn emit_dyn_array_len(&mut self, header: &LlvmVal) -> LlvmVal {
        let len_field = self.emit_dyn_array_field_ptr(header, 0, "array_len_field");
        let len = self.fresh("array_len");
        self.emit_indent(&format!(
            "{} = load i64, ptr {}",
            self.l(&len),
            self.l(&len_field)
        ));
        LlvmVal::Local(len)
    }

    fn emit_dyn_array_literal(&mut self, elem_type: &Type, elements: &[Expr]) -> LlvmVal {
        let header = self.emit_alloc_dyn_array_header();
        let elem_llvm = self.type_to_llvm(elem_type);
        let count = elements.len();
        let bytes = (count as u64)
            .saturating_mul(elem_type.llvm_size() as u64)
            .max(1);
        let data = self.fresh("array_storage");
        self.emit_indent(&format!(
            "{} = call ptr @malloc(i64 {})",
            self.l(&data),
            bytes
        ));

        self.emit_init_dyn_array_header(&header, count as i64, count as i64, &self.l(&data));

        for (i, elem) in elements.iter().enumerate() {
            let val = self.emit_expr(elem);
            let elem_ptr = self.fresh("array_elem");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds {}, ptr {}, i64 {}",
                self.l(&elem_ptr),
                elem_llvm,
                self.l(&data),
                i
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                elem_llvm,
                val.to_str(),
                self.l(&elem_ptr)
            ));
        }

        LlvmVal::Local(header)
    }

    fn emit_dyn_array_repeat(&mut self, elem_type: &Type, value: &Expr, count: &Expr) -> LlvmVal {
        let Expr::Int(count) = count else {
            return LlvmVal::Const("null".to_string());
        };
        let elements: Vec<Expr> = (0..(*count).max(0)).map(|_| value.clone()).collect();
        self.emit_dyn_array_literal(elem_type, &elements)
    }

    fn emit_init_dyn_array_header(&mut self, header: &str, len: i64, cap: i64, data: &str) {
        let header_val = LlvmVal::Local(header.to_string());
        let len_field = self.emit_dyn_array_field_ptr(&header_val, 0, "array_len_field");
        self.emit_indent(&format!("store i64 {}, ptr {}", len, self.l(&len_field)));
        let cap_field = self.emit_dyn_array_field_ptr(&header_val, 1, "array_cap_field");
        self.emit_indent(&format!("store i64 {}, ptr {}", cap, self.l(&cap_field)));
        let data_field = self.emit_dyn_array_field_ptr(&header_val, 2, "array_data_field");
        self.emit_indent(&format!("store ptr {}, ptr {}", data, self.l(&data_field)));
    }

    fn emit_dyn_array_push(&mut self, header: &LlvmVal, elem_type: &Type, value: &Expr) -> LlvmVal {
        let elem_llvm = self.type_to_llvm(elem_type);
        let len = self.emit_dyn_array_len(header);
        let old_data = self.emit_dyn_array_data_ptr(header);
        let new_len = self.fresh("array_new_len");
        self.emit_indent(&format!(
            "{} = add i64 {}, 1",
            self.l(&new_len),
            len.to_str()
        ));
        let byte_len = self.fresh("array_byte_len");
        self.emit_indent(&format!(
            "{} = mul i64 {}, {}",
            self.l(&byte_len),
            self.l(&new_len),
            elem_type.llvm_size()
        ));
        let new_data = self.fresh("array_new_data");
        self.emit_indent(&format!(
            "{} = call ptr @malloc(i64 {})",
            self.l(&new_data),
            self.l(&byte_len)
        ));
        let old_byte_len = self.fresh("array_old_byte_len");
        self.emit_indent(&format!(
            "{} = mul i64 {}, {}",
            self.l(&old_byte_len),
            len.to_str(),
            elem_type.llvm_size()
        ));
        self.emit_indent(&format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {}, ptr {}, i64 {}, i1 false)",
            self.l(&new_data),
            old_data,
            self.l(&old_byte_len)
        ));

        let value = self.emit_expr(value);
        let slot = self.fresh("array_push_slot");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds {}, ptr {}, i64 {}",
            self.l(&slot),
            elem_llvm,
            self.l(&new_data),
            len.to_str()
        ));
        self.emit_indent(&format!(
            "store {} {}, ptr {}",
            elem_llvm,
            value.to_str(),
            self.l(&slot)
        ));

        let new_header = self.emit_alloc_dyn_array_header();
        self.emit_init_dyn_array_header(&new_header, 0, 0, &self.l(&new_data));
        let new_header_val = LlvmVal::Local(new_header.clone());
        let len_field = self.emit_dyn_array_field_ptr(&new_header_val, 0, "array_len_field");
        self.emit_indent(&format!(
            "store i64 {}, ptr {}",
            self.l(&new_len),
            self.l(&len_field)
        ));
        let cap_field = self.emit_dyn_array_field_ptr(&new_header_val, 1, "array_cap_field");
        self.emit_indent(&format!(
            "store i64 {}, ptr {}",
            self.l(&new_len),
            self.l(&cap_field)
        ));

        LlvmVal::Local(new_header)
    }

    fn emit_struct_literal(&mut self, type_name: &str, fields: &[(String, Expr)]) -> LlvmVal {
        let struct_name = format!("%{}_struct", type_name);
        let alloca_name = self.fresh("struct");
        self.emit_indent(&format!(
            "{} = alloca {}",
            self.l(&alloca_name),
            struct_name
        ));

        for (i, (field_name, value)) in fields.iter().enumerate() {
            let val = self.emit_expr(value);
            let field_type = self.get_field_type(&Type::Named(type_name.to_string()), field_name);
            let field_ptr = self.fresh("sfptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                self.l(&field_ptr),
                struct_name,
                self.l(&alloca_name),
                i
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                self.type_to_llvm(&field_type),
                val.to_str(),
                self.l(&field_ptr)
            ));
        }

        // Load the struct value from the alloca so it can be used as a first-class value
        let load_name = self.fresh("sload");
        self.emit_indent(&format!(
            "{} = load {}, ptr {}",
            self.l(&load_name),
            struct_name,
            self.l(&alloca_name)
        ));
        LlvmVal::Local(load_name)
    }

    fn emit_enum_variant(
        &mut self,
        type_name: &str,
        variant: &str,
        value: &Option<Box<Expr>>,
    ) -> LlvmVal {
        let enum_name = format!("%{}_enum", type_name);
        let result_name = self.fresh("enum");
        self.emit_indent(&format!("{} = alloca {}", self.l(&result_name), enum_name));

        // Find the variant tag
        let tag = self.get_variant_tag(type_name, variant);
        let tag_ptr = self.fresh("etag_ptr");
        self.emit_indent(&format!(
            "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 0",
            self.l(&tag_ptr),
            enum_name,
            self.l(&result_name)
        ));
        self.emit_indent(&format!("store i32 {}, ptr {}", tag, self.l(&tag_ptr)));

        // Store the payload if present
        if let Some(val_expr) = value {
            let val = self.emit_expr(val_expr);
            let val_type = self.infer_type_of_expr(val_expr);
            let payload_ptr = self.fresh("epay_ptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 1",
                self.l(&payload_ptr),
                enum_name,
                self.l(&result_name)
            ));
            // Bitcast payload area to the correct type and store
            let payload_typed = self.fresh("epay_typed");
            self.emit_indent(&format!(
                "{} = bitcast ptr {} to ptr",
                self.l(&payload_typed),
                self.l(&payload_ptr)
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                self.type_to_llvm(&val_type),
                val.to_str(),
                self.l(&payload_typed)
            ));
        }

        LlvmVal::Local(result_name)
    }

    fn emit_array_literal(&mut self, elements: &[Expr]) -> LlvmVal {
        if elements.is_empty() {
            let result_name = self.fresh("arr");
            self.emit_indent(&format!("{} = alloca [0 x i32]", self.l(&result_name)));
            return LlvmVal::Local(result_name);
        }

        let elem_type = self.infer_type_of_expr(&elements[0]);
        let elem_llvm = self.type_to_llvm(&elem_type);
        let count = elements.len();
        let result_name = self.fresh("arr");
        self.emit_indent(&format!(
            "{} = alloca [{} x {}]",
            self.l(&result_name),
            count,
            elem_llvm
        ));

        for (i, elem) in elements.iter().enumerate() {
            let val = self.emit_expr(elem);
            let elem_ptr = self.fresh("aeptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds [{} x {}], ptr {}, i32 0, i32 {}",
                self.l(&elem_ptr),
                count,
                elem_llvm,
                self.l(&result_name),
                i
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                elem_llvm,
                val.to_str(),
                self.l(&elem_ptr)
            ));
        }

        LlvmVal::Local(result_name)
    }

    fn emit_repeat_literal(&mut self, value: &Expr, count: &Expr) -> LlvmVal {
        let Expr::Int(count) = count else {
            return LlvmVal::Const("null".to_string());
        };
        let count = (*count).max(0) as usize;
        let elem_type = self.infer_type_of_expr(value);
        let elem_llvm = self.type_to_llvm(&elem_type);
        let result_name = self.fresh("repeat");
        self.emit_indent(&format!(
            "{} = alloca [{} x {}]",
            self.l(&result_name),
            count,
            elem_llvm
        ));

        for i in 0..count {
            let val = self.emit_expr(value);
            let elem_ptr = self.fresh("repptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds [{} x {}], ptr {}, i32 0, i32 {}",
                self.l(&elem_ptr),
                count,
                elem_llvm,
                self.l(&result_name),
                i
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                elem_llvm,
                val.to_str(),
                self.l(&elem_ptr)
            ));
        }

        LlvmVal::Local(result_name)
    }

    fn emit_tuple_literal(&mut self, elements: &[Expr]) -> LlvmVal {
        let types: Vec<Type> = elements
            .iter()
            .map(|e| self.infer_type_of_expr(e))
            .collect();
        let llvm_types: Vec<String> = types.iter().map(|t| self.type_to_llvm(t)).collect();

        let result_name = self.fresh("tuple");
        self.emit_indent(&format!(
            "{} = alloca {{ {} }}",
            self.l(&result_name),
            llvm_types.join(", ")
        ));

        for (i, elem) in elements.iter().enumerate() {
            let val = self.emit_expr(elem);
            let field_ptr = self.fresh("tfptr");
            self.emit_indent(&format!(
                "{} = getelementptr inbounds {{ {} }}, ptr {}, i32 0, i32 {}",
                self.l(&field_ptr),
                llvm_types.join(", "),
                self.l(&result_name),
                i
            ));
            self.emit_indent(&format!(
                "store {} {}, ptr {}",
                llvm_types[i],
                val.to_str(),
                self.l(&field_ptr)
            ));
        }

        LlvmVal::Local(result_name)
    }

    // =========================================================================
    // Type conversion helpers
    // =========================================================================

    fn common_int_type(&self, left: &Type, right: &Type) -> Option<Type> {
        if !left.is_signed_int()
            && !left.is_unsigned_int()
            && !right.is_signed_int()
            && !right.is_unsigned_int()
        {
            return None;
        }

        if left.is_unsigned_int() || right.is_unsigned_int() {
            return Some(Type::U64);
        }

        Some(Type::I64)
    }

    fn coerce_int_value(&mut self, value: &LlvmVal, from: &Type, to: &Type) -> String {
        if from == to || !from.is_numeric() || !to.is_numeric() {
            return value.to_str();
        }

        let from_size = from.llvm_size();
        let to_size = to.llvm_size();
        if from_size == to_size {
            return value.to_str();
        }

        let cast = self.fresh("int_cast");
        let op = if from_size < to_size {
            if from.is_unsigned_int() {
                "zext"
            } else {
                "sext"
            }
        } else {
            "trunc"
        };
        self.emit_indent(&format!(
            "{} = {} {} {} to {}",
            self.l(&cast),
            op,
            self.type_to_llvm(from),
            value.to_str(),
            self.type_to_llvm(to)
        ));
        self.l(&cast)
    }

    fn type_to_llvm(&self, ty: &Type) -> String {
        match ty {
            Type::I8 => "i8".to_string(),
            Type::I16 => "i16".to_string(),
            Type::I32 => "i32".to_string(),
            Type::I64 => "i64".to_string(),
            Type::I128 => "i128".to_string(),
            Type::U8 => "i8".to_string(),
            Type::U16 => "i16".to_string(),
            Type::U32 => "i32".to_string(),
            Type::U64 => "i64".to_string(),
            Type::U128 => "i128".to_string(),
            Type::F16 => "half".to_string(),
            Type::F32 => "float".to_string(),
            Type::F64 => "double".to_string(),
            Type::F128 => "fp128".to_string(),
            Type::Bool => "i1".to_string(),
            Type::Char => "i32".to_string(),
            Type::Str => "ptr".to_string(),
            Type::Unit => "void".to_string(),
            Type::Named(name) => format!("%{}_struct", name),
            Type::Array { element, size } => format!("[{} x {}]", size, self.type_to_llvm(element)),
            Type::DynArray(_) => "ptr".to_string(),
            Type::Tuple(_) => "ptr".to_string(),
            Type::Map { .. } => "ptr".to_string(),
            Type::Set(_) => "ptr".to_string(),
            Type::Fn { .. } => "ptr".to_string(),
            Type::Option(_) => "ptr".to_string(),
            Type::Result { .. } => "ptr".to_string(),
            Type::Var(_) => "ptr".to_string(),
            Type::Generic { name, .. } => format!("%{}_struct", name),
            Type::Unknown => "ptr".to_string(),
        }
    }

    fn type_size(&self, ty: &Type) -> u64 {
        match ty {
            Type::I8 | Type::U8 | Type::Bool => 1,
            Type::I16 | Type::U16 => 2,
            Type::I32 | Type::U32 | Type::F32 | Type::Char => 4,
            Type::I64 | Type::U64 | Type::F64 => 8,
            Type::I128 | Type::U128 | Type::F128 => 16,
            Type::Str => 8,
            Type::Named(name) => {
                if let Some(TypeInfo::Def { fields, .. }) = self.env.lookup_type(name) {
                    fields.iter().map(|(_, t)| self.type_size(t)).sum()
                } else {
                    8
                }
            }
            _ => 8,
        }
    }

    fn get_field_index(&self, ty: &Type, field: &str) -> usize {
        if let Type::Named(name) = ty {
            if let Some(TypeInfo::Def { fields, .. }) = self.env.lookup_type(name) {
                for (i, (f_name, _)) in fields.iter().enumerate() {
                    if f_name == field {
                        return i;
                    }
                }
            }
        }
        0
    }

    fn get_field_type(&self, ty: &Type, field: &str) -> Type {
        if let Type::Named(name) = ty {
            if let Some(TypeInfo::Def { fields, .. }) = self.env.lookup_type(name) {
                for (f_name, f_type) in fields {
                    if f_name == field {
                        return f_type.clone();
                    }
                }
            }
        }
        Type::I32
    }

    fn get_variant_tag(&self, type_name: &str, variant_name: &str) -> i32 {
        if let Some(TypeInfo::Enum { variants, .. }) = self.env.lookup_type(type_name) {
            for (i, (name, _)) in variants.iter().enumerate() {
                if name == variant_name {
                    return i as i32;
                }
            }
        }
        0
    }

    fn resolve_field_type(&self, field: &FieldDef) -> Type {
        self.resolve_type_expr_simple(&field.type_ann)
    }

    fn resolve_type_expr_simple(&self, expr: &TypeExpr) -> Type {
        match expr {
            TypeExpr::Named(name) => match name.as_str() {
                "i8" => Type::I8,
                "i16" => Type::I16,
                "i32" => Type::I32,
                "i64" => Type::I64,
                "i128" => Type::I128,
                "u8" => Type::U8,
                "u16" => Type::U16,
                "u32" => Type::U32,
                "u64" => Type::U64,
                "u128" => Type::U128,
                "f16" => Type::F16,
                "f32" => Type::F32,
                "f64" => Type::F64,
                "f128" => Type::F128,
                "bool" => Type::Bool,
                "char" => Type::Char,
                "str" => Type::Str,
                _ => Type::Named(name.clone()),
            },
            TypeExpr::Unit => Type::Unit,
            TypeExpr::SelfType => Type::Named("Self".to_string()),
            TypeExpr::Generic { name, args } => {
                let resolved: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_expr_simple(a))
                    .collect();
                vita_std::resolve_generic_type(name, &resolved)
                    .and_then(|result| result.ok())
                    .unwrap_or(Type::Generic {
                        name: name.clone(),
                        args: resolved,
                    })
            }
            TypeExpr::Array { element, size } => {
                let elem = self.resolve_type_expr_simple(element);
                match size {
                    Some(s) => Type::Array {
                        element: Box::new(elem),
                        size: *s,
                    },
                    None => Type::DynArray(Box::new(elem)),
                }
            }
            TypeExpr::Tuple(types) => Type::Tuple(
                types
                    .iter()
                    .map(|t| self.resolve_type_expr_simple(t))
                    .collect(),
            ),
            _ => Type::Unknown,
        }
    }

    fn infer_type_of_expr(&self, expr: &Expr) -> Type {
        match expr {
            Expr::Int(_) => Type::I32,
            Expr::Float(_) => Type::F32,
            Expr::Bool(_) => Type::Bool,
            Expr::String(_) => Type::Str,
            Expr::Char(_) => Type::Char,
            Expr::Unit => Type::Unit,
            Expr::Ident(name) => self.env.lookup_var(name).unwrap_or(Type::I32),
            Expr::Binary { op, left, .. } => match op {
                BinOp::Eq
                | BinOp::Neq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or => Type::Bool,
                _ => self.infer_type_of_expr(left),
            },
            Expr::Unary { op, operand } => match op {
                UnOp::Not => Type::Bool,
                UnOp::Neg => self.infer_type_of_expr(operand),
            },
            Expr::StructLiteral { type_name, .. } => Type::Named(type_name.clone()),
            Expr::EnumVariant { type_name, .. } => Type::Named(type_name.clone()),
            Expr::FieldAccess { object, field } => {
                let obj_type = self.infer_type_of_expr(object);
                self.get_field_type(&obj_type, field)
            }
            Expr::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    Type::DynArray(Box::new(Type::I32))
                } else {
                    Type::DynArray(Box::new(self.infer_type_of_expr(&elems[0])))
                }
            }
            Expr::RangeLiteral { .. } => Type::Generic {
                name: "Range".to_string(),
                args: vec![Type::I64],
            },
            Expr::RepeatLiteral { value, .. } => {
                Type::DynArray(Box::new(self.infer_type_of_expr(value)))
            }
            Expr::Index { object, .. } => {
                let object_type = self.infer_type_of_expr(object);
                match object_type {
                    Type::Array { element, .. } | Type::DynArray(element) => *element,
                    Type::Str => Type::Char,
                    _ => Type::I32,
                }
            }
            Expr::TupleLiteral(elems) => {
                Type::Tuple(elems.iter().map(|e| self.infer_type_of_expr(e)).collect())
            }
            Expr::Call { func, .. } => {
                if let Expr::Ident(name) = func.as_ref() {
                    self.env
                        .lookup_fn(name)
                        .and_then(|fns| fns.first())
                        .map(|f| f.return_type.clone())
                        .unwrap_or(Type::I32)
                } else {
                    Type::I32
                }
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                let receiver_type = self.infer_type_of_expr(receiver);
                if matches!(receiver_type, Type::DynArray(_)) {
                    return match method.as_str() {
                        "len" => Type::I64,
                        "push" => receiver_type,
                        _ => Type::I32,
                    };
                }
                self.env
                    .lookup_fn(method)
                    .and_then(|fns| {
                        fns.iter()
                            .find(|f| f.receiver_type.as_ref() == Some(&receiver_type))
                    })
                    .map(|f| f.return_type.clone())
                    .unwrap_or(Type::I32)
            }
            Expr::If {
                then_block,
                else_block,
                ..
            } => {
                if let Some(tail) = &then_block.tail {
                    self.infer_type_of_expr(tail)
                } else if let Some(else_b) = else_block {
                    if let Some(tail) = &else_b.tail {
                        self.infer_type_of_expr(tail)
                    } else {
                        Type::Unit
                    }
                } else {
                    Type::Unit
                }
            }
            Expr::Match { arms, .. } => arms
                .first()
                .map(|arm| self.infer_type_of_expr(&arm.body))
                .unwrap_or(Type::Unit),
            _ => Type::I32,
        }
    }
}
