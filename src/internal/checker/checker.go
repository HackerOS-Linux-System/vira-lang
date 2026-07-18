package checker

import (
	"fmt"
	"strings"

	"hyperlang.dev/hyperc/internal/ast"
)

type Severity int

const (
	SevError Severity = iota
	SevWarning
)

type Diagnostic struct {
	Severity Severity
	Message  string
	Line     int
	Col      int
	File     string
}

func (d Diagnostic) String() string {
	kind := "error"
	if d.Severity == SevWarning {
		kind = "warning"
	}
	if d.Line == 0 {
		return fmt.Sprintf("%s: %s", kind, d.Message)
	}
	return fmt.Sprintf("%s:%d:%d: %s", kind, d.Line, d.Col, d.Message)
}

// ---------------------------------------------------------------------
// Rejestry (zebrane w pierwszym przebiegu)
// ---------------------------------------------------------------------

type structInfo struct {
	name           string
	fields         map[string]string // pole -> typ (tekstowy)
	order          []string
	requiredFields map[string]bool // pola bez wartości domyślnej
	pos            ast.Pos
	pub            bool
}

type enumInfo struct {
	name    string
	variant map[string][]ast.Param // wariant -> pola pozycyjne
	order   []string
	pos     ast.Pos
	pub     bool
}

type traitInfo struct {
	name    string
	methods map[string]bool
	pos     ast.Pos
}

type funcSig struct {
	name       string
	params     []ast.Param
	returnType string
	pos        ast.Pos
	pub        bool
}

type implInfo struct {
	target  string
	methods map[string]*funcSig
}

// ---------------------------------------------------------------------
// Zmienne / zakresy (drugi przebieg)
// ---------------------------------------------------------------------

type varInfo struct {
	typ     string
	mutable bool
	isConst bool
}

type scope struct {
	vars   map[string]varInfo
	parent *scope
}

func newScope(parent *scope) *scope { return &scope{vars: map[string]varInfo{}, parent: parent} }

func (s *scope) define(name string, v varInfo) { s.vars[name] = v }

func (s *scope) lookup(name string) (varInfo, bool) {
	for cur := s; cur != nil; cur = cur.parent {
		if v, ok := cur.vars[name]; ok {
			return v, true
		}
	}
	return varInfo{}, false
}

// ---------------------------------------------------------------------
// Checker
// ---------------------------------------------------------------------

type Checker struct {
	diags []Diagnostic

	structs            map[string]*structInfo
	enums              map[string]*enumInfo
	traits             map[string]*traitInfo
	funcs              map[string]*funcSig
	implsByTarget      map[string]*implInfo
	implTraitsByTarget map[string][]string
	imported           map[string]bool // nazwy wprowadzone przez `import`
}

func New() *Checker {
	return &Checker{
		structs:            map[string]*structInfo{},
		enums:              map[string]*enumInfo{},
		traits:             map[string]*traitInfo{},
		funcs:              map[string]*funcSig{},
		implsByTarget:      map[string]*implInfo{},
		implTraitsByTarget: map[string][]string{},
		imported:           map[string]bool{},
	}
}

// zbiór wbudowanych globali runtime'u JS/Node - identyfikatory, które checker
// zawsze uznaje za zdefiniowane, bo pochodzą spoza świata Hyper.
var jsGlobals = map[string]bool{
	"console": true, "Math": true, "JSON": true, "Object": true, "Array": true,
	"Promise": true, "Error": true, "TypeError": true, "RangeError": true,
	"Map": true, "Set": true, "WeakMap": true, "WeakSet": true, "Symbol": true,
	"Date": true, "RegExp": true, "globalThis": true, "process": true,
	"require": true, "module": true, "exports": true, "Number": true,
	"String": true, "Boolean": true, "NaN": true, "Infinity": true,
	"fetch": true, "setTimeout": true, "setInterval": true, "clearTimeout": true,
	"clearInterval": true, "parseInt": true, "parseFloat": true, "isNaN": true,
	"isFinite": true, "structuredClone": true, "BigInt": true, "Reflect": true,
	"Proxy": true, "__dirname": true, "__filename": true, "self": true,
	"window": true, "document": true, "navigator": true, "queueMicrotask": true,
	"Buffer": true, "encodeURIComponent": true, "decodeURIComponent": true,
	"encodeURI": true, "decodeURI": true, "URL": true, "URLSearchParams": true,
	"TextEncoder": true, "TextDecoder": true, "AbortController": true,
	"performance": true, "crypto": true,
}

