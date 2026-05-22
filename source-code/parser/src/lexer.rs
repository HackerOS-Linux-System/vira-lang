use logos::Logos;

/// All tokens of the Vira language.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
pub enum Token {
    // ── Comments ──────────────────────────────────────────────────────────────
    #[regex(r"///[^\n]*", |lex| lex.slice().to_owned(), priority = 5)]
    DocComment(String),

    #[regex(r"//[^\n]*", |lex| lex.slice().to_owned(), priority = 4)]
    LineComment(String),

    #[regex(r";;[^\n]*", |lex| lex.slice().to_owned(), priority = 4)]
    SingleComment(String),

    // ── Keywords priority=2 (beat Ident priority=1) ───────────────────────────
    #[token("use",      priority = 2)] Use,
    #[token("using",    priority = 2)] Using,
    #[token("usage",    priority = 2)] Usage,
    #[token("from",     priority = 2)] From,
    #[token("fn",       priority = 2)] Fn,
    #[token("let",      priority = 2)] Let,
    #[token("var",      priority = 2)] Var,
    #[token("const",    priority = 2)] Const,
    #[token("type",     priority = 2)] Type,
    #[token("struct",   priority = 2)] Struct,
    #[token("enum",     priority = 2)] Enum,
    #[token("trait",    priority = 2)] Trait,
    #[token("impl",     priority = 2)] Impl,
    #[token("return",   priority = 2)] Return,
    #[token("if",       priority = 2)] If,
    #[token("else",     priority = 2)] Else,
    #[token("elif",     priority = 2)] Elif,
    #[token("while",    priority = 2)] While,
    #[token("for",      priority = 2)] For,
    #[token("in",       priority = 2)] In,
    #[token("match",    priority = 2)] Match,
    #[token("when",     priority = 2)] When,
    #[token("break",    priority = 2)] Break,
    #[token("continue", priority = 2)] Continue,
    #[token("pub",      priority = 2)] Pub,
    #[token("extern",   priority = 2)] Extern,
    #[token("async",    priority = 2)] Async,
    #[token("await",    priority = 2)] Await,
    #[token("spawn",    priority = 2)] Spawn,
    #[token("defer",    priority = 2)] Defer,
    #[token("comptime", priority = 2)] Comptime,
    #[token("inline",   priority = 2)] Inline,
    #[token("unsafe",   priority = 2)] Unsafe,
    #[token("nil",      priority = 2)] Nil,
    #[token("true",     priority = 2)] True,
    #[token("false",    priority = 2)] False,
    #[token("self",     priority = 2)] SelfKw,
    #[token("Self",     priority = 2)] SelfType,
    #[token("init",     priority = 2)] Init,
    #[token("deinit",   priority = 2)] Deinit,
    #[token("throw",    priority = 2)] Throw,
    #[token("try",      priority = 2)] Try,
    #[token("catch",    priority = 2)] Catch,
    #[token("as",       priority = 2)] As,
    #[token("is",       priority = 2)] Is,
    #[token("and",      priority = 2)] And,
    #[token("or",       priority = 2)] Or,
    #[token("not",      priority = 2)] Not,
    #[token("where",    priority = 2)] Where,
    #[token("arena",    priority = 2)] Arena,

    // ── Built-in types ────────────────────────────────────────────────────────
    #[token("i8",    priority = 2)] Ti8,
    #[token("i16",   priority = 2)] Ti16,
    #[token("i32",   priority = 2)] Ti32,
    #[token("i64",   priority = 2)] Ti64,
    #[token("i128",  priority = 2)] Ti128,
    #[token("u8",    priority = 2)] Tu8,
    #[token("u16",   priority = 2)] Tu16,
    #[token("u32",   priority = 2)] Tu32,
    #[token("u64",   priority = 2)] Tu64,
    #[token("u128",  priority = 2)] Tu128,
    #[token("f32",   priority = 2)] Tf32,
    #[token("f64",   priority = 2)] Tf64,
    #[token("bool",  priority = 2)] Tbool,
    #[token("str",   priority = 2)] Tstr,
    #[token("char",  priority = 2)] Tchar,
    #[token("void",  priority = 2)] Tvoid,
    #[token("usize", priority = 2)] Tusize,
    #[token("isize", priority = 2)] Tisize,

    // ── Literals ──────────────────────────────────────────────────────────────
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?", |lex| lex.slice().to_owned())]
    NumberLit(String),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice().to_owned())]
    StringLit(String),

    #[regex(r"'([^'\\]|\\.)'", |lex| lex.slice().to_owned())]
    CharLit(String),

    // ── Identifier — priority 1 (keywords above win at priority 2) ────────────
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_owned(), priority = 1)]
    Ident(String),

    // ── Multi-char operators (higher priority than single-char) ───────────────
    #[token("...", priority = 3)] Ellipsis,
    #[token("..",  priority = 2)] DotDot,
    #[token("::",  priority = 2)] DoubleColon,
    #[token("->",  priority = 2)] Arrow,
    #[token("=>",  priority = 2)] FatArrow,
    #[token("&&",  priority = 2)] AmpAmp,
    #[token("||",  priority = 2)] PipePipe,
    #[token("+=",  priority = 2)] AddAssign,
    #[token("-=",  priority = 2)] SubAssign,
    #[token("*=",  priority = 2)] MulAssign,
    #[token("/=",  priority = 2)] DivAssign,
    #[token("%=",  priority = 2)] ModAssign,
    #[token("==",  priority = 2)] EqEq,
    #[token("!=",  priority = 2)] NotEq,
    #[token("<=",  priority = 2)] LtEq,
    #[token(">=",  priority = 2)] GtEq,

    // ── Single-char ───────────────────────────────────────────────────────────
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
    #[token(":")] Colon,
    #[token(";")] Semicolon,
    #[token("?")] Question,
    #[token("!")] Bang,
    #[token("@")] At,
    #[token("#")] Hash,
    #[token("&")] Amp,
    #[token("|")] Pipe,
    #[token("^")] Caret,
    #[token("~")] Tilde,
    #[token("=")] Assign,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
}

// ─── Span ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default)]
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
    pub fn dummy() -> Self {
        Span::default()
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

pub type SpannedToken = Spanned<Token>;

// ─── Lex error ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: Span,
    pub src: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unrecognized character '{}' at {}:{}", self.src, self.span.line, self.span.col)
    }
}

// ─── Lex ──────────────────────────────────────────────────────────────────────

pub fn lex(source: &str) -> Result<Vec<SpannedToken>, Vec<LexError>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut line = 1usize;
    let mut line_start = 0usize;

    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let range = lexer.span();

        // track newlines
        for ch in source[line_start..range.start].chars() {
            if ch == '\n' {
                line += 1;
                line_start = range.start;
            }
        }

        let col = range.start.saturating_sub(line_start) + 1;
        let span = Span::new(range.start, range.end, line, col);

        match result {
            Ok(tok) => match &tok {
                Token::LineComment(_) | Token::SingleComment(_) => {}
                _ => tokens.push(Spanned::new(tok, span)),
            },
            Err(_) => errors.push(LexError {
                span,
                src: source[range.clone()].to_owned(),
            }),
        }
    }

    if errors.is_empty() { Ok(tokens) } else { Err(errors) }
}
