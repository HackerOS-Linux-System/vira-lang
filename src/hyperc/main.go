package main

import (
	"encoding/json"
	"fmt"
	"os"

	"hyperlang.dev/hyperc/internal/checker"
	"hyperlang.dev/hyperc/internal/compile"
)

type jsonDiagnostic struct {
	Severity string `json:"severity"`
	Message  string `json:"message"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
}

type jsonOutput struct {
	OK          bool             `json:"ok"`
	JS          string           `json:"js,omitempty"`
	ParseError  string           `json:"parseError,omitempty"`
	Diagnostics []jsonDiagnostic `json:"diagnostics"`
	Files       []string         `json:"files"`
}

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(1)
	}

	var srcPath, outPath string
	checkOnly := false
	jsonOut := false

	for i := 1; i < len(os.Args); i++ {
		switch a := os.Args[i]; a {
		case "-o":
			if i+1 >= len(os.Args) {
				fmt.Fprintln(os.Stderr, "hyperc: -o wymaga argumentu")
				os.Exit(1)
			}
			outPath = os.Args[i+1]
			i++
		case "--check":
			checkOnly = true
		case "--json":
			jsonOut = true
		case "-h", "--help":
			usage()
			return
		default:
			if srcPath != "" {
				fmt.Fprintln(os.Stderr, "hyperc: nieoczekiwany argument", a)
				os.Exit(1)
			}
			srcPath = a
		}
	}

	if srcPath == "" {
		usage()
		os.Exit(1)
	}

	res := compile.File(srcPath)

	if jsonOut {
		out := jsonOutput{OK: !res.HasErrors(), ParseError: res.ParseError, Files: res.Files}
		if !checkOnly && !res.HasErrors() {
			out.JS = res.JS
		}
		for _, d := range res.Diagnostics {
			sev := "error"
			if d.Severity == checker.SevWarning {
				sev = "warning"
			}
			out.Diagnostics = append(out.Diagnostics, jsonDiagnostic{Severity: sev, Message: d.Message, Line: d.Line, Col: d.Col})
		}
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		_ = enc.Encode(out)
		if res.HasErrors() {
			os.Exit(1)
		}
		return
	}

	if res.HasErrors() {
		src, _ := os.ReadFile(srcPath)
		fmt.Fprint(os.Stderr, res.FormatDiagnostics(srcPath, string(src)))
		fmt.Fprintf(os.Stderr, "hyperc: kompilacja przerwana - błędy w %s\n", srcPath)
		os.Exit(1)
	}

	if checkOnly {
		fmt.Println(srcPath + ": OK")
		return
	}

	if outPath == "" {
		fmt.Print(res.JS)
		return
	}
	if err := os.WriteFile(outPath, []byte(res.JS), 0644); err != nil {
		fmt.Fprintln(os.Stderr, "hyperc: nie można zapisać pliku wyjściowego:", err)
		os.Exit(1)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, `hyperc - kompilator Hyper Lang

Użycie:
  hyperc plik.hyper [-o wyjście.js] [--check] [--json]

hyperc robi wyłącznie kompilację. Uruchamianie, formatowanie, LSP,
zarządzanie zależnościami i budowanie binarek - patrz 'vira'.`)
}