func (c *Checker) errorAt(pos ast.Pos, format string, args ...interface{}) {
	c.diags = append(c.diags, Diagnostic{Severity: SevError, Message: fmt.Sprintf(format, args...), Line: pos.Line, Col: pos.Col, File: pos.File})
}

func (c *Checker) warnAt(pos ast.Pos, format string, args ...interface{}) {
	c.diags = append(c.diags, Diagnostic{Severity: SevWarning, Message: fmt.Sprintf(format, args...), Line: pos.Line, Col: pos.Col, File: pos.File})
}

// checkVisibility egzekwuje `pub` NA GRANICY MODUŁÓW. Bundler spłaszcza
// wszystkie pliki w jedną przestrzeń nazw, ale każdy węzeł niesie Pos.File
// wskazujące, z którego pliku pochodzi (patrz ast.StampFile) - więc
// "granica modułu" to po prostu różnica plików. W obrębie tego samego
// pliku deklaracje są zawsze widoczne, niezależnie od `pub` (prywatność
// nie ma sensu względem samej siebie).
func (c *Checker) checkVisibility(usePos ast.Pos, declPos ast.Pos, pub bool, kind, name string) {
	if pub {
		return
	}
	if usePos.File == "" || declPos.File == "" || usePos.File == declPos.File {
		return
	}
	c.errorAt(usePos, "%s `%s` (zdefiniowane w %s) jest prywatne - dodaj `pub` przed deklaracją, żeby udostępnić je innym plikom", kind, name, declPos.File)
}

// Check uruchamia pełną analizę i zwraca listę diagnostyk (błędów i ostrzeżeń).
func Check(prog *ast.Program) []Diagnostic {
	c := New()
	c.register(prog.Statements)
	c.checkTraitConformance()
	root := newScope(nil)
	c.checkStmts(prog.Statements, root, "")
	return c.diags
}

// SymbolInfo to jeden wpis w indeksie symboli top-level, używany przez LSP
// do hover i go-to-definition. Celowo płaski i tekstowy (Signature jest
// już wyrenderowanym stringiem) - LSP nie musi znać wewnętrznych typów
// checkera.
type SymbolInfo struct {
	Kind      string // "fn" | "struct" | "enum" | "trait"
	Signature string
	Pos       ast.Pos
}

// Index buduje mapę nazwa -> SymbolInfo dla wszystkich deklaracji top-level
// (funkcje, struktury, enumy, traity), bez uruchamiania sprawdzania ciał -
// to tylko przebieg rejestracji, tani i bezpieczny do wołania na każde
// zdarzenie edytora (didChange).
func Index(prog *ast.Program) map[string]SymbolInfo {
	c := New()
	c.register(prog.Statements)

	idx := map[string]SymbolInfo{}
	for name, fs := range c.funcs {
		idx[name] = SymbolInfo{Kind: "fn", Signature: formatFuncSignature(name, fs), Pos: fs.pos}
	}
	for name, s := range c.structs {
		idx[name] = SymbolInfo{Kind: "struct", Signature: formatStructSignature(name, s), Pos: s.pos}
	}
	for name, e := range c.enums {
		idx[name] = SymbolInfo{Kind: "enum", Signature: formatEnumSignature(name, e), Pos: e.pos}
	}
	for name, t := range c.traits {
		idx[name] = SymbolInfo{Kind: "trait", Signature: fmt.Sprintf("trait %s { ... }", name), Pos: t.pos}
	}
	return idx
}

