use logos::Logos;

/// All tokens of the Vira language.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")] // skip whitespace
pub enum Token {
    // ─── Comments ──────────────────────────────────────────────────────────────
    /// Documentation comment: /// ...
    #[regex(r"///[^\n]*", |lex| lex.slice().to_owned())]
    DocComment(String),

    /// Multi-line comment: // content \\
    #[regex(r"//[^\\][^\n]*\\\\", |lex| lex.slice().to_owned())]
    MultiLineComment(String),

    /// Single-line comment: ;; ...
    #[regex(r";;[^\n]*", |lex| lex.slice().to_owned())]
    LineComment(String),

    // ─── Imports ───────────────────────────────────────────────────────────────
    /// Native API import: use <tauri>, use <gtk:4.0>, use <qt:6>
    #[token("use")]
    Use,

    /// Ecosystem Rust crate import: using <name:version>
    #[token("using")]
    Using,

    // ─── Keywords ──────────────────────────────────────────────────────────────
    #[token("fn")]      Fn,
    #[token("let")]     Let,
    #[token("var")]     Var,
    #[token("const")]   Const,
    #[token("type")]    Type,
    #[token("struct")]  Struct,
    #[token("enum")]    Enum,
    #[token("trait")]   Trait,
    #[token("impl")]    Impl,
    #[token("return")]  Return,
    #[token("if")]      If,
    #[token("else")]    Else,
    #[token("elif")]    Elif,
    #[token("while")]   While,
    #[token("for")]     For,
    #[token("in")]      In,
    #[token("match")]   Match,
    #[token("when")]    When,
    #[token("break")]   Break,
    #[token("continue")]Continue,
    #[token("pub")]     Pub,
    #[token("extern")]  Extern,
    #[token("async")]   Async,
    #[token("await")]   Await,
    #[token("spawn")]   Spawn,
    #[token("defer")]   Defer,
    #[token("comptime")]Comptime,
    #[token("inline")]  Inline,
    #[token("unsafe")]  Unsafe,
    #[token("nil")]     Nil,
    #[token("true")]    True,
    #[token("false")]   False,
    #[token("self")]    SelfKw,
    #[token("Self")]    SelfType,
    #[token("init")]    Init,
    #[token("deinit")]  Deinit,
    #[token("throw")]   Throw,
    #[token("try")]     Try,
    #[token("catch")]   Catch,
    #[token("as")]      As,
    #[token("is")]      Is,
    #[token("and")]     And,
    #[token("or")]      Or,
    #[token("not")]     Not,
    #[token("where")]   Where,
    #[token("arena")]   Arena,

    // ─── Built-in types ────────────────────────────────────────────────────────
    #[token("i8")]   Ti8,
    #[token("i16")]  Ti16,
    #[token("i32")]  Ti32,
    #[token("i64")]  Ti64,
    #[token("i128")] Ti128,
    #[token("u8")]   Tu8,
    #[token("u16")]  Tu16,
    #[token("u32")]  Tu32,
    #[token("u64")]  Tu64,
    #[token("u128")] Tu128,
    #[token("f32")]  Tf32,
    #[token("f64")]  Tf64,
    #[token("bool")] Tbool,
    #[token("str")]  Tstr,
    #[token("char")] Tchar,
    #[token("void")] Tvoid,
    #[token("usize")]Tusize,
    #[token("isize")]Tisize,

    // ─── Literals ──────────────────────────────────────────────────────────────
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?", |lex| lex.slice().to_owned())]
    NumberLit(String),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice().to_owned())]
    StringLit(String),

    #[regex(r"'([^'\\]|\\.)'", |lex| lex.slice().to_owned())]
    CharLit(String),

    // ─── Identifiers ───────────────────────────────────────────────────────────
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_owned())]
    Ident(String),

    // ─── Punctuation ───────────────────────────────────────────────────────────
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token("<")] LAngle,
    #[token(">")] RAngle,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token("..")] DotDot,
    #[token("...")] Ellipsis,
    #[token(":")] Colon,
    #[token("::")] DoubleColon,
    #[token(";")] Semicolon,
    #[token("->")] Arrow,
    #[token("=>")] FatArrow,
    #[token("?")] Question,
    #[token("!")] Bang,
    #[token("@")] At,
    #[token("#")] Hash,
    #[token("&")] Amp,
    #[token("&&")] AmpAmp,
    #[token("|")] Pipe,
    #[token("||")] PipePipe,
    #[token("^")] Caret,
    #[token("~")] Tilde,
    #[token("_")] Underscore,

    // ─── Operators ─────────────────────────────────────────────────────────────
    #[token("=")] Assign,
    #[token("+=")] AddAssign,
    #[token("-=")] SubAssign,
    #[token("*=")] MulAssign,
    #[token("/=")] DivAssign,
    #[token("%=")] ModAssign,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
    #[token("==")] EqEq,
    #[token("!=")] NotEq,
    #[token("<=")] LtEq,
    #[token(">=")] GtEq,
}

// ─── Span + Spanned token ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Span { start, end, line, col }
    }
}

#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned { node, span }
    }
}

// ─── Lex helper ───────────────────────────────────────────────────────────────

pub type SpannedToken = Spanned<Token>;

pub fn lex(source: &str) -> Result<Vec<SpannedToken>, Vec<LexError>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    let mut line = 1usize;
    let mut last_newline = 0usize;

    let mut lex = Token::lexer(source);

    while let Some(result) = lex.next() {
        let range = lex.span();
        // track lines
        for ch in &source[last_newline..range.start].chars().collect::<Vec<_>>() {
            if *ch == '\n' {
                line += 1;
                last_newline = range.start;
            }
        }
        let col = range.start.saturating_sub(last_newline) + 1;
        let span = Span::new(range.start, range.end, line, col);

        match result {
            Ok(token) => {
                // skip comments from token stream but preserve doc comments
                match &token {
                    Token::LineComment(_) | Token::MultiLineComment(_) => {
                        // swallow normal comments
                    }
                    _ => tokens.push(Spanned::new(token, span)),
                }
            }
            Err(_) => {
                errors.push(LexError {
                    span,
                    src: source[range.start..range.end].to_owned(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: Span,
    pub src: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unexpected token '{}' at line {}:{}",
            self.src, self.span.line, self.span.col
        )
    }
}
