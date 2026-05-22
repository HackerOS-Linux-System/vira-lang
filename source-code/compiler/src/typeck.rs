use std::collections::HashMap;
use vira_parser::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticBag};

// ─── Type representation ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ViraType {
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128, Usize, Isize,
    F32, F64,
    Bool,
    Str,        // String
    Char,
    Void,
    Never,
    Unknown,    // type inference — not yet resolved
    Named(String, Vec<ViraType>),  // Foo<Bar>
    Optional(Box<ViraType>),       // T?
    Result(Box<ViraType>, Box<ViraType>), // T!E
    Slice(Box<ViraType>),          // [T]
    Tuple(Vec<ViraType>),
    Function(Vec<ViraType>, Box<ViraType>),
}

impl ViraType {
    pub fn from_type_expr(te: &TypeExpr) -> Self {
        match te {
            TypeExpr::Named(name, args) => {
                let targs: Vec<ViraType> = args.iter().map(ViraType::from_type_expr).collect();
                match name.as_str() {
                    "i8"     => ViraType::I8,    "i16"  => ViraType::I16,
                    "i32"    => ViraType::I32,   "i64"  => ViraType::I64,
                    "i128"   => ViraType::I128,
                    "u8"     => ViraType::U8,    "u16"  => ViraType::U16,
                    "u32"    => ViraType::U32,   "u64"  => ViraType::U64,
                    "u128"   => ViraType::U128,  "usize"=> ViraType::Usize,
                    "isize"  => ViraType::Isize,
                    "f32"    => ViraType::F32,   "f64"  => ViraType::F64,
                    "bool"   => ViraType::Bool,
                    "String" | "str" => ViraType::Str,
                    "char"   => ViraType::Char,
                    _ => ViraType::Named(name.clone(), targs),
                }
            }
            TypeExpr::Optional(inner) => ViraType::Optional(Box::new(ViraType::from_type_expr(inner))),
            TypeExpr::Result(ok, err)  => ViraType::Result(
                Box::new(ViraType::from_type_expr(ok)),
                                                           Box::new(err.as_ref().map(|e| ViraType::from_type_expr(e))
                                                           .unwrap_or(ViraType::Named("ViraError".into(), vec![]))),
            ),
            TypeExpr::Slice(inner)  => ViraType::Slice(Box::new(ViraType::from_type_expr(inner))),
            TypeExpr::Tuple(types)  => ViraType::Tuple(types.iter().map(ViraType::from_type_expr).collect()),
            TypeExpr::Void          => ViraType::Void,
            TypeExpr::Never         => ViraType::Never,
            TypeExpr::Infer         => ViraType::Unknown,
            TypeExpr::SelfTy        => ViraType::Named("Self".into(), vec![]),
            TypeExpr::Ref(inner)    => ViraType::from_type_expr(inner),
            TypeExpr::RefMut(inner) => ViraType::from_type_expr(inner),
            TypeExpr::Ptr(inner)    => ViraType::from_type_expr(inner),
            TypeExpr::Array(inner, _) => ViraType::Slice(Box::new(ViraType::from_type_expr(inner))),
            TypeExpr::Function(args, ret) => ViraType::Function(
                args.iter().map(ViraType::from_type_expr).collect(),
                                                                Box::new(ViraType::from_type_expr(ret)),
            ),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, ViraType::I8 | ViraType::I16 | ViraType::I32 | ViraType::I64
        | ViraType::I128 | ViraType::U8 | ViraType::U16 | ViraType::U32
        | ViraType::U64 | ViraType::U128 | ViraType::Usize | ViraType::Isize
        | ViraType::F32 | ViraType::F64)
    }

    pub fn display(&self) -> String {
        match self {
            ViraType::I32   => "i32".into(),   ViraType::I64 => "i64".into(),
            ViraType::U32   => "u32".into(),   ViraType::U64 => "u64".into(),
            ViraType::F32   => "f32".into(),   ViraType::F64 => "f64".into(),
            ViraType::Bool  => "bool".into(),  ViraType::Str => "str".into(),
            ViraType::Char  => "char".into(),  ViraType::Void => "void".into(),
            ViraType::Never => "never".into(), ViraType::Unknown => "_".into(),
            ViraType::Usize => "usize".into(), ViraType::Isize => "isize".into(),
            ViraType::Optional(t) => format!("{}?", t.display()),
            ViraType::Result(t,e) => format!("{}!{}", t.display(), e.display()),
            ViraType::Slice(t)    => format!("[{}]", t.display()),
            ViraType::Named(n,gs) if gs.is_empty() => n.clone(),
            ViraType::Named(n,gs) => format!("{}<{}>", n, gs.iter().map(|g| g.display()).collect::<Vec<_>>().join(", ")),
            ViraType::Tuple(ts)   => format!("({})", ts.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ")),
            ViraType::Function(args, ret) => format!(
                "fn({}) -> {}",
                                                     args.iter().map(|a| a.display()).collect::<Vec<_>>().join(", "),
                                                     ret.display()
            ),
            _ => "?".into(),
        }
    }
}

// ─── Symbol table ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: ViraType,
    pub mutable: bool,
    pub defined_at: (usize, usize), // (line, col)
}

