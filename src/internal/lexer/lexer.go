package lexer

// CommentRange to zakres linii zajmowany przez jeden komentarz (// albo /* */).
// Lekser go nie zwraca jako tokenu (parser go nie potrzebuje), ale
// formatter potrzebuje wiedzieć, gdzie są komentarze, żeby ich nie
// przeformatować/zgubić.
type CommentRange struct {
	StartLine int
	EndLine   int
}

type Lexer struct {
	input   []rune
	pos     int
	readPos int
	ch      rune
	line    int
	col     int

	Comments []CommentRange
}

func New(input string) *Lexer {
	l := &Lexer{input: []rune(input), line: 1, col: 0}
	l.readChar()
	return l
}

func (l *Lexer) readChar() {
	if l.readPos >= len(l.input) {
		l.ch = 0
	} else {
		l.ch = l.input[l.readPos]
	}
	l.pos = l.readPos
	l.readPos++
	if l.ch == '\n' {
		l.line++
		l.col = 0
	} else {
		l.col++
	}
}

func (l *Lexer) peekChar() rune {
	if l.readPos >= len(l.input) {
		return 0
	}
	return l.input[l.readPos]
}

func (l *Lexer) skipWhitespaceAndComments() {
	for {
		for l.ch == ' ' || l.ch == '\t' || l.ch == '\n' || l.ch == '\r' {
			l.readChar()
		}
		if l.ch == '/' && l.peekChar() == '/' {
			startLine := l.line
			for l.ch != '\n' && l.ch != 0 {
				l.readChar()
			}
			l.Comments = append(l.Comments, CommentRange{StartLine: startLine, EndLine: startLine})
			continue
		}
		if l.ch == '/' && l.peekChar() == '*' {
			startLine := l.line
			l.readChar()
			l.readChar()
			for !(l.ch == '*' && l.peekChar() == '/') && l.ch != 0 {
				l.readChar()
			}
			l.readChar()
			l.readChar()
			l.Comments = append(l.Comments, CommentRange{StartLine: startLine, EndLine: l.line})
			continue
		}
		break
	}
}

func isLetter(ch rune) bool {
	return ch == '_' || (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z')
}
func isDigit(ch rune) bool { return ch >= '0' && ch <= '9' }

func (l *Lexer) NextToken() Token {
	l.skipWhitespaceAndComments()
	line, col := l.line, l.col
	var tok Token

	switch l.ch {
	case '(':
		tok = Token{LPAREN, "(", line, col}
	case ')':
		tok = Token{RPAREN, ")", line, col}
	case '{':
		tok = Token{LBRACE, "{", line, col}
	case '}':
		tok = Token{RBRACE, "}", line, col}
	case '[':
		tok = Token{LBRACKET, "[", line, col}
	case ']':
		tok = Token{RBRACKET, "]", line, col}
	case ',':
		tok = Token{COMMA, ",", line, col}
	case ':':
		tok = Token{COLON, ":", line, col}
	case ';':
		tok = Token{SEMI, ";", line, col}
	case '.':
		if l.peekChar() == '.' {
			l.readChar()
			if l.peekChar() == '.' {
				l.readChar()
				tok = Token{SPREAD, "...", line, col}
			} else {
				tok = Token{ILLEGAL, "..", line, col}
			}
		} else {
			tok = Token{DOT, ".", line, col}
		}
	case '?':
		if l.peekChar() == '.' {
			l.readChar()
			tok = Token{QDOT, "?.", line, col}
		} else if l.peekChar() == '?' {
			l.readChar()
			tok = Token{NULLISH, "??", line, col}
		} else {
			tok = Token{QUESTION, "?", line, col}
		}
	case '-':
		if l.peekChar() == '>' {
			l.readChar()
			tok = Token{ARROW, "->", line, col}
		} else if l.peekChar() == '=' {
			l.readChar()
			tok = Token{MINUSEQ, "-=", line, col}
		} else {
			tok = Token{MINUS, "-", line, col}
		}
	case '=':
		if l.peekChar() == '>' {
			l.readChar()
			tok = Token{FATARROW, "=>", line, col}
		} else if l.peekChar() == '=' {
			l.readChar()
			tok = Token{EQ, "==", line, col}
		} else {
			tok = Token{ASSIGN, "=", line, col}
		}
	case '!':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{NEQ, "!=", line, col}
		} else {
			tok = Token{BANG, "!", line, col}
		}
	case '<':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{LE, "<=", line, col}
		} else {
			tok = Token{LT, "<", line, col}
		}
	case '>':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{GE, ">=", line, col}
		} else {
			tok = Token{GT, ">", line, col}
		}
	case '+':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{PLUSEQ, "+=", line, col}
		} else {
			tok = Token{PLUS, "+", line, col}
		}
	case '*':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{STAREQ, "*=", line, col}
		} else {
			tok = Token{STAR, "*", line, col}
		}
	case '/':
		if l.peekChar() == '=' {
			l.readChar()
			tok = Token{SLASHEQ, "/=", line, col}
		} else {
			tok = Token{SLASH, "/", line, col}
		}
	case '%':
		tok = Token{PERCENT, "%", line, col}
	case '&':
		if l.peekChar() == '&' {
			l.readChar()
			tok = Token{ANDAND, "&&", line, col}
		} else {
			tok = Token{ILLEGAL, "&", line, col}
		}
	case '|':
		if l.peekChar() == '|' {
			l.readChar()
			tok = Token{OROR, "||", line, col}
		} else {
			tok = Token{PIPE, "|", line, col}
		}
	case '"', '\'':
		tok = Token{STRING, l.readString(l.ch), line, col}
	case '`':
		tok = Token{TEMPLATE, l.readTemplate(), line, col}
	case 0:
		tok = Token{EOF, "", line, col}
	default:
		if isLetter(l.ch) {
			ident := l.readIdentifier()
			return Token{LookupIdent(ident), ident, line, col}
		} else if isDigit(l.ch) {
			return l.readNumber(line, col)
		}
		tok = Token{ILLEGAL, string(l.ch), line, col}
	}

	l.readChar()
	return tok
}