func formatFuncSignature(name string, fs *funcSig) string {
	parts := make([]string, 0, len(fs.params))
	for _, p := range fs.params {
		if p.Name == "self" {
			continue
		}
		if p.Type != "" {
			parts = append(parts, fmt.Sprintf("%s: %s", p.Name, p.Type))
		} else {
			parts = append(parts, p.Name)
		}
	}
	ret := ""
	if fs.returnType != "" {
		ret = " -> " + fs.returnType
	}
	pub := ""
	if fs.pub {
		pub = "pub "
	}
	return fmt.Sprintf("%sfn %s(%s)%s", pub, name, strings.Join(parts, ", "), ret)
}

func formatStructSignature(name string, s *structInfo) string {
	parts := make([]string, 0, len(s.order))
	for _, f := range s.order {
		parts = append(parts, fmt.Sprintf("%s: %s", f, s.fields[f]))
	}
	pub := ""
	if s.pub {
		pub = "pub "
	}
	return fmt.Sprintf("%sstruct %s { %s }", pub, name, strings.Join(parts, ", "))
}

func formatEnumSignature(name string, e *enumInfo) string {
	pub := ""
	if e.pub {
		pub = "pub "
	}
	return fmt.Sprintf("%senum %s { %s }", pub, name, strings.Join(e.order, ", "))
}

// ---------------------------------------------------------------------
// Przebieg 1: rejestracja deklaracji (żeby odwołania w przód działały)
// ---------------------------------------------------------------------

func (c *Checker) register(stmts []ast.Stmt) {
	for _, s := range stmts {
		switch d := s.(type) {
		case *ast.StructDecl:
			info := &structInfo{name: d.Name, fields: map[string]string{}, requiredFields: map[string]bool{}, pos: d.Pos, pub: d.Pub}
			for _, f := range d.Fields {
				info.fields[f.Name] = f.Type
				info.order = append(info.order, f.Name)
				if f.Default == nil {
					info.requiredFields[f.Name] = true
				}
			}
			c.structs[d.Name] = info
		case *ast.EnumDecl:
			info := &enumInfo{name: d.Name, variant: map[string][]ast.Param{}, pos: d.Pos, pub: d.Pub}
			for _, v := range d.Variants {
				info.variant[v.Name] = v.Fields
				info.order = append(info.order, v.Name)
			}
			c.enums[d.Name] = info
		case *ast.TraitDecl:
			info := &traitInfo{name: d.Name, methods: map[string]bool{}, pos: d.Pos}
			for _, m := range d.Methods {
				info.methods[m.Name] = true
			}
			c.traits[d.Name] = info
		case *ast.FnDecl:
			c.funcs[d.Name] = &funcSig{name: d.Name, params: d.Params, returnType: d.ReturnType, pos: d.Pos, pub: d.Pub}
		case *ast.ImplDecl:
			impl, ok := c.implsByTarget[d.Target]
			if !ok {
				impl = &implInfo{target: d.Target, methods: map[string]*funcSig{}}
				c.implsByTarget[d.Target] = impl
			}
			for _, m := range d.Methods {
				impl.methods[m.Name] = &funcSig{name: m.Name, params: m.Params, returnType: m.ReturnType, pos: m.Pos}
			}
			if d.Trait != "" {
				c.implTraitsByTarget[d.Target] = append(c.implTraitsByTarget[d.Target], d.Trait)
			}
		case *ast.ImportStmt:
			if d.Default != "" {
				c.imported[d.Default] = true
			}
			for _, n := range d.Named {
				if n.Alias != "" {
					c.imported[n.Alias] = true
				} else {
					c.imported[n.Name] = true
				}
			}
		case *ast.BlockStmt:
			c.register(d.Statements)
		}
	}
}