#[derive(Default)]
pub struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn define(&mut self, name: String, ty: ViraType, mutable: bool, line: usize, col: usize) {
        self.symbols.insert(name.clone(), Symbol { name, ty, mutable, defined_at: (line, col) });
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

pub struct TypeEnvironment {
    scopes: Vec<Scope>,
    /// Function signatures: name → (param_types, return_type)
    pub functions: HashMap<String, (Vec<ViraType>, ViraType)>,
    /// Struct definitions: name → field types
    pub structs: HashMap<String, HashMap<String, ViraType>>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        let mut env = TypeEnvironment {
            scopes: vec![Scope::default()],
            functions: HashMap::new(),
            structs: HashMap::new(),
        };
        // Built-in functions
        env.functions.insert("println".into(), (vec![ViraType::Str], ViraType::Void));
        env.functions.insert("eprintln".into(), (vec![ViraType::Str], ViraType::Void));
        env.functions.insert("format".into(), (vec![ViraType::Str], ViraType::Str));
        env.functions.insert("panic".into(), (vec![ViraType::Str], ViraType::Never));
        env.functions.insert("assert".into(), (vec![ViraType::Bool], ViraType::Void));
        env
    }

    pub fn push_scope(&mut self) { self.scopes.push(Scope::default()); }
    pub fn pop_scope(&mut self)  { if self.scopes.len() > 1 { self.scopes.pop(); } }

    pub fn define(&mut self, name: String, ty: ViraType, mutable: bool, line: usize, col: usize) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.define(name, ty, mutable, line, col);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.lookup(name) {
                return Some(sym);
            }
        }
        None
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&(Vec<ViraType>, ViraType)> {
        self.functions.get(name)
    }
}

// ─── Type checker ─────────────────────────────────────────────────────────────

pub struct TypeChecker {
    pub env: TypeEnvironment,
    pub bag: DiagnosticBag,
    file: String,
}

impl TypeChecker {
    pub fn new(file: impl Into<String>) -> Self {
        TypeChecker {
            env: TypeEnvironment::new(),
            bag: DiagnosticBag::new(),
            file: file.into(),
        }
    }

    pub fn check_program(&mut self, program: &Program) {
        // First pass: collect all top-level function/struct signatures
        for item in &program.items {
            self.collect_item_sig(item);
        }
        // Second pass: check bodies
        for item in &program.items {
            self.check_item(item);
        }
    }

