package compile

import (
	"fmt"
	"os"
	"strings"

	"hyperlang.dev/hyperc/internal/bundler"
	"hyperlang.dev/hyperc/internal/checker"
	"hyperlang.dev/hyperc/internal/codegen"
)

type Result struct {
	JS          string
	Files       []string // wszystkie pliki .hyper wzięte pod uwagę (entry + lokalne importy)
	Diagnostics []checker.Diagnostic
	ParseError  string // niepuste, jeśli parsowanie (lexer/parser/bundler) się nie powiodło
	Symbols     map[string]checker.SymbolInfo
}

// HasErrors mówi, czy wynik zawiera cokolwiek blokującego wygenerowanie JS.
func (r *Result) HasErrors() bool {
	if r.ParseError != "" {
		return true
	}
	for _, d := range r.Diagnostics {
		if d.Severity == checker.SevError {
			return true
		}
	}
	return false
}

// File kompiluje plik wejściowy (i cały graf jego lokalnych importów) do JS.
// W przeciwieństwie do zwykłego `error`, błędy parsowania/typów trafiają do
// Result, żeby LSP mogło je pokazać jako diagnostyki zamiast twardego crasha.
func File(path string) *Result {
	res := &Result{}

	prog, files, err := bundler.Bundle(path)
	if err != nil {
		res.ParseError = err.Error()
		return res
	}
	res.Files = files

	res.Diagnostics = checker.Check(prog)
	res.Symbols = checker.Index(prog)
	if res.HasErrors() {
		return res
	}

	gen := codegen.New()
	res.JS = gen.Generate(prog)
	return res
}

// FormatDiagnostics renderuje diagnostyki jako tekst z numerem linii i
// fragmentem kodu - do użytku przez CLI. Każda diagnostyka niesie własne
// Pos.File (patrz ast.StampFile + bundler), więc dla projektów
// wieloplikowych fragment kodu pochodzi z RZECZYWISTEGO pliku, w którym
// wystąpił błąd - nie tylko z pliku wejściowego.
func (r *Result) FormatDiagnostics(entryPath string, entrySource string) string {
	var sb strings.Builder
	if r.ParseError != "" {
		sb.WriteString(r.ParseError)
		sb.WriteString("\n")
	}

	sourceCache := map[string][]string{entryPath: strings.Split(entrySource, "\n")}
	linesFor := func(file string) []string {
		if file == "" {
			file = entryPath
		}
		if cached, ok := sourceCache[file]; ok {
			return cached
		}
		data, err := os.ReadFile(file)
		if err != nil {
			sourceCache[file] = nil
			return nil
		}
		lines := strings.Split(string(data), "\n")
		sourceCache[file] = lines
		return lines
	}

	for _, d := range r.Diagnostics {
		file := d.File
		if file == "" {
			file = entryPath
		}
		sb.WriteString(fmt.Sprintf("%s:%s\n", file, d.String()))
		lines := linesFor(file)
		if d.Line > 0 && d.Line <= len(lines) {
			snippet := lines[d.Line-1]
			sb.WriteString(fmt.Sprintf("  %d | %s\n", d.Line, snippet))
			pad := strings.Repeat(" ", len(fmt.Sprintf("%d | ", d.Line))+max0(d.Col-1))
			sb.WriteString(fmt.Sprintf("  %s^\n", pad))
		}
	}
	return sb.String()
}

func max0(a int) int {
	if a > 0 {
		return a
	}
	return 0
}
