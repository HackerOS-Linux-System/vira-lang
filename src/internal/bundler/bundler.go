package bundler

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"hyperlang.dev/hyperc/internal/ast"
	"hyperlang.dev/hyperc/internal/lexer"
	"hyperlang.dev/hyperc/internal/parser"
)

type moduleResult struct {
	path string
	prog *ast.Program
}

// Bundle zwraca scalony program gotowy do checkera/codegenu, oraz listę
// wszystkich plików źródłowych wziętych pod uwagę (przydatne np. dla LSP
// do inwalidacji cache'u czy dla `vira` do śledzenia zależności builda).
func Bundle(entryPath string) (*ast.Program, []string, error) {
	absEntry, err := filepath.Abs(entryPath)
	if err != nil {
		return nil, nil, err
	}

	visited := map[string]bool{}
	visiting := map[string]bool{}
	var order []moduleResult
	var files []string
	declaredBy := map[string]string{}
	declaredNames := map[string]map[string]bool{} // ścieżka -> zbiór nazw top-level w tym pliku

	var visit func(path string) error
	visit = func(path string) error {
		if visited[path] {
			return nil
		}
		if visiting[path] {
			return fmt.Errorf("cykl importów wykryty przy %s", path)
		}
		visiting[path] = true

		src, err := os.ReadFile(path)
		if err != nil {
			return fmt.Errorf("nie można odczytać %s: %w", path, err)
		}
		l := lexer.New(string(src))
		p := parser.New(l)
		prog := p.ParseProgram()
		if errs := p.Errors(); len(errs) > 0 {
			return fmt.Errorf("błędy parsowania w %s:\n  %s", path, strings.Join(errs, "\n  "))
		}
		ast.StampFile(prog, path)
		files = append(files, path)

		names := map[string]bool{}
		for _, n := range topLevelNames(prog) {
			names[n] = true
		}
		declaredNames[path] = names

		dir := filepath.Dir(path)
		for _, stmt := range prog.Statements {
			imp, ok := stmt.(*ast.ImportStmt)
			if !ok || !isLocalImport(imp.Path) {
				continue
			}
			depPath := resolveLocalPath(dir, imp.Path)
			if _, err := os.Stat(depPath); err != nil {
				return fmt.Errorf("%s: nie znaleziono lokalnego modułu %q (szukano: %s)", path, imp.Path, depPath)
			}
			if err := visit(depPath); err != nil {
				return err
			}
			// Teraz, gdy depPath jest w pełni odwiedzone, znamy jego deklaracje -
			// sprawdźmy, że każda zaimportowana nazwa RZECZYWIŚCIE tam istnieje,
			// zamiast dowiadywać się o literówce dopiero z ReferenceError w node.
			for _, n := range imp.Named {
				if !declaredNames[depPath][n.Name] {
					return fmt.Errorf("%s: moduł %q nie eksportuje `%s` (sprawdź literówkę albo czy `%s` w ogóle tam istnieje)",
						path, imp.Path, n.Name, n.Name)
				}
			}
		}

		for _, name := range topLevelNames(prog) {
			if other, exists := declaredBy[name]; exists && other != path {
				return fmt.Errorf("konflikt nazw: `%s` zdefiniowane zarówno w %s, jak i w %s - zmień jedną z nazw", name, other, path)
			}
			declaredBy[name] = path
		}

		visiting[path] = false
		visited[path] = true
		order = append(order, moduleResult{path: path, prog: prog})
		return nil
	}

	if err := visit(absEntry); err != nil {
		return nil, nil, err
	}

	merged := &ast.Program{}

	// Importy zewnętrzne scalamy PO ŚCIEŻCE (nie po całej sygnaturze) - różne
	// pliki mogą importować różne podzbiory nazw z tego samego modułu JS
	// (np. manifest.hyper importuje { readFileSync, writeFileSync } z "fs",
	// a lockfile.hyper tylko { writeFileSync } z "fs"). Bez scalania każdy
	// plik dokładałby OSOBNY `import ... from "fs"`, a powtórzona nazwa w
	// dwóch niezależnych importach to błąd składni w JS (SyntaxError:
	// Identifier has already been declared).
	type mergedImport struct {
		defaultName string
		named       map[string]string // nazwa -> alias ("" = brak aliasu)
		namedOrder  []string
		firstSeenAt int // pozycja w merged.Statements, gdzie wstawić
	}
	externalByPath := map[string]*mergedImport{}
	var externalOrder []string

	for _, m := range order {
		for _, stmt := range m.prog.Statements {
			imp, isImport := stmt.(*ast.ImportStmt)
			if !isImport {
				merged.Statements = append(merged.Statements, stmt)
				continue
			}
			if isLocalImport(imp.Path) {
				continue // scalone do wspólnej przestrzeni nazw - import znika
			}
			mi, ok := externalByPath[imp.Path]
			if !ok {
				mi = &mergedImport{named: map[string]string{}}
				externalByPath[imp.Path] = mi
				externalOrder = append(externalOrder, imp.Path)
			}
			if imp.Default != "" {
				if mi.defaultName != "" && mi.defaultName != imp.Default {
					return nil, nil, fmt.Errorf(
						"%s: domyślny import z %q jako `%s` koliduje z wcześniejszym `%s` w innym pliku - użyj tej samej nazwy albo `as`",
						m.path, imp.Path, imp.Default, mi.defaultName)
				}
				mi.defaultName = imp.Default
			}
			for _, n := range imp.Named {
				if existingAlias, seen := mi.named[n.Name]; seen && existingAlias != n.Alias {
					return nil, nil, fmt.Errorf(
						"%s: import `%s` z %q z aliasem %q koliduje z wcześniejszym aliasem %q w innym pliku",
						m.path, n.Name, imp.Path, n.Alias, existingAlias)
				}
				if _, seen := mi.named[n.Name]; !seen {
					mi.namedOrder = append(mi.namedOrder, n.Name)
				}
				mi.named[n.Name] = n.Alias
			}
		}
	}

	var importStmts []ast.Stmt
	for _, path := range externalOrder {
		mi := externalByPath[path]
		imp := &ast.ImportStmt{Default: mi.defaultName, Path: path}
		for _, name := range mi.namedOrder {
			imp.Named = append(imp.Named, ast.ImportedName{Name: name, Alias: mi.named[name]})
		}
		importStmts = append(importStmts, imp)
	}
	merged.Statements = append(importStmts, merged.Statements...)

	return merged, files, nil
}