// checkTraitConformance: `impl Trait for X` musi zaimplementować KAŻDĄ metodę
// zadeklarowaną w `trait Trait`. To jest realny mechanizm, nie tylko erasure -
// błąd tutaj zatrzymuje kompilację, tak jak w Rust czy Swift.
func (c *Checker) checkTraitConformance() {
	for target, traitNames := range c.implTraitsByTarget {
		impl := c.implsByTarget[target]
		for _, tname := range traitNames {
			trait, ok := c.traits[tname]
			if !ok {
				c.errorAt(ast.Pos{}, "nieznany trait %q użyty w `impl %s for %s`", tname, tname, target)
				continue
			}
			var missing []string
			for method := range trait.methods {
				if impl == nil || impl.methods[method] == nil {
					missing = append(missing, method)
				}
			}
			if len(missing) > 0 {
				c.errorAt(trait.pos, "typ `%s` nie implementuje wymaganych metod traitu `%s`: %s",
					target, tname, strings.Join(missing, ", "))
			}
		}
	}
}

// ---------------------------------------------------------------------
// Przebieg 2: sprawdzanie ciał (zmienne, wywołania, typy literałów)
// ---------------------------------------------------------------------

// selfType to nazwa typu, na który wskazuje `self` w obecnie sprawdzanej
// metodzie (pusta poza ciałem `impl`).
func (c *Checker) checkStmts(stmts []ast.Stmt, sc *scope, selfType string) {
	for _, s := range stmts {
		c.checkStmt(s, sc, selfType)
	}
}

func (c *Checker) checkStmt(s ast.Stmt, sc *scope, selfType string) {
	switch n := s.(type) {
	case *ast.LetStmt:
		var inferred string
		if n.Value != nil {
			inferred = c.checkExpr(n.Value, sc, selfType)
		}
		declType := normalizeType(n.Type)
		if declType != "" && inferred != "" {
			if !typesCompatible(declType, inferred) {
				c.errorAt(n.Pos, "niezgodność typów: zmienna `%s` zadeklarowana jako `%s`, ale przypisana wartość ma typ `%s`",
					n.Name, n.Type, inferred)
			}
		}
		useType := declType
		if useType == "" {
			useType = inferred
		}
		sc.define(n.Name, varInfo{typ: useType, mutable: n.Mutable, isConst: n.Const || !n.Mutable})

	case *ast.DestructureLetStmt:
		c.checkExpr(n.Value, sc, selfType)
		for _, t := range n.Targets {
			sc.define(t.Name, varInfo{typ: "", mutable: n.Mutable, isConst: n.Const || !n.Mutable})
		}

	case *ast.FnDecl:
		c.checkFn(n, sc, "")

	case *ast.StructDecl, *ast.EnumDecl, *ast.TraitDecl, *ast.TypeAliasStmt, *ast.DeclareModuleStmt, *ast.ImportStmt:
		// już zarejestrowane / erasure - nic do sprawdzenia w ciele

	case *ast.ImplDecl:
		for _, m := range n.Methods {
			c.checkFn(m, sc, n.Target)
		}

	case *ast.ExprStmt:
		c.checkExpr(n.X, sc, selfType)

	case *ast.ReturnStmt:
		if n.Value != nil {
			c.checkExpr(n.Value, sc, selfType)
		}

	case *ast.IfStmt:
		c.checkExpr(n.Cond, sc, selfType)
		c.checkStmts(n.Then.Statements, newScope(sc), selfType)
		switch e := n.Else.(type) {
		case *ast.BlockStmt:
			c.checkStmts(e.Statements, newScope(sc), selfType)
		case *ast.IfStmt:
			c.checkStmt(e, sc, selfType)
		}

	case *ast.WhileStmt:
		c.checkExpr(n.Cond, sc, selfType)
		c.checkStmts(n.Body.Statements, newScope(sc), selfType)

	case *ast.ForInStmt:
		c.checkExpr(n.Iterable, sc, selfType)
		inner := newScope(sc)
		inner.define(n.VarName, varInfo{typ: "", mutable: true})
		c.checkStmts(n.Body.Statements, inner, selfType)

	case *ast.BlockStmt:
		c.checkStmts(n.Statements, newScope(sc), selfType)

	case *ast.ThrowStmt:
		c.checkExpr(n.Value, sc, selfType)

	case *ast.TryStmt:
		c.checkStmts(n.Try.Statements, newScope(sc), selfType)
		if n.Catch != nil {
			catchScope := newScope(sc)
			if n.CatchParam != "" {
				catchScope.define(n.CatchParam, varInfo{typ: "", mutable: true})
			}
			c.checkStmts(n.Catch.Statements, catchScope, selfType)
		}
		if n.Finally != nil {
			c.checkStmts(n.Finally.Statements, newScope(sc), selfType)
		}

	case *ast.BreakStmt, *ast.ContinueStmt:
		// brak

	default:
		// nieznana instrukcja - nic nie robimy (permisywnie)
	}
}

