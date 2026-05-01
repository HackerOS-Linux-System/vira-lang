use crate::lexer::Span;
use thiserror::Error;

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Error, Clone)]
pub enum ParseError {
    #[error("unexpected token: expected {expected}, got {got} at {line}:{col}")]
    Unexpected {
        expected: String,
        got: String,
        line: usize,
        col: usize,
        #[source]
        source: Option<Box<ParseError>>,
    },
    #[error("unexpected end of file at {line}:{col}")]
    Eof { line: usize, col: usize },
    #[error("lex error: {message} at {line}:{col}")]
    LexError { message: String, line: usize, col: usize },
    #[error("multiple errors:\n{}", .0.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ParseError>),
}

impl ParseError {
    pub fn unexpected(expected: String, got: String, span: Span) -> Self {
        ParseError::Unexpected {
            expected,
            got,
            line: span.line,
            col: span.col,
            source: None,
        }
    }

    pub fn eof(span: Span) -> Self {
        ParseError::Eof {
            line: span.line,
            col: span.col,
        }
    }

    pub fn lex(message: String, span: Span) -> Self {
        ParseError::LexError {
            message,
            line: span.line,
            col: span.col,
        }
    }
}
