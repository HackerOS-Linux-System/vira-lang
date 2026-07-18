package formatter

import (
	"fmt"
	"strings"

	"hyperlang.dev/hyperc/internal/lexer"
)

const indentUnit = "  "

type lineState struct {
	depth         int
	firstIsCloser bool
	protected     bool // wewnątrz wieloliniowego template/stringu - nie ruszamy
}

// Format zwraca sformatowaną wersję źródła.
func Format(src string) (string, error) {
	lines := strings.Split(src, "\n")
	states := make([]lineState, len(lines)+2) // +2 zapas na krawędziach

	l := lexer.New(src)
	depth := 0
	lastLine := 0

	for {
		tok := l.NextToken()
		if tok.Type == lexer.EOF {
			break
		}
		if tok.Line != lastLine {
			isCloser := tok.Type == lexer.RBRACE || tok.Type == lexer.RPAREN || tok.Type == lexer.RBRACKET
			// wypełnij ewentualną lukę (puste linie / linie tylko-komentarzowe)
			// tą samą głębokością - one i tak nie zmieniają depth.
			for ln := lastLine + 1; ln <= tok.Line && ln < len(states); ln++ {
				states[ln] = lineState{depth: depth, firstIsCloser: ln == tok.Line && isCloser}
			}
			lastLine = tok.Line
		}

		endLine := tok.Line + strings.Count(tok.Literal, "\n")
		if endLine > tok.Line && (tok.Type == lexer.TEMPLATE || tok.Type == lexer.STRING) {
			for ln := tok.Line + 1; ln <= endLine && ln < len(states); ln++ {
				states[ln] = lineState{protected: true}
			}
			lastLine = endLine
		}

		switch tok.Type {
		case lexer.LBRACE, lexer.LPAREN, lexer.LBRACKET:
			depth++
		case lexer.RBRACE, lexer.RPAREN, lexer.RBRACKET:
			depth--
			if depth < 0 {
				depth = 0
			}
		}
	}

	for _, c := range l.Comments {
		for ln := c.StartLine; ln <= c.EndLine && ln < len(states); ln++ {
			if ln == c.StartLine && !states[ln].protected {
				// pierwszy wiersz komentarza dostaje bieżącą głębokość jak zwykły token
				continue
			}
			states[ln].protected = true
		}
	}

	var out strings.Builder
	for i, raw := range lines {
		lineNo := i + 1
		st := states[lineNo]

		if st.protected {
			out.WriteString(raw)
		} else {
			content := strings.TrimSpace(raw)
			if content == "" {
				// zostaw pustą linię pustą
			} else {
				indent := st.depth
				if st.firstIsCloser {
					indent--
				}
				if indent < 0 {
					indent = 0
				}
				out.WriteString(strings.Repeat(indentUnit, indent))
				out.WriteString(content)
			}
		}
		if i < len(lines)-1 {
			out.WriteString("\n")
		}
	}

	return out.String(), nil
}

// FormatFile to wygodny wrapper zwracający też informację czy coś się zmieniło.
func FormatFile(src string) (formatted string, changed bool, err error) {
	out, err := Format(src)
	if err != nil {
		return "", false, fmt.Errorf("formatter: %w", err)
	}
	return out, out != src, nil
}