func (c *Checker) checkFn(fn *ast.FnDecl, outer *scope, selfType string) {
	inner := newScope(outer)
	if selfType != "" {
		inner.define("self", varInfo{typ: selfType, mutable: true})
	}
	for _, p := range fn.Params {
		if p.Name == "self" {
			continue
		}
		inner.define(p.Name, varInfo{typ: normalizeType(p.Type), mutable: true})
		if p.Default != nil {
			c.checkExpr(p.Default, outer, selfType)
		}
	}
	if fn.Body != nil {
		c.checkStmts(fn.Body.Statements, inner, selfType)
	} else if fn.ExprBody != nil {
		c.checkExpr(fn.ExprBody, inner, selfType)
	}
}

// checkExpr zwraca (najlepszy dostępny) inferowany typ wyrażenia jako string,
// albo "" gdy nie da się/nie warto go ustalić (permisywny fallback).
func (c *Checker) checkExpr(e ast.Expr, sc *scope, selfType string) string {
	switch n := e.(type) {
	case *ast.Ident:
		if _, ok := sc.lookup(n.Name); ok {
			return c.identType(n.Name, sc)
		}
		if fs, ok := c.funcs[n.Name]; ok {
			c.checkVisibility(n.Pos, fs.pos, fs.pub, "funkcja", n.Name)
			return ""
		}
		if c.imported[n.Name] || jsGlobals[n.Name] {
			return ""
		}
		if info, ok := c.structs[n.Name]; ok {
			c.checkVisibility(n.Pos, info.pos, info.pub, "struktura", n.Name)
			return ""
		}
		if info, ok := c.enums[n.Name]; ok {
			c.checkVisibility(n.Pos, info.pos, info.pub, "enum", n.Name)
			return ""
		}
		c.errorAt(n.Pos, "niezdefiniowany identyfikator `%s`", n.Name)
		return ""

	case *ast.IntLit:
		return "i32"
	case *ast.FloatLit:
		return "f64"
	case *ast.StringLit, *ast.TemplateLit:
		return "string"
	case *ast.BoolLit:
		return "bool"
	case *ast.NullLit:
		return "null"

	case *ast.ArrayLit:
		for _, el := range n.Elements {
			c.checkExpr(el, sc, selfType)
		}
		return ""

	case *ast.ObjectLit:
		for _, k := range n.Order {
			c.checkExpr(n.Values[k], sc, selfType)
		}
		return ""

	case *ast.SpreadExpr:
		c.checkExpr(n.Value, sc, selfType)
		return ""

	case *ast.StructLit:
		c.checkStructLit(n, sc, selfType)
		return n.Name

	case *ast.BinaryExpr:
		lt := c.checkExpr(n.Left, sc, selfType)
		rt := c.checkExpr(n.Right, sc, selfType)
		c.checkBinaryTypes(n, lt, rt)
		if isComparisonOp(n.Op) {
			return "bool"
		}
		return lt

	case *ast.UnaryExpr:
		return c.checkExpr(n.Operand, sc, selfType)

	case *ast.NullishExpr:
		c.checkExpr(n.Left, sc, selfType)
		return c.checkExpr(n.Right, sc, selfType)

	case *ast.TernaryExpr:
		c.checkExpr(n.Cond, sc, selfType)
		thenType := c.checkExpr(n.Then, sc, selfType)
		elseType := c.checkExpr(n.Else, sc, selfType)
		if thenType != "" && thenType == elseType {
			return thenType
		}
		return "" // gałęzie różnych typów - permisywnie, bez unii typów w v0.1

	case *ast.AssignExpr:
		c.checkAssign(n, sc, selfType)
		return c.checkExpr(n.Value, sc, selfType)

	case *ast.CallExpr:
		return c.checkCall(n, sc, selfType)

	case *ast.NewExpr:
		for _, a := range n.Args {
			c.checkExpr(a, sc, selfType)
		}
		return ""

	case *ast.MemberExpr:
		objType := c.checkExpr(n.Object, sc, selfType)
		if objType == "" {
			return ""
		}
		if info, ok := c.structs[objType]; ok {
			if ftype, ok := info.fields[n.Property]; ok {
				return normalizeType(ftype)
			}
			if impl, ok := c.implsByTarget[objType]; ok {
				if _, ok := impl.methods[n.Property]; ok {
					return "" // referencja do metody jako wartości - nie modelujemy typów funkcyjnych
				}
			}
			if !n.Optional {
				c.errorAt(n.Pos, "struktura `%s` nie ma pola ani metody `%s`", objType, n.Property)
			}
		}
		// obiekt nieznanego/złożonego typu (enum, any, wartość z JS) - permisywnie
		return ""

	case *ast.IndexExpr:
		c.checkExpr(n.Object, sc, selfType)
		c.checkExpr(n.Index, sc, selfType)
		return ""

	case *ast.AwaitExpr:
		return c.checkExpr(n.Value, sc, selfType)

	case *ast.SelfExpr:
		if selfType == "" {
			c.errorAt(n.Pos, "użycie `self` poza metodą `impl`")
		}
		return selfType

	case *ast.ArrowFnExpr:
		inner := newScope(sc)
		for _, p := range n.Params {
			inner.define(p.Name, varInfo{typ: normalizeType(p.Type), mutable: true})
		}
		if n.Body != nil {
			c.checkStmts(n.Body.Statements, inner, selfType)
		} else if n.ExprBody != nil {
			c.checkExpr(n.ExprBody, inner, selfType)
		}
		return ""

	case *ast.MatchExpr:
		return c.checkMatch(n, sc, selfType)

	default:
		return ""
	}
}

