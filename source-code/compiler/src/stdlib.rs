#[derive(Debug, Clone)]
pub struct MethodResolution {
    /// How to emit the call in Rust
    pub rust_emit: EmitStrategy,
    /// Whether this method mutates the receiver (needs &mut)
    pub mutates: bool,
    /// Return type hint for type inference
    pub return_hint: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub enum EmitStrategy {
    /// Direct passthrough: receiver.method(args)
    Direct,
    /// Renamed: receiver.rust_name(args)
    Rename(&'static str),
    /// Wrapped: wrapper(receiver, args)
    Wrapper(&'static str),
    /// Custom template with {recv} and {args} placeholders
    Template(&'static str),
}

/// Resolve a method call on a known type.
/// Returns None if we don't know about it (fall through to Rust).
pub fn resolve_method(method: &str) -> MethodResolution {
    match method {
        // ── String methods ────────────────────────────────────────────────────
        "contains"    => mr(EmitStrategy::Direct,              false, Some("bool")),
        "starts_with" => mr(EmitStrategy::Direct,              false, Some("bool")),
        "ends_with"   => mr(EmitStrategy::Direct,              false, Some("bool")),
        "len"         => mr(EmitStrategy::Direct,              false, Some("usize")),
        "is_empty"    => mr(EmitStrategy::Direct,              false, Some("bool")),
        "to_string"   => mr(EmitStrategy::Direct,              false, Some("String")),
        "trim"        => mr(EmitStrategy::Direct,              false, Some("&str")),
        "trim_start"  => mr(EmitStrategy::Direct,              false, Some("&str")),
        "trim_end"    => mr(EmitStrategy::Direct,              false, Some("&str")),
        "to_uppercase"=> mr(EmitStrategy::Direct,              false, Some("String")),
        "to_lowercase"=> mr(EmitStrategy::Direct,              false, Some("String")),
        "replace"     => mr(EmitStrategy::Direct,              false, Some("String")),
        "split"       => mr(EmitStrategy::Template("{recv}.split({args}).collect::<Vec<_>>()"), false, Some("Vec<&str>")),
        "join"        => mr(EmitStrategy::Template("{recv}.join({args})"),                     false, Some("String")),
        "chars"       => mr(EmitStrategy::Direct,              false, Some("Chars")),
        "bytes"       => mr(EmitStrategy::Direct,              false, Some("Bytes")),
        "parse_i64"   => mr(EmitStrategy::Template("{recv}.parse::<i64>().unwrap_or(0)"),      false, Some("i64")),
        "parse_f64"   => mr(EmitStrategy::Template("{recv}.parse::<f64>().unwrap_or(0.0)"),    false, Some("f64")),
        "parse_i32"   => mr(EmitStrategy::Template("{recv}.parse::<i32>().unwrap_or(0)"),      false, Some("i32")),
        "char_at"     => mr(EmitStrategy::Template("{recv}.chars().nth({args})"),              false, Some("Option<char>")),
        "repeat"      => mr(EmitStrategy::Direct,              false, Some("String")),
        "lines"       => mr(EmitStrategy::Template("{recv}.lines().collect::<Vec<_>>()"),      false, Some("Vec<&str>")),
        "as_str"      => mr(EmitStrategy::Direct,              false, Some("&str")),

        // ── Vec / slice methods ───────────────────────────────────────────────
        "push"        => mr(EmitStrategy::Direct,              true,  None),
        "pop"         => mr(EmitStrategy::Direct,              true,  Some("Option<T>")),
        "retain"      => mr(EmitStrategy::Direct,              true,  None),
        "filter"      => mr(EmitStrategy::Template("{recv}.iter().filter({args}).cloned().collect::<Vec<_>>()"), false, Some("Vec<T>")),
        "map"         => mr(EmitStrategy::Template("{recv}.iter().map({args}).collect::<Vec<_>>()"),            false, Some("Vec<T>")),
        "find"        => mr(EmitStrategy::Template("{recv}.iter().find({args}).cloned()"),                     false, Some("Option<T>")),
        "any"         => mr(EmitStrategy::Template("{recv}.iter().any({args})"),                               false, Some("bool")),
        "all"         => mr(EmitStrategy::Template("{recv}.iter().all({args})"),                               false, Some("bool")),
        "sort"        => mr(EmitStrategy::Direct,              true,  None),
        "sort_by"     => mr(EmitStrategy::Direct,              true,  None),
        "reverse"     => mr(EmitStrategy::Direct,              true,  None),
        "extend"      => mr(EmitStrategy::Direct,              true,  None),
        "clear"       => mr(EmitStrategy::Direct,              true,  None),
        "insert"      => mr(EmitStrategy::Direct,              true,  None),
        "remove"      => mr(EmitStrategy::Direct,              true,  Some("T")),
        "get"         => mr(EmitStrategy::Direct,              false, Some("Option<&T>")),
        "first"       => mr(EmitStrategy::Direct,              false, Some("Option<&T>")),
        "last"        => mr(EmitStrategy::Direct,              false, Some("Option<&T>")),
        "iter"        => mr(EmitStrategy::Direct,              false, Some("Iter")),
        "iter_mut"    => mr(EmitStrategy::Direct,              true,  Some("IterMut")),
        "into_iter"   => mr(EmitStrategy::Direct,              false, Some("IntoIter")),
        "sum"         => mr(EmitStrategy::Template("{recv}.iter().sum::<_>()"),                                false, Some("T")),
        "count"       => mr(EmitStrategy::Direct,              false, Some("usize")),
        "collect"     => mr(EmitStrategy::Direct,              false, Some("Vec<T>")),
        "enumerate"   => mr(EmitStrategy::Direct,              false, None),
        "zip"         => mr(EmitStrategy::Direct,              false, None),
        "flat_map"    => mr(EmitStrategy::Template("{recv}.iter().flat_map({args}).collect::<Vec<_>>()"),      false, None),
        "flatten"     => mr(EmitStrategy::Template("{recv}.iter().flatten().collect::<Vec<_>>()"),             false, None),

        // ── HashMap methods ───────────────────────────────────────────────────
        "entry"       => mr(EmitStrategy::Direct,              true,  None),
        "or_insert"   => mr(EmitStrategy::Direct,              true,  None),
        "keys"        => mr(EmitStrategy::Template("{recv}.keys().cloned().collect::<Vec<_>>()"),              false, None),
        "values"      => mr(EmitStrategy::Template("{recv}.values().cloned().collect::<Vec<_>>()"),            false, None),

        // ── Option methods ────────────────────────────────────────────────────
        "unwrap"      => mr(EmitStrategy::Direct,              false, Some("T")),
        "unwrap_or"   => mr(EmitStrategy::Direct,              false, Some("T")),
        "unwrap_or_else"=> mr(EmitStrategy::Direct,            false, Some("T")),
        "expect"      => mr(EmitStrategy::Direct,              false, Some("T")),
        "is_some"     => mr(EmitStrategy::Direct,              false, Some("bool")),
        "is_none"     => mr(EmitStrategy::Direct,              false, Some("bool")),
        "map_or"      => mr(EmitStrategy::Direct,              false, Some("T")),
        "and_then"    => mr(EmitStrategy::Direct,              false, Some("Option<T>")),
        "or_else"     => mr(EmitStrategy::Direct,              false, Some("Option<T>")),
        "ok_or"       => mr(EmitStrategy::Direct,              false, Some("Result<T,E>")),

        // ── Result methods ────────────────────────────────────────────────────
        "ok"          => mr(EmitStrategy::Direct,              false, Some("Option<T>")),
        "err"         => mr(EmitStrategy::Direct,              false, Some("Option<E>")),
        "is_ok"       => mr(EmitStrategy::Direct,              false, Some("bool")),
        "is_err"      => mr(EmitStrategy::Direct,              false, Some("bool")),
        "map_err"     => mr(EmitStrategy::Direct,              false, None),
        "context"     => mr(EmitStrategy::Template("{recv}.with_context(|| {args})"), false, None),

        // ── Numeric methods ───────────────────────────────────────────────────
        "abs"         => mr(EmitStrategy::Direct,              false, None),
        "min"         => mr(EmitStrategy::Direct,              false, None),
        "max"         => mr(EmitStrategy::Direct,              false, None),
        "clamp"       => mr(EmitStrategy::Direct,              false, None),
        "pow"         => mr(EmitStrategy::Direct,              false, None),
        "sqrt"        => mr(EmitStrategy::Direct,              false, None),
        "floor"       => mr(EmitStrategy::Direct,              false, None),
        "ceil"        => mr(EmitStrategy::Direct,              false, None),
        "round"       => mr(EmitStrategy::Direct,              false, None),

        // ── Display / formatting ──────────────────────────────────────────────
        "display"     => mr(EmitStrategy::Template("format!(\"{{}}\", {recv})"),      false, Some("String")),
        "debug"       => mr(EmitStrategy::Template("format!(\"{{:?}}\", {recv})"),    false, Some("String")),

        // Unknown: pass through to Rust
        _             => mr(EmitStrategy::Direct, false, None),
    }
}

fn mr(emit: EmitStrategy, mutates: bool, ret: Option<&'static str>) -> MethodResolution {
    MethodResolution { rust_emit: emit, mutates, return_hint: ret }
}

/// Apply a method resolution to generate Rust code.
pub fn emit_method_call(
    recv: &str,
    method: &str,
    args: &str,
) -> String {
    let res = resolve_method(method);
    match res.rust_emit {
        EmitStrategy::Direct => {
            if args.is_empty() {
                format!("{recv}.{method}()")
            } else {
                format!("{recv}.{method}({args})")
            }
        }
        EmitStrategy::Rename(rust_name) => {
            if args.is_empty() {
                format!("{recv}.{rust_name}()")
            } else {
                format!("{recv}.{rust_name}({args})")
            }
        }
        EmitStrategy::Wrapper(wrap) => {
            if args.is_empty() {
                format!("{wrap}({recv})")
            } else {
                format!("{wrap}({recv}, {args})")
            }
        }
        EmitStrategy::Template(tmpl) => {
            tmpl.replace("{recv}", recv).replace("{args}", args)
        }
    }
}

// ─── Vira stdlib preamble ──────────────────────────────────────────────────────
// Injected at the top of every generated Rust file.

pub const STDLIB_PREAMBLE: &str = r#"
// ── Vira stdlib ──────────────────────────────────────────────────────────────
#[allow(unused_macros)]
macro_rules! vira_println {
    ($($arg:tt)*) => { println!($($arg)*) }
}
#[allow(unused_macros)]
macro_rules! vira_eprintln {
    ($($arg:tt)*) => { eprintln!($($arg)*) }
}

/// Vira string concat operator: a + b where b is any Display type
#[allow(dead_code)]
#[inline(always)]
fn __vira_concat<A: std::fmt::Display, B: std::fmt::Display>(a: A, b: B) -> String {
    std::format!("{}{}", a, b)
}

/// ViraError — base error type for throw/catch
#[derive(Debug)]
pub struct ViraError {
    pub message: String,
    pub kind: ViraErrorKind,
}

#[derive(Debug)]
pub enum ViraErrorKind {
    Runtime,
    NotFound,
    InvalidInput,
    IoError,
    Custom(String),
}

impl std::fmt::Display for ViraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ViraError {}

impl ViraError {
    pub fn new(msg: impl Into<String>) -> Self {
        ViraError { message: msg.into(), kind: ViraErrorKind::Runtime }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        ViraError { message: msg.into(), kind: ViraErrorKind::NotFound }
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        ViraError { message: msg.into(), kind: ViraErrorKind::InvalidInput }
    }
}

impl From<std::io::Error> for ViraError {
    fn from(e: std::io::Error) -> Self { ViraError { message: e.to_string(), kind: ViraErrorKind::IoError } }
}
impl From<String> for ViraError {
    fn from(s: String) -> Self { ViraError::new(s) }
}
impl From<&str> for ViraError {
    fn from(s: &str) -> Self { ViraError::new(s) }
}
impl From<Box<dyn std::error::Error>> for ViraError {
    fn from(e: Box<dyn std::error::Error>) -> Self { ViraError::new(e.to_string()) }
}

/// Vira Result type alias
pub type ViraResult<T> = Result<T, ViraError>;

// Note: tauri::Error conversion is handled by the app-level build

/// Helper for Vira string operations — ensures &str is used correctly
#[allow(dead_code)]
#[inline]
fn __vira_str<T: AsRef<str>>(s: T) -> String { s.as_ref().to_owned() }

/// Vira println — accepts both String and &str
#[allow(unused_macros)]
macro_rules! println {
    ($s:expr) => { ::std::println!("{}", $s) };
    ($fmt:expr, $($arg:tt)*) => { ::std::println!($fmt, $($arg)*) };
}
// ── end Vira stdlib ───────────────────────────────────────────────────────────
"#;
