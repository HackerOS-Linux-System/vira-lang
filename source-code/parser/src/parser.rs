use crate::ast::*;
use crate::lexer::{Span, SpannedToken, Token};
use crate::error::{ParseError, ParseResult};

// ─── Parser state ─────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    /// Pending doc comments to attach to next item
    pending_docs: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, pos: 0, pending_docs: Vec::new() }
    }

    // ── Core helpers ──────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.node)
    }

    fn peek2(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1).map(|t| &t.node)
    }

    fn span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.clone())
            .unwrap_or(Span::new(0, 0, 0, 0))
    }

    fn advance(&mut self) -> Option<SpannedToken> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            // collect doc comments
            if let Token::DocComment(s) = &tok.node {
                self.pending_docs.push(s.trim_start_matches('/').trim().to_owned());
                return self.advance();
            }
            Some(tok)
        } else {
            None
        }
    }

    fn take_docs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_docs)
    }

    fn expect(&mut self, expected: &Token) -> ParseResult<SpannedToken> {
        match self.peek() {
            Some(tok) if tok == expected => Ok(self.advance().unwrap()),
            Some(tok) => Err(ParseError::unexpected(
                format!("{expected:?}"),
                format!("{tok:?}"),
                self.span(),
            )),
            None => Err(ParseError::eof(self.span())),
        }
    }

    fn expect_ident(&mut self) -> ParseResult<(String, Span)> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Ident(name)) => {
                self.advance();
                Ok((name, sp))
            }
            Some(tok) => Err(ParseError::unexpected("identifier".into(), format!("{tok:?}"), sp)),
            None => Err(ParseError::eof(sp)),
        }
    }

    fn eat(&mut self, tok: &Token) -> bool {
        if self.peek() == Some(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    // ── Visibility ────────────────────────────────────────────────────────────

    fn parse_visibility(&mut self) -> Visibility {
        if self.eat(&Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    // ── Program ───────────────────────────────────────────────────────────────

    pub fn parse_program(&mut self) -> ParseResult<Program> {
        let mut imports = Vec::new();
        let mut items = Vec::new();

        while !self.at_end() {
            match self.peek() {
                Some(Token::Use) | Some(Token::Using) => {
                    imports.push(self.parse_import()?);
                }
                _ => {
                    items.push(self.parse_item()?);
                }
            }
        }

        Ok(Program { imports, items })
    }

    // ── Imports ───────────────────────────────────────────────────────────────

    fn parse_import(&mut self) -> ParseResult<Import> {
        let sp = self.span();
        let kind = if self.eat(&Token::Use) {
            ImportKind::Native
        } else {
            self.expect(&Token::Using)?;
            ImportKind::Crate
        };

        self.expect(&Token::LAngle)?;
        let (name_raw, _) = self.expect_ident()?;

        // optional :version
        let version = if self.eat(&Token::Colon) {
            let (ver, _) = self.expect_ident()?;
            Some(ver)
        } else {
            None
        };

        self.expect(&Token::RAngle)?;

        Ok(Import { kind, name: name_raw, version, span: sp })
    }

    // ── Items ─────────────────────────────────────────────────────────────────

    fn parse_item(&mut self) -> ParseResult<Item> {
        let docs = self.take_docs();
        let vis = self.parse_visibility();

        // flags
        let is_async   = self.eat(&Token::Async);
        let is_inline  = self.eat(&Token::Inline);
        let is_unsafe  = self.eat(&Token::Unsafe);
        let is_comptime = self.eat(&Token::Comptime);

        match self.peek().cloned() {
            Some(Token::Fn) | Some(Token::Init) | Some(Token::Deinit) => {
                Ok(Item::Function(self.parse_fn(
                    docs, vis, is_async, is_inline, is_unsafe, is_comptime,
                )?))
            }
            Some(Token::Struct) => {
                Ok(Item::Struct(self.parse_struct(docs, vis)?))
            }
            Some(Token::Enum) => {
                Ok(Item::Enum(self.parse_enum(docs, vis)?))
            }
            Some(Token::Trait) => {
                Ok(Item::Trait(self.parse_trait(docs, vis)?))
            }
            Some(Token::Impl) => {
                Ok(Item::Impl(self.parse_impl()?))
            }
            Some(Token::Type) => {
                Ok(Item::TypeAlias(self.parse_type_alias(docs, vis)?))
            }
            Some(Token::Const) => {
                Ok(Item::Constant(self.parse_const(docs, vis)?))
            }
            Some(Token::Extern) => {
                Ok(Item::ExternBlock(self.parse_extern()?))
            }
            Some(tok) => Err(ParseError::unexpected(
                "item".into(), format!("{tok:?}"), self.span(),
            )),
            None => Err(ParseError::eof(self.span())),
        }
    }

    // ── Functions ─────────────────────────────────────────────────────────────

    fn parse_fn(
        &mut self,
        docs: Vec<String>,
        visibility: Visibility,
        is_async: bool,
        is_inline: bool,
        is_unsafe: bool,
        is_comptime: bool,
    ) -> ParseResult<FunctionDef> {
        let sp = self.span();
        // `fn` or `init` or `deinit`
        let name = match self.advance().map(|t| t.node) {
            Some(Token::Fn) => {
                let (n, _) = self.expect_ident()?;
                n
            }
            Some(Token::Init) => "init".to_owned(),
            Some(Token::Deinit) => "deinit".to_owned(),
            _ => return Err(ParseError::eof(sp.clone())),
        };

        let generics = self.parse_generics_params()?;
        let params = self.parse_fn_params()?;

        let return_type = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let where_clause = self.parse_where_clause()?;

        let body = if self.peek() == Some(&Token::LBrace) {
            Some(self.parse_block()?)
        } else {
            // abstract / trait method sig
            None
        };

        Ok(FunctionDef {
            docs, visibility, is_async, is_inline, is_unsafe, is_comptime,
            name, generics, params, return_type, where_clause, body, span: sp,
        })
    }

    fn parse_fn_params(&mut self) -> ParseResult<Vec<Param>> {
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();

        while self.peek() != Some(&Token::RParen) {
            params.push(self.parse_param()?);
            if !self.eat(&Token::Comma) {
                break;
            }
        }

        self.expect(&Token::RParen)?;
        Ok(params)
    }

    fn parse_param(&mut self) -> ParseResult<Param> {
        let sp = self.span();

        // self param
        if self.peek() == Some(&Token::SelfKw) {
            self.advance();
            return Ok(Param {
                name: "self".to_owned(),
                ty: TypeExpr::SelfTy,
                default: None,
                is_self: true,
                span: sp,
            });
        }

        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type()?;

        let default = if self.eat(&Token::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        Ok(Param { name, ty, default, is_self: false, span: sp })
    }

    // ── Struct ────────────────────────────────────────────────────────────────

    fn parse_struct(&mut self, docs: Vec<String>, visibility: Visibility) -> ParseResult<StructDef> {
        let sp = self.span();
        self.expect(&Token::Struct)?;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics_params()?;
        self.expect(&Token::LBrace)?;

        let mut fields = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let fdocs = self.take_docs();
            let fvis = self.parse_visibility();
            let (fname, fsp) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(&Token::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.eat(&Token::Comma);
            fields.push(StructField {
                docs: fdocs, visibility: fvis,
                name: fname, ty, default, span: fsp,
            });
        }

        self.expect(&Token::RBrace)?;
        Ok(StructDef { docs, visibility, name, generics, fields, span: sp })
    }

    // ── Enum ──────────────────────────────────────────────────────────────────

    fn parse_enum(&mut self, docs: Vec<String>, visibility: Visibility) -> ParseResult<EnumDef> {
        let sp = self.span();
        self.expect(&Token::Enum)?;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics_params()?;
        self.expect(&Token::LBrace)?;

        let mut variants = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let vdocs = self.take_docs();
            let (vname, vsp) = self.expect_ident()?;

            let fields = if self.peek() == Some(&Token::LParen) {
                self.advance();
                let mut types = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    types.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                EnumVariantFields::Tuple(types)
            } else if self.peek() == Some(&Token::LBrace) {
                self.advance();
                let mut sfields = Vec::new();
                while self.peek() != Some(&Token::RBrace) {
                    let (fn_, fsp) = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let ty = self.parse_type()?;
                    self.eat(&Token::Comma);
                    sfields.push(StructField {
                        docs: vec![], visibility: Visibility::Public,
                        name: fn_, ty, default: None, span: fsp,
                    });
                }
                self.expect(&Token::RBrace)?;
                EnumVariantFields::Struct(sfields)
            } else {
                EnumVariantFields::Unit
            };

            self.eat(&Token::Comma);
            variants.push(EnumVariant { docs: vdocs, name: vname, fields, span: vsp });
        }

        self.expect(&Token::RBrace)?;
        Ok(EnumDef { docs, visibility, name, generics, variants, span: sp })
    }

    // ── Trait ─────────────────────────────────────────────────────────────────

    fn parse_trait(&mut self, docs: Vec<String>, visibility: Visibility) -> ParseResult<TraitDef> {
        let sp = self.span();
        self.expect(&Token::Trait)?;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics_params()?;

        let mut supertraits = Vec::new();
        if self.eat(&Token::Colon) {
            supertraits.push(self.parse_type()?);
            while self.eat(&Token::Plus) {
                supertraits.push(self.parse_type()?);
            }
        }

        self.expect(&Token::LBrace)?;
        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let ti_docs = self.take_docs();
            let ti_vis = self.parse_visibility();
            let is_async = self.eat(&Token::Async);
            if self.peek() == Some(&Token::Fn) {
                items.push(TraitItem::Method(self.parse_fn(
                    ti_docs, ti_vis, is_async, false, false, false,
                )?));
            } else if self.eat(&Token::Type) {
                let (tn, _) = self.expect_ident()?;
                let default = if self.eat(&Token::Assign) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                items.push(TraitItem::AssocType(tn, default));
            } else if self.peek() == Some(&Token::Const) {
                items.push(TraitItem::Constant(self.parse_const(ti_docs, ti_vis)?));
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(TraitDef { docs, visibility, name, generics, supertraits, items, span: sp })
    }

    // ── Impl ──────────────────────────────────────────────────────────────────

    fn parse_impl(&mut self) -> ParseResult<ImplBlock> {
        let sp = self.span();
        self.expect(&Token::Impl)?;
        let generics = self.parse_generics_params()?;

        // impl Trait for Type  OR  impl Type
        let first = self.parse_type()?;
        let (trait_name, self_type) = if self.eat(&Token::For) {
            let st = self.parse_type()?;
            (Some(first), st)
        } else {
            (None, first)
        };

        let where_clause = self.parse_where_clause()?;
        self.expect(&Token::LBrace)?;

        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            items.push(self.parse_item()?);
        }

        self.expect(&Token::RBrace)?;
        Ok(ImplBlock { generics, trait_name, self_type, where_clause, items, span: sp })
    }

    // ── Type alias ────────────────────────────────────────────────────────────

    fn parse_type_alias(&mut self, docs: Vec<String>, visibility: Visibility) -> ParseResult<TypeAlias> {
        let sp = self.span();
        self.expect(&Token::Type)?;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics_params()?;
        self.expect(&Token::Assign)?;
        let ty = self.parse_type()?;
        Ok(TypeAlias { docs, visibility, name, generics, ty, span: sp })
    }

    // ── Const ─────────────────────────────────────────────────────────────────

    fn parse_const(&mut self, docs: Vec<String>, visibility: Visibility) -> ParseResult<ConstDef> {
        let sp = self.span();
        self.expect(&Token::Const)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(ConstDef { docs, visibility, name, ty, value, span: sp })
    }

    // ── Extern ────────────────────────────────────────────────────────────────

    fn parse_extern(&mut self) -> ParseResult<ExternBlock> {
        let sp = self.span();
        self.expect(&Token::Extern)?;
        let abi = if let Some(Token::StringLit(s)) = self.peek().cloned() {
            self.advance();
            Some(s.trim_matches('"').to_owned())
        } else {
            None
        };
        self.expect(&Token::LBrace)?;
        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let docs = self.take_docs();
            let vis = self.parse_visibility();
            let is_async = self.eat(&Token::Async);
            items.push(self.parse_fn(docs, vis, is_async, false, false, false)?);
        }
        self.expect(&Token::RBrace)?;
        Ok(ExternBlock { abi, items, span: sp })
    }

    // ── Generics ──────────────────────────────────────────────────────────────

    fn parse_generics_params(&mut self) -> ParseResult<Vec<GenericParam>> {
        if self.peek() != Some(&Token::LAngle) {
            return Ok(vec![]);
        }
        self.advance();
        let mut params = Vec::new();
        while self.peek() != Some(&Token::RAngle) {
            let (name, _) = self.expect_ident()?;
            let bounds = if self.eat(&Token::Colon) {
                let mut bs = vec![self.parse_type()?];
                while self.eat(&Token::Plus) {
                    bs.push(self.parse_type()?);
                }
                bs
            } else {
                vec![]
            };
            params.push(GenericParam { name, bounds });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RAngle)?;
        Ok(params)
    }

    fn parse_where_clause(&mut self) -> ParseResult<Vec<WherePredicate>> {
        if !self.eat(&Token::Where) {
            return Ok(vec![]);
        }
        let mut preds = Vec::new();
        loop {
            let ty = self.parse_type()?;
            self.expect(&Token::Colon)?;
            let mut bounds = vec![self.parse_type()?];
            while self.eat(&Token::Plus) {
                bounds.push(self.parse_type()?);
            }
            preds.push(WherePredicate { ty, bounds });
            if !self.eat(&Token::Comma) { break; }
            if matches!(self.peek(), Some(Token::LBrace) | None) { break; }
        }
        Ok(preds)
    }

    // ── Types ─────────────────────────────────────────────────────────────────

    fn parse_type(&mut self) -> ParseResult<TypeExpr> {
        let sp = self.span();
        let base = match self.peek().cloned() {
            // &T
            Some(Token::Amp) => {
                self.advance();
                let inner = self.parse_type()?;
                TypeExpr::Ref(Box::new(inner))
            }
            // *T
            Some(Token::Star) => {
                self.advance();
                let inner = self.parse_type()?;
                TypeExpr::Ptr(Box::new(inner))
            }
            // [T] or [T; N]
            Some(Token::LBracket) => {
                self.advance();
                let inner = self.parse_type()?;
                if self.eat(&Token::Semicolon) {
                    let len = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    TypeExpr::Array(Box::new(inner), Box::new(len))
                } else {
                    self.expect(&Token::RBracket)?;
                    TypeExpr::Slice(Box::new(inner))
                }
            }
            // (T, U) tuple
            Some(Token::LParen) => {
                self.advance();
                let mut types = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    types.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                TypeExpr::Tuple(types)
            }
            Some(Token::Bang) => {
                self.advance();
                TypeExpr::Never
            }
            Some(Token::Underscore) => {
                self.advance();
                TypeExpr::Infer
            }
            Some(Token::SelfType) => {
                self.advance();
                TypeExpr::SelfTy
            }
            Some(Token::Tvoid) => {
                self.advance();
                TypeExpr::Void
            }
            Some(Token::Fn) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let mut args = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    args.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                self.expect(&Token::Arrow)?;
                let ret = self.parse_type()?;
                TypeExpr::Function(args, Box::new(ret))
            }
            // Built-ins & named
            _ => {
                let name = self.parse_type_name()?;
                let args = if self.peek() == Some(&Token::LAngle) {
                    self.advance();
                    let mut gs = Vec::new();
                    while self.peek() != Some(&Token::RAngle) {
                        gs.push(self.parse_type()?);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::RAngle)?;
                    gs
                } else {
                    vec![]
                };
                TypeExpr::Named(name, args)
            }
        };

        // T? → Optional
        if self.eat(&Token::Question) {
            return Ok(TypeExpr::Optional(Box::new(base)));
        }
        // T! → Result<T, Error>
        if self.eat(&Token::Bang) {
            return Ok(TypeExpr::Result(Box::new(base), None));
        }

        Ok(base)
    }

    fn parse_type_name(&mut self) -> ParseResult<String> {
        let sp = self.span();
        let name = match self.peek().cloned() {
            Some(Token::Ti8)    => { self.advance(); "i8" }
            Some(Token::Ti16)   => { self.advance(); "i16" }
            Some(Token::Ti32)   => { self.advance(); "i32" }
            Some(Token::Ti64)   => { self.advance(); "i64" }
            Some(Token::Ti128)  => { self.advance(); "i128" }
            Some(Token::Tu8)    => { self.advance(); "u8" }
            Some(Token::Tu16)   => { self.advance(); "u16" }
            Some(Token::Tu32)   => { self.advance(); "u32" }
            Some(Token::Tu64)   => { self.advance(); "u64" }
            Some(Token::Tu128)  => { self.advance(); "u128" }
            Some(Token::Tf32)   => { self.advance(); "f32" }
            Some(Token::Tf64)   => { self.advance(); "f64" }
            Some(Token::Tbool)  => { self.advance(); "bool" }
            Some(Token::Tstr)   => { self.advance(); "String" }
            Some(Token::Tchar)  => { self.advance(); "char" }
            Some(Token::Tusize) => { self.advance(); "usize" }
            Some(Token::Tisize) => { self.advance(); "isize" }
            Some(Token::Ident(s)) => { let s2 = s.clone(); self.advance(); Box::leak(s2.into_boxed_str()) }
            _ => return Err(ParseError::unexpected("type".into(), format!("{:?}", self.peek()), sp)),
        };
        Ok(name.to_owned())
    }

    // ── Block ─────────────────────────────────────────────────────────────────

    fn parse_block(&mut self) -> ParseResult<Block> {
        let sp = self.span();
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();

        while self.peek() != Some(&Token::RBrace) {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }

        // Check if last stmt is a bare expr (tail)
        let tail = if let Some(Stmt::Expr(e)) = stmts.last().cloned() {
            stmts.pop();
            Some(Box::new(e))
        } else {
            None
        };

        self.expect(&Token::RBrace)?;
        Ok(Block { stmts, tail, span: sp })
    }

    // ── Statements ────────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        let sp = self.span();

        match self.peek().cloned() {
            Some(Token::Let) => {
                self.advance();
                let name = self.parse_pattern()?;
                let ty = if self.eat(&Token::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let value = if self.eat(&Token::Assign) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                Ok(Stmt::Let(LetStmt { name, ty, value, span: sp }))
            }
            Some(Token::Var) => {
                self.advance();
                let name = self.parse_pattern()?;
                let ty = if self.eat(&Token::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let value = if self.eat(&Token::Assign) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                Ok(Stmt::Var(VarStmt { name, ty, value, span: sp }))
            }
            Some(Token::Return) => {
                self.advance();
                let val = if self.peek() == Some(&Token::RBrace) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(Stmt::Return(val, sp))
            }
            Some(Token::Break) => {
                self.advance();
                let val = if matches!(self.peek(), Some(Token::RBrace) | Some(Token::Semicolon) | None) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(Stmt::Break(val, sp))
            }
            Some(Token::Continue) => {
                self.advance();
                Ok(Stmt::Continue(sp))
            }
            Some(Token::Defer) => {
                self.advance();
                let e = self.parse_expr()?;
                Ok(Stmt::Defer(e, sp))
            }
            Some(Token::Throw) => {
                self.advance();
                let e = self.parse_expr()?;
                Ok(Stmt::Throw(e, sp))
            }
            // Nested items
            Some(Token::Fn) | Some(Token::Struct) | Some(Token::Enum)
            | Some(Token::Trait) | Some(Token::Impl) | Some(Token::Type)
            | Some(Token::Const) => {
                Ok(Stmt::Item(self.parse_item()?))
            }
            _ => {
                let e = self.parse_expr()?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    // ── Patterns ──────────────────────────────────────────────────────────────

    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        match self.peek().cloned() {
            Some(Token::Underscore) => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Some(Token::Ident(name)) => {
                self.advance();
                Ok(Pattern::Ident(name))
            }
            Some(Token::LParen) => {
                self.advance();
                let mut pats = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    pats.push(self.parse_pattern()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                Ok(Pattern::Tuple(pats))
            }
            _ => Ok(Pattern::Wildcard),
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────────

    pub fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_or()?;
        let sp = lhs.span.clone();

        match self.peek().cloned() {
            Some(Token::Assign) => {
                self.advance();
                let rhs = self.parse_assign()?;
                Ok(Expr { kind: ExprKind::Assign(Box::new(lhs), Box::new(rhs)), span: sp })
            }
            Some(Token::AddAssign) => {
                self.advance();
                let rhs = self.parse_assign()?;
                Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Add, Box::new(lhs), Box::new(rhs)), span: sp })
            }
            Some(Token::SubAssign) => {
                self.advance();
                let rhs = self.parse_assign()?;
                Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Sub, Box::new(lhs), Box::new(rhs)), span: sp })
            }
            Some(Token::MulAssign) => {
                self.advance();
                let rhs = self.parse_assign()?;
                Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Mul, Box::new(lhs), Box::new(rhs)), span: sp })
            }
            Some(Token::DivAssign) => {
                self.advance();
                let rhs = self.parse_assign()?;
                Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Div, Box::new(lhs), Box::new(rhs)), span: sp })
            }
            _ => Ok(lhs),
        }
    }

    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Some(Token::PipePipe) | Some(Token::Or)) {
            let sp = lhs.span.clone();
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Expr { kind: ExprKind::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_equality()?;
        while matches!(self.peek(), Some(Token::AmpAmp) | Some(Token::And)) {
            let sp = lhs.span.clone();
            self.advance();
            let rhs = self.parse_equality()?;
            lhs = Expr { kind: ExprKind::Binary(BinOp::And, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_equality(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_comparison()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::EqEq)  => BinOp::Eq,
                Some(Token::NotEq) => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_comparison()?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_comparison(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_additive()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::LAngle) => BinOp::Lt,
                Some(Token::RAngle) => BinOp::Gt,
                Some(Token::LtEq)   => BinOp::LtEq,
                Some(Token::GtEq)   => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_additive()?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::Plus)  => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative()?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::Star)    => BinOp::Mul,
                Some(Token::Slash)   => BinOp::Div,
                Some(Token::Percent) => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Minus) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Unary(UnaryOp::Neg, Box::new(e)), span: sp })
            }
            Some(Token::Bang) | Some(Token::Not) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Unary(UnaryOp::Not, Box::new(e)), span: sp })
            }
            Some(Token::Amp) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Ref(Box::new(e)), span: sp })
            }
            Some(Token::Star) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Deref(Box::new(e)), span: sp })
            }
            Some(Token::Await) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Await(Box::new(e)), span: sp })
            }
            Some(Token::Spawn) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Spawn(Box::new(e)), span: sp })
            }
            Some(Token::Comptime) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::Comptime(Box::new(e)), span: sp })
            }
            Some(Token::Arena) => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr { kind: ExprKind::ArenaAlloc(Box::new(e)), span: sp })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut base = self.parse_primary()?;

        loop {
            let sp = base.span.clone();
            match self.peek().cloned() {
                // method call / field
                Some(Token::Dot) => {
                    self.advance();
                    let (name, _) = self.expect_ident()?;
                    if self.peek() == Some(&Token::LParen) {
                        let args = self.parse_call_args()?;
                        base = Expr {
                            kind: ExprKind::MethodCall(Box::new(base), name, vec![], args),
                            span: sp,
                        };
                    } else {
                        base = Expr {
                            kind: ExprKind::Field(Box::new(base), name),
                            span: sp,
                        };
                    }
                }
                // index
                Some(Token::LBracket) => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    base = Expr {
                        kind: ExprKind::Index(Box::new(base), Box::new(idx)),
                        span: sp,
                    };
                }
                // call
                Some(Token::LParen) => {
                    let args = self.parse_call_args()?;
                    base = Expr {
                        kind: ExprKind::Call(Box::new(base), args),
                        span: sp,
                    };
                }
                // try propagation ?
                Some(Token::Question) => {
                    self.advance();
                    base = Expr {
                        kind: ExprKind::Try(Box::new(base)),
                        span: sp,
                    };
                }
                // cast: expr as Type
                Some(Token::As) => {
                    self.advance();
                    let ty = self.parse_type()?;
                    base = Expr {
                        kind: ExprKind::Cast(Box::new(base), ty),
                        span: sp,
                    };
                }
                // is Type check
                Some(Token::Is) => {
                    self.advance();
                    let ty = self.parse_type()?;
                    base = Expr {
                        kind: ExprKind::Is(Box::new(base), ty),
                        span: sp,
                    };
                }
                _ => break,
            }
        }

        Ok(base)
    }

    fn parse_call_args(&mut self) -> ParseResult<Vec<CallArg>> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();

        while self.peek() != Some(&Token::RParen) {
            // labelled arg: name: value
            let label = if let Some(Token::Ident(n)) = self.peek().cloned() {
                if self.peek2() == Some(&Token::Colon) {
                    self.advance();
                    self.advance();
                    Some(n)
                } else {
                    None
                }
            } else {
                None
            };
            let value = self.parse_expr()?;
            args.push(CallArg { label, value });
            if !self.eat(&Token::Comma) { break; }
        }

        self.expect(&Token::RParen)?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        let sp = self.span();

        match self.peek().cloned() {
            // Literals
            Some(Token::NumberLit(s)) => {
                self.advance();
                let lit = if s.contains('.') || s.contains('e') || s.contains('E') {
                    LiteralKind::Float(s.replace('_', "").parse().unwrap_or(0.0))
                } else {
                    LiteralKind::Int(s.replace('_', "").parse().unwrap_or(0))
                };
                Ok(Expr { kind: ExprKind::Literal(lit), span: sp })
            }
            Some(Token::StringLit(s)) => {
                self.advance();
                let inner = s[1..s.len()-1].to_owned();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Str(inner)), span: sp })
            }
            Some(Token::CharLit(s)) => {
                self.advance();
                let ch = s.chars().nth(1).unwrap_or('\0');
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Char(ch)), span: sp })
            }
            Some(Token::True) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Bool(true)), span: sp })
            }
            Some(Token::False) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Bool(false)), span: sp })
            }
            Some(Token::Nil) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Nil), span: sp })
            }
            Some(Token::SelfKw) => {
                self.advance();
                Ok(Expr { kind: ExprKind::SelfExpr, span: sp })
            }
            // Block
            Some(Token::LBrace) => {
                let block = self.parse_block()?;
                Ok(Expr { kind: ExprKind::Block(block), span: sp })
            }
            // Grouped / tuple
            Some(Token::LParen) => {
                self.advance();
                if self.peek() == Some(&Token::RParen) {
                    self.advance();
                    return Ok(Expr { kind: ExprKind::Tuple(vec![]), span: sp });
                }
                let e = self.parse_expr()?;
                if self.eat(&Token::Comma) {
                    let mut elems = vec![e];
                    while self.peek() != Some(&Token::RParen) {
                        elems.push(self.parse_expr()?);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr { kind: ExprKind::Tuple(elems), span: sp })
                } else {
                    self.expect(&Token::RParen)?;
                    Ok(e)
                }
            }
            // Array
            Some(Token::LBracket) => {
                self.advance();
                let mut elems = Vec::new();
                while self.peek() != Some(&Token::RBracket) {
                    elems.push(self.parse_expr()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr { kind: ExprKind::Array(elems), span: sp })
            }
            // if / elif / else
            Some(Token::If) => self.parse_if(),
            // while
            Some(Token::While) => {
                self.advance();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Expr { kind: ExprKind::While(Box::new(cond), body), span: sp })
            }
            // for x in iter
            Some(Token::For) => {
                self.advance();
                let pat = self.parse_pattern()?;
                self.expect(&Token::In)?;
                let iter = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Expr { kind: ExprKind::For(pat, Box::new(iter), body), span: sp })
            }
            // match
            Some(Token::Match) => self.parse_match(),
            // closure: |params| -> RetType body
            Some(Token::Pipe) => self.parse_closure(),
            // unsafe block
            Some(Token::Unsafe) => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr { kind: ExprKind::Unsafe(block), span: sp })
            }
            // Identifier (possibly struct literal)
            Some(Token::Ident(name)) => {
                self.advance();
                // struct literal: Foo { field: val, ... }
                if self.peek() == Some(&Token::LBrace) {
                    // peek to confirm it's a struct lit
                    self.advance();
                    let mut fields = Vec::new();
                    while self.peek() != Some(&Token::RBrace) {
                        let (fname, _) = self.expect_ident()?;
                        self.expect(&Token::Colon)?;
                        let fval = self.parse_expr()?;
                        self.eat(&Token::Comma);
                        fields.push((fname, fval));
                    }
                    self.expect(&Token::RBrace)?;
                    Ok(Expr { kind: ExprKind::StructLit(name, fields), span: sp })
                } else {
                    Ok(Expr { kind: ExprKind::Ident(name), span: sp })
                }
            }
            Some(tok) => Err(ParseError::unexpected("expression".into(), format!("{tok:?}"), sp)),
            None => Err(ParseError::eof(sp)),
        }
    }

    fn parse_if(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;

        let mut elif_branches = Vec::new();
        let mut else_branch = None;

        loop {
            if self.eat(&Token::Elif) {
                let ec = self.parse_expr()?;
                let eb = self.parse_block()?;
                elif_branches.push((ec, eb));
            } else if self.eat(&Token::Else) {
                else_branch = Some(self.parse_block()?);
                break;
            } else {
                break;
            }
        }

        Ok(Expr {
            kind: ExprKind::If(Box::new(cond), body, elif_branches, else_branch),
            span: sp,
        })
    }

    fn parse_match(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::Match)?;
        let subject = self.parse_expr()?;
        self.expect(&Token::LBrace)?;

        let mut arms = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let pattern = self.parse_pattern()?;
            let guard = if self.eat(&Token::When) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&Token::FatArrow)?;
            let body = self.parse_expr()?;
            self.eat(&Token::Comma);
            arms.push(MatchArm { pattern, guard, body });
        }

        self.expect(&Token::RBrace)?;
        Ok(Expr { kind: ExprKind::Match(Box::new(subject), arms), span: sp })
    }

    fn parse_closure(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::Pipe)?;
        let mut params = Vec::new();
        while self.peek() != Some(&Token::Pipe) {
            let psp = self.span();
            let (name, _) = self.expect_ident()?;
            let ty = if self.eat(&Token::Colon) {
                self.parse_type()?
            } else {
                TypeExpr::Infer
            };
            params.push(Param { name, ty, default: None, is_self: false, span: psp });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::Pipe)?;
        let ret_ty = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_expr()?;
        Ok(Expr { kind: ExprKind::Closure(params, ret_ty, Box::new(body)), span: sp })
    }
}