func (c *Checker) identType(name string, sc *scope) string {
	v, _ := sc.lookup(name)
	return v.typ
}

func (c *Checker) checkAssign(n *ast.AssignExpr, sc *scope, selfType string) {
	if id, ok := n.Target.(*ast.Ident); ok {
		if v, ok := sc.lookup(id.Name); ok {
			if v.isConst {
				c.errorAt(n.Pos, "nie można przypisać do `%s` - zadeklarowano jako niemutowalne (użyj `let mut`, by pozwolić na zmianę)", id.Name)
			}
		}
	}
	c.checkExpr(n.Target, sc, selfType)
}

// checkCall sprawdza wywołanie (argumenty, arność jeśli znamy sygnaturę) i
// zwraca inferowany typ zwracany, jeśli da się go ustalić. To JEDYNE miejsce,
// które odwiedza n.Callee - checkExpr dla *ast.CallExpr deleguje tu w całości,
// żeby uniknąć podwójnej wizytacji (a przez to podwójnych diagnostyk) obiektu
// w wywołaniach metod typu `obj.metoda()`.
func (c *Checker) checkCall(n *ast.CallExpr, sc *scope, selfType string) string {
	for _, a := range n.Args {
		c.checkExpr(a, sc, selfType)
	}

	switch callee := n.Callee.(type) {
	case *ast.Ident:
		fs, ok := c.funcs[callee.Name]
		if !ok {
			// Nie jest znaną funkcją Hyper - ALE wciąż musi być czymś: zmienną
			// (funkcja przekazana jako wartość), importem albo globalem JS.
			// Jeśli nie jest niczym z powyższych, to naprawdę niezdefiniowany
			// identyfikator - wykrywamy to TERAZ, a nie jako ReferenceError w node.
			c.checkExpr(callee, sc, selfType)
			return ""
		}
		checkArity(c, n, callee.Name, fs)
		c.checkVisibility(n.Pos, fs.pos, fs.pub, "funkcja", callee.Name)
		return normalizeType(fs.returnType)

	case *ast.MemberExpr:
		objType := c.checkExpr(callee.Object, sc, selfType)
		if objType == "" {
			return ""
		}
		if impl, ok := c.implsByTarget[objType]; ok {
			if fs, ok := impl.methods[callee.Property]; ok {
				checkArity(c, n, callee.Property, fs)
				return normalizeType(fs.returnType)
			}
		}
		if _, ok := c.structs[objType]; ok && !callee.Optional {
			c.errorAt(callee.Pos, "typ `%s` nie ma metody `%s`", objType, callee.Property)
		}
		return ""

	default:
		// łańcuch wywołań na czymś innym (np. main().catch(fn)) - wciąż
		// odwiedzamy Callee, tylko bez sprawdzania arności (nie znamy sygnatury
		// wartości zwróconej dynamicznie).
		c.checkExpr(n.Callee, sc, selfType)
		return ""
	}
}

