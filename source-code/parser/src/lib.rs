pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::Program;
pub use error::{ParseError, ParseResult};
pub use lexer::lex;
pub use parser::Parser;

/// Convenience: lex + parse a source string into an AST Program.
pub fn parse(source: &str) -> ParseResult<Program> {
    let tokens = lex(source).map_err(|errs| {
        let pe: Vec<ParseError> = errs
            .into_iter()
            .map(|e| ParseError::lex(e.src, e.span))
            .collect();
        if pe.len() == 1 {
            pe.into_iter().next().unwrap()
        } else {
            ParseError::Multiple(pe)
        }
    })?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}
