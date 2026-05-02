use std::fmt::Write;

/// Severity of a diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A single diagnostic message in Vira's Elm-style format.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub title: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
    pub col_end: usize,
    pub message: String,
    pub hint: Option<String>,
    pub source_line: Option<String>,
    /// Optional secondary spans
    pub notes: Vec<DiagNote>,
}

#[derive(Debug, Clone)]
pub struct DiagNote {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl Diagnostic {
    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            title: title.into(),
            file: None,
            line: 0,
            col: 0,
            col_end: 0,
            message: message.into(),
            hint: None,
            source_line: None,
            notes: vec![],
        }
    }

    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            title: title.into(),
            file: None,
            line: 0,
            col: 0,
            col_end: 0,
            message: message.into(),
            hint: None,
            source_line: None,
            notes: vec![],
        }
    }

    pub fn at(mut self, line: usize, col: usize, col_end: usize) -> Self {
        self.line = line;
        self.col = col;
        self.col_end = col_end;
        self
    }

    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_source(mut self, source_line: impl Into<String>) -> Self {
        self.source_line = Some(source_line.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn note(mut self, line: usize, col: usize, msg: impl Into<String>) -> Self {
        self.notes.push(DiagNote { line, col, message: msg.into() });
        self
    }

    /// Render to a human-readable string (Elm style, with ANSI colors).
    pub fn render(&self, source: Option<&str>) -> String {
        let mut out = String::new();
        let use_color = std::env::var("NO_COLOR").is_err();

        let (sev_color, sev_label, bar_color) = match self.severity {
            Severity::Error   => ("\x1b[31m", "ERROR",   "\x1b[31m"),
            Severity::Warning => ("\x1b[33m", "WARNING", "\x1b[33m"),
            Severity::Note    => ("\x1b[36m", "NOTE",    "\x1b[36m"),
        };

        let reset = if use_color { "\x1b[0m" } else { "" };
        let bold  = if use_color { "\x1b[1m" } else { "" };
        let dim   = if use_color { "\x1b[2m" } else { "" };
        let sev_c = if use_color { sev_color } else { "" };
        let bar_c = if use_color { bar_color } else { "" };
        let cyan  = if use_color { "\x1b[36m" } else { "" };
        let yellow = if use_color { "\x1b[33m" } else { "" };

        // ── Header bar ────────────────────────────────────────────────────────
        let file_part = self.file.as_deref().unwrap_or("unknown");
        let title_part = format!(" {sev_c}{bold}{sev_label}{reset} ");
        let file_part_fmt = format!(" {cyan}{file_part}{reset}:{dim}{}{reset}", self.line);
        let bar_width = 60usize;
        let inner = format!("{title_part}");
        let dashes_left  = "─".repeat(2);
        let dashes_right = "─".repeat(bar_width.saturating_sub(inner.len() + file_part_fmt.len() + 4));

        writeln!(out, "\n{bar_c}{dashes_left}{reset}{inner}{bar_c}{dashes_right}{reset}{file_part_fmt}").ok();
        writeln!(out).ok();

        // ── Main message ──────────────────────────────────────────────────────
        for line in self.message.lines() {
            writeln!(out, "    {line}").ok();
        }
        writeln!(out).ok();

        // ── Source snippet ────────────────────────────────────────────────────
        let source_line = self.source_line.clone().or_else(|| {
            source.and_then(|src| {
                src.lines().nth(self.line.saturating_sub(1)).map(|l| l.to_owned())
            })
        });

        if let Some(src_line) = source_line {
            // Context lines (one before and after if available)
            if let Some(src) = source {
                let lines: Vec<&str> = src.lines().collect();
                let li = self.line.saturating_sub(1);
                if li > 0 {
                    let prev = lines.get(li - 1).copied().unwrap_or("");
                    writeln!(out, "    {dim}{:>4}{reset} {dim}│{reset}  {dim}{prev}{reset}", li).ok();
                }
            }

            let line_no = self.line;
            writeln!(out, "    {bold}{:>4}{reset} {bar_c}│{reset}  {src_line}", line_no).ok();

            // Underline caret
            let col = self.col.saturating_sub(1);
            let span_len = if self.col_end > self.col { self.col_end - self.col } else { 1 };
            let spaces = " ".repeat(col + 7); // 4 + " │  " = 7
            let carets = format!("{sev_c}{}{reset}", "^".repeat(span_len));
            writeln!(out, "{spaces}{carets}").ok();

            if let Some(src) = source {
                let lines: Vec<&str> = src.lines().collect();
                let li = self.line.saturating_sub(1);
                if let Some(next) = lines.get(li + 1) {
                    writeln!(out, "    {dim}{:>4}{reset} {dim}│{reset}  {dim}{next}{reset}", line_no + 1).ok();
                }
            }

            writeln!(out).ok();
        }

        // ── Secondary notes ───────────────────────────────────────────────────
        for note in &self.notes {
            writeln!(out, "    {cyan}Note{reset} (line {}:{}): {}", note.line, note.col, note.message).ok();
        }
        if !self.notes.is_empty() { writeln!(out).ok(); }

        // ── Hint ──────────────────────────────────────────────────────────────
        if let Some(hint) = &self.hint {
            writeln!(out, "    {yellow}Hint:{reset}").ok();
            for line in hint.lines() {
                writeln!(out, "        {line}").ok();
            }
            writeln!(out).ok();
        }

        // ── Footer ────────────────────────────────────────────────────────────
        writeln!(out, "    {dim}{}─{reset}", "─".repeat(56)).ok();
        writeln!(out).ok();

        out
    }
}

// ─── Diagnostic collection ────────────────────────────────────────────────────

#[derive(Default)]
pub struct DiagnosticBag {
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self { DiagnosticBag::default() }

    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Warning).count()
    }

    pub fn render_all(&self, source: Option<&str>) -> String {
        let mut out = String::new();
        for d in &self.diagnostics {
            out.push_str(&d.render(source));
        }
        // Summary line
        let ec = self.error_count();
        let wc = self.warning_count();
        if ec > 0 || wc > 0 {
            let use_color = std::env::var("NO_COLOR").is_err();
            let reset = if use_color { "\x1b[0m" } else { "" };
            let red    = if use_color { "\x1b[31m" } else { "" };
            let yellow = if use_color { "\x1b[33m" } else { "" };
            let bold   = if use_color { "\x1b[1m" } else { "" };
            if ec > 0 {
                out.push_str(&format!(
                    "{red}{bold}✗ {} error{}{reset}",
                    ec, if ec == 1 { "" } else { "s" }
                ));
                if wc > 0 { out.push_str(", "); }
            }
            if wc > 0 {
                out.push_str(&format!(
                    "{yellow}{bold}⚠ {} warning{}{reset}",
                    wc, if wc == 1 { "" } else { "s" }
                ));
            }
            out.push('\n');
        }
        out
    }
}