func checkArity(c *Checker, n *ast.CallExpr, name string, fs *funcSig) {
	required := 0
	hasRest := false
	for _, p := range fs.params {
		if p.Rest {
			hasRest = true
			continue
		}
		if p.Default == nil && p.Name != "self" {
			required++
		}
	}
	total := len(fs.params)
	for _, p := range fs.params {
		if p.Name == "self" {
			total--
		}
	}
	if !hasRest && (len(n.Args) < required || len(n.Args) > total) {
		c.errorAt(n.Pos, "`%s` oczekuje %s argumentów, otrzymano %d", name, arityDesc(required, total), len(n.Args))
	}
}

func arityDesc(required, total int) string {
	if required == total {
		return fmt.Sprintf("%d", total)
	}
	return fmt.Sprintf("od %d do %d", required, total)
}

func (c *Checker) checkStructLit(n *ast.StructLit, sc *scope, selfType string) {
	for _, v := range n.Values {
		c.checkExpr(v, sc, selfType)
	}
	info, ok := c.structs[n.Name]
	if !ok {
		return // nieznana struktura (np. z innego modułu) - nie blokujemy
	}
	c.checkVisibility(n.Pos, info.pos, info.pub, "struktura", n.Name)
	for fname := range n.Values {
		if _, known := info.fields[fname]; !known {
			c.errorAt(n.Pos, "struktura `%s` nie ma pola `%s`", n.Name, fname)
		}
	}
	for fname := range info.requiredFields {
		if _, provided := n.Values[fname]; !provided {
			c.errorAt(n.Pos, "brakuje wymaganego pola `%s` w literale struktury `%s`", fname, n.Name)
		}
	}
}

func (c *Checker) checkBinaryTypes(n *ast.BinaryExpr, lt, rt string) {
	if lt == "" || rt == "" {
		return
	}
	catL, okL := typeCategory(lt)
	catR, okR := typeCategory(rt)
	if !okL || !okR {
		return
	}
	if n.Op == "+" && catL == "string" || catR == "string" {
		return // konkatenacja string+cokolwiek jest w JS legalna i częsta
	}
	if catL != catR {
		c.errorAt(n.Pos, "niezgodne typy w wyrażeniu binarnym: `%s` (%s) %s `%s` (%s)", lt, catL, n.Op, rt, catR)
	}
}