// ─── Token equality for matching ─────────────────────────────────────────────

trait TokenPlus {
    fn plus(&self) -> bool;
}
impl TokenPlus for Token {
    fn plus(&self) -> bool { matches!(self, Token::Pipe) }
}

impl Token {
    fn for_(&self) -> bool { matches!(self, Token::For) }
}

// Convenience: allow `Plus` for trait bounds
#[allow(dead_code)]
const PLUS_STR: &str = "+";

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        use Token::*;
        match (self, other) {
            (DocComment(a), DocComment(b)) => a == b,
            (MultiLineComment(a), MultiLineComment(b)) => a == b,
            (LineComment(a), LineComment(b)) => a == b,
            (NumberLit(a), NumberLit(b)) => a == b,
            (StringLit(a), StringLit(b)) => a == b,
            (CharLit(a), CharLit(b)) => a == b,
            (Ident(a), Ident(b)) => a == b,
            // For all other tokens, just check discriminant
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

// We need a `Plus` token for trait bound parsing (use Pipe as + placeholder)
impl Parser {
    #[allow(dead_code)]
    fn eat_plus(&mut self) -> bool {
        // In Vira syntax `+` for trait bounds is represented by `|` token for now
        // or we could extend Token with a Plus variant — here we use `Pipe`
        self.eat(&Token::Pipe)
    }
}
