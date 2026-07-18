package lsp

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"

	"hyperlang.dev/hyperc/internal/checker"
	"hyperlang.dev/hyperc/internal/compile"
	"hyperlang.dev/hyperc/internal/formatter"
	"hyperlang.dev/hyperc/internal/lexer"
)

// ---------------------------------------------------------------------
// Ramka JSON-RPC 2.0 po stdio (Content-Length: N\r\n\r\n<json>)
// ---------------------------------------------------------------------

type rpcMessage struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  interface{}     `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func readMessage(r *bufio.Reader) ([]byte, error) {
	var length int
	for {
		line, err := r.ReadString('\n')
		if err != nil {
			return nil, err
		}
		line = strings.TrimRight(line, "\r\n")
		if line == "" {
			break // koniec nagłówków
		}
		if strings.HasPrefix(line, "Content-Length:") {
			v := strings.TrimSpace(strings.TrimPrefix(line, "Content-Length:"))
			n, err := strconv.Atoi(v)
			if err != nil {
				return nil, fmt.Errorf("lsp: zły nagłówek Content-Length: %w", err)
			}
			length = n
		}
	}
	if length == 0 {
		return nil, fmt.Errorf("lsp: brak/zerowy Content-Length")
	}
	body := make([]byte, length)
	if _, err := io.ReadFull(r, body); err != nil {
		return nil, err
	}
	return body, nil
}

func writeMessage(w io.Writer, v interface{}) error {
	body, err := json.Marshal(v)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(w, "Content-Length: %d\r\n\r\n", len(body)); err != nil {
		return err
	}
	_, err = w.Write(body)
	return err
}

// ---------------------------------------------------------------------
// Stan serwera
// ---------------------------------------------------------------------

type server struct {
	w       io.Writer
	docs    map[string]string          // uri -> treść w pamięci (LSP full document sync)
	results map[string]*compile.Result // uri -> wynik ostatniej kompilacji (dla hover/definition)
}

// Run uruchamia pętlę serwera na podanych strumieniach. Blokuje aż do
// otrzymania `exit` albo zamknięcia wejścia.
func Run(in io.Reader, out io.Writer) error {
	s := &server{w: out, docs: map[string]string{}, results: map[string]*compile.Result{}}
	r := bufio.NewReader(in)

	for {
		raw, err := readMessage(r)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
		var msg rpcMessage
		if err := json.Unmarshal(raw, &msg); err != nil {
			continue // wiadomość nie do sparsowania - ignorujemy, nie zabijamy serwera
		}
		if msg.Method == "exit" {
			return nil
		}
		s.handle(msg)
	}
}

func (s *server) handle(msg rpcMessage) {
	switch msg.Method {
	case "initialize":
		s.reply(msg.ID, map[string]interface{}{
			"capabilities": map[string]interface{}{
				"textDocumentSync":           1, // Full
				"documentFormattingProvider": true,
				"hoverProvider":              true,
				"definitionProvider":         true,
			},
			"serverInfo": map[string]interface{}{
				"name":    "hyperc-lsp",
				"version": "0.1.0",
			},
		})
	case "initialized", "$/setTrace":
		// notyfikacje bez odpowiedzi

	case "textDocument/didOpen":
		var p struct {
			TextDocument struct {
				URI  string `json:"uri"`
				Text string `json:"text"`
			} `json:"textDocument"`
		}
		if err := json.Unmarshal(msg.Params, &p); err == nil {
			s.docs[p.TextDocument.URI] = p.TextDocument.Text
			s.publishDiagnostics(p.TextDocument.URI)
		}

	case "textDocument/didChange":
		var p struct {
			TextDocument struct {
				URI string `json:"uri"`
			} `json:"textDocument"`
			ContentChanges []struct {
				Text string `json:"text"`
			} `json:"contentChanges"`
		}
		if err := json.Unmarshal(msg.Params, &p); err == nil && len(p.ContentChanges) > 0 {
			// Full sync (capability 1) - ostatni element to cały nowy dokument.
			s.docs[p.TextDocument.URI] = p.ContentChanges[len(p.ContentChanges)-1].Text
			s.publishDiagnostics(p.TextDocument.URI)
		}

	case "textDocument/didClose":
		var p struct {
			TextDocument struct {
				URI string `json:"uri"`
			} `json:"textDocument"`
		}
		if err := json.Unmarshal(msg.Params, &p); err == nil {
			delete(s.docs, p.TextDocument.URI)
		}

	case "textDocument/formatting":
		var p struct {
			TextDocument struct {
				URI string `json:"uri"`
			} `json:"textDocument"`
		}
		if err := json.Unmarshal(msg.Params, &p); err != nil {
			s.reply(msg.ID, []interface{}{})
			return
		}
		src, ok := s.docs[p.TextDocument.URI]
		if !ok {
			s.reply(msg.ID, []interface{}{})
			return
		}
		formatted, err := formatter.Format(src)
		if err != nil {
			s.reply(msg.ID, []interface{}{})
			return
		}
		if formatted == src {
			s.reply(msg.ID, []interface{}{})
			return
		}
		lastLine, lastCol := endPosition(src)
		s.reply(msg.ID, []interface{}{
			map[string]interface{}{
				"range": map[string]interface{}{
					"start": map[string]int{"line": 0, "character": 0},
					"end":   map[string]int{"line": lastLine, "character": lastCol},
				},
				"newText": formatted,
			},
		})

	case "textDocument/hover":
		s.handleHover(msg)

	case "textDocument/definition":
		s.handleDefinition(msg)

	case "shutdown":
		s.reply(msg.ID, nil)

	default:
		if msg.ID != nil {
			s.reply(msg.ID, nil) // nieznana metoda żądająca odpowiedzi - odpowiadamy pusto, nie zrywamy połączenia
		}
	}
}

func (s *server) reply(id json.RawMessage, result interface{}) {
	_ = writeMessage(s.w, rpcMessage{JSONRPC: "2.0", ID: id, Result: result})
}

func (s *server) notify(method string, params interface{}) {
	body, _ := json.Marshal(params)
	_ = writeMessage(s.w, struct {
		JSONRPC string          `json:"jsonrpc"`
		Method  string          `json:"method"`
		Params  json.RawMessage `json:"params"`
	}{"2.0", method, body})
}

// publishDiagnostics kompiluje dokument (zapisując go tymczasowo na dysk,
// bo bundler operuje na ścieżkach plików - potrzebne do rozwiązywania
// lokalnych importów tak samo jak przy `hyperc build`) i wysyła
// textDocument/publishDiagnostics do klienta.
func (s *server) publishDiagnostics(uri string) {
	src := s.docs[uri]
	path := uriToPath(uri)

	diags := []map[string]interface{}{}

	// Podmieniamy zawartość na dysku tylko jeśli różni się od bieżącego
	// stanu pliku (edytor jeszcze nie zapisał) - piszemy do pliku
	// tymczasowego obok oryginału, żeby lokalne importy nadal się rozwiązały,
	// ale nie nadpisywać niezapisanej pracy użytkownika na stałe.
	if path != "" {
		if err := os.WriteFile(path+".hyperlsp.tmp", []byte(src), 0644); err == nil {
			defer os.Remove(path + ".hyperlsp.tmp")
			res := compile.File(path + ".hyperlsp.tmp")
			s.results[uri] = res
			for _, d := range res.Diagnostics {
				sev := 1 // Error
				if d.Severity == checker.SevWarning {
					sev = 2 // Warning
				}
				line := d.Line - 1
				if line < 0 {
					line = 0
				}
				col := d.Col - 1
				if col < 0 {
					col = 0
				}
				diags = append(diags, map[string]interface{}{
					"range": map[string]interface{}{
						"start": map[string]int{"line": line, "character": col},
						"end":   map[string]int{"line": line, "character": col + 1},
					},
					"severity": sev,
					"source":   "hyperc",
					"message":  d.Message,
				})
			}
			if res.ParseError != "" {
				diags = append(diags, map[string]interface{}{
					"range": map[string]interface{}{
						"start": map[string]int{"line": 0, "character": 0},
						"end":   map[string]int{"line": 0, "character": 1},
					},
					"severity": 1,
					"source":   "hyperc",
					"message":  res.ParseError,
				})
			}
		}
	}

	s.notify("textDocument/publishDiagnostics", map[string]interface{}{
		"uri":         uri,
		"diagnostics": diags,
	})
}

func uriToPath(uri string) string {
	return strings.TrimPrefix(uri, "file://")
}

func endPosition(src string) (line, col int) {
	lines := strings.Split(src, "\n")
	line = len(lines) - 1
	col = len([]rune(lines[len(lines)-1]))
	return
}

// ---------------------------------------------------------------------
// Hover + go-to-definition
// ---------------------------------------------------------------------

type positionParams struct {
	TextDocument struct {
		URI string `json:"uri"`
	} `json:"textDocument"`
	Position struct {
		Line      int `json:"line"`
		Character int `json:"character"`
	} `json:"position"`
}

// identifierAtPosition retokenizuje dokument (prawdziwym lekserem, więc
// poprawnie pomija stringi/komentarze/template literale) i zwraca literał
// identyfikatora pokrywającego podaną pozycję (0-indeksowaną, jak w LSP),
// albo "" gdy pod kursorem nie ma identyfikatora.
func identifierAtPosition(src string, line, character int) string {
	targetLine := line + 1
	targetCol := character + 1
	l := lexer.New(src)
	for {
		tok := l.NextToken()
		if tok.Type == lexer.EOF {
			return ""
		}
		if tok.Type != lexer.IDENT || tok.Line != targetLine {
			continue
		}
		startCol := tok.Col
		endCol := tok.Col + len([]rune(tok.Literal))
		if targetCol >= startCol && targetCol <= endCol {
			return tok.Literal
		}
	}
}

// resolveSymbol znajduje symbol pod kursorem dla danego URI, tłumacząc
// Pos.File z powrotem na prawdziwą ścieżkę, jeśli wskazuje na plik
// tymczasowy użyty wewnętrznie przez publishDiagnostics (ten plik jest
// usuwany zaraz po kompilacji, więc nigdy nie powinien wyciekać do
// odpowiedzi LSP).
func (s *server) resolveSymbol(uri string, line, character int) (checker.SymbolInfo, bool) {
	src, ok := s.docs[uri]
	if !ok {
		return checker.SymbolInfo{}, false
	}
	ident := identifierAtPosition(src, line, character)
	if ident == "" {
		return checker.SymbolInfo{}, false
	}
	res := s.results[uri]
	if res == nil || res.Symbols == nil {
		return checker.SymbolInfo{}, false
	}
	sym, ok := res.Symbols[ident]
	if !ok {
		return checker.SymbolInfo{}, false
	}
	realPath := uriToPath(uri)
	if sym.Pos.File == realPath+".hyperlsp.tmp" {
		sym.Pos.File = realPath
	}
	return sym, true
}

func (s *server) handleHover(msg rpcMessage) {
	var p positionParams
	if err := json.Unmarshal(msg.Params, &p); err != nil {
		s.reply(msg.ID, nil)
		return
	}
	sym, ok := s.resolveSymbol(p.TextDocument.URI, p.Position.Line, p.Position.Character)
	if !ok {
		s.reply(msg.ID, nil)
		return
	}
	s.reply(msg.ID, map[string]interface{}{
		"contents": map[string]interface{}{
			"kind":  "markdown",
			"value": fmt.Sprintf("```hyper\n%s\n```", sym.Signature),
		},
	})
}

func (s *server) handleDefinition(msg rpcMessage) {
	var p positionParams
	if err := json.Unmarshal(msg.Params, &p); err != nil {
		s.reply(msg.ID, nil)
		return
	}
	sym, ok := s.resolveSymbol(p.TextDocument.URI, p.Position.Line, p.Position.Character)
	if !ok || sym.Pos.File == "" {
		s.reply(msg.ID, nil)
		return
	}
	line := sym.Pos.Line - 1
	if line < 0 {
		line = 0
	}
	col := sym.Pos.Col - 1
	if col < 0 {
		col = 0
	}
	s.reply(msg.ID, map[string]interface{}{
		"uri": "file://" + sym.Pos.File,
		"range": map[string]interface{}{
			"start": map[string]int{"line": line, "character": col},
			"end":   map[string]int{"line": line, "character": col + 1},
		},
	})
}
