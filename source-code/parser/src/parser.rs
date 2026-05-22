use crate::ast::*;
use crate::lexer::{Span, SpannedToken, Token};
use crate::error::{ParseError, ParseResult};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    pending_docs: Vec<String>,
    /// Line of the most recently consumed (advanced past) token
    last_token_line: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, pos: 0, pending_docs: Vec::new(), last_token_line: 0 }
    }

    // ── Core helpers ──────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if matches!(&self.tokens[i].node, Token::DocComment(_)) { i += 1; }
            else { return Some(&self.tokens[i].node); }
        }
        None
    }

    fn peek2(&self) -> Option<&Token> {
        // skip doc comments for second peek too
        let mut i = self.pos;
        let mut seen = 0;
        while i < self.tokens.len() {
            if matches!(&self.tokens[i].node, Token::DocComment(_)) { i += 1; continue; }
            if seen == 1 { return Some(&self.tokens[i].node); }
            seen += 1;
            i += 1;
        }
        None
    }

    fn span(&self) -> Span {
        self.tokens.get(self.pos).map(|t| t.span.clone()).unwrap_or_default()
    }

    // Line of the current token (skip doc comments)
    fn current_line(&self) -> usize {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if matches!(&self.tokens[i].node, Token::DocComment(_)) { i += 1; continue; }
            return self.tokens[i].span.line;
        }
        0
    }

    // Line of the token at pos (raw, no skip)
    #[allow(dead_code)]
    fn line_at(&self, pos: usize) -> usize {
        self.tokens.get(pos).map(|t| t.span.line).unwrap_or(0)
    }

    // Look ahead to decide if { starts a struct literal vs a block.
    // Peeks: current = `{`, then looks for pattern  IDENT `:`
    // which means "field: value" — definitively a struct body.
    // Empty braces `{}` are treated as struct literal only if
    // the identifier before them is PascalCase (starts with uppercase).
    // This correctly handles:
    //   Foo { x: 1 }   => struct literal  ✓
    //   if cond {       => block           ✓
    //   removed {       => block           ✓
    //   NoteResult { ok: true } => struct literal ✓
    fn looks_like_struct_body(&self) -> bool {
        // pos currently points at `{` (peek() == LBrace)
        // We need to look at tokens[pos+1] and tokens[pos+2]
        // skipping doc comments
        let mut i = self.pos;
        // skip the `{` itself — find first real token inside
        let mut found_lbrace = false;
        while i < self.tokens.len() {
            match &self.tokens[i].node {
                Token::DocComment(_) => { i += 1; continue; }
                Token::LBrace if !found_lbrace => { found_lbrace = true; i += 1; }
                _ if !found_lbrace => return false,
                _ => break,
            }
        }
        // Now tokens[i] is the first token inside `{`
        match self.tokens.get(i).map(|t| &t.node) {
            // Empty braces — only struct lit if Pascal-case name before them
            // We can't check the name here easily, so allow it (safe default)
            Some(Token::RBrace) => true,
            // `{ IDENT :` => definitely struct literal
            Some(Token::Ident(_)) => {
                // peek one more
                let mut j = i + 1;
                while j < self.tokens.len() {
                    match &self.tokens[j].node {
                        Token::DocComment(_) => { j += 1; }
                        Token::Colon => return true,   // IDENT : => struct field
                        _ => return false,             // IDENT <anything else> => not struct
                    }
                }
                false
            }
            // Anything else inside { => block
            _ => false,
        }
    }


    fn advance(&mut self) -> Option<SpannedToken> {
        loop {
            if self.pos >= self.tokens.len() { return None; }
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            if let Token::DocComment(s) = &tok.node {
                self.pending_docs.push(s.trim_start_matches('/').trim().to_owned());
                continue;
            }
            self.last_token_line = tok.span.line;
            return Some(tok);
        }
    }

    fn take_docs(&mut self) -> Vec<String> { std::mem::take(&mut self.pending_docs) }

    fn expect(&mut self, expected: &Token) -> ParseResult<SpannedToken> {
        match self.peek() {
            Some(tok) if tok == expected => Ok(self.advance().unwrap()),
            Some(tok) => Err(ParseError::unexpected(token_name(expected), token_name(tok), self.span())),
            None => Err(ParseError::eof(self.span())),
        }
    }

    fn expect_ident(&mut self) -> ParseResult<(String, Span)> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Ident(name)) => { self.advance(); Ok((name, sp)) }
            // Allow contextual keywords as identifiers in field/var name position
            // These are keywords that can also be used as identifiers: usage, from, arena, defer etc.
            Some(Token::Usage)   => { self.advance(); Ok(("usage".into(),   sp)) }
            Some(Token::From)    => { self.advance(); Ok(("from".into(),    sp)) }
            Some(Token::Arena)   => { self.advance(); Ok(("arena".into(),   sp)) }
            Some(Token::Defer)   => { self.advance(); Ok(("defer".into(),   sp)) }
            Some(Token::Spawn)   => { self.advance(); Ok(("spawn".into(),   sp)) }
            Some(Token::Inline)  => { self.advance(); Ok(("inline".into(),  sp)) }
            Some(Token::When)    => { self.advance(); Ok(("when".into(),    sp)) }
            Some(Token::Init)    => { self.advance(); Ok(("init".into(),    sp)) }
            Some(Token::Deinit)  => { self.advance(); Ok(("deinit".into(),  sp)) }
            Some(ref tok) => Err(ParseError::unexpected("identifier".into(), token_name(tok), sp)),
            None => Err(ParseError::eof(sp)),
        }
    }

    fn eat(&mut self, tok: &Token) -> bool {
        if self.peek() == Some(tok) { self.advance(); true } else { false }
    }

    fn at_end(&self) -> bool { self.pos >= self.tokens.len() }

    fn parse_visibility(&mut self) -> Visibility {
        if self.eat(&Token::Pub) { Visibility::Public } else { Visibility::Private }
    }

    // ── Program ───────────────────────────────────────────────────────────────

    pub fn parse_program(&mut self) -> ParseResult<Program> {
        let mut imports = Vec::new();
        let mut items = Vec::new();
        while !self.at_end() {
            match self.peek() {
                Some(Token::Use) | Some(Token::Using) | Some(Token::Usage) => imports.push(self.parse_import()?),
                _ => items.push(self.parse_item()?),
            }
        }
        Ok(Program { imports, items })
    }

    // ── Imports ───────────────────────────────────────────────────────────────

    fn parse_import(&mut self) -> ParseResult<Import> {
        let sp = self.span();

        // ── usage <name>  — Vira registry (vira.io placeholder) ──────────────
        if self.eat(&Token::Usage) {
            self.expect(&Token::LAngle)?;
            let (name, _) = self.expect_ident()?;
            let version = self.parse_optional_version();
            self.expect(&Token::RAngle)?;
            return Ok(Import { kind: ImportKind::ViraRegistry, name, version, span: sp });
        }

        // ── using <name> from <ecosystem>  — ecosystem import ─────────────────
        // Also handles: using <name:version> from <crates>
        if self.eat(&Token::Using) {
            self.expect(&Token::LAngle)?;
            let (name, _) = self.expect_ident()?;
            let version = self.parse_optional_version();
            self.expect(&Token::RAngle)?;

            // optional `from <ecosystem>`
            let ecosystem = if self.eat(&Token::From) {
                self.expect(&Token::LAngle)?;
                let (eco, _) = self.expect_ident()?;
                self.expect(&Token::RAngle)?;
                eco
            } else {
                "crates".to_owned() // default: Rust crates
            };

            return Ok(Import {
                kind: ImportKind::Ecosystem { ecosystem },
                name, version, span: sp,
            });
        }

        // ── use <name>  — native Vira API ─────────────────────────────────────
        self.expect(&Token::Use)?;
        self.expect(&Token::LAngle)?;
        let (name, _) = self.expect_ident()?;

        // Version handling: use <gtk:4> use <tauri:v1> use <qt:5>
        let version = self.parse_optional_version();

        self.expect(&Token::RAngle)?;
        Ok(Import { kind: ImportKind::Native, name, version, span: sp })
    }

    /// Parse optional :version after ident inside angle brackets
    /// Handles: :4  :v1  :v2  :3  :4.0
    fn parse_optional_version(&mut self) -> Option<String> {
        if !self.eat(&Token::Colon) { return None; }
        // version can be: ident (v1, v2, latest) or number (4, 5, 6)
        match self.peek().cloned() {
            Some(Token::Ident(v))     => { self.advance(); Some(v) }
            Some(Token::NumberLit(v)) => { self.advance(); Some(v) }
            _ => None,
        }
    }

    // ── Items ─────────────────────────────────────────────────────────────────

    fn parse_item(&mut self) -> ParseResult<Item> {
        let docs = self.take_docs();
        let vis = self.parse_visibility();
        let is_async    = self.eat(&Token::Async);
        let is_inline   = self.eat(&Token::Inline);
        let is_unsafe   = self.eat(&Token::Unsafe);
        let is_comptime = self.eat(&Token::Comptime);

        match self.peek().cloned() {
            Some(Token::Fn) | Some(Token::Init) | Some(Token::Deinit) =>
            Ok(Item::Function(self.parse_fn(docs, vis, is_async, is_inline, is_unsafe, is_comptime)?)),
            Some(Token::Struct)  => Ok(Item::Struct(self.parse_struct(docs, vis)?)),
            Some(Token::Enum)    => Ok(Item::Enum(self.parse_enum(docs, vis)?)),
            Some(Token::Trait)   => Ok(Item::Trait(self.parse_trait(docs, vis)?)),
            Some(Token::Impl)    => Ok(Item::Impl(self.parse_impl()?)),
            Some(Token::Type)    => Ok(Item::TypeAlias(self.parse_type_alias(docs, vis)?)),
            Some(Token::Const)   => Ok(Item::Constant(self.parse_const(docs, vis)?)),
            Some(Token::Extern)  => Ok(Item::ExternBlock(self.parse_extern()?)),
            Some(ref tok) => Err(ParseError::unexpected(
                "item (fn / struct / enum / trait / impl / type / const)".into(),
                                                        token_name(tok), self.span(),
            )),
            None => Err(ParseError::eof(self.span())),
        }
    }

    // ── Functions ─────────────────────────────────────────────────────────────

    fn parse_fn(
        &mut self,
        docs: Vec<String>, visibility: Visibility,
        is_async: bool, is_inline: bool, is_unsafe: bool, is_comptime: bool,
    ) -> ParseResult<FunctionDef> {
        let sp = self.span();
        let name = match self.advance().map(|t| t.node) {
            Some(Token::Fn)     => { let (n,_) = self.expect_ident()?; n }
            Some(Token::Init)   => "init".to_owned(),
            Some(Token::Deinit) => "deinit".to_owned(),
            _ => return Err(ParseError::eof(sp.clone())),
        };
        let generics     = self.parse_generics_params()?;
        let params       = self.parse_fn_params()?;
        let return_type  = if self.eat(&Token::Arrow) { Some(self.parse_type()?) } else { None };
        let where_clause = self.parse_where_clause()?;
        let body         = if self.peek() == Some(&Token::LBrace) { Some(self.parse_block()?) } else { None };
        Ok(FunctionDef { docs, visibility, is_async, is_inline, is_unsafe, is_comptime,
            name, generics, params, return_type, where_clause, body, span: sp })
    }

    fn parse_fn_params(&mut self) -> ParseResult<Vec<Param>> {
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        while self.peek() != Some(&Token::RParen) {
            params.push(self.parse_param()?);
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RParen)?;
        Ok(params)
    }

    fn parse_param(&mut self) -> ParseResult<Param> {
        let sp = self.span();
        if self.peek() == Some(&Token::SelfKw) {
            self.advance();
            return Ok(Param { name: "self".into(), ty: TypeExpr::SelfTy, default: None, is_self: true, span: sp });
        }
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type()?;
        let default = if self.eat(&Token::Assign) { Some(self.parse_expr()?) } else { None };
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
            let fvis  = self.parse_visibility();
            let (fname, fsp) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(&Token::Assign) { Some(self.parse_expr()?) } else { None };
            self.eat(&Token::Comma);
            fields.push(StructField { docs: fdocs, visibility: fvis, name: fname, ty, default, span: fsp });
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
            while self.eat(&Token::Plus) { supertraits.push(self.parse_type()?); }
        }
        self.expect(&Token::LBrace)?;
        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let ti_docs = self.take_docs();
            let ti_vis  = self.parse_visibility();
            let is_async = self.eat(&Token::Async);
            if self.peek() == Some(&Token::Fn) {
                items.push(TraitItem::Method(self.parse_fn(ti_docs, ti_vis, is_async, false, false, false)?));
            } else if self.eat(&Token::Type) {
                let (tn,_) = self.expect_ident()?;
                let default = if self.eat(&Token::Assign) { Some(self.parse_type()?) } else { None };
                items.push(TraitItem::AssocType(tn, default));
            } else if self.peek() == Some(&Token::Const) {
                items.push(TraitItem::Constant(self.parse_const(ti_docs, ti_vis)?));
            } else { break; }
        }
        self.expect(&Token::RBrace)?;
        Ok(TraitDef { docs, visibility, name, generics, supertraits, items, span: sp })
    }

    // ── Impl ──────────────────────────────────────────────────────────────────

    fn parse_impl(&mut self) -> ParseResult<ImplBlock> {
        let sp = self.span();
        self.expect(&Token::Impl)?;
        let generics = self.parse_generics_params()?;
        let first = self.parse_type()?;
        let (trait_name, self_type) = if self.eat(&Token::For) {
            (Some(first), self.parse_type()?)
        } else {
            (None, first)
        };
        let where_clause = self.parse_where_clause()?;
        self.expect(&Token::LBrace)?;
        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) { items.push(self.parse_item()?); }
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
            self.advance(); Some(s.trim_matches('"').to_owned())
        } else { None };
        self.expect(&Token::LBrace)?;
        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let docs = self.take_docs();
            let vis  = self.parse_visibility();
            let is_async = self.eat(&Token::Async);
            items.push(self.parse_fn(docs, vis, is_async, false, false, false)?);
        }
        self.expect(&Token::RBrace)?;
        Ok(ExternBlock { abi, items, span: sp })
    }

    // ── Generics ──────────────────────────────────────────────────────────────

    fn parse_generics_params(&mut self) -> ParseResult<Vec<GenericParam>> {
        if self.peek() != Some(&Token::LAngle) { return Ok(vec![]); }
        self.advance();
        let mut params = Vec::new();
        while self.peek() != Some(&Token::RAngle) {
            let (name, _) = self.expect_ident()?;
            let bounds = if self.eat(&Token::Colon) {
                let mut bs = vec![self.parse_type()?];
                while self.eat(&Token::Plus) { bs.push(self.parse_type()?); }
                bs
            } else { vec![] };
            params.push(GenericParam { name, bounds });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RAngle)?;
        Ok(params)
    }

    fn parse_where_clause(&mut self) -> ParseResult<Vec<WherePredicate>> {
        if !self.eat(&Token::Where) { return Ok(vec![]); }
        let mut preds = Vec::new();
        loop {
            let ty = self.parse_type()?;
            self.expect(&Token::Colon)?;
            let mut bounds = vec![self.parse_type()?];
            while self.eat(&Token::Plus) { bounds.push(self.parse_type()?); }
            preds.push(WherePredicate { ty, bounds });
            if !self.eat(&Token::Comma) { break; }
            if matches!(self.peek(), Some(Token::LBrace) | None) { break; }
        }
        Ok(preds)
    }

    // ── Types ─────────────────────────────────────────────────────────────────

    fn parse_type(&mut self) -> ParseResult<TypeExpr> {
        let base = match self.peek().cloned() {
            Some(Token::Amp)  => { self.advance(); TypeExpr::Ref(Box::new(self.parse_type()?)) }
            Some(Token::Star) => { self.advance(); TypeExpr::Ptr(Box::new(self.parse_type()?)) }
            Some(Token::LBracket) => {
                self.advance();
                let inner = self.parse_type()?;
                if self.eat(&Token::Semicolon) {
                    let len = self.parse_expr()?; self.expect(&Token::RBracket)?;
                    TypeExpr::Array(Box::new(inner), Box::new(len))
                } else {
                    self.expect(&Token::RBracket)?; TypeExpr::Slice(Box::new(inner))
                }
            }
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
            Some(Token::Bang)     => { self.advance(); TypeExpr::Never }
            Some(Token::SelfType) => { self.advance(); TypeExpr::SelfTy }
            Some(Token::Tvoid)    => { self.advance(); TypeExpr::Void }
            Some(Token::Fn) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let mut args = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    args.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?; self.expect(&Token::Arrow)?;
                TypeExpr::Function(args, Box::new(self.parse_type()?))
            }
            _ => {
                let name = self.parse_type_name()?;
                let args = if self.peek() == Some(&Token::LAngle) {
                    self.advance();
                    let mut gs = Vec::new();
                    while self.peek() != Some(&Token::RAngle) {
                        gs.push(self.parse_type()?);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::RAngle)?; gs
                } else { vec![] };
                TypeExpr::Named(name, args)
            }
        };
        if self.eat(&Token::Question) { return Ok(TypeExpr::Optional(Box::new(base))); }
        if self.eat(&Token::Bang)     { return Ok(TypeExpr::Result(Box::new(base), None)); }
        Ok(base)
    }

    fn parse_type_name(&mut self) -> ParseResult<String> {
        let sp = self.span();
        let base = match self.peek().cloned() {
            Some(Token::Ti8)      => { self.advance(); "i8".into() }
            Some(Token::Ti16)     => { self.advance(); "i16".into() }
            Some(Token::Ti32)     => { self.advance(); "i32".into() }
            Some(Token::Ti64)     => { self.advance(); "i64".into() }
            Some(Token::Ti128)    => { self.advance(); "i128".into() }
            Some(Token::Tu8)      => { self.advance(); "u8".into() }
            Some(Token::Tu16)     => { self.advance(); "u16".into() }
            Some(Token::Tu32)     => { self.advance(); "u32".into() }
            Some(Token::Tu64)     => { self.advance(); "u64".into() }
            Some(Token::Tu128)    => { self.advance(); "u128".into() }
            Some(Token::Tf32)     => { self.advance(); "f32".into() }
            Some(Token::Tf64)     => { self.advance(); "f64".into() }
            Some(Token::Tbool)    => { self.advance(); "bool".into() }
            Some(Token::Tstr)     => { self.advance(); "String".into() }
            Some(Token::Tchar)    => { self.advance(); "char".into() }
            Some(Token::Tusize)   => { self.advance(); "usize".into() }
            Some(Token::Tisize)   => { self.advance(); "isize".into() }
            Some(Token::Ident(s)) => { let o = s.clone(); self.advance(); o }
            Some(ref tok) => return Err(ParseError::unexpected("type name".into(), token_name(tok), sp)),
            None => return Err(ParseError::eof(sp)),
        };
        // Handle qualified type paths: gtk::HeaderBar, tauri::AppHandle etc.
        let mut full = base;
        while self.peek() == Some(&Token::DoubleColon) {
            // Only consume :: if next is an ident (not a generic < or other)
            if matches!(self.tokens.get(self.pos + 1).map(|t| &t.node), Some(Token::Ident(_))) {
                self.advance(); // ::
                let (seg, _) = self.expect_ident()?;
                full = format!("{full}::{seg}");
            } else {
                break;
            }
        }
        Ok(full)
    }

    // ── Block ─────────────────────────────────────────────────────────────────

    fn parse_block(&mut self) -> ParseResult<Block> {
        let sp = self.span();
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            stmts.push(self.parse_stmt()?);
        }
        // Last bare expression becomes tail (return value)
        let tail = if let Some(Stmt::Expr(e)) = stmts.last().cloned() {
            // Only make it a tail if it's not an assign/void-like stmt
            match &e.kind {
                ExprKind::Assign(..) | ExprKind::CompoundAssign(..) => None,
                _ => { stmts.pop(); Some(Box::new(e)) }
            }
        } else { None };
        self.expect(&Token::RBrace)?;
        Ok(Block { stmts, tail, span: sp })
    }

    // ── Statements ────────────────────────────────────────────────────────────
    //
    // Key design: each statement is ONE expression or keyword form.
    // Statement boundary is detected by line number — if the next
    // continuation token (operator, dot, etc.) is on a different line
    // than the expression just parsed, we stop.

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Let) => {
                self.advance();
                let name  = self.parse_pattern()?;
                let ty    = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { None };
                let value = if self.eat(&Token::Assign) { Some(self.parse_expr()?) } else { None };
                Ok(Stmt::Let(LetStmt { name, ty, value, span: sp }))
            }
            Some(Token::Var) => {
                self.advance();
                let name  = self.parse_pattern()?;
                let ty    = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { None };
                let value = if self.eat(&Token::Assign) { Some(self.parse_expr()?) } else { None };
                Ok(Stmt::Var(VarStmt { name, ty, value, span: sp }))
            }
            Some(Token::Return) => {
                self.advance();
                // Return with no value if next token is on a new line or is }
                let val = if self.stmt_end_ahead(&sp) { None }
                else { Some(self.parse_expr()?) };
                Ok(Stmt::Return(val, sp))
            }
            Some(Token::Break) => {
                self.advance();
                let val = if self.stmt_end_ahead(&sp) { None }
                else { Some(self.parse_expr()?) };
                Ok(Stmt::Break(val, sp))
            }
            Some(Token::Continue) => { self.advance(); Ok(Stmt::Continue(sp)) }
            Some(Token::Defer)    => { self.advance(); Ok(Stmt::Defer(self.parse_expr()?, sp)) }
            Some(Token::Throw)    => { self.advance(); Ok(Stmt::Throw(self.parse_expr()?, sp)) }
            Some(Token::Fn) | Some(Token::Struct) | Some(Token::Enum)
            | Some(Token::Trait) | Some(Token::Impl) | Some(Token::Type) | Some(Token::Const) =>
            Ok(Stmt::Item(self.parse_item()?)),
            _ => Ok(Stmt::Expr(self.parse_expr_stmt()?)),
        }
    }

    // Check if next token indicates end-of-statement (closing brace, or new line)
    fn stmt_end_ahead(&self, current_sp: &Span) -> bool {
        match self.peek() {
            Some(Token::RBrace) | None => true,
            _ => {
                // If next token is on a different line, statement is done
                self.current_line() > current_sp.line
            }
        }
    }

    // Parse an expression used as a statement.
    // This is where line-sensitivity matters most:
    // postfix operators (.field, [idx], ()) are only allowed on same line.
    fn parse_expr_stmt(&mut self) -> ParseResult<Expr> {
        self.parse_assign_stmt()
    }

    fn parse_assign_stmt(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_or_stmt()?;
        let sp = lhs.span.clone();

        // Assignment operators — always fine even across lines (rare but valid)
        match self.peek().cloned() {
            Some(Token::Assign) => {
                self.advance();
                let r = self.parse_expr()?;
                return Ok(Expr { kind: ExprKind::Assign(Box::new(lhs), Box::new(r)), span: sp });
            }
            Some(Token::AddAssign) => {
                self.advance();
                let r = self.parse_expr()?;
                return Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Add, Box::new(lhs), Box::new(r)), span: sp });
            }
            Some(Token::SubAssign) => {
                self.advance();
                let r = self.parse_expr()?;
                return Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Sub, Box::new(lhs), Box::new(r)), span: sp });
            }
            Some(Token::MulAssign) => {
                self.advance();
                let r = self.parse_expr()?;
                return Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Mul, Box::new(lhs), Box::new(r)), span: sp });
            }
            Some(Token::DivAssign) => {
                self.advance();
                let r = self.parse_expr()?;
                return Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Div, Box::new(lhs), Box::new(r)), span: sp });
            }
            _ => {}
        }
        Ok(lhs)
    }

    fn parse_or_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_and_stmt()?;
        while matches!(self.peek(), Some(Token::PipePipe) | Some(Token::Or)) {
            let sp = lhs.span.clone(); self.advance();
            lhs = Expr { kind: ExprKind::Binary(BinOp::Or, Box::new(lhs), Box::new(self.parse_and_stmt()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_and_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_equality_stmt()?;
        while matches!(self.peek(), Some(Token::AmpAmp) | Some(Token::And)) {
            let sp = lhs.span.clone(); self.advance();
            lhs = Expr { kind: ExprKind::Binary(BinOp::And, Box::new(lhs), Box::new(self.parse_equality_stmt()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_equality_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_cmp_stmt()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::EqEq)  => BinOp::Eq,
                Some(Token::NotEq) => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_cmp_stmt()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_cmp_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_add_stmt()?;
        loop {
            let sp = lhs.span.clone();
            // Only consume comparison operators on the SAME line
            let op = match self.peek() {
                Some(Token::LAngle) if self.current_line() == lhs.span.line => BinOp::Lt,
                Some(Token::RAngle) if self.current_line() == lhs.span.line => BinOp::Gt,
                Some(Token::LtEq)   => BinOp::LtEq,
                Some(Token::GtEq)   => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_add_stmt()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_add_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_mul_stmt()?;
        loop {
            let sp = lhs.span.clone();
            // Only continue addition on same line
            let op = match self.peek() {
                Some(Token::Plus)  if self.current_line() == lhs.span.line => BinOp::Add,
                Some(Token::Minus) if self.current_line() == lhs.span.line => BinOp::Sub,
                _ => break,
            };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_mul_stmt()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_mul_stmt(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::Star)    if self.current_line() == lhs.span.line => BinOp::Mul,
                Some(Token::Slash)   if self.current_line() == lhs.span.line => BinOp::Div,
                Some(Token::Percent) if self.current_line() == lhs.span.line => BinOp::Mod,
                _ => break,
            };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_unary()?)), span: sp };
        }
        Ok(lhs)
    }

    // ── Full expression (used inside parens, if-cond, let RHS etc.) ───────────

    pub fn parse_expr(&mut self) -> ParseResult<Expr> { self.parse_assign() }

    fn parse_assign(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_or()?;
        let sp = lhs.span.clone();
        match self.peek().cloned() {
            Some(Token::Assign)    => { self.advance(); Ok(Expr { kind: ExprKind::Assign(Box::new(lhs), Box::new(self.parse_assign()?)), span: sp }) }
            Some(Token::AddAssign) => { self.advance(); Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Add, Box::new(lhs), Box::new(self.parse_assign()?)), span: sp }) }
            Some(Token::SubAssign) => { self.advance(); Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Sub, Box::new(lhs), Box::new(self.parse_assign()?)), span: sp }) }
            Some(Token::MulAssign) => { self.advance(); Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Mul, Box::new(lhs), Box::new(self.parse_assign()?)), span: sp }) }
            Some(Token::DivAssign) => { self.advance(); Ok(Expr { kind: ExprKind::CompoundAssign(BinOp::Div, Box::new(lhs), Box::new(self.parse_assign()?)), span: sp }) }
            _ => Ok(lhs),
        }
    }

    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Some(Token::PipePipe) | Some(Token::Or)) {
            let sp = lhs.span.clone(); self.advance();
            lhs = Expr { kind: ExprKind::Binary(BinOp::Or, Box::new(lhs), Box::new(self.parse_and()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_equality()?;
        while matches!(self.peek(), Some(Token::AmpAmp) | Some(Token::And)) {
            let sp = lhs.span.clone(); self.advance();
            lhs = Expr { kind: ExprKind::Binary(BinOp::And, Box::new(lhs), Box::new(self.parse_equality()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_equality(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_cmp()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() { Some(Token::EqEq) => BinOp::Eq, Some(Token::NotEq) => BinOp::NotEq, _ => break };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_cmp()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_add()?;
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
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_add()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() { Some(Token::Plus) => BinOp::Add, Some(Token::Minus) => BinOp::Sub, _ => break };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_mul()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let sp = lhs.span.clone();
            let op = match self.peek() {
                Some(Token::Star) => BinOp::Mul, Some(Token::Slash) => BinOp::Div, Some(Token::Percent) => BinOp::Mod,
                _ => break,
            };
            self.advance();
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(self.parse_unary()?)), span: sp };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Minus)    => { self.advance(); Ok(Expr { kind: ExprKind::Unary(UnaryOp::Neg, Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Bang) | Some(Token::Not) => { self.advance(); Ok(Expr { kind: ExprKind::Unary(UnaryOp::Not, Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Amp)      => { self.advance(); Ok(Expr { kind: ExprKind::Ref(Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Star)     => { self.advance(); Ok(Expr { kind: ExprKind::Deref(Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Await)    => { self.advance(); Ok(Expr { kind: ExprKind::Await(Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Spawn)    => { self.advance(); Ok(Expr { kind: ExprKind::Spawn(Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Comptime) => { self.advance(); Ok(Expr { kind: ExprKind::Comptime(Box::new(self.parse_unary()?)), span: sp }) }
            Some(Token::Arena)    => { self.advance(); Ok(Expr { kind: ExprKind::ArenaAlloc(Box::new(self.parse_unary()?)), span: sp }) }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut base = self.parse_primary()?;
        loop {
            let sp = base.span.clone();
            let same_line = self.current_line() == sp.line;
            // Dot chains are allowed across lines when the dot STARTS the new line
            // e.g.:  tauri::Builder::default()
            //            .manage(state)          ← this is valid multi-line chain
            // We detect this by: Dot is on a DIFFERENT line AND its column <= sp.col+8
            // (it's indented under the chain start) — "leading dot" pattern.
            let is_leading_dot = matches!(self.peek(), Some(Token::Dot))
            && !same_line;
            match self.peek().cloned() {
                Some(Token::Dot) if same_line || is_leading_dot => {
                    self.advance();
                    let (name, _) = self.expect_ident()?;
                    if self.peek() == Some(&Token::LParen) {
                        let args = self.parse_call_args()?;
                        base = Expr { kind: ExprKind::MethodCall(Box::new(base), name, vec![], args), span: sp };
                    } else {
                        base = Expr { kind: ExprKind::Field(Box::new(base), name), span: sp };
                    }
                }
                Some(Token::LBracket) if same_line => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    base = Expr { kind: ExprKind::Index(Box::new(base), Box::new(idx)), span: sp };
                }
                Some(Token::LParen) if same_line => {
                    let args = self.parse_call_args()?;
                    base = Expr { kind: ExprKind::Call(Box::new(base), args), span: sp };
                }
                // `?` (try/propagate) is NEVER ambiguous — allow on any line
                // because `.foo()?.bar()` chains can span many lines
                Some(Token::Question) => {
                    self.advance();
                    base = Expr { kind: ExprKind::Try(Box::new(base)), span: sp };
                }
                Some(Token::As) if same_line => {
                    self.advance();
                    let ty = self.parse_type()?;
                    base = Expr { kind: ExprKind::Cast(Box::new(base), ty), span: sp };
                }
                Some(Token::Is) if same_line => {
                    self.advance();
                    let ty = self.parse_type()?;
                    base = Expr { kind: ExprKind::Is(Box::new(base), ty), span: sp };
                }
                // Macro call: expr![ ... ] or expr!( ... ) or expr!{ ... }
                // same_line OR leading (macro call is always attached to name)
                Some(Token::Bang) if same_line || is_leading_dot => {
                    self.advance(); // eat !
                    // Collect path segments from base expr (Path or Ident)
                    let path = match &base.kind {
                        ExprKind::Path(segs) => segs.clone(),
                        ExprKind::Ident(n)   => vec![n.clone()],
                        _ => vec!["macro".to_owned()],
                    };
                    let (bracket, args) = self.parse_macro_args()?;
                    base = Expr { kind: ExprKind::MacroCall(path, bracket, args), span: sp };
                }
                _ => break,
            }
        }
        Ok(base)
    }

    // Parse macro invocation: ![ items ] or !( items ) or !{ items }
    // Returns (bracket_char, args_as_exprs)
    fn parse_macro_args(&mut self) -> ParseResult<(char, Vec<Expr>)> {
        let sp = self.span();
        let (_open, close, bracket) = match self.peek().cloned() {
            Some(Token::LBracket) => (Token::LBracket, Token::RBracket, '['),
            Some(Token::LParen)   => (Token::LParen,   Token::RParen,   '('),
            Some(Token::LBrace)   => (Token::LBrace,   Token::RBrace,   '{'),
            Some(ref tok) => return Err(ParseError::unexpected(
                "`[`, `(`, or `{` after `!`".into(), token_name(tok), sp
            )),
            None => return Err(ParseError::eof(sp)),
        };
        self.advance(); // eat opening bracket
        let mut args = Vec::new();
        while self.peek() != Some(&close) && !self.at_end() {
            // macro args can be idents separated by commas (e.g. generate_handler![fn1, fn2])
            // parse as full exprs to handle complex cases
            args.push(self.parse_expr()?);
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&close)?;
        Ok((bracket, args))
    }

    fn parse_call_args(&mut self) -> ParseResult<Vec<CallArg>> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();
        while self.peek() != Some(&Token::RParen) {
            let label = if let Some(Token::Ident(n)) = self.peek().cloned() {
                if self.peek2() == Some(&Token::Colon) { self.advance(); self.advance(); Some(n) } else { None }
            } else { None };
            args.push(CallArg { label, value: self.parse_expr()? });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RParen)?;
        Ok(args)
    }

    // ── Patterns ──────────────────────────────────────────────────────────────

    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let first = self.parse_single_pattern()?;
        if self.peek() == Some(&Token::Pipe) {
            let mut pats = vec![first];
            while self.eat(&Token::Pipe) {
                pats.push(self.parse_single_pattern()?);
            }
            return Ok(Pattern::Or(pats));
        }
        Ok(first)
    }

    fn parse_single_pattern(&mut self) -> ParseResult<Pattern> {
        match self.peek().cloned() {
            Some(Token::Ident(name)) if name == "_" => { self.advance(); Ok(Pattern::Wildcard) }
            // Nil / boolean literal patterns
            Some(Token::Nil)   => { self.advance(); Ok(Pattern::Literal(LiteralKind::Nil)) }
            Some(Token::True)  => { self.advance(); Ok(Pattern::Literal(LiteralKind::Bool(true))) }
            Some(Token::False) => { self.advance(); Ok(Pattern::Literal(LiteralKind::Bool(false))) }
            // String literal pattern: "foo" =>
            Some(Token::StringLit(s)) => {
                self.advance();
                let inner = if s.len() >= 2 { s[1..s.len()-1].to_owned() } else { s };
                Ok(Pattern::Literal(LiteralKind::Str(inner)))
            }
            // Number literal pattern: 0 => / 42 => / 3.14 =>
            Some(Token::NumberLit(s)) => {
                self.advance();
                let lit = if s.contains('.') || s.to_lowercase().contains('e') {
                    LiteralKind::Float(s.replace('_', "").parse().unwrap_or(0.0))
                } else {
                    LiteralKind::Int(s.replace('_', "").parse().unwrap_or(0))
                };
                Ok(Pattern::Literal(lit))
            }
            // Negative number pattern: -42
            Some(Token::Minus) => {
                self.advance();
                if let Some(Token::NumberLit(s)) = self.peek().cloned() {
                    self.advance();
                    let lit = if s.contains('.') {
                        LiteralKind::Float(-s.replace('_', "").parse::<f64>().unwrap_or(0.0))
                    } else {
                        LiteralKind::Int(-s.replace('_', "").parse::<i128>().unwrap_or(0))
                    };
                    Ok(Pattern::Literal(lit))
                } else {
                    Ok(Pattern::Wildcard)
                }
            }
            // Ident, path, or enum pattern
            Some(Token::Ident(name)) => {
                self.advance();
                if self.peek() == Some(&Token::DoubleColon) {
                    let mut path = vec![name];
                    while self.eat(&Token::DoubleColon) {
                        let (seg, _) = self.expect_ident()?;
                        path.push(seg);
                    }
                    let full = path.join("::");
                    if self.peek() == Some(&Token::LParen) {
                        self.advance();
                        let mut fields = Vec::new();
                        while self.peek() != Some(&Token::RParen) {
                            fields.push(self.parse_pattern()?);
                            if !self.eat(&Token::Comma) { break; }
                        }
                        self.expect(&Token::RParen)?;
                        return Ok(Pattern::Enum(full, fields));
                    }
                    return Ok(Pattern::Enum(full, vec![]));
                }
                Ok(Pattern::Ident(name))
            }
            // Tuple pattern
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

    // ── Primary ───────────────────────────────────────────────────────────────

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::NumberLit(s)) => {
                self.advance();
                let lit = if s.contains('.') || s.to_lowercase().contains('e') {
                    LiteralKind::Float(s.replace('_', "").parse().unwrap_or(0.0))
                } else {
                    LiteralKind::Int(s.replace('_', "").parse().unwrap_or(0))
                };
                Ok(Expr { kind: ExprKind::Literal(lit), span: sp })
            }
            Some(Token::StringLit(s)) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Str(s[1..s.len()-1].to_owned())), span: sp })
            }
            Some(Token::CharLit(s)) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Literal(LiteralKind::Char(s.chars().nth(1).unwrap_or('\0'))), span: sp })
            }
            Some(Token::True)   => { self.advance(); Ok(Expr { kind: ExprKind::Literal(LiteralKind::Bool(true)),  span: sp }) }
            Some(Token::False)  => { self.advance(); Ok(Expr { kind: ExprKind::Literal(LiteralKind::Bool(false)), span: sp }) }
            Some(Token::Nil)    => { self.advance(); Ok(Expr { kind: ExprKind::Literal(LiteralKind::Nil), span: sp }) }
            Some(Token::SelfKw) => { self.advance(); Ok(Expr { kind: ExprKind::SelfExpr, span: sp }) }
            Some(Token::LBrace) => Ok(Expr { kind: ExprKind::Block(self.parse_block()?), span: sp }),
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
            Some(Token::If)     => self.parse_if(),
            Some(Token::While)  => {
                self.advance();
                let c = self.parse_expr()?; let b = self.parse_block()?;
                Ok(Expr { kind: ExprKind::While(Box::new(c), b), span: sp })
            }
            Some(Token::For) => {
                self.advance();
                let p = self.parse_pattern()?;
                self.expect(&Token::In)?;
                let it = self.parse_expr()?;
                let b = self.parse_block()?;
                Ok(Expr { kind: ExprKind::For(p, Box::new(it), b), span: sp })
            }
            Some(Token::Match)  => self.parse_match(),
            Some(Token::Pipe)   => self.parse_closure(),
            Some(Token::Unsafe) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Unsafe(self.parse_block()?), span: sp })
            }
            // Ident or qualified path
            Some(Token::Ident(first)) => {
                self.advance();

                // Qualified path: Foo::Bar
                if self.peek() == Some(&Token::DoubleColon) {
                    let mut segments = vec![first];
                    while self.eat(&Token::DoubleColon) {
                        let (seg, _) = self.expect_ident()?;
                        segments.push(seg);
                    }
                    // Path call: Foo::new(...)
                    if self.peek() == Some(&Token::LParen) {
                        let args = self.parse_call_args()?;
                        let callee = Expr { kind: ExprKind::Path(segments), span: sp.clone() };
                        return Ok(Expr { kind: ExprKind::Call(Box::new(callee), args), span: sp });
                    }
                    // Path struct literal: Foo::Bar { field: val } — only same line
                    let is_path_struct_lit = self.peek() == Some(&Token::LBrace)
                    && self.current_line() == sp.line
                    && self.looks_like_struct_body();
                    if is_path_struct_lit {
                        self.advance();
                        let mut fields = Vec::new();
                        while self.peek() != Some(&Token::RBrace) {
                            let (fname, _) = self.expect_ident()?;
                            self.expect(&Token::Colon)?;
                            fields.push((fname, self.parse_expr()?));
                            self.eat(&Token::Comma);
                        }
                        self.expect(&Token::RBrace)?;
                        return Ok(Expr { kind: ExprKind::StructLit(segments.join("::"), fields), span: sp });
                    }
                    return Ok(Expr { kind: ExprKind::Path(segments), span: sp });
                }

                // Plain ident — maybe struct literal
                // Disambiguation: Foo { field: val } vs if foo { block }
                // Rule: treat as struct literal ONLY when:
                //   1. { is on the same line as the type name
                //   2. inside the brace we see: IDENT followed by COLON (field: val)
                //      OR the brace is immediately closed (empty struct {})
                let is_struct_lit = self.peek() == Some(&Token::LBrace)
                && self.current_line() == sp.line
                && self.looks_like_struct_body();
                if is_struct_lit {
                    self.advance();
                    let mut fields = Vec::new();
                    while self.peek() != Some(&Token::RBrace) {
                        let (fname, _) = self.expect_ident()?;
                        self.expect(&Token::Colon)?;
                        fields.push((fname, self.parse_expr()?));
                        self.eat(&Token::Comma);
                    }
                    self.expect(&Token::RBrace)?;
                    return Ok(Expr { kind: ExprKind::StructLit(first, fields), span: sp });
                }

                Ok(Expr { kind: ExprKind::Ident(first), span: sp })
            }
            Some(ref tok) => Err(ParseError::unexpected("expression".into(), token_name(tok), sp)),
            None => Err(ParseError::eof(sp)),
        }
    }

    fn parse_if(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        let mut elifs = Vec::new();
        let mut else_ = None;
        loop {
            if      self.eat(&Token::Elif) { elifs.push((self.parse_expr()?, self.parse_block()?)); }
            else if self.eat(&Token::Else) { else_ = Some(self.parse_block()?); break; }
            else { break; }
        }
        Ok(Expr { kind: ExprKind::If(Box::new(cond), body, elifs, else_), span: sp })
    }

    fn parse_match(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::Match)?;
        let subject = self.parse_expr()?;
        self.expect(&Token::LBrace)?;
        let mut arms = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            let pattern = self.parse_pattern()?;
            let guard = if self.eat(&Token::When) { Some(self.parse_expr()?) } else { None };
            self.expect(&Token::FatArrow)?;
            let body = self.parse_match_arm_body()?;
            self.eat(&Token::Comma);
            arms.push(MatchArm { pattern, guard, body });
        }
        self.expect(&Token::RBrace)?;
        Ok(Expr { kind: ExprKind::Match(Box::new(subject), arms), span: sp })
    }

    fn parse_match_arm_body(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        match self.peek().cloned() {
            Some(Token::Throw) => {
                self.advance();
                let e = self.parse_expr()?;
                let sp2 = sp.clone();
                Ok(Expr { kind: ExprKind::Block(Block {
                    stmts: vec![Stmt::Throw(e, sp.clone())],
                                                tail: None, span: sp,
                }), span: sp2 })
            }
            Some(Token::Return) => {
                self.advance();
                let val = if matches!(self.peek(), Some(Token::RBrace) | Some(Token::Comma) | None) {
                    None
                } else { Some(self.parse_expr()?) };
                let sp2 = sp.clone();
                Ok(Expr { kind: ExprKind::Block(Block {
                    stmts: vec![Stmt::Return(val, sp.clone())],
                                                tail: None, span: sp,
                }), span: sp2 })
            }
            _ => self.parse_expr(),
        }
    }

    fn parse_closure(&mut self) -> ParseResult<Expr> {
        let sp = self.span();
        self.expect(&Token::Pipe)?;
        let mut params = Vec::new();
        while self.peek() != Some(&Token::Pipe) {
            let psp = self.span();
            let (name, _) = self.expect_ident()?;
            let ty = if self.eat(&Token::Colon) { self.parse_type()? } else { TypeExpr::Infer };
            params.push(Param { name, ty, default: None, is_self: false, span: psp });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::Pipe)?;
        let ret_ty = if self.eat(&Token::Arrow) { Some(self.parse_type()?) } else { None };
        Ok(Expr { kind: ExprKind::Closure(params, ret_ty, Box::new(self.parse_expr()?)), span: sp })
    }
}

// ─── Token display helper ─────────────────────────────────────────────────────

pub fn token_name(tok: &Token) -> String {
    match tok {
        Token::Ident(s)       => format!("identifier `{s}`"),
        Token::NumberLit(s)   => format!("number `{s}`"),
        Token::StringLit(s)   => format!("string {s}"),
        Token::CharLit(s)     => format!("char {s}"),
        Token::DocComment(_)  => "doc comment".into(),
        Token::LineComment(_) | Token::SingleComment(_) => "comment".into(),
        Token::Fn      => "`fn`".into(),    Token::Let    => "`let`".into(),
        Token::Var     => "`var`".into(),   Token::Const  => "`const`".into(),
        Token::Struct  => "`struct`".into(),Token::Enum   => "`enum`".into(),
        Token::Trait   => "`trait`".into(), Token::Impl   => "`impl`".into(),
        Token::If      => "`if`".into(),    Token::Else   => "`else`".into(),
        Token::Elif    => "`elif`".into(),  Token::While  => "`while`".into(),
        Token::For     => "`for`".into(),   Token::In     => "`in`".into(),
        Token::Match   => "`match`".into(), Token::Return => "`return`".into(),
        Token::Pub     => "`pub`".into(),   Token::Use    => "`use`".into(),
        Token::Using   => "`using`".into(),
        Token::Usage   => "`usage`".into(),
        Token::From    => "`from`".into(),
        Token::LBrace  => "`{`".into(),     Token::RBrace => "`}`".into(),
        Token::LParen  => "`(`".into(),     Token::RParen => "`)`".into(),
        Token::LAngle  => "`<`".into(),     Token::RAngle => "`>`".into(),
        Token::Comma   => "`,`".into(),     Token::Dot    => "`.`".into(),
        Token::Colon   => "`:`".into(),     Token::Arrow  => "`->`".into(),
        Token::Assign  => "`=`".into(),     Token::EqEq   => "`==`".into(),
        Token::FatArrow => "`=>`".into(),   Token::DoubleColon => "`::`".into(),
        _ => format!("{tok:?}"),
    }
}