// checkMatch: obok zwykłego sprawdzenia ramion, próbuje ustalić czy `match`
// nad znanym enumem jest WYCZERPUJĄCY - to konkretna, praktyczna korzyść
// wynikająca z tego, że enum+match są tu obywatelami pierwszej klasy.
func (c *Checker) checkMatch(n *ast.MatchExpr, sc *scope, selfType string) string {
	subjectType := c.checkExpr(n.Subject, sc, selfType)

	hasWildcard := false
	covered := map[string]bool{}
	for _, arm := range n.Arms {
		inner := newScope(sc)
		if arm.Pattern.Wildcard {
			hasWildcard = true
		} else if arm.Pattern.Variant != "" {
			covered[arm.Pattern.Variant] = true
			if fields, ok := c.enumVariantFields(arm.Pattern.Variant); ok {
				for i, bind := range arm.Pattern.Bindings {
					fname := bind
					if i < len(fields) {
						fname = fields[i]
					}
					_ = fname
					inner.define(bind, varInfo{typ: "", mutable: true})
				}
			} else {
				for _, bind := range arm.Pattern.Bindings {
					inner.define(bind, varInfo{typ: "", mutable: true})
				}
			}
		} else if arm.Pattern.Literal != nil {
			c.checkExpr(arm.Pattern.Literal, sc, selfType)
		}
		c.checkExpr(arm.Body, inner, selfType)
	}

	if !hasWildcard && subjectType != "" {
		if enumInfo, ok := c.enums[subjectType]; ok {
			var missing []string
			for _, v := range enumInfo.order {
				if !covered[v] {
					missing = append(missing, v)
				}
			}
			if len(missing) > 0 {
				c.warnAt(n.Pos, "match nad `%s` nie jest wyczerpujący - brakuje wariantów: %s (dodaj je albo klauzulę `_`)",
					subjectType, strings.Join(missing, ", "))
			}
		}
	}
	return ""
}

func (c *Checker) enumVariantFields(variantName string) ([]string, bool) {
	for _, info := range c.enums {
		if fields, ok := info.variant[variantName]; ok {
			var names []string
			for _, f := range fields {
				names = append(names, f.Name)
			}
			return names, true
		}
	}
	return nil, false
}

// ---------------------------------------------------------------------
// Pomocnicze: kategorie typów prymitywnych
// ---------------------------------------------------------------------

func normalizeType(t string) string { return strings.TrimSpace(t) }

var numericTypes = map[string]bool{
	"i8": true, "i16": true, "i32": true, "i64": true,
	"u8": true, "u16": true, "u32": true, "u64": true,
	"f32": true, "f64": true, "number": true,
}

// typeCategory zwraca ogólną kategorię typu prostego. Drugi wynik == false
// oznacza "nie próbuj tego sprawdzać" (typ złożony: struct/enum/any/generyk/
// tablica/opcjonalny/unia) - unikamy fałszywych alarmów kosztem pełności.
func typeCategory(t string) (string, bool) {
	t = normalizeType(t)
	if t == "" || t == "any" || t == "unknown" || t == "void" || t == "null" {
		return "", false
	}
	if strings.ContainsAny(t, "[]?<>|") {
		return "", false
	}
	if numericTypes[t] {
		return "numeric", true
	}
	if t == "string" {
		return "string", true
	}
	if t == "bool" {
		return "bool", true
	}
	return "", false // typ nazwany (struct/enum) - nie kategoryzujemy
}

func typesCompatible(declared, actual string) bool {
	dc, dok := typeCategory(declared)
	ac, aok := typeCategory(actual)
	if !dok || !aok {
		return true // permisywnie: nie porównujemy typów złożonych w v0.1
	}
	return dc == ac
}

func isComparisonOp(op string) bool {
	switch op {
	case "==", "!=", "<", ">", "<=", ">=", "&&", "||":
		return true
	}
	return false
}