func isLocalImport(p string) bool {
	return strings.HasPrefix(p, "./") || strings.HasPrefix(p, "../")
}

func resolveLocalPath(dir, importPath string) string {
	p := filepath.Join(dir, importPath)
	if !strings.HasSuffix(p, ".hyper") {
		p += ".hyper"
	}
	return p
}

func importSignature(imp *ast.ImportStmt) string {
	parts := []string{"d:" + imp.Default}
	for _, n := range imp.Named {
		parts = append(parts, n.Name+">"+n.Alias)
	}
	return strings.Join(parts, ",")
}

// topLevelNames zwraca nazwy deklaracji, które "eksportuje" moduł - dziś
// wszystkie deklaracje top-level (fn/struct/enum/trait), niezależnie od
// `pub`. Egzekwowanie widoczności na podstawie `pub` to kolejny krok
// (patrz README, sekcja "co dalej") - dziś `pub` jest parsowane, ale jeszcze
// nieegzekwowane na granicy modułów.
func topLevelNames(prog *ast.Program) []string {
	var names []string
	for _, s := range prog.Statements {
		switch d := s.(type) {
		case *ast.FnDecl:
			names = append(names, d.Name)
		case *ast.StructDecl:
			names = append(names, d.Name)
		case *ast.EnumDecl:
			names = append(names, d.Name)
		case *ast.TraitDecl:
			names = append(names, d.Name)
		case *ast.LetStmt:
			names = append(names, d.Name)
		case *ast.TypeAliasStmt:
			names = append(names, d.Name)
		}
	}
	return names
}
