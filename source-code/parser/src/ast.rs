use crate::lexer::Span;

// ─── Top-level ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
}

// ─── Imports ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Import {
    pub kind: ImportKind,
    pub name: String,
    pub version: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportKind {
    Native,   // use <tauri>
    Crate,    // using <serde>
}

// ─── Items ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplBlock),
    TypeAlias(TypeAlias),
    Constant(ConstDef),
    ExternBlock(ExternBlock),
}

// ─── Functions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_inline: bool,
    pub is_unsafe: bool,
    pub is_comptime: bool,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
    pub is_self: bool,
    pub span: Span,
}

// ─── Structs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StructDef {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
    pub span: Span,
}

// ─── Enums ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub docs: Vec<String>,
    pub name: String,
    pub fields: EnumVariantFields,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum EnumVariantFields {
    Unit,
    Tuple(Vec<TypeExpr>),
    Struct(Vec<StructField>),
}

// ─── Traits ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TraitDef {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub supertraits: Vec<TypeExpr>,
    pub items: Vec<TraitItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TraitItem {
    Method(FunctionDef),
    AssocType(String, Option<TypeExpr>),
    Constant(ConstDef),
}

// ─── Impl ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub generics: Vec<GenericParam>,
    pub trait_name: Option<TypeExpr>,
    pub self_type: TypeExpr,
    pub where_clause: Vec<WherePredicate>,
    pub items: Vec<Item>,
    pub span: Span,
}

// ─── Type alias ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub ty: TypeExpr,
    pub span: Span,
}

// ─── Constants ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub docs: Vec<String>,
    pub visibility: Visibility,
    pub name: String,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: Span,
}

// ─── Extern ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExternBlock {
    pub abi: Option<String>,
    pub items: Vec<FunctionDef>,
    pub span: Span,
}

// ─── Generics ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<TypeExpr>,
}

#[derive(Debug, Clone)]
pub struct WherePredicate {
    pub ty: TypeExpr,
    pub bounds: Vec<TypeExpr>,
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String, Vec<TypeExpr>),
    Ref(Box<TypeExpr>),
    RefMut(Box<TypeExpr>),
    Ptr(Box<TypeExpr>),
    Slice(Box<TypeExpr>),
    Array(Box<TypeExpr>, Box<Expr>),
    Tuple(Vec<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
    Result(Box<TypeExpr>, Option<Box<TypeExpr>>),
    Never,
    Infer,
    SelfTy,
    Void,
}

impl TypeExpr {
    pub fn is_infer(&self) -> bool {
        matches!(self, TypeExpr::Infer)
    }
}

// ─── Statements ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Var(VarStmt),
    Expr(Expr),
    Return(Option<Expr>, Span),
    Break(Option<Expr>, Span),
    Continue(Span),
    Defer(Expr, Span),
    Throw(Expr, Span),
    Item(Item),
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub name: Pattern,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct VarStmt {
    pub name: Pattern,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
    pub span: Span,
}

// ─── Patterns ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Pattern {
    Ident(String),
    Wildcard,
    Tuple(Vec<Pattern>),
    Struct(String, Vec<(String, Pattern)>),
    Enum(String, Vec<Pattern>),
    Literal(LiteralKind),
    Or(Vec<Pattern>),
    Ref(Box<Pattern>),
    Range(Box<Pattern>, Box<Pattern>),
}

// ─── Expressions ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(LiteralKind),
    Ident(String),
    // Qualified path: Foo::Bar, Foo::Bar::Baz, Foo::Bar(args)
    Path(Vec<String>),
    SelfExpr,
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    CompoundAssign(BinOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<CallArg>),
    MethodCall(Box<Expr>, String, Vec<TypeExpr>, Vec<CallArg>),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Closure(Vec<Param>, Option<TypeExpr>, Box<Expr>),
    Block(Block),
    If(Box<Expr>, Block, Vec<(Expr, Block)>, Option<Block>),
    While(Box<Expr>, Block),
    For(Pattern, Box<Expr>, Block),
    Match(Box<Expr>, Vec<MatchArm>),
    // Struct literal: Foo { field: val }
    StructLit(String, Vec<(String, Expr)>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Range(Option<Box<Expr>>, Option<Box<Expr>>, bool),
    Cast(Box<Expr>, TypeExpr),
    Is(Box<Expr>, TypeExpr),
    Try(Box<Expr>),
    Await(Box<Expr>),
    Spawn(Box<Expr>),
    Comptime(Box<Expr>),
    ArenaAlloc(Box<Expr>),
    Ref(Box<Expr>),
    RefMut(Box<Expr>),
    Deref(Box<Expr>),
    Unsafe(Block),
    // Macro invocation: foo!(...) or foo![...] or foo!{...}
    // path = ["tauri", "generate_handler"], bang_bracket = '[' or '(' or '{'
    MacroCall(Vec<String>, char, Vec<Expr>),
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub label: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

// ─── Literals ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LiteralKind {
    Int(i128),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Nil,
}

// ─── Operators ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
    Deref,
    Ref,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Private,
    Public,
}
