//! Type checker for the Vita language.
//!
//! Performs semantic analysis: name resolution, type checking, spec verification.

use crate::diagnostics::error::{CompileError, ErrorKind, Result, Span};
use crate::semantics::std as vita_std;
use crate::semantics::types::{FnInfo, Type, TypeEnv, TypeInfo};
use crate::syntax::ast::*;

fn explicit_method_param_count(function: &FnInfo) -> usize {
    function.params.len()
        - usize::from(
            function
                .params
                .first()
                .is_some_and(|(name, _)| name == "self"),
        )
}

fn receiver_types_match(expected: &Type, got: &Type) -> bool {
    match (expected, got) {
        (Type::DynArray(expected_elem), Type::DynArray(_))
            if matches!(**expected_elem, Type::Var(_)) =>
        {
            true
        }
        _ => expected == got,
    }
}

fn specialize_method_type(ty: &Type, receiver: &Type) -> Type {
    match (ty, receiver) {
        (Type::DynArray(elem), Type::DynArray(actual)) if matches!(**elem, Type::Var(_)) => {
            Type::DynArray(actual.clone())
        }
        (Type::Var(_), Type::DynArray(actual)) => *actual.clone(),
        _ => ty.clone(),
    }
}

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

        // Third pass: validate function and method bodies in scoped environments.
        for item in items {
            self.check_item_body(item)?;
        }

        // Fourth pass: register local variables from function bodies.
        //
        // Codegen still uses the populated top-level env for expression type
        // inference, so keep this compatibility pass until the backend owns
        // its own scoped locals.
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

    fn check_item_body(&self, item: &Item) -> Result<()> {
        match item {
            Item::Fn(fn_item) => {
                let fn_info = self
                    .env
                    .lookup_fn(&fn_item.name)
                    .and_then(|fns| fns.iter().find(|f| f.receiver_type.is_none()))
                    .ok_or_else(|| {
                        CompileError::new(
                            ErrorKind::UndefinedName(fn_item.name.clone()),
                            Span::zero(),
                        )
                    })?;
                self.check_fn_body(fn_item, fn_info.return_type.clone(), None)
            }
            Item::Impl(impl_item) => {
                let receiver_type = Type::Named(impl_item.target_type.clone());
                for method in &impl_item.methods {
                    let fn_info = self
                        .env
                        .lookup_fn(&method.name)
                        .and_then(|fns| {
                            fns.iter()
                                .find(|f| f.receiver_type.as_ref() == Some(&receiver_type))
                        })
                        .ok_or_else(|| {
                            CompileError::new(
                                ErrorKind::UndefinedName(method.name.clone()),
                                Span::zero(),
                            )
                        })?;
                    self.check_fn_body(
                        method,
                        fn_info.return_type.clone(),
                        Some(receiver_type.clone()),
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_fn_body(
        &self,
        fn_item: &FnItem,
        expected_return: Type,
        receiver_type: Option<Type>,
    ) -> Result<()> {
        let mut local_env = self.env.child();

        for param in &fn_item.params {
            let param_type = if param.type_ann == TypeExpr::SelfType {
                receiver_type
                    .clone()
                    .unwrap_or(Type::Named("Self".to_string()))
            } else {
                self.resolve_type_expr(&param.type_ann)?
            };

            if let Some(default) = &param.default {
                let default_type = self.infer_expr_type_in_env(&local_env, default)?;
                self.expect_type(&param_type, &default_type)?;
            }

            local_env.define_var(param.name.clone(), param_type);
        }

        let body_type = self.check_block(&mut local_env, &fn_item.body, Some(&expected_return))?;
        self.expect_type(&expected_return, &body_type)
    }

    fn check_block(
        &self,
        env: &mut TypeEnv,
        block: &Block,
        expected_return: Option<&Type>,
    ) -> Result<Type> {
        let mut has_explicit_return = false;

        for stmt in &block.stmts {
            if self.check_stmt(env, stmt, expected_return)? {
                has_explicit_return = true;
            }
        }

        if let Some(tail) = &block.tail {
            self.infer_expr_type_in_env(env, tail)
        } else if has_explicit_return {
            Ok(expected_return.cloned().unwrap_or(Type::Unit))
        } else {
            Ok(Type::Unit)
        }
    }

    fn check_stmt(
        &self,
        env: &mut TypeEnv,
        stmt: &Stmt,
        expected_return: Option<&Type>,
    ) -> Result<bool> {
        match stmt {
            Stmt::Let {
                name,
                type_ann,
                value,
            } => {
                let value_type = self.infer_expr_type_in_env(env, value)?;
                let binding_type = if let Some(type_ann) = type_ann {
                    let annotated = self.resolve_type_expr(type_ann)?;
                    self.expect_type(&annotated, &value_type)?;
                    annotated
                } else {
                    value_type
                };
                env.define_var(name.clone(), binding_type);
                Ok(false)
            }
            Stmt::Expr(expr) | Stmt::SemiExpr(expr) => {
                self.infer_expr_type_in_env(env, expr)?;
                Ok(false)
            }
            Stmt::Break | Stmt::Continue => Ok(false),
            Stmt::Return(expr) => {
                let return_type = if let Some(expr) = expr {
                    self.infer_expr_type_in_env(env, expr)?
                } else {
                    Type::Unit
                };
                if let Some(expected) = expected_return {
                    self.expect_type(expected, &return_type)?;
                }
                Ok(true)
            }
        }
    }

    fn infer_expr_type_in_env(&self, env: &TypeEnv, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::Int(_) => Ok(Type::I32),
            Expr::Float(_) => Ok(Type::F32),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::Str),
            Expr::Char(_) => Ok(Type::Char),
            Expr::Unit => Ok(Type::Unit),
            Expr::Ident(name) => env.lookup_var(name).ok_or_else(|| {
                CompileError::new(ErrorKind::UndefinedName(name.clone()), Span::zero())
            }),
            Expr::Grouped(inner) => self.infer_expr_type_in_env(env, inner),
            Expr::Unary { op, operand } => {
                let operand_type = self.infer_expr_type_in_env(env, operand)?;
                match op {
                    UnOp::Not => {
                        self.expect_type(&Type::Bool, &operand_type)?;
                        Ok(Type::Bool)
                    }
                    UnOp::Neg => {
                        if operand_type.is_numeric() {
                            Ok(operand_type)
                        } else {
                            Err(self.type_mismatch("numeric", &operand_type))
                        }
                    }
                }
            }
            Expr::Binary { op, left, right } => {
                let left_type = self.infer_expr_type_in_env(env, left)?;
                let right_type = self.infer_expr_type_in_env(env, right)?;
                self.check_binary_type(*op, &left_type, &right_type)
            }
            Expr::Assign { target, value } => {
                let target_type = self.infer_assignment_target_type(env, target)?;
                let value_type = self.infer_expr_type_in_env(env, value)?;
                self.expect_type(&target_type, &value_type)?;
                Ok(Type::Unit)
            }
            Expr::Call { func, args } => self.infer_call_type(env, func, args),
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.infer_method_call_type(env, receiver, method, args),
            Expr::FieldAccess { object, field } => {
                let object_type = self.infer_expr_type_in_env(env, object)?;
                self.lookup_field_type(&object_type, field)
            }
            Expr::TupleAccess { tuple, index } => {
                let tuple_type = self.infer_expr_type_in_env(env, tuple)?;
                match tuple_type {
                    Type::Tuple(types) => types.get(*index).cloned().ok_or_else(|| {
                        CompileError::new(
                            ErrorKind::UndefinedName(format!("tuple field {}", index)),
                            Span::zero(),
                        )
                    }),
                    got => Err(self.type_mismatch("tuple", &got)),
                }
            }
            Expr::Index { object, index } => {
                let object_type = self.infer_expr_type_in_env(env, object)?;
                let index_type = self.infer_expr_type_in_env(env, index)?;
                if !index_type.is_signed_int() && !index_type.is_unsigned_int() {
                    return Err(self.type_mismatch("integer index", &index_type));
                }
                match object_type {
                    Type::Array { element, .. } | Type::DynArray(element) => Ok(*element),
                    Type::Str => Ok(Type::Char),
                    got => Err(self.type_mismatch("array", &got)),
                }
            }
            Expr::If {
                condition,
                then_block,
                else_if_clauses,
                else_block,
            } => {
                let condition_type = self.infer_expr_type_in_env(env, condition)?;
                self.expect_type(&Type::Bool, &condition_type)?;

                let then_type = self.check_block(&mut env.child(), then_block, None)?;
                for (else_if_condition, else_if_block) in else_if_clauses {
                    let condition_type = self.infer_expr_type_in_env(env, else_if_condition)?;
                    self.expect_type(&Type::Bool, &condition_type)?;
                    let else_if_type = self.check_block(&mut env.child(), else_if_block, None)?;
                    self.expect_type(&then_type, &else_if_type)?;
                }

                if let Some(else_block) = else_block {
                    let else_type = self.check_block(&mut env.child(), else_block, None)?;
                    self.expect_type(&then_type, &else_type)?;
                    Ok(then_type)
                } else {
                    Ok(Type::Unit)
                }
            }
            Expr::Match { subject, arms } => {
                self.infer_expr_type_in_env(env, subject)?;
                let mut arm_type = None;
                for arm in arms {
                    let ty = self.infer_expr_type_in_env(env, &arm.body)?;
                    if let Some(existing) = &arm_type {
                        self.expect_type(existing, &ty)?;
                    } else {
                        arm_type = Some(ty);
                    }
                }
                Ok(arm_type.unwrap_or(Type::Unit))
            }
            Expr::Loop(body) => {
                self.check_block(&mut env.child(), body, None)?;
                Ok(Type::Unit)
            }
            Expr::While { condition, body } => {
                let condition_type = self.infer_expr_type_in_env(env, condition)?;
                self.expect_type(&Type::Bool, &condition_type)?;
                self.check_block(&mut env.child(), body, None)?;
                Ok(Type::Unit)
            }
            Expr::ForEach {
                var,
                type_ann,
                iterable,
                body,
            } => {
                let iterable_type = self.infer_expr_type_in_env(env, iterable)?;
                let element_type = match iterable_type {
                    Type::Array { element, .. } | Type::DynArray(element) => *element,
                    Type::Generic { name, .. } if name == "Range" => type_ann
                        .as_ref()
                        .map(|type_ann| self.resolve_type_expr(type_ann))
                        .transpose()?
                        .unwrap_or(Type::I64),
                    Type::Set(element) => *element,
                    got => return Err(self.type_mismatch("iterable", &got)),
                };
                if let Some(type_ann) = type_ann {
                    let annotated = self.resolve_type_expr(type_ann)?;
                    if matches!(iterable.as_ref(), Expr::RangeLiteral { .. }) {
                        if !annotated.is_signed_int() && !annotated.is_unsigned_int() {
                            return Err(self.type_mismatch("integer range variable", &annotated));
                        }
                    } else {
                        self.expect_type(&annotated, &element_type)?;
                    }
                }
                let mut loop_env = env.child();
                loop_env.define_var(var.clone(), element_type);
                self.check_block(&mut loop_env, body, None)?;
                Ok(Type::Unit)
            }
            Expr::Fallible { block, handler } => {
                let block_type = self.check_block(&mut env.child(), block, None)?;
                if let Some(handler) = handler {
                    match handler {
                        FallibleHandler::Catch { err_name, body } => {
                            let mut handler_env = env.child();
                            handler_env.define_var(err_name.clone(), Type::Unknown);
                            let handler_type = self.check_block(&mut handler_env, body, None)?;
                            self.expect_type(&block_type, &handler_type)?;
                        }
                        FallibleHandler::CatchMatch { err_name, arms } => {
                            let mut handler_env = env.child();
                            handler_env.define_var(err_name.clone(), Type::Unknown);
                            for arm in arms {
                                let arm_type =
                                    self.infer_expr_type_in_env(&handler_env, &arm.body)?;
                                self.expect_type(&block_type, &arm_type)?;
                            }
                        }
                    }
                }
                Ok(block_type)
            }
            Expr::StructLiteral { type_name, fields } => {
                self.check_struct_literal(env, type_name, fields)?;
                Ok(Type::Named(type_name.clone()))
            }
            Expr::EnumVariant {
                type_name,
                variant,
                value,
            } => {
                self.check_enum_variant(env, type_name, variant, value.as_deref())?;
                Ok(Type::Named(type_name.clone()))
            }
            Expr::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    return Ok(Type::DynArray(Box::new(Type::Unknown)));
                }
                let first_type = self.infer_expr_type_in_env(env, &elements[0])?;
                for element in &elements[1..] {
                    let element_type = self.infer_expr_type_in_env(env, element)?;
                    self.expect_type(&first_type, &element_type)?;
                }
                Ok(Type::Array {
                    element: Box::new(first_type),
                    size: elements.len(),
                })
            }
            Expr::RangeLiteral { start, end } => {
                let start_type = self.infer_expr_type_in_env(env, start)?;
                let end_type = self.infer_expr_type_in_env(env, end)?;
                if !start_type.is_signed_int() && !start_type.is_unsigned_int() {
                    return Err(self.type_mismatch("integer range start", &start_type));
                }
                if !end_type.is_signed_int() && !end_type.is_unsigned_int() {
                    return Err(self.type_mismatch("integer range end", &end_type));
                }
                self.expect_type(&start_type, &end_type)?;
                Ok(Type::Generic {
                    name: "Range".to_string(),
                    args: vec![Type::I64],
                })
            }
            Expr::RepeatLiteral { value, count } => {
                let value_type = self.infer_expr_type_in_env(env, value)?;
                let count_type = self.infer_expr_type_in_env(env, count)?;
                if !count_type.is_signed_int() && !count_type.is_unsigned_int() {
                    return Err(self.type_mismatch("integer repeat count", &count_type));
                }
                if let Expr::Int(count) = count.as_ref() {
                    Ok(Type::Array {
                        element: Box::new(value_type),
                        size: (*count).max(0) as usize,
                    })
                } else {
                    Ok(Type::DynArray(Box::new(value_type)))
                }
            }
            Expr::TupleLiteral(elements) => elements
                .iter()
                .map(|element| self.infer_expr_type_in_env(env, element))
                .collect::<Result<Vec<_>>>()
                .map(Type::Tuple),
            Expr::MapLiteral(entries) => {
                if entries.is_empty() {
                    return Ok(Type::Map {
                        key: Box::new(Type::Unknown),
                        value: Box::new(Type::Unknown),
                    });
                }
                let key_type = self.infer_expr_type_in_env(env, &entries[0].0)?;
                let value_type = self.infer_expr_type_in_env(env, &entries[0].1)?;
                for (key, value) in &entries[1..] {
                    let next_key_type = self.infer_expr_type_in_env(env, key)?;
                    let next_value_type = self.infer_expr_type_in_env(env, value)?;
                    self.expect_type(&key_type, &next_key_type)?;
                    self.expect_type(&value_type, &next_value_type)?;
                }
                Ok(Type::Map {
                    key: Box::new(key_type),
                    value: Box::new(value_type),
                })
            }
            Expr::SetLiteral(elements) => {
                if elements.is_empty() {
                    return Ok(Type::Set(Box::new(Type::Unknown)));
                }
                let first_type = self.infer_expr_type_in_env(env, &elements[0])?;
                for element in &elements[1..] {
                    let element_type = self.infer_expr_type_in_env(env, element)?;
                    self.expect_type(&first_type, &element_type)?;
                }
                Ok(Type::Set(Box::new(first_type)))
            }
            Expr::Lambda { params, body } => {
                let mut lambda_env = env.child();
                let mut param_types = Vec::new();
                for param in params {
                    let param_type = self.resolve_type_expr(&param.type_ann)?;
                    lambda_env.define_var(param.name.clone(), param_type.clone());
                    param_types.push(param_type);
                }
                let ret = self.infer_expr_type_in_env(&lambda_env, body)?;
                Ok(Type::Fn {
                    params: param_types,
                    ret: Box::new(ret),
                })
            }
        }
    }

    fn infer_assignment_target_type(&self, env: &TypeEnv, target: &Expr) -> Result<Type> {
        match target {
            Expr::Ident(name) => env.lookup_var(name).ok_or_else(|| {
                CompileError::new(ErrorKind::UndefinedName(name.clone()), Span::zero())
            }),
            Expr::FieldAccess { object, field } => {
                let object_type = self.infer_expr_type_in_env(env, object)?;
                self.lookup_field_type(&object_type, field)
            }
            Expr::Index { object, .. } => {
                let object_type = self.infer_expr_type_in_env(env, object)?;
                match object_type {
                    Type::Array { element, .. } | Type::DynArray(element) => Ok(*element),
                    Type::Str => Ok(Type::Char),
                    got => Err(self.type_mismatch("array", &got)),
                }
            }
            other => self.infer_expr_type_in_env(env, other),
        }
    }

    fn check_binary_type(&self, op: BinOp, left: &Type, right: &Type) -> Result<Type> {
        match op {
            BinOp::Add if *left == Type::Str && *right == Type::Str => Ok(Type::Str),
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if left.is_numeric() && self.types_compatible(left, right) {
                    Ok(left.clone())
                } else {
                    Err(CompileError::new(
                        ErrorKind::TypeMismatch {
                            expected: left.to_string(),
                            got: right.to_string(),
                        },
                        Span::zero(),
                    ))
                }
            }
            BinOp::Eq | BinOp::Neq => {
                self.expect_type(left, right)?;
                Ok(Type::Bool)
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                if left.is_numeric() && self.types_compatible(left, right) {
                    Ok(Type::Bool)
                } else {
                    Err(self.type_mismatch("numeric comparison", right))
                }
            }
            BinOp::And | BinOp::Or => {
                self.expect_type(&Type::Bool, left)?;
                self.expect_type(&Type::Bool, right)?;
                Ok(Type::Bool)
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                if (left.is_signed_int() || left.is_unsigned_int())
                    && self.types_compatible(left, right)
                {
                    Ok(left.clone())
                } else {
                    Err(self.type_mismatch("integer", right))
                }
            }
        }
    }

    fn infer_call_type(&self, env: &TypeEnv, func: &Expr, args: &[Expr]) -> Result<Type> {
        match func {
            Expr::Ident(name) => {
                let functions = env.lookup_fn(name).ok_or_else(|| {
                    CompileError::new(ErrorKind::UndefinedName(name.clone()), Span::zero())
                })?;
                for function in functions.iter().filter(|function| {
                    function.receiver_type.is_none() && args.len() <= function.params.len()
                }) {
                    let mut matches = true;
                    for (arg, (_, expected_type)) in args.iter().zip(function.params.iter()) {
                        let arg_type = self.infer_expr_type_in_env(env, arg)?;
                        if !self.types_compatible(expected_type, &arg_type) {
                            matches = false;
                            break;
                        }
                    }
                    if matches {
                        return Ok(function.return_type.clone());
                    }
                }

                Err(CompileError::new(
                    ErrorKind::WrongNumberOfArguments {
                        expected: functions
                            .iter()
                            .find(|function| function.receiver_type.is_none())
                            .map(|function| function.params.len())
                            .unwrap_or(0),
                        got: args.len(),
                    },
                    Span::zero(),
                ))
            }
            other => {
                let func_type = self.infer_expr_type_in_env(env, other)?;
                match func_type {
                    Type::Fn { params, ret } => {
                        if params.len() != args.len() {
                            return Err(CompileError::new(
                                ErrorKind::WrongNumberOfArguments {
                                    expected: params.len(),
                                    got: args.len(),
                                },
                                Span::zero(),
                            ));
                        }
                        for (arg, expected_type) in args.iter().zip(params.iter()) {
                            let arg_type = self.infer_expr_type_in_env(env, arg)?;
                            self.expect_type(expected_type, &arg_type)?;
                        }
                        Ok(*ret)
                    }
                    got => Err(self.type_mismatch("function", &got)),
                }
            }
        }
    }

    fn infer_method_call_type(
        &self,
        env: &TypeEnv,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Result<Type> {
        let receiver_type = self.infer_expr_type_in_env(env, receiver)?;
        let functions = env.lookup_fn(method).ok_or_else(|| {
            CompileError::new(ErrorKind::UndefinedName(method.to_string()), Span::zero())
        })?;
        let function = functions
            .iter()
            .find(|function| {
                let expected_arg_count = explicit_method_param_count(function);
                function
                    .receiver_type
                    .as_ref()
                    .is_some_and(|expected| receiver_types_match(expected, &receiver_type))
                    && args.len() <= expected_arg_count
            })
            .ok_or_else(|| {
                CompileError::new(
                    ErrorKind::WrongNumberOfArguments {
                        expected: functions
                            .iter()
                            .find(|function| {
                                function.receiver_type.as_ref().is_some_and(|expected| {
                                    receiver_types_match(expected, &receiver_type)
                                })
                            })
                            .map(explicit_method_param_count)
                            .unwrap_or(0),
                        got: args.len(),
                    },
                    Span::zero(),
                )
            })?;

        let explicit_params = if function
            .params
            .first()
            .is_some_and(|(name, _)| name == "self")
        {
            &function.params[1..]
        } else {
            &function.params[..]
        };

        for (arg, (_, expected_type)) in args.iter().zip(explicit_params.iter()) {
            let arg_type = self.infer_expr_type_in_env(env, arg)?;
            let expected_type = specialize_method_type(expected_type, &receiver_type);
            self.expect_type(&expected_type, &arg_type)?;
        }

        Ok(specialize_method_type(
            &function.return_type,
            &receiver_type,
        ))
    }

    fn check_struct_literal(
        &self,
        env: &TypeEnv,
        type_name: &str,
        fields: &[(String, Expr)],
    ) -> Result<()> {
        let TypeInfo::Def {
            fields: type_fields,
            ..
        } = self.env.lookup_type(type_name).ok_or_else(|| {
            CompileError::new(
                ErrorKind::UndefinedType(type_name.to_string()),
                Span::zero(),
            )
        })?
        else {
            return Err(CompileError::new(
                ErrorKind::UndefinedType(type_name.to_string()),
                Span::zero(),
            ));
        };

        for (field_name, value) in fields {
            let Some((_, expected_type)) = type_fields.iter().find(|(name, _)| name == field_name)
            else {
                return Err(CompileError::new(
                    ErrorKind::MissingField {
                        type_name: type_name.to_string(),
                        field: field_name.clone(),
                    },
                    Span::zero(),
                ));
            };
            let value_type = self.infer_expr_type_in_env(env, value)?;
            self.expect_type(expected_type, &value_type)?;
        }

        Ok(())
    }

    fn check_enum_variant(
        &self,
        env: &TypeEnv,
        type_name: &str,
        variant_name: &str,
        value: Option<&Expr>,
    ) -> Result<()> {
        let TypeInfo::Enum { variants, .. } = self.env.lookup_type(type_name).ok_or_else(|| {
            CompileError::new(
                ErrorKind::UndefinedType(type_name.to_string()),
                Span::zero(),
            )
        })?
        else {
            return Err(CompileError::new(
                ErrorKind::UndefinedType(type_name.to_string()),
                Span::zero(),
            ));
        };

        let Some((_, expected_payload)) = variants.iter().find(|(name, _)| name == variant_name)
        else {
            return Err(CompileError::new(
                ErrorKind::UndefinedName(format!("{}::{}", type_name, variant_name)),
                Span::zero(),
            ));
        };

        match (expected_payload, value) {
            (Some(expected_type), Some(value)) => {
                let value_type = self.infer_expr_type_in_env(env, value)?;
                self.expect_type(expected_type, &value_type)
            }
            (None, None) => Ok(()),
            (Some(_), None) => Err(CompileError::new(
                ErrorKind::WrongNumberOfArguments {
                    expected: 1,
                    got: 0,
                },
                Span::zero(),
            )),
            (None, Some(_)) => Err(CompileError::new(
                ErrorKind::WrongNumberOfArguments {
                    expected: 0,
                    got: 1,
                },
                Span::zero(),
            )),
        }
    }

    fn lookup_field_type(&self, object_type: &Type, field: &str) -> Result<Type> {
        match object_type {
            Type::Named(name) => match self.env.lookup_type(name) {
                Some(TypeInfo::Def { fields, .. }) => fields
                    .iter()
                    .find(|(field_name, _)| field_name == field)
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            ErrorKind::MissingField {
                                type_name: name.clone(),
                                field: field.to_string(),
                            },
                            Span::zero(),
                        )
                    }),
                _ => Err(self.type_mismatch("def type", object_type)),
            },
            Type::Tuple(types) => field
                .parse::<usize>()
                .ok()
                .and_then(|idx| types.get(idx).cloned())
                .ok_or_else(|| {
                    CompileError::new(
                        ErrorKind::MissingField {
                            type_name: object_type.to_string(),
                            field: field.to_string(),
                        },
                        Span::zero(),
                    )
                }),
            got => Err(self.type_mismatch("field-bearing type", got)),
        }
    }

    fn expect_type(&self, expected: &Type, got: &Type) -> Result<()> {
        if self.types_compatible(expected, got) {
            Ok(())
        } else {
            Err(CompileError::new(
                ErrorKind::TypeMismatch {
                    expected: expected.to_string(),
                    got: got.to_string(),
                },
                Span::zero(),
            ))
        }
    }

    fn types_compatible(&self, expected: &Type, got: &Type) -> bool {
        expected == got
            || matches!(expected, Type::Unknown)
            || matches!(got, Type::Unknown)
            || matches!(
                (expected, got),
                (Type::DynArray(expected), Type::DynArray(got))
                    if matches!(**expected, Type::Unknown) || matches!(**got, Type::Unknown)
            )
            || (expected.is_signed_int() && got.is_signed_int())
            || (expected.is_unsigned_int() && got.is_unsigned_int())
            || (expected.is_float() && got.is_float())
    }

    fn type_mismatch(&self, expected: &str, got: &Type) -> CompileError {
        CompileError::new(
            ErrorKind::TypeMismatch {
                expected: expected.to_string(),
                got: got.to_string(),
            },
            Span::zero(),
        )
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

                if let Some(std_type) = vita_std::resolve_generic_type(name, &resolved_args) {
                    return std_type.map_err(|expected| {
                        CompileError::new(
                            ErrorKind::WrongNumberOfArguments {
                                expected,
                                got: resolved_args.len(),
                            },
                            Span::zero(),
                        )
                    });
                }

                if self.env.lookup_type(name).is_some() {
                    Ok(Type::Generic {
                        name: name.clone(),
                        args: resolved_args,
                    })
                } else {
                    Err(CompileError::new(
                        ErrorKind::UndefinedType(name.clone()),
                        Span::zero(),
                    ))
                }
            }
            TypeExpr::Array { element, size } => {
                let elem = self.resolve_type_expr(element)?;
                match size {
                    Some(size) => Ok(Type::Array {
                        element: Box::new(elem),
                        size: *size,
                    }),
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
            Expr::RangeLiteral { .. } => Type::Generic {
                name: "Range".to_string(),
                args: vec![Type::I64],
            },
            Expr::RepeatLiteral { value, .. } => {
                Type::DynArray(Box::new(self.infer_expr_type(value)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::Lexer;
    use crate::syntax::parser::Parser;

    fn check_source(source: &str) -> Result<TypeEnv> {
        let tokens = Lexer::tokenize(source)?;
        let mut parser = Parser::new(tokens);
        let items = parser.parse()?;
        TypeChecker::new().check(&items)
    }

    #[test]
    fn accepts_printing_supported_builtin_types() {
        let source = r#"
            fn main() {
                print("hello");
                print(42);
                print(true);
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn rejects_undefined_local_names_in_bodies() {
        let source = r#"
            fn main() {
                print(missing);
            }
        "#;

        let err = check_source(source).expect_err("undefined names should fail");
        assert!(matches!(err.kind, ErrorKind::UndefinedName(name) if name == "missing"));
    }

    #[test]
    fn rejects_mismatched_return_type() {
        let source = r#"
            fn answer() -> i32 {
                "nope"
            }
        "#;

        let err = check_source(source).expect_err("bad return type should fail");
        assert!(matches!(err.kind, ErrorKind::TypeMismatch { .. }));
    }

    #[test]
    fn rejects_non_bool_conditions() {
        let source = r#"
            fn main() {
                ? 1 {
                    print("bad");
                }
            }
        "#;

        let err = check_source(source).expect_err("non-bool conditions should fail");
        assert!(matches!(err.kind, ErrorKind::TypeMismatch { .. }));
    }

    #[test]
    fn accepts_string_indexing_for_source_scanning() {
        let source = r#"
            fn first(text: str) -> char {
                let i: i64 = 0;
                text[i]
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_integer_counters_against_string_len() {
        let source = r#"
            fn main() {
                let text = "abc";
                let i: i64 = 0;
                ? i < text.len() {
                    print(text[i]);
                }
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_foreach_over_range_literal() {
        let source = r#"
            fn main() {
                let text = "abc";
                *? let i: [0..text.len()] {
                    print(text[i]);
                };
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_typed_foreach_range_variable() {
        let source = r#"
            fn main() {
                let text = "abc";
                *? let i: u32: [0..text.len()] {
                    print(text[i]);
                };
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_fixed_and_repeat_array_indexing() {
        let source = r#"
            fn main() {
                let xs = [10, 20, 30];
                let zeros = [0; 3];
                print(xs[1]);
                print(zeros[2]);
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_string_equality_and_char_classification() {
        let source = r#"
            fn main() {
                let text = "a1 ";
                ? text == "a1 " {
                    print(text[0].is_alpha());
                    print(text[1].is_digit());
                    print(text[2].is_whitespace());
                };
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_file_io_builtins() {
        let source = r#"
            fn main() {
                let ok = write_file("/tmp/vita-checker-file.txt", "hello");
                let text = read_file("/tmp/vita-checker-file.txt");
                print(ok);
                print(text);
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_string_slices_for_lexemes() {
        let source = r#"
            fn main() {
                let text = "hello world";
                let word = text.slice(0, 5);
                ? word == "hello" {
                    print(word);
                };
            }
        "#;

        assert!(check_source(source).is_ok());
    }

    #[test]
    fn accepts_growable_array_token_buffer_shape() {
        let source = r#"
            fn main() {
                let source = "let x";
                let tokens: Array<str> = [];
                tokens = tokens.push(source.slice(0, 3));
                tokens = tokens.push(source.slice(4, 5));
                print(tokens.len());
                print(tokens[0]);
                print(tokens[1]);
            }
        "#;

        assert!(check_source(source).is_ok());
    }
}