// ─── Convert from ParseError to Diagnostic ───────────────────────────────────

pub fn parse_error_to_diagnostic(e: &vira_parser::ParseError, file: &str) -> Diagnostic {
    use vira_parser::ParseError;
    match e {
        ParseError::Unexpected { expected, got, line, col, .. } => {
            Diagnostic::error(
                "Unexpected token",
                format!(
                    "I was reading your code and found something I did not expect.\n\
I was looking for {expected},\n\
but instead I found {got}."
                ),
            )
            .at(*line, *col, col + got.len())
            .in_file(file)
            .hint(format!(
                "If you are not sure what goes here, try adding {expected} \
at line {line}, column {col}."
            ))
        }
        ParseError::Eof { line, col } => {
            Diagnostic::error(
                "Unexpected end of file",
                "I reached the end of the file before the code was complete.\n\
It looks like something is missing — maybe a closing `}` or `)`?",
            )
            .at(*line, *col, *col + 1)
            .in_file(file)
            .hint("Check that every `{` has a matching `}` and every `(` has a matching `)`.")
        }
        ParseError::LexError { message, line, col } => {
            Diagnostic::error(
                "Unrecognized character",
                format!(
                    "I found a character that is not part of the Vira language.\n\
Details: {message}"
                ),
            )
            .at(*line, *col, *col + 1)
            .in_file(file)
        }
        ParseError::Multiple(errs) => {
            // Return the first one as primary
            if let Some(first) = errs.first() {
                let mut d = parse_error_to_diagnostic(first, file);
                for extra in errs.iter().skip(1).take(3) {
                    if let ParseError::Unexpected { line, col, got, .. } = extra {
                        d = d.note(*line, *col, format!("Also unexpected: {got}"));
                    }
                }
                d
            } else {
                Diagnostic::error("Multiple errors", "Several parse errors were found.").in_file(file)
            }
        }
    }
}
