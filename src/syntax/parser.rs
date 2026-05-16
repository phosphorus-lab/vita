//! Recursive descent parser for the Vita language.
//!
//! Converts a token stream into an AST.

use crate::diagnostics::error::{CompileError, ErrorKind, Result, Span};
use crate::syntax::ast::*;
use crate::syntax::token::{Token, TokenKind};

/// Parser state.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// When true, `ident {` is NOT parsed as a struct literal.
    /// Used in contexts like while conditions: `* x <= n { }`
    /// where the `{` belongs to the control flow, not the expression.
    no_struct_literal: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            no_struct_literal: false,
        }
    }

    /// Parse a complete Vita source file.
    pub fn parse(&mut self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        while !self.is_eof() {
            items.push(self.parse_item()?);
        }
        Ok(items)
    }

    // --- Navigation helpers ---

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len() || self.peek().kind == TokenKind::Eof
    }

    fn peek(&self) -> &Token {
        static EOF: Token = Token {
            kind: TokenKind::Eof,
            text: String::new(),
            line: 0,
            col: 0,
        };
        self.tokens.get(self.pos).unwrap_or(&EOF)
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    fn advance(&mut self) -> Token {
        let tok = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| Token::new(TokenKind::Eof, "", 0, 0));
        self.pos += 1;
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token> {
        let tok = self.advance();
        if tok.kind != kind {
            Err(CompileError::new(
                ErrorKind::UnexpectedToken {
                    expected: kind.to_string(),
                    got: tok.kind.to_string(),
                },
                Span::new(tok.line, tok.col, 0),
            ))
        } else {
            Ok(tok)
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::Ident | TokenKind::KwSelf => Ok(tok.text.clone()),
            _ => Err(CompileError::new(
                ErrorKind::UnexpectedToken {
                    expected: "identifier".to_string(),
                    got: tok.kind.to_string(),
                },
                Span::new(tok.line, tok.col, 0),
            )),
        }
    }

    fn match_kind(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek_kind() == kind {
            Some(self.advance())
        } else {
            None
        }
    }

    fn span_here(&self) -> Span {
        let tok = self.peek();
        Span::new(tok.line, tok.col, 0)
    }

    // --- Item parsing ---

    fn parse_item(&mut self) -> Result<Item> {
        match self.peek_kind() {
            TokenKind::KwDef => self.parse_def().map(Item::Def),
            TokenKind::KwEnum => self.parse_enum().map(Item::Enum),
            TokenKind::KwSpec => self.parse_spec().map(Item::Spec),
            TokenKind::KwImpl => self.parse_impl().map(Item::Impl),
            TokenKind::KwFn => self.parse_fn_item(false).map(Item::Fn),
            TokenKind::KwPub => {
                self.advance(); // consume `pub`
                match self.peek_kind() {
                    TokenKind::KwFn => self.parse_fn_item(true).map(Item::Fn),
                    _ => Err(CompileError::new(
                        ErrorKind::UnexpectedToken {
                            expected: "fn after pub".to_string(),
                            got: self.peek_kind().to_string(),
                        },
                        self.span_here(),
                    )),
                }
            }
            TokenKind::KwUse => self.parse_use().map(Item::Use),
            _ => Err(CompileError::new(
                ErrorKind::UnexpectedToken {
                    expected: "item (def, enum, spec, impl, fn, use)".to_string(),
                    got: self.peek_kind().to_string(),
                },
                self.span_here(),
            )),
        }
    }

    fn parse_def(&mut self) -> Result<DefItem> {
        self.expect(TokenKind::KwDef)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let type_ann = self.parse_type_expr()?;
            fields.push(FieldDef {
                name: field_name,
                type_ann,
            });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RBrace)?;

        Ok(DefItem {
            name,
            generics,
            fields,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumItem> {
        self.expect(TokenKind::KwEnum)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LBrace)?;

        let mut variants = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let variant_name = self.expect_ident()?;
            let payload = if self.peek_kind() == TokenKind::LParen {
                self.advance();
                let ty = self.parse_type_expr()?;
                self.expect(TokenKind::RParen)?;
                Some(ty)
            } else {
                None
            };
            variants.push(VariantDef {
                name: variant_name,
                payload,
            });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RBrace)?;

        Ok(EnumItem {
            name,
            generics,
            variants,
        })
    }

    fn parse_spec(&mut self) -> Result<SpecItem> {
        self.expect(TokenKind::KwSpec)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;

        let mut members = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let is_pub = self.match_kind(TokenKind::KwPub).is_some();
            if self.peek_kind() == TokenKind::KwFn {
                // Spec function requirement
                self.advance();
                let fn_name = self.expect_ident()?;
                let params = self.parse_param_list()?;
                let return_type = if self.match_kind(TokenKind::Arrow).is_some() {
                    Some(self.parse_type_expr()?)
                } else {
                    None
                };
                self.expect(TokenKind::Semicolon)?;
                members.push(SpecMember::Fn {
                    is_pub,
                    name: fn_name,
                    params,
                    return_type,
                });
            } else {
                // Spec field requirement
                let field_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let type_ann = self.parse_type_expr()?;
                self.expect(TokenKind::Semicolon)?;
                members.push(SpecMember::Field {
                    name: field_name,
                    type_ann,
                });
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(SpecItem { name, members })
    }

    fn parse_impl(&mut self) -> Result<ImplItem> {
        self.expect(TokenKind::KwImpl)?;
        let target_type = self.expect_ident()?;
        let target_generics = self.parse_generic_params()?;

        let spec_name = if self.match_kind(TokenKind::Colon).is_some() {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(TokenKind::LBrace)?;

        let mut methods = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let is_pub = self.match_kind(TokenKind::KwPub).is_some();
            // Note: KwFn is consumed inside parse_fn_item
            methods.push(self.parse_fn_item(is_pub)?);
        }
        self.expect(TokenKind::RBrace)?;

        Ok(ImplItem {
            target_type,
            target_generics,
            spec_name,
            methods,
        })
    }

    fn parse_fn_item(&mut self, is_pub: bool) -> Result<FnItem> {
        self.expect(TokenKind::KwFn)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let params = self.parse_param_list()?;
        let return_type = if self.match_kind(TokenKind::Arrow).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(FnItem {
            is_pub,
            name,
            generics,
            params,
            return_type,
            body,
        })
    }

    fn parse_use(&mut self) -> Result<UseItem> {
        self.expect(TokenKind::KwUse)?;
        let mut path = Vec::new();

        // Parse the path segments (which may start with dots)
        let first = self.advance();
        match first.kind {
            TokenKind::Ident => path.push(first.text.clone()),
            TokenKind::Dot => {
                // Relative import: . or .. or ...
                let mut dots = 1;
                while self.peek_kind() == TokenKind::Dot {
                    self.advance();
                    dots += 1;
                }
                // Push parent indicators as special segments
                for _ in 0..dots {
                    path.push(".".to_string());
                }
                // Next segment is the actual module name
                if self.peek_kind() == TokenKind::Ident {
                    path.push(self.expect_ident()?);
                }
            }
            _ => {
                return Err(CompileError::new(
                    ErrorKind::UnexpectedToken {
                        expected: "identifier or dot".to_string(),
                        got: first.kind.to_string(),
                    },
                    Span::new(first.line, first.col, 0),
                ));
            }
        }

        // Continue parsing path segments
        while self.match_kind(TokenKind::Dot).is_some() {
            if self.peek_kind() == TokenKind::Ident {
                path.push(self.expect_ident()?);
            } else {
                break;
            }
        }

        // Check for alias: `as name`
        let alias = if self.match_kind(TokenKind::KwAs).is_some() {
            Some(self.expect_ident()?)
        } else {
            None
        };

        // Check for symbol import: `.{ Symbol }`
        let symbols = if self.match_kind(TokenKind::Dot).is_some()
            && self.match_kind(TokenKind::LBrace).is_some()
        {
            let mut syms = Vec::new();
            while self.peek_kind() != TokenKind::RBrace {
                let name = self.expect_ident()?;
                let alias = if self.match_kind(TokenKind::KwAs).is_some() {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                syms.push(UseSymbol { name, alias });
                self.match_kind(TokenKind::Comma);
            }
            self.expect(TokenKind::RBrace)?;
            syms
        } else {
            Vec::new()
        };

        Ok(UseItem {
            path,
            alias,
            symbols,
        })
    }

    // --- Generics ---

    fn parse_generic_params(&mut self) -> Result<Vec<String>> {
        if self.match_kind(TokenKind::Lt).is_none() {
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        while self.peek_kind() != TokenKind::Gt {
            params.push(self.expect_ident()?);
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::Gt)?;
        Ok(params)
    }

    // --- Parameters ---

    fn parse_param_list(&mut self) -> Result<Vec<Param>> {
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while self.peek_kind() != TokenKind::RParen {
            let name = self.expect_ident()?;
            // `self` may not have a type annotation in method definitions
            let type_ann = if self.match_kind(TokenKind::Colon).is_some() {
                self.parse_type_expr()?
            } else {
                TypeExpr::SelfType
            };
            let default = if self.match_kind(TokenKind::Eq).is_some() {
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param {
                name,
                type_ann,
                default,
            });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    // --- Type expressions ---

    fn parse_type_expr(&mut self) -> Result<TypeExpr> {
        if self.peek_kind() == TokenKind::KwSelf {
            self.advance();
            return Ok(TypeExpr::SelfType);
        }

        if self.peek_kind() == TokenKind::LParen {
            // Tuple or unit type
            self.advance();
            if self.match_kind(TokenKind::RParen).is_some() {
                return Ok(TypeExpr::Unit);
            }
            let mut types = vec![self.parse_type_expr()?];
            while self.match_kind(TokenKind::Comma).is_some() {
                types.push(self.parse_type_expr()?);
            }
            self.expect(TokenKind::RParen)?;
            return Ok(TypeExpr::Tuple(types));
        }

        if self.peek_kind() == TokenKind::LBracket {
            // Array type: [T; N] or [T]
            self.advance();
            let element = self.parse_type_expr()?;
            let size = if self.match_kind(TokenKind::Semicolon).is_some() {
                let tok = self.advance();
                Some(tok.text.parse::<usize>().map_err(|_| {
                    CompileError::new(
                        ErrorKind::InvalidNumber(tok.text.clone()),
                        Span::new(tok.line, tok.col, 0),
                    )
                })?)
            } else {
                None
            };
            self.expect(TokenKind::RBracket)?;
            return Ok(TypeExpr::Array {
                element: Box::new(element),
                size,
            });
        }

        // Named type, possibly with generic args
        let name = self.expect_ident()?;

        if self.peek_kind() == TokenKind::Lt {
            self.advance();
            let mut args = Vec::new();
            while self.peek_kind() != TokenKind::Gt {
                args.push(self.parse_type_expr()?);
                self.match_kind(TokenKind::Comma);
            }
            self.expect(TokenKind::Gt)?;
            Ok(TypeExpr::Generic { name, args })
        } else {
            Ok(TypeExpr::Named(name))
        }
    }

    // --- Blocks ---

    fn parse_block(&mut self) -> Result<Block> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        let mut tail = None;

        while self.peek_kind() != TokenKind::RBrace {
            // Check for let binding (statement form)
            if self.peek_kind() == TokenKind::KwLet {
                stmts.push(self.parse_let_stmt()?);
                continue;
            }

            // Check for break/continue/return (statement form)
            if self.peek_kind() == TokenKind::KwBreak {
                self.advance();
                stmts.push(Stmt::Break);
                self.match_kind(TokenKind::Semicolon);
                continue;
            }
            if self.peek_kind() == TokenKind::KwContinue {
                self.advance();
                stmts.push(Stmt::Continue);
                self.match_kind(TokenKind::Semicolon);
                continue;
            }
            if self.peek_kind() == TokenKind::KwReturn {
                self.advance();
                let ret_expr = if self.peek_kind() != TokenKind::Semicolon
                    && self.peek_kind() != TokenKind::RBrace
                {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.match_kind(TokenKind::Semicolon);
                stmts.push(Stmt::Return(ret_expr));
                continue;
            }

            // Try to parse an expression
            let expr = self.parse_expr()?;

            if self.match_kind(TokenKind::Semicolon).is_some() {
                // Expression with semicolon is a statement
                stmts.push(Stmt::SemiExpr(expr));
            } else if self.peek_kind() == TokenKind::RBrace {
                // Last expression without semicolon is the tail (return value)
                tail = Some(Box::new(expr));
                break;
            } else {
                // Could be a let binding or other statement form
                stmts.push(Stmt::Expr(expr));
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(Block { stmts, tail })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt> {
        self.expect(TokenKind::KwLet)?;
        let name = self.expect_ident()?;
        let type_ann = if self.match_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.match_kind(TokenKind::Semicolon);
        Ok(Stmt::Let {
            name,
            type_ann,
            value,
        })
    }

    // --- Expression parsing (precedence climbing) ---

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let expr = self.parse_or()?;

        match self.peek_kind() {
            TokenKind::Eq => {
                self.advance();
                let value = self.parse_assignment()?;
                Ok(Expr::Assign {
                    target: Box::new(expr),
                    value: Box::new(value),
                })
            }
            TokenKind::PlusEq
            | TokenKind::MinusEq
            | TokenKind::StarEq
            | TokenKind::SlashEq
            | TokenKind::PercentEq => {
                let op = match self.advance().kind {
                    TokenKind::PlusEq => CompoundOp::AddEq,
                    TokenKind::MinusEq => CompoundOp::SubEq,
                    TokenKind::StarEq => CompoundOp::MulEq,
                    TokenKind::SlashEq => CompoundOp::DivEq,
                    TokenKind::PercentEq => CompoundOp::ModEq,
                    _ => unreachable!(),
                };
                let value = self.parse_assignment()?;
                let bin_op = match op {
                    CompoundOp::AddEq => BinOp::Add,
                    CompoundOp::SubEq => BinOp::Sub,
                    CompoundOp::MulEq => BinOp::Mul,
                    CompoundOp::DivEq => BinOp::Div,
                    CompoundOp::ModEq => BinOp::Mod,
                };
                Ok(Expr::Assign {
                    target: Box::new(expr.clone()),
                    value: Box::new(Expr::Binary {
                        op: bin_op,
                        left: Box::new(expr),
                        right: Box::new(value),
                    }),
                })
            }
            _ => Ok(expr),
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.peek_kind() == TokenKind::PipePipe {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;
        while self.peek_kind() == TokenKind::AmpAmp {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let left = self.parse_bitwise()?;
        if let Some(op) = BinOp::from_token(self.peek_kind()) {
            if matches!(
                op,
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq
            ) {
                self.advance();
                let right = self.parse_bitwise()?;
                return Ok(Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                });
            }
        }
        Ok(left)
    }

    fn parse_bitwise(&mut self) -> Result<Expr> {
        let mut left = self.parse_shift()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Amp => BinOp::BitAnd,
                TokenKind::Pipe => BinOp::BitOr,
                TokenKind::Caret => BinOp::BitXor,
                _ => break,
            };
            self.advance();
            let right = self.parse_shift()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::LtLt => BinOp::Shl,
                TokenKind::GtGt => BinOp::Shr,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star | TokenKind::StarMul => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek_kind() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    operand: Box::new(operand),
                })
            }
            TokenKind::Bang => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.advance();
                    // Could be field access or tuple access
                    if let Some(tok) = self.match_kind(TokenKind::IntLiteral) {
                        let idx: usize = tok.text.parse().map_err(|_| {
                            CompileError::new(
                                ErrorKind::InvalidNumber(tok.text.clone()),
                                Span::new(tok.line, tok.col, 0),
                            )
                        })?;
                        expr = Expr::TupleAccess {
                            tuple: Box::new(expr),
                            index: idx,
                        };
                    } else {
                        let field = self.expect_ident()?;
                        // Check for method call
                        if self.peek_kind() == TokenKind::LParen {
                            let args = self.parse_call_args()?;
                            expr = Expr::MethodCall {
                                receiver: Box::new(expr),
                                method: field,
                                args,
                            };
                        } else {
                            expr = Expr::FieldAccess {
                                object: Box::new(expr),
                                field,
                            };
                        }
                    }
                }
                TokenKind::LParen => {
                    // Function call
                    let args = self.parse_call_args()?;
                    expr = Expr::Call {
                        func: Box::new(expr),
                        args,
                    };
                }
                TokenKind::LBracket => {
                    // Index access
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        while self.peek_kind() != TokenKind::RParen {
            args.push(self.parse_expr()?);
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.peek_kind() {
            // Literals
            TokenKind::IntLiteral => {
                let tok = self.advance();
                let val: i64 = tok.text.replace('i', "").parse().map_err(|_| {
                    CompileError::new(
                        ErrorKind::InvalidNumber(tok.text.clone()),
                        Span::new(tok.line, tok.col, 0),
                    )
                })?;
                Ok(Expr::Int(val))
            }
            TokenKind::FloatLiteral => {
                let tok = self.advance();
                let val: f64 = tok.text.parse().map_err(|_| {
                    CompileError::new(
                        ErrorKind::InvalidNumber(tok.text.clone()),
                        Span::new(tok.line, tok.col, 0),
                    )
                })?;
                Ok(Expr::Float(val))
            }
            TokenKind::StringLiteral => {
                let tok = self.advance();
                Ok(Expr::String(tok.text.clone()))
            }
            TokenKind::CharLiteral => {
                let tok = self.advance();
                let ch = tok.text.chars().next().unwrap_or('\0');
                Ok(Expr::Char(ch))
            }
            TokenKind::KwTrue => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::KwFalse => {
                self.advance();
                Ok(Expr::Bool(false))
            }

            // Identifier, possibly with :: for enum variant
            TokenKind::Ident => {
                let name = self.expect_ident()?;
                if self.match_kind(TokenKind::DoubleColon).is_some() {
                    // Enum variant: Name::Variant
                    let variant = self.expect_ident()?;
                    let value = if self.peek_kind() == TokenKind::LParen {
                        self.advance();
                        let v = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        Some(Box::new(v))
                    } else {
                        None
                    };
                    Ok(Expr::EnumVariant {
                        type_name: name,
                        variant,
                        value,
                    })
                } else if !self.no_struct_literal && self.peek_kind() == TokenKind::LBrace {
                    // Struct literal: Name { field: value, ... }
                    // Only when not in a "no struct literal" context (e.g., while condition)
                    self.advance();
                    let mut fields = Vec::new();
                    while self.peek_kind() != TokenKind::RBrace {
                        let field_name = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push((field_name, value));
                        self.match_kind(TokenKind::Comma);
                    }
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::StructLiteral {
                        type_name: name,
                        fields,
                    })
                } else {
                    Ok(Expr::Ident(name))
                }
            }

            // Grouped expression or tuple or unit
            TokenKind::LParen => {
                self.advance();
                if self.match_kind(TokenKind::RParen).is_some() {
                    return Ok(Expr::Unit);
                }
                let first = self.parse_expr()?;
                if self.match_kind(TokenKind::Comma).is_some() {
                    // Tuple
                    let mut elements = vec![first];
                    while self.peek_kind() != TokenKind::RParen {
                        elements.push(self.parse_expr()?);
                        self.match_kind(TokenKind::Comma);
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::TupleLiteral(elements))
                } else {
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Grouped(Box::new(first)))
                }
            }

            // Array literal
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                while self.peek_kind() != TokenKind::RBracket {
                    elements.push(self.parse_expr()?);
                    self.match_kind(TokenKind::Comma);
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Expr::ArrayLiteral(elements))
            }

            // If expression: ? condition { } !? condition { } ! { }
            TokenKind::Question => {
                self.advance();
                // Parse condition without struct literals to avoid ambiguity
                self.no_struct_literal = true;
                let condition = self.parse_expr()?;
                self.no_struct_literal = false;
                let then_block = self.parse_block()?;

                let mut else_if_clauses = Vec::new();
                let mut else_block = None;

                loop {
                    if self.match_kind(TokenKind::BangQuestion).is_some() {
                        let cond = self.parse_expr()?;
                        let block = self.parse_block()?;
                        else_if_clauses.push((cond, Box::new(block)));
                    } else if self.match_kind(TokenKind::Bang).is_some() {
                        else_block = Some(Box::new(self.parse_block()?));
                        break;
                    } else {
                        break;
                    }
                }

                Ok(Expr::If {
                    condition: Box::new(condition),
                    then_block: Box::new(then_block),
                    else_if_clauses,
                    else_block,
                })
            }

            // Match expression: $ expr { }
            TokenKind::Dollar => {
                self.advance();
                let subject = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let mut arms = Vec::new();
                while self.peek_kind() != TokenKind::RBrace {
                    let pattern = self.parse_pattern()?;
                    self.expect(TokenKind::FatArrow)?;
                    let body = self.parse_expr()?;
                    self.match_kind(TokenKind::Comma);
                    arms.push(MatchArm { pattern, body });
                }
                self.expect(TokenKind::RBrace)?;
                Ok(Expr::Match {
                    subject: Box::new(subject),
                    arms,
                })
            }

            // Loop: * { }
            TokenKind::Star => {
                self.advance();
                // Could be * { } (loop), * cond { } (while), or *? var: iter { } (for-each)
                if self.peek_kind() == TokenKind::LBrace {
                    let body = self.parse_block()?;
                    Ok(Expr::Loop(Box::new(body)))
                } else {
                    // Parse condition WITHOUT allowing struct literals,
                    // so that `* x <= n { }` doesn't parse `x { }` as struct literal
                    self.no_struct_literal = true;
                    let condition = self.parse_expr()?;
                    self.no_struct_literal = false;
                    let body = self.parse_block()?;
                    Ok(Expr::While {
                        condition: Box::new(condition),
                        body: Box::new(body),
                    })
                }
            }

            // For-each: *? item: items { }
            TokenKind::StarQuestion => {
                self.advance();
                let var = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let iterable = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Expr::ForEach {
                    var,
                    iterable: Box::new(iterable),
                    body: Box::new(body),
                })
            }

            // Fallible block: ?? { }
            TokenKind::DoubleQuestion => {
                self.advance();
                let block = self.parse_block()?;

                let handler = if self.match_kind(TokenKind::DoubleBang).is_some() {
                    let err_name = self.expect_ident()?;
                    let body = self.parse_block()?;
                    Some(FallibleHandler::Catch {
                        err_name,
                        body: Box::new(body),
                    })
                } else if self.match_kind(TokenKind::BangDollar).is_some() {
                    let err_name = self.expect_ident()?;
                    self.expect(TokenKind::LBrace)?;
                    let mut arms = Vec::new();
                    while self.peek_kind() != TokenKind::RBrace {
                        let pattern = self.parse_pattern()?;
                        self.expect(TokenKind::FatArrow)?;
                        let body = self.parse_expr()?;
                        self.match_kind(TokenKind::Comma);
                        arms.push(MatchArm { pattern, body });
                    }
                    self.expect(TokenKind::RBrace)?;
                    Some(FallibleHandler::CatchMatch { err_name, arms })
                } else {
                    None
                };

                Ok(Expr::Fallible {
                    block: Box::new(block),
                    handler,
                })
            }

            // Let binding is handled in parse_block, not here
            // This case should not be reached normally

            // Self
            TokenKind::KwSelf => {
                self.advance();
                Ok(Expr::Ident("self".to_string()))
            }

            _ => Err(CompileError::new(
                ErrorKind::UnexpectedToken {
                    expected: "expression".to_string(),
                    got: self.peek_kind().to_string(),
                },
                self.span_here(),
            )),
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        match self.peek_kind() {
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::IntLiteral => {
                let tok = self.advance();
                let val: i64 = tok.text.parse().map_err(|_| {
                    CompileError::new(
                        ErrorKind::InvalidNumber(tok.text.clone()),
                        Span::new(tok.line, tok.col, 0),
                    )
                })?;
                Ok(Pattern::Int(val))
            }
            TokenKind::KwTrue => {
                self.advance();
                Ok(Pattern::Bool(true))
            }
            TokenKind::KwFalse => {
                self.advance();
                Ok(Pattern::Bool(false))
            }
            TokenKind::StringLiteral => {
                let tok = self.advance();
                Ok(Pattern::String(tok.text.clone()))
            }
            TokenKind::Ident => {
                let name = self.expect_ident()?;
                // Check for Type::Variant(binding)
                if self.match_kind(TokenKind::DoubleColon).is_some() {
                    let variant = self.expect_ident()?;
                    let binding = if self.peek_kind() == TokenKind::LParen {
                        self.advance();
                        let b = self.expect_ident()?;
                        self.expect(TokenKind::RParen)?;
                        Some(b)
                    } else {
                        None
                    };
                    Ok(Pattern::Variant {
                        type_name: Some(name),
                        variant,
                        binding,
                    })
                } else if self.peek_kind() == TokenKind::LParen {
                    // Variant(binding) without type name
                    self.advance();
                    let binding = if self.peek_kind() == TokenKind::RParen {
                        None
                    } else {
                        Some(self.expect_ident()?)
                    };
                    self.expect(TokenKind::RParen)?;
                    Ok(Pattern::Variant {
                        type_name: None,
                        variant: name,
                        binding,
                    })
                } else {
                    Ok(Pattern::Ident(name))
                }
            }
            _ => Err(CompileError::new(
                ErrorKind::UnexpectedToken {
                    expected: "pattern".to_string(),
                    got: self.peek_kind().to_string(),
                },
                self.span_here(),
            )),
        }
    }
}