func (l *Lexer) readIdentifier() string {
	start := l.pos
	for isLetter(l.ch) || isDigit(l.ch) {
		l.readChar()
	}
	return string(l.input[start:l.pos])
}

func (l *Lexer) readNumber(line, col int) Token {
	start := l.pos
	isFloat := false
	for isDigit(l.ch) {
		l.readChar()
	}
	if l.ch == '.' && isDigit(l.peekChar()) {
		isFloat = true
		l.readChar()
		for isDigit(l.ch) {
			l.readChar()
		}
	}
	lit := string(l.input[start:l.pos])
	if isFloat {
		return Token{FLOAT, lit, line, col}
	}
	return Token{INT, lit, line, col}
}

// readString zwraca zawartość BEZ otaczających cudzysłowów.
func (l *Lexer) readString(quote rune) string {
	l.readChar() // pomiń otwierający cudzysłów
	var sb []rune
	for l.ch != quote && l.ch != 0 {
		if l.ch == '\\' {
			l.readChar()
			sb = append(sb, decodeEscape(l.ch))
			l.readChar()
			continue
		}
		sb = append(sb, l.ch)
		l.readChar()
	}
	return string(sb)
}

// decodeEscape tłumaczy znak PO backslashu na jego rzeczywistą wartość.
// Bez tego np. "linia1\nlinia2" trafiałoby do wygenerowanego JS jako
// dosłowny backslash+n zamiast znaku nowej linii.
func decodeEscape(ch rune) rune {
	switch ch {
	case 'n':
		return '\n'
	case 't':
		return '\t'
	case 'r':
		return '\r'
	case '0':
		return 0
	case '\\':
		return '\\'
	case '"':
		return '"'
	case '\'':
		return '\''
	case '`':
		return '`'
	default:
		return ch // nieznana sekwencja - przepuszczamy znak dosłownie
	}
}

// readTemplate zwraca CAŁY literał template WRAZ z otaczającymi backtickami,
// ponieważ składnia template literal w Hyper == składnia JS (przechodzi 1:1).
func (l *Lexer) readTemplate() string {
	start := l.pos
	depth := 0
	l.readChar() // pomiń otwierający backtick
	for {
		if l.ch == 0 {
			break
		}
		if l.ch == '\\' {
			l.readChar()
			l.readChar()
			continue
		}
		if l.ch == '$' && l.peekChar() == '{' {
			depth++
			l.readChar()
			l.readChar()
			continue
		}
		if depth > 0 && l.ch == '}' {
			depth--
			l.readChar()
			continue
		}
		if depth == 0 && l.ch == '`' {
			break
		}
		l.readChar()
	}
	return string(l.input[start : l.pos+1])
}
