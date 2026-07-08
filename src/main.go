package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"hyperlang.dev/hyperc/internal/codegen"
	"hyperlang.dev/hyperc/internal/lexer"
	"hyperlang.dev/hyperc/internal/parser"
)

func main() {
	if len(os.Args) < 3 {
		usage()
		os.Exit(1)
	}

	cmd := os.Args[1]
	srcPath := os.Args[2]

	switch cmd {
	case "build":
		out := ""
		for i := 3; i < len(os.Args); i++ {
			if os.Args[i] == "-o" && i+1 < len(os.Args) {
				out = os.Args[i+1]
			}
		}
		js, err := compile(srcPath)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		if out == "" {
			fmt.Print(js)
			return
		}
		if err := os.WriteFile(out, []byte(js), 0644); err != nil {
			fmt.Fprintln(os.Stderr, "hyperc: nie można zapisać pliku wyjściowego:", err)
			os.Exit(1)
		}
		fmt.Println("hyperc: zapisano", out)

	case "run":
		js, err := compile(srcPath)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		tmp, err := os.CreateTemp("", "hyperc-*.mjs")
		if err != nil {
			fmt.Fprintln(os.Stderr, "hyperc: nie można utworzyć pliku tymczasowego:", err)
			os.Exit(1)
		}
		defer os.Remove(tmp.Name())
		if _, err := tmp.WriteString(js); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		tmp.Close()

		node := exec.Command("node", tmp.Name())
		node.Stdout = os.Stdout
		node.Stderr = os.Stderr
		node.Stdin = os.Stdin
		if err := node.Run(); err != nil {
			os.Exit(1)
		}

	default:
		usage()
		os.Exit(1)
	}
}

func compile(path string) (string, error) {
	if filepath.Ext(path) != ".hyper" {
		return "", fmt.Errorf("hyperc: oczekiwano pliku .hyper, otrzymano %q", path)
	}
	src, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("hyperc: nie można odczytać %q: %w", path, err)
	}

	l := lexer.New(string(src))
	p := parser.New(l)
	prog := p.ParseProgram()

	if errs := p.Errors(); len(errs) > 0 {
		return "", fmt.Errorf("hyperc: błędy parsowania w %s:\n  %s", path, strings.Join(errs, "\n  "))
	}

	gen := codegen.New()
	return gen.Generate(prog), nil
}

func usage() {
	fmt.Fprintln(os.Stderr, `hyperc - kompilator Hyper Lang

Użycie:
  hyperc build <plik.hyper> [-o wyjście.js]
  hyperc run <plik.hyper>`)
}
