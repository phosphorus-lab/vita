//! Type checker for the Vita language.
//!
//! Performs semantic analysis: name resolution, type checking, spec verification.

use crate::diagnostics::error::{CompileError, ErrorKind, Result, Span};
use crate::semantics::types::{FnInfo, Type, TypeEnv, TypeInfo};
use crate::syntax::ast::*;

/// The type checker. Walks the AST and validates types.
pub struct TypeChecker {
    env: TypeEnv,
    errors: Vec<CompileError>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: TypeEnv::new(),
            errors: Vec::new(),
        }
    }

    /// Check a list of top-level items. Returns the populated type environment.
    pub fn check(&mut self, items: &[Item]) -> Result<TypeEnv> {
        // First pass: register all type and function names
        for item in items {
            self.register_item(item)?;
        }

        // Second pass: verify spec implementations
        for item in items {
            if let Item::Impl(impl_item) = item {
                if let Some(spec_name) = &impl_item.spec_name {
                    self.verify_spec_impl(&impl_item.target_type, spec_name)?;
                }
            }
        }

        // Third pass: register local variables from function bodies
        for item in items {
            self.register_local_vars(item);
        }

        if self.errors.is_empty() {
            Ok(self.env.clone())
        } else {
            Err(self.errors.remove(0))
        }
    }

    fn register_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::Def(def) => {
                let fields: Vec<(String, Type)> = def
                    .fields
                    .iter()
                    .map(|f| Ok((f.name.clone(), self.resolve_type_expr(&f.type_ann)?)))
                    .collect::<Result<_>>()?;

                self.env.define_type(
                    def.name.clone(),
                    TypeInfo::Def {
                        name: def.name.clone(),
                        generics: def.generics.clone(),
                        fields,
                    },
                );
            }
            Item::Enum(enum_item) => {
                let variants: Vec<(String, Option<Type>)> = enum_item
                    .variants
                    .iter()
                    .map(|v| {
                        Ok((
                            v.name.clone(),
                            v.payload
                                .as_ref()
                                .map(|t| self.resolve_type_expr(t))
                                .transpose()?,
                        ))
                    })
                    .collect::<Result<_>>()?;

                self.env.define_type(
                    enum_item.name.clone(),
                    TypeInfo::Enum {
                        name: enum_item.name.clone(),
                        generics: enum_item.generics.clone(),
                        variants,
                    },
                );
            }
            Item::Spec(spec) => {
                let mut fields = Vec::new();
                let mut methods = Vec::new();

                for member in &spec.members {
                    match member {
                        SpecMember::Field { name, type_ann } => {
                            fields.push((name.clone(), self.resolve_type_expr(type_ann)?));
                        }
                        SpecMember::Fn {
                            name,
                            params,
                            return_type,
                            ..
                        } => {
                            let param_types: Vec<Type> = params
                                .iter()
                                .map(|p| self.resolve_type_expr(&p.type_ann))
                                .collect::<Result<_>>()?;
                            let ret = return_type
                                .as_ref()
                                .map(|t| self.resolve_type_expr(t))
                                .transpose()?
                                .unwrap_or(Type::Unit);
                            methods.push((name.clone(), param_types, Some(ret)));
                        }
                    }
                }

                self.env.define_type(
                    spec.name.clone(),
                    TypeInfo::Spec {
                        name: spec.name.clone(),
                        fields,
                        methods,
                    },
                );
            }
            Item::Impl(impl_item) => {
                let target_type = Type::Named(impl_item.target_type.clone());
                for method in &impl_item.methods {
                    let param_types: Vec<Type> = method
                        .params
                        .iter()
                        .map(|p| {
                            let t = self.resolve_type_expr(&p.type_ann)?;
                            // Replace Self with the actual target type
                            Ok(if t == Type::Named("Self".to_string()) {
                                target_type.clone()
                            } else {
                                t
                            })
                        })
                        .collect::<Result<_>>()?;
                    let ret = method
                        .return_type
                        .as_ref()
                        .map(|t| {
                            let resolved = self.resolve_type_expr(t)?;
                            Ok(if resolved == Type::Named("Self".to_string()) {
                                target_type.clone()
                            } else {
                                resolved
                            })
                        })
                        .transpose()?
                        .unwrap_or(Type::Unit);

                    self.env.define_fn(
                        method.name.clone(),
                        FnInfo {
                            name: method.name.clone(),
                            params: method
                                .params
                                .iter()
                                .zip(param_types.iter())
                                .map(|(p, t)| (p.name.clone(), t.clone()))
                                .collect(),
                            return_type: ret,
                            is_pub: method.is_pub,
                            receiver_type: Some(target_type.clone()),
                        },
                    );
                }
            }
            Item::Fn(fn_item) => {
                let param_types: Vec<Type> = fn_item
                    .params
                    .iter()
                    .map(|p| self.resolve_type_expr(&p.type_ann))
                    .collect::<Result<_>>()?;
                let ret = fn_item
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_expr(t))
                    .transpose()?
                    .unwrap_or(Type::Unit);

                self.env.define_fn(
                    fn_item.name.clone(),
                    FnInfo {
                        name: fn_item.name.clone(),
                        params: fn_item
                            .params
                            .iter()
                            .zip(param_types.iter())
                            .map(|(p, t)| (p.name.clone(), t.clone()))
                            .collect(),
                        return_type: ret,
                        is_pub: fn_item.is_pub,
                        receiver_type: None,
                    },
                );
            }
            Item::Use(_) => {
                // Use items are handled at the module level; skip for now
            }
        }
        Ok(())
    }

    fn register_local_vars(&mut self, item: &Item) {
        match item {
            Item::Fn(fn_item) => {
                // Register function parameters as variables
                for (p_name, p_type) in self.get_fn_param_types(fn_item) {
                    self.env.define_var(p_name, p_type);
                }
                // Walk the body to register let bindings
                self.register_block_vars(&fn_item.body);
            }
            Item::Impl(impl_item) => {
                for method in &impl_item.methods {
                    let target_type = Type::Named(impl_item.target_type.clone());
                    // Register method parameters
                    for p in &method.params {
                        let p_type = if p.type_ann == TypeExpr::SelfType {
                            target_type.clone()
                        } else {
                            self.resolve_type_expr(&p.type_ann).unwrap_or(Type::Unknown)
                        };
                        self.env.define_var(p.name.clone(), p_type);
                    }
                    self.register_block_vars(&method.body);
                }
            }
            _ => {}
        }
    }

    fn get_fn_param_types(&self, fn_item: &FnItem) -> Vec<(String, Type)> {
        fn_item
            .params
            .iter()
            .map(|p| {
                let t = self.resolve_type_expr(&p.type_ann).unwrap_or(Type::Unknown);
                (p.name.clone(), t)
            })
            .collect()
    }

    fn register_block_vars(&mut self, block: &Block) {
        for stmt in &block.stmts {
            if let Stmt::Let {
                name,
                type_ann,
                value,
            } = stmt
            {
                let ty = type_ann
                    .as_ref()
                    .and_then(|t| self.resolve_type_expr(t).ok())
                    .unwrap_or_else(|| self.infer_expr_type(value));
                self.env.define_var(name.clone(), ty);
            }
        }
        // Also register the tail if it's a let-like pattern
    }

    fn verify_spec_impl(&self, type_name: &str, spec_name: &str) -> Result<()> {
        let spec_info = self.env.lookup_type(spec_name).ok_or_else(|| {
            CompileError::new(ErrorKind::UnknownSpec(spec_name.to_string()), Span::zero())
        })?;

        let type_info = self.env.lookup_type(type_name);

        if let TypeInfo::Spec {
            fields, methods, ..
        } = &spec_info
        {
            // Check that the type has all required fields
            if let Some(TypeInfo::Def {
                fields: type_fields,
                ..
            }) = &type_info
            {
                for (field_name, _field_type) in fields {
                    let has_field = type_fields.iter().any(|(n, _)| n == field_name);
                    if !has_field {
                        return Err(CompileError::new(
                            ErrorKind::MissingField {
                                type_name: type_name.to_string(),
                                field: field_name.clone(),
                            },
                            Span::zero(),
                        ));
                    }
                }
            }

            // Check that all required methods are implemented
            for (method_name, _param_types, _ret_type) in methods {
                let fns = self.env.lookup_fn(method_name);
                let has_method = fns.is_some_and(|fns| {
                    fns.iter().any(|f| {
                        f.receiver_type.as_ref() == Some(&Type::Named(type_name.to_string()))
                    })
                });
                if !has_method {
                    return Err(CompileError::new(
                        ErrorKind::MissingMethod {
                            type_name: type_name.to_string(),
                            method: method_name.clone(),
                            spec: spec_name.to_string(),
                        },
                        Span::zero(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Resolve a type expression from the AST to a concrete Type.
    pub fn resolve_type_expr(&self, expr: &TypeExpr) -> Result<Type> {
        match expr {
            TypeExpr::Named(name) => self.resolve_named_type(name),
            TypeExpr::Generic { name, args } => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_expr(a))
                    .collect::<Result<_>>()?;

                // Handle known generic types
                match name.as_str() {
                    "Array" if resolved_args.len() == 1 => {
                        Ok(Type::DynArray(Box::new(resolved_args[0].clone())))
                    }
                    "Option" if resolved_args.len() == 1 => {
                        Ok(Type::Option(Box::new(resolved_args[0].clone())))
                    }
                    "Result" if resolved_args.len() == 2 => Ok(Type::Result {
                        ok: Box::new(resolved_args[0].clone()),
                        err: Box::new(resolved_args[1].clone()),
                    }),
                    "Map" if resolved_args.len() == 2 => Ok(Type::Map {
                        key: Box::new(resolved_args[0].clone()),
                        value: Box::new(resolved_args[1].clone()),
                    }),
                    "Set" if resolved_args.len() == 1 => {
                        Ok(Type::Set(Box::new(resolved_args[0].clone())))
                    }
                    _ => Ok(Type::Generic {
                        name: name.clone(),
                        args: resolved_args,
                    }),
                }
            }
            TypeExpr::Array { element, size } => {
                let elem = self.resolve_type_expr(element)?;
                match size {
                    Some(_) => {
                        // Fixed-size array: we'll use a default size for now
                        Ok(Type::Array {
                            element: Box::new(elem),
                            size: 0,
                        })
                    }
                    None => Ok(Type::DynArray(Box::new(elem))),
                }
            }
            TypeExpr::Tuple(types) => {
                let resolved: Vec<Type> = types
                    .iter()
                    .map(|t| self.resolve_type_expr(t))
                    .collect::<Result<_>>()?;
                Ok(Type::Tuple(resolved))
            }
            TypeExpr::Fn { params, ret } => {
                let param_types: Vec<Type> = params
                    .iter()
                    .map(|t| self.resolve_type_expr(t))
                    .collect::<Result<_>>()?;
                let ret_type = self.resolve_type_expr(ret)?;
                Ok(Type::Fn {
                    params: param_types,
                    ret: Box::new(ret_type),
                })
            }
            TypeExpr::SelfType => {
                // Try to resolve to the current impl target type
                // For now, we look for "Self" which should be replaced by context
                Ok(Type::Named("Self".to_string()))
            }
            TypeExpr::Unit => Ok(Type::Unit),
        }
    }

    fn resolve_named_type(&self, name: &str) -> Result<Type> {
        match name {
            "i8" => Ok(Type::I8),
            "i16" => Ok(Type::I16),
            "i32" => Ok(Type::I32),
            "i64" => Ok(Type::I64),
            "i128" => Ok(Type::I128),
            "u8" => Ok(Type::U8),
            "u16" => Ok(Type::U16),
            "u32" => Ok(Type::U32),
            "u64" => Ok(Type::U64),
            "u128" => Ok(Type::U128),
            "f16" => Ok(Type::F16),
            "f32" => Ok(Type::F32),
            "f64" => Ok(Type::F64),
            "f128" => Ok(Type::F128),
            "bool" => Ok(Type::Bool),
            "char" => Ok(Type::Char),
            "str" => Ok(Type::Str),
            "()" => Ok(Type::Unit),
            _ => {
                if self.env.lookup_type(name).is_some() {
                    Ok(Type::Named(name.to_string()))
                } else {
                    Err(CompileError::new(
                        ErrorKind::UndefinedType(name.to_string()),
                        Span::zero(),
                    ))
                }
            }
        }
    }

    /// Infer the type of an expression.
    pub fn infer_expr_type(&self, expr: &Expr) -> Type {
        match expr {
            Expr::Int(_) => Type::I32,
            Expr::Float(_) => Type::F32,
            Expr::Bool(_) => Type::Bool,
            Expr::String(_) => Type::Str,
            Expr::Char(_) => Type::Char,
            Expr::Unit => Type::Unit,
            Expr::Ident(name) => self.env.lookup_var(name).unwrap_or(Type::Unknown),
            Expr::Binary { op, left, right } => {
                let lt = self.infer_expr_type(left);
                let rt = self.infer_expr_type(right);
                match op {
                    BinOp::Eq
                    | BinOp::Neq
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::LtEq
                    | BinOp::GtEq
                    | BinOp::And
                    | BinOp::Or => Type::Bool,
                    _ => {
                        if lt.is_numeric() {
                            lt
                        } else if rt.is_numeric() {
                            rt
                        } else {
                            Type::Unknown
                        }
                    }
                }
            }
            Expr::Unary { op, operand } => {
                let inner = self.infer_expr_type(operand);
                match op {
                    UnOp::Not => Type::Bool,
                    UnOp::Neg => inner,
                }
            }
            Expr::Call { func, args: _ } => {
                if let Expr::Ident(name) = func.as_ref() {
                    if let Some(fns) = self.env.lookup_fn(name) {
                        if let Some(first) = fns.first() {
                            return first.return_type.clone();
                        }
                    }
                }
                Type::Unknown
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                if let Some(fns) = self.env.lookup_fn(method) {
                    let receiver_type = self.infer_expr_type(receiver);
                    for f in fns {
                        if f.receiver_type.as_ref() == Some(&receiver_type) {
                            return f.return_type.clone();
                        }
                    }
                }
                Type::Unknown
            }
            Expr::If {
                then_block,
                else_block,
                ..
            } => {
                if let Some(tail) = &then_block.tail {
                    self.infer_expr_type(tail)
                } else if let Some(else_b) = else_block {
                    if let Some(tail) = &else_b.tail {
                        self.infer_expr_type(tail)
                    } else {
                        Type::Unit
                    }
                } else {
                    Type::Unit
                }
            }
            Expr::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    Type::DynArray(Box::new(Type::Unknown))
                } else {
                    let elem_type = self.infer_expr_type(&elements[0]);
                    Type::Array {
                        element: Box::new(elem_type),
                        size: elements.len(),
                    }
                }
            }
            Expr::StructLiteral { type_name, .. } => Type::Named(type_name.clone()),
            Expr::EnumVariant { type_name, .. } => Type::Named(type_name.clone()),
            Expr::TupleLiteral(elements) => {
                Type::Tuple(elements.iter().map(|e| self.infer_expr_type(e)).collect())
            }
            _ => Type::Unknown,
        }
    }
}