    fn collect_item_sig(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                let param_types: Vec<ViraType> = f.params.iter()
                .map(|p| ViraType::from_type_expr(&p.ty))
                .collect();
                let ret = f.return_type.as_ref()
                .map(ViraType::from_type_expr)
                .unwrap_or(ViraType::Void);
                self.env.functions.insert(f.name.clone(), (param_types, ret));
            }
            Item::Struct(s) => {
                let fields: HashMap<String, ViraType> = s.fields.iter()
                .map(|f| (f.name.clone(), ViraType::from_type_expr(&f.ty)))
                .collect();
                self.env.structs.insert(s.name.clone(), fields);
            }
            Item::Impl(imp) => {
                for item in &imp.items {
                    self.collect_item_sig(item);
                }
            }
            _ => {}
        }
    }

    fn check_item(&mut self, item: &Item) {
        match item {
            Item::Function(f)    => self.check_fn(f),
            Item::Impl(imp)      => { for it in &imp.items { self.check_item(it); } }
            Item::Struct(_)      => {} // struct fields checked at usage
            _ => {}
        }
    }

    fn check_fn(&mut self, f: &FunctionDef) {
        self.env.push_scope();
        // Define parameters
        for param in &f.params {
            if param.is_self { continue; }
            let ty = ViraType::from_type_expr(&param.ty);
            self.env.define(param.name.clone(), ty, false, param.span.line, param.span.col);
        }
        if let Some(body) = &f.body {
            let _ret = self.check_block(body);
            // TODO: compare _ret with f.return_type when type inference is complete
        }
        self.env.pop_scope();
    }

    fn check_block(&mut self, block: &Block) -> ViraType {
        self.env.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
        let tail_ty = block.tail.as_ref()
        .map(|e| self.check_expr(e))
        .unwrap_or(ViraType::Void);
        self.env.pop_scope();
        tail_ty
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(l) => {
                let val_ty = l.value.as_ref()
                .map(|e| self.check_expr(e))
                .unwrap_or(ViraType::Unknown);
                let ty = l.ty.as_ref()
                .map(ViraType::from_type_expr)
                .unwrap_or(val_ty.clone());
                if let Pattern::Ident(name) = &l.name {
                    self.env.define(name.clone(), ty, false, l.span.line, l.span.col);
                }
            }
            Stmt::Var(v) => {
                let val_ty = v.value.as_ref()
                .map(|e| self.check_expr(e))
                .unwrap_or(ViraType::Unknown);
                let ty = v.ty.as_ref()
                .map(ViraType::from_type_expr)
                .unwrap_or(val_ty.clone());
                if let Pattern::Ident(name) = &v.name {
                    self.env.define(name.clone(), ty, true, v.span.line, v.span.col);
                }
            }
            Stmt::Expr(e) => { self.check_expr(e); }
            Stmt::Return(Some(e), _) => { self.check_expr(e); }
            Stmt::Throw(e, _sp) => {
                let _ty = self.check_expr(e);
                // throw must be throwable
                // (we just record, actual check would need return type context)
            }
            Stmt::Item(item) => self.check_item(item),
            _ => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> ViraType {
        match &expr.kind {
            ExprKind::Literal(lit) => self.type_of_literal(lit),

            ExprKind::Ident(name) => {
                match self.env.lookup(name) {
                    Some(sym) => sym.ty.clone(),
                    None => {
                        // Only warn for lowercase names — uppercase likely a type/const
                        if name.chars().next().map_or(false, |c| c.is_lowercase()) {
                            self.warn_undefined(name, &expr.span);
                        }
                        ViraType::Unknown
                    }
                }
            }

            ExprKind::Path(_) => ViraType::Unknown, // paths (enum variants etc) — skip

            ExprKind::Binary(op, lhs, rhs) => {
                let lt = self.check_expr(lhs);
                let rt = self.check_expr(rhs);
                match op {
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt |
                    BinOp::LtEq | BinOp::GtEq | BinOp::And | BinOp::Or => ViraType::Bool,
                    BinOp::Add if lt == ViraType::Str || rt == ViraType::Str => ViraType::Str,
                    _ => if lt != ViraType::Unknown { lt } else { rt },
                }
            }

            ExprKind::Call(callee, args) => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => Some(n.as_str()),
                    ExprKind::Path(segs) => segs.last().map(|s| s.as_str()),
                    _ => None,
                };
                // Check argument count for known functions
                if let Some(fname) = name {
                    if let Some((param_tys, ret_ty)) = self.env.lookup_fn(fname).cloned() {
                        let expected = param_tys.len();
                        let got = args.len();
                        // allow self methods to have one less arg
                        if expected != got && !(got + 1 == expected) {
                            self.bag.push(
                                Diagnostic::warning(
                                    "Argument count mismatch",
                                    format!(
                                        "Function `{fname}` expects {expected} argument(s), \
but you passed {got}."
                                    ),
                                )
                                .at(expr.span.line, expr.span.col, expr.span.col + fname.len())
                                .in_file(&self.file)
                                .hint(format!("Check the signature of `{fname}`."))
                            );
                        }
                        for arg in args { self.check_expr(&arg.value); }
                        return ret_ty;
                    }
                }
                for arg in args { self.check_expr(&arg.value); }
                ViraType::Unknown
            }

            ExprKind::MethodCall(recv, method, _, args) => {
                let _recv_ty = self.check_expr(recv);
                for arg in args { self.check_expr(&arg.value); }
                // Look up return type hint from stdlib
                crate::stdlib::resolve_method(method)
                .return_hint
                .map(|h| ViraType::Named(h.into(), vec![]))
                .unwrap_or(ViraType::Unknown)
            }

            ExprKind::If(cond, then, elifs, else_) => {
                let cond_ty = self.check_expr(cond);
                if cond_ty != ViraType::Bool && cond_ty != ViraType::Unknown {
                    self.bag.push(
                        Diagnostic::warning(
                            "Non-boolean condition",
                            format!(
                                "The `if` condition has type `{}`, but I expected `bool`.",
                                cond_ty.display()
                            ),
                        )
                        .at(expr.span.line, expr.span.col, expr.span.col + 2)
                        .in_file(&self.file)
                        .hint("Make sure your condition evaluates to true or false.")
                    );
                }
                self.check_block(then);
                for (ec, eb) in elifs { self.check_expr(ec); self.check_block(eb); }
                if let Some(e) = else_ { self.check_block(e); }
                ViraType::Unknown
            }

            ExprKind::Block(b) => self.check_block(b),

            ExprKind::While(cond, body) => {
                self.check_expr(cond);
                self.check_block(body);
                ViraType::Void
            }

            ExprKind::For(pat, iter, body) => {
                let iter_ty = self.check_expr(iter);
                self.env.push_scope();
                // Register loop variable in scope
                if let Pattern::Ident(name) = pat {
                    // iter type is Slice<T> → T is the element type
                    let elem_ty = match &iter_ty {
                        ViraType::Slice(inner) => *inner.clone(),
                        _ => ViraType::Unknown,
                    };
                    self.env.define(name.clone(), elem_ty, true, expr.span.line, expr.span.col);
                }
                self.check_block(body);
                self.env.pop_scope();
                ViraType::Void
            }

            ExprKind::Match(subject, arms) => {
                self.check_expr(subject);
                for arm in arms {
                    self.env.push_scope();
                    // Register binding from pattern into scope (e.g. `snap => ...`)
                    match &arm.pattern {
                        Pattern::Ident(name) if name != "_" => {
                            self.env.define(name.clone(), ViraType::Unknown, false,
                                            expr.span.line, expr.span.col);
                        }
                        Pattern::Enum(_, fields) => {
                            for f in fields {
                                if let Pattern::Ident(n) = f {
                                    self.env.define(n.clone(), ViraType::Unknown, false,
                                                    expr.span.line, expr.span.col);
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(g) = &arm.guard { self.check_expr(g); }
                    self.check_expr(&arm.body);
                    self.env.pop_scope();
                }
                ViraType::Unknown
            }

            ExprKind::Assign(lhs, rhs) => {
                // Check that lhs is mutable
                if let ExprKind::Ident(name) = &lhs.kind {
                    if let Some(sym) = self.env.lookup(name) {
                        if !sym.mutable {
                            self.bag.push(
                                Diagnostic::error(
                                    "Cannot assign to immutable binding",
                                    format!(
                                        "The variable `{name}` is defined with `let` (immutable).\n\
To make it mutable, use `var` instead."
                                    ),
                                )
                                .at(expr.span.line, expr.span.col, expr.span.col + name.len())
                                .in_file(&self.file)
                                .hint(format!("Change `let {name}` to `var {name}` where it is defined."))
                            );
                        }
                    }
                }
                self.check_expr(lhs);
                self.check_expr(rhs);
                ViraType::Void
            }

            ExprKind::Field(obj, _) => {
                self.check_expr(obj);
                ViraType::Unknown
            }

            ExprKind::Await(e) | ExprKind::Try(e) | ExprKind::Deref(e) | ExprKind::Ref(e) => {
                self.check_expr(e)
            }

            ExprKind::Closure(params, ret, body) => {
                self.env.push_scope();
                for p in params {
                    let ty = ViraType::from_type_expr(&p.ty);
                    self.env.define(p.name.clone(), ty, false, p.span.line, p.span.col);
                }
                let body_ty = self.check_expr(body);
                self.env.pop_scope();
                let ret_ty = ret.as_ref().map(ViraType::from_type_expr).unwrap_or(body_ty);
                ViraType::Function(
                    params.iter().map(|p| ViraType::from_type_expr(&p.ty)).collect(),
                                   Box::new(ret_ty),
                )
            }

            ExprKind::Array(elems) => {
                for e in elems { self.check_expr(e); }
                ViraType::Slice(Box::new(ViraType::Unknown))
            }

            ExprKind::Tuple(elems) => {
                let tys: Vec<_> = elems.iter().map(|e| self.check_expr(e)).collect();
                ViraType::Tuple(tys)
            }

            ExprKind::StructLit(name, fields) => {
                for (_, v) in fields { self.check_expr(v); }
                ViraType::Named(name.clone(), vec![])
            }

            _ => ViraType::Unknown,
        }
    }

    fn type_of_literal(&self, lit: &LiteralKind) -> ViraType {
        match lit {
            LiteralKind::Int(_)   => ViraType::I64,
            LiteralKind::Float(_) => ViraType::F64,
            LiteralKind::Str(_)   => ViraType::Str,
            LiteralKind::Char(_)  => ViraType::Char,
            LiteralKind::Bool(_)  => ViraType::Bool,
            LiteralKind::Nil      => ViraType::Optional(Box::new(ViraType::Unknown)),
        }
    }

    fn warn_undefined(&mut self, name: &str, span: &vira_parser::lexer::Span) {
        self.bag.push(
            Diagnostic::warning(
                "Possibly undefined variable",
                format!(
                    "I could not find `{name}` in the current scope.\n\
It might be defined in an outer scope or not yet declared."
                ),
            )
            .at(span.line, span.col, span.col + name.len())
            .in_file(&self.file)
        );
    }
}
