package lexer

type TokenType int

const (
	EOF TokenType = iota
	ILLEGAL

	IDENT
	INT
	FLOAT
	STRING   // "..." lub '...'
	TEMPLATE // `...` (przekazywany 1:1 do JS - identyczna składnia template literal)

	// słowa kluczowe
	LET
	CONST
	MUT
	FN
	STRUCT
	IMPL
	TRAIT
	ENUM
	MATCH
	IF
	ELSE
	WHILE
	FOR
	IN
	RETURN
	IMPORT
	FROM
	AS
	ASYNC
	AWAIT
	TRUE
	FALSE
	NULLKW
	NEW
	SELF
	PUB
	TYPE
	BREAK
	CONTINUE
	THROW
	TRY
	CATCH
	FINALLY
	DECLARE
	MODULE

	// symbole
	LPAREN
	RPAREN
	LBRACE
	RBRACE
	LBRACKET
	RBRACKET
	COMMA
	DOT
	QDOT // ?.
	COLON
	SEMI
	ARROW    // ->
	FATARROW // =>
	ASSIGN
	EQ
	NEQ
	LT
	GT
	LE
	GE
	PLUS
	MINUS
	STAR
	SLASH
	PERCENT
	BANG
	ANDAND // &&
	OROR   // ||
	QUESTION
	NULLISH // ??
	PIPE    // |  (unie typów: A | B)
	PLUSEQ
	MINUSEQ
	STAREQ
	SLASHEQ
	SPREAD // ...
)

var keywords = map[string]TokenType{
	"let": LET, "const": CONST, "mut": MUT, "fn": FN, "struct": STRUCT,
	"impl": IMPL, "trait": TRAIT, "enum": ENUM, "match": MATCH, "if": IF,
	"else": ELSE, "while": WHILE, "for": FOR, "in": IN, "return": RETURN,
	"import": IMPORT, "from": FROM, "as": AS, "async": ASYNC, "await": AWAIT,
	"true": TRUE, "false": FALSE, "null": NULLKW, "new": NEW, "self": SELF,
	"pub": PUB, "type": TYPE, "break": BREAK, "continue": CONTINUE, "throw": THROW,
	"try": TRY, "catch": CATCH, "finally": FINALLY,
	"declare": DECLARE, "module": MODULE,
}

func LookupIdent(ident string) TokenType {
	if tok, ok := keywords[ident]; ok {
		return tok
	}
	return IDENT
}

type Token struct {
	Type    TokenType
	Literal string
	Line    int
	Col     int
}

var names = map[TokenType]string{
	EOF: "EOF", ILLEGAL: "ILLEGAL", IDENT: "IDENT", INT: "INT", FLOAT: "FLOAT",
	STRING: "STRING", TEMPLATE: "TEMPLATE", LPAREN: "(", RPAREN: ")",
	LBRACE: "{", RBRACE: "}", LBRACKET: "[", RBRACKET: "]", COMMA: ",",
	DOT: ".", QDOT: "?.", COLON: ":", SEMI: ";", ARROW: "->", FATARROW: "=>",
	ASSIGN: "=", EQ: "==", NEQ: "!=", LT: "<", GT: ">", LE: "<=", GE: ">=",
	PLUS: "+", MINUS: "-", STAR: "*", SLASH: "/", PERCENT: "%", BANG: "!",
	ANDAND: "&&", OROR: "||", QUESTION: "?", NULLISH: "??", PIPE: "|",
}

func (t TokenType) String() string {
	if n, ok := names[t]; ok {
		return n
	}
	return "KEYWORD"
}
